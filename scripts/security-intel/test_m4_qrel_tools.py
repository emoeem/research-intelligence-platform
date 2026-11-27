#!/usr/bin/env python3
from __future__ import annotations

import csv
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BUILD = ROOT / "scripts/security-intel/build_m4_label_studio_batch.py"
IMPORT = ROOT / "scripts/security-intel/import_m4_label_studio_annotations.py"
EVALUATE = ROOT / "scripts/security-intel/evaluate_m4_qrels.py"


def row(candidate_id: str, query_id: str, rank: int) -> dict[str, str]:
    return {
        "batch_id": "M4-QREL-TEST",
        "candidate_id": candidate_id,
        "query_id": query_id,
        "query": "SQL injection",
        "rank": str(rank),
        "resource_id": f"doc-{candidate_id}",
        "resource_type": "catalog_entry",
        "title": f"Title {candidate_id}",
        "source_system": "knowledge.cisa-kev",
        "source_entity_id": candidate_id,
        "source_version": "0.1.2",
        "canonical_url": "https://example.invalid/" + candidate_id,
        "description": "Synthetic fixture description.",
        "source_path": "fixture.json",
        "source_commit": "abc123",
        "score": str(1 / rank),
        "fts_score": "1",
        "keyword_coverage": "1",
        "keyword_matches": "sql | injection",
        "source_annotations": "fixture",
        "ai_suggested_grade": "3",
        "ai_suggestion_reason": "Synthetic advisory suggestion.",
        "ai_suggestion_confidence": "0.95",
        "ai_suggestion_status": "suggested_unconfirmed",
        "human_grade": "",
        "human_notes": "",
        "review_status": "unjudged",
    }


def write_csv(path: Path, rows: list[dict[str, str]]) -> None:
    with path.open("w", encoding="utf-8-sig", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)


class M4QrelToolsTests(unittest.TestCase):
    def test_label_studio_export_contains_advisory_prediction_only(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            batch = tmp_path / "batch.csv"
            rows = [
                row(f"Q{query:02d}-R{rank:02d}", f"Q{query:02d}", rank)
                for query in range(1, 15)
                for rank in range(1, 11)
            ]
            write_csv(batch, rows)
            out = tmp_path / "ls"
            result = subprocess.run(
                [sys.executable, str(BUILD), "--input", str(batch), "--output-dir", str(out)],
                check=True,
                capture_output=True,
                text=True,
            )
            manifest = json.loads(result.stdout)
            tasks = json.loads((out / "tasks.json").read_text())
            self.assertEqual(manifest["task_count"], 140)
            self.assertEqual(len(tasks), 140)
            self.assertEqual(tasks[0]["data"]["ai_suggestion_status"], "advisory_unconfirmed")
            self.assertEqual(tasks[0]["predictions"][0]["result"][0]["value"]["choices"], ["3 — directly relevant"])
            self.assertNotIn("human_grade", tasks[0]["data"])

    def test_import_annotations_changes_only_human_judgment_fields(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            batch = tmp_path / "batch.csv"
            rows = [row("Q01-R01", "Q01", 1), row("Q01-R02", "Q01", 2)]
            write_csv(batch, rows)
            exported = [
                {
                    "data": {"candidate_id": "Q01-R01"},
                    "annotations": [
                        {
                            "completed_by": {"email": "reviewer@example.invalid"},
                            "created_at": "2026-11-27T00:00:00Z",
                            "result": [
                                {
                                    "from_name": "relevance",
                                    "type": "choices",
                                    "value": {"choices": ["2 — relevant"]},
                                },
                                {
                                    "from_name": "human_notes",
                                    "type": "textarea",
                                    "value": {"text": ["Relevant CVE class."]},
                                },
                            ],
                        }
                    ],
                }
            ]
            export_path = tmp_path / "export.json"
            export_path.write_text(json.dumps(exported), encoding="utf-8")
            subprocess.run(
                [sys.executable, str(IMPORT), "--input", str(export_path), "--batch", str(batch)],
                check=True,
                capture_output=True,
                text=True,
            )
            with batch.open(encoding="utf-8-sig", newline="") as handle:
                updated = list(csv.DictReader(handle))
            self.assertEqual(updated[0]["human_grade"], "2")
            self.assertEqual(updated[0]["ai_suggested_grade"], "3")
            self.assertEqual(updated[0]["review_status"], "human-reviewed")
            self.assertEqual(updated[0]["human_notes"], "Relevant CVE class.")
            self.assertEqual(updated[1]["human_grade"], "")

    def test_conflicting_annotators_are_rejected_without_modifying_csv(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            batch = tmp_path / "batch.csv"
            write_csv(batch, [row("Q01-R01", "Q01", 1)])
            before = batch.read_bytes()
            exported = [
                {
                    "data": {"candidate_id": "Q01-R01"},
                    "annotations": [
                        {"result": [{"from_name": "relevance", "type": "choices", "value": {"choices": ["2 — relevant"]}}]},
                        {"result": [{"from_name": "relevance", "type": "choices", "value": {"choices": ["3 — directly relevant"]}}]},
                    ],
                }
            ]
            export_path = tmp_path / "export.json"
            export_path.write_text(json.dumps(exported), encoding="utf-8")
            result = subprocess.run(
                [sys.executable, str(IMPORT), "--input", str(export_path), "--batch", str(batch)],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(batch.read_bytes(), before)

    def test_evaluator_refuses_ai_only_batch(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            batch = tmp_path / "batch.csv"
            rows = [row(f"Q01-R{i:02d}", "Q01", i) for i in range(1, 11)]
            write_csv(batch, rows)
            output = tmp_path / "metrics.json"
            evaluator_python = Path.home() / ".cache/rip-m4-evaluation-venv/bin/python"
            if not evaluator_python.exists():
                self.skipTest("isolated ir-measures environment is not installed")
            result = subprocess.run(
                [str(evaluator_python), str(EVALUATE),
                 str(batch), "--output", str(output), "--trec-dir", str(tmp_path / "trec")],
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            payload = json.loads(output.read_text())
            self.assertEqual(payload["status"], "pending_human_judgment")
            self.assertEqual(payload["judged_pair_count"], 0)

    def test_evaluator_uses_ir_measures_for_complete_human_qrels(self) -> None:
        evaluator_python = Path.home() / ".cache/rip-m4-evaluation-venv/bin/python"
        if not evaluator_python.exists():
            self.skipTest("isolated ir-measures environment is not installed")
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            batch = tmp_path / "batch.csv"
            rows = [row(f"Q01-R{i:02d}", "Q01", i) for i in range(1, 11)]
            for item in rows:
                item["human_grade"] = "3" if item["rank"] == "1" else "0"
            write_csv(batch, rows)
            output = tmp_path / "metrics.json"
            result = subprocess.run(
                [str(evaluator_python), str(EVALUATE), str(batch), "--output", str(output),
                 "--trec-dir", str(tmp_path / "trec")],
                check=True, capture_output=True, text=True,
            )
            payload = json.loads(output.read_text())
            self.assertEqual(payload["status"], "evaluated")
            self.assertEqual(payload["judged_pair_count"], 10)
            self.assertEqual(payload["metrics"]["judged_at_10"], 1.0)
            self.assertEqual(payload["metrics"]["mrr_at_10"], 1.0)
            self.assertEqual(len((tmp_path / "trec/m4-qrels.txt").read_text().splitlines()), 10)


if __name__ == "__main__":
    unittest.main()
