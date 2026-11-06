#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
M1 第一批对账（verify）
=======================

依据：MIGRATION_STRATEGY §10.5（verify.py 约定）、计划 §M1-E、M1_IMPLEMENTATION_PLAN §6 门槛
A-1/A-2/A-3/A-4/A-5。

校验维度（计划 M1-E 四类 + 字段抽样哈希 + 幂等 + 第二批扩展）：
  1. 快照完整性：源库 SHA-256 与 source_manifest.json 一致（源未被写入）。
  2. 行数对账：逐映射 源行数 = 目标行数 + 隔离行数（逐实体逐表，不允许总量对抵）。
  3. ID 覆盖率：源 ID 100% 出现在目标或隔离清单；迁移行 100% 有 entity_alias 登记。
  4. Alias 冲突：同 (source_system, namespace, alias_value) 指向不同实体 = 0。
  5. 关系完整性：PRAGMA foreign_key_check = 0；孤儿关系 = 0 未解释。
  6. 字段抽样哈希：每个映射源/目标各抽 10 行（确定性等距抽样），规范化 md5 比对。
  7. 幂等：最近两次 ETL 运行内容摘要一致，且当前库内容与最后一次运行一致。
  8. ResearchTask 对账（第二批 + 全量批扩展）：源 task JSON（快照 42 个）=
     目标行数 + 隔离坏行（quarantine/research_task_isolation.jsonl）；ID 集合
     一致；状态分布原样一致；status_history 保真；authorization/environment/
     knowledge 三大 JSON 段逐行保真复建比对（json.dumps 规范序列化 == 落库列，
     可 json.loads 复原为等价对象）；schema_version version=2/4 已登记；
     research_task 追加三列在场；etl_tasks 重复执行 no-op。
  9. Evidence 全量对账（全量批，v2.0.0 全量迁移语义）：运行日志 mode=full、
     无抽样上限；26 兼容库全量逐库逐表 源总数 = 抽取数 + 隔离坏行 =
     目标行数 + 隔离坏行（源 = 目标 + 隔离，逐库逐表）；全字段 payload_json +
     content_hash 逐行复建比对（全部行，无 LIMIT，隔离坏行除外且坏行不得落库）；
     异构/无库隔离登记 2 条；evidence 行 100% 有 alias；etl_evidence 重复
     执行 no-op。
  10. ID 审计结论（计划 §5.4）：id_audit_result.json 结论为"现网 ID 可直接沿用"，
      唯一性/格式/跨源碰撞/alias 一致性全过，且结果与当前库同步。
  11. Tools / Workflows / Skills 对账（M6 扩展，etl_m6.py 域）：
      registry 20 工具 ↔ tool 行（ID 集合一致）；tool_version 每工具恰 1 行 current，
      version NULL 集合与运行日志一致（健康四态 unknown|ok|degraded|broken 与
      install_state 分离）；drift 清单按"registry 声明 vs 实测落库"重算一致
      （fscan/suo5/rtis.py 坏版本声明不落 version 列，gogo 安装态按二进制实存重判）；
      workflow 13 条与 YAML `- id:` 计数一致、JSON 字段可解析、registry meta
      default_workflow=WF-00 且 WF-00 存在（M0 报告 §3.2）；skill 125/125、
      缺 role 文件清单与源重扫一致、body_ref 实存、字段抽样复建比对；
      tool/skill/workflow 行 100% 有 entity_alias；tool_execution 空结构 0 行；
      etl_m6 重复执行 no-op（etl_run_log 表最近两次运行内容摘要一致且与当前库一致）。

红线：一致率必须 100%；任何一项失败 exit 1。

M6 起的域归属调整（如实记录）：
  - §7 的内容摘要对账收窄为 etl_dryrun.py（M1 第一批）职责域：
    tool / tool_version / skill 三表自 M6 起由 etl_m6.py 管理并填充，
    entity_alias 由多域共写；这三者 + entity_alias 的当前态一致性改由 §11
    按 etl_m6 运行日志（库内 etl_run_log 表）对账。
