#!/usr/bin/env python3
"""RIP 数据管线健康（只读）：ETL 最近运行状态 + 生产库完整性。

读取 migrations/etl_*_last_run.json 与 platform.db 的 PRAGMA quick_check，
不触发任何重新计算。
"""
from __future__ import annotations

import json
import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main() -> int:
    for name in ("etl_last_run.json", "etl_evidence_last_run.json", "etl_tasks_last_run.json"):
        p = ROOT / "migrations" / name
        if not p.exists():
            print(f"{name}: 缺失")
            continue
        try:
            d = json.loads(p.read_text())
            keys = {k: d[k] for k in d if isinstance(d[k], (int, str, bool)) and k != "started_at"}
            print(f"{name}: {json.dumps(keys, ensure_ascii=False)[:200]}")
        except Exception as e:
            print(f"{name}: 解析失败 {e}")

    db = ROOT / "migrations" / "platform.db"
    c = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
    ok = c.execute("PRAGMA quick_check").fetchone()[0]
    n_res = c.execute("SELECT COUNT(*) FROM resource").fetchone()[0]
    n_ev = c.execute("SELECT COUNT(*) FROM evidence").fetchone()[0]
    n_task = c.execute("SELECT COUNT(*) FROM research_task").fetchone()[0]
    print(f"platform.db: quick_check={ok} | resources={n_res} evidence={n_ev} tasks={n_task}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
