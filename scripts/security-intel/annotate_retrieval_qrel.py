#!/usr/bin/env python3
"""M8 qrels 交互式标注器：逐条展示查询与命中文档，键入 0-3 打分。

用法：
  .venv-check/bin/python scripts/security-intel/annotate_retrieval_qrel.py
可选参数：
  --task docs/knowledge-governance/retrieval-qrel-task-2026-11-28.jsonl
  --out  docs/knowledge-governance/retrieval-qrel-labeled-2026-11-28.jsonl

键位：0/1/2/3 打分；s 跳过；u 撤销上一条；q 保存退出。
每条即时落盘（中断不丢进度）；重新运行自动从 --out 续标。
完成后用 evaluate_retrieval.py 出指标。
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--task", default=str(ROOT / "docs/knowledge-governance/retrieval-qrel-task-2026-11-28.jsonl"))
    ap.add_argument("--out", default=str(ROOT / "docs/knowledge-governance/retrieval-qrel-labeled-2026-11-28.jsonl"))
    args = ap.parse_args()

    task_path, out_path = Path(args.task), Path(args.out)
    rows = [json.loads(l) for l in task_path.read_text().splitlines() if l.strip()]
    labeled: dict[tuple, dict] = {}
    if out_path.exists():
        for line in out_path.read_text().splitlines():
            if line.strip():
                r = json.loads(line)
                labeled[(r["query_id"], r.get("rank"), r.get("doc_id"))] = r
    pending = [r for r in rows if (r["query_id"], r.get("rank"), r.get("doc_id")) not in labeled]
    done = len(rows) - len(pending)
    print(f"任务 {len(rows)} 条，已完成 {done}，待标 {len(pending)}；0-3 打分 / s 跳过 / u 撤销 / q 保存退出")

    out_f = out_path.open("a", encoding="utf-8")
    history: list[tuple] = []
    last_query = None
    try:
        for row in pending:
            key = (row["query_id"], row.get("rank"), row.get("doc_id"))
            if row["query_id"] != last_query:
                last_query = row["query_id"]
                print(f"\n=== [{row['category']}] {row['query']}"
                      + ("（候选通道）" if row.get("include_candidates") else ""))
            if row.get("notes") == "NO_RESULT" or row.get("rank") == 0:
                labeled[key] = {**row, "relevance": None}
                out_f.write(json.dumps(labeled[key], ensure_ascii=False) + "\n")
                out_f.flush()
                continue
            print(f"  #{row['rank']} [{row.get('governance_status') or '-'}] "
                  f"{(row.get('title') or '')[:100]}  src={row.get('source_system')}")
            while True:
                choice = input("    分值 [0-3/s/u/q]> ").strip().lower()
                if choice in {"0", "1", "2", "3"}:
                    labeled[key] = {**row, "relevance": int(choice)}
                    history.append(key)
                    out_f.write(json.dumps(labeled[key], ensure_ascii=False) + "\n")
                    out_f.flush()
                    break
                if choice == "s":
                    break
                if choice == "u" and history:
                    prev = history.pop()
                    labeled.pop(prev, None)
                    print("    已撤销上一条（重跑本脚本会重新询问该条）")
                    out_f.close()
                    tmp = out_path.with_suffix(".tmp")
                    tmp.write_text("".join(json.dumps(v, ensure_ascii=False) + "\n" for v in labeled.values()))
                    tmp.replace(out_path)
                    out_f = out_path.open("a", encoding="utf-8")
                    return main()
                if choice == "q":
                    raise KeyboardInterrupt
                print("    无效输入")
    except KeyboardInterrupt:
        pass
    finally:
        out_f.close()
    print(f"\n已保存 {len(labeled)}/{len(rows)} → {out_path}\n"
          f"下一步：.venv-check/bin/python scripts/security-intel/evaluate_retrieval.py --labeled {out_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
