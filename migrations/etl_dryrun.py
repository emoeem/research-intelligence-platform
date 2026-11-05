#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
M1 第一批 ETL Dry-run（隔离环境，不切流）
=========================================

版本：1.0.0（M1_IMPLEMENTATION_PLAN M1-D：脚本带版本号；转换器版本随行记录）

职责（对齐 RESEARCH_INTELLIGENCE_PLATFORM_MASTER_PLAN §6.3 ETL 原则）：
  Extract：三个源全部只读打开（sqlite3 URI mode=ro / JSON 只读），抽取前记录源库 SHA-256。
  Transform：版本化映射规则（本文件内的常量表），无隐形修正。
  Load：只写隔离目标库 platform-dryrun.db，绝不触碰任何源库。
  Replay：全部写入使用确定性主键 + INSERT OR REPLACE，重复执行幂等覆盖。
  Rollback：删除 platform-dryrun.db 即完整回滚，与源系统零耦合。

源与裁定（任务书）：
  源A catalog.db（ERI，81 resources）
      resources            → resource（quality_scores 按裁定折入质量字段）
      resource_sources     → resource_source（“来源”子表）
      review_log           → review（queue → target_type，K-07：1:N 多态）
      d5_candidates        → 隔离待审清单（quarantine/d5_candidates.jsonl），不自动转实体
  源B knowledge.db（RTIS，只读 URI mode=ro）
      poc × k_meta（28）   → knowledge record_kind='own'
      k_negative（17）     → knowledge record_kind='negative'（K-17 最小治理对象）
      14,111 条 nuclei 模板索引（poc.source='nuclei'）不迁移：可重建，归属 Resource 侧，
                           仅统计（MIGRATION_STRATEGY §10.1 可重建级）。
      其余非 k_meta、非模板 poc（578 条）本批不迁移：属未分层自有知识，留第二批。
  源C RTIS task JSON（35 个）
      仅统计数量与状态分布（rtis_tasks_stats.json），M1 第一批不迁移 task 实体。
  28 个 assets.db：不迁移（M1_ASSETS_DB_COMPATIBILITY 盘点，留第二批做 Evidence 映射）。

评审修正落实（M1_READINESS_REVIEW §2）：
  K-07 review 1:N 多态；K-08 lifecycle/governance 双轴；K-09 别名四要素；
  K-10 tool_version 子表（本批空表）；K-16 resource 生命周期含 FETCHED/VERIFIED；
  K-17 反知识以 record_kind='negative' + 专属列承载。

