#!/usr/bin/env python3
"""Compare candidate ranking runs and optionally produce a ranx RRF fusion run."""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--qrels", type=Path, required=True, help="human-judged TREC qrels")
    parser.add_argument(
        "--run",
        action="append",
        required=True,
        metavar="NAME=PATH",
        help="TREC run file; provide at least two runs",
    )
    parser.add_argument("--output-dir", type=Path, default=Path("test-results/m4-ranking-comparison"))
    parser.add_argument("--rrf-k", type=int, default=60)
    args = parser.parse_args()

    if len(args.run) < 2:
        parser.error("at least two ranking runs are required")
    try:
        from ranx import Qrels, Run, fuse, evaluate
        import ir_measures
        from ir_measures import Judged
    except ImportError:
        print("Install scripts/security-intel/requirements-m4-evaluation.txt in an isolated venv.", file=sys.stderr)
        return 3

    qrels = Qrels.from_file(str(args.qrels), kind="trec")
    ir_qrels = list(ir_measures.read_trec_qrels(str(args.qrels)))
    runs = []
    run_names = set()
    for spec in args.run:
        if "=" not in spec:
            parser.error(f"invalid --run {spec!r}; expected NAME=PATH")
        name, path_text = spec.split("=", 1)
        if not name or name in run_names:
            parser.error(f"run names must be non-empty and unique: {name!r}")
        path = Path(path_text)
        run_names.add(name)
        runs.append(Run.from_file(str(path), kind="trec", name=name))

    coverage = {}
    for run in runs:
        trec_run = list(ir_measures.read_trec_run(str(next(Path(s.split("=", 1)[1]) for s in args.run if s.split("=", 1)[0] == run.name))))
        judged = ir_measures.calc_aggregate([Judged @ 10], ir_qrels, trec_run)[Judged @ 10]
        coverage[run.name] = round(float(judged), 6)
    incomplete = {name: value for name, value in coverage.items() if value < 1.0}
    if incomplete:
        print(
            json.dumps(
                {
                    "status": "blocked_unjudged_results",
                    "judged_at_10": coverage,
                    "reason": "Every compared top-10 result must have a human qrel; unjudged is not treated as irrelevant.",
                },
                ensure_ascii=False,
                indent=2,
            )
        )
        return 2

    metrics = ["ndcg@10", "mrr@10", "map@10", "precision@10"]
    results = {
        run.name: {
            key: float(value)
            for key, value in evaluate(qrels, run, metrics=metrics).items()
        }
        for run in runs
    }
    fused = fuse(runs, method="rrf", params={"k": args.rrf_k})
    args.output_dir.mkdir(parents=True, exist_ok=True)
    fused_path = args.output_dir / f"ranx-rrf-k{args.rrf_k}.run"
    lines = []
    for query_id, docs in sorted(fused.run.items()):
        ranked = sorted(docs.items(), key=lambda item: (-float(item[1]), str(item[0])))
        for rank, (doc_id, score) in enumerate(ranked, 1):
            lines.append(f"{query_id} Q0 {doc_id} {rank} {float(score):.12f} ranx-rrf-k{args.rrf_k}")
    fused_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    fused_trec = list(ir_measures.read_trec_run(str(fused_path)))
    fused_coverage = ir_measures.calc_aggregate([Judged @ 10], ir_qrels, fused_trec)[Judged @ 10]
    if fused_coverage < 1.0:
        raise RuntimeError("RRF fusion introduced unjudged top-10 documents; refusing to score it.")

    result = {
        "status": "compared",
        "library": "ranx",
        "metrics": metrics,
        "judged_at_10": coverage,
        "runs": results,
        "rrf": {
            "k": args.rrf_k,
            "run_file": str(fused_path),
            "metrics": {
                key: float(value)
                for key, value in evaluate(qrels, fused, metrics=metrics).items()
            },
        },
        "note": "RRF is an offline experiment only; it does not replace RIP's Rust production fusion implementation.",
    }
    (args.output_dir / "comparison.json").write_text(
        json.dumps(result, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
