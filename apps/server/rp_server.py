#!/usr/bin/env python3
"""RP Server: RIP M7 统一工作台（只读 + Token 鉴权）。

单文件 http.server 实现，Python stdlib-only：
- 静态页（apps/server/static/）
- 9 个只读 JSON API（sqlite3 URI mode=ro 直查生产库 platform.db）：
  M7 四端点 /api/stats /api/resources /api/reviews /api/tasks；
  Phase 5 增量五端点 /api/overview /api/knowledge /api/tools /api/runs
  /api/runtime（对应 ROADMAP P5-4 的本阶段落地形态，api-server crate 化
  顺延，见 docs/phase5/PHASE5-PLAN.md 偏差登记 1）
- 无任何写端点，默认绑定 127.0.0.1（容器内通过 RP_HOST=0.0.0.0 覆盖）
- Bearer Token 鉴权：配置 RP_AUTH_TOKEN 或 RP_AUTH_TOKEN_FILE 后，
  所有 /api/* 请求必须携带有效令牌（constant-time 比较 + 失败节流）；
  未配置任何令牌时为本地开发模式（鉴权关闭，启动时显著告警）。
- RP_AUX_DB 只读 ATTACH 通道：Phase 1-4 数据面（tool_run/artifact/
  runtime_node 等）尚未并入主库时，经只读 ATTACH 挂载开发库副本读取；
  缺库缺表按 sqlite_master 守卫返回真实空态说明，不造假数据。
- P6-8 插件面增量（ROADMAP P6-8，计划 §10.4 E）：
  * 读端点 /api/plugins 与 /api/plugins/<id>（三级加载 ?level=1|2|3）：
    打开 RIP_PLUGIN_DB（插件数据面开发副本，009/010 迁移）只读直查，
    缺库守卫降级为真实空态说明，不造假数据；
  * 写端点 /api/plugins/*（enable/disable/uninstall/install/upgrade/ingest）：
    一律经 subprocess 调 rp-plugin CLI（plugin-runtime 状态机 + audit-engine
    唯一出口），本文件不新增任何直连 SQL 写（写面零 SQL，selftest 有静态断言）；
  * 写端点强制 Bearer Token（与 GET 同一 require_auth），鉴权关闭时拒绝写。

环境变量：
- RP_PORT            监听端口，默认 7800
- RP_HOST            监听地址，默认 127.0.0.1
- RP_DB_PATH         数据库路径，默认仓库内 migrations/platform.db
- RP_AUX_DB          Phase 1-4 数据面只读库（如 migrations/dev-b5.db），缺省不挂载
- RP_AUTH_TOKEN      明文令牌（优先）
- RP_AUTH_TOKEN_FILE 令牌文件路径（默认仓库根 .rp-workbench-token，权限应为 600）
- RIP_PLUGIN_DB       插件数据面开发副本（默认 migrations/dev-p6.db）
- RP_PLUGIN_BIN       rp-plugin 可执行（默认 cargo）
- RP_PLUGIN_BIN_ARGS  rp-plugin 前置参数（默认 run --offline -q -p plugin-runtime
                      --bin rp-plugin --；预编译后可换 target/debug/rp-plugin）
"""

from __future__ import annotations

import hmac
import json
import os
import posixpath
import sqlite3
import sys
import time
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, unquote, urlparse

APP_DIR = os.path.dirname(os.path.abspath(__file__))
STATIC_DIR = os.path.join(APP_DIR, "static")
REPO_ROOT = os.path.dirname(os.path.dirname(APP_DIR))
DEFAULT_DB = os.path.join(REPO_ROOT, "migrations", "platform.db")
DEFAULT_TOKEN_FILE = os.path.join(REPO_ROOT, ".rp-workbench-token")

HOST = os.environ.get("RP_HOST", "127.0.0.1")
PORT = int(os.environ.get("RP_PORT", "7800"))
DB_PATH = os.environ.get("RP_DB_PATH", DEFAULT_DB)
AUX_DB_PATH = (os.environ.get("RP_AUX_DB") or "").strip() or None
PLUGIN_DB_PATH = (os.environ.get("RIP_PLUGIN_DB") or "").strip() \
    or os.path.join(REPO_ROOT, "migrations", "dev-p6.db")
PLUGIN_BIN = (os.environ.get("RP_PLUGIN_BIN") or "cargo").strip()
# RP_PLUGIN_BIN_ARGS 语义：显式空串 = 无前置参数（直用预编译二进制时）；
# 未设置 = cargo run 默认前置。
_raw_bin_args = os.environ.get("RP_PLUGIN_BIN_ARGS")
PLUGIN_BIN_ARGS = (_raw_bin_args if _raw_bin_args is not None
                   else "run --offline -q -p plugin-runtime --bin rp-plugin --").split()
AUTH_FAIL_DELAY = 0.3


def _load_auth_token() -> str | None:
    """令牌来源优先级：RP_AUTH_TOKEN > RP_AUTH_TOKEN_FILE > 默认令牌文件。"""
    env_token = os.environ.get("RP_AUTH_TOKEN", "").strip()
    if env_token:
        return env_token
    token_file = os.environ.get("RP_AUTH_TOKEN_FILE", DEFAULT_TOKEN_FILE)
    try:
        if os.path.isfile(token_file):
            mode = os.stat(token_file).st_mode & 0o777
            if mode & 0o077:
                print(
                    f"[rp-server] 警告：令牌文件权限 {oct(mode)} 过宽，应为 600：{token_file}",
                    file=sys.stderr,
                )
            with open(token_file, encoding="utf-8") as fh:
                token = fh.read().strip()
            if token:
                return token
    except OSError as exc:
        print(f"[rp-server] 令牌文件读取失败：{exc}", file=sys.stderr)
    return None


AUTH_TOKEN = _load_auth_token()
AUTH_ENABLED = AUTH_TOKEN is not None

MAX_LIMIT = 200
DEFAULT_LIMIT = 50
REVIEWS_PER_QUEUE_CAP = 100

CONTENT_TYPES = {
    ".html": "text/html; charset=utf-8",
    ".css": "text/css; charset=utf-8",
    ".js": "application/javascript; charset=utf-8",
    ".svg": "image/svg+xml",
    ".png": "image/png",
    ".ico": "image/x-icon",
    ".json": "application/json; charset=utf-8",
}


class ApiError(Exception):
    def __init__(self, status: int, code: str, message: str):
        super().__init__(message)
        self.status = status
        self.code = code
        self.message = message


def connect_ro() -> sqlite3.Connection:
    """打开只读连接：URI mode=ro + query_only 双保险，不存在则 503。"""
    if not os.path.isfile(DB_PATH):
        raise ApiError(503, "db_missing", f"数据库文件不存在：{DB_PATH}")
    try:
        conn = sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True)
    except sqlite3.Error as exc:
        raise ApiError(503, "db_unavailable", f"数据库打开失败：{exc}") from exc
    conn.execute("PRAGMA query_only = ON")
    conn.row_factory = sqlite3.Row
    attach_aux(conn)
    return conn


def attach_aux(conn: sqlite3.Connection) -> None:
    """尽力挂载 RP_AUX_DB 只读库（URI mode=ro，主连接已开 URI）。

    失败只告警不抛错：aux 缺失只影响 Phase 1-4 五端点的数据面，绝不能
    连累既有四端点（query_only 对整个连接生效，挂载的库同样只读）。
    """
    if not AUX_DB_PATH:
        return
    if not os.path.isfile(AUX_DB_PATH):
        print(f"[rp-server] 警告：RP_AUX_DB 文件不存在，aux 端点返回缺失说明：{AUX_DB_PATH}", file=sys.stderr)
        return
    try:
        conn.execute("ATTACH DATABASE ? AS aux", (f"file:{AUX_DB_PATH}?mode=ro",))
    except sqlite3.Error as exc:
        print(f"[rp-server] 警告：RP_AUX_DB 挂载失败，aux 端点返回缺失说明：{exc}", file=sys.stderr)


