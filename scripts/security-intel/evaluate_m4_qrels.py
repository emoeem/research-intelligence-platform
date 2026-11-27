#!/usr/bin/env python3
"""Evaluate the M4 pooled top-10 qrels with ir-measures after human review."""
from __future__ import annotations

import argparse
import csv
import json
import sys
from collections import defaultdict
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "input",
        type=Path,
        nargs="?",
        default=Path("test-results/m4-cross-source-qrel-review-batch-001.csv"),
    )
    parser.add_argument("--output", type=Path, default=Path("test-results/m4-qrel-metrics.json"))
    parser.add_argument("--trec-dir", type=Path, default=Path("test-results/m4-trec"))
    args = parser.parse_args()

    try:
        import ir_measures
        from ir_measures import AP, Judged, P, RR, nDCG
    except ImportError:
        print(
            "Missing ir-measures. Install the pinned toolchain with:\n"
            "  python3 -m venv ~/.cache/rip-m4-evaluation-venv\n"
            "  ~/.cache/rip-m4-evaluation-venv/bin/pip install -r scripts/security-intel/requirements-m4-evaluation.txt",
            file=sys.stderr,
        )
        return 3

    with args.input.open(encoding="utf-8-sig", newline="") as handle:
        rows = list(csv.DictReader(handle))
    if not rows:
        parser.error("qrel input is empty")

    pending = [
        row
        for row in rows
        if row.get("human_grade", "").strip() not in {"0", "1", "2", "3"}
    ]
    if pending:
        result = {
            "status": "pending_human_judgment",
            "candidate_pair_count": len(rows),
            "judged_pair_count": len(rows) - len(pending),
            "unjudged_pair_count": len(pending),
            "metrics": None,
            "evaluator": f"ir-measures {ir_measures.__version__}",
        }
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return 2

    grouped: dict[str, list[dict[str, str]]] = defaultdict(list)
    for row in rows:
        grouped[row["query_id"]].append(row)

    qrels = []
    run = []
    for query_id, candidates in sorted(grouped.items()):
        candidates.sort(key=lambda row: int(row["rank"]))
        if [int(row["rank"]) for row in candidates] != list(range(1, 11)):
            parser.error(f"{query_id} must contain exactly ranks 1..10")
        seen_docs = set()
        for row in candidates:
            doc_id = row["resource_id"]
            if doc_id in seen_docs:
                parser.error(f"{query_id} contains duplicate resource_id {doc_id!r}")
            seen_docs.add(doc_id)
            qrels.append(ir_measures.Qrel(query_id, doc_id, int(row["human_grade"])))
            run.append(ir_measures.ScoredDoc(query_id, doc_id, 11 - int(row["rank"])))

    args.trec_dir.mkdir(parents=True, exist_ok=True)
    qrels_path = args.trec_dir / "m4-qrels.txt"
    run_path = args.trec_dir / "m4-current-run.txt"
    with qrels_path.open("w", encoding="utf-8") as handle:
        for item in qrels:
            handle.write(f"{item.query_id} 0 {item.doc_id} {item.relevance}\n")
    with run_path.open("w", encoding="utf-8") as handle:
        for item in run:
            rank = 11 - int(item.score)
            handle.write(f"{item.query_id} Q0 {item.doc_id} {rank} {item.score:.6f} RIP-M4\n")

    measures = [
        nDCG @ 10,
        RR(rel=2) @ 10,
        P(rel=2) @ 5,
        P(rel=2) @ 10,
        AP(rel=2) @ 10,
        Judged @ 10,
    ]
    aggregate = ir_measures.calc_aggregate(measures, qrels, run)
    per_query: dict[str, dict[str, float]] = defaultdict(dict)
    for metric in ir_measures.iter_calc(measures, qrels, run):
        per_query[metric.query_id][str(metric.measure)] = round(float(metric.value), 6)

    metric_aliases = {
        "ndcg_at_10": nDCG @ 10,
        "mrr_at_10": RR(rel=2) @ 10,
        "precision_at_5": P(rel=2) @ 5,
        "precision_at_10": P(rel=2) @ 10,
        "map_at_10": AP(rel=2) @ 10,
        "judged_at_10": Judged @ 10,
    }
    metrics = {
        alias: round(float(aggregate[measure]), 6)
        for alias, measure in metric_aliases.items()
    }
    per_query_rows = []
    for query_id, candidates in sorted(grouped.items()):
        values = per_query[query_id]
        per_query_rows.append(
            {
                "query_id": query_id,
                "query": candidates[0]["query"],
                **{
                    alias: values.get(str(measure), 0.0)
                    for alias, measure in metric_aliases.items()
                },
            }
        )

    result = {
        "status": "evaluated",
        "candidate_pair_count": len(rows),
        "judged_pair_count": len(rows),
        "query_count": len(grouped),
        "relevance_threshold": 2,
        "metrics": metrics,
        "per_query": per_query_rows,
        "evaluator": f"ir-measures {ir_measures.__version__}",
        "trec_qrels": str(qrels_path),
        "trec_run": str(run_path),
        "note": (
            "Metrics use only the human-reviewed pooled top-10 candidates for these queries. "
            "They are not full-corpus recall estimates and must not be generalized beyond this pool."
        ),
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