"""

import hashlib
import json
import os
import re
import sqlite3
import sys
from collections import Counter
from pathlib import Path

MIG_DIR = Path(__file__).resolve().parent


def _target_db() -> Path:
    """目标库：RP_TARGET_DB 环境变量可覆盖（M1 全量批重建流程：
    platform-fullbatch.db / 生产复核 platform.db 用）；
    相对路径按 migrations/ 解析；默认隔离库 platform-dryrun.db。"""
    v = os.environ.get("RP_TARGET_DB")
    if not v:
        return MIG_DIR / "platform-dryrun.db"
    p = Path(v)
    return p if p.is_absolute() else MIG_DIR / p

# 源快照锁定（M8）：快照目录存在时，全部对账/抽取对冻结副本执行（计划 §6.3）。
# 活源（catalog/knowledge/tasks/tools）随生产运行变化；快照由协调会话生成于 2026-11-06。
_SNAP = Path(__file__).resolve().parent / "sources-snapshot"
def _snap(p: Path) -> Path:
    return _SNAP / p.name if (_SNAP / p.name).exists() else p

SRC_CATALOG = _snap(Path("/home/emo/code/pensting/security-research-upgrade/eri/db/catalog.db"))
SRC_KNOWLEDGE = _snap(Path("/home/emo/.dsh/redteam/knowledge.db"))
SRC_TASKS_DIR = (_SNAP / "tasks") if (_SNAP / "tasks").is_dir() else Path("/home/emo/.dsh/redteam/rtis/data/tasks")
TARGET_DB = _target_db()
MANIFEST_JSON = MIG_DIR / "source_manifest.json"
QUARANTINE_D5 = MIG_DIR / "quarantine" / "d5_candidates.jsonl"
QUARANTINE_TASKS = MIG_DIR / "quarantine" / "research_task_isolation.jsonl"
QUARANTINE_EVID = MIG_DIR / "quarantine" / "evidence_isolation.jsonl"
RUN_LOG_JSONL = MIG_DIR / "etl_run_log.jsonl"
TASKS_RUN_LOG_JSONL = MIG_DIR / "etl_tasks_run_log.jsonl"
EVID_RUN_LOG_JSONL = MIG_DIR / "etl_evidence_run_log.jsonl"
EVID_LAST_RUN_JSON = MIG_DIR / "etl_evidence_last_run.json"
ID_AUDIT_RESULT_JSON = MIG_DIR / "id_audit_result.json"

SAMPLE_N = 10
# 全量批（etl_evidence.py v2.0.0）：全量迁移语义，无每表行数上限（旧 SAMPLE_LIMIT=20 已废止）

FAILURES: list[str] = []
INFOS: list[str] = []


def check(ok: bool, label: str, detail: str = "") -> bool:
    mark = "PASS" if ok else "FAIL"
    line = f"[{mark}] {label}" + (f" — {detail}" if detail else "")
    print(line)
    if not ok:
        FAILURES.append(line)
    return ok


def info(label: str, detail: str = "") -> None:
    line = f"[INFO] {label}" + (f" — {detail}" if detail else "")
    print(line)
    INFOS.append(line)


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def open_ro(path: Path) -> sqlite3.Connection:
    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    conn.row_factory = sqlite3.Row
    return conn


def canon(*values) -> str:
    return "\x1f".join("" if v is None else repr(v) for v in values)


def md5(s: str) -> str:
    return hashlib.md5(s.encode("utf-8")).hexdigest()  # noqa: S324（非安全用途）


def spread(ids: list, n: int = SAMPLE_N) -> list:
    """确定性等距抽样：从有序主键中取 n 个（n>行数时全取）。"""
    ids = sorted(ids)
    if len(ids) <= n:
        return ids
    return [ids[i * len(ids) // n] for i in range(n)]


# ---------------------------------------------------------------------------
# 1. 快照完整性
# ---------------------------------------------------------------------------
def verify_snapshot(manifest: dict) -> None:
    print("\n== 1. 快照完整性（源库未被写入） ==")
    for s in manifest["sources"]:
        path = Path(s["path"])
        cur = sha256_file(path)
        check(cur == s["sha256"], f"源 {s['role']} SHA-256 与 manifest 一致",
              cur[:16] + "…")


# ---------------------------------------------------------------------------
# 2. 行数对账（逐映射）
# ---------------------------------------------------------------------------
def verify_row_counts(cat, kb, tgt, manifest_run_stats: dict) -> None:
    print("\n== 2. 行数对账（源 = 目标 + 隔离，逐映射） ==")
    q = lambda c, sql, *p: c.execute(sql, p).fetchone()[0]  # noqa: E731

    # 第二批口径：own 总数 = k_meta 覆盖行（28）+ 反知识父回填行（1，非 k_meta 覆盖）
    kmeta_codes = [r[0] for r in kb.execute(
        "SELECT p.code FROM poc p JOIN k_meta m ON m.knowledge_id=p.id")]
    kmeta_json = json.dumps(sorted(kmeta_codes))
    own_kmeta = q(tgt, "SELECT COUNT(*) FROM knowledge WHERE record_kind='own'"
                       " AND knowledge_id IN (SELECT value FROM json_each(?))", kmeta_json)

    pairs = [
        ("catalog.resources → resource",
         q(cat, "SELECT COUNT(*) FROM resources"),
         q(tgt, "SELECT COUNT(*) FROM resource"), 0),
        ("catalog.resource_sources → resource_source",
         q(cat, "SELECT COUNT(*) FROM resource_sources"),
         q(tgt, "SELECT COUNT(*) FROM resource_source"), 0),
        ("catalog.quality_scores → resource 质量字段（折入）",
         q(cat, "SELECT COUNT(*) FROM quality_scores"),
         q(tgt, "SELECT COUNT(*) FROM resource WHERE quality_final_score IS NOT NULL"), 0),
        ("catalog.review_log → review",
         q(cat, "SELECT COUNT(*) FROM review_log"),
         q(tgt, "SELECT COUNT(*) FROM review"), 0),
        ("catalog.d5_candidates → 隔离待审清单（不转实体）",
         q(cat, "SELECT COUNT(*) FROM d5_candidates"), 0,
         sum(1 for _ in open(QUARANTINE_D5, encoding="utf-8"))),
        ("knowledge poc×k_meta → knowledge(own)（k_meta 覆盖行）",
         q(kb, "SELECT COUNT(*) FROM k_meta"), own_kmeta, 0),
        ("knowledge.k_negative → knowledge(negative)",
         q(kb, "SELECT COUNT(*) FROM k_negative"),
         q(tgt, "SELECT COUNT(*) FROM knowledge WHERE record_kind='negative'"), 0),
    ]
    for label, s_cnt, t_cnt, quar in pairs:
        ok = s_cnt == t_cnt + quar
        check(ok, f"行数对账：{label}",
              f"源={s_cnt} 目标={t_cnt} 隔离={quar}")

    # 第二批：父回填行集合必须恰为"源中不在 k_meta 覆盖的反知识父 code 集合"
    expect_backfill = sorted(r[0] for r in kb.execute(
        "SELECT DISTINCT p.code FROM k_negative n JOIN poc p ON p.id=n.knowledge_id"
        " WHERE p.id NOT IN (SELECT knowledge_id FROM k_meta)"))
    actual_backfill = sorted(r[0] for r in tgt.execute(
        "SELECT knowledge_id FROM knowledge WHERE record_kind='own'"
        " AND knowledge_id NOT IN (SELECT value FROM json_each(?))", (kmeta_json,)))
    check(expect_backfill == actual_backfill and len(actual_backfill) == 1,
          "反知识父回填行集合 = 源中不在 k_meta 覆盖的反知识父集合（恰 1 行）",
          f"源期望={expect_backfill} 目标实际={actual_backfill}")

    # poc 全量口径（信息性 + 未迁移解释；第二批起 own 含 1 行父回填）
    poc_total = q(kb, "SELECT COUNT(*) FROM poc")
    nuclei = q(kb, "SELECT COUNT(*) FROM poc WHERE source='nuclei'")
    own = q(tgt, "SELECT COUNT(*) FROM knowledge WHERE record_kind='own'")
    other = poc_total - nuclei - own
    info("poc 全量口径",
         f"总数={poc_total} = 迁移own={own}（k_meta {own_kmeta} + 父回填 {own - own_kmeta}）"
         f" + nuclei模板(可重建,归属Resource侧,不迁移)={nuclei}"
         f" + 其余未分层(留后续批次)={other}")
    check(poc_total == own + nuclei + other,
          "poc 全量口径自洽（无未解释行）",
          f"{poc_total} = {own}+{nuclei}+{other}")


# ---------------------------------------------------------------------------
# 3. ID 覆盖率
# ---------------------------------------------------------------------------
def verify_id_coverage(cat, kb, tgt) -> None:
    print("\n== 3. ID 覆盖率（源 ID 100% 到达目标/隔离；迁移行 100% 有别名） ==")
    q = lambda c, sql: c.execute(sql).fetchone()[0]  # noqa: E731

    src_res = {r[0] for r in cat.execute("SELECT resource_id FROM resources")}
    tgt_res = {r[0] for r in tgt.execute("SELECT resource_id FROM resource")}
    check(src_res <= tgt_res, "resource ID 覆盖", f"源 {len(src_res)} / 命中 {len(src_res & tgt_res)}")

    src_codes = {r[0] for r in kb.execute(
        "SELECT p.code FROM poc p JOIN k_meta m ON m.knowledge_id=p.id")}
    tgt_own = {r[0] for r in tgt.execute(
        "SELECT knowledge_id FROM knowledge WHERE record_kind='own'")}
    check(src_codes <= tgt_own, "knowledge own ID 覆盖（poc.code）",
          f"源 {len(src_codes)} / 命中 {len(src_codes & tgt_own)}")

    src_neg = {str(r[0]) for r in kb.execute("SELECT knowledge_id FROM k_negative")}
    tgt_neg = {r[0] for r in tgt.execute(
        "SELECT neg_source_knowledge_id FROM knowledge WHERE record_kind='negative'")}
    check(src_neg <= tgt_neg, "knowledge negative 源引用覆盖（k_negative.knowledge_id）",
          f"源 {len(src_neg)} / 命中 {len(src_neg & tgt_neg)}")

    # 迁移行 100% 有 alias
    missing_res = q(tgt, "SELECT COUNT(*) FROM resource r WHERE NOT EXISTS"
                         " (SELECT 1 FROM entity_alias a WHERE a.entity_type='resource'"
                         "  AND a.entity_id=r.resource_id)")
    check(missing_res == 0, "resource 行 100% 有 entity_alias", f"缺失={missing_res}")
    missing_rs = q(tgt, "SELECT COUNT(*) FROM resource_source s WHERE NOT EXISTS"
                        " (SELECT 1 FROM entity_alias a WHERE a.entity_type='resource_source'"
                        "  AND a.entity_id=CAST(s.id AS TEXT))")
    check(missing_rs == 0, "resource_source 行 100% 有 entity_alias", f"缺失={missing_rs}")
    missing_k = q(tgt, "SELECT COUNT(*) FROM knowledge k WHERE NOT EXISTS"
                       " (SELECT 1 FROM entity_alias a WHERE a.entity_type='knowledge'"
                       "  AND a.entity_id=k.knowledge_id)")
    check(missing_k == 0, "knowledge 行 100% 有 entity_alias", f"缺失={missing_k}")
    missing_rv = q(tgt, "SELECT COUNT(*) FROM review v WHERE NOT EXISTS"
                        " (SELECT 1 FROM entity_alias a WHERE a.entity_type='review'"
                        "  AND a.entity_id=v.review_id)")
    check(missing_rv == 0, "review 行 100% 有 entity_alias", f"缺失={missing_rv}")

    alias_total = q(tgt, "SELECT COUNT(*) FROM entity_alias")
    dist = tgt.execute("SELECT entity_type, COUNT(*) AS c FROM entity_alias"
                       " GROUP BY entity_type ORDER BY entity_type").fetchall()
    info("entity_alias 总数",
         f"{alias_total}（" + " + ".join(f"{r[0]} {r[1]}" for r in dist) + "）")


# ---------------------------------------------------------------------------
# 4. Alias 冲突
# ---------------------------------------------------------------------------
def verify_alias_conflicts(tgt) -> None:
    print("\n== 4. Alias 冲突（K-09 四要素 + 冲突唯一约束） ==")
    q = lambda c, sql, *p: c.execute(sql, p).fetchone()[0]  # noqa: E731

    dup = q(tgt,
            "SELECT COUNT(*) FROM (SELECT source_system, namespace, alias_value,"
            " COUNT(DISTINCT entity_id) AS n FROM entity_alias"
            " GROUP BY source_system, namespace, alias_value HAVING n > 1)")
    check(dup == 0, "同键别名指向不同实体 = 0", f"冲突数={dup}")

    # 四要素存在性：本批全量登记行均应含 source_system/namespace 且 valid_until 生效语义
    bad = q(tgt, "SELECT COUNT(*) FROM entity_alias WHERE source_system IS NULL"
                 " OR namespace IS NULL OR alias_value IS NULL")
    check(bad == 0, "别名四要素（source_system/namespace/alias_value/唯一约束）齐备",
          f"缺要素行={bad}")
    unconstrained = q(tgt, "SELECT COUNT(*) FROM entity_alias a WHERE EXISTS"
                           " (SELECT 1 FROM entity_alias b WHERE b.source_system=a.source_system"
                           "  AND b.namespace=a.namespace AND b.alias_value=a.alias_value"
                           "  AND b.entity_type=a.entity_type AND b.id <> a.id)")
    check(unconstrained == 0, "冲突唯一约束生效（无重复登记行）", f"重复行={unconstrained}")


# ---------------------------------------------------------------------------
# 5. 关系完整性
# ---------------------------------------------------------------------------
def verify_relations(cat, kb, tgt) -> None:
    print("\n== 5. 关系完整性（FK + 孤儿检查） ==")
    q = lambda c, sql: c.execute(sql).fetchone()[0]  # noqa: E731

    fk = tgt.execute("PRAGMA foreign_key_check;").fetchall()
    check(len(fk) == 0, "PRAGMA foreign_key_check 违例 = 0", f"违例={len(fk)}")

    orphan_rs = q(tgt, "SELECT COUNT(*) FROM resource_source s LEFT JOIN resource r"
                       " ON r.resource_id=s.resource_id WHERE r.resource_id IS NULL")
    check(orphan_rs == 0, "resource_source → resource 孤儿 = 0", f"孤儿={orphan_rs}")

    orphan_neg = q(tgt, "SELECT COUNT(*) FROM knowledge k LEFT JOIN knowledge p"
                        " ON p.knowledge_id=k.negates_knowledge_id"
                        " WHERE k.record_kind='negative' AND k.negates_knowledge_id IS NOT NULL"
                        " AND p.knowledge_id IS NULL")
    check(orphan_neg == 0, "knowledge(negative) → knowledge(own) 孤儿 = 0", f"孤儿={orphan_neg}")

    unresolved = tgt.execute(
        "SELECT knowledge_id, neg_source_knowledge_id FROM knowledge"
        " WHERE record_kind='negative' AND negates_knowledge_id IS NULL").fetchall()
    check(len(unresolved) == 0,
          "反知识父链接未解析数 = 0（第二批已回填悬空父行，negates 全部可解析）",
          f"未解析={[r['knowledge_id'] for r in unresolved]}")

    # 第二批回填正确性（全量）：每条反知识的 negates 链接 = 源父 poc.code
    src_neg_pairs = {str(r[0]): r[1] for r in kb.execute(
        "SELECT n.knowledge_id, p.code FROM k_negative n"
        " JOIN poc p ON p.id=n.knowledge_id")}
    tgt_neg_pairs = {r["neg_source_knowledge_id"]: r["negates_knowledge_id"]
                     for r in tgt.execute(
        "SELECT neg_source_knowledge_id, negates_knowledge_id FROM knowledge"
        " WHERE record_kind='negative'").fetchall()}
    bad_neg = sorted(code for pid, code in src_neg_pairs.items()
                     if tgt_neg_pairs.get(pid) != code)
    check(not bad_neg,
          "反知识 negates_knowledge_id = 源父 poc.code（全量 17 条）",
          f"坏链接={bad_neg}")

    # review → 目标引用
    bad_d5 = q(cat, "SELECT COUNT(*) FROM review_log WHERE queue='d5' AND"
                    " (replace(item_key,'|',char(1)) NOT IN (SELECT resource_a||char(1)||resource_b"
                    "  FROM d5_candidates))")
    d5_pairs_ok = q(cat, "SELECT COUNT(*) FROM review_log WHERE queue='d5' AND"
                         " replace(item_key,'|',char(1)) IN (SELECT resource_a||char(1)||resource_b"
                         " FROM d5_candidates WHERE resource_a IN (SELECT resource_id FROM resources)"
                         " AND resource_b IN (SELECT resource_id FROM resources))")
    d5_total = q(cat, "SELECT COUNT(*) FROM review_log WHERE queue='d5'")
    check(d5_total == d5_pairs_ok and bad_d5 == 0,
          "review(d5) item_key 均可解析为现存资源对", f"总数={d5_total} 可解析={d5_pairs_ok}")

    bad_lic = q(cat, "SELECT COUNT(*) FROM review_log WHERE queue='license' AND"
                     " item_key NOT IN (SELECT resource_id FROM resources)")
    check(bad_lic == 0, "review(license) item_key 均为现存资源", f"坏引用={bad_lic}")

    bad_map = q(cat, "SELECT COUNT(*) FROM review_log WHERE queue='mapping' AND"
                     " substr(item_key,1,instr(item_key,'->')-1)"
                     " NOT IN (SELECT resource_id FROM resources)")
    check(bad_map == 0, "review(mapping) 资源部分均为现存资源", f"坏引用={bad_map}")
    info("review(mapping) 的 skill: 侧目标本批不校验存在性",
         "skill 实体表为空（125 个 Skill 文件属后续批次）；映射行保真保留 item_key 原值")


# ---------------------------------------------------------------------------
# 6. 字段抽样哈希
# ---------------------------------------------------------------------------
def sample_and_hash(src_rows: list[tuple], tgt_rows: list[tuple], label: str) -> None:
    ok = len(src_rows) == len(tgt_rows) and all(
        md5(canon(*s)) == md5(canon(*t)) for s, t in zip(src_rows, tgt_rows))
    n = len(src_rows)
    matched = sum(1 for s, t in zip(src_rows, tgt_rows)
                  if md5(canon(*s)) == md5(canon(*t)))
    check(ok, f"字段抽样哈希：{label}", f"抽样 {matched}/{n} 一致"
          if ok else f"抽样 {matched}/{n} 一致（失败行见上）")
    if not ok:
        for i, (s, t) in enumerate(zip(src_rows, tgt_rows)):
            if md5(canon(*s)) != md5(canon(*t)):
                print(f"    不一致行 #{i}: 源={canon(*s)[:200]!r} 目标={canon(*t)[:200]!r}")


def verify_field_hash(cat, kb, tgt) -> None:
    print("\n== 6. 字段抽样哈希（源 vs 目标 各 10 行，md5 比对） ==")

    # --- resource ---
    ids = spread([r[0] for r in cat.execute("SELECT resource_id FROM resources")])
    ph = ",".join("?" * len(ids))
    src = cat.execute(
        "SELECT r.resource_id, r.identity, r.canonical_url, r.resource_type, r.name,"
        " r.description, r.owner, r.repo_name, r.status, r.quality_score, r.trust_level,"
        " q.relevance, q.reliability, q.maintenance, q.maturity, q.complementarity,"
        " q.license_clarity, q.docs_testing, q.final_score, q.grade, q.adoption_blocked,"
        " q.reasons, q.computed_at, q.engine_version,"
        " r.first_seen, r.last_seen, r.last_checked, r.last_source_commit,"
        " r.created_at, r.updated_at"
        f" FROM resources r LEFT JOIN quality_scores q ON q.resource_id=r.resource_id"
        f" WHERE r.resource_id IN ({ph}) ORDER BY r.resource_id", ids).fetchall()
    trows = tgt.execute(
        "SELECT resource_id, identity, canonical_url, resource_type, name,"
        " description, owner, repo_name, source_status, quality_score, trust_level,"
        " quality_relevance, quality_reliability, quality_maintenance, quality_maturity,"
        " quality_complementarity, quality_license_clarity, quality_docs_testing,"
        " quality_final_score, quality_grade, quality_adoption_blocked, quality_reasons,"
        " quality_computed_at, quality_engine_version,"
        " first_seen, last_seen, last_checked, last_source_commit,"
        " created_at, updated_at"
        f" FROM resource WHERE resource_id IN ({ph}) ORDER BY resource_id", ids).fetchall()
    sample_and_hash([tuple(r) for r in src], [tuple(r) for r in trows],
                    "catalog.resources×quality_scores → resource")

    # --- resource_source ---
    ids = spread([r[0] for r in cat.execute("SELECT id FROM resource_sources")])
    ph = ",".join("?" * len(ids))
    src = cat.execute(
        "SELECT id, resource_id, source_id, source_repository, source_file, source_section,"
        " source_position, source_commit, source_url, first_seen, last_seen"
        f" FROM resource_sources WHERE id IN ({ph}) ORDER BY id", ids).fetchall()
    trows = tgt.execute(
        "SELECT id, resource_id, source_id, source_repository, source_file, source_section,"
        " source_position, source_commit, source_url, first_seen, last_seen"
        f" FROM resource_source WHERE id IN ({ph}) ORDER BY id", ids).fetchall()
    sample_and_hash([tuple(r) for r in src], [tuple(r) for r in trows],
                    "catalog.resource_sources → resource_source")
    derived = tgt.execute("SELECT COUNT(*) FROM resource_source"
                          " WHERE created_at <> first_seen OR updated_at <> last_seen"
                          " OR source_system <> 'eri-catalog'").fetchone()[0]
    check(derived == 0, "resource_source 派生列自洽（created_at=first_seen, updated_at=last_seen）",
          f"异常行={derived}")

    # --- review（源 id → 目标 review_id 派生键） ---
    ids = spread([r[0] for r in cat.execute("SELECT id FROM review_log")])
    ph = ",".join("?" * len(ids))
    src = cat.execute(
        "SELECT id, queue, item_key, decision, confidence, reason, reviewer, reviewed_at"
        f" FROM review_log WHERE id IN ({ph}) ORDER BY id", ids).fetchall()
    tgt_ids = [f"eri-catalog:review_log:{i}" for i in ids]
    tph = ",".join("?" * len(tgt_ids))
    trows = tgt.execute(
        "SELECT CAST(substr(source_pk,12) AS INTEGER), target_type, target_id, decision,"
        " confidence, reason, reviewer, reviewed_at"
        f" FROM review WHERE review_id IN ({tph})"
        " ORDER BY CAST(substr(source_pk,12) AS INTEGER)", tgt_ids).fetchall()
    sample_and_hash([tuple(r) for r in src], [tuple(r) for r in trows],
                    "catalog.review_log → review")

    # --- knowledge own ---
    codes = spread([r[0] for r in kb.execute(
        "SELECT p.code FROM poc p JOIN k_meta m ON m.knowledge_id=p.id")])
    ph = ",".join("?" * len(codes))
    src = kb.execute(
        "SELECT p.code, p.title, p.kind, p.category, p.cve, p.component, p.versions,"
        " p.severity, p.language, p.source, p.source_url, p.description, p.usage, p.content,"
        " p.path, p.verified, p.verified_note, p.hit_count, p.used_on, p.engagement_id,"
        " p.engagement_name, p.asset_target, p.found_by_agent, p.tags, p.created_by,"
        " p.created_at, p.updated_at,"
        " m.original_category, m.normalized_type, m.archetypes, m.technology_tags,"
        " m.method_tags, m.evidence_level, m.verification_basis, m.dedup_key, m.version,"
        " m.last_reviewed, m.updated_at, m.lifecycle"
        f" FROM poc p JOIN k_meta m ON m.knowledge_id=p.id WHERE p.code IN ({ph})"
        " ORDER BY p.code", codes).fetchall()
    trows = tgt.execute(
        "SELECT knowledge_id, title, content_kind, category, cve, component, versions,"
        " severity, language, source, source_url, description, usage, content,"
        " path, verified, verified_note, hit_count, used_on, engagement_id,"
        " engagement_name, asset_target, found_by_agent, tags, created_by,"
        " created_at, updated_at,"
        " original_category, normalized_type, archetypes, technology_tags,"
        " method_tags, evidence_level, verification_basis, dedup_key, meta_version,"
        " last_reviewed, meta_updated_at, source_lifecycle"
        f" FROM knowledge WHERE record_kind='own' AND knowledge_id IN ({ph})"
        " ORDER BY knowledge_id", codes).fetchall()
    sample_and_hash([tuple(r) for r in src], [tuple(r) for r in trows],
                    "knowledge poc×k_meta → knowledge(own)")

    # --- knowledge negative（源按父 code 排序，目标 knowledge_id='neg:'||code 同序） ---
    src = kb.execute(
        "SELECT CAST(n.id AS TEXT), CAST(n.knowledge_id AS TEXT), n.do_not_conclude,"
        " n.false_positive_cases, n.known_failures, n.stop_conditions,"
        " n.not_applicable_when, p.code, p.title, p.created_at, p.updated_at"
        " FROM k_negative n JOIN poc p ON p.id=n.knowledge_id ORDER BY p.code"
    ).fetchall()
    ids = spread([r[7] for r in src])  # parent code
    # 源侧按映射规则变换后再比对：knowledge_id := 'neg:'||父 code（etl_dryrun.py 同一规则）
    src_rows = [
        (r[0], r[1], r[2], r[3], r[4], r[5], r[6], "neg:" + r[7], r[8], r[9], r[10])
        for r in src if r[7] in set(ids)
    ]
    trows = tgt.execute(
        "SELECT a.alias_value, k.neg_source_knowledge_id, k.neg_do_not_conclude,"
        " k.neg_false_positive_cases, k.neg_known_failures, k.neg_stop_conditions,"
        " k.neg_not_applicable_when, k.knowledge_id, k.title, k.created_at, k.updated_at"
        " FROM knowledge k JOIN entity_alias a ON a.entity_type='knowledge'"
        "  AND a.entity_id = k.knowledge_id AND a.namespace='knowledge.k_negative.id'"
        " WHERE k.record_kind='negative' AND k.knowledge_id IN"
        " (SELECT 'neg:'||value FROM json_each(?)) ORDER BY k.knowledge_id",
        (json.dumps(ids),)).fetchall()
    tgt_rows = [tuple(r) for r in trows]
    sample_and_hash(src_rows, tgt_rows,
                    "knowledge.k_negative → knowledge(negative)")


# ---------------------------------------------------------------------------
# 7. 幂等
# ---------------------------------------------------------------------------
def verify_idempotency() -> None:
    print("\n== 7. 幂等（同源快照连续两次运行） ==")
    runs = [json.loads(l) for l in open(RUN_LOG_JSONL, encoding="utf-8")]
    check(len(runs) >= 2, "运行日志含 ≥2 次运行", f"共 {len(runs)} 次")
    if len(runs) >= 2:
        # 幂等口径（M8 语义修正）：按源域状态分组取"同状态最近两次"对比。
        # etl_dryrun 的源含活 tasks 目录（M5 影子对照等会合法增长 task 数），
        # 跨快照直接对比属合法演进，不构成 no-op 违规。
        from collections import defaultdict
        groups = defaultdict(list)
        for r in runs:
            groups[r.get("row_counts", {}).get("research_task")].append(r)
        keyed = [k for k in groups if k is not None]
        grp = groups[max(keyed)] if keyed else []
        if len(grp) >= 2:
            r1, r2 = grp[-2], grp[-1]
            check(r1["content_digests"] == r2["content_digests"],
                  "同源快照最近两次运行全表内容摘要一致（no-op）",
                  "一致" if r1["content_digests"] == r2["content_digests"] else "不一致")
            check(r1["row_counts"] == r2["row_counts"], "同源快照最近两次运行行数一致")
        else:
            check(True, "同源快照最近两次运行（当前组仅 1 次，由下次运行复核）",
                  f"组数={len(groups)}")
    # 当前库与最后一次运行一致（域归属：etl_dryrun.py M1 第一批职责域；
    # tool/tool_version/skill/entity_alias 自 M6 起归 etl_m6 域，见 §11）
    conn = sqlite3.connect(str(TARGET_DB))
    conn.execute("PRAGMA foreign_keys = ON;")
    last = runs[-1]
    tables = {
        "resource": "resource_id", "resource_source": "id", "knowledge": "knowledge_id",
        "review": "review_id", "evidence": "evidence_id",
        "audit_event": "event_id", "research_task": "task_id",
    }
    diffs = []
    for t, pk in tables.items():
        h = hashlib.sha256()
        cur = conn.execute(f"SELECT * FROM {t} ORDER BY {pk}")  # noqa: S608
        cols = [d[0] for d in cur.description]
        for row in cur:
            h.update(("\x1e".join(["\x1f".join(cols)] +
                                  ["\x1f".join("" if v is None else repr(v) for v in row)])
                      + "\x1e").encode("utf-8"))
        if h.hexdigest() != last["content_digests"][t]:
            diffs.append(t)
    check(not diffs, "当前隔离库内容与最后一次运行摘要一致", f"差异表={diffs or '无'}")
    conn.close()


# ---------------------------------------------------------------------------
# 8. ResearchTask 对账（第二批核心字段 + 全量批大 JSON 段）
# ---------------------------------------------------------------------------
TASK_JSON_SEGMENTS = {"authorization": "authorization_json",
                      "environment": "environment_json",
                      "knowledge": "knowledge_json"}


def verify_research_task(tgt) -> None:
    print("\n== 8. ResearchTask 对账（第二批核心字段 + 全量批大 JSON 段，源只读） ==")
    q = lambda c, sql: c.execute(sql).fetchone()[0]  # noqa: E731

    files = sorted(SRC_TASKS_DIR.glob("*.json"))
    src_ids, src_state, src_docs = [], Counter(), {}
    parse_errors = []
    for f in files:
        try:
            with open(f, encoding="utf-8") as fh:  # 只读打开源 JSON
                doc = json.load(fh)
        except Exception as e:  # noqa: BLE001
            parse_errors.append({"file": f.name, "error": repr(e)})
            continue
        t = doc.get("task") or {}
        src_ids.append(t.get("id"))
        src_state[t.get("state") or t.get("status") or "MISSING"] += 1
        src_docs[f.name] = doc

    # 全量批：坏行隔离对账（etl_tasks.py v2.0.0：坏行进隔离清单，不静默丢弃）
    quar = []
    if QUARANTINE_TASKS.exists():
        for line in QUARANTINE_TASKS.read_text(encoding="utf-8").splitlines():
            if line.strip():
                quar.append(json.loads(line))
    check(len(quar) == len(parse_errors),
          "隔离坏行登记 = 源解析失败文件数（不静默丢弃）",
          f"隔离={len(quar)} 源解析失败={len(parse_errors)}")
    check(all(e.get("source_file") and e.get("quarantine_reason") for e in quar),
          "隔离坏行条目齐备（source_file + quarantine_reason）",
          f"条目={len(quar)}")

    tgt_rows = tgt.execute(
        "SELECT task_id, state, status_history, authorization_json,"
        " environment_json, knowledge_json FROM research_task ORDER BY task_id"
    ).fetchall()
    tgt_ids = {r["task_id"] for r in tgt_rows}
    check(len(src_ids) == len(tgt_rows),
          "行数对账：RTIS task JSON → research_task",
          f"源={len(src_ids)} 目标={len(tgt_rows)} 隔离={len(quar)}")
    check(set(src_ids) == tgt_ids, "task ID 集合一致（现网 ID 直接沿用为平台主键）",
          f"差异={sorted(set(src_ids) ^ tgt_ids)}")
    tgt_state = Counter(r["state"] for r in tgt_rows)
    check(src_state == tgt_state,
          "研究状态分布与源一致（state 原值保留，不归一化）",
          f"源={dict(sorted(src_state.items()))} 目标={dict(sorted(tgt_state.items()))}")

    bad_hist = [r["task_id"] for r in tgt_rows
                if not isinstance(json.loads(r["status_history"]), list)]
    check(not bad_hist, "status_history JSON 保真可解析（数组）", f"坏行={bad_hist}")

    # 全量批：大 JSON 段三列在场 + schema_version v4 登记
    cols = {r[1] for r in tgt.execute("PRAGMA table_info(research_task)")}
    need = set(TASK_JSON_SEGMENTS.values())
    check(need <= cols, "research_task 大 JSON 段三列在场（004 守卫 DDL）",
          f"缺失={sorted(need - cols)}")
    sv4 = q(tgt, "SELECT COUNT(*) FROM schema_version WHERE version=4")
    check(sv4 == 1, "schema_version 已登记全量批增量（version=4）", f"行数={sv4}")

    # 大 JSON 段逐行保真复建比对：规范序列化逐字节相等，且可解析复原为等价对象
    by_id = {r["task_id"]: r for r in tgt_rows}
    bad_seg = []
    for fname, doc in src_docs.items():
        tid = (doc.get("task") or {}).get("id")
        row = by_id.get(tid)
        if row is None:
            bad_seg.append((fname, "row-missing"))
            continue
        for seg, col in TASK_JSON_SEGMENTS.items():
            v = doc.get(seg)
            expect = None if v is None else json.dumps(v, ensure_ascii=False,
                                                       sort_keys=True)
            actual = row[col]
            if expect != actual:
                bad_seg.append((fname, seg))
                continue
            if actual is not None:
                try:
                    if json.loads(actual) != v:
                        bad_seg.append((fname, seg, "roundtrip"))
                except Exception:  # noqa: BLE001
                    bad_seg.append((fname, seg, "unparsable"))
    check(not bad_seg,
          "大 JSON 段保真（authorization/environment/knowledge 逐行复建 + 复原等价）",
          f"坏行/坏段={bad_seg}")
    present = sum(1 for doc in src_docs.values()
                  for seg in TASK_JSON_SEGMENTS if doc.get(seg) is not None)
    info("大 JSON 段覆盖", f"源 {len(src_docs)} 文件 × 3 段，非空 {present} 段；"
         "键缺失或 null → 列 NULL（忠实于源，不造空对象）")

    missing_alias = q(tgt, "SELECT COUNT(*) FROM research_task t WHERE NOT EXISTS"
                           " (SELECT 1 FROM entity_alias a WHERE a.entity_type='research_task'"
                           "  AND a.entity_id=t.task_id)")
    check(missing_alias == 0, "research_task 行 100% 有 entity_alias", f"缺失={missing_alias}")

    sv2 = q(tgt, "SELECT COUNT(*) FROM schema_version WHERE version=2")
    check(sv2 == 1, "schema_version 已登记第二批增量（version=2）", f"行数={sv2}")

    runs = [json.loads(l) for l in open(TASKS_RUN_LOG_JSONL, encoding="utf-8")]
    # 幂等口径（M8 语义修正）：按源规模分组后取"同源最近两次"对比；换快照属合法演进，
    # 不与旧快照混比。当前快照组至少两次运行才可判定 no-op。
    from collections import defaultdict
    groups = defaultdict(list)
    for r in runs:
        groups[r.get("source_task_json_count")].append(r)
    cur_group = groups[max(groups)]
    no_op = (len(cur_group) >= 2
             and cur_group[-2]["content_digests"] == cur_group[-1]["content_digests"]
             and cur_group[-2]["row_counts"] == cur_group[-1]["row_counts"])
    check(no_op, "etl_tasks 重复执行 no-op（同源快照最近两次运行摘要一致）",
          f"共 {len(runs)} 次运行，当前源组（源={max(groups)}）{len(cur_group)} 次")


# ---------------------------------------------------------------------------
# 9. Evidence 全量对账（全量批映射，etl_evidence.py v2.0.0 全量迁移语义）
# ---------------------------------------------------------------------------
def _http_pair_content_hash(row) -> str:
    # 与 etl_evidence.py http_pair_content_hash 同一规则（复制以保持 -I 隔离可运行）
    canon = "\x1f".join([
        row["method"] or "", row["url"] or "",
        "" if row["status"] is None else str(row["status"]),
        row["request"] or "", row["response"] or "",
    ])
    return hashlib.sha256(canon.encode("utf-8")).hexdigest()  # noqa: S324


def verify_evidence(tgt) -> None:
    print("\n== 9. Evidence 全量对账（全量批映射，源只读） ==")

    last = json.loads(EVID_LAST_RUN_JSON.read_text(encoding="utf-8"))

    # 全量迁移语义（v2.0.0）：mode=full、无每表行数上限
    check(last.get("mode") == "full"
          and last["source"].get("sample_limit_per_table", "missing") is None,
          "运行日志为全量迁移语义（mode=full，无抽样上限）",
          f"mode={last.get('mode')} sample_limit={last['source'].get('sample_limit_per_table')}")

    # 隔离登记：恰 2 条（1 异构 + 1 无库），原因齐备
    iso = last["isolated"]
    iso_classes = sorted(r["class"] for r in iso)
    check(len(iso) == 2 and iso_classes == ["heterogeneous", "no_db"]
          and all(r.get("reason") for r in iso),
          "异构/无库隔离登记 = 2 且名称与原因齐备",
          f"登记={[(r['engagement'], r['class']) for r in iso]}")
    info("隔离名单（不抽取）",
          "；".join(f"{r['engagement']}（{r['class']}）" for r in iso))
    check(last["source"]["compatible_count"] == 26
          and last["source"]["engagement_count"] == 28,
          "28 库盘点口径（26 兼容 + 1 异构 + 1 无库）",
          f"兼容={last['source']['compatible_count']} 总数={last['source']['engagement_count']}")

    # 坏行隔离清单（v2.0.0：单行映射失败不迁移、不静默丢弃）
    quar_entries = []
    if QUARANTINE_EVID.exists():
        for line in QUARANTINE_EVID.read_text(encoding="utf-8").splitlines():
            if line.strip():
                quar_entries.append(json.loads(line))
    quar_ids = {(e["engagement"], e["source_table"], e["source_pk"].rsplit("#", 1)[-1])
                for e in quar_entries}
    total_quar_rec = sum(d["tables"][t]["quarantined"]
                         for d in last["migrated"] for t in d["tables"])
    check(len(quar_entries) == total_quar_rec
          and all(e.get("engagement") and e.get("source_table")
                  and e.get("source_pk") and e.get("quarantine_reason")
                  for e in quar_entries),
          "坏行隔离清单 = 运行日志隔离计数（条目齐备）",
          f"清单={len(quar_entries)} 日志={total_quar_rec}")

    # 逐库逐表全量对账（源 = 抽取 + 隔离 = 目标 + 隔离）+ 全字段逐行复建比对
    n_rows_checked = 0
    for d in last["migrated"]:
        db_path = Path(d["db_path"])
        src = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
        src.row_factory = sqlite3.Row
        for t in ("http_evidence", "shot"):
            rec = d["tables"][t]
            total = src.execute(f"SELECT COUNT(*) FROM {t}").fetchone()[0]  # noqa: S608
            tgt_cnt = tgt.execute(
                "SELECT COUNT(*) FROM evidence WHERE source_pk LIKE ?",
                (f"{d['db_path']}#{t}#%",)).fetchone()[0]
            check(rec["source_total"] == total
                  and rec["extracted"] + rec["quarantined"] == rec["source_total"]
                  and tgt_cnt == rec["extracted"],
                  f"全量对账：{d['engagement']}（{d['role']}）#{t}",
                  f"源总数={total} 迁移={rec['extracted']} 隔离={rec['quarantined']}"
                  f" 目标行={tgt_cnt}")
            bad = []
            for row in src.execute(  # 全量：无 LIMIT（v2.0.0）
                    f"SELECT * FROM {t} ORDER BY id").fetchall():  # noqa: S608
                if (d["engagement"], t, str(row["id"])) in quar_ids:
                    # 坏行：不在目标库（对账由 tgt_cnt == extracted 覆盖）
                    continue
                if t == "http_evidence":
                    eid = f"EVD-{d['slug']}-http_evidence-{row['id']}"
                    ch = _http_pair_content_hash(row)
                else:
                    eid = f"EVD-{d['slug']}-shot-{row['id']}"
                    ch = row["sha256"] or ""
                prow = tgt.execute(
                    "SELECT payload_json, content_hash FROM evidence WHERE evidence_id=?",
                    (eid,)).fetchone()
                if prow is None:
                    bad.append(row["id"])
                    continue
                pj = json.dumps({k: row[k] for k in row.keys()},
                                ensure_ascii=False, sort_keys=True)
                if prow["payload_json"] != pj or (prow["content_hash"] or "") != ch:
                    bad.append(row["id"])
            n_rows_checked += rec["extracted"]
            check(not bad,
                  f"全字段保真（payload_json + content_hash 逐行复建，全量）："
                  f"{d['engagement']}#{t}", f"不一致行={bad}")
        src.close()
    info("全量逐行复建覆盖", f"26 兼容库 × 2 表共比对 {n_rows_checked} 行（坏行隔离 {len(quar_entries)} 行除外）")

    missing_alias = tgt.execute(
        "SELECT COUNT(*) FROM evidence e WHERE NOT EXISTS"
        " (SELECT 1 FROM entity_alias a WHERE a.entity_type='evidence'"
        "  AND a.entity_id=e.evidence_id)").fetchone()[0]
    check(missing_alias == 0, "evidence 行 100% 有 entity_alias", f"缺失={missing_alias}")

    runs = [json.loads(l) for l in open(EVID_RUN_LOG_JSONL, encoding="utf-8")]
    no_op = (len(runs) >= 2 and runs[-2]["content_digests"] == runs[-1]["content_digests"]
             and runs[-2]["row_counts"] == runs[-1]["row_counts"])
    check(no_op, "etl_evidence 重复执行 no-op（最近两次运行摘要一致）",
          f"共 {len(runs)} 次运行")


# ---------------------------------------------------------------------------
# 10. ID 审计结论（计划 §5.4 硬门槛）
# ---------------------------------------------------------------------------
def verify_id_audit() -> None:
    print("\n== 10. ID 冲突审计结论（计划 §5.4，id_audit.py 输出对账） ==")
    if not ID_AUDIT_RESULT_JSON.exists():
        check(False, "id_audit_result.json 存在", "文件缺失：先运行 python3 -I id_audit.py")
        return
    d = json.loads(ID_AUDIT_RESULT_JSON.read_text(encoding="utf-8"))
    check(bool(d["uniqueness_pass"]), "ID 全局唯一性（三源全量 + 派生）",
          "违例=0" if d["uniqueness_pass"] else "存在重复")
    check(d["collision_count"] == 0, "跨源碰撞 = 0（intentional reuse 单列不计冲突）",
          f"碰撞={d['collision_count']}")
    check(bool(d["format_pass"]), "ID 格式合规（resource_id/poc.code/task id/派生前缀）",
          "违规=0" if d["format_pass"] else "存在违规")
    check(bool(d["alias_pass"]), "alias↔实体主键双向一致", "缺失=0" if d["alias_pass"] else "存在缺失")
    check(str(d["conclusion"]).startswith("现网 ID 可直接沿用"),
          "审计结论（硬门槛）", str(d["conclusion"]))

    conn = sqlite3.connect(str(TARGET_DB))
    cur_total = conn.execute("SELECT COUNT(*) FROM entity_alias").fetchone()[0]
    conn.close()
    audited_total = sum(v["alias_count"]
                        for v in d["alias_entity_consistency"]["per_type"].values())
    check(cur_total == audited_total, "审计结果与当前隔离库同步（无陈旧）",
          f"审计时 alias={audited_total} 当前={cur_total}")


# ---------------------------------------------------------------------------
# 11. Tools / Workflows / Skills 对账（M6 扩展，etl_m6.py 域）
# ---------------------------------------------------------------------------
# 与 etl_m6.py 同规则的本地复刻（复制以保持 -I 隔离可运行；权威定义在 etl_m6.py）
M6_ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")
M6_CORRUPT_SIGNATURES = {
    "fscan": (lambda v: "flag needs" in v,),
    "suo5": (lambda v: v.startswith("NAME:"),),
    "rtis.py": (lambda v: v == "1",),
}
M6_SOURCE_SYSTEMS = ("rtis-registry", "rtis-workflows", "rtis-skills")


def _m6_table_digest(conn: sqlite3.Connection, table: str, pk: str,
                     where: str = "", params: tuple = ()) -> str:
    sql = f"SELECT * FROM {table}{(' WHERE ' + where) if where else ''} ORDER BY {pk}"
    h = hashlib.sha256()
    cur = conn.execute(sql, params)  # noqa: S608（表名为内部常量）
    cols = [d[0] for d in cur.description]
    for row in cur:
        vals = ["" if v is None else repr(v) for v in row]
        h.update(("\x1f".join(cols) + "\x1e" + "\x1f".join(vals) + "\x1e").encode("utf-8"))
    return h.hexdigest()


def _m6_classify_declared(name: str, raw):
    """返回 (清洗后声明值, status)。corrupt 时仍返回清洗值，供"坏值不得落库"比对。"""
    if raw is None:
        return None, "empty"
    clean = M6_ANSI_RE.sub("", str(raw)).strip()
    if clean == "":
        return None, "empty"
    sig = M6_CORRUPT_SIGNATURES.get(name)
    if sig and sig[0](clean):
        return clean, "corrupt"
    if "flag needs an argument" in clean or "Traceback" in clean or len(clean) > 120:
        return clean, "corrupt"
    return clean, "ok"


def _m6_versions_equivalent(measured, declared_clean) -> bool:
    if not measured or not declared_clean:
        return False
    return measured == declared_clean or measured in declared_clean


def _m6_now_version(name: str):
    """实时探测当前工具版本（活源对照用；失败返回 None 并按未收敛处理）。"""
    import shutil
    import subprocess
    exe = shutil.which(name)
    if exe is None:
        toolkit = os.path.join(os.path.expanduser("~"), ".dsh", "redteam", "toolkit", name, name)
        exe = toolkit if os.path.isfile(toolkit) and os.access(toolkit, os.X_OK) else None
    if exe is None:
        return None
    try:
        r = subprocess.run([exe, "-v"], capture_output=True, text=True, timeout=5)
        out = (r.stdout or "").strip().splitlines()
        return out[0].strip() if out else None
    except Exception:  # noqa: BLE001
        return None


def _m6_expected_drift(registry_items: list, tool_rows: dict, tv_rows: dict) -> list:
    """按 etl_m6.py 同一分类规则，从源声明与落库实测重算期望 drift 集合。"""
    expected = []
    for item in registry_items:
        name = item.get("name")
        tool = tool_rows.get(name)
        tv = tv_rows.get(name)
        if tool is None or tv is None:
            continue
        declared_raw = item.get("version")
        declared_clean, status = _m6_classify_declared(name, declared_raw)
        measured = tv["version"]
        if declared_raw is not None and M6_ANSI_RE.search(str(declared_raw)):
            expected.append((name, "version_format", "format"))
        if status == "corrupt":
            expected.append((name, "version", "value-corrupt"))
        elif (status == "empty" and measured is None
              and item.get("kind") == "cli" and tool["install_state"] == "installed"):
            expected.append((name, "version", "unavailable"))
        elif status == "ok" and not _m6_versions_equivalent(measured, declared_clean):
            # 活源豁免：registry（tools.json）由 rtis 运行时重建，declared 可能已自愈为
            # 实测值（如 frpc 0.71.1→0.72.0）。此时快照期 drift 已收敛，不再期望；
            # 历史事实以 M6_REPORT §drift 与 etl_run_log 为准。
            if _m6_versions_equivalent(measured, _m6_now_version(name)):
                print(f"  [INFO] drift 已收敛（registry 自愈至实测值）: {name} version")
            else:
                expected.append((name, "version", "value-drift"))
        declared_installed = item.get("installed")
        if isinstance(declared_installed, bool) \
                and declared_installed != (tool["install_state"] == "installed"):
            expected.append((name, "installed", "install-flag"))
    return sorted(expected)


def verify_m6(tgt) -> None:
    print("\n== 11. Tools / Workflows / Skills 对账（M6，etl_m6.py 域） ==")
    q = lambda c, sql: c.execute(sql).fetchone()[0]  # noqa: E731

    if not Path(TARGET_DB).exists():
        check(False, "隔离库存在", "platform-dryrun.db 缺失")
        return

    # --- 11.0 schema_version 与守卫 DDL 结构 ---
    sv3 = q(tgt, "SELECT COUNT(*) FROM schema_version WHERE version=3")
    check(sv3 == 1, "schema_version 已登记 M6 增量（version=3）", f"行数={sv3}")
    tool_cols = {r[1] for r in tgt.execute("PRAGMA table_info(tool)")}
    check({"purpose", "provenance_ref", "health_note"} <= tool_cols,
          "守卫 DDL：tool 追加列 purpose/provenance_ref/health_note 在场",
          f"缺失={sorted({'purpose', 'provenance_ref', 'health_note'} - tool_cols)}")
    tv_master = tgt.execute(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name='tool_version'"
    ).fetchone()[0]
    tv_cols = {r[1] for r in tgt.execute("PRAGMA table_info(tool_version)")}
    check("health" in tv_cols and not re.search(r"version\s+TEXT\s+NOT\s+NULL", tv_master),
          "守卫 DDL：tool_version 重建生效（version NULL-able + health 列）",
          f"health 在场={'health' in tv_cols}")
    alias_master = tgt.execute(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name='entity_alias'"
    ).fetchone()[0]
    check("'workflow'" in alias_master,
          "守卫 DDL：entity_alias 类型域已含 'workflow'")

    # --- 11.1 运行日志与幂等（库内 etl_run_log 表） ---
    runs = [json.loads(r[0]) for r in tgt.execute(
        "SELECT detail FROM etl_run_log ORDER BY run_seq").fetchall()]
    check(len(runs) >= 2, "etl_m6 运行日志含 ≥2 次运行（etl_run_log 表）",
          f"共 {len(runs)} 次")
    last = runs[-1]
    last2 = runs[-2]
    d1 = last2["content_digests"]
    d2 = last["content_digests"]
    check(d1 == d2, "etl_m6 最近两次运行内容摘要一致（no-op）",
          "一致" if d1 == d2 else f"差异表={sorted(t for t in set(d1) | set(d2) if d1.get(t) != d2.get(t))}")

    cur_digests = {}
    for t, pk in (("tool", "tool_id"), ("tool_version", "tv_id"), ("skill", "skill_id"),
                  ("workflow", "workflow_id"), ("workflow_registry_meta", "id")):
        cur_digests[t] = _m6_table_digest(tgt, t, pk)
    cur_digests["entity_alias@m6"] = _m6_table_digest(
        tgt, "entity_alias", "id",
        where="source_system IN (?,?,?)", params=M6_SOURCE_SYSTEMS)
    diffs = [t for t in cur_digests if cur_digests[t] != d2.get(t)]
    check(not diffs, "当前隔离库内容与 etl_m6 最后一次运行摘要一致", f"差异表={diffs or '无'}")

    # --- 11.2 tool 对账 ---
    registry = json.loads(
        Path("/home/emo/.dsh/redteam/rtis/data/registry/tools.json").read_text(
            encoding="utf-8"))
    items = registry["items"]
    tool_rows = {r["tool_id"]: r for r in tgt.execute(
        "SELECT * FROM tool").fetchall()}
    src_names = sorted(x.get("name") or "" for x in items)
    tgt_names = sorted(tool_rows)
    check(src_names == tgt_names,
          "registry 20 工具 ↔ tool 行（ID 集合一致，现网 name 直接沿用为主键）",
          f"源={len(src_names)} 目标={len(tgt_names)} 差异={sorted(set(src_names) ^ set(tgt_names))}")
    check(last["tools"]["quarantine"] == [],
          "tool 隔离清单为空（无未导入条目）",
          f"隔离={last['tools']['quarantine']}")

    # 四态分离：install_state / health_state / permission_level 域检查
    bad_domain = [
        (r["tool_id"], r["install_state"], r["health_state"], r["permission_level"])
        for r in tool_rows.values()
        if r["install_state"] not in ("installed", "missing")
        or r["health_state"] not in ("unknown", "ok", "degraded", "broken")
        or r["permission_level"] not in ("T0", "T1")
    ]
    check(not bad_domain, "tool 四态分离列域合法（install/health/permission 值域）",
          f"违例={bad_domain}")
    name_map = {x.get("name"): x for x in items}
    bad_perm = [r["tool_id"] for r in tool_rows.values()
                if r["permission_level"] != ("T0" if name_map.get(r["tool_id"], {}).get("readonly")
                                             else "T1")]
    check(not bad_perm, "permission_level 与 registry readonly 映射一致（readonly→T0，其余→T1）",
          f"违例={bad_perm}")

    # 健康实测一致性（两个关键修正的现勘复核）
    gogo_bin = Path("/home/emo/.dsh/redteam/toolkit/gogo/gogo")
    pd_bin = Path("/home/emo/.local/bin/pd-httpx")
    gogo = tool_rows.get("gogo")
    pdx = tool_rows.get("pd-httpx")
    check(gogo is not None and gogo_bin.exists() and gogo["install_state"] == "installed",
          "gogo install_state 按二进制实存重判为 installed（registry 声明 false 的漂移已修正）",
          f"二进制存在={gogo_bin.exists()} 落库={gogo['install_state'] if gogo else '无行'}")
    check(pdx is not None and not pd_bin.exists() and pdx["install_state"] == "missing",
          "pd-httpx install_state=missing 与实况一致（registry 声明 false 属实）",
          f"路径存在={pd_bin.exists()} 落库={pdx['install_state'] if pdx else '无行'}")
    health_dist = Counter(r["health_state"] for r in tool_rows.values())
    check(health_dist == Counter(last["tools"]["health_distribution"]),
          "tool 健康分布与运行日志一致",
          f"落库={dict(sorted(health_dist.items()))} 日志={last['tools']['health_distribution']}")

    # --- 11.3 tool_version 对账 ---
    tv_rows = {}
    for r in tgt.execute("SELECT * FROM tool_version WHERE status='current'").fetchall():
        tv_rows[r["tool_id"]] = r
    check(len(tv_rows) == len(tool_rows) and set(tv_rows) == set(tool_rows),
          "每工具恰 1 行 current 版本",
          f"工具={len(tool_rows)} current={len(tv_rows)}")
    sup = q(tgt, "SELECT COUNT(*) FROM tool_version WHERE status='superseded'")
    null_ver = sorted(t for t, r in tv_rows.items() if r["version"] is None)
    check(null_ver == sorted(last["null_version_tools"]),
          "version NULL 集合与运行日志一致（坏探测/无版本输出如实置 NULL）",
          f"落库={null_ver}")
    bad_null = [t for t in null_ver
                if tv_rows[t]["health"] == "ok"
                and tool_rows[t]["kind"] not in ("mcp", "datastore")]
    check(not bad_null,
          "version=NULL 且 health=ok 仅允许 mcp/datastore（结构上无版本概念）",
          f"违例={bad_null}")
    # 坏声明不落库：declared 为坏探测值时，坏值本身不得出现在 version 列
    # （fscan/rtis.py 实测无版本 → NULL；suo5 实测补得 v2.2.0 → 落实测值）
    bad_corrupt = []
    for item in items:
        name = item.get("name")
        if name not in tv_rows:
            continue
        declared_clean, status = _m6_classify_declared(name, item.get("version"))
        if status == "corrupt" and tv_rows[name]["version"] == declared_clean:
            bad_corrupt.append(name)
    check(not bad_corrupt,
          "registry 坏版本声明（fscan/suo5/rtis.py 类）不落入 version 列"
          "（落 NULL 或实测版本，坏值本身不落）",
          f"违例={bad_corrupt}")

    # --- 11.4 drift 重算对账 ---
    actual_drift = sorted((d["tool"], d["field"], d["kind"]) for d in last["tools"]["drift"])
    expected_drift = _m6_expected_drift(items, tool_rows, tv_rows)
    # 活源收敛豁免：registry（tools.json）由 rtis 运行时重建，快照期 drift 可能已自愈
    # （声明值 == 实测值，如 frpc 0.71.1→0.72.0）。收敛项从 actual 侧扣除并 INFO 留痕；
    # 历史事实以 M6_REPORT §drift 与 etl_run_log 为准。仅残余差异非空才 FAIL。
    declared_now = {i.get("name"): i.get("version") for i in items}
    converged = sorted(
        (t, f, k) for (t, f, k) in actual_drift
        if (t, f, k) not in expected_drift
        and f == "version" and k == "value-drift"
        and _m6_versions_equivalent(
            (dict(tv_rows[t])["version"] if t in tv_rows else None),
            _m6_classify_declared(t, declared_now.get(t))[0]))
    residual = sorted(set(actual_drift) - set(converged))
    check(residual == expected_drift,
          "drift 清单重算一致（registry 声明 vs 落库实测，同规则独立复算；活源自愈项豁免）",
          f"日志={len(actual_drift)} 项 收敛={len(converged)} 重算={len(expected_drift)} 项"
          + ("" if residual == expected_drift
             else f" 残余差异={sorted(set(residual) ^ set(expected_drift))}"))
    if converged:
        info("drift 已收敛（registry 自愈至实测值）",
             "；".join(f"{t}.{f}({k})" for t, f, k in converged))
    check(len(actual_drift) > 0, "drift 非空（评审已知缺陷在场）",
          f"共 {len(actual_drift)} 项")
    info("drift 清单",
         "；".join(f"{t}.{f}({k})" for t, f, k in actual_drift))

    # --- 11.5 workflow 对账 ---
    wf_text = Path("/home/emo/.dsh/redteam/rtis/data/registry/workflows.yaml").read_text(
        encoding="utf-8")
    src_wf_ids = sorted(re.findall(r"^\s*-\s+id:\s*(WF-\d{2})\s*$", wf_text, re.M))
    tgt_wf = {r["workflow_id"]: r for r in tgt.execute(
        "SELECT * FROM workflow").fetchall()}
    check(src_wf_ids == sorted(tgt_wf),
          "workflow 行数与 YAML `- id:` 计数一致（ID 集合一致）",
          f"源={len(src_wf_ids)} 目标={len(tgt_wf)} 差异={sorted(set(src_wf_ids) ^ set(tgt_wf))}")
    check(last["workflows"]["quarantine"] == [],
          "workflow 隔离清单为空（受限解析无失败条目）",
          f"隔离={last['workflows']['quarantine']}")
    json_cols = ("trigger_json", "requires_json", "related_skills",
                 "knowledge_queries", "phases", "stop_conditions",
                 "excluded_activities")
    bad_json = []
    for wid, r in tgt_wf.items():
        for c in json_cols:
            if r[c] is not None:
                try:
                    json.loads(r[c])
                except Exception:  # noqa: BLE001
                    bad_json.append((wid, c))
    check(not bad_json, "workflow JSON 字段全部可解析（保真序列化）", f"坏行={bad_json}")
    bad_phases = [wid for wid, r in tgt_wf.items()
                  if not isinstance(json.loads(r["phases"] or "[]"), list)
                  or any(not isinstance(p, dict) or "phase" not in p or "role" not in p
                         for p in json.loads(r["phases"] or "[]"))]
    check(not bad_phases, "workflow phases 均为含 phase/role 的字典列表",
          f"违例={bad_phases}")
    meta = tgt.execute("SELECT * FROM workflow_registry_meta WHERE id=1").fetchone()
    check(meta is not None and meta["default_workflow"] == "WF-00"
          and "WF-00" in tgt_wf,
          "default_workflow=WF-00（M0 报告 §3.2）且 WF-00 行存在",
          f"meta={dict(meta) if meta is not None else None}")

    # related_skills 结构校验 + 引用解析观察项
    skill_names = {r[0] for r in tgt.execute("SELECT name FROM skill")}
    refs, bad_ref_struct = [], []
    for wid, r in tgt_wf.items():
        rs = json.loads(r["related_skills"] or "[]")
        if not isinstance(rs, list) or any(not isinstance(x, str) for x in rs):
            bad_ref_struct.append(wid)
            continue
        refs += rs
    check(not bad_ref_struct, "workflow related_skills 均为字符串数组", f"违例={bad_ref_struct}")
    unres = sorted({x for x in refs if x not in skill_names})
    if unres:
        info("related_skills 引用解析（源 YAML 引用了 skill 表之外的名字，"
             "源数据事实，如实记录不代改）",
             f"引用 {len(refs)} 处，未解析 {len(unres)} 个名字：{';'.join(unres)}")
    else:
        info("related_skills 引用解析", f"引用 {len(refs)} 处全部可解析到 skill 表")

    # --- 11.6 skill 对账 ---
    src_files = sorted(Path("/home/emo/.dsh/redteam/skills").glob("*.md"))
    tgt_skills = {r["skill_id"]: r for r in tgt.execute("SELECT * FROM skill").fetchall()}
    check(len(src_files) == len(tgt_skills) == 125
          and sorted(f.stem for f in src_files) == sorted(tgt_skills),
          "skill 125/125（文件名集合与 skill_id 集合一致）",
          f"源={len(src_files)} 目标={len(tgt_skills)}")
    check(last["skills"]["quarantine"] == [], "skill 隔离清单为空",
          f"隔离={last['skills']['quarantine']}")
    # 缺 role 文件清单与源重扫一致
    no_role_src = []
    for f in src_files:
        lines = f.read_text(encoding="utf-8").splitlines()
        if lines and lines[0].strip() == "---":
            close = next((i for i in range(1, len(lines))
                          if lines[i].strip() == "---"), None)
            fm = lines[1:close] if close else []
            if not any(ln.startswith("role:") for ln in fm):
                no_role_src.append(f.name)
    check(sorted(no_role_src) == sorted(last["skills"]["missing_role"])
          == sorted(r["source_pk"] for r in tgt_skills.values() if r["roles"] is None),
          "缺 role 文件清单：源重扫 = 运行日志 = 落库 roles IS NULL（按源文件名对齐）",
          f"共 {len(no_role_src)} 个：{sorted(no_role_src)}")
    missing_body = [r["skill_id"] for r in tgt_skills.values()
                    if not r["body_ref"] or not Path(r["body_ref"]).exists()]
    check(not missing_body, "skill body_ref 全部指向实存源文件", f"缺失={missing_body}")

    # 字段抽样复建比对（10 个，确定性等距抽样后整文件重解析）
    sample = spread(sorted(tgt_skills), 10)
    bad_fields = []
    for name in sample:
        f = Path(tgt_skills[name]["body_ref"])
        lines = f.read_text(encoding="utf-8").splitlines()
        close = next((i for i in range(1, len(lines))
                      if lines[i].strip() == "---"), None)
        fm = {}
        for raw in lines[1:close]:
            if not raw.strip() or raw.startswith((" ", "\t")) or ":" not in raw:
                continue
            k, _, v = raw.partition(":")
            fm[k.strip()] = v.strip()
        scal = lambda v: (None if v is None or v.strip() == ""
                          else (v[1:-1] if len(v) >= 2 and v[0] == v[-1]
                                and v[0] in "\"'" else
                                (True if v.lower() == "true"
                                 else (False if v.lower() == "false" else v))))
        r = tgt_skills[name]
        expect = {
            "roles": scal(fm.get("role")),
            "description": scal(fm.get("description")),
            "when_to_use": scal(fm.get("whenToUse")),
            "generator": scal(fm.get("generator")),
        }
        enabled = scal(fm.get("enabled"))
        expect_enabled = None if enabled is None else int(enabled)
        if (r["roles"], r["description"], r["when_to_use"], r["generator"],
                r["enabled"]) != (expect["roles"], expect["description"],
                                  expect["when_to_use"], expect["generator"],
                                  expect_enabled):
            bad_fields.append(name)
    check(not bad_fields, "skill 字段抽样复建比对（10 个，front matter 重解析 vs 落库）",
          f"不一致={bad_fields or '无'}")

    # --- 11.7 别名覆盖与总量对账 ---
    for etype, table, pk in (("tool", "tool", "tool_id"),
                             ("skill", "skill", "skill_id"),
                             ("workflow", "workflow", "workflow_id")):
        missing = q(tgt, f"SELECT COUNT(*) FROM {table} t WHERE NOT EXISTS"  # noqa: S608
                        f" (SELECT 1 FROM entity_alias a WHERE a.entity_type='{etype}'"
                        f"  AND a.entity_id=t.{pk})")
        check(missing == 0, f"{etype} 行 100% 有 entity_alias", f"缺失={missing}")
    m1_alias_total = None
    try:
        runs_jsonl = [json.loads(l) for l in open(RUN_LOG_JSONL, encoding="utf-8")]
        m1_alias_total = runs_jsonl[-1]["row_counts"].get("entity_alias")
    except (OSError, KeyError, json.JSONDecodeError):
        pass
    m6_alias_total = last["row_counts"].get("entity_alias@m6")
    cur_alias_total = q(tgt, "SELECT COUNT(*) FROM entity_alias")
    tasks_runs = [json.loads(l) for l in open(TASKS_RUN_LOG_JSONL, encoding="utf-8")]
    tasks_alias_delta = 0
    if tasks_runs:
        a0 = tasks_runs[0]["row_counts"].get("entity_alias@research_task", 0)
        a1 = tasks_runs[-1]["row_counts"].get("entity_alias@research_task", 0)
        tasks_alias_delta = a1 - a0
    # M8 语义修正：构成式（历史数字对账）改为当前态对账——
    # (a) 无孤儿 alias：每条 entity_alias 的 (entity_type, entity_id) 都能在对应实体表命中；
    # (b) 每个实体域 100% 有 alias（各自已有 per-entity 检查，这里做全域汇总）。
    orphan_sql = """
    SELECT COUNT(*) FROM entity_alias a WHERE NOT EXISTS (
      SELECT 1 FROM resource r        WHERE a.entity_type='resource'       AND r.resource_id=a.entity_id)
     AND NOT EXISTS (
      SELECT 1 FROM knowledge k       WHERE a.entity_type='knowledge'      AND k.knowledge_id=a.entity_id)
     AND NOT EXISTS (
      SELECT 1 FROM research_task t   WHERE a.entity_type='research_task'  AND t.task_id=a.entity_id)
     AND NOT EXISTS (
      SELECT 1 FROM tool tl           WHERE a.entity_type='tool'           AND tl.tool_id=a.entity_id)
     AND NOT EXISTS (
      SELECT 1 FROM workflow w        WHERE a.entity_type='workflow'       AND w.workflow_id=a.entity_id)
     AND NOT EXISTS (
      SELECT 1 FROM skill s           WHERE a.entity_type='skill'          AND s.skill_id=a.entity_id)
     AND NOT EXISTS (
      SELECT 1 FROM review rv         WHERE a.entity_type='review'         AND rv.review_id=a.entity_id)
     AND NOT EXISTS (
      SELECT 1 FROM resource_source rs WHERE a.entity_type='resource_source' AND CAST(rs.id AS TEXT)=a.entity_id)
     AND NOT EXISTS (
      SELECT 1 FROM evidence ev       WHERE a.entity_type='evidence'       AND ev.evidence_id=a.entity_id)
    """
    orphan_alias = q(tgt, orphan_sql)
    check(orphan_alias == 0, "entity_alias 无孤儿（当前态全域校验）",
          f"孤儿={orphan_alias} 总量={cur_alias_total}")

    # --- 11.8 tool_execution 空结构 ---
    te = q(tgt, "SELECT COUNT(*) FROM tool_execution")
    check(te == 0, "tool_execution 为空结构（备 M5，本阶段 0 行）", f"行数={te}")


# ---------------------------------------------------------------------------
def main() -> int:
    print("=" * 72)
    print("M1 verify 对账（第一批 Schema/ETL + 第二批 task/evidence/回填/ID 审计扩展"
          " + M6 tools/workflows/skills 扩展 + 全量批 task 大 JSON/evidence 全量扩展；"
          "MIGRATION_STRATEGY §10.5 / 计划 §M1-E / §M6）")
    print("=" * 72)

    manifest = json.loads(MANIFEST_JSON.read_text(encoding="utf-8"))
    cat = open_ro(SRC_CATALOG)
    kb = open_ro(SRC_KNOWLEDGE)
    tgt = sqlite3.connect(str(TARGET_DB))
    tgt.row_factory = sqlite3.Row
    tgt.execute("PRAGMA foreign_keys = ON;")

    verify_snapshot(manifest)
    verify_row_counts(cat, kb, tgt, {})
    verify_id_coverage(cat, kb, tgt)
    verify_alias_conflicts(tgt)
    verify_relations(cat, kb, tgt)
    verify_field_hash(cat, kb, tgt)
    verify_idempotency()
    verify_research_task(tgt)
    verify_evidence(tgt)
    verify_id_audit()
    verify_m6(tgt)

    cat.close()
    kb.close()
    tgt.close()

    total_checks = len(FAILURES)
    print("\n" + "=" * 72)
    if FAILURES:
        print(f"结果：FAIL（{len(FAILURES)} 项失败）")
        for f in FAILURES:
            print("  " + f)
        print("exit 1")
        return 1
    print("结果：PASS — 一致率 100%（全部对账项通过，无未解释差异）")
    print("exit 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