def query_all(conn: sqlite3.Connection, sql: str, params: tuple = ()) -> list:
    return [dict(row) for row in conn.execute(sql, params).fetchall()]


def query_one(conn: sqlite3.Connection, sql: str, params: tuple = ()):
    row = conn.execute(sql, params).fetchone()
    return dict(row) if row is not None else None


def like_escape(keyword: str) -> str:
    """转义 LIKE 通配符，保证关键词按字面匹配。"""
    return keyword.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_")


def clean_str(params: dict, name: str) -> str | None:
    value = (params.get(name, [""])[0] or "").strip()
    return value or None


def parse_limit(params: dict) -> int:
    raw = params.get("limit", [str(DEFAULT_LIMIT)])[0]
    try:
        value = int(raw)
    except (TypeError, ValueError):
        raise ApiError(400, "bad_limit", f"limit 必须是整数，收到：{raw!r}")
    if value < 1 or value > MAX_LIMIT:
        raise ApiError(400, "bad_limit", f"limit 需在 1..{MAX_LIMIT} 之间，收到：{value}")
    return value


# ---------------------------------------------------------------------------
# Phase 1-4 数据面（RP_AUX_DB 通道）守卫
# ---------------------------------------------------------------------------


def attached_schemas(conn: sqlite3.Connection) -> set[str]:
    return {row[1] for row in conn.execute("PRAGMA database_list")}


def table_exists(conn: sqlite3.Connection, schema: str, table: str) -> bool:
    row = conn.execute(
        f"SELECT 1 FROM {schema}.sqlite_master WHERE type='table' AND name=?", (table,)
    ).fetchone()
    return row is not None


def phase14_source(conn: sqlite3.Connection, table: str) -> str | None:
    """返回某张 Phase 1-4 表应读取的 schema：主库已建表一律主库优先（主库是
    全量注册面，如 tool=20）；aux 只承载主库未建的表（tool_run/artifact/
    runtime_node 等尚在 Phase 2-4 开发库副本中的面）；都无则 None。"""
    if table_exists(conn, "main", table):
        return "main"
    if "aux" in attached_schemas(conn) and table_exists(conn, "aux", table):
        return "aux"
    return None


def phase14_unavailable(conn: sqlite3.Connection, table: str) -> dict:
    """数据面不可读时的真实空态说明（不是空数据集）：按缺什么说什么。"""
    schemas = attached_schemas(conn)
    if AUX_DB_PATH is None:
        reason = "aux_db_not_configured"
        message = f"未配置 RP_AUX_DB，且主库无 {table} 表：Phase 1-4 数据面暂不可读（真实空态说明，非空数据）"
    elif not os.path.isfile(AUX_DB_PATH):
        reason = "aux_db_missing"
        message = f"RP_AUX_DB 指向的文件不存在：{AUX_DB_PATH}，{table} 表不可读"
    elif "aux" not in schemas:
        reason = "aux_db_unavailable"
        message = f"RP_AUX_DB 挂载失败（详见服务端日志）：{AUX_DB_PATH}，{table} 表不可读"
    else:
        reason = "table_missing"
        message = f"aux 库（{os.path.basename(AUX_DB_PATH)}）中不存在 {table} 表：对应 Phase 迁移未应用"
    return {"available": False, "reason": reason, "table": table, "message": message, "total": 0, "items": []}


def parse_json_or_none(text: str | None):
    """JSON 文本列安全解析：解析失败保留原文（不静默丢字段）。"""
    if text is None or text == "":
        return None
    try:
        return json.loads(text)
    except (TypeError, ValueError):
        return {"_raw": text}


def check_enum(params: dict, name: str, allowed: tuple) -> str | None:
    value = clean_str(params, name)
    if value is not None and value not in allowed:
        raise ApiError(400, f"bad_{name}", f"{name} 仅支持 {'/'.join(allowed)}，收到：{value!r}")
    return value


def heartbeat_age_seconds(last_heartbeat_at: str | None) -> float | None:
    """ISO8601 心跳时刻到现在的时间；解析失败返回 None（不猜）。"""
    if not last_heartbeat_at:
        return None
    try:
        ts = datetime.fromisoformat(last_heartbeat_at.replace("Z", "+00:00"))
    except ValueError:
        return None
    if ts.tzinfo is None:
        ts = ts.replace(tzinfo=timezone.utc)
    return round((datetime.now(timezone.utc) - ts).total_seconds(), 3)


# ---------------------------------------------------------------------------
# API handlers
# ---------------------------------------------------------------------------


def handle_stats(conn: sqlite3.Connection) -> dict:
    grade_rows = query_all(
        conn, "SELECT quality_grade AS k, COUNT(*) AS n FROM resource GROUP BY quality_grade ORDER BY k"
    )
    knowledge_lifecycle = query_all(
        conn, "SELECT lifecycle AS k, COUNT(*) AS n FROM knowledge GROUP BY lifecycle ORDER BY n DESC"
    )
    knowledge_verified_rows = query_all(
        conn,
        "SELECT CASE WHEN verified = 1 THEN 'verified' WHEN verified = 0 THEN 'unverified'"
        " ELSE 'unknown' END AS k, COUNT(*) AS n FROM knowledge GROUP BY k",
    )
    review_pairs = query_all(
        conn,
        "SELECT target_type AS k, decision AS d, COUNT(*) AS n FROM review GROUP BY target_type, decision",
    )
    queues: dict[str, dict] = {}
    for row in review_pairs:
        bucket = queues.setdefault(row["k"], {"total": 0, "by_decision": {}})
        bucket["total"] += row["n"]
        bucket["by_decision"][row["d"]] = row["n"]
    task_states = query_all(
        conn, "SELECT state AS k, COUNT(*) AS n FROM research_task GROUP BY state ORDER BY n DESC"
    )
    task_types = query_all(
        conn, "SELECT task_type AS k, COUNT(*) AS n FROM research_task GROUP BY task_type ORDER BY n DESC"
    )
    evidence_types = query_all(
        conn, "SELECT evidence_type AS k, COUNT(*) AS n FROM evidence GROUP BY evidence_type"
    )
    recent_reviews = query_all(
        conn,
        "SELECT review_id, target_type, target_id, decision, confidence, reason, reviewer, reviewed_at"
        " FROM review ORDER BY reviewed_at DESC, review_id DESC LIMIT 8",
    )
    to_map = lambda rows: {r["k"]: r["n"] for r in rows}
    return {
        "database": {
            "file": os.path.basename(DB_PATH),
            "mode": "readonly",
            "schema_version": query_all(
                conn, "SELECT version, name, applied_at FROM schema_version ORDER BY version"
            ),
        },
        "resources": {
            "total": conn.execute("SELECT COUNT(*) FROM resource").fetchone()[0],
            "by_grade": to_map(grade_rows),
            "by_lifecycle": to_map(
                query_all(conn, "SELECT lifecycle AS k, COUNT(*) AS n FROM resource GROUP BY lifecycle")
            ),
        },
        "knowledge": {
            "total": conn.execute("SELECT COUNT(*) FROM knowledge").fetchone()[0],
            "by_lifecycle": to_map(knowledge_lifecycle),
            "by_verified": to_map(knowledge_verified_rows),
        },
        "reviews": {"total": conn.execute("SELECT COUNT(*) FROM review").fetchone()[0], "by_queue": queues},
        "tasks": {
            "total": conn.execute("SELECT COUNT(*) FROM research_task").fetchone()[0],
            "by_state": to_map(task_states),
            "by_type": to_map(task_types),
        },
        "evidence": {
            "total": conn.execute("SELECT COUNT(*) FROM evidence").fetchone()[0],
            "by_type": to_map(evidence_types),
        },
        "entity_alias": {"total": conn.execute("SELECT COUNT(*) FROM entity_alias").fetchone()[0]},
        "tools": {"total": conn.execute("SELECT COUNT(*) FROM tool").fetchone()[0]},
        "skills": {"total": conn.execute("SELECT COUNT(*) FROM skill").fetchone()[0]},
        "recent_reviews": recent_reviews,
    }


