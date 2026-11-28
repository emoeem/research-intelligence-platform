#!/usr/bin/env python3
"""RIP retrieval evaluation with explicit AI/human label provenance."""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
from collections import defaultdict
from pathlib import Path


def ndcg(ranked: list[int], k: int) -> float:
    dcg = sum(
        (2 ** rel - 1) / math.log2(pos + 2)
        for pos, rel in enumerate(ranked[:k])
    )
    ideal = sorted(ranked, reverse=True)
    idcg = sum(
        (2 ** rel - 1) / math.log2(pos + 2)
        for pos, rel in enumerate(ideal[:k])
    )
    return dcg / idcg if idcg else 0.0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--labeled", required=True, type=Path)
    parser.add_argument("--metrics-out", type=Path)
    parser.add_argument(
        "--label-mode",
        choices=["human-ground-truth", "ai-assisted-characterization"],
        default="ai-assisted-characterization",
    )
    args = parser.parse_args()

    rows = []
    for line_no, line in enumerate(args.labeled.read_text(encoding="utf-8").splitlines(), 1):
        if line.strip():
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError as exc:
                print(json.dumps({"error": "invalid_jsonl", "line": line_no,
                                  "detail": str(exc)}, ensure_ascii=False))
                return 2
    if not rows:
        print(json.dumps({"error": "empty_labeled_file"}, ensure_ascii=False))
        return 2

    annotators = {
        str(row.get("annotator") or "").strip().lower()
        for row in rows
    }
    human = {"human", "reviewer", "human-review"}
    ai = {"ai-assist", "ai-assisted", "model"}
    if args.label_mode == "human-ground-truth":
        if annotators - human:
            print(json.dumps({
                "error": "human_annotation_required",
                "annotators": sorted(annotators),
                "allowed": sorted(human),
            }, ensure_ascii=False))
            return 2
    elif annotators - ai:
        print(json.dumps({
            "error": "ai_assisted_mode_requires_model_labels",
            "annotators": sorted(annotators),
            "allowed": sorted(ai),
        }, ensure_ascii=False))
        return 2

    by_query = defaultdict(list)
    no_result = set()
    for row in rows:
        rank = int(row.get("rank") or 0)
        rel = row.get("relevance")
        if rank > 0 and rel not in (0, 1, 2, 3):
            print(json.dumps({"error": "unlabeled_or_invalid_row",
                              "query_id": row.get("query_id"),
                              "rank": rank}, ensure_ascii=False))
            return 2
        if rank == 0:
            if row.get("notes") != "NO_RESULT" or rel is not None:
                print(json.dumps({"error": "invalid_no_result_probe",
                                  "query_id": row.get("query_id")}, ensure_ascii=False))
                return 2
            no_result.add(row["query_id"])
        else:
            by_query[row["query_id"]].append(row)

    recall5 = []
    recall10 = []
    precision5 = []
    precision10 = []
    mrr = []
    ndcg10 = []
    leakage = 0
    non_candidate = 0
    source_coverage = []

    for qid, docs in by_query.items():
        docs.sort(key=lambda row: int(row["rank"]))
        ranks = [int(row["rank"]) for row in docs]
        if ranks != list(range(1, len(ranks) + 1)):
            print(json.dumps({"error": "non_contiguous_ranks",
                              "query_id": qid, "ranks": ranks}, ensure_ascii=False))
            return 2
        keys = [
            (row.get("doc_id"), row.get("source_system") or "")
            for row in docs
        ]
        if len(keys) != len(set(keys)):
            print(json.dumps({"error": "duplicate_doc_id",
                              "query_id": qid}, ensure_ascii=False))
            return 2

        ranked_rel = [int(row.get("relevance") or 0) for row in docs]
        relevant_total = sum(rel >= 2 for rel in ranked_rel)
        for k, recall, precision in (
            (5, recall5, precision5),
            (10, recall10, precision10),
        ):
            top = ranked_rel[:k]
            hits = sum(rel >= 2 for rel in top)
            recall.append(hits / relevant_total if relevant_total else None)
            precision.append(hits / len(top) if top else None)

        first = docs[0]
        if not first.get("include_candidates"):
            non_candidate += 1
            if any(str(row.get("governance_status") or "").upper() == "CANDIDATE"
                   for row in docs):
                leakage += 1
        source_coverage.append(
            len({row.get("source_system") or "" for row in docs} - {""})
        )
        first_rel = next(
            (1.0 / pos for pos, rel in enumerate(ranked_rel, 1) if rel >= 2),
            0.0,
        )
        mrr.append(first_rel)
        ndcg10.append(ndcg(ranked_rel, 10))

    def mean(values: list[float | None]) -> float:
        values = [value for value in values if value is not None]
        return round(sum(values) / len(values), 4) if values else 0.0

    result = {
        "command": "rip_retrieval_evaluation",
        "labeled_file": str(args.labeled),
        "label_mode": args.label_mode,
        "human_ground_truth_claim": args.label_mode == "human-ground-truth",
        "labeled_sha256": hashlib.sha256(args.labeled.read_bytes()).hexdigest(),
        "labeled_row_count": len(rows),
        "queries_labeled": len(by_query),
        "no_result_rate": round(
            len(no_result) / max(1, len(by_query) + len(no_result)), 4
        ),
        "recall@5": mean(recall5),
        "recall@10": mean(recall10),
        "precision@5": mean(precision5),
        "precision@10": mean(precision10),
        "mrr": mean(mrr),
        "ndcg@10": mean(ndcg10),
        "candidate_leakage_rate": round(
            leakage / max(1, non_candidate), 4
        ),
        "source_coverage_avg": round(
            sum(source_coverage) / max(1, len(source_coverage)), 2
        ),
    }
    if args.metrics_out:
        args.metrics_out.parent.mkdir(parents=True, exist_ok=True)
        args.metrics_out.write_text(
            json.dumps(result, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
