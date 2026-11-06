#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
M1 第二批：ID 冲突审计（全量，计划 §5.4 硬门槛）
================================================

版本：1.1.1（M6 起 alias↔实体一致性补 tool/skill/workflow 三类，etl_m6.py 域）
  v1.1.1 变更（M1 全量批）：目标库可配置（RP_TARGET_DB 环境变量，全量批重建
  流程 platform-fullbatch.db 用），默认仍为 platform-dryrun.db；审计规则不变。

审计范围（三源现有稳定 ID，全量）：
  A. catalog.db resources.resource_id（81，gh:/url: 两型）
  B. knowledge.db poc.code（14,717，含 14,111 条 nuclei 模板索引 code，全量口径）
  C. RTIS task id（35 个 task JSON 的 task.id + 36 个任务目录名，格式
     rtis-YYYYMMDD-HHMMSS-ms）
  D. 隔离目标库中的确定性派生 ID：knowledge neg:*（17）、review
     eri-catalog:review_log:*（165）、evidence EVD-*（23）、research_task
     沿用 C 的现网 ID（35）

检查项：
  1. 全局唯一性：各 ID 集内部重复 = 0。
  2. 跨源碰撞：A/B/C/D 两两精确交集 = 0（C 与 research_task 主键的同名沿用
     属"同一现网 ID 有意沿用为平台主键"，单列为 intentional reuse，不计冲突）。
  3. 格式合规：resource_id（gh:owner/repo | url:hash16）、poc.code（非空、无
     空白/控制字符）、task id（rtis-\\d{8}-\\d{6}-\\d{2,}）、派生 ID 前缀格式。
  4. alias 表与实体主键一致性：entity_alias 每行可解析到对应实体主键（正向），
     每个已迁移实体主键有别名（反向）。

结论输出（机器可读 id_audit_result.json + 人读摘要 stdout）：
  全部通过 → 「现网 ID 可直接沿用，无需批量生成新 ID」；任一失败 → 列出冲突
  清单并给出相反结论。

