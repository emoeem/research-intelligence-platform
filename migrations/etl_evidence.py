#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
M1 第二批 ETL：Evidence 映射（隔离环境，不切流）
==================================================

版本：2.0.0（M1_IMPLEMENTATION_PLAN M1-D：脚本带版本号）
  v2.0.0 变更（M1 全量批）：
    - 全量迁移：兼容库从抽样 4 个扩展为全部（26 个），每源表抽取上限
      （前 20 行）取消，http_evidence / shot 全表迁移。
    - 坏行隔离：单行映射失败（如时间戳断言不通过）不再中止整个 ETL，逐行
      写入 quarantine/evidence_isolation.jsonl（确定性覆写），不迁移、
      不静默丢弃。
    - 目标库可配置：RP_TARGET_DB 环境变量（全量批重建流程
      platform-fullbatch.db 用），默认仍为隔离库 platform-dryrun.db。
  v1.0.0（第二批）：抽样映射（4 兼容库、每表前 20 行）。

职责：
  Extract：/home/emo/.dsh/redteam/engagements/*/assets.db 全部只读打开
           （URI mode=ro），抽取前登记表集指纹与文件大小。
  Select（确定性规则，全部写进运行日志）：
    - 兼容类 = assets.db 表集指纹与多数派一致的库（26 个），全量选择，
      角色标记 compatible-full。
    - 异构库 / 无库 engagement 只登记隔离（quarantine/assets_isolation.jsonl），
      不抽取、不猜测转换（计划 §6.2）。
  Transform：版本化映射（与 v1.0.0 同一映射规则，mapping v1）
    - http_evidence → evidence(evidence_type='http_pair')
      content_hash := sha256(method|url|status|request|response 规范拼接)
    - shot → evidence(evidence_type='screenshot')，content_hash := 源 sha256
    - evidence_id := 'EVD-<库slug>-<源表>-<源行id>'（确定性派生）
    - source_pk（即任务书的 source_ref）:= '<库绝对路径>#<源表>#<源行id>'
    - payload_json := 源行全字段 JSON（ensure_ascii=False, sort_keys=True），
      全字段保真，schema 未覆盖列经此承载、可逐字节复原。
  Load：只写隔离目标库 evidence 表 + entity_alias。
  Replay：确定性主键 + INSERT OR REPLACE，重复执行 no-op。

硬约束：assets.db 只读，绝不写入/升级/修复（计划 §6.2）；stdlib-only；
运行时刻只写执行日志，不进数据行（确定性 → 幂等）。
"""

import hashlib
import json
import os
import re
import sqlite3
import sys
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path

ETL_VERSION = "2.0.0"

MIG_DIR = Path(__file__).resolve().parent

SRC_ENGAGEMENTS_DIR = Path("/home/emo/.dsh/redteam/engagements")


def _target_db() -> Path:
    """目标库：RP_TARGET_DB 环境变量可覆盖（M1 全量批重建流程
    platform-fullbatch.db 用）；相对路径按 migrations/ 解析；默认隔离库。"""
    v = os.environ.get("RP_TARGET_DB")
    if not v:
        return MIG_DIR / "platform-dryrun.db"
    p = Path(v)
    return p if p.is_absolute() else MIG_DIR / p


TARGET_DB = _target_db()
SCHEMA_002_SQL = MIG_DIR / "002_tasks.sql"

EVID_RUN_LOG_JSONL = MIG_DIR / "etl_evidence_run_log.jsonl"
EVID_LAST_RUN_JSON = MIG_DIR / "etl_evidence_last_run.json"
ISOLATION_JSONL = MIG_DIR / "quarantine" / "assets_isolation.jsonl"
QUARANTINE_EVID_JSONL = MIG_DIR / "quarantine" / "evidence_isolation.jsonl"

SCHEMA_VERSION = 1
SOURCE_SYSTEM_ASSETS = "eri-assets"

# v2.0.0：全量迁移（无每表行数上限）；v1.0.0 为 SAMPLE_LIMIT=20 抽样。
MIGRATION_MODE = "full"

EXTRACT_TABLES = ("http_evidence", "shot")

CONTENT_TABLES = {
    "evidence": "evidence_id",
}


def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def open_ro(path: Path) -> sqlite3.Connection:
    """只读打开源库：URI mode=ro，绝不写源。"""
    if not path.exists():
        raise SystemExit(f"FATAL: 源库不存在：{path}")
    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    conn.row_factory = sqlite3.Row
    return conn


def open_target() -> sqlite3.Connection:
    conn = sqlite3.connect(str(TARGET_DB))
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA foreign_keys = ON;")
    return conn


def table_digest(conn: sqlite3.Connection, table: str, pk: str) -> str:
    h = hashlib.sha256()
    cur = conn.execute(f"SELECT * FROM {table} ORDER BY {pk}")  # noqa: S608（表名为内部常量）
    cols = [d[0] for d in cur.description]
    for row in cur:
        vals = ["" if v is None else repr(v) for v in row]
        h.update(("\x1f".join(cols) + "\x1e" + "\x1f".join(vals) + "\x1e").encode("utf-8"))
    return h.hexdigest()


def alias_key(entity_type: str, source_system: str, namespace: str, alias_value: str) -> str:
    raw = f"{entity_type}|{source_system}|{namespace}|{alias_value}"
    return hashlib.md5(raw.encode("utf-8")).hexdigest()  # noqa: S324（非安全用途，仅确定性键）


def register_alias(cur: sqlite3.Cursor, entity_type: str, entity_id: str,
                   source_system: str, namespace: str, alias_value: str,
                   stats: Counter) -> None:
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
# 库 slug：ascii 片段（小写、非 [a-z0-9_-] 归并为 -，截 32）+ md5(原名) 前 8 位。
# 纯中文目录名 ascii 片段为空，退化为 md5 前 8 位，保证确定性且全局唯一。
# ---------------------------------------------------------------------------
def db_slug(engagement_name: str) -> str:
    ascii_part = re.sub(r"[^a-z0-9_-]+", "-", engagement_name.lower()).strip("-_")[:32]
    h = hashlib.md5(engagement_name.encode("utf-8")).hexdigest()[:8]  # noqa: S324
    return f"{ascii_part}-{h}" if ascii_part else h


# ---------------------------------------------------------------------------
# 守卫 DDL
# ---------------------------------------------------------------------------
def apply_002_schema(conn: sqlite3.Connection) -> None:
    conn.executescript(SCHEMA_002_SQL.read_text(encoding="utf-8"))
    conn.execute("PRAGMA foreign_keys = ON;")


def ensure_evidence_payload_column(conn: sqlite3.Connection) -> None:
    """evidence 追加 payload_json 列（002_tasks.sql 权威记录；SQLite 无条件 DDL）。"""
    cols = {r[1] for r in conn.execute("PRAGMA table_info(evidence)").fetchall()}
    if "payload_json" not in cols:
        conn.execute("ALTER TABLE evidence ADD COLUMN payload_json TEXT;")


# ---------------------------------------------------------------------------
# 源盘点与选库
# ---------------------------------------------------------------------------
def inventory() -> tuple[list[dict], list[dict]]:
    """返回 (兼容库列表, 隔离登记列表)。兼容 = 表集指纹与多数派一致。"""
    entries = []
    for d in sorted(p for p in SRC_ENGAGEMENTS_DIR.iterdir() if p.is_dir()):
        db = d / "assets.db"
        if not db.exists():
            entries.append({"engagement": d.name, "db_path": None, "class": "no_db",
                            "reason": "目录无 assets.db（无证据库）",
                            "table_count": None, "fingerprint_md5": None,
                            "size_bytes": None})
            continue
        conn = open_ro(db)
        names = sorted(r[0] for r in conn.execute(
            "SELECT name FROM sqlite_master WHERE type='table'"
            " AND name NOT LIKE 'sqlite_%'"))
        conn.close()
        fp = hashlib.md5(",".join(names).encode("utf-8")).hexdigest()  # noqa: S324
        entries.append({"engagement": d.name, "db_path": str(db), "class": None,
                        "reason": None, "table_count": len(names),
                        "fingerprint_md5": fp, "size_bytes": db.stat().st_size})

    # 多数派指纹 = 兼容基线（数据驱动，不硬编码表集）
    fp_counter = Counter(e["fingerprint_md5"] for e in entries
                         if e["fingerprint_md5"])
    baseline_fp, baseline_n = fp_counter.most_common(1)[0]
    baseline_tables = None
    compat = []
    isolated = []
    for e in entries:
        if e["class"] == "no_db":
            e["disposition"] = "隔离登记（无证据库）；不抽取"
            isolated.append(e)
            continue
        if e["fingerprint_md5"] == baseline_fp:
            e["class"] = "compatible"
            e["reason"] = f"表集指纹与多数派（{baseline_n} 库）一致"
            compat.append(e)
        else:
            e["class"] = "heterogeneous"
            e["reason"] = (f"表集指纹偏离多数派（{baseline_n} 库同构基线）："
                           f"表数 {e['table_count']}，未知结构不猜测转换（计划 §6.2）")
            e["disposition"] = "隔离登记；不抽取"
            isolated.append(e)

    # 读取兼容库两表行数（用于选库与覆盖判定）
    for e in compat:
        conn = open_ro(Path(e["db_path"]))
        e["http_evidence_rows"] = conn.execute(
            "SELECT COUNT(*) FROM http_evidence").fetchone()[0]
        e["shot_rows"] = conn.execute("SELECT COUNT(*) FROM shot").fetchone()[0]
        conn.close()
        e["rows_total"] = e["http_evidence_rows"] + e["shot_rows"]

    # slug 唯一性断言（ID 派生安全性）
    slugs = [db_slug(e["engagement"]) for e in entries]
    if len(slugs) != len(set(slugs)):
        raise SystemExit("FATAL: 库 slug 派生冲突，需改用更长哈希")

    # 兼容基线表集（写进日志供 verify 复核）
    for e in entries:
        if e["fingerprint_md5"] == baseline_fp:
            e["baseline_tables"] = None  # 只在 baseline 记一份
    compat_entry = next(e for e in compat)
    conn = open_ro(Path(compat_entry["db_path"]))
    baseline_tables = sorted(r[0] for r in conn.execute(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'"))
    conn.close()
    return compat, isolated, baseline_tables


def select_all(compat: list[dict]) -> list[dict]:
    """v2.0.0 全量选择：全部兼容库（26 个），角色标记 compatible-full。"""
    if not compat:
        raise SystemExit("FATAL: 无兼容库")
    for d in compat:
        d["role"] = "compatible-full"
    return list(compat)


# ---------------------------------------------------------------------------
# 映射（Transform 规则：与 verify.py 复核逻辑保持同一函数式定义）
# ---------------------------------------------------------------------------
def row_payload(row: sqlite3.Row) -> str:
    return json.dumps({k: row[k] for k in row.keys()},
                      ensure_ascii=False, sort_keys=True)


def http_pair_content_hash(row: sqlite3.Row) -> str:
    canon = "\x1f".join([
        row["method"] or "", row["url"] or "",
        "" if row["status"] is None else str(row["status"]),
        row["request"] or "", row["response"] or "",
    ])
    return hashlib.sha256(canon.encode("utf-8")).hexdigest()  # noqa: S324


def map_http_evidence(slug: str, db_path: str, row: sqlite3.Row) -> dict:
    if not row["captured_at"]:
        # v2.0.0：单行映射失败 → 由调用方逐行隔离（quarantine），不再中止整个 ETL
        raise ValueError(
            f"http_evidence id={row['id']} 无 captured_at（时间戳断言失败）")
    source_ref = f"{db_path}#http_evidence#{row['id']}"
    return {
        "evidence_id": f"EVD-{slug}-http_evidence-{row['id']}",
        "evidence_type": "http_pair",
        "storage_ref": None,
        "content_hash": http_pair_content_hash(row),
        "captured_at": row["captured_at"],
        "captured_by": row["captured_by"],
        "task_ref": None,
        "permission_level": None,
        "sensitivity": None,
        "lifecycle": "RAW",
        "governance": "ACTIVE",
        "source_system": SOURCE_SYSTEM_ASSETS,
        "source_pk": source_ref,
        "created_at": row["captured_at"],
        "updated_at": row["captured_at"],
        "payload_json": row_payload(row),
    }


def map_shot(slug: str, db_path: str, row: sqlite3.Row) -> dict:
    captured = row["captured_at"] or row["created_at"]
    if not captured:
        # v2.0.0：单行映射失败 → 由调用方逐行隔离（quarantine），不再中止整个 ETL
        raise ValueError(
            f"shot id={row['id']} 无 captured_at/created_at（时间戳断言失败）")
    source_ref = f"{db_path}#shot#{row['id']}"
    return {
        "evidence_id": f"EVD-{slug}-shot-{row['id']}",
        "evidence_type": "screenshot",
        "storage_ref": row["file"],
        "content_hash": row["sha256"],
        "captured_at": captured,
        "captured_by": row["agent"] or row["found_by_agent"],
        "task_ref": None,
        "permission_level": None,
        "sensitivity": None,
        "lifecycle": "RAW",
        "governance": "ACTIVE",
        "source_system": SOURCE_SYSTEM_ASSETS,
        "source_pk": source_ref,
        "created_at": captured,
        "updated_at": captured,
        "payload_json": row_payload(row),
    }


EVIDENCE_COLS = ("evidence_id", "evidence_type", "storage_ref", "content_hash",
                 "captured_at", "captured_by", "task_ref", "permission_level",
                 "sensitivity", "lifecycle", "governance", "source_system",
                 "source_pk", "created_at", "updated_at", "schema_version",
                 "payload_json")


def etl_evidence(conn: sqlite3.Connection, migrated: list[dict], stats: Counter,
                 quarantine: list) -> dict:
    per_db = []
    cur = conn.cursor()
    for d in migrated:
        slug = db_slug(d["engagement"])
        src = open_ro(Path(d["db_path"]))
        tables_out = {}
        for t, mapper in (("http_evidence", map_http_evidence),
                          ("shot", map_shot)):
            total = src.execute(f"SELECT COUNT(*) FROM {t}").fetchone()[0]  # noqa: S608
            rows = src.execute(  # v2.0.0：全表，无 LIMIT
                f"SELECT * FROM {t} ORDER BY id").fetchall()  # noqa: S608
            extracted = 0
            quarantined = 0
            for row in rows:
                try:
                    rec = mapper(slug, d["db_path"], row)
                except Exception as e:  # noqa: BLE001
                    # v2.0.0：单行映射失败 → 逐行隔离（不迁移、不静默丢弃）
                    quarantine.append({
                        "source_system": SOURCE_SYSTEM_ASSETS,
                        "engagement": d["engagement"],
                        "source_table": t,
                        "source_pk": f"{d['db_path']}#{t}#{row['id']}",
                        "quarantine_reason": repr(e),
                        "disposition": "全量批坏行隔离（不迁移，不静默丢弃）",
                        "record": {k: row[k] for k in row.keys()},
                    })
                    stats["evidence_quarantined"] += 1
                    quarantined += 1
                    continue
                exists = cur.execute(
                    "SELECT evidence_id FROM evidence WHERE evidence_id=?",
                    (rec["evidence_id"],)).fetchone()
                cur.execute(
                    "INSERT OR REPLACE INTO evidence (" + ",".join(EVIDENCE_COLS)
                    + ") VALUES (" + ",".join(["?"] * len(EVIDENCE_COLS)) + ")",
                    tuple(rec.get(c, SCHEMA_VERSION) if c == "schema_version"
                          else rec.get(c) for c in EVIDENCE_COLS),
                )
                if exists is None:
                    stats["evidence_inserted"] += 1
                else:
                    stats["evidence_skipped_existing"] += 1
                register_alias(cur, "evidence", rec["evidence_id"],
                               SOURCE_SYSTEM_ASSETS,
                               f"assets.{t}.id", rec["source_pk"], stats)
                extracted += 1
            tables_out[t] = {"source_total": total, "extracted": extracted,
                             "quarantined": quarantined}
        src.close()
        per_db.append({
            "engagement": d["engagement"], "db_path": d["db_path"], "role": d["role"],
            "slug": slug, "size_bytes": d["size_bytes"], "tables": tables_out,
        })
    return {"migrated": per_db}


# ---------------------------------------------------------------------------
def main() -> int:
    started = now_iso()
    print(f"[etl_evidence] v{ETL_VERSION} started_at={started}")
    print(f"[etl_evidence] 目标（隔离）：{TARGET_DB}")

    compat, isolated, baseline_tables = inventory()

    conn = open_target()
    apply_002_schema(conn)
    ensure_evidence_payload_column(conn)

    stats: Counter = Counter()
    stats["alias_inserted"] = 0
    stats["alias_skipped_existing"] = 0

    before = {t: table_digest(conn, t, pk) for t, pk in CONTENT_TABLES.items()}

    migrated = select_all(compat)
    quarantine: list = []
    src_meta = etl_evidence(conn, migrated, stats, quarantine)
    conn.commit()

    after = {t: table_digest(conn, t, pk) for t, pk in CONTENT_TABLES.items()}
    changed = sorted(t for t in after if before[t] != after[t])
    row_counts = {
        "evidence": conn.execute("SELECT COUNT(*) FROM evidence").fetchone()[0],
        "entity_alias@evidence": conn.execute(
            "SELECT COUNT(*) FROM entity_alias WHERE entity_type='evidence'").fetchone()[0],
    }
    fk_violations = conn.execute("PRAGMA foreign_key_check;").fetchall()
    conn.close()

    # 隔离登记（每次运行确定性覆写；覆盖全部 28 个 engagement）
    ISOLATION_JSONL.parent.mkdir(parents=True, exist_ok=True)
    registry = []
    for e in isolated:
        registry.append({
            "engagement": e["engagement"], "class": e["class"],
            "db_path": e["db_path"], "reason": e["reason"],
            "disposition": e["disposition"],
            "table_count": e["table_count"], "fingerprint_md5": e["fingerprint_md5"],
        })
    with open(ISOLATION_JSONL, "w", encoding="utf-8") as f:
        for e in isolated:
            f.write(json.dumps(e, ensure_ascii=False, sort_keys=True) + "\n")

    # v2.0.0：坏行隔离登记（确定性覆写；无坏行时为空文件）
    QUARANTINE_EVID_JSONL.parent.mkdir(parents=True, exist_ok=True)
    with open(QUARANTINE_EVID_JSONL, "w", encoding="utf-8") as f:
        for entry in quarantine:
            f.write(json.dumps(entry, ensure_ascii=False, sort_keys=True) + "\n")

    result = {
        "etl_script": "etl_evidence.py",
        "etl_version": ETL_VERSION,
        "started_at": started,
        "finished_at": now_iso(),
        "target_db": str(TARGET_DB),
        "mode": MIGRATION_MODE,
        "source": {"engagements_dir": str(SRC_ENGAGEMENTS_DIR),
                   "engagement_count": len(compat) + len(isolated),
                   "compatible_count": len(compat),
                   "isolated_count": len(isolated),
                   "baseline_fingerprint_md5": baseline_tables is not None and
                       hashlib.md5(",".join(baseline_tables).encode("utf-8")).hexdigest(),
                   "baseline_tables": baseline_tables,
                   "sample_limit_per_table": None,
                   "mapping_version": "evidence-mapping-v1-full (EVD-<slug>-<table>-<id>)"},
        "migrated": src_meta["migrated"],
        "isolated": registry,
        "row_counts": row_counts,
        "tables_changed_vs_previous_state": changed,
        "content_digests": {f"evidence:{t}": d for t, d in after.items()},
        "fk_violations": len(fk_violations),
        "stats": dict(stats),
        "quarantine_file": str(QUARANTINE_EVID_JSONL),
        "quarantined_count": len(quarantine),
    }
    EVID_LAST_RUN_JSON.write_text(
        json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8")
    with open(EVID_RUN_LOG_JSONL, "a", encoding="utf-8") as f:
        f.write(json.dumps(result, ensure_ascii=False, sort_keys=True) + "\n")

    print(f"[etl_evidence] 兼容={len(compat)}（全量迁移） 隔离={len(isolated)}"
          f" 坏行隔离={len(quarantine)}")
    for d in src_meta["migrated"]:
        print(f"[etl_evidence] 迁移：{d['engagement']}（{d['role']}）"
              f" {json.dumps(d['tables'], ensure_ascii=False)}")
    print(f"[etl_evidence] 行数：{json.dumps(row_counts, ensure_ascii=False)}")
    print(f"[etl_evidence] 与上一状态相比变化的表：{changed or '（无，no-op）'}")
    print(f"[etl_evidence] FK 违例：{len(fk_violations)}")
    print(f"[etl_evidence] done at {result['finished_at']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
