#!/usr/bin/env python3
from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
EVALUATOR = ROOT / "apps/server/rip_retrieval_evaluator.py"


def row(rank: int, annotator: str) -> dict:
    return {
        "query_id": "q1",
        "category": "keyword",
        "query": "test",
        "include_candidates": False,
        "rank": rank,
        "doc_id": f"doc-{rank}",
        "title": f"Doc {rank}",
        "source_system": "fixture",
        "governance_status": "APPROVED",
        "relevance": 3 if rank == 1 else 0,
        "notes": "",
        "annotator": annotator,
    }


class RetrievalEvaluatorTests(unittest.TestCase):
    def run_evaluator(self, rows: list[dict], mode: str):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "labeled.jsonl"
            path.write_text(
                "".join(json.dumps(r, ensure_ascii=False) + "\n" for r in rows),
                encoding="utf-8",
            )
            return subprocess.run(
                [sys.executable, str(EVALUATOR), "--labeled", str(path),
                 "--label-mode", mode],
                capture_output=True,
                text=True,
            )

    def test_ai_assisted_mode_has_explicit_non_ground_truth_claim(self) -> None:
        result = self.run_evaluator(
            [row(rank, "ai-assist") for rank in range(1, 11)],
            "ai-assisted-characterization",
        )
        self.assertEqual(result.returncode, 0)
        payload = json.loads(result.stdout)
        self.assertFalse(payload["human_ground_truth_claim"])
        self.assertEqual(payload["label_mode"], "ai-assisted-characterization")

    def test_human_mode_rejects_ai_labels(self) -> None:
        result = self.run_evaluator(
            [row(rank, "ai-assist") for rank in range(1, 11)],
            "human-ground-truth",
        )
        self.assertEqual(result.returncode, 2)
        payload = json.loads(result.stdout)
        self.assertEqual(payload["error"], "human_annotation_required")


if __name__ == "__main__":
    raise SystemExit(unittest.main())
