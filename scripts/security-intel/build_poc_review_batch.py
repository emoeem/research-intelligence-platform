#!/usr/bin/env python3
"""Create a sanitized human-review batch for true RTIS PoC-kind records only."""
from __future__ import annotations
import argparse, json
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, default=Path("docs/knowledge-governance/poc-governance-candidates-2026-11-27.json"))
    parser.add_argument("--output", type=Path, default=Path("docs/knowledge-governance/poc-review-batch-001-2026-11-27.json"))
    args = parser.parse_args()
    report = json.loads(args.input.read_text(encoding="utf-8"))
    rows = [row for row in report["candidates"] if row["kind"] == "poc"]
    rank = {"critical": 0, "high": 1, "medium": 2, "low": 3, "info": 4, "unknown": 5}
    rows.sort(key=lambda row: (rank.get(row["severity"], 5), not row["legacy_verified_flag"], row["rtis_id"]))
    tasks = []
    for index, row in enumerate(rows, 1):
        tasks.append({
            "batch_id": "poc-review-001-2026-11-27",
            "candidate_id": f"rtis-poc-{row['rtis_id']}",
            "priority_rank": index,
            "priority_band": "P0" if row["severity"] in {"critical", "high"} else "P1" if row["severity"] == "medium" else "P2" if row["severity"] in {"low", "info"} else "P3",
            "identity": {"entity_id": row["entity_id"], "rtis_id": row["rtis_id"], "code_sha256": row["code_sha256"]},
            "metadata": {key: row[key] for key in (
                "title", "category", "severity", "cve_ids", "component_present", "affected_versions_present",
                "language", "source", "source_url_state", "source_file_state", "source_file_sha256",
                "content_sha256", "content_bytes", "legacy_verified_flag", "legacy_verification_note_present",
                "legacy_verification_note_sha256", "verification_basis", "lifecycle", "evidence_level",
                "static_behavior_flags", "secret_indicator_types", "review_reasons"
            )},
            "review": {
                "provenance": "PENDING", "applicability": "PENDING", "safety": "PENDING",
                "license": "PENDING", "reproducibility": "NOT_TESTED", "remediation": "UNKNOWN",
                "final_status": "CANDIDATE", "reviewer": None, "reviewed_at": None, "notes": None,
            },
            "guardrails": {
                "source_body_included": False, "verification_note_included": False,
                "execution_authorized": False, "target_authorized": False, "auto_approved": False,
            },
        })
    payload = {
        "schema_version": "1.0.0", "batch_id": "poc-review-001-2026-11-27",
        "status": "READY_FOR_HUMAN_REVIEW", "task_count": len(tasks),
        "source_report": str(args.input), "human_labels_included": False,
        "raw_source_content_exported": False, "historical_verified_flag_is_not_lab_proof": True,
        "tasks": tasks,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"output": str(args.output), "task_count": len(tasks), "priority_bands": {band: sum(t['priority_band']==band for t in tasks) for band in ('P0','P1','P2','P3')}, "raw_source_content_exported": False}, ensure_ascii=False, indent=2))
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
