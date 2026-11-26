#!/usr/bin/env python3
"""Read-only search helper for the RIP-owned RTIS curation overlay."""
from __future__ import annotations

import argparse
import json
from pathlib import Path


OVERLAY = Path(__file__).with_name("k_meta_overlay.json")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence-level", action="append", choices=["K0", "K1", "K2", "K3", "K4"])
    parser.add_argument("--lifecycle")
    parser.add_argument("--category")
    parser.add_argument("--dedup-status")
    parser.add_argument("--missing-evidence", action="store_true")
    parser.add_argument("--relation-poc")
    args = parser.parse_args()

    data = json.loads(OVERLAY.read_text(encoding="utf-8"))
    entries = data["entries"]

    if args.evidence_level:
        entries = [e for e in entries if e.get("evidence_level") in set(args.evidence_level)]
    if args.lifecycle:
        entries = [e for e in entries if e.get("lifecycle") == args.lifecycle]
    if args.category:
        entries = [e for e in entries if args.category.lower() in str(e.get("original_category", "")).lower()]
    if args.dedup_status:
        entries = [e for e in entries if e.get("dedup_status") == args.dedup_status]
    if args.missing_evidence:
        entries = [e for e in entries if e.get("missing_evidence")]
    if args.relation_poc:
        code = next((e["code"] for e in data["entries"] if str(e["knowledge_id"]) == args.relation_poc), args.relation_poc)
        matched = {
            edge["src"] if edge["dst"] == code else edge["dst"]
            for edge in data["relations"]
            if edge["src"] == code or edge["dst"] == code
        }
        entries = [e for e in entries if e["code"] in matched]

    print(json.dumps({
        "count": len(entries),
        "knowledge_ids": [e["knowledge_id"] for e in entries],
        "entries": entries,
    }, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
