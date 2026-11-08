#!/usr/bin/env python3
"""RIP 资源清单统计（只读）。

用法: rip_inventory.py [质量等级过滤 A/B/C/D，留空输出全部统计]
"""
from __future__ import annotations

import sqlite3
import sys
from collections import Counter
from pathlib import Path

DB = Path(__file__).resolve().parents[1] / "migrations" / "platform.db"


def main() -> int:
    grade = sys.argv[1].strip().upper() if len(sys.argv) > 1 else ""
    c = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
    c.row_factory = sqlite3.Row

    total = c.execute("SELECT COUNT(*) FROM resource").fetchone()[0]
    by_grade = Counter(r[0] or "?" for r in c.execute("SELECT quality_grade FROM resource"))
    by_lifecycle = Counter(r[0] or "?" for r in c.execute("SELECT lifecycle FROM resource"))
    by_trust = Counter(r[0] or "?" for r in c.execute("SELECT trust_level FROM resource"))
    print(f"资源总量: {total}")
    print("按质量等级:", dict(sorted(by_grade.items())))
    print("按生命周期:", dict(sorted(by_lifecycle.items())))
    print("按信任级:", dict(sorted(by_trust.items())))

    if grade:
        rows = c.execute(
            "SELECT resource_id, name, lifecycle, quality_final_score, quality_reasons "
            "FROM resource WHERE quality_grade = ? ORDER BY resource_id", (grade,)).fetchall()
        print(f"\n== 质量等级 {grade}（{len(rows)} 条）==")
        for r in rows[:40]:
            reasons = (r["quality_reasons"] or "").replace("\n", " ")[:70]
            print(f"  {r['resource_id']}  {r['name'][:36]}  [{r['lifecycle']}] 分 {r['quality_final_score']}")
            if reasons:
                print(f"      {reasons}")
        if len(rows) > 40:
            print(f"  ... 其余 {len(rows) - 40} 条省略")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