RESOURCE_COLUMNS = (
    "resource_id, identity, canonical_url, resource_type, name, description, owner, repo_name,"
    " lifecycle, source_status, quality_score, quality_final_score, quality_grade,"
    " quality_adoption_blocked, quality_reasons, first_seen, last_seen, source_system"
)


def handle_resources(conn: sqlite3.Connection, params: dict) -> dict:
    keyword = clean_str(params, "keyword")
    grade = clean_str(params, "grade")
    if grade is not None and grade not in ("A", "B", "C", "D"):
        raise ApiError(400, "bad_grade", f"grade 仅支持 A/B/C/D，收到：{grade!r}")
    limit = parse_limit(params)

    where, args = [], []
    if keyword:
        needle = f"%{like_escape(keyword)}%"
        like_fields = ["name", "repo_name", "identity", "description", "canonical_url", "owner"]
        clauses = " OR ".join(f"{f} LIKE ? ESCAPE '\\'" for f in like_fields)
        where.append(f"({clauses})")
        args.extend([needle] * len(like_fields))
    if grade:
        where.append("quality_grade = ?")
        args.append(grade)
    where_sql = f"WHERE {' AND '.join(where)}" if where else ""

    total = conn.execute(f"SELECT COUNT(*) FROM resource {where_sql}", args).fetchone()[0]
    items = query_all(
        conn,
        f"SELECT {RESOURCE_COLUMNS} FROM resource {where_sql}"
        " ORDER BY quality_final_score DESC, name COLLATE NOCASE ASC LIMIT ?",
        tuple(args) + (limit,),
    )
    result = {
        "total": total,
        "returned": len(items),
        "limit": limit,
        "filters": {"keyword": keyword, "grade": grade},
        "items": items,
    }
    if total == 0:
        result["message"] = "没有匹配的资源，调整关键词或等级后重试"
    return result


VALID_QUEUES = ("d5", "license", "mapping")
REVIEW_QUEUE_LABELS = {"d5": "D5 重复对", "license": "License 许可", "mapping": "Mapping 映射"}


def handle_reviews(conn: sqlite3.Connection, params: dict) -> dict:
    queue_raw = clean_str(params, "queue")
    limit = parse_limit(params)
    if queue_raw is not None and queue_raw.lower() not in VALID_QUEUES:
        raise ApiError(
            400,
            "bad_queue",
            f"queue 仅支持 {'/'.join(VALID_QUEUES)}，收到：{queue_raw!r}",
        )
    wanted = None if queue_raw is None else queue_raw.lower()

    pairs = query_all(
        conn,
        "SELECT target_type AS k, decision AS d, COUNT(*) AS n FROM review GROUP BY target_type, decision",
    )
    items_by_queue: dict[str, list] = {}
    for q in VALID_QUEUES:
        rows = query_all(
            conn,
            "SELECT review_id, target_type, target_id, decision, confidence, reason, reviewer, reviewed_at,"
            " source_system FROM review WHERE target_type = ?"
            " ORDER BY reviewed_at DESC, review_id DESC LIMIT ?",
            (q, limit),
        )
        items_by_queue[q] = rows

    queues = {}
    for q in VALID_QUEUES:
        if wanted is not None and q != wanted:
            continue
        by_decision = {r["d"]: r["n"] for r in pairs if r["k"] == q}
        total = sum(by_decision.values())
        queues[q] = {
            "label": REVIEW_QUEUE_LABELS[q],
            "total": total,
            "by_decision": by_decision,
            "returned": len(items_by_queue[q]),
            "items": items_by_queue[q],
        }
    grand_total = conn.execute("SELECT COUNT(*) FROM review").fetchone()[0]
    return {
        "total": grand_total,
        "queues": queues,
        "filters": {"queue": wanted, "limit": limit},
    }


def handle_tasks(conn: sqlite3.Connection, params: dict) -> dict:
    limit = parse_limit(params)
    total = conn.execute("SELECT COUNT(*) FROM research_task").fetchone()[0]
    by_state = {
        r["k"]: r["n"]
        for r in query_all(
            conn, "SELECT state AS k, COUNT(*) AS n FROM research_task GROUP BY state ORDER BY n DESC"
        )
    }
    by_type = {
        r["k"]: r["n"]
        for r in query_all(
            conn, "SELECT task_type AS k, COUNT(*) AS n FROM research_task GROUP BY task_type ORDER BY n DESC"
        )
    }
    items = query_all(
        conn,
        "SELECT task_id, title, objective, target, task_type, state, source_file, source_system,"
        " created_at, updated_at FROM research_task ORDER BY updated_at DESC, task_id DESC LIMIT ?",
        (limit,),
    )
    result = {"total": total, "by_state": by_state, "by_type": by_type, "limit": limit, "items": items}
    if total == 0:
        result["message"] = "暂无任务：research_task 为空，待 M1 全量数据接入后显示"
    return result


# ---------------------------------------------------------------------------
# Phase 5 增量五端点（ROADMAP P5-4 本阶段落地形态，只读）
# ---------------------------------------------------------------------------

KNOWLEDGE_LIFECYCLE = ("DISCOVERED", "FETCHED", "PARSED", "CANDIDATE", "VERIFIED", "INDEXED", "STALE")
TOOL_LIFECYCLE = ("REGISTERED", "VALIDATED", "ENABLED", "DISABLED", "DEPRECATED")
TOOL_GOVERNANCE = ("ACTIVE", "QUARANTINED", "REJECTED", "ARCHIVED")
RUN_STATUS = ("QUEUED", "WAITING_FOR_RUNNER", "PREPARING", "RUNNING",
              "SUCCEEDED", "FAILED", "TIMED_OUT", "CANCELLED")

KNOWLEDGE_LIST_COLUMNS = (
    "knowledge_id, record_kind, lifecycle, governance, title, content_kind, category, cve,"
    " component, versions, severity, language, evidence_level, verified, verified_note,"
    " hit_count, tags, source, source_url, updated_at"
)

TOOL_LIST_COLUMNS = (
    "tool_id, name, kind, lifecycle, governance, install_state, health_state,"
    " permission_level, timeout_s, source_system, created_at, updated_at"
)

RUN_LIST_COLUMNS = (
    "run_id, tool_id, tool_version, task_id, workspace_id, status, error_category,"
    " duration_ms, created_at, started_at, finished_at, updated_at"
)


def handle_overview(conn: sqlite3.Connection, params: dict) -> dict:
    """Overview 页聚合：M 面计数取主库（与 /api/stats 同源缩小版），
    Phase 1-4 面（tools/runs/runtime）走守卫，缺什么说什么。"""
    result: dict = {
        "database": {
            "file": os.path.basename(DB_PATH),
            "mode": "readonly",
            "aux": {
                "configured": AUX_DB_PATH is not None,
                "file": os.path.basename(AUX_DB_PATH) if AUX_DB_PATH else None,
                "attached": "aux" in attached_schemas(conn),
            },
        }
    }
    result["knowledge"] = {
        "total": conn.execute("SELECT COUNT(*) FROM knowledge").fetchone()[0],
        "by_lifecycle": {
            r["k"]: r["n"]
            for r in query_all(conn, "SELECT lifecycle AS k, COUNT(*) AS n FROM knowledge GROUP BY lifecycle")
        },
        "by_verified": {
            r["k"]: r["n"]
            for r in query_all(
                conn,
                "SELECT CASE WHEN verified = 1 THEN 'verified' WHEN verified = 0 THEN 'unverified'"
                " ELSE 'unknown' END AS k, COUNT(*) AS n FROM knowledge GROUP BY k",
            )
        },
    }
    result["tasks"] = {
        "total": conn.execute("SELECT COUNT(*) FROM research_task").fetchone()[0],
        "by_state": {
            r["k"]: r["n"]
            for r in query_all(conn, "SELECT state AS k, COUNT(*) AS n FROM research_task GROUP BY state")
        },
    }
    for name, table in (("resources", "resource"), ("evidence", "evidence"), ("reviews", "review")):
        result[name] = {"total": conn.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]}

    tools_src = phase14_source(conn, "tool")
    if tools_src:
        result["tools"] = {
            "available": True,
            "source": tools_src,
            "total": conn.execute(f"SELECT COUNT(*) FROM {tools_src}.tool").fetchone()[0],
        }
    else:
        result["tools"] = phase14_unavailable(conn, "tool")
    runs_src = phase14_source(conn, "tool_run")
    if runs_src:
        result["runs"] = {
            "available": True,
            "source": runs_src,
            "total": conn.execute(f"SELECT COUNT(*) FROM {runs_src}.tool_run").fetchone()[0],
            "by_status": {
                r["k"]: r["n"]
                for r in query_all(
                    conn, f"SELECT status AS k, COUNT(*) AS n FROM {runs_src}.tool_run GROUP BY status"
                )
            },
        }
    else:
        result["runs"] = phase14_unavailable(conn, "tool_run")
    runtime_src = phase14_source(conn, "runtime_node")
    if runtime_src:
        result["runtime"] = {
            "available": True,
            "source": runtime_src,
            "total": conn.execute(f"SELECT COUNT(*) FROM {runtime_src}.runtime_node").fetchone()[0],
            "by_health": {
                r["k"]: r["n"]
                for r in query_all(
                    conn, f"SELECT health AS k, COUNT(*) AS n FROM {runtime_src}.runtime_node GROUP BY health"
                )
            },
        }
    else:
        result["runtime"] = phase14_unavailable(conn, "runtime_node")
    return result


