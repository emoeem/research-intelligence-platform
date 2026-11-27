#!/usr/bin/env python3
"""Read-only exact-CVE self-consistency check for the SQLite Nuclei FTS index.

This is not a human-judged relevance benchmark: gold IDs are derived from the
structured nuclei_cve_ids_json field in the same immutable index snapshot.
"""
from __future__ import annotations

import argparse
import json
import os
import sqlite3
import subprocess
import sys
from pathlib import Path
from statistics import mean


def run_query(binary: Path, index: Path, cve: str) -> list[dict]:
    proc = subprocess.run(
        [
            str(binary), "query", "--index-db", str(index),
            "--keywords", cve, "--resource-type", "nuclei_template",
            "--limit", "100",
        ],
        capture_output=True,
        text=True,
        timeout=15,
        check=False,
    )
    if proc.returncode:
        raise RuntimeError(f"{cve}: rp-search-index exited {proc.returncode}: {proc.stderr[-500:]}")
    try:
        payload = json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        raise RuntimeError(f"{cve}: invalid JSON output: {exc}") from exc
    return payload.get("hits") or []


def main() -> int:
    root = Path(__file__).resolve().parents[2]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--index", type=Path, required=True, help="path to a rebuilt v7 derived search index")
    parser.add_argument(
        "--bin",
        type=Path,
        default=Path(os.environ.get("RP_SEARCH_INDEX_BIN", root / "target/debug/rp-search-index")),
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=root / "test-results/m4-exact-cve-search-baseline-2026-11-26.json",
    )
    parser.add_argument("--query-count", type=int, default=100)
    args = parser.parse_args()

    index = args.index.resolve()
    binary = args.bin.resolve()
    if not index.is_file() or not binary.is_file():
        raise SystemExit(f"index or binary missing: index={index} binary={binary}")

    conn = sqlite3.connect(f"file:{index}?mode=ro", uri=True)
    try:
        meta = dict(conn.execute("SELECT key,value FROM search_index_meta"))
        rows = conn.execute(
            "SELECT resource_id,nuclei_cve_ids_json FROM search_index_doc "
            "WHERE resource_type='nuclei_template' ORDER BY resource_id"
        ).fetchall()
        fts_rows = conn.execute("SELECT COUNT(*) FROM search_index_fts").fetchone()[0]
        doc_count = conn.execute("SELECT COUNT(*) FROM search_index_doc").fetchone()[0]
    finally:
        conn.close()

    if meta.get("schema_version") != "7":
        raise SystemExit(f"expected schema_version=7, got {meta.get('schema_version')!r}")

    gold_by_cve: dict[str, set[str]] = {}
    for resource_id, raw in rows:
        for cve in json.loads(raw):
            gold_by_cve.setdefault(cve.upper(), set()).add(resource_id)

    cves = sorted(gold_by_cve)[: args.query_count]
    if len(cves) < args.query_count:
        raise SystemExit(f"requested {args.query_count} queries but only {len(cves)} CVEs are indexed")

    results = []
    orders: dict[str, list[str]] = {}
    for cve in cves:
        hits = run_query(binary, index, cve)
        ranked = [hit["resource_id"] for hit in hits]
        if len(ranked) != len(set(ranked)):
            raise RuntimeError(f"{cve}: duplicate resource_id in search result")
        relevant = gold_by_cve[cve]
        top10 = ranked[:10]
        found10 = len(set(top10) & relevant)
        first_rank = next((i + 1 for i, rid in enumerate(ranked) if rid in relevant), None)
        results.append(
            {
                "query": cve,
                "relevant_count": len(relevant),
                "returned_count": len(ranked),
                "relevant_in_top10": found10,
                "recall_at_10": found10 / len(relevant),
                "precision_at_10": found10 / 10,
                "precision_at_returned": found10 / len(top10) if top10 else 0.0,
                "mrr": 1 / first_rank if first_rank else 0.0,
                "first_relevant_rank": first_rank,
                "all_relevant_in_top100": relevant.issubset(set(ranked)),
            }
        )
        if len(orders) < 10:
            orders[cve] = ranked

    # Determinism check: same exact-ID query must return the same ordered IDs.
    deterministic = True
    for cve, expected in orders.items():
        actual = [hit["resource_id"] for hit in run_query(binary, index, cve)]
        deterministic = deterministic and actual == expected

    summary = {
        "benchmark_type": "structured_exact_cve_self_consistency",
        "not_a_human_judged_relevance_benchmark": True,
        "index_path": str(index),
        "index_schema_version": int(meta["schema_version"]),
        "index_doc_count": doc_count,
        "index_fts_rows": fts_rows,
        "nuclei_template_count": len(rows),
        "unique_nuclei_cve_count": len(gold_by_cve),
        "query_count": len(results),
        "failed_queries": 0,
        "mean_recall_at_10": mean(row["recall_at_10"] for row in results),
        "mean_precision_at_10": mean(row["precision_at_10"] for row in results),
        "mean_precision_at_returned": mean(row["precision_at_returned"] for row in results),
        "mean_mrr": mean(row["mrr"] for row in results),
        "first_relevant_rank_1_rate": sum(row["first_relevant_rank"] == 1 for row in results) / len(results),
        "all_relevant_in_top100_rate": sum(row["all_relevant_in_top100"] for row in results) / len(results),
        "deterministic_first_10_queries": deterministic,
        "queries": results,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({k: v for k, v in summary.items() if k != "queries"}, ensure_ascii=False, indent=2))
    print(f"report: {args.output}")
    return 0 if deterministic and not summary["failed_queries"] else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, sqlite3.Error, subprocess.TimeoutExpired) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