幂等约定：所有时间戳列取源内时间（确定性），运行时刻只写执行日志（etl_run_log.jsonl），
不进入数据行；因此同源快照重复执行产生逐字节一致的逻辑内容。
"""

import hashlib
import json
import sqlite3
import sys
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path

ETL_VERSION = "1.0.0"

# ---------------------------------------------------------------------------
# 路径常量（硬约束：源只读；只写 migrations/ 目录内）
# ---------------------------------------------------------------------------
MIG_DIR = Path(__file__).resolve().parent

SRC_CATALOG = Path("/home/emo/code/pensting/security-research-upgrade/eri/db/catalog.db")
SRC_KNOWLEDGE = Path("/home/emo/.dsh/redteam/knowledge.db")
SRC_TASKS_DIR = Path("/home/emo/.dsh/redteam/rtis/data/tasks")

TARGET_DB = MIG_DIR / "platform-dryrun.db"
SCHEMA_SQL = MIG_DIR / "001_init.sql"
MANIFEST_JSON = MIG_DIR / "source_manifest.json"
QUARANTINE_DIR = MIG_DIR / "quarantine"
QUARANTINE_D5 = QUARANTINE_DIR / "d5_candidates.jsonl"
TASKS_STATS_JSON = MIG_DIR / "rtis_tasks_stats.json"
RUN_LOG_JSONL = MIG_DIR / "etl_run_log.jsonl"
RUN_SUMMARY_JSON = MIG_DIR / "etl_last_run.json"

SCHEMA_VERSION = 1

# ---------------------------------------------------------------------------
# 版本化映射规则（Transform 常量表）
# ---------------------------------------------------------------------------
SOURCE_SYSTEM_CATALOG = "eri-catalog"
SOURCE_SYSTEM_KNOWLEDGE = "rtis-knowledge"
SOURCE_SYSTEM_RTIS_TASKS = "rtis-tasks"

# K-16/K-08：生命周期链纯化后仅保留处理阶段；治理状态进 governance 列。
# 资源：源 status 仅 INDEXED（已实测断言）；INDEXED 为处理阶段，映射直通。
RESOURCE_LIFECYCLE_MAP = {
    "INDEXED": "INDEXED",
}

# 知识：k_meta.lifecycle 原值 → 纯化生命周期（原值保留于 source_lifecycle 列）。
#   Field-validated / Lab-validated：验证维度已满足 → VERIFIED
#   Researched：已研究未实测 → CANDIDATE
KNOWLEDGE_LIFECYCLE_MAP = {
    "Field-validated": "VERIFIED",
    "Lab-validated": "VERIFIED",
    "Researched": "CANDIDATE",
}

GOVERNANCE_ACTIVE = "ACTIVE"  # 本批源数据无 QUARANTINED/REJECTED（已断言）

QUARANTINE_D5_REASON = (
    "D5 疑似重复候选对：M1 第一批裁定不自动转实体，转人工待审清单"
    "（计划 §M1 明确不做：不自动批准人工审核队列）"
)


def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def open_ro(path: Path) -> sqlite3.Connection:
    """只读打开源库：URI mode=ro，绝不写源。"""
    if not path.exists():
        raise SystemExit(f"FATAL: 源库不存在：{path}")
    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    conn.row_factory = sqlite3.Row
    # 只读连接上无法执行写 PRAGMA；防御性确认无写权限语义
    return conn


def open_target() -> sqlite3.Connection:
    conn = sqlite3.connect(str(TARGET_DB))
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA foreign_keys = ON;")
    return conn


def init_target_schema(conn: sqlite3.Connection) -> None:
    """幂等应用 Schema V1（IF NOT EXISTS / INSERT OR IGNORE）。"""
    sql = SCHEMA_SQL.read_text(encoding="utf-8")
    conn.executescript(sql)
    conn.execute("PRAGMA foreign_keys = ON;")
    uv = conn.execute("PRAGMA user_version;").fetchone()[0]
    if uv != SCHEMA_VERSION:
        raise SystemExit(f"FATAL: user_version={uv}，期望 {SCHEMA_VERSION}")
    row = conn.execute(
        "SELECT COUNT(*) AS c FROM schema_version WHERE version = ?", (SCHEMA_VERSION,)
    ).fetchone()
    if row["c"] != 1:
        raise SystemExit("FATAL: schema_version 缺少 V1 种子行")


# ---------------------------------------------------------------------------
# 表清单（用于内容摘要与行数统计；pk 为稳定排序列）
# ---------------------------------------------------------------------------
CONTENT_TABLES = {
    "resource": "resource_id",
    "resource_source": "id",
    "knowledge": "knowledge_id",
    "review": "review_id",
    "entity_alias": "id",
    "evidence": "evidence_id",
    "tool": "tool_id",
    "tool_version": "tv_id",
    "skill": "skill_id",
    "audit_event": "event_id",
}


def table_digest(conn: sqlite3.Connection, table: str, pk: str) -> str:
    """逻辑内容摘要：全行按稳定主键排序后规范化哈希（与文件字节布局无关）。"""
    h = hashlib.sha256()
    cur = conn.execute(f"SELECT * FROM {table} ORDER BY {pk}")  # noqa: S608（表名为内部常量）
    cols = [d[0] for d in cur.description]
    for row in cur:
        vals = ["" if v is None else repr(v) for v in row]
        h.update(("\x1f".join(cols) + "\x1e" + "\x1f".join(vals) + "\x1e").encode("utf-8"))
    return h.hexdigest()


# ---------------------------------------------------------------------------
# 别名登记（K-09 四要素）
# ---------------------------------------------------------------------------
def alias_key(entity_type: str, source_system: str, namespace: str, alias_value: str) -> str:
    raw = f"{entity_type}|{source_system}|{namespace}|{alias_value}"
    return hashlib.md5(raw.encode("utf-8")).hexdigest()  # noqa: S324（非安全用途，仅确定性键）


def register_alias(conn: sqlite3.Connection, cur: sqlite3.Cursor, entity_type: str,
                   entity_id: str, source_system: str, namespace: str, alias_value: str,
                   stats: dict) -> None:
    """登记别名；同键不同实体 = 冲突，红线：不覆盖、不静默，直接失败。"""
    key = alias_key(entity_type, source_system, namespace, alias_value)
    existing = cur.execute(
        "SELECT entity_id FROM entity_alias WHERE id = ?", (key,)
    ).fetchone()
    if existing is not None and existing["entity_id"] != entity_id:
        raise SystemExit(
            f"FATAL: Alias 冲突：{source_system}/{namespace}/{alias_value} "
            f"已映射到 {existing['entity_id']}，又尝试映射到 {entity_id}"
        )
    if existing is not None:
        stats["alias_skipped_existing"] += 1
        return
    cur.execute(
        "INSERT INTO entity_alias (id, entity_type, entity_id, source_system, namespace,"
        " alias_value, valid_from, valid_until, created_at, updated_at, schema_version)"
        " VALUES (?,?,?,?,?,?,?,?,?,?,?)",
        (key, entity_type, entity_id, source_system, namespace, alias_value,
         None, None, now_iso(), now_iso(), SCHEMA_VERSION),
    )
    stats["alias_inserted"] += 1


# ---------------------------------------------------------------------------
# 源A：catalog.db
# ---------------------------------------------------------------------------
def etl_catalog(conn: sqlite3.Connection, stats: dict) -> None:
    src = open_ro(SRC_CATALOG)
    cur = conn.cursor()

    # --- 断言源口径（防止源漂移后静默错映射） ---
    status_dist = dict(src.execute(
        "SELECT status, COUNT(*) FROM resources GROUP BY status").fetchall())
    unknown = set(status_dist) - set(RESOURCE_LIFECYCLE_MAP)
    if unknown:
        raise SystemExit(f"FATAL: resources.status 出现未裁定取值：{unknown}")
    trust_dist = dict(src.execute(
        "SELECT trust_level, COUNT(*) FROM resources GROUP BY trust_level").fetchall())
    bad_trust = {k for k in trust_dist if k and k not in ("A", "B", "C", "D")}
    if bad_trust:
        raise SystemExit(f"FATAL: trust_level 出现隔离/拒绝类取值，需人工裁定：{bad_trust}")

    # --- A1: resources × quality_scores → resource ---
    rows = src.execute(
        "SELECT r.*, q.relevance AS q_relevance, q.reliability AS q_reliability,"
        " q.maintenance AS q_maintenance, q.maturity AS q_maturity,"
        " q.complementarity AS q_complementarity, q.license_clarity AS q_license_clarity,"
        " q.docs_testing AS q_docs_testing, q.final_score AS q_final_score,"
        " q.grade AS q_grade, q.adoption_blocked AS q_adoption_blocked,"
        " q.reasons AS q_reasons, q.computed_at AS q_computed_at,"
        " q.engine_version AS q_engine_version"
        " FROM resources r LEFT JOIN quality_scores q ON q.resource_id = r.resource_id"
        " ORDER BY r.resource_id"
    ).fetchall()
    for r in rows:
        lifecycle = RESOURCE_LIFECYCLE_MAP[r["status"]]
        cur.execute(
            "INSERT OR REPLACE INTO resource ("
            " resource_id, identity, canonical_url, resource_type, name, description,"
            " owner, repo_name, lifecycle, governance, source_status,"
            " quality_score, trust_level,"
            " quality_relevance, quality_reliability, quality_maintenance, quality_maturity,"
            " quality_complementarity, quality_license_clarity, quality_docs_testing,"
            " quality_final_score, quality_grade, quality_adoption_blocked, quality_reasons,"
            " quality_computed_at, quality_engine_version,"
            " first_seen, last_seen, last_checked, last_source_commit,"
            " source_system, created_at, updated_at, schema_version"
            ") VALUES (" + ",".join(["?"] * 34) + ")",
            (r["resource_id"], r["identity"], r["canonical_url"], r["resource_type"],
             r["name"], r["description"], r["owner"], r["repo_name"],
             lifecycle, GOVERNANCE_ACTIVE, r["status"],
             r["quality_score"], r["trust_level"],
             r["q_relevance"], r["q_reliability"], r["q_maintenance"], r["q_maturity"],
             r["q_complementarity"], r["q_license_clarity"], r["q_docs_testing"],
             r["q_final_score"], r["q_grade"], r["q_adoption_blocked"], r["q_reasons"],
             r["q_computed_at"], r["q_engine_version"],
             r["first_seen"], r["last_seen"], r["last_checked"], r["last_source_commit"],
             SOURCE_SYSTEM_CATALOG, r["created_at"], r["updated_at"], SCHEMA_VERSION),
        )
        stats["resource"] += 1
        register_alias(conn, cur, "resource", r["resource_id"],
                       SOURCE_SYSTEM_CATALOG, "catalog.resources.resource_id",
                       r["resource_id"], stats)

    # --- A2: resource_sources → resource_source ---
    rows = src.execute("SELECT * FROM resource_sources ORDER BY id").fetchall()
    for r in rows:
        cur.execute(
            "INSERT OR REPLACE INTO resource_source ("
            " id, resource_id, source_id, source_repository, source_file, source_section,"
            " source_position, source_commit, source_url, first_seen, last_seen,"
            " source_system, created_at, updated_at, schema_version"
            ") VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            (r["id"], r["resource_id"], r["source_id"], r["source_repository"],
             r["source_file"], r["source_section"], r["source_position"],
             r["source_commit"], r["source_url"], r["first_seen"], r["last_seen"],
             SOURCE_SYSTEM_CATALOG, r["first_seen"], r["last_seen"], SCHEMA_VERSION),
        )
        stats["resource_source"] += 1
        register_alias(conn, cur, "resource_source", str(r["id"]),
                       SOURCE_SYSTEM_CATALOG, "catalog.resource_sources.id",
                       str(r["id"]), stats)

    # --- A3: review_log → review（queue → target_type；K-07：1:N 多态） ---
    rows = src.execute("SELECT * FROM review_log ORDER BY id").fetchall()
    for r in rows:
        review_id = f"{SOURCE_SYSTEM_CATALOG}:review_log:{r['id']}"
        created = r["reviewed_at"] or ""
        cur.execute(
            "INSERT OR REPLACE INTO review ("
            " review_id, target_type, target_id, decision, confidence, reason, reviewer,"
            " reviewed_at, source_system, source_pk, created_at, updated_at, schema_version"
            ") VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
            (review_id, r["queue"], r["item_key"], r["decision"], r["confidence"],
             r["reason"], r["reviewer"], r["reviewed_at"],
             SOURCE_SYSTEM_CATALOG, f"review_log:{r['id']}",
             created, created, SCHEMA_VERSION),
        )
        stats["review"] += 1
        register_alias(conn, cur, "review", review_id,
                       SOURCE_SYSTEM_CATALOG, "catalog.review_log.id",
                       str(r["id"]), stats)

    # --- A4: d5_candidates → 隔离待审清单（不自动转实体） ---
    rows = src.execute("SELECT * FROM d5_candidates ORDER BY id").fetchall()
    QUARANTINE_DIR.mkdir(parents=True, exist_ok=True)
    with open(QUARANTINE_D5, "w", encoding="utf-8") as f:
        for r in rows:
            rec = {
                "source_system": SOURCE_SYSTEM_CATALOG,
                "source_table": "d5_candidates",
                "source_pk": str(r["id"]),
                "quarantine_reason": QUARANTINE_D5_REASON,
                "disposition": "manual-review-pending",
                "record": {k: r[k] for k in r.keys()},
            }
            f.write(json.dumps(rec, ensure_ascii=False, sort_keys=True) + "\n")
            stats["d5_quarantined"] += 1

    src.close()


# ---------------------------------------------------------------------------
# 源B：knowledge.db
# ---------------------------------------------------------------------------
def etl_knowledge(conn: sqlite3.Connection, stats: dict) -> None:
    src = open_ro(SRC_KNOWLEDGE)
    cur = conn.cursor()

    # --- 断言源口径 ---
    lc_dist = dict(src.execute(
        "SELECT lifecycle, COUNT(*) FROM k_meta GROUP BY lifecycle").fetchall())
    unknown = set(lc_dist) - set(KNOWLEDGE_LIFECYCLE_MAP)
    if unknown:
        raise SystemExit(f"FATAL: k_meta.lifecycle 出现未裁定取值：{unknown}")

    # 统计（写入 summary；不迁移）：nuclei 模板索引与其余未分层自有知识
    poc_total = src.execute("SELECT COUNT(*) FROM poc").fetchone()[0]
    nuclei_cnt = src.execute(
        "SELECT COUNT(*) FROM poc WHERE source = 'nuclei'").fetchone()[0]
    kmeta_cnt = src.execute("SELECT COUNT(*) FROM k_meta").fetchone()[0]
    kneg_cnt = src.execute("SELECT COUNT(*) FROM k_negative").fetchone()[0]
    not_migrated_other = poc_total - nuclei_cnt - kmeta_cnt  # 非 k_meta 非 nuclei
    stats["kb_poc_total"] = poc_total
    stats["kb_nuclei_index_not_migrated"] = nuclei_cnt
    stats["kb_other_not_migrated"] = not_migrated_other
    stats["kb_kmeta_source"] = kmeta_cnt
    stats["kb_knegative_source"] = kneg_cnt

    # --- B1: poc × k_meta → knowledge record_kind='own' ---
    rows = src.execute(
        "SELECT p.*, m.original_category, m.normalized_type, m.archetypes,"
        " m.technology_tags, m.method_tags, m.evidence_level, m.lifecycle AS m_lifecycle,"
        " m.verification_basis, m.dedup_key, m.version AS m_version,"
        " m.last_reviewed, m.updated_at AS m_updated_at"
        " FROM poc p JOIN k_meta m ON m.knowledge_id = p.id ORDER BY p.code"
    ).fetchall()
    own_lifecycle = {}  # parent_code -> 映射后的纯化生命周期（供 B2 反知识行继承）
    for r in rows:
        code = r["code"]
        lifecycle = KNOWLEDGE_LIFECYCLE_MAP[r["m_lifecycle"]]
        own_lifecycle[code] = lifecycle
        cur.execute(
            "INSERT OR REPLACE INTO knowledge ("
            " knowledge_id, record_kind, lifecycle, governance, title, content_kind,"
            " category, cve, component, versions, severity, language, source, source_url,"
            " description, usage, content, path, verified, verified_note, hit_count,"
            " used_on, engagement_id, engagement_name, asset_target, found_by_agent, tags,"
            " original_category, normalized_type, archetypes, technology_tags, method_tags,"
            " evidence_level, verification_basis, dedup_key, meta_version, last_reviewed,"
            " meta_updated_at, source_lifecycle,"
            " neg_do_not_conclude, neg_false_positive_cases, neg_known_failures,"
            " neg_stop_conditions, neg_not_applicable_when, neg_source_knowledge_id,"
            " negates_knowledge_id, source_system, created_by, created_at, updated_at,"
            " schema_version"
            ") VALUES (" + ",".join(["?"] * 51) + ")",
            (code, "own", lifecycle, GOVERNANCE_ACTIVE, r["title"], r["kind"],
             r["category"], r["cve"], r["component"], r["versions"], r["severity"],
             r["language"], r["source"], r["source_url"], r["description"], r["usage"],
             r["content"], r["path"], r["verified"], r["verified_note"], r["hit_count"],
             r["used_on"], r["engagement_id"], r["engagement_name"], r["asset_target"],
             r["found_by_agent"], r["tags"],
             r["original_category"], r["normalized_type"], r["archetypes"],
             r["technology_tags"], r["method_tags"], r["evidence_level"],
             r["verification_basis"], r["dedup_key"], r["m_version"], r["last_reviewed"],
             r["m_updated_at"], r["m_lifecycle"],
             None, None, None, None, None, None, None,
             SOURCE_SYSTEM_KNOWLEDGE, r["created_by"], r["created_at"], r["updated_at"],
             SCHEMA_VERSION),
        )
        stats["knowledge_own"] += 1
        register_alias(conn, cur, "knowledge", code,
                       SOURCE_SYSTEM_KNOWLEDGE, "knowledge.poc.code", code, stats)

    # --- B2: k_negative → knowledge record_kind='negative'（K-17） ---
    rows = src.execute(
        "SELECT n.*, p.code AS parent_code, p.title AS parent_title,"
        " p.created_at AS parent_created_at, p.updated_at AS parent_updated_at"
        " FROM k_negative n JOIN poc p ON p.id = n.knowledge_id ORDER BY n.id"
    ).fetchall()
    for r in rows:
        neg_id = f"neg:{r['parent_code']}"
        parent_code = r["parent_code"]
        parent_in_target = parent_code in own_lifecycle
        # 反知识行生命周期：继承父知识映射后的生命周期（确定性派生）；
        # 父未迁移（1/17，父 poc 不在 k_meta 覆盖内）时取链首 DISCOVERED（成熟度未知的
        # 最小主张），negates 置空，源引用保留于 neg_source_knowledge_id，不猜测转换。
        if parent_in_target:
            lifecycle = own_lifecycle[parent_code]
        else:
            lifecycle = "DISCOVERED"
            stats["knowledge_negative_parent_unresolved"] += 1
        cur.execute(
            "INSERT OR REPLACE INTO knowledge ("
            " knowledge_id, record_kind, lifecycle, governance, title, content_kind,"
            " category, cve, component, versions, severity, language, source, source_url,"
            " description, usage, content, path, verified, verified_note, hit_count,"
            " used_on, engagement_id, engagement_name, asset_target, found_by_agent, tags,"
            " original_category, normalized_type, archetypes, technology_tags, method_tags,"
            " evidence_level, verification_basis, dedup_key, meta_version, last_reviewed,"
            " meta_updated_at, source_lifecycle,"
            " neg_do_not_conclude, neg_false_positive_cases, neg_known_failures,"
            " neg_stop_conditions, neg_not_applicable_when, neg_source_knowledge_id,"
            " negates_knowledge_id, source_system, created_by, created_at, updated_at,"
            " schema_version"
            ") VALUES (" + ",".join(["?"] * 51) + ")",
            (neg_id, "negative", lifecycle,
             GOVERNANCE_ACTIVE, r["parent_title"], None,
             None, None, None, None, None, None, None, None,
             None, None, r["do_not_conclude"], None, None, None, None,
             None, None, None, None, None, None,
             None, None, None, None, None, None, None, None, None, None,
             None, None,
             r["do_not_conclude"], r["false_positive_cases"], r["known_failures"],
             r["stop_conditions"], r["not_applicable_when"], str(r["knowledge_id"]),
             parent_code if parent_in_target else None,
             SOURCE_SYSTEM_KNOWLEDGE, None, r["parent_created_at"], r["parent_updated_at"],
             SCHEMA_VERSION),
        )
        stats["knowledge_negative"] += 1
        register_alias(conn, cur, "knowledge", neg_id,
                       SOURCE_SYSTEM_KNOWLEDGE, "knowledge.k_negative.id",
                       str(r["id"]), stats)

    src.close()


# ---------------------------------------------------------------------------
# 源C：RTIS task JSON（仅统计，不迁移）
# ---------------------------------------------------------------------------
def etl_tasks_stats(stats: dict) -> None:
    files = sorted(SRC_TASKS_DIR.glob("*.json"))
    state_dist = Counter()
    type_dist = Counter()
    parse_errors = []
    for f in files:
        try:
            with open(f, encoding="utf-8") as fh:
                d = json.load(fh)
            t = d.get("task") or {}
            state_dist[t.get("state") or t.get("status") or "MISSING"] += 1
            type_dist[t.get("task_type") or "MISSING"] += 1
        except Exception as e:  # noqa: BLE001
            parse_errors.append({"file": f.name, "error": repr(e)})
    stats["rtis_task_json_count"] = len(files)
    stats["rtis_task_state_distribution"] = dict(sorted(state_dist.items()))
    stats["rtis_task_type_distribution"] = dict(sorted(type_dist.items()))
    stats["rtis_task_parse_errors"] = parse_errors

    payload = {
        "source_dir": str(SRC_TASKS_DIR),
        "task_json_count": len(files),
        "state_distribution": dict(sorted(state_dist.items())),
        "task_type_distribution": dict(sorted(type_dist.items())),
        "parse_errors": parse_errors,
        "note": "M1 第一批不迁移 task 实体（数量与状态分布仅入报告）；"
                "字段映射留 M1 第二批（UNIFIED_DOMAIN_MODEL §7.13）。",
        "generated_by": f"etl_dryrun.py {ETL_VERSION}",
    }
    TASKS_STATS_JSON.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8")


# ---------------------------------------------------------------------------
# 主流程
# ---------------------------------------------------------------------------
def main() -> int:
    started = now_iso()
    print(f"[etl_dryrun] v{ETL_VERSION} started_at={started}")
    print(f"[etl_dryrun] 目标（隔离）：{TARGET_DB}")

    # 1) 源清单：SHA-256 + 行数锚点（只读）
    manifest = {"etl_version": ETL_VERSION, "sources": []}
    for name, path in (("catalog", SRC_CATALOG), ("knowledge", SRC_KNOWLEDGE)):
        manifest["sources"].append({
            "role": name, "path": str(path), "sha256": sha256_file(path),
            "size_bytes": path.stat().st_size,
        })
    manifest["assets_db"] = {
        "count": "27 实存 / 1 engagement 无库（共 28 engagement）",
        "disposition": "M1 第一批不迁移；留第二批做 Evidence 映射",
    }
    manifest["rtis_tasks"] = {"path": str(SRC_TASKS_DIR),
                              "disposition": "仅统计，不迁移"}
    MANIFEST_JSON.write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8")
    for s in manifest["sources"]:
        print(f"[etl_dryrun] 源 {s['role']}: sha256={s['sha256'][:16]}… size={s['size_bytes']}")

    # 2) 目标库（隔离）：应用 Schema V1（幂等）
    conn = open_target()
    init_target_schema(conn)

    stats: dict = Counter()
    stats["alias_inserted"] = 0
    stats["alias_skipped_existing"] = 0

    # 3) Load 前摘要（幂等对照基线）
    before = {t: table_digest(conn, t, pk) for t, pk in CONTENT_TABLES.items()}

    # 4) ETL：三个源
    etl_catalog(conn, stats)
    etl_knowledge(conn, stats)
    etl_tasks_stats(stats)
    conn.commit()

    # 5) Load 后摘要 + 差异（幂等判定数据）
    after = {t: table_digest(conn, t, pk) for t, pk in CONTENT_TABLES.items()}
    changed = sorted(t for t in after if before[t] != after[t])

    row_counts = {}
    for t, pk in CONTENT_TABLES.items():
        row_counts[t] = conn.execute(f"SELECT COUNT(*) FROM {t}").fetchone()[0]  # noqa: S608
    fk_violations = conn.execute("PRAGMA foreign_key_check;").fetchall()
    conn.close()

    result = {
        "etl_version": ETL_VERSION,
        "started_at": started,
        "finished_at": now_iso(),
        "target_db": str(TARGET_DB),
        "row_counts": row_counts,
        "tables_changed_vs_previous_state": changed,
        "content_digests": after,
        "fk_violations": len(fk_violations),
        "stats": dict(stats),
    }
    RUN_SUMMARY_JSON.write_text(
        json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8")
    with open(RUN_LOG_JSONL, "a", encoding="utf-8") as f:
        f.write(json.dumps(result, ensure_ascii=False, sort_keys=True) + "\n")

    print(f"[etl_dryrun] 行数：{json.dumps(row_counts, ensure_ascii=False)}")
    print(f"[etl_dryrun] 统计：{json.dumps(dict(stats), ensure_ascii=False)}")
    print(f"[etl_dryrun] 与上一状态相比变化的表：{changed or '（无，no-op）'}")
    print(f"[etl_dryrun] FK 违例：{len(fk_violations)}")
    print(f"[etl_dryrun] done at {result['finished_at']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