def handle_knowledge(conn: sqlite3.Connection, params: dict) -> dict:
    """知识条目列表：主库 knowledge（M 面数据），q（keyword 别名）/lifecycle/verified
    过滤；?knowledge_id= 返回单条全字段详情。"""
    knowledge_id = clean_str(params, "knowledge_id")
    if knowledge_id:
        item = query_one(conn, "SELECT * FROM knowledge WHERE knowledge_id = ?", (knowledge_id,))
        if item is None:
            raise ApiError(404, "no_knowledge", f"知识条目不存在：{knowledge_id}")
        item["tags"] = parse_json_or_none(item.get("tags"))
        return {"available": True, "source": "main", "item": item}

    # q 为 keyword 的别名（Phase 5 Web 前端契约口径，P5-6 起两者等价）
    q_raw = clean_str(params, "q")
    keyword = q_raw if q_raw is not None else clean_str(params, "keyword")
    lifecycle = check_enum(params, "lifecycle", KNOWLEDGE_LIFECYCLE)
    verified_raw = clean_str(params, "verified")
    if verified_raw is not None and verified_raw not in ("true", "false"):
        raise ApiError(400, "bad_verified", f"verified 仅支持 true/false，收到：{verified_raw!r}")
    limit = parse_limit(params)

    where, args = [], []
    if keyword:
        needle = f"%{like_escape(keyword)}%"
        like_fields = ["title", "knowledge_id", "component", "cve", "category"]
        clauses = " OR ".join(f"{f} LIKE ? ESCAPE '\\'" for f in like_fields)
        where.append(f"({clauses})")
        args.extend([needle] * len(like_fields))
    if lifecycle:
        where.append("lifecycle = ?")
        args.append(lifecycle)
    if verified_raw is not None:
        where.append("verified = ?")
        args.append(1 if verified_raw == "true" else 0)
    where_sql = f"WHERE {' AND '.join(where)}" if where else ""

    total = conn.execute(f"SELECT COUNT(*) FROM knowledge {where_sql}", args).fetchone()[0]
    items = query_all(
        conn,
        f"SELECT {KNOWLEDGE_LIST_COLUMNS} FROM knowledge {where_sql}"
        " ORDER BY updated_at DESC, knowledge_id DESC LIMIT ?",
        tuple(args) + (limit,),
    )
    for item in items:
        item["tags"] = parse_json_or_none(item.get("tags"))
    result = {
        "available": True,
        "source": "main",
        "total": total,
        "returned": len(items),
        "limit": limit,
        "filters": {"keyword": keyword, "lifecycle": lifecycle, "verified": verified_raw},
        "items": items,
    }
    if total == 0:
        result["message"] = "没有匹配的知识条目，调整关键词或生命周期后重试"
    return result


def handle_tools(conn: sqlite3.Connection, params: dict) -> dict:
    """工具目录（Phase 1 Registry 面）：?tool_id= 详情含 input/output Schema
    与 tool_version 行；数据面 aux 优先、主库回退，缺表返回真实空态说明。"""
    src = phase14_source(conn, "tool")
    if src is None:
        return phase14_unavailable(conn, "tool")
    # P6-9：aux 库 tool 表增量合并（Phase 6 工具脚手架注册面在开发副本；
    # 主库 tool 是 ETL 全量注册面，aux 只补主库没有的行）。

    tool_id = clean_str(params, "tool_id")
    if tool_id:
        tool = query_one(conn, f"SELECT * FROM {src}.tool WHERE tool_id = ?", (tool_id,))
        detail_src = src
        # 列表口径固定在主库注册面；详情按 id 精确查找，主库未命中且 aux 挂载
        # 时回退 aux（Phase 1 契约工具暂存开发库副本，含真实 input/output Schema）。
        if (
            tool is None
            and src == "main"
            and "aux" in attached_schemas(conn)
            and table_exists(conn, "aux", "tool")
        ):
            tool = query_one(conn, "SELECT * FROM aux.tool WHERE tool_id = ?", (tool_id,))
            detail_src = "aux"
        if tool is None:
            raise ApiError(404, "no_tool", f"工具不存在：{tool_id}")
        versions = query_all(
            conn,
            f"SELECT tv_id, tool_id, version, status, provenance_json, content_hash, validated_at, created_at"
            f" FROM {detail_src}.tool_version WHERE tool_id = ? ORDER BY created_at DESC, tv_id DESC",
            (tool_id,),
        )
        for version in versions:
            version["provenance"] = parse_json_or_none(version.pop("provenance_json", None))
        return {
            "available": True,
            "source": detail_src,
            "tool": tool,
            "input_schema": parse_json_or_none(tool.get("input_schema")),
            "output_schema": parse_json_or_none(tool.get("output_schema")),
            "versions": versions,
        }

    keyword = clean_str(params, "keyword")
    lifecycle = check_enum(params, "lifecycle", TOOL_LIFECYCLE)
    # governance 过滤（Phase 5 Web 前端 Tool Center 契约，P5-7 起提供）
    governance = check_enum(params, "governance", TOOL_GOVERNANCE)
    limit = parse_limit(params)
    where, args = [], []
    if keyword:
        needle = f"%{like_escape(keyword)}%"
        where.append("(name LIKE ? ESCAPE '\\' OR tool_id LIKE ? ESCAPE '\\')")
        args.extend([needle, needle])
    if lifecycle:
        where.append("lifecycle = ?")
        args.append(lifecycle)
    if governance:
        where.append("governance = ?")
        args.append(governance)
    where_sql = f"WHERE {' AND '.join(where)}" if where else ""

    total = conn.execute(f"SELECT COUNT(*) FROM {src}.tool {where_sql}", args).fetchone()[0]
    items = query_all(
        conn,
        f"SELECT {TOOL_LIST_COLUMNS} FROM {src}.tool {where_sql}"
        " ORDER BY updated_at DESC, name COLLATE NOCASE ASC LIMIT ?",
        tuple(args) + (limit,),
    )
    # P6-9：aux 库 tool 表增量合并（工具脚手架注册面在开发副本；主库 tool
    # 是 ETL 全量注册面，aux 只补主库没有的行，不重复主库已有条目）。
    aux_ids = {item["tool_id"] for item in items}
    if "aux" in attached_schemas(conn) and table_exists(conn, "aux", "tool"):
        aux_total = conn.execute("SELECT COUNT(*) FROM aux.tool").fetchone()[0]
        aux_items = query_all(
            conn,
            f"SELECT {TOOL_LIST_COLUMNS} FROM aux.tool"
            " ORDER BY updated_at DESC, name COLLATE NOCASE ASC",
        )
        for row in aux_items:
            if row["tool_id"] not in aux_ids:
                items.append(row)
                total += 1
                aux_ids.add(row["tool_id"])
    return {
        "available": True,
        "source": src,
        "total": total,
        "returned": len(items),
        "limit": limit,
        "filters": {"keyword": keyword, "lifecycle": lifecycle, "governance": governance},
        "items": items,
    }


