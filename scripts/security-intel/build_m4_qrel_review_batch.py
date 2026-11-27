#!/usr/bin/env python3
"""Build a reproducible, unjudged top-10 cross-source qrel review batch for M4."""
from __future__ import annotations

import argparse
import csv
import json
import re
import subprocess
import sys
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
QUERIES = [
    "SQL injection",
    "remote code execution",
    "authentication bypass",
    "directory traversal",
    "server side request forgery",
    "cross site scripting",
    "command injection",
    "deserialization",
    "file upload",
    "Spring Cloud Gateway",
    "Docker container escape",
    "API rate limit bypass",
    "Nginx path traversal",
    "Jenkins script console",
]
FIELDS = [
    "batch_id", "candidate_id", "query_id", "query", "rank", "resource_id",
    "resource_type", "title", "source_system", "source_entity_id", "source_version",
    "canonical_url", "description", "source_path", "source_commit", "score",
    "fts_score", "keyword_coverage", "keyword_matches", "source_annotations",
    "ai_suggested_grade", "ai_suggestion_reason", "ai_suggestion_confidence",
    "ai_suggestion_status", "human_grade", "human_notes", "review_status",
]


def tokens(query: str) -> str:
    return ",".join(re.findall(r"[A-Za-z0-9]+", query))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--index-db", type=Path, default=ROOT / "rip-data/derived/curated-poc-search.db")
    parser.add_argument("--search-bin", type=Path, default=ROOT / "target/debug/rp-search-index")
    parser.add_argument("--output", type=Path, default=ROOT / "test-results/m4-cross-source-qrel-review-batch-001.csv")
    parser.add_argument("--limit", type=int, default=10)
    args = parser.parse_args()

    if not args.search_bin.is_file() or not args.index_db.is_file():
        parser.error("search binary or derived index is missing; build the v7 index first")
    if args.limit != 10:
        parser.error("M4 batch is fixed at top-10 per query so NDCG@10 can be evaluated")

    import sqlite3
    conn = sqlite3.connect(f"file:{args.index_db.resolve()}?mode=ro", uri=True)
    meta = dict(conn.execute("SELECT key, value FROM search_index_meta"))
    conn.close()
    if meta.get("schema_version") != "7":
        parser.error(f"expected search index schema 7, got {meta.get('schema_version')!r}")

    batch_id = "M4-QREL-2026-11-27-001"
    rows: list[dict[str, str]] = []
    query_summaries = []
    for number, query in enumerate(QUERIES, start=1):
        command = [
            str(args.search_bin), "query", "--index-db", str(args.index_db),
            "--keywords", tokens(query), "--limit", str(args.limit),
        ]
        proc = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, check=False)
        if proc.returncode != 0:
            raise RuntimeError(f"query {query!r} failed: {(proc.stderr or proc.stdout)[-1000:]}")
        payload = json.loads(proc.stdout)
        hits = payload.get("hits") or []
        if len(hits) < args.limit:
            raise RuntimeError(f"query {query!r} returned only {len(hits)} hits; expected {args.limit}")
        query_id = f"Q{number:02d}"
        source_counts: Counter[str] = Counter()
        for rank, hit in enumerate(hits[:args.limit], start=1):
            identity = hit.get("identity") or {}
            source_system = identity.get("source_system") or (
                "rtis" if hit.get("resource_type") == "rtis_poc" else "platform"
            )
            source_counts[source_system] += 1
            rows.append({
                "batch_id": batch_id,
                "candidate_id": f"{query_id}-R{rank:02d}",
                "query_id": query_id,
                "query": query,
                "rank": str(rank),
                "resource_id": hit.get("resource_id", ""),
                "resource_type": hit.get("resource_type", ""),
                "title": hit.get("name", ""),
                "source_system": source_system,
                "source_entity_id": identity.get("source_entity_id") or "",
                "source_version": identity.get("source_version") or "",
                "canonical_url": hit.get("canonical_url", ""),
                "description": (hit.get("description") or "").replace("\n", " ")[:500],
                "source_path": hit.get("source_path") or "",
                "source_commit": hit.get("source_commit") or "",
                "score": str(hit.get("score", "")),
                "fts_score": str(hit.get("fts_score", "")),
                "keyword_coverage": str(hit.get("keyword_coverage", "")),
                "keyword_matches": " | ".join(hit.get("keyword_matches") or []),
                "source_annotations": " | ".join(hit.get("source_annotations") or []),
                "ai_suggested_grade": "",
                "ai_suggestion_reason": "",
                "ai_suggestion_confidence": "",
                "ai_suggestion_status": "not_generated",
                "human_grade": "",
                "human_notes": "",
                "review_status": "unjudged",
            })
        query_summaries.append({
            "query_id": query_id,
            "query": query,
            "total_matched": payload.get("total_matched", len(hits)),
            "pooled": len(hits[:args.limit]),
            "source_systems": dict(source_counts),
        })

    if not 100 <= len(rows) <= 150:
        raise RuntimeError(f"candidate count {len(rows)} is outside the required 100-150 range")
    if len({(row["query_id"], row["resource_id"]) for row in rows}) != len(rows):
        raise RuntimeError("duplicate query/resource pairs detected")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", encoding="utf-8-sig", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=FIELDS)
        writer.writeheader()
        writer.writerows(rows)

    source_counts = Counter(row["source_system"] for row in rows)
    type_counts = Counter(row["resource_type"] for row in rows)
    summary = {
        "batch_id": batch_id,
        "created_at": datetime.now(timezone.utc).isoformat(),
        "index": {
            "path": str(args.index_db),
            "schema_version": int(meta["schema_version"]),
            "doc_count": int(meta.get("doc_count", 0)),
            "built_at": meta.get("built_at"),
        },
        "query_count": len(QUERIES),
        "candidate_pair_count": len(rows),
        "source_system_counts": dict(source_counts),
        "resource_type_counts": dict(type_counts),
        "human_judgments_complete": False,
        "grade_scale": {"0": "irrelevant", "1": "weakly related", "2": "relevant", "3": "directly relevant"},
        "queries": query_summaries,
        "note": "All human_grade fields are intentionally blank. Search rank and AI suggestions are not human relevance judgments.",
    }
    summary_path = args.output.with_suffix(".summary.json")
    summary_path.write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({
        "csv": str(args.output),
        "summary": str(summary_path),
        "query_count": len(QUERIES),
        "candidate_pair_count": len(rows),
        "source_system_counts": dict(source_counts),
        "resource_type_counts": dict(type_counts),
        "human_judgments_complete": False,
    }, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, RuntimeError, json.JSONDecodeError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(2)
