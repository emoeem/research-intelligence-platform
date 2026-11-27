#!/usr/bin/env python3
"""Create a redacted local review queue for POC/weapon records with secret indicators."""
from __future__ import annotations
import argparse, json
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, default=Path("docs/knowledge-governance/poc-governance-candidates-2026-11-27.json"))
    parser.add_argument("--output", type=Path, default=Path("docs/knowledge-governance/poc-sensitive-review-batch-001-2026-11-27.json"))
    args = parser.parse_args()
    report = json.loads(args.input.read_text(encoding="utf-8"))
    rows = [row for row in report["candidates"] if row["secret_indicator_types"]]
    rows.sort(key=lambda row: (not bool(row["secret_indicator_types"]), row["rtis_id"]))
    tasks = [{
        "candidate_id": row["entity_id"], "rtis_id": row["rtis_id"], "title": row["title"],
        "source_hashes": {"content": row["content_sha256"], "file": row["source_file_sha256"], "verification_note": row["legacy_verification_note_sha256"]},
        "indicator_types": row["secret_indicator_types"],
        "review": {"confirm_false_positive":"PENDING", "remove_or_redact_secret":"PENDING", "rotate_credential_if_real":"PENDING", "review_source_history":"PENDING", "status":"CANDIDATE"},
        "guardrails": {"raw_value_exported":False, "source_content_exported":False, "credential_usable":False},
    } for row in rows]
    payload = {"schema_version":"1.0.0", "batch_id":"poc-sensitive-review-001-2026-11-27", "status":"RESTRICTED_LOCAL_REVIEW_REQUIRED", "task_count":len(tasks), "tasks":tasks, "raw_secret_values_exported":False}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload,ensure_ascii=False,indent=2)+"\\n",encoding="utf-8")
    print(json.dumps({"output":str(args.output),"task_count":len(tasks),"raw_secret_values_exported":False},ensure_ascii=False,indent=2))
    return 0

if __name__ == "__main__": raise SystemExit(main())
