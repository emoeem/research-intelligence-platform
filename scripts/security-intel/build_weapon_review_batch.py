#!/usr/bin/env python3
"""Build sanitized, prioritized review tasks for all RTIS weapon asset kinds (not just POCs)."""
from __future__ import annotations
import argparse
import json
from pathlib import Path


def priority(row: dict) -> int:
    if row.get("secret_indicator_types"):
        return 0
    kind = str(row.get("kind", "")).lower()
    if kind == "poc":
        return 1
    if kind == "exp":
        return 2
    if kind == "script":
        return 3
    if row.get("static_behavior_flags"):
        return 4
    if kind == "payload":
        return 5
    if kind == "rule":
        return 6
    return 7


def build(report: dict, offset: int, limit: int) -> dict:
    rows = sorted(report["candidates"], key=lambda row: (priority(row), row.get("severity", "unknown"), row["entity_id"]))
    selected = rows[offset:offset + limit]
    tasks = []
    for rank, row in enumerate(selected, offset + 1):
        tasks.append({
            "batch_id": f"weapon-review-{offset // limit + 1:03d}-2026-11-27",
            "candidate_id": row["entity_id"],
            "priority_rank": rank,
            "priority_band": f"P{priority(row)}",
            "asset": {k: row.get(k) for k in (
                "rtis_id", "title", "kind", "category", "severity", "cve_ids", "cve_value_malformed",
                "component_present", "affected_versions_present", "language", "source", "source_url_state",
                "source_file_state", "source_file_sha256", "source_file_bytes", "content_sha256",
                "content_bytes", "legacy_verified_flag", "legacy_verification_note_present",
                "legacy_verification_note_sha256", "verification_basis", "lifecycle", "evidence_level",
                "structured_metadata_present", "secret_indicator_types", "static_behavior_flags",
                "review_reasons", "duplicate_code_candidate", "duplicate_content_candidate",
            )},
            "review": {
                "identity_and_kind": "PENDING", "source_provenance": "PENDING", "license": "PENDING",
                "affected_versions": "PENDING", "secret_handling": "PENDING", "behavior_and_scope": "PENDING",
                "verification_evidence": "PENDING", "final_status": "CANDIDATE",
                "reviewer": None, "reviewed_at": None, "notes": None,
            },
            "guardrails": {
                "raw_code_or_payload_included": False, "code_or_payload_executed": False,
                "target_contacted": False, "legacy_verified_treated_as_lab_proof": False,
                "auto_approved": False,
            },
        })
    return {
        "schema_version": "1.0.0",
        "batch_id": tasks[0]["batch_id"] if tasks else None,
        "status": "READY_FOR_HUMAN_REVIEW",
        "offset": offset,
        "limit": limit,
        "total_candidates": len(rows),
        "task_count": len(tasks),
        "next_offset": offset + len(tasks) if offset + len(tasks) < len(rows) else None,
        "tasks": tasks,
        "source_report": "poc-governance-candidates-2026-11-27.json",
        "raw_code_or_payload_exported": False,
        "execution_performed": False,
        "auto_approval": False,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, default=Path("docs/knowledge-governance/poc-governance-candidates-2026-11-27.json"))
    parser.add_argument("--output", type=Path, default=Path("docs/knowledge-governance/weapon-review-batch-001-2026-11-27.json"))
    parser.add_argument("--offset", type=int, default=0)
    parser.add_argument("--limit", type=int, default=100)
    args = parser.parse_args()
    if args.offset < 0 or not 1 <= args.limit <= 1000:
        parser.error("offset must be >=0 and limit must be 1..1000")
    payload = build(json.loads(args.input.read_text(encoding="utf-8")), args.offset, args.limit)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"output": str(args.output), "batch_id": payload["batch_id"], "task_count": payload["task_count"], "total_candidates": payload["total_candidates"], "next_offset": payload["next_offset"], "priority_bands": {f"P{i}": sum(t["priority_band"] == f"P{i}" for t in payload["tasks"]) for i in range(8)}}, ensure_ascii=False, indent=2))
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