约束：三源与目标库全部只读；stdlib-only；重复执行产出一致（覆盖写 JSON）。
"""

import hashlib
import json
import re
import sqlite3
import sys
from datetime import datetime, timezone
from pathlib import Path

ID_AUDIT_VERSION = "1.1.1"

MIG_DIR = Path(__file__).resolve().parent

SRC_CATALOG = Path("/home/emo/code/pensting/security-research-upgrade/eri/db/catalog.db")
SRC_KNOWLEDGE = Path("/home/emo/.dsh/redteam/knowledge.db")
SRC_TASKS_DIR = Path("/home/emo/.dsh/redteam/rtis/data/tasks")


def _target_db() -> Path:
    """目标库：RP_TARGET_DB 环境变量可覆盖（M1 全量批重建流程用）；
    相对路径按 migrations/ 解析；默认隔离库 platform-dryrun.db。"""
    import os
    v = os.environ.get("RP_TARGET_DB")
    if not v:
        return MIG_DIR / "platform-dryrun.db"
    p = Path(v)
    return p if p.is_absolute() else MIG_DIR / p


TARGET_DB = _target_db()
RESULT_JSON = MIG_DIR / "id_audit_result.json"

RE_RESOURCE_GH = re.compile(r"^gh:[^\s/]+/[^\s]+$")
RE_RESOURCE_URL = re.compile(r"^url:[0-9a-f]{16}$")
RE_TASK_ID = re.compile(r"^rtis-\d{8}-\d{6}-\d{2,}$")
RE_NEG_ID = re.compile(r"^neg:\S+$")
RE_REVIEW_ID = re.compile(r"^eri-catalog:review_log:\d+$")
RE_EVD_ID = re.compile(r"^EVD-[A-Za-z0-9_-]+-(http_evidence|shot)-\d+$")

WS_OR_CTRL = re.compile(r"[\s\x00-\x1f\x7f]")


def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def open_ro(path: Path) -> sqlite3.Connection:
    if not path.exists():
        raise SystemExit(f"FATAL: 库不存在：{path}")
    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    conn.row_factory = sqlite3.Row
    return conn


def dup_report(values: list[str]) -> list[dict]:
    seen: dict[str, int] = {}
    for v in values:
        seen[v] = seen.get(v, 0) + 1
    return [{"id": k, "occurrences": n} for k, n in sorted(seen.items()) if n > 1]


def audit_resource_ids(cat: sqlite3.Connection) -> dict:
    ids = [r[0] for r in cat.execute("SELECT resource_id FROM resources ORDER BY resource_id")]
    dups = dup_report(ids)
    violations = [i for i in ids
                  if not (RE_RESOURCE_GH.match(i) or RE_RESOURCE_URL.match(i))]
    prefixes: dict[str, int] = {}
    for i in ids:
        p = i.split(":", 1)[0] if ":" in i else "(none)"
        prefixes[p] = prefixes.get(p, 0) + 1
    return {
        "source": "catalog.db resources.resource_id",
        "count": len(ids), "distinct": len(set(ids)),
        "uniqueness_pass": len(dups) == 0,
        "duplicates": dups,
        "format_rule": "gh:owner/repo 或 url:16位小写hex",
        "format_violations": violations,
        "prefix_distribution": dict(sorted(prefixes.items())),
    }


def audit_poc_codes(kb: sqlite3.Connection) -> dict:
    ids = [r[0] for r in kb.execute("SELECT code FROM poc ORDER BY code")]
    dups = dup_report(ids)
    violations = [i for i in ids
                  if (not i) or bool(WS_OR_CTRL.search(i)) or len(i) > 256]
    nuclei = [r[0] for r in kb.execute("SELECT code FROM poc WHERE source='nuclei'")]
    cjk = sum(1 for i in ids if any(ord(ch) > 0x2E7F for ch in i))
    return {
        "source": "knowledge.db poc.code（全量，含 nuclei 模板索引）",
        "count": len(ids), "distinct": len(set(ids)),
        "uniqueness_pass": len(dups) == 0,
        "duplicates": dups,
        "format_rule": "非空且不含空白/控制字符，长度<=256（源约束 UNIQUE(code)）",
        "format_violations": violations,
        "charset_profile": {
            "nuclei_template_codes": len(nuclei),
            "contains_cjk_or_wide": cjk,
            "ascii_only": len(ids) - cjk,
        },
    }


def audit_task_ids() -> dict:
    files = sorted(SRC_TASKS_DIR.glob("*.json"))
    ids, parse_errors = [], []
    for f in files:
        try:
            with open(f, encoding="utf-8") as fh:
                t = (json.load(fh).get("task") or {})
            tid = t.get("id")
            if not tid:
                parse_errors.append({"file": f.name, "error": "task.id 缺失"})
            else:
                ids.append(tid)
        except Exception as e:  # noqa: BLE001
            parse_errors.append({"file": f.name, "error": repr(e)})
    dirs = sorted(p.name for p in SRC_TASKS_DIR.iterdir() if p.is_dir())
    dups = dup_report(ids)
    violations = [i for i in ids if not RE_TASK_ID.match(i)]
    dir_violations = [d for d in dirs if not RE_TASK_ID.match(d)]
    return {
        "source": "RTIS rtis/data/tasks/*.json task.id + 任务目录名",
        "json_count": len(files), "id_count": len(ids), "distinct": len(set(ids)),
        "uniqueness_pass": len(dups) == 0,
        "duplicates": dups,
        "format_rule": "rtis-YYYYMMDD-HHMMSS-ms（ms 为 2 位以上数字）",
        "format_violations": violations,
        "dir_count": len(dirs),
        "dir_format_violations": dir_violations,
        "dir_without_json": sorted(set(dirs) - set(ids)),
        "json_without_dir": sorted(set(ids) - set(dirs)),
        "parse_errors": parse_errors,
        "pairing_note": "目录与 JSON 的配对差异属工件布局事实，登记备查，不构成 ID 冲突",
    }


def audit_derived_target(tgt: sqlite3.Connection) -> dict:
    def one(sql: str) -> list[str]:
        return [r[0] for r in tgt.execute(sql)]

    neg = one("SELECT knowledge_id FROM knowledge WHERE record_kind='negative'")
    rev = one("SELECT review_id FROM review")
    evd = one("SELECT evidence_id FROM evidence")
    rt = one("SELECT task_id FROM research_task")
    checks = {
        "knowledge_negative": (neg, RE_NEG_ID),
        "review": (rev, RE_REVIEW_ID),
        "evidence": (evd, RE_EVD_ID),
        "research_task": (rt, RE_TASK_ID),
    }
    out = {}
    for name, (ids, rx) in checks.items():
        dups = dup_report(ids)
        out[name] = {
            "count": len(ids), "distinct": len(set(ids)),
            "uniqueness_pass": len(dups) == 0,
            "duplicates": dups,
            "format_rule": rx.pattern,
            "format_violations": [i for i in ids if not rx.match(i)],
        }
    return out


def audit_cross_source(sets: dict[str, set[str]]) -> dict:
    names = sorted(sets)
    pairs, count = [], 0
    for i, a in enumerate(names):
        for b in names[i + 1:]:
            inter = sets[a] & sets[b]
            if inter:
                count += len(inter)
                pairs.append({"set_a": a, "set_b": b,
                              "intersection_size": len(inter),
                              "sample": sorted(inter)[:10]})
    return {"count": count, "pairs": pairs}


def audit_alias_entity_consistency(tgt: sqlite3.Connection) -> dict:
    pk_sql = {
        "resource": "SELECT resource_id FROM resource",
        "resource_source": "SELECT CAST(id AS TEXT) FROM resource_source",
        "knowledge": "SELECT knowledge_id FROM knowledge",
        "review": "SELECT review_id FROM review",
        "research_task": "SELECT task_id FROM research_task",
        "evidence": "SELECT evidence_id FROM evidence",
        # M6 扩展（etl_m6.py 域）：tool/skill 沿用现网 name，workflow 沿用源 WF-xx id
        "tool": "SELECT tool_id FROM tool",
        "skill": "SELECT skill_id FROM skill",
        "workflow": "SELECT workflow_id FROM workflow",
    }
    per_type = {}
    alias_rows = tgt.execute(
        "SELECT entity_type, entity_id FROM entity_alias").fetchall()
    by_type: dict[str, list[str]] = {}
    for r in alias_rows:
        by_type.setdefault(r["entity_type"], []).append(r["entity_id"])
    for etype, sql in pk_sql.items():
        pks = {r[0] for r in tgt.execute(sql)}
        al = by_type.get(etype, [])
        forward_missing = sorted(set(al) - pks)
        reverse_missing = sorted(pks - set(al))
        per_type[etype] = {
            "pk_count": len(pks), "alias_count": len(al),
            "forward_missing": forward_missing,
            "reverse_missing": reverse_missing,
            "pass": not forward_missing and not reverse_missing,
        }
    empty_types = sorted(set(by_type) - set(pk_sql))
    return {
        "pass": all(v["pass"] for v in per_type.values()) and not empty_types,
        "per_type": per_type,
        "alias_types_without_pk_table": empty_types,
    }


def main() -> int:
    print("=" * 72)
    print(f"M1 第二批 ID 冲突审计 v{ID_AUDIT_VERSION}（计划 §5.4 硬门槛；全量只读）")
    print("=" * 72)

    cat = open_ro(SRC_CATALOG)
    kb = open_ro(SRC_KNOWLEDGE)
    tgt = sqlite3.connect(str(TARGET_DB))
    tgt.row_factory = sqlite3.Row

    res = audit_resource_ids(cat)
    poc = audit_poc_codes(kb)
    task = audit_task_ids()
    derived = audit_derived_target(tgt)
    alias = audit_alias_entity_consistency(tgt)

    sets = {
        "catalog_resource_id": {r[0] for r in cat.execute("SELECT resource_id FROM resources")},
        "knowledge_poc_code": {r[0] for r in kb.execute("SELECT code FROM poc")},
        "rtis_task_id": set(),
    }
    files = sorted(SRC_TASKS_DIR.glob("*.json"))
    for f in files:
        with open(f, encoding="utf-8") as fh:
            tid = (json.load(fh).get("task") or {}).get("id")
        if tid:
            sets["rtis_task_id"].add(tid)
    sets["target_knowledge_negative"] = {r[0] for r in tgt.execute(
        "SELECT knowledge_id FROM knowledge WHERE record_kind='negative'")}
    sets["target_review"] = {r[0] for r in tgt.execute("SELECT review_id FROM review")}
    sets["target_evidence"] = {r[0] for r in tgt.execute("SELECT evidence_id FROM evidence")}

    collisions = audit_cross_source(sets)
    rt_ids = {r[0] for r in tgt.execute("SELECT task_id FROM research_task")}
    intentional = {
        "set_a": "rtis_task_id", "set_b": "research_task.task_id",
        "intersection_size": len(sets["rtis_task_id"] & rt_ids),
        "note": "同一现网 task ID 有意沿用为平台主键（002_tasks.sql），非跨源冲突",
    }

    uniqueness_pass = (res["uniqueness_pass"] and poc["uniqueness_pass"]
                       and task["uniqueness_pass"]
                       and all(v["uniqueness_pass"] for v in derived.values()))
    format_violations = (res["format_violations"] + poc["format_violations"]
                         + task["format_violations"] + task["dir_format_violations"]
                         + [v for d in derived.values() for v in d["format_violations"]])
    format_pass = not format_violations
    alias_pass = alias["pass"]

    if uniqueness_pass and format_pass and collisions["count"] == 0 and alias_pass:
        conclusion = "现网 ID 可直接沿用，无需批量生成新 ID"
    else:
        conclusion = ("存在冲突/违规，不满足直接沿用前提："
                      "uniqueness=%s format=%s collisions=%d alias=%s"
                      % (uniqueness_pass, format_pass, collisions["count"], alias_pass))

    result = {
        "id_audit_version": ID_AUDIT_VERSION,
        "generated_at": now_iso(),
        "scope": {
            "catalog": str(SRC_CATALOG),
            "knowledge": str(SRC_KNOWLEDGE),
            "tasks_dir": str(SRC_TASKS_DIR),
            "target_db": str(TARGET_DB),
            "mode": "全量只读（计划 §5.4：ID 冲突审计完成前禁止批量生成新 ID）",
        },
        "sources": {
            "catalog_resource_id": res,
            "knowledge_poc_code": poc,
            "rtis_task_id": task,
        },
        "derived_target_ids": derived,
        "cross_source_collisions": collisions,
        "intentional_reuse": intentional,
        "alias_entity_consistency": alias,
        "uniqueness_pass": uniqueness_pass,
        "format_pass": format_pass,
        "alias_pass": alias_pass,
        "collision_count": collisions["count"],
        "conclusion": conclusion,
    }
    RESULT_JSON.write_text(
        json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8")

    print(f"A. catalog resource_id : {res['count']} 行，唯一={res['uniqueness_pass']}"
          f"，格式违规={len(res['format_violations'])}，前缀分布={res['prefix_distribution']}")
    print(f"B. knowledge poc.code  : {poc['count']} 行，唯一={poc['uniqueness_pass']}"
          f"，格式违规={len(poc['format_violations'])}"
          f"（nuclei 模板 {poc['charset_profile']['nuclei_template_codes']}，"
          f"含 CJK {poc['charset_profile']['contains_cjk_or_wide']}）")
    print(f"C. RTIS task id        : JSON {task['json_count']} / 目录 {task['dir_count']}"
          f"，唯一={task['uniqueness_pass']}，格式违规={len(task['format_violations'])}"
          f"，目录无JSON={len(task['dir_without_json'])}，JSON无目录={len(task['json_without_dir'])}")
    for name, d in derived.items():
        print(f"D. 派生 {name:<22}: {d['count']} 行，唯一={d['uniqueness_pass']}"
              f"，格式违规={len(d['format_violations'])}")
    print(f"2. 跨源碰撞：{collisions['count']}（"
          f"intentional reuse: rtis_task_id∩research_task={intentional['intersection_size']}）")
    print("4. alias↔主键一致性：", end="")
    for etype, v in alias["per_type"].items():
        print(f"{etype}(pk={v['pk_count']},alias={v['alias_count']},"
              f"fwd缺={len(v['forward_missing'])},rev缺={len(v['reverse_missing'])}) ", end="")
    print(f"\n   pass={alias['pass']}")
    print("-" * 72)
    print(f"结论：{conclusion}")
    print(f"机器可读结果：{RESULT_JSON}")
    cat.close()
    kb.close()
    tgt.close()
    return 0 if (uniqueness_pass and format_pass and collisions["count"] == 0
                 and alias_pass) else 1


if __name__ == "__main__":
    sys.exit(main())
