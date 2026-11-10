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

环境变量：
- RP_PORT            监听端口，默认 7800
- RP_HOST            监听地址，默认 127.0.0.1
- RP_DB_PATH         数据库路径，默认仓库内 migrations/platform.db
- RP_AUX_DB          Phase 1-4 数据面只读库（如 migrations/dev-b5.db），缺省不挂载
- RP_AUTH_TOKEN      明文令牌（优先）
- RP_AUTH_TOKEN_FILE 令牌文件路径（默认仓库根 .rp-workbench-token，权限应为 600）
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
    limit = parse_limit(params)
    where, args = [], []
    if keyword:
        needle = f"%{like_escape(keyword)}%"
        where.append("(name LIKE ? ESCAPE '\\' OR tool_id LIKE ? ESCAPE '\\')")
        args.extend([needle, needle])
    if lifecycle:
        where.append("lifecycle = ?")
        args.append(lifecycle)
    where_sql = f"WHERE {' AND '.join(where)}" if where else ""

    total = conn.execute(f"SELECT COUNT(*) FROM {src}.tool {where_sql}", args).fetchone()[0]
    items = query_all(
        conn,
        f"SELECT {TOOL_LIST_COLUMNS} FROM {src}.tool {where_sql}"
        " ORDER BY updated_at DESC, name COLLATE NOCASE ASC LIMIT ?",
        tuple(args) + (limit,),
    )
    return {
        "available": True,
        "source": src,
        "total": total,
        "returned": len(items),
        "limit": limit,
        "filters": {"keyword": keyword, "lifecycle": lifecycle},
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


API_ROUTES = {
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
    server_version = "rp-server/0.3.0-phase5"

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
                if handler is None:
                    raise ApiError(404, "no_route", f"未知 API：{parsed.path}")
                conn = connect_ro()
                try:
                    self.send_json(handler(conn, params))
                finally:
                    conn.close()
                return
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
            {"error": {"code": "readonly", "message": "只读工作台：本服务不提供写端点"}},
            status=405,
        )

    do_POST = do_PUT = do_DELETE = do_PATCH = _reject_write


def main() -> int:
    if not os.path.isdir(STATIC_DIR):
        print(f"[rp-server] 静态目录缺失：{STATIC_DIR}", file=sys.stderr)
        return 1
    server = ThreadingHTTPServer((HOST, PORT), RpHandler)
    print(f"[rp-server] listening http://{HOST}:{PORT}  (db: {DB_PATH}, mode=ro)")
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