def handle_runs(conn: sqlite3.Connection, params: dict) -> dict:
    """工具运行列表（Phase 2 Run 面）：?run_id= 详情含全字段（JSON 列解析）
    与 artifact 列表；数据面 aux 优先、主库回退，缺表返回真实空态说明。"""
    src = phase14_source(conn, "tool_run")
    if src is None:
        return phase14_unavailable(conn, "tool_run")

    run_id = clean_str(params, "run_id")
    if run_id:
        run = query_one(conn, f"SELECT * FROM {src}.tool_run WHERE run_id = ?", (run_id,))
        if run is None:
            raise ApiError(404, "no_run", f"Run 不存在：{run_id}")
        for column in ("input_refs_json", "permission_ctx", "budget_json", "error_json",
                       "usage_json", "environment_json", "replay_json"):
            if column in run:
                run[column] = parse_json_or_none(run[column])
        artifacts: list = []
        artifacts_note = None
        artifact_src = phase14_source(conn, "artifact")
        if artifact_src:
            artifacts = query_all(
                conn,
                f"SELECT artifact_id, run_id, role, uri, sha256, size_bytes, mime_type, provenance, created_at"
                f" FROM {artifact_src}.artifact WHERE run_id = ? ORDER BY created_at ASC",
                (run_id,),
            )
        else:
            artifacts_note = "artifact 表不存在（对应迁移未应用），返回空列表"
        result = {"available": True, "source": src, "run": run, "artifacts": artifacts}
        if artifacts_note:
            result["artifacts_note"] = artifacts_note
        return result

    status = check_enum(params, "status", RUN_STATUS)
    tool_filter = clean_str(params, "tool_id")
    limit = parse_limit(params)
    where, args = [], []
    if status:
        where.append("status = ?")
        args.append(status)
    if tool_filter:
        where.append("tool_id = ?")
        args.append(tool_filter)
    where_sql = f"WHERE {' AND '.join(where)}" if where else ""

    total = conn.execute(f"SELECT COUNT(*) FROM {src}.tool_run {where_sql}", args).fetchone()[0]
    items = query_all(
        conn,
        f"SELECT {RUN_LIST_COLUMNS} FROM {src}.tool_run {where_sql}"
        " ORDER BY created_at DESC, run_id DESC LIMIT ?",
        tuple(args) + (limit,),
    )
    return {
        "available": True,
        "source": src,
        "total": total,
        "returned": len(items),
        "limit": limit,
        "filters": {"status": status, "tool_id": tool_filter},
        "items": items,
    }


def handle_runtime(conn: sqlite3.Connection, params: dict) -> dict:
    """运行时节点与心跳（Phase 2 Gateway 面）：节点行 + 心跳时龄推导；
    数据面 aux 优先、主库回退，缺表返回真实空态说明。"""
    src = phase14_source(conn, "runtime_node")
    if src is None:
        return phase14_unavailable(conn, "runtime_node")

    kind = check_enum(params, "kind", ("python_worker", "podman_host", "podman_vm"))
    limit = parse_limit(params)
    where, args = [], []
    if kind:
        where.append("kind = ?")
        args.append(kind)
    where_sql = f"WHERE {' AND '.join(where)}" if where else ""

    total = conn.execute(f"SELECT COUNT(*) FROM {src}.runtime_node {where_sql}", args).fetchone()[0]
    items = query_all(
        conn,
        f"SELECT node_id, kind, endpoint, health, version, last_heartbeat_at, created_at, updated_at"
        f" FROM {src}.runtime_node {where_sql} ORDER BY updated_at DESC, node_id ASC LIMIT ?",
        tuple(args) + (limit,),
    )
    for item in items:
        item["heartbeat_age_seconds"] = heartbeat_age_seconds(item.get("last_heartbeat_at"))
    result = {
        "available": True,
        "source": src,
        "total": total,
        "returned": len(items),
        "limit": limit,
        "filters": {"kind": kind},
        "items": items,
    }
    if total == 0:
        result["message"] = "暂无运行时节点：runtime_node 为空（Gateway 心跳尚未接入或节点未登记）"
    return result




# ---------------------------------------------------------------------------
# P6-8 插件面（ROADMAP P6-8，计划 §10.4 E）：读端点直查 RIP_PLUGIN_DB
# （mode=ro，缺库守卫降级），写端点经 rp-plugin 子进程走 plugin-runtime
# 状态机与 audit-engine，本文件零直连 SQL 写（selftest 静态断言）。
# ---------------------------------------------------------------------------


def connect_plugin_ro() -> sqlite3.Connection:
    """打开插件数据面只读连接；缺库抛 503（调用侧守卫降级）。"""
    if not os.path.isfile(PLUGIN_DB_PATH):
        raise ApiError(503, "plugin_db_missing", f"插件数据面不存在：{PLUGIN_DB_PATH}")
    try:
        conn = sqlite3.connect(f"file:{PLUGIN_DB_PATH}?mode=ro", uri=True)
    except sqlite3.Error as exc:
        raise ApiError(503, "plugin_db_unavailable", f"插件数据面打开失败：{exc}") from exc
    conn.execute("PRAGMA query_only = ON")
    conn.row_factory = sqlite3.Row
    return conn


def plugin_db_ready() -> bool:
    return os.path.isfile(PLUGIN_DB_PATH) and os.path.getsize(PLUGIN_DB_PATH) > 0


def plugin_unavailable() -> dict:
    return {
        "available": False,
        "reason": "plugin_db_not_configured",
        "message": f"插件数据面不存在或为空：{PLUGIN_DB_PATH}（rp-plugin install 会创建）",
        "items": [],
        "total": 0,
    }


def plugin_rows(conn: sqlite3.Connection) -> list:
    return query_all(
        conn,
        "SELECT plugin_id, version, name, kind, description, license, trust_level, state,"
        " update_pending, is_active, governance, source_type, source_ref, pinned_json,"
        " installed_dir, ingested_policy, source_uninstalled, updated_at"
        " FROM plugin ORDER BY plugin_id, version",
    )


def knowledge_source_stats(conn: sqlite3.Connection) -> dict:
    """SK-2：knowledge_source_entry 审核与策略统计（表缺失按空态守卫，
    迁移前的旧插件库不造数据）。"""
    table = query_one(
        conn,
        "SELECT name FROM sqlite_master WHERE type='table' AND name='knowledge_source_entry'",
    )
    if table is None:
        return {"available": False, "reason": "knowledge_source_entry_missing"}
    by_review = query_all(
        conn,
        "SELECT plugin_id, review_status, COUNT(*) AS n FROM knowledge_source_entry"
        " GROUP BY plugin_id, review_status",
    )
    by_kind = query_all(
        conn,
        "SELECT plugin_id, entry_kind, COUNT(*) AS n FROM knowledge_source_entry"
        " GROUP BY plugin_id, entry_kind",
    )
    last_seen = query_all(
        conn,
        "SELECT plugin_id, MAX(last_seen_at) AS last_seen_at FROM knowledge_source_entry"
        " GROUP BY plugin_id",
    )
    stats: dict[str, dict] = {}
    for row in by_review:
        slot = stats.setdefault(row["plugin_id"], {})
        slot.setdefault("review", {})[row["review_status"]] = row["n"]
    for row in by_kind:
        slot = stats.setdefault(row["plugin_id"], {})
        slot.setdefault("entry_kinds", {})[row["entry_kind"]] = row["n"]
    for row in last_seen:
        stats.setdefault(row["plugin_id"], {})["last_seen_at"] = row["last_seen_at"]
    return {"available": True, "stats": stats}


