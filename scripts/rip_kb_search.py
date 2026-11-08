#!/usr/bin/env python3
"""RIP 知识/工具/资源/任务关键词检索（只读）。

用法: rip_kb_search.py <关键词> [每组最大条数，默认 5]
对 platform.db 的 tool/skill/resource/knowledge/research_task 做 LIKE 检索，分组输出。
永不写库。
"""
from __future__ import annotations

import sqlite3
import sys
from pathlib import Path

DB = Path(__file__).resolve().parents[1] / "migrations" / "platform.db"

QUERIES = [
    ("工具", "tool",
     "SELECT tool_id, name, health_state FROM tool WHERE tool_id LIKE ? OR name LIKE ? ORDER BY tool_id LIMIT ?",
     lambda r: f"{r['tool_id']}  {r['name']}  [健康:{r['health_state']}]"),
    ("技能", "skill",
     "SELECT skill_id, name, substr(description,1,60) AS d FROM skill "
     "WHERE skill_id LIKE ? OR name LIKE ? OR description LIKE ? ORDER BY skill_id LIMIT ?",
     lambda r: f"{r['skill_id']}  {r['name']}  {r['d'] or ''}"),
    ("资源", "resource",
     "SELECT resource_id, name, lifecycle, quality_grade, trust_level FROM resource "
     "WHERE resource_id LIKE ? OR name LIKE ? OR description LIKE ? ORDER BY resource_id LIMIT ?",
     lambda r: f"{r['resource_id']}  {r['name']}  [等级:{r['quality_grade']} 信任:{r['trust_level']}]"),
    ("知识", "knowledge",
     "SELECT knowledge_id, title, lifecycle, verified, severity, component FROM knowledge "
     "WHERE knowledge_id LIKE ? OR title LIKE ? OR description LIKE ? OR component LIKE ? OR tags LIKE ? "
     "ORDER BY knowledge_id LIMIT ?",
     lambda r: (f"知识 {r['knowledge_id'][:40]}  {r['title'][:50]}  "
                f"[{r['lifecycle']}/{'已验证' if r['verified'] else '未验证'}"
                f"{'/' + r['severity'] if r['severity'] else ''}]")),
    ("研究任务", "research_task",
     "SELECT task_id, title, state, target FROM research_task "
     "WHERE task_id LIKE ? OR title LIKE ? OR objective LIKE ? OR target LIKE ? ORDER BY task_id LIMIT ?",
     lambda r: f"{r['task_id']}  {r['title'][:44]}  [{r['state']}" +
               (f" | 目标:{r['target']}" if r["target"] else "") + "]"),
]


def main() -> int:
    if len(sys.argv) < 2 or not sys.argv[1].strip():
        print(__doc__)
        return 64
    kw = sys.argv[1].strip()
    limit = int(sys.argv[2]) if len(sys.argv) > 2 else 5
    like = f"%{kw}%"
    c = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
    c.row_factory = sqlite3.Row

    hit_total = 0
    for label, _table, sql, fmt in QUERIES:
        try:
            rows = c.execute(sql, (like,) * (sql.count("?") - 1) + (limit,)).fetchall()
        except sqlite3.Error as e:
            print(f"[{label}] 查询失败: {e}")
            continue
        print(f"== {label} ==")
        if rows:
            hit_total += len(rows)
            for r in rows:
                print("  ", fmt(r))
        else:
            print("   (无命中)")
    print(f"\n共 {hit_total} 条命中（每组最多 {limit} 条）。只读查询。")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
