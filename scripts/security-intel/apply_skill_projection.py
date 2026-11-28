#!/usr/bin/env python3
"""M7 Skill 投影回填：把 rp-batch-review project 生成的计划写回
platform.db 的 skill.required_tools / knowledge_refs。

纪律（M7）：只回填 APPROVED 关系投影；计划由 rip_platform 台账生成，本脚本
不改变任何审核状态；dry-run 默认，--apply 显式提交；写前自动备份 platform.db
到 rip-data/dev/backups/；字段格式为 JSON 数组字符串（'["nmap"]'），空集写
NULL（清除投影）。

用法：
  rp-batch-review project | apply_skill_projection.py            # dry-run（stdin 计划）
  rp-batch-review project | apply_skill_projection.py --apply
  apply_skill_projection.py --plan plan.json --apply [--db PATH]
"""
from __future__ import annotations

import json
import os
import shutil
import sqlite3
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_DB = ROOT / "migrations/platform.db"
BACKUP_DIR = ROOT / "rip-data/dev/backups"
BATCH_BIN = ROOT / "target/debug/rp-batch-review"


def load_plan_from_cli() -> dict:
    dsn = os.environ.get("RP_PLATFORM_PG_DSN") or os.environ.get("RP_PG_DSN") or ""
    if not dsn:
        print(json.dumps({"error": "RP_PLATFORM_PG_DSN 未配置（source rip-data/dev/rip-platform-dsn.env）"}))
        sys.exit(2)
    env = dict(os.environ)
    env["RP_PG_DSN"] = dsn
    proc = subprocess.run(
        [str(BATCH_BIN), "project"], capture_output=True, text=True,
        timeout=120, cwd=str(ROOT), env=env, check=False,
    )
    if proc.returncode != 0:
        print(proc.stderr[-800:] or proc.stdout[-800:])
        sys.exit(2)
    return json.loads(proc.stdout)


def norm_field(raw) -> list:
    if raw in (None, "", "null"):
        return []
    try:
        value = json.loads(raw)
    except json.JSONDecodeError:
        return []
    return value if isinstance(value, list) else []


def main() -> int:
    argv = sys.argv[1:]
    apply_mode = False
    db_path = DEFAULT_DB
    plan_file: Path | None = None
    i = 0
    while i < len(argv):
        if argv[i] == "--apply":
            apply_mode = True
        elif argv[i] == "--db" and i + 1 < len(argv):
            db_path = Path(argv[i + 1])
            i += 1
        elif argv[i] == "--plan" and i + 1 < len(argv):
            plan_file = Path(argv[i + 1])
            i += 1
        else:
            plan_file = Path(argv[i])
        i += 1

    if plan_file is not None:
        plan_doc = json.loads(plan_file.read_text())
    else:
        plan_doc = load_plan_from_cli()
    plan = plan_doc.get("plan") or []
    if not isinstance(plan, list):
        print(json.dumps({"error": "plan 格式非法（缺 plan 数组）"}))
        return 2

    conn = sqlite3.connect(f"file:{db_path}?mode=rw", uri=True)
    conn.row_factory = sqlite3.Row
    try:
        rows = conn.execute(
            "SELECT skill_id, required_tools, knowledge_refs FROM skill WHERE enabled=1"
        ).fetchall()
        known = {r["skill_id"].removeprefix("skill:"): r for r in rows}

        changes = []
        missing = []
        for entry in plan:
            skill = entry.get("skill") or ""
            if skill not in known:
                missing.append(skill)
                continue
            row = known[skill]
            cur_tools = sorted(norm_field(row["required_tools"]))
            cur_refs = sorted(norm_field(row["knowledge_refs"]))
            new_tools = sorted(set(entry.get("required_tools") or []))
            new_refs = sorted(set(entry.get("knowledge_refs") or []))
            if cur_tools != new_tools or cur_refs != new_refs:
                changes.append({
                    "skill": skill,
                    "required_tools": {"from": cur_tools, "to": new_tools},
                    "knowledge_refs": {"from": cur_refs, "to": new_refs},
                })

        result = {
            "command": "apply_skill_projection",
            "mode": "applied" if apply_mode else "dry_run",
            "plan_skills": len(plan),
            "changed": len(changes),
            "missing_in_platform_db": missing,
            "changes": changes,
        }
        if not apply_mode:
            result["next_step"] = "rerun with --apply to write platform.db (auto-backup first)"
            print(json.dumps(result, ensure_ascii=False, indent=2))
            return 0
        if not changes:
            result["note"] = "nothing to change"
            print(json.dumps(result, ensure_ascii=False, indent=2))
            return 0

        BACKUP_DIR.mkdir(parents=True, exist_ok=True)
        backup = BACKUP_DIR / f"platform-pre-skillprojection-{time.strftime('%Y%m%d-%H%M%S')}.db"
        shutil.copy2(db_path, backup)
        for change in changes:
            row = known[change["skill"]]
            tools = json.dumps(change["required_tools"]["to"]) if change["required_tools"]["to"] else None
            refs = json.dumps(change["knowledge_refs"]["to"]) if change["knowledge_refs"]["to"] else None
            conn.execute(
                "UPDATE skill SET required_tools=?, knowledge_refs=? WHERE skill_id=?",
                (tools, refs, row["skill_id"]),
            )
        conn.commit()
        result["backup"] = str(backup)
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return 0
    finally:
        conn.close()


if __name__ == "__main__":
    sys.exit(main())