def handle_plugins_list(conn: sqlite3.Connection, params: dict) -> dict:  # noqa: ARG001
    """/api/plugins：L1 目录级（元数据卡片：id/版本/state/kind/trust，无正文）。
    SK-2 增量：pinned（快照 Commit）与 knowledge_stats（审核/类型计数）。"""
    if not plugin_db_ready():
        return plugin_unavailable()
    try:
        ro = connect_plugin_ro()
    except ApiError as exc:
        return {**plugin_unavailable(), "reason": exc.code, "message": exc.message}
    try:
        rows = plugin_rows(ro)
        caps = query_all(
            ro,
            "SELECT plugin_id, plugin_version, capability_id, capability_type, summary"
            " FROM plugin_capability ORDER BY plugin_id, capability_id",
        )
        audits = query_all(
            ro,
            "SELECT action, COUNT(*) AS n FROM audit_event WHERE action LIKE 'plugin.%'"
            " GROUP BY action",
        )
        kse_stats = knowledge_source_stats(ro)
    finally:
        ro.close()
    stats_map = kse_stats.get("stats", {}) if kse_stats.get("available") else {}
    items = []
    for row in rows:
        item = dict(row)
        item["capabilities_count"] = sum(
            1 for c in caps
            if c["plugin_id"] == row["plugin_id"] and c["plugin_version"] == row["version"]
        )
        # SK-2：快照 Commit（009 pinned_json {kind, value}）与来源知识统计。
        if row["pinned_json"]:
            try:
                item["pinned"] = json.loads(row["pinned_json"])
            except json.JSONDecodeError:
                item["pinned"] = None
        else:
            item["pinned"] = None
        item["knowledge_stats"] = stats_map.get(row["plugin_id"])
        items.append(item)
    # 分类（计划 §10.4 E）：已安装 / 可更新 / 已禁用 / 其他（含已卸载）。
    categories = {
        "installed": [p["plugin_id"] for p in rows if p["is_active"] and p["state"] not in ("DISABLED", "UNINSTALLED")],
        "updatable": [p["plugin_id"] for p in rows if p["update_pending"]],
        "disabled": [p["plugin_id"] for p in rows if p["state"] == "DISABLED"],
        "uninstalled": [p["plugin_id"] for p in rows if p["state"] == "UNINSTALLED"],
    }
    return {
        "available": True,
        "db": PLUGIN_DB_PATH,
        "total": len(items),
        "items": items,
        "categories": categories,
        "audit_summary": audits,
    }


def _plugin_detail(conn: sqlite3.Connection, plugin_id: str, level: int) -> dict:
    """详情：L1 元数据卡片 → L2 +context.summary/capability 清单 → L3 +完整
    Manifest 与 docs 正文（三级按需加载，PLUGIN-CONTRACT §9）。"""
    row = query_one(
        conn,
        "SELECT plugin_id, version, name, kind, description, license, trust_level, state,"
        " update_pending, is_active, governance, source_type, source_ref, pinned_json,"
        " lock_sha256, manifest_json, installed_dir, ingested_policy, source_uninstalled,"
        " created_at, updated_at"
        " FROM plugin WHERE plugin_id = ?1 AND is_active = 1",
        (plugin_id,),
    )
    if row is None:
        raise ApiError(404, "plugin_not_found", f"插件不存在或非 active：{plugin_id}")
    detail = {
        "level": level,
        "plugin_id": row["plugin_id"],
        "version": row["version"],
        "name": row["name"],
        "kind": row["kind"],
        "description": row["description"],
        "license": row["license"],
        "trust_level": row["trust_level"],
        "state": row["state"],
        "update_pending": bool(row["update_pending"]),
        "is_active": bool(row["is_active"]),
        "governance": row["governance"],
        "source_type": row["source_type"],
        "source_ref": row["source_ref"],
        "lock_sha256": row["lock_sha256"],
        "installed_dir": row["installed_dir"],
        "source_uninstalled": bool(row["source_uninstalled"]),
        "updated_at": row["updated_at"],
    }
    # SK-2：快照 Commit（pinned）与来源主页（manifest homepage）。
    if row["pinned_json"]:
        try:
            detail["pinned"] = json.loads(row["pinned_json"])
        except json.JSONDecodeError:
            detail["pinned"] = None
    else:
        detail["pinned"] = None
    if level >= 2:
        manifest = json.loads(row["manifest_json"])
        capability_rows = query_all(
            conn,
            "SELECT capability_id, capability_type, entry, summary, runtime, requires_json"
            " FROM plugin_capability WHERE plugin_id = ?1 AND plugin_version = ?2"
            " ORDER BY capability_id",
            (row["plugin_id"], row["version"]),
        )
        capabilities = []
        for cap in capability_rows:
            requires = []
            if cap["requires_json"]:
                try:
                    requires = json.loads(cap["requires_json"])
                except json.JSONDecodeError:
                    requires = []
            capabilities.append({
                "capability_id": cap["capability_id"],
                "capability_type": cap["capability_type"],
                "entry": cap["entry"],
                "summary": cap["summary"],
                "runtime": cap["runtime"],
                "requires": requires,
            })
        detail["context"] = manifest.get("context")
        detail["capabilities"] = capabilities
        detail["compatibility"] = manifest.get("compatibility")
        detail["dependencies"] = manifest.get("dependencies", [])
        # SK-2：来源主页、内容类型与 per-kind 许可/持久化策略（manifest provides）。
        detail["homepage"] = manifest.get("homepage")
        knowledge_provides = {}
        for cap in manifest.get("capabilities", []) or []:
            if cap.get("type") == "knowledge_source":
                provides = cap.get("provides") or {}
                knowledge_provides = {
                    "corpus_id": provides.get("corpus_id"),
                    "content_kinds": provides.get("content_kinds", []),
                    "kinds": {
                        k: {
                            "license_policy": v.get("license_policy"),
                            "source_path": v.get("source_path"),
                            "parser": v.get("parser"),
                        }
                        for k, v in (provides.get("kinds") or {}).items()
                    },
                }
                break
        detail["knowledge_provides"] = knowledge_provides
        kse_stats = knowledge_source_stats(conn)
        detail["knowledge_stats"] = (
            kse_stats.get("stats", {}).get(row["plugin_id"]) if kse_stats.get("available") else None
        )
    if level >= 3:
        detail["manifest"] = manifest
        docs_body = None
        docs_path = manifest.get("context", {}).get("docs")
        installed_dir = row["installed_dir"]
        if docs_path and installed_dir:
            candidate = os.path.join(installed_dir, docs_path)
            if os.path.isfile(candidate) and os.path.realpath(candidate).startswith(
                os.path.realpath(installed_dir) + os.sep
            ):
                try:
                    with open(candidate, encoding="utf-8", errors="replace") as fh:
                        docs_body = fh.read(65536)
                except OSError as exc:
                    docs_body = f"(docs 读取失败：{exc})"
            else:
                docs_body = "(docs 文件不在安装目录内或超出读取上限)"
        detail["docs"] = {"path": docs_path, "body": docs_body}
    audit = query_all(
        conn,
        "SELECT event_id, ts, actor, action, entity_id, detail FROM audit_event"
        " WHERE entity_type = 'plugin' AND entity_id LIKE ?1"
        " ORDER BY event_id DESC LIMIT 50",
        (f"{row['plugin_id']}@%",),
    )
    detail["audit"] = audit
    return detail


def handle_plugin_detail(conn: sqlite3.Connection, params: dict, plugin_id: str) -> dict:
    """/api/plugins/<id>?level=1|2|3：三级按需加载。"""
    if not plugin_db_ready():
        return {**plugin_unavailable(), "plugin_id": plugin_id}
    try:
        ro = connect_plugin_ro()
    except ApiError as exc:
        return {**plugin_unavailable(), "reason": exc.code, "plugin_id": plugin_id}
    try:
        raw_level = (params.get("level", ["2"])[0] or "2").strip()
        try:
            level = int(raw_level)
        except ValueError:
            raise ApiError(400, "bad_level", f"level 必须是 1|2|3，收到：{raw_level!r}")
        if level not in (1, 2, 3):
            raise ApiError(400, "bad_level", f"level 必须是 1|2|3，收到：{level}")
        return _plugin_detail(ro, plugin_id, level)
    finally:
        ro.close()


