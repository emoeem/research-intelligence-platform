#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
M1 第二批 ETL：ResearchTask 映射（隔离环境，不切流）
==================================================

版本：2.0.0（M1_IMPLEMENTATION_PLAN M1-D：脚本带版本号）
  v2.0.0 变更（M1 全量批）：
    - research_task 大 JSON 段映射：源 task JSON 顶层 authorization /
      environment / knowledge 三个段逐字段保真序列化入库（新增
      authorization_json / environment_json / knowledge_json 三列，守卫 DDL，
      权威记录 004_fullbatch.sql + schema_version version=4）。
    - 坏行隔离：JSON 解析失败的文件、大 JSON 段序列化失败的行，写入
      quarantine/research_task_isolation.jsonl（确定性覆写），不迁移、
      不静默丢弃。
    - 目标库可配置：RP_TARGET_DB 环境变量（全量批重建流程
      platform-fullbatch.db 用），默认仍为隔离库 platform-dryrun.db。
  v1.0.0（第二批）：task.id→task_id、task.status→state 原值直通、
    status_history JSON 保真序列化；状态语义不归一化。

职责：
  Extract：/home/emo/.dsh/redteam/rtis/data/tasks/*.json（或 sources-snapshot/
           tasks/ 快照，存在时优先，M8 §6.3）只读打开。
  Transform：版本化映射（核心字段 + 三大 JSON 段保真序列化）。
  Load：只写目标库（默认 platform-dryrun.db）的 research_task 表 + entity_alias。
  Replay：确定性主键 + INSERT OR REPLACE，重复执行 no-op。
  Rollback：DELETE FROM research_task WHERE task_id LIKE 'rtis-%' 即本映射回滚，
            或删除目标库整体回滚。

Schema：
  research_task 表由 002_tasks.sql 建表（幂等）；三大 JSON 段三列由本文件
  ensure_task_json_columns() 守卫执行（004_fullbatch.sql 权威记录）；
  entity_alias 的 entity_type CHECK 域扩展（加入 'research_task'）由本文件
  ensure_entity_alias_domain() 守卫执行（SQLite 无条件 DDL）。

硬约束：源 JSON 只读；绝不写 knowledge.db / catalog.db / assets.db；
stdlib-only；运行时刻只写执行日志，不进数据行（确定性 → 幂等）。
"""

import hashlib
import json
import os
import sqlite3
import sys
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path

ETL_VERSION = "2.0.0"

MIG_DIR = Path(__file__).resolve().parent

# 源快照锁定（M8）：快照目录存在时对冻结副本执行（计划 §6.3）。
_SNAP = Path(__file__).resolve().parent / "sources-snapshot"
SRC_TASKS_DIR = (_SNAP / "tasks") if (_SNAP / "tasks").is_dir() else Path("/home/emo/.dsh/redteam/rtis/data/tasks")


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
SCHEMA_004_SQL = MIG_DIR / "004_fullbatch.sql"

TASKS_RUN_LOG_JSONL = MIG_DIR / "etl_tasks_run_log.jsonl"
TASKS_LAST_RUN_JSON = MIG_DIR / "etl_tasks_last_run.json"
QUARANTINE_TASKS_JSONL = MIG_DIR / "quarantine" / "research_task_isolation.jsonl"

SCHEMA_VERSION = 1
SOURCE_SYSTEM_RTIS_TASKS = "rtis-tasks"
ALIAS_NAMESPACE_TASK_ID = "rtis.data.tasks.task.id"

# content digest 覆盖的表（本脚本职责域）
CONTENT_TABLES = {
    "research_task": "task_id",
}

# ---------------------------------------------------------------------------
# v2.0.0 大 JSON 段映射：源 task JSON 顶层段 → research_task 追加列。
# 键存在且值非 null → json.dumps(ensure_ascii=False, sort_keys=True) 保真；
# 键缺失或 null → NULL（忠实于源）；序列化失败 → 整行隔离。
# ---------------------------------------------------------------------------
TASK_JSON_SEGMENTS = {
    "authorization": "authorization_json",
    "environment": "environment_json",
    "knowledge": "knowledge_json",
}


def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def open_target() -> sqlite3.Connection:
    conn = sqlite3.connect(str(TARGET_DB))
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA foreign_keys = ON;")
    return conn


def table_digest(conn: sqlite3.Connection, table: str, pk: str,
                 where: str = "", params: tuple = ()) -> str:
    """逻辑内容摘要：全行按稳定主键排序后规范化哈希。"""
    sql = f"SELECT * FROM {table}{(' WHERE ' + where) if where else ''} ORDER BY {pk}"
    h = hashlib.sha256()
    cur = conn.execute(sql, params)  # noqa: S608（表名为内部常量）
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
# 守卫 DDL：SQLite 无条件 DDL，只能脚本侧判定后执行
# ---------------------------------------------------------------------------
def apply_002_schema(conn: sqlite3.Connection) -> None:
    """幂等应用 002_tasks.sql（CREATE TABLE IF NOT EXISTS + INSERT OR IGNORE）。"""
    conn.executescript(SCHEMA_002_SQL.read_text(encoding="utf-8"))
    conn.execute("PRAGMA foreign_keys = ON;")


def apply_004_schema(conn: sqlite3.Connection) -> None:
    """幂等应用 004_fullbatch.sql（schema_version version=4 登记，v2.0.0）。"""
    conn.executescript(SCHEMA_004_SQL.read_text(encoding="utf-8"))
    conn.execute("PRAGMA foreign_keys = ON;")


def ensure_task_json_columns(conn: sqlite3.Connection) -> list:
    """research_task 追加大 JSON 段三列（幂等守卫；权威记录 004_fullbatch.sql）。

    SQLite 无条件 DDL（无 ADD COLUMN IF NOT EXISTS），只能脚本侧判定后执行。
    """
    added = []
    cols = {r[1] for r in conn.execute("PRAGMA table_info(research_task)")}
    for col in TASK_JSON_SEGMENTS.values():
        if col not in cols:
            conn.execute(f"ALTER TABLE research_task ADD COLUMN {col} TEXT")  # noqa: S608（内部常量列名）
            added.append(col)
    return added


def ensure_entity_alias_domain(conn: sqlite3.Connection) -> None:
    """entity_alias.entity_type CHECK 域扩展：加入 'research_task'。

    幂等守卫：已扩展（sqlite_master 的建表 SQL 含 'research_task'）则跳过。
    重建方式：新表 → 全量拷贝 → 换名（entity_alias 无被引用 FK，可安全重建）。
    """
    sql = conn.execute(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name='entity_alias'"
    ).fetchone()[0]
    if "research_task" in sql:
        return
    conn.executescript("""
      DROP TABLE IF EXISTS entity_alias_v2;
      CREATE TABLE entity_alias_v2 (
        id            TEXT PRIMARY KEY,
        entity_type   TEXT NOT NULL CHECK (entity_type IN
                        ('resource','resource_source','knowledge','evidence',
                         'tool','tool_version','skill','review','research_task')),
        entity_id     TEXT NOT NULL,
        source_system TEXT NOT NULL,
        namespace     TEXT NOT NULL,
        alias_value   TEXT NOT NULL,
        valid_from    TEXT,
        valid_until   TEXT,
        created_at    TEXT NOT NULL,
        updated_at    TEXT NOT NULL,
        schema_version INTEGER NOT NULL DEFAULT 1,
        UNIQUE (source_system, namespace, alias_value, entity_type)
      );
      INSERT INTO entity_alias_v2
        (id, entity_type, entity_id, source_system, namespace, alias_value,
         valid_from, valid_until, created_at, updated_at, schema_version)
        SELECT id, entity_type, entity_id, source_system, namespace, alias_value,
               valid_from, valid_until, created_at, updated_at, schema_version
          FROM entity_alias;
      DROP TABLE entity_alias;
      ALTER TABLE entity_alias_v2 RENAME TO entity_alias;
      CREATE INDEX IF NOT EXISTS ix_alias_entity ON entity_alias(entity_type, entity_id);
    """)
    conn.execute("PRAGMA foreign_keys = ON;")


# ---------------------------------------------------------------------------
# Extract + Transform + Load
# ---------------------------------------------------------------------------
REQUIRED_KEYS = ("id", "objective", "created_at", "updated_at", "status_history")


def etl_tasks(conn: sqlite3.Connection, stats: Counter) -> dict:
    files = sorted(SRC_TASKS_DIR.glob("*.json"))
    if not files:
        raise SystemExit(f"FATAL: 源目录无 task JSON：{SRC_TASKS_DIR}")
    state_dist: Counter = Counter()
    parse_errors = []
    quarantine = []  # 坏行隔离条目（v2.0.0：不静默丢弃）
    big_json_stats: Counter = Counter()
    for f in files:
        try:
            with open(f, encoding="utf-8") as fh:  # 只读打开源 JSON
                doc = json.load(fh)
        except Exception as e:  # noqa: BLE001
            parse_errors.append({"file": f.name, "error": repr(e)})
            stats["task_parse_error"] += 1
            quarantine.append({
                "source_system": SOURCE_SYSTEM_RTIS_TASKS,
                "source_table": "rtis/data/tasks/*.json",
                "source_file": f.name,
                "source_pk": None,
                "quarantine_reason": f"源 JSON 解析失败：{e!r}",
                "disposition": "全量批坏行隔离（不迁移，不静默丢弃）",
            })
            stats["task_quarantined"] += 1
            continue
        t = doc.get("task") or {}
        missing = [k for k in REQUIRED_KEYS if not t.get(k)]
        if missing:
            raise SystemExit(f"FATAL: {f.name} 缺必需字段：{missing}")
        task_id = t["id"]
        state = t.get("state") or t.get("status") or ""
        if not state:
            raise SystemExit(f"FATAL: {f.name} 无 state/status 字段")
        status_history = json.dumps(t["status_history"], ensure_ascii=False,
                                    sort_keys=True)
        # v2.0.0：三大 JSON 段保真序列化（键缺失/null → NULL；失败 → 整行隔离）
        seg_cols: dict = {}
        bad_seg = None
        for seg, col in TASK_JSON_SEGMENTS.items():
            v = doc.get(seg)
            if v is None:
                seg_cols[col] = None
                continue
            try:
                seg_cols[col] = json.dumps(v, ensure_ascii=False, sort_keys=True)
            except Exception as e:  # noqa: BLE001
                bad_seg = (seg, repr(e))
                break
            big_json_stats[f"{seg}_present"] += 1
            big_json_stats[f"{seg}_serialized_bytes"] += len(seg_cols[col])
        if bad_seg is not None:
            quarantine.append({
                "source_system": SOURCE_SYSTEM_RTIS_TASKS,
                "source_table": "rtis/data/tasks/*.json",
                "source_file": f.name,
                "source_pk": task_id,
                "quarantine_reason": f"大 JSON 段 {bad_seg[0]} 序列化失败：{bad_seg[1]}",
                "disposition": "全量批坏行隔离（不迁移，不静默丢弃）",
            })
            stats["task_quarantined"] += 1
            continue
        conn.execute(
            "INSERT OR REPLACE INTO research_task ("
            " task_id, title, objective, target, task_type, state, status_history,"
            " authorization_json, environment_json, knowledge_json,"
            " source_file, source_system, created_at, updated_at, schema_version"
            ") VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            (task_id, t.get("title"), t["objective"], t.get("target"),
             t.get("task_type"), state, status_history,
             seg_cols["authorization_json"], seg_cols["environment_json"],
             seg_cols["knowledge_json"],
             f.name, SOURCE_SYSTEM_RTIS_TASKS, t["created_at"], t["updated_at"],
             SCHEMA_VERSION),
        )
        stats["research_task"] += 1
        state_dist[state] += 1
        register_alias(conn.cursor(), "research_task", task_id,
                       SOURCE_SYSTEM_RTIS_TASKS, ALIAS_NAMESPACE_TASK_ID,
                       task_id, stats)
    return {
        "source_task_json_count": len(files),
        "state_distribution": dict(sorted(state_dist.items())),
        "parse_errors": parse_errors,
        "quarantine_entries": quarantine,
        "quarantine_count": len(quarantine),
        "big_json_segments": dict(sorted(big_json_stats.items())),
    }


# ---------------------------------------------------------------------------
def main() -> int:
    started = now_iso()
    print(f"[etl_tasks] v{ETL_VERSION} started_at={started}")
    print(f"[etl_tasks] 目标（隔离）：{TARGET_DB}")

    conn = open_target()
    apply_002_schema(conn)
    apply_004_schema(conn)
    ensure_entity_alias_domain(conn)
    added_cols = ensure_task_json_columns(conn)
    if added_cols:
        conn.commit()
        print(f"[etl_tasks] 守卫 DDL：research_task 追加列={added_cols}")

    stats: Counter = Counter()
    stats["alias_inserted"] = 0
    stats["alias_skipped_existing"] = 0

    before = {t: table_digest(conn, t, pk) for t, pk in CONTENT_TABLES.items()}

    src_meta = etl_tasks(conn, stats)
    conn.commit()

    after = {t: table_digest(conn, t, pk) for t, pk in CONTENT_TABLES.items()}
    changed = sorted(t for t in after if before[t] != after[t])
    row_counts = {t: conn.execute(f"SELECT COUNT(*) FROM {t}").fetchone()[0]
                  for t in CONTENT_TABLES}  # noqa: S608
    row_counts["entity_alias@research_task"] = conn.execute(
        "SELECT COUNT(*) FROM entity_alias WHERE entity_type='research_task'"
    ).fetchone()[0]
    fk_violations = conn.execute("PRAGMA foreign_key_check;").fetchall()
    conn.close()

    # 坏行隔离登记（v2.0.0：确定性覆写；不静默丢弃）
    QUARANTINE_TASKS_JSONL.parent.mkdir(parents=True, exist_ok=True)
    with open(QUARANTINE_TASKS_JSONL, "w", encoding="utf-8") as f:
        for entry in src_meta["quarantine_entries"]:
            f.write(json.dumps(entry, ensure_ascii=False, sort_keys=True) + "\n")

    result = {
        "etl_script": "etl_tasks.py",
        "etl_version": ETL_VERSION,
        "started_at": started,
        "finished_at": now_iso(),
        "target_db": str(TARGET_DB),
        "source": {"dir": str(SRC_TASKS_DIR),
                   "sha256_dir_listing": hashlib.sha256(
                       "\n".join(sorted(p.name for p in SRC_TASKS_DIR.glob("*.json")))
                       .encode("utf-8")).hexdigest()},
        "guard_ddl": {"research_task_columns_added": added_cols},
        "row_counts": row_counts,
        "tables_changed_vs_previous_state": changed,
        "content_digests": {f"research_task:{t}": d for t, d in after.items()},
        "fk_violations": len(fk_violations),
        "stats": dict(stats),
        "quarantine_file": str(QUARANTINE_TASKS_JSONL),
        "quarantine_count": src_meta["quarantine_count"],
        "big_json_segments": src_meta["big_json_segments"],
        **{k: v for k, v in src_meta.items() if k != "quarantine_entries"},
    }
    TASKS_LAST_RUN_JSON.write_text(
        json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8")
    with open(TASKS_RUN_LOG_JSONL, "a", encoding="utf-8") as f:
        f.write(json.dumps(result, ensure_ascii=False, sort_keys=True) + "\n")

    print(f"[etl_tasks] 行数：{json.dumps(row_counts, ensure_ascii=False)}")
    print(f"[etl_tasks] 状态分布：{json.dumps(src_meta['state_distribution'], ensure_ascii=False)}")
    print(f"[etl_tasks] 与上一状态相比变化的表：{changed or '（无，no-op）'}")
    print(f"[etl_tasks] FK 违例：{len(fk_violations)}")
    print(f"[etl_tasks] done at {result['finished_at']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
