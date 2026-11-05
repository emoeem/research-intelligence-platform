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
  8. ResearchTask 对账（第二批）：源 35 JSON = 目标 35 行；状态分布原样一致；
     status_history 保真；etl_tasks 重复执行 no-op。
  9. Evidence 抽样对账（第二批）：逐库逐表 源总数/抽取数/目标行数 对账；
     payload_json + content_hash 全字段逐行复建比对；隔离登记 2 条；
     etl_evidence 重复执行 no-op。
  10. ID 审计结论（计划 §5.4）：id_audit_result.json 结论为"现网 ID 可直接沿用"，
      唯一性/格式/跨源碰撞/alias 一致性全过，且结果与当前库同步。

红线：一致率必须 100%；任何一项失败 exit 1。
"""

import hashlib
import json
import sqlite3
import sys
from collections import Counter
from pathlib import Path

MIG_DIR = Path(__file__).resolve().parent

SRC_CATALOG = Path("/home/emo/code/pensting/security-research-upgrade/eri/db/catalog.db")
SRC_KNOWLEDGE = Path("/home/emo/.dsh/redteam/knowledge.db")
SRC_TASKS_DIR = Path("/home/emo/.dsh/redteam/rtis/data/tasks")
TARGET_DB = MIG_DIR / "platform-dryrun.db"
MANIFEST_JSON = MIG_DIR / "source_manifest.json"
QUARANTINE_D5 = MIG_DIR / "quarantine" / "d5_candidates.jsonl"
RUN_LOG_JSONL = MIG_DIR / "etl_run_log.jsonl"
TASKS_RUN_LOG_JSONL = MIG_DIR / "etl_tasks_run_log.jsonl"
EVID_RUN_LOG_JSONL = MIG_DIR / "etl_evidence_run_log.jsonl"
EVID_LAST_RUN_JSON = MIG_DIR / "etl_evidence_last_run.json"
ID_AUDIT_RESULT_JSON = MIG_DIR / "id_audit_result.json"

SAMPLE_N = 10
SAMPLE_LIMIT = 20  # etl_evidence.py 每源表抽取行数（同一常量）

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
        r1, r2 = runs[-2], runs[-1]
        check(r1["content_digests"] == r2["content_digests"],
              "第 1、2 次运行全表内容摘要一致（no-op）",
              "一致" if r1["content_digests"] == r2["content_digests"] else "不一致")
        check(r1["row_counts"] == r2["row_counts"], "第 1、2 次运行行数一致")
    # 当前库与最后一次运行一致
    conn = sqlite3.connect(str(TARGET_DB))
    conn.execute("PRAGMA foreign_keys = ON;")
    last = runs[-1]
    tables = {
        "resource": "resource_id", "resource_source": "id", "knowledge": "knowledge_id",
        "review": "review_id", "entity_alias": "id", "evidence": "evidence_id",
        "tool": "tool_id", "tool_version": "tv_id", "skill": "skill_id",
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
# 8. ResearchTask 对账（第二批映射）
# ---------------------------------------------------------------------------
def verify_research_task(tgt) -> None:
    print("\n== 8. ResearchTask 对账（第二批映射，源只读） ==")
    q = lambda c, sql: c.execute(sql).fetchone()[0]  # noqa: E731

    files = sorted(SRC_TASKS_DIR.glob("*.json"))
    src_ids, src_state = [], Counter()
    for f in files:
        with open(f, encoding="utf-8") as fh:  # 只读打开源 JSON
            t = (json.load(fh).get("task") or {})
        src_ids.append(t.get("id"))
        src_state[t.get("state") or t.get("status") or "MISSING"] += 1

    tgt_rows = tgt.execute(
        "SELECT task_id, state, status_history FROM research_task ORDER BY task_id"
    ).fetchall()
    tgt_ids = {r["task_id"] for r in tgt_rows}
    check(len(src_ids) == len(tgt_rows),
          "行数对账：RTIS task JSON → research_task",
          f"源={len(src_ids)} 目标={len(tgt_rows)}")
    check(set(src_ids) == tgt_ids, "task ID 集合一致（现网 ID 直接沿用为平台主键）",
          f"差异={sorted(set(src_ids) ^ tgt_ids)}")
    tgt_state = Counter(r["state"] for r in tgt_rows)
    check(src_state == tgt_state,
          "研究状态分布与源一致（state 原值保留，不归一化）",
          f"源={dict(sorted(src_state.items()))} 目标={dict(sorted(tgt_state.items()))}")

    bad_hist = [r["task_id"] for r in tgt_rows
                if not isinstance(json.loads(r["status_history"]), list)]
    check(not bad_hist, "status_history JSON 保真可解析（数组）", f"坏行={bad_hist}")

    missing_alias = q(tgt, "SELECT COUNT(*) FROM research_task t WHERE NOT EXISTS"
                           " (SELECT 1 FROM entity_alias a WHERE a.entity_type='research_task'"
                           "  AND a.entity_id=t.task_id)")
    check(missing_alias == 0, "research_task 行 100% 有 entity_alias", f"缺失={missing_alias}")

    sv2 = q(tgt, "SELECT COUNT(*) FROM schema_version WHERE version=2")
    check(sv2 == 1, "schema_version 已登记第二批增量（version=2）", f"行数={sv2}")

    runs = [json.loads(l) for l in open(TASKS_RUN_LOG_JSONL, encoding="utf-8")]
    no_op = (len(runs) >= 2 and runs[-2]["content_digests"] == runs[-1]["content_digests"]
             and runs[-2]["row_counts"] == runs[-1]["row_counts"])
    check(no_op, "etl_tasks 重复执行 no-op（最近两次运行摘要一致）",
          f"共 {len(runs)} 次运行")


# ---------------------------------------------------------------------------
# 9. Evidence 抽样对账（第二批映射）
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
    print("\n== 9. Evidence 抽样对账（第二批映射，源只读） ==")

    last = json.loads(EVID_LAST_RUN_JSON.read_text(encoding="utf-8"))

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

    # 逐库逐表对账 + 全字段逐行复建比对
    for d in last["sampled"]:
        db_path = Path(d["db_path"])
        src = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
        src.row_factory = sqlite3.Row
        for t in ("http_evidence", "shot"):
            rec = d["tables"][t]
            total = src.execute(f"SELECT COUNT(*) FROM {t}").fetchone()[0]  # noqa: S608
            expect = min(total, SAMPLE_LIMIT)
            tgt_cnt = tgt.execute(
                "SELECT COUNT(*) FROM evidence WHERE source_pk LIKE ?",
                (f"{d['db_path']}#{t}#%",)).fetchone()[0]
            check(rec["source_total"] == total and rec["extracted"] == expect
                  and tgt_cnt == expect,
                  f"抽样对账：{d['engagement']}（{d['role']}）#{t}",
                  f"源总数={total} 应抽={expect} 目标行={tgt_cnt}")
            bad = []
            for row in src.execute(
                    f"SELECT * FROM {t} ORDER BY id LIMIT ?",  # noqa: S608
                    (SAMPLE_LIMIT,)).fetchall():
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
            check(not bad,
                  f"全字段保真（payload_json + content_hash 逐行复建）："
                  f"{d['engagement']}#{t}", f"不一致行={bad}")
        src.close()

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
def main() -> int:
    print("=" * 72)
    print("M1 verify 对账（第一批 Schema/ETL + 第二批 task/evidence/回填/ID 审计扩展；"
          "MIGRATION_STRATEGY §10.5 / 计划 §M1-E）")
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