PLUGIN_WRITE_ACTIONS = ("enable", "disable", "uninstall", "ingest")


def plugin_write(action: str, plugin_id: str, body: dict) -> dict:
    """写操作统一走 rp-plugin 子进程（plugin-runtime 状态机 + audit-engine），
    本进程零直连 SQL 写。返回 CLI 的 JSON stdout；非零退出 → 502 带错误 JSON。"""
    import subprocess

    args = [*PLUGIN_BIN_ARGS]
    if action == "install":
        manifest_path = (body.get("manifest_path") or "").strip()
        if not manifest_path:
            raise ApiError(400, "manifest_path_required", "install 需要 manifest_path")
        real = os.path.realpath(os.path.join(REPO_ROOT, manifest_path))
        root = os.path.realpath(REPO_ROOT)
        if not (real.startswith(root + os.sep) and os.path.isfile(real)):
            raise ApiError(400, "manifest_path_outside_repo",
                           f"manifest_path 必须是仓库内的真实文件：{manifest_path}")
        rel = os.path.relpath(real, root)
        args += ["install", "--manifest", rel]
        trust = (body.get("trust") or "local").strip()
        if trust not in ("official", "community", "local", "remote"):
            raise ApiError(400, "bad_trust", f"trust 取值非法：{trust!r}")
        args += ["--trust", trust]
    elif action == "upgrade":
        manifest_path = (body.get("manifest_path") or "").strip()
        if not manifest_path:
            raise ApiError(400, "manifest_path_required", "upgrade 需要 manifest_path（新版本 Manifest）")
        real = os.path.realpath(os.path.join(REPO_ROOT, manifest_path))
        root = os.path.realpath(REPO_ROOT)
        if not (real.startswith(root + os.sep) and os.path.isfile(real)):
            raise ApiError(400, "manifest_path_outside_repo",
                           f"manifest_path 必须是仓库内的真实文件：{manifest_path}")
        args += ["upgrade", plugin_id, "--manifest", os.path.relpath(real, root)]
    else:
        args += [action, plugin_id]
    try:
        proc = subprocess.run(
            [PLUGIN_BIN, *PLUGIN_BIN_ARGS, "--db", PLUGIN_DB_PATH, *args],
            capture_output=True, text=True, timeout=600,
            cwd=REPO_ROOT,
        )
    except subprocess.TimeoutExpired as exc:
        raise ApiError(504, "plugin_cmd_timeout", f"rp-plugin {action} 超时：{exc}") from exc
    except OSError as exc:
        raise ApiError(502, "plugin_cmd_spawn_failed", f"rp-plugin 启动失败：{exc}") from exc
    payload: dict = {}
    if proc.stdout.strip():
        try:
            payload = json.loads(proc.stdout)
        except json.JSONDecodeError:
            payload = {"raw": proc.stdout[-4000:]}
    if proc.returncode != 0:
        err = {}
        if proc.stderr.strip():
            try:
                err = json.loads(proc.stderr)
            except json.JSONDecodeError:
                err = {"code": "plugin_cmd_failed", "message": proc.stderr[-2000:]}
        raise ApiError(502, err.get("code", "plugin_cmd_failed"),
                       err.get("message", f"rp-plugin {action} 退出码 {proc.returncode}"))
    return {"ok": True, "action": action, "plugin_id": plugin_id, "result": payload}


def handle_knowledge_sources(_conn: sqlite3.Connection, params: dict) -> dict:
    """SK-2 知识来源条目（knowledge_source_entry，011 迁移）：插件摄取的
    catalog_entry / document_markdown / skill_candidate 元数据列表，展示
    来源、分类、许可状态、快照 Commit 与审核状态（P6-10 第 6 条）。
    数据面 = RIP_PLUGIN_DB（只读直查，与 /api/plugins 同一守卫模式）。
    过滤：plugin_id / entry_kind / review_status / q（标题与 URL LIKE）；
    附 per-plugin 汇总。表缺失按真实空态守卫（旧插件库不造数据）。"""
    if not plugin_db_ready():
        return {
            **plugin_unavailable(),
            "items": [],
            "total": 0,
            "plugins": [],
            "summary": [],
        }
    try:
        conn = connect_plugin_ro()
    except ApiError as exc:
        return {
            "available": False,
            "reason": exc.code,
            "message": exc.message,
            "items": [],
            "total": 0,
            "plugins": [],
            "summary": [],
        }
    try:
        return _knowledge_sources_payload(conn, params)
    finally:
        conn.close()


def _knowledge_sources_payload(conn: sqlite3.Connection, params: dict) -> dict:
    table = query_one(
        conn,
        "SELECT name FROM sqlite_master WHERE type='table' AND name='knowledge_source_entry'",
    )
    plugins = query_all(
        conn,
        "SELECT plugin_id, name, version, state, license FROM plugin WHERE is_active = 1"
        " ORDER BY plugin_id",
    )
    if table is None:
        return {
            "available": False,
            "reason": "knowledge_source_entry_missing",
            "message": "knowledge_source_entry 表不存在（需 011 迁移的插件库副本）",
            "plugins": [{"plugin_id": p["plugin_id"], "name": p["name"],
                         "state": p["state"], "license": p["license"]} for p in plugins],
            "items": [],
            "total": 0,
        }

    plugin_id = clean_str(params, "plugin_id")
    entry_kind = check_enum(params, "entry_kind",
                            ("catalog_entry", "document_markdown", "skill_candidate"))
    review_status = check_enum(params, "review_status",
                               ("DISCOVERED", "UNREVIEWED", "APPROVED", "REJECTED", "DEPRECATED"))
    q_raw = clean_str(params, "q")
    limit = parse_limit(params)

    where, args = [], []
    if plugin_id:
        where.append("plugin_id = ?")
        args.append(plugin_id)
    if entry_kind:
        where.append("entry_kind = ?")
        args.append(entry_kind)
    if review_status:
        where.append("review_status = ?")
        args.append(review_status)
    if q_raw:
        needle = f"%{like_escape(q_raw)}%"
        where.append("(title LIKE ? ESCAPE '\\' OR canonical_url LIKE ? ESCAPE '\\'"
                     " OR description LIKE ? ESCAPE '\\')")
        args.extend([needle, needle, needle])
    where_sql = f"WHERE {' AND '.join(where)}" if where else ""

    total = conn.execute(
        f"SELECT COUNT(*) FROM knowledge_source_entry {where_sql}", args
    ).fetchone()[0]
    items = query_all(
        conn,
        "SELECT plugin_id, plugin_version, corpus_id, entry_kind, item_key, title,"
        " description, category_path_json, canonical_url, license_status, rights_policy,"
        " review_status, source_commit, source_url, source_location_json, body_bytes,"
        " first_seen_at, last_seen_at"
        " FROM knowledge_source_entry"
        f" {where_sql} ORDER BY plugin_id, entry_kind, item_key LIMIT ?",
        tuple(args) + (limit,),
    )
    for item in items:
        item["category_path"] = parse_json_or_none(item.get("category_path_json")) or []
        item["source_location"] = parse_json_or_none(item.get("source_location_json")) or {}
        item.pop("category_path_json", None)
        item.pop("source_location_json", None)
    summary = query_all(
        conn,
        "SELECT plugin_id, entry_kind, review_status, COUNT(*) AS n FROM knowledge_source_entry"
        " GROUP BY plugin_id, entry_kind, review_status ORDER BY plugin_id",
    )
    return {
        "available": True,
        "total": total,
        "returned": len(items),
        "limit": limit,
        "filters": {
            "plugin_id": plugin_id,
            "entry_kind": entry_kind,
            "review_status": review_status,
            "q": q_raw,
        },
        "plugins": [
            {
                "plugin_id": p["plugin_id"],
                "name": p["name"],
                "version": p["version"],
                "state": p["state"],
                "license": p["license"],
            }
            for p in plugins
        ],
        "summary": summary,
        "items": items,
    }


