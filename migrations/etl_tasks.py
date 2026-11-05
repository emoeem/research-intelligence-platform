#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
M1 第二批 ETL：ResearchTask 映射（隔离环境，不切流）
==================================================

版本：1.0.0（M1_IMPLEMENTATION_PLAN M1-D：脚本带版本号）

职责：
  Extract：/home/emo/.dsh/redteam/rtis/data/tasks/*.json 只读打开（35 个）。
  Transform：版本化映射（task.id→task_id、task.status→state 原值直通、
             task.status_history→JSON 保真序列化；状态语义不归一化）。
  Load：只写隔离目标库 platform-dryrun.db 的 research_task 表 + entity_alias。
  Replay：确定性主键 + INSERT OR REPLACE，重复执行 no-op。
  Rollback：DELETE FROM research_task WHERE task_id LIKE 'rtis-%' 即本映射回滚，
            或删除 platform-dryrun.db 整体回滚。

Schema：
  research_task 表由 002_tasks.sql 建表（幂等）；
  entity_alias 的 entity_type CHECK 域扩展（加入 'research_task'）由本文件
  ensure_entity_alias_domain() 守卫执行（SQLite 无条件 DDL）。

硬约束：源 JSON 只读；绝不写 knowledge.db / catalog.db / assets.db；
stdlib-only；运行时刻只写执行日志，不进数据行（确定性 → 幂等）。
"""

import hashlib
import json
import sqlite3
import sys
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path

ETL_VERSION = "1.0.0"

MIG_DIR = Path(__file__).resolve().parent

SRC_TASKS_DIR = Path("/home/emo/.dsh/redteam/rtis/data/tasks")
TARGET_DB = MIG_DIR / "platform-dryrun.db"
SCHEMA_002_SQL = MIG_DIR / "002_tasks.sql"

TASKS_RUN_LOG_JSONL = MIG_DIR / "etl_tasks_run_log.jsonl"
TASKS_LAST_RUN_JSON = MIG_DIR / "etl_tasks_last_run.json"

SCHEMA_VERSION = 1
SOURCE_SYSTEM_RTIS_TASKS = "rtis-tasks"
ALIAS_NAMESPACE_TASK_ID = "rtis.data.tasks.task.id"

# content digest 覆盖的表（本脚本职责域）
CONTENT_TABLES = {
    "research_task": "task_id",
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
    for f in files:
        try:
            with open(f, encoding="utf-8") as fh:  # 只读打开源 JSON
                doc = json.load(fh)
        except Exception as e:  # noqa: BLE001
            parse_errors.append({"file": f.name, "error": repr(e)})
            stats["task_parse_error"] += 1
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
        conn.execute(
            "INSERT OR REPLACE INTO research_task ("
            " task_id, title, objective, target, task_type, state, status_history,"
            " source_file, source_system, created_at, updated_at, schema_version"
            ") VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
            (task_id, t.get("title"), t["objective"], t.get("target"),
             t.get("task_type"), state, status_history, f.name,
             SOURCE_SYSTEM_RTIS_TASKS, t["created_at"], t["updated_at"],
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
    }


# ---------------------------------------------------------------------------
def main() -> int:
    started = now_iso()
    print(f"[etl_tasks] v{ETL_VERSION} started_at={started}")
    print(f"[etl_tasks] 目标（隔离）：{TARGET_DB}")

    conn = open_target()
    apply_002_schema(conn)
    ensure_entity_alias_domain(conn)

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
        "row_counts": row_counts,
        "tables_changed_vs_previous_state": changed,
        "content_digests": {f"research_task:{t}": d for t, d in after.items()},
        "fk_violations": len(fk_violations),
        "stats": dict(stats),
        **src_meta,
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
