#!/usr/bin/env python3
"""Build a prioritized, sanitized Nuclei human-review batch without template bodies."""
from __future__ import annotations
import argparse, json
from pathlib import Path


def priority(row):
    if row["parse_status"] != "parsed": return (0, 0, row["item_key"])
    if row["cisa_kev_cve_ids"] and row["severity"] in {"critical", "high"}: return (1, 0, row["item_key"])
    if row["cisa_kev_cve_ids"]: return (2, 0, row["item_key"])
    if row["severity"] in {"critical", "high"}: return (3, 0, row["item_key"])
    if row["cve_ids"]: return (4, 0, row["item_key"])
    return (5, 0, row["item_key"])


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, default=Path("docs/knowledge-governance/vulnerability-nuclei-governance-2026-11-27.json"))
    parser.add_argument("--output", type=Path, default=Path("docs/knowledge-governance/nuclei-review-batch-001-2026-11-27.json"))
    parser.add_argument("--offset", type=int, default=0)
    parser.add_argument("--limit", type=int, default=200)
    args = parser.parse_args()
    if args.offset < 0 or not 1 <= args.limit <= 1000: parser.error("offset must be >=0 and limit must be 1..1000")
    report = json.loads(args.input.read_text(encoding="utf-8"))
    rows = sorted(report["nuclei_review_candidates"], key=priority)
    selected = rows[args.offset:args.offset + args.limit]
    tasks = []
    for i, row in enumerate(selected, args.offset + 1):
        p = priority(row)[0]
        tasks.append({
            "batch_id": f"nuclei-review-{args.offset // args.limit + 1:03d}-2026-11-27",
            "candidate_id": row["entity_id"], "priority_rank": i,
            "priority_band": f"P{p}",
            "template": {k: row[k] for k in (
                "item_key", "title", "template_id", "content_sha256", "source_commit", "source_url",
                "license_status", "source_review_status", "parse_status", "body_bytes", "severity",
                "protocols", "cve_ids", "cwe_ids", "cvss_score", "epss_score", "matcher_count",
                "request_block_count", "has_flow", "upstream_verified_claim", "cisa_kev_cve_ids", "review_flags"
            )},
            "review": {"source_provenance":"PENDING", "license":"PENDING", "yaml_engine_validation":"NOT_RUN", "request_safety":"PENDING", "scope_safety":"PENDING", "final_status":"CANDIDATE", "reviewer":None, "reviewed_at":None, "notes":None},
            "guardrails": {"template_body_included":False, "template_executed":False, "target_contacted":False, "auto_approved":False},
        })
    payload = {
        "schema_version":"1.0.0", "batch_id":tasks[0]["batch_id"] if tasks else None,
        "status":"READY_FOR_HUMAN_REVIEW", "offset":args.offset, "limit":args.limit,
        "total_candidates":len(rows), "task_count":len(tasks),
        "next_offset":args.offset + len(tasks) if args.offset + len(tasks) < len(rows) else None,
        "tasks":tasks, "source_report":str(args.input), "raw_template_bodies_exported":False,
        "engine_validation_performed":False, "auto_approval":False,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload, ensure_ascii=False, indent=2)+"\n", encoding="utf-8")
    print(json.dumps({"output":str(args.output),"batch_id":payload["batch_id"],"task_count":len(tasks),"total_candidates":len(rows),"next_offset":payload["next_offset"],"priority_bands":{f"P{i}":sum(t['priority_band']==f'P{i}' for t in tasks) for i in range(6)}},ensure_ascii=False,indent=2))
    return 0

if __name__ == "__main__": raise SystemExit(main())
