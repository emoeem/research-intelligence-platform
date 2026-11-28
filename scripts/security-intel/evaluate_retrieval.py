#!/usr/bin/env python3
"""M8 检索质量评测器：读人工标注的 qrels JSONL，输出文档 §M8 要求的指标。

relevant 定义：relevance >= 2（3=直接命中，2=相关；1=沾边不算 relevant）。

指标：Recall@5/10、Precision@5/10、MRR、nDCG@10、No-result Rate、
Source Coverage（各查询 Top10 的去重来源数分布）、Candidate Leakage Rate
（非 candidates 查询里出现 governance_status=CANDIDATE 的占比）。

用法：
  evaluate_retrieval.py --labeled docs/knowledge-governance/retrieval-qrel-labeled-2026-11-28.jsonl
"""
from __future__ import annotations

import argparse
import json
import math
import sys
from collections import defaultdict
from pathlib import Path

RELEVANT_MIN = 2


def load(path: Path) -> list[dict]:
    rows = []
    for line in path.read_text().splitlines():
        line = line.strip()
        if not line:
            continue
        row = json.loads(line)
        if row.get("relevance") is None and row.get("rank", 0) > 0:
            print(json.dumps({"error": "unlabeled row", "query_id": row.get("query_id"),
                              "rank": row.get("rank")}, ensure_ascii=False))
            sys.exit(2)
        rows.append(row)
    return rows


def ndcg_at_k(ranked_rel: list[int], k: int) -> float:
    dcg = sum((2 ** rel - 1) / math.log2(pos + 2)
              for pos, rel in enumerate(ranked_rel[:k]))
    ideal = sorted(ranked_rel, reverse=True)
    idcg = sum((2 ** rel - 1) / math.log2(pos + 2)
               for pos, rel in enumerate(ideal[:k]))
    return dcg / idcg if idcg > 0 else 0.0


HUMAN_ANNOTATORS = {"human", "reviewer", "human-review"}
AI_ANNOTATORS = {"ai-assist", "ai-assisted", "ai"}


def enforce_label_mode(rows: list[dict], label_mode: str) -> dict:
    """PB-004 双轨强制：AI-only 行不得进 human Ground Truth 轨。

    ai-assisted-characterization（缺省）：系统自动运行/持续监测轨，
    允许 ai-* 与空 annotator；human-ground-truth：仅人工标注行，
    命中 ai-* annotator 直接 exit 3 拒绝（不产出可被误引用为人工
    GT 的指标行）。
    """
    annotator_counts: dict[str, int] = {}
    for row in rows:
        annotator = (row.get("annotator") or "unlabeled").strip().lower() or "unlabeled"
        annotator_counts[annotator] = annotator_counts.get(annotator, 0) + 1
    ai_rows = sum(v for k, v in annotator_counts.items() if k in AI_ANNOTATORS)
    human_rows = sum(v for k, v in annotator_counts.items() if k in HUMAN_ANNOTATORS)
    if label_mode == "human-ground-truth":
        if ai_rows > 0:
            print(json.dumps({
                "error": "ai-assisted rows cannot be promoted to human Ground Truth",
                "label_mode": label_mode,
                "ai_assist_rows": ai_rows,
                "fix": "rerun with --label-mode ai-assisted-characterization, "
                       "or replace AI scores via human review/dispute repair",
            }, ensure_ascii=False))
            sys.exit(3)
        if human_rows == 0:
            print(json.dumps({
                "error": "human-ground-truth mode requires annotator in "
                         f"{sorted(HUMAN_ANNOTATORS)} on rows",
                "label_mode": label_mode,
            }, ensure_ascii=False))
            sys.exit(3)
    return {
        "label_mode": label_mode,
        "human_ground_truth": label_mode == "human-ground-truth",
        "annotator_counts": annotator_counts,
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--labeled", required=True)
    ap.add_argument("--relevant-min", type=int, default=RELEVANT_MIN)
    ap.add_argument(
        "--label-mode",
        choices=["ai-assisted-characterization", "human-ground-truth"],
        default="ai-assisted-characterization",
        help="缺省 ai-assisted-characterization（PB-004：AI-only 行不进人工 GT 轨）",
    )
    args = ap.parse_args()

    rows = load(Path(args.labeled))
    label_track = enforce_label_mode(rows, args.label_mode)
    by_query: dict[str, list[dict]] = defaultdict(list)
    no_result_queries: set[str] = set()
    for row in rows:
        if row.get("notes") == "NO_RESULT" or row.get("rank") == 0:
            no_result_queries.add(row["query_id"])
            continue
        by_query[row["query_id"]].append(row)

    recall_at = {5: [], 10: []}
    precision_at = {5: [], 10: []}
    mrr_list: list[float] = []
    ndcg10_list: list[float] = []
    leakage_queries = 0
    non_candidate_queries = 0
    sources_per_query: list[int] = []
    per_category: dict[str, list[dict]] = defaultdict(list)

    for qid, docs in by_query.items():
        docs = sorted(docs, key=lambda d: d["rank"])
        ranked_rel = [int(d.get("relevance") or 0) for d in docs]
        total_rel = sum(1 for r in ranked_rel if r >= args.relevant_min)
        category = docs[0].get("category") or "unknown"
        is_candidate_query = bool(docs[0].get("include_candidates"))
        if not is_candidate_query:
            non_candidate_queries += 1
            if any((d.get("governance_status") or "").upper() == "CANDIDATE" for d in docs):
                leakage_queries += 1
        sources_per_query.append(len({d.get("source_system") or "" for d in docs} - {""}))

        for k in (5, 10):
            top = ranked_rel[:k]
            hits = sum(1 for r in top if r >= args.relevant_min)
            recall_at[k].append(hits / total_rel if total_rel else None)
            precision_at[k].append(hits / len(top) if top else None)
        mrr = 0.0
        for pos, r in enumerate(ranked_rel, start=1):
            if r >= args.relevant_min:
                mrr = 1.0 / pos
                break
        mrr_list.append(mrr)
        ndcg10_list.append(ndcg_at_k(ranked_rel, 10))
        per_category[category].append({
            "recall10": recall_at[10][-1], "mrr": mrr, "ndcg10": ndcg10_list[-1],
        })

    def mean(values: list) -> float:
        vals = [v for v in values if v is not None]
        return round(sum(vals) / len(vals), 4) if vals else 0.0

    result = {
        "command": "evaluate_retrieval",
        "labeled_file": args.labeled,
        "relevant_min": args.relevant_min,
        "label_track": label_track,
        "queries_labeled": len(by_query),
        "no_result_queries": len(no_result_queries),
        "no_result_rate": round(len(no_result_queries) /
                                max(1, len(by_query) + len(no_result_queries)), 4),
        "recall@5": mean(recall_at[5]),
        "recall@10": mean(recall_at[10]),
        "precision@5": mean(precision_at[5]),
        "precision@10": mean(precision_at[10]),
        "mrr": mean(mrr_list),
        "ndcg@10": mean(ndcg10_list),
        "candidate_leakage_rate": round(leakage_queries / max(1, non_candidate_queries), 4),
        "source_coverage_avg": round(sum(sources_per_query) / max(1, len(sources_per_query)), 2),
        "per_category_avg": {
            cat: {
                "queries": len(items),
                "recall@10": mean([i["recall10"] for i in items]),
                "mrr": mean([i["mrr"] for i in items]),
                "ndcg@10": mean([i["ndcg10"] for i in items]),
            }
            for cat, items in sorted(per_category.items())
        },
    }
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