API_ROUTES = {
    "/api/plugins": handle_plugins_list,
    "/api/knowledge_sources": handle_knowledge_sources,
    "/api/stats": lambda conn, params: handle_stats(conn),
    "/api/resources": handle_resources,
    "/api/reviews": handle_reviews,
    "/api/tasks": handle_tasks,
    "/api/overview": handle_overview,
    "/api/knowledge": handle_knowledge,
    "/api/tools": handle_tools,
    "/api/runs": handle_runs,
    "/api/runtime": handle_runtime,
}


# ---------------------------------------------------------------------------
# HTTP layer
# ---------------------------------------------------------------------------


class RpHandler(BaseHTTPRequestHandler):
    server_version = "rp-server/0.4.0-phase6"

    def log_message(self, fmt: str, *args) -> None:  # noqa: A002
        sys.stdout.write("%s - %s\n" % (self.address_string(), fmt % args))
        sys.stdout.flush()

    # -- helpers -----------------------------------------------------------

    def send_json(self, payload: dict, status: int = 200) -> None:
        body = json.dumps(payload, ensure_ascii=False, indent=2).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def send_api_error(self, exc: ApiError) -> None:
        body = json.dumps({"error": {"code": exc.code, "message": exc.message}}, ensure_ascii=False, indent=2).encode("utf-8")
        self.send_response(exc.status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        if exc.status == 401:
            self.send_header("WWW-Authenticate", "Bearer")
        self.end_headers()
        self.wfile.write(body)

    def require_auth(self) -> None:
        """Bearer Token 校验：constant-time 比较，失败统一 401 + 节流。"""
        if not AUTH_ENABLED:
            return
        supplied = self.headers.get("Authorization", "").strip()
        expected = f"Bearer {AUTH_TOKEN}"
        if supplied.startswith("Bearer ") and hmac.compare_digest(
            supplied.encode("utf-8"), expected.encode("utf-8")
        ):
            return
        time.sleep(AUTH_FAIL_DELAY)
        raise ApiError(401, "unauthorized", "缺少或无效的访问令牌")

    def resolve_static(self, url_path: str) -> str | None:
        """把 URL 路径安全映射到 static 目录内文件，越界返回 None。"""
        rel = posixpath.normpath(unquote(url_path).lstrip("/"))
        if rel in ("", "."):
            rel = "index.html"
        if rel.startswith("..") or os.path.isabs(rel) or "\x00" in rel:
            return None
        candidate = os.path.realpath(os.path.join(STATIC_DIR, rel))
        root = os.path.realpath(STATIC_DIR)
        if candidate != root and not candidate.startswith(root + os.sep):
            return None
        if not os.path.isfile(candidate):
            return None
        return candidate

    def send_static_file(self, path: str) -> None:
        with open(path, "rb") as fh:
            body = fh.read()
        ext = os.path.splitext(path)[1].lower()
        self.send_response(200)
        self.send_header("Content-Type", CONTENT_TYPES.get(ext, "application/octet-stream"))
        self.send_header("Content-Length", str(len(body)))
        self.send_header("X-Content-Type-Options", "nosniff")
        self.end_headers()
        self.wfile.write(body)

    # -- verbs --------------------------------------------------------------

    def do_GET(self) -> None:  # noqa: N802
        parsed = urlparse(self.path)
        params = parse_qs(parsed.query, keep_blank_values=True)
        try:
            if parsed.path.startswith("/api/"):
                self.require_auth()
                handler = API_ROUTES.get(parsed.path)
                if handler is not None:
                    conn = connect_ro()
                    try:
                        self.send_json(handler(conn, params))
                    finally:
                        conn.close()
                    return
                # P6-8 插件详情动态路径：/api/plugins/<id>?level=1|2|3
                if parsed.path.startswith("/api/plugins/"):
                    plugin_id = unquote(parsed.path[len("/api/plugins/"):]).strip("/")
                    if plugin_id and "/" not in plugin_id:
                        self.send_json(handle_plugin_detail(None, params, plugin_id))
                        return
                raise ApiError(404, "no_route", f"未知 API：{parsed.path}")
            file_path = self.resolve_static(parsed.path)
            if file_path is None:
                raise ApiError(404, "not_found", f"路径不存在或越界：{parsed.path}")
            self.send_static_file(file_path)
        except ApiError as exc:
            self.send_api_error(exc)
        except BrokenPipeError:
            pass
        except Exception as exc:  # 兜底：不泄露堆栈给客户端
            self.send_json({"error": {"code": "internal", "message": f"内部错误：{exc}"}}, status=500)

    def _reject_write(self) -> None:
        self.send_json(
            {"error": {"code": "readonly", "message": "只读工作台：该路径不提供写端点"}},
            status=405,
        )

    def do_POST(self) -> None:  # noqa: N802
        """P6-8：唯一放行的写面是 /api/plugins/*（经 rp-plugin 子进程走
        plugin-runtime 状态机与 audit-engine，本文件零直连 SQL 写）；
        其余路径保持只读 405。写端点强制 Bearer（鉴权关闭时明确拒绝写）。"""
        parsed = urlparse(self.path)
        try:
            if not parsed.path.startswith("/api/plugins/"):
                raise ApiError(405, "readonly", "只读工作台：该路径不提供写端点")
            self.require_auth()
            if not AUTH_ENABLED:
                raise ApiError(403, "auth_required_for_write",
                               "插件写端点要求启用鉴权（配置 RP_AUTH_TOKEN 或令牌文件）")
            length = int(self.headers.get("Content-Length") or 0)
            raw = self.rfile.read(length).decode("utf-8") if length else "{}"
            try:
                body = json.loads(raw) if raw else {}
            except json.JSONDecodeError:
                raise ApiError(400, "bad_json", "请求体不是合法 JSON")
            parts = parsed.path.strip("/").split("/")
            if len(parts) == 3 and parts[2] == "install":
                self.send_json(plugin_write("install", body.get("plugin_id") or "", body))
                return
            if len(parts) == 4:
                plugin_id, action = unquote(parts[2]), parts[3]
                if action not in PLUGIN_WRITE_ACTIONS and action != "upgrade":
                    raise ApiError(404, "no_route", f"未知插件写操作：{action}")
                self.send_json(plugin_write(action, plugin_id, body))
                return
            raise ApiError(404, "no_route", f"未知写端点：{parsed.path}")
        except ApiError as exc:
            self.send_api_error(exc)
        except Exception as exc:  # 兜底：不泄露堆栈
            self.send_json({"error": {"code": "internal", "message": f"内部错误：{exc}"}}, status=500)

    do_PUT = do_DELETE = do_PATCH = _reject_write


def main() -> int:
    if not os.path.isdir(STATIC_DIR):
        print(f"[rp-server] 静态目录缺失：{STATIC_DIR}", file=sys.stderr)
        return 1
    server = ThreadingHTTPServer((HOST, PORT), RpHandler)
    print(f"[rp-server] listening http://{HOST}:{PORT}  (db: {DB_PATH}, mode=ro)")
    print(f"[rp-server] plugin db (P6-8 插件数据面, ro): {PLUGIN_DB_PATH}"
          f" | rp-plugin: {PLUGIN_BIN} {' '.join(PLUGIN_BIN_ARGS)}")
    if AUX_DB_PATH:
        print(f"[rp-server] aux db (Phase 1-4 数据面, ro attach): {AUX_DB_PATH}")
    else:
        print("[rp-server] aux db: 未配置（/api/tools|runs|runtime 按守卫降级为空态说明）")
    print(f"[rp-server] static dir: {STATIC_DIR}")
    if AUTH_ENABLED:
        print("[rp-server] 鉴权：已启用（Bearer Token）")
    else:
        print(
            "[rp-server] 警告：鉴权未启用（本地开发模式）。任何能访问该端口的人都可读取全部 API 数据；"
            "对外或非常规环境使用请先配置 RP_AUTH_TOKEN 或令牌文件（apps/server/rp_service.sh gen-token）",
            file=sys.stderr,
        )
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("[rp-server] shutting down")
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
