#!/usr/bin/env python3
"""RP Server: RIP M7 统一工作台（只读 + Token 鉴权）。

单文件 http.server 实现，Python stdlib-only：
- 静态页（apps/server/static/）
- 只读 JSON API（sqlite3 URI mode=ro 直查生产库 platform.db）：
  M7 四端点 /api/stats /api/resources /api/reviews /api/tasks；
  Phase 5 增量五端点 /api/overview /api/knowledge /api/tools /api/runs
  /api/runtime（对应 ROADMAP P5-4 的本阶段落地形态，api-server crate 化
  顺延，见 docs/phase5/PHASE5-PLAN.md 偏差登记 1）；
  P7-5 /api/traces（六 ID 统一关联）；P7-7 /api/agents /api/agent-runs
  （Agent Studio 读面：定义与能力绑定、AgentRun 列表、步骤轨迹与 013
  三记忆表视图，主库加 aux 双 schema 归并，只读 SELECT 零 SQL 写）；
  P8-3 /api/workflows /api/workflow-runs（Workflow Studio 读面：定义版本
  与 run 六态 + 018 六 ID 关联列 + awaiting_approval 控制行视图，写面
  run/pause/resume/retry 经 rp-workflow 子进程零直连 SQL 写）；
  P8-4 写面追加 /api/workflows put（画布保存：definition-v1 JSON 经
  rp-workflow put 同步子进程落库，版本由存储侧分配递增）与读面 ?id=
  详情逐版 graph 形状（版本历史回看渲染画布）
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
- P9-3 REST 对外契约收口（docs/phase9/REST-API-CONTRACT.md 同源）：
  * trace 关联透出（SESSION-HANDOFF §十 Phase 9 追加项）：run/trace 详情
    响应体带 correlation_id 顶层锚（六 ID 关联字段既有 correlation 字段不
    动），HTTP 层按 trace_response_headers 统一透出 X-Trace-Id 与
    X-Correlation-Id 响应头（锚不在场不发头）；
  * 全局限频：服务进程级滑动窗口（RP_RATE_LIMIT_RPM，默认 600 req/60s，
    0 关闭），超限 429 rate_limited + Retry-After；
  * 对外访问审计：RP_API_AUDIT_LOG 指向 JSONL 文件时每个 /api/* 请求落
    一行 api_access 事件（行形状与 migrations/019_external_api_audit.sql
    的 external_api_audit 表列对应）；本文件零 SQL 写纪律不变；
  * 子进程超时口径显式常量：rp-plugin 600s（PLUGIN_SUBPROCESS_TIMEOUT_S）、
    rp-workflow 同步 60s（WORKFLOW_SYNC_TIMEOUT_S）。

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
- RP_RATE_LIMIT_RPM   全局限频阈值（req/60s，默认 600；0 = 关闭）。P9-3：
                      服务进程级滑动窗口，只计 /api/* 请求（静态资源不计），
                      超限 429 rate_limited + Retry-After
- RP_API_AUDIT_LOG    对外访问审计 JSONL 落盘路径（缺省关闭）。P9-3：每个
                      /api/* 请求追加一行 api_access 事件（ts/method/path/
                      status/remote/duration_ms/trace_id/correlation_id/
                      rate_limited/auth_enabled），行形状与
                      migrations/019_external_api_audit.sql 的
                      external_api_audit 表列一一对应（供消费侧装载）；
                      本文件零 SQL 写（Phase 6 静态门禁）不变
"""

from __future__ import annotations

import hmac
import json
import os
import posixpath
import re
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
from collections import deque
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
# P8-3 workflow 写面：rp-workflow 子进程落库（本进程零直连 SQL 写，
# plugin_write 同款纪律）；读面 workflow_run/workflow_definition 沿
# RP_AUX_DB 只读 ATTACH 通道（主库无 016/017/018 迁移，守卫降级空态说明）。
WORKFLOW_DB_PATH = (os.environ.get("RP_WORKFLOW_DB") or "").strip()     or (AUX_DB_PATH or "")
WORKFLOW_BIN = (os.environ.get("RP_WORKFLOW_BIN") or "target/debug/rp-workflow").strip()
WORKFLOW_LOG_DIR = os.environ.get("RP_WORKFLOW_LOG_DIR", "/tmp/rp-workflow-run-logs")
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

# P9-3 对外契约显式常量（REST-API-CONTRACT.md §限频与超时口径同源）：
# - 子进程超时口径：rp-plugin 同步子进程 600s（504 plugin_cmd_timeout）；
#   rp-workflow 同步子进程 60s（504 workflow_cmd_timeout）；workflow run/
#   resume/retry 为异步承载（Popen 不等待，日志落 WORKFLOW_LOG_DIR），
#   不适用同步超时。
# - 全局限频：服务进程级滑动窗口，只计 /api/*（静态资源不计）；
#   RP_RATE_LIMIT_RPM=0 关闭；超限 429 rate_limited + Retry-After。
PLUGIN_SUBPROCESS_TIMEOUT_S = 600
WORKFLOW_SYNC_TIMEOUT_S = 60
RATE_LIMIT_WINDOW_S = 60
RATE_LIMIT_RPM = int((os.environ.get("RP_RATE_LIMIT_RPM") or "600").strip() or "600")
API_AUDIT_LOG = (os.environ.get("RP_API_AUDIT_LOG") or "").strip() or None

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


# ---------------------------------------------------------------------------
# P9-3 对外契约三件：全局限频 / 对外访问审计 / trace 关联头
# ---------------------------------------------------------------------------

_rate_lock = threading.Lock()
_rate_hits: deque[float] = deque()


def check_rate_limit() -> None:
    """全局限频（超限 429）：服务进程级滑动窗口，/api/* 每请求一个命中。
    ThreadingHTTPServer 多线程并发下用锁串行记账；静态资源不计。"""
    if RATE_LIMIT_RPM <= 0:
        return
    now = time.monotonic()
    with _rate_lock:
        while _rate_hits and now - _rate_hits[0] >= RATE_LIMIT_WINDOW_S:
            _rate_hits.popleft()
        if len(_rate_hits) >= RATE_LIMIT_RPM:
            raise ApiError(429, "rate_limited",
                           f"请求超过全局限频阈值（{RATE_LIMIT_RPM} req/"
                           f"{RATE_LIMIT_WINDOW_S}s），请稍后重试")
        _rate_hits.append(now)


def audit_api_access(record: dict) -> None:
    """对外访问审计：追加一行 JSON 到 RP_API_AUDIT_LOG（缺省关闭）。
    本文件零 SQL 写（Phase 6 selftest 静态门禁），审计落盘走 JSONL 流，
    行形状与 migrations/019_external_api_audit.sql 的 external_api_audit
    表列一一对应（rate_limited/auth_enabled 以 0/1 记，供装载对账）。"""
    if not API_AUDIT_LOG:
        return
    try:
        with open(API_AUDIT_LOG, "a", encoding="utf-8") as fh:
            fh.write(json.dumps(record, ensure_ascii=False, separators=(",", ":")) + "\n")
    except OSError as exc:
        print(f"[rp-server] 审计写入失败（不阻塞响应）：{exc}", file=sys.stderr)


def trace_response_headers(payload: dict) -> dict:
    """P9-3 trace 透出（SESSION-HANDOFF §十 Phase 9 追加项）：从响应载荷
    提取关联锚透出到响应头。取值顺序（第一个非空者胜）：
    - X-Trace-Id：顶层 trace_id > run.trace_id > correlation.trace_id；
    - X-Correlation-Id：顶层 correlation_id > run.run_id。
    锚不在场的载荷（列表页、空态说明）两个头都不发（契约见
    docs/phase9/REST-API-CONTRACT.md §trace 关联透出）。"""
    trace_id = payload.get("trace_id")
    if not trace_id:
        run = payload.get("run")
        if isinstance(run, dict) and run.get("trace_id"):
            trace_id = run["trace_id"]
        else:
            corr = payload.get("correlation")
            if isinstance(corr, dict):
                trace_id = corr.get("trace_id")
    correlation_id = payload.get("correlation_id")
    if not correlation_id:
        run = payload.get("run")
        if isinstance(run, dict) and run.get("run_id"):
            correlation_id = run["run_id"]
    headers: dict = {}
    if trace_id:
        headers["X-Trace-Id"] = str(trace_id)
    if correlation_id:
        headers["X-Correlation-Id"] = str(correlation_id)
    return headers


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


def has_column(conn: sqlite3.Connection, schema: str, table: str, column: str) -> bool:
    """列存在守卫（P7-5 trace 关联面）：同一表在主库/aux 库可能处于不同
    迁移水位（platform*.db 停在 001..004 词表面，开发副本已到 015），
    trace_id 列按库水位条件出现，读面按列存在与否降级，不炸查询。"""
    if not table_exists(conn, schema, table):
        return False
    cols = {
        row[1]
        for row in conn.execute(f"PRAGMA {schema}.table_info({table})").fetchall()
    }
    return column in cols


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


def run_list_columns(conn: sqlite3.Connection, src: str) -> str:
    """P7-5：trace_id 列按库水位条件追加（015 迁移之后的开发副本含列）。"""
    if has_column(conn, src, "tool_run", "trace_id"):
        return RUN_LIST_COLUMNS + ", trace_id"
    return RUN_LIST_COLUMNS


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
    total, items = _tool_catalog_rows(conn, src, keyword, lifecycle, governance, limit)
    return {
        "available": True,
        "source": src,
        "total": total,
        "returned": len(items),
        "limit": limit,
        "filters": {"keyword": keyword, "lifecycle": lifecycle, "governance": governance},
        "items": items,
    }


# MCP 能力面固定词表（crates/mcp-server/src/lib.rs CAPABILITY_TOOLS 六条的
# 同源镜像；P9-4 /api/mcp-catalog 对账用，wire 级一致由 rp-mcp-parity CI 门禁
# 断言：MCP tools/list 能力面 == 本词表）。
MCP_CAPABILITY_TOOLS = (
    "rip.list_tools",
    "rip.get_tool_schema",
    "rip.run_tool",
    "rip.search_knowledge",
    "rip.get_run_status",
    "rip.cancel_run",
)


def _tool_catalog_rows(conn: sqlite3.Connection, src: str, keyword: str | None,
                       lifecycle: str | None, governance: str | None,
                       limit: int) -> tuple[int, list]:
    """工具目录合并读面（handle_tools 列表分支原逻辑抽为共用）：P9-4 起由
    /api/tools（分页列表）与 /api/mcp-catalog（MCP 目录同步对账）共用同一
    实现，不建第二套合并逻辑。返回 (total, items)。"""
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
    return total, items


def handle_mcp_catalog(conn: sqlite3.Connection, params: dict) -> dict:
    """P9-4 MCP 目录同步状态（Tool Center MCP 状态卡数据面）：目录条数 +
    读时对账结果。对账语义（rp_server 只读纪律，不落状态行，每次请求即对）：
    1. catalog_count = 合并注册面条数（与 MCP tools/list 注册面 total 同一
       读面，共用 _tool_catalog_rows）；
    2. 注册面投影完整性：tool_id 非空且无重复、lifecycle/governance 落词表
       （MCP 目录投影 rip.<tool_id> 与 Tool Center 列表同列，词表外即漂移）；
    3. 能力面词表 = MCP_CAPABILITY_TOOLS 六条（mcp-server lib 同源镜像）。
    跨通道 wire 级对账（MCP tools/list vs /api/tools vs 本端点）由
    crates/rp-mcp-parity CI 门禁常驻承载，本端点是读面快照不是替代品。"""
    src = phase14_source(conn, "tool")
    if src is None:
        return phase14_unavailable(conn, "tool")
    total, items = _tool_catalog_rows(conn, src, "", None, None, MAX_LIMIT)
    drift: list = []
    seen: set = set()
    for row in items:
        tool_id = row.get("tool_id") or ""
        if not tool_id:
            drift.append({"check": "tool_id_present", "tool_id": tool_id, "detail": "tool_id 为空"})
        elif tool_id in seen:
            drift.append({"check": "tool_id_unique", "tool_id": tool_id, "detail": "tool_id 重复"})
        else:
            seen.add(tool_id)
        lifecycle = row.get("lifecycle")
        if lifecycle is not None and lifecycle not in TOOL_LIFECYCLE:
            drift.append({"check": "lifecycle_wordlist", "tool_id": tool_id,
                          "detail": f"lifecycle={lifecycle} 不在词表"})
        governance = row.get("governance")
        if governance is not None and governance not in TOOL_GOVERNANCE:
            drift.append({"check": "governance_wordlist", "tool_id": tool_id,
                          "detail": f"governance={governance} 不在词表"})
    return {
        "available": True,
        "source": src,
        "catalog_count": total,
        "capability_count": len(MCP_CAPABILITY_TOOLS),
        "capability_tools": list(MCP_CAPABILITY_TOOLS),
        "reconciliation": {
            "ok": len(drift) == 0 and len(MCP_CAPABILITY_TOOLS) == 6,
            "reconciled_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "drift": drift,
            "scope": "读面对账（注册面完整性 + 能力面词表）；跨通道 wire 对账归 rp-mcp-parity CI 门禁",
        },
        "note": "MCP tools/list 注册面与 /api/tools 共用同一 tool 表读面（catalog.rs 对齐 handle_tools 口径）；本端点为 Tool Center MCP 状态卡数据面，读时对账不落状态行。",
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
            artifact_cols = (
                "artifact_id, run_id, role, uri, sha256, size_bytes, mime_type, provenance, created_at"
                + (", trace_id" if has_column(conn, artifact_src, "artifact", "trace_id") else "")
            )
            artifacts = query_all(
                conn,
                f"SELECT {artifact_cols}"
                f" FROM {artifact_src}.artifact WHERE run_id = ? ORDER BY created_at ASC",
                (run_id,),
            )
        else:
            artifacts_note = "artifact 表不存在（对应迁移未应用），返回空列表"
        result = {"available": True, "source": src, "run": run, "artifacts": artifacts}
        # P7-5 六 ID 关联面（读面聚合，不写库）：
        #   correlation 六字段 = trace_id / task_id / agent_run_id / tool_run_id /
        #   artifact_ids / evidence_ids；agent_run_id 从 submit_key
        #   「agent:{run}:{seq}:{tool}」（P7-4 步序形状）解析，非 agent 驱动
        #   为 None；evidence 行按 source_pk = run_id 归并（trace_id 列按库
        #   水位条件读取）。
        correlation: dict = {
            "trace_id": run.get("trace_id"),
            "task_id": run.get("task_id"),
            "agent_run_id": None,
            "tool_run_id": run_id,
            "artifact_ids": [a.get("artifact_id") for a in artifacts],
            "evidence_ids": [],
        }
        submit_key = run.get("submit_key") or ""
        if submit_key.startswith("agent:"):
            parts = submit_key.split(":")
            if len(parts) >= 3:
                correlation["agent_run_id"] = parts[1]
        evidence_note = None
        # evidence 证据面按 schema 逐库归并（主库 V1 有 evidence 表、工具运行
        # 证据行在 aux 开发副本；两库都可能承载，source_pk 归并去重）。
        evidence_schemas: list[str] = []
        if table_exists(conn, "main", "evidence"):
            evidence_schemas.append("main")
        if "aux" in attached_schemas(conn) and table_exists(conn, "aux", "evidence"):
            evidence_schemas.append("aux")
        result["evidence"] = []
        for evidence_src in evidence_schemas:
            evidence_cols = (
                "evidence_id, evidence_type, storage_ref, content_hash, captured_at, "
                "captured_by, task_ref, source_pk"
                + (", trace_id" if has_column(conn, evidence_src, "evidence", "trace_id") else "")
            )
            result["evidence"] += query_all(
                conn,
                f"SELECT {evidence_cols} FROM {evidence_src}.evidence"
                " WHERE source_pk = ? ORDER BY created_at ASC",
                (run_id,),
            )
        if not evidence_schemas:
            evidence_note = "evidence 表不可读（主库/aux 均缺），返回空列表"
        correlation["evidence_ids"] = [
            e.get("evidence_id") for e in result["evidence"]
        ]
        result["correlation"] = correlation
        # P9-3 对外契约：顶层 correlation_id = tool run_id（audit_event
        # correlation_id 同锚语义）；trace 关联头由 HTTP 层统一透出。
        result["correlation_id"] = run_id
        if evidence_note:
            result["evidence_note"] = evidence_note
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
        f"SELECT {run_list_columns(conn, src)} FROM {src}.tool_run {where_sql}"
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


PLUGIN_WRITE_ACTIONS = ("enable", "disable", "uninstall", "ingest", "cleanup-rollback")

# P10-2：信任级策略表缓存（plugin-registry::trust_policy 单一事实来源，
# rp-plugin trust-policy 子命令承载；进程内缓存一次，CLI 词形漂移即失效）。
_TRUST_POLICY_CACHE: dict | None = None


def _rp_plugin_json(extra_args: list, timeout_s: int = PLUGIN_SUBPROCESS_TIMEOUT_S) -> dict:
    """跑 rp-plugin 子进程并解析单行 JSON stdout（no-db 只读子命令通道：
    manifest-inspect / trust-policy）。非零退出 → 502（stderr 机器可读
    JSON 原样透出 code/message）。"""
    import subprocess

    try:
        proc = subprocess.run(
            [PLUGIN_BIN, *PLUGIN_BIN_ARGS, *extra_args],
            capture_output=True, text=True, timeout=timeout_s, cwd=REPO_ROOT,
        )
    except subprocess.TimeoutExpired as exc:
        raise ApiError(504, "plugin_cmd_timeout", f"rp-plugin 超时（{timeout_s}s）：{exc}") from exc
    except OSError as exc:
        raise ApiError(502, "plugin_cmd_spawn_failed", f"rp-plugin 启动失败：{exc}") from exc
    if proc.returncode != 0:
        try:
            err = json.loads(proc.stderr) if proc.stderr.strip() else {}
        except json.JSONDecodeError:
            err = {}
        raise ApiError(502, err.get("code", "plugin_cmd_failed"),
                       err.get("message", f"rp-plugin 退出码 {proc.returncode}"))
    try:
        return json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        raise ApiError(502, "plugin_cmd_bad_output", f"rp-plugin 输出非 JSON：{exc}") from exc


def load_trust_policy() -> dict:
    """四档信任级策略表（§6 确认强度分级 + D10；缓存一次）。"""
    global _TRUST_POLICY_CACHE
    if _TRUST_POLICY_CACHE is None:
        payload = _rp_plugin_json(["trust-policy"])
        _TRUST_POLICY_CACHE = payload.get("policy") or {}
        if not _TRUST_POLICY_CACHE.get("tiers"):
            raise ApiError(502, "trust_policy_unavailable",
                           "信任级策略表为空（rp-plugin trust-policy 输出异常）")
    return _TRUST_POLICY_CACHE


def trust_tier(trust: str) -> dict:
    """取单档策略（install_confirmation / required_confirm_fields 等）。"""
    for tier in load_trust_policy().get("tiers", []):
        if tier.get("trust_level") == trust:
            return tier
    raise ApiError(500, "trust_policy_missing_tier", f"策略表缺少信任级：{trust}")


def handle_plugin_trust_policy(_conn, params) -> dict:  # noqa: ARG001
    """/api/plugins/trust-policy：四档策略表（web 对话框与文档面单一来源）。"""
    return load_trust_policy()


def plugin_manifest_inspect(body: dict) -> dict:
    """POST /api/plugins/manifest-inspect：Manifest 只读预检（安装确认
    对话框的预览面：id/version/license + 来源推导信任档位与确认强度）。"""
    manifest_path = (body.get("manifest_path") or "").strip()
    if not manifest_path:
        raise ApiError(400, "manifest_path_required", "manifest-inspect 需要 manifest_path")
    rel = _resolve_repo_manifest(manifest_path)
    payload = _rp_plugin_json(["manifest-inspect", "--manifest", rel])
    return {"ok": True, "result": payload}


def _resolve_repo_manifest(manifest_path: str) -> str:
    """仓库内真实文件守卫（install/upgrade/manifest-inspect 共用）。"""
    real = os.path.realpath(os.path.join(REPO_ROOT, manifest_path))
    root = os.path.realpath(REPO_ROOT)
    if not (real.startswith(root + os.sep) and os.path.isfile(real)):
        raise ApiError(400, "manifest_path_outside_repo",
                       f"manifest_path 必须是仓库内的真实文件：{manifest_path}")
    return os.path.relpath(real, root)


def _active_plugin_trust(plugin_id: str) -> str:
    """当前 active 插件的信任级（upgrade 沿用；无 active 行 → 404）。"""
    if not plugin_db_ready():
        raise ApiError(503, "plugin_db_missing", f"插件数据面不存在：{PLUGIN_DB_PATH}")
    conn = connect_plugin_ro()
    try:
        row = query_one(
            conn,
            "SELECT trust_level FROM plugin WHERE plugin_id = ?1 AND is_active = 1",
            (plugin_id,),
        )
    finally:
        conn.close()
    if row is None:
        raise ApiError(404, "plugin_not_found", f"插件不存在或非 active：{plugin_id}")
    return row["trust_level"]


def _enforce_confirmation(trust: str, body: dict, plugin_id: str, manifest_rel: str) -> None:
    """安装/升级确认分级强制（§6 信任级驱动 + D10 语义的写入闸）：
    按策略表 required_confirm_fields 执行；remote 的 confirm_id 与
    Manifest id 对账（服务端 CLI 预检，fail-closed）。"""
    tier = trust_tier(trust)
    required = tier.get("required_confirm_fields") or []
    if "confirm" in required and not body.get("confirm"):
        raise ApiError(
            400, "install_confirmation_required",
            f"信任级 {trust}（{tier.get('confirmation_label', trust)}）需要 confirm=true",
        )
    if "confirm_id" in required:
        expected = _rp_plugin_json(["manifest-inspect", "--manifest", manifest_rel]).get("id")
        if not expected:
            raise ApiError(502, "manifest_inspect_failed",
                           f"Manifest id 解析失败：{manifest_rel}")
        if body.get("confirm_id") != expected:
            raise ApiError(
                400, "install_confirm_id_mismatch",
                f"信任级 {trust} 需要输入插件 id 二次确认：期望 {expected!r}，"
                f"收到 {body.get('confirm_id')!r}",
            )
    if not plugin_id:
        # install 场景 plugin_id 允许为空（body 只带 manifest_path）。
        return


def plugin_write(action: str, plugin_id: str, body: dict) -> dict:
    """写操作统一走 rp-plugin 子进程（plugin-runtime 状态机 + audit-engine），
    本进程零直连 SQL 写。返回 CLI 的 JSON stdout；非零退出 → 502 带错误 JSON。"""
    import subprocess

    args = [*PLUGIN_BIN_ARGS]
    if action == "install":
        manifest_path = (body.get("manifest_path") or "").strip()
        if not manifest_path:
            raise ApiError(400, "manifest_path_required", "install 需要 manifest_path")
        rel = _resolve_repo_manifest(manifest_path)
        args += ["install", "--manifest", rel]
        trust = (body.get("trust") or "local").strip()
        if trust not in ("official", "community", "local", "remote"):
            raise ApiError(400, "bad_trust", f"trust 取值非法：{trust!r}")
        args += ["--trust", trust]
        # P10-2：安装确认强度分级强制（§6 信任级驱动，策略表单一来源
        # plugin-registry::trust_policy）。official 无额外字段；community/
        # local 需 confirm=true；remote 需 confirm=true 且 confirm_id 与
        # Manifest id 一致（服务端 CLI 预检解析，fail-closed）。
        _enforce_confirmation(trust, body, plugin_id, rel)
    elif action == "upgrade":
        manifest_path = (body.get("manifest_path") or "").strip()
        if not manifest_path:
            raise ApiError(400, "manifest_path_required", "upgrade 需要 manifest_path（新版本 Manifest）")
        rel = _resolve_repo_manifest(manifest_path)
        # P10-2：升级信任级沿用当前 active 插件信任级（§6 信任级驱动升级
        # 确认强度；CLI 缺省 local 会静默降档，server 侧显式对齐）。
        current_trust = _active_plugin_trust(plugin_id)
        args += ["upgrade", plugin_id, "--manifest", rel, "--trust", current_trust]
        _enforce_confirmation(current_trust, body, plugin_id, rel)
    elif action == "cleanup-rollback":
        # P10-2 D6：回滚窗口清理（§5.3 第 5 步，用户确认后清理）。
        args += ["cleanup-rollback", plugin_id]
        if body.get("confirm"):
            args += ["--confirm"]
    else:
        args += [action, plugin_id]
    try:
        proc = subprocess.run(
            [PLUGIN_BIN, *PLUGIN_BIN_ARGS, "--db", PLUGIN_DB_PATH, *args],
            capture_output=True, text=True, timeout=PLUGIN_SUBPROCESS_TIMEOUT_S,
            cwd=REPO_ROOT,
        )
    except subprocess.TimeoutExpired as exc:
        raise ApiError(504, "plugin_cmd_timeout",
                       f"rp-plugin {action} 超时（{PLUGIN_SUBPROCESS_TIMEOUT_S}s）：{exc}") from exc
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


def handle_traces(conn: sqlite3.Connection, params: dict) -> dict:
    """P7-5 统一只读关联查询（SESSION-HANDOFF §十追加项 1）：?id=<任一关联
    ID>（trace_id/task_id/agent_run_id/tool_run_id/artifact_id/evidence_id）
    按解析方向汇出任务/意图/上下文/工具/证据/审计全链。SQL 与 Rust 读面
    （agent-engine trace::trace_query）同拓扑：trace_id 五面列为锚，
    context_manifest（意图/上下文面）经 task_id（或无任务 run_id 兜底）
    关联，research_task（任务面）经 agent_run.task_id 关联。
    只读 SELECT；主库与 aux 同表不同迁移水位（platform*.db 停在 V1，
    evidence/audit_event 两库并存），按 schema 逐库查询后归并排序，
    trace_id 列按库水位条件出现，缺列库零贡献（不炸、不伪造）。"""
    query_id = clean_str(params, "id")
    if not query_id:
        raise ApiError(400, "missing_id", "需 ?id=<关联 ID>（六 ID 之一）")
    limit = parse_limit(params)
    limit = max(1, min(limit, 200))

    def schemas_with(table: str) -> list[str]:
        out = []
        if table_exists(conn, "main", table):
            out.append("main")
        if "aux" in attached_schemas(conn) and table_exists(conn, "aux", table):
            out.append("aux")
        return out

    tool_schemas = schemas_with("tool_run")
    agent_schemas = schemas_with("agent_run")
    if not tool_schemas or not agent_schemas:
        return {
            "available": False,
            "reason": "trace_tables_missing",
            "message": "tool_run/agent_run 数据面不可读（对应迁移未应用或库未挂载）：trace 关联面暂不可查",
            "query_id": query_id,
            "trace_ids": [],
            "counts": {},
        }
    audit_schemas = schemas_with("audit_event")
    ev_schemas = schemas_with("evidence")
    art_schemas = schemas_with("artifact")
    wf_schemas = schemas_with("workflow_run")

    def col(src: str, table: str, column: str) -> str:
        return column if has_column(conn, src, table, column) else f"NULL AS {column}"

    def probe_all(template: str, schemas: list[str]) -> list[str]:
        rows: list[str] = []
        for src in schemas:
            try:
                rows += [
                    r[0]
                    for r in conn.execute(template.format(s=src), (query_id,)).fetchall()
                ]
            except sqlite3.Error:
                continue
        return rows

    # ---- 解析 trace_id 集合（探测顺序与 Rust 读面一致）----
    probes = [
        ("agent_run_id",
         "SELECT DISTINCT trace_id FROM {s}.agent_run WHERE run_id = ? AND trace_id IS NOT NULL",
         agent_schemas),
        ("tool_run_id",
         "SELECT DISTINCT trace_id FROM {s}.tool_run WHERE run_id = ? AND trace_id IS NOT NULL",
         tool_schemas),
        ("artifact_id",
         "SELECT DISTINCT trace_id FROM {s}.artifact WHERE artifact_id = ? AND trace_id IS NOT NULL",
         art_schemas),
        ("artifact_id",
         "SELECT DISTINCT tr.trace_id FROM {s}.tool_run tr JOIN {s}.artifact a"
         " ON a.run_id = tr.run_id WHERE a.artifact_id = ? AND tr.trace_id IS NOT NULL",
         tool_schemas),
        ("evidence_id",
         "SELECT DISTINCT trace_id FROM {s}.evidence WHERE evidence_id = ? AND trace_id IS NOT NULL",
         ev_schemas),
        ("evidence_id",
         "SELECT DISTINCT tr.trace_id FROM {s}.tool_run tr JOIN {s}.evidence e"
         " ON e.source_pk = tr.run_id WHERE e.evidence_id = ? AND tr.trace_id IS NOT NULL",
         tool_schemas),
        ("trace_id",
         "SELECT DISTINCT trace_id FROM {s}.audit_event WHERE trace_id = ? AND trace_id IS NOT NULL",
         audit_schemas),
        ("trace_id",
         "SELECT DISTINCT trace_id FROM {s}.audit_event WHERE correlation_id = ?"
         " AND trace_id IS NOT NULL",
         audit_schemas),
        ("task_id",
         "SELECT DISTINCT trace_id FROM {s}.agent_run WHERE task_id = ? AND trace_id IS NOT NULL",
         agent_schemas),
        ("task_id",
         "SELECT DISTINCT trace_id FROM {s}.tool_run WHERE task_id = ? AND trace_id IS NOT NULL",
         tool_schemas),
    ]
    trace_ids: list[str] = []
    matched_via: str | None = None
    # P8-3：workflow_run 反查入参（wfr- run id 即链锚 trace_id，018 关联列）
    # 前置于六 ID 探测（run_id = query_id 直命中；017 库缺列零贡献）。
    if wf_schemas:
        probes = [
            ("workflow_run_id",
             "SELECT trace_id FROM {s}.workflow_run WHERE run_id = ? AND trace_id IS NOT NULL",
             wf_schemas),
            ("trace_id",
             "SELECT trace_id FROM {s}.workflow_run WHERE trace_id = ? AND trace_id IS NOT NULL",
             wf_schemas),
        ] + probes
    for via, template, schemas in probes:
        if not schemas:
            continue
        rows = probe_all(template, schemas)
        if rows:
            trace_ids, matched_via = rows, via
            break
    if not trace_ids and schemas_with("context_manifest"):
        task_row = None
        for man_src in schemas_with("context_manifest"):
            try:
                task_row = conn.execute(
                    f"SELECT task_id FROM {man_src}.context_manifest WHERE manifest_id = ?",
                    (query_id,),
                ).fetchone()
                if task_row and task_row[0]:
                    break
            except sqlite3.Error:
                continue
        if task_row and task_row[0]:
            rows = probe_all(
                "SELECT DISTINCT trace_id FROM {s}.agent_run"
                " WHERE (task_id = ? OR run_id = ?) AND trace_id IS NOT NULL",
                agent_schemas,
            )
            # 反查入参是 task 值本身，重跑参数化探针。
            rows = []
            for src in agent_schemas:
                try:
                    rows += [
                        r[0]
                        for r in conn.execute(
                            f"SELECT DISTINCT trace_id FROM {src}.agent_run"
                            " WHERE (task_id = ?1 OR run_id = ?1) AND trace_id IS NOT NULL",
                            (task_row[0],),
                        ).fetchall()
                    ]
                except sqlite3.Error:
                    continue
            if rows:
                trace_ids, matched_via = rows, "context_manifest_id"
    ph = ",".join("?" for _ in trace_ids)
    args = tuple(trace_ids)

    def q_all(templates: list[tuple[str, str]], params: tuple, order: str) -> list[dict]:
        """逐 schema 查询后归并排序（order 为 SQL ORDER BY 列，作用于归并行）。"""
        rows: list[dict] = []
        for sql, src in templates:
            try:
                cols = [d[0] for d in conn.execute(sql, params).description]
                for row in conn.execute(sql, params).fetchall():
                    item = dict(zip(cols, row))
                    item["_src"] = src
                    rows.append(item)
            except sqlite3.Error:
                continue

        def sort_key(item: dict):
            v = item.get(order.split(" ")[0].split(".")[-1])
            return (v is None, str(v) if v is not None else "")
        rows.sort(key=sort_key)
        return rows[:limit]

    chain: dict = {
        "available": True,
        "query_id": query_id,
        "trace_ids": trace_ids,
        "matched_via": matched_via,
        # P9-3 对外契约：correlation_id = 请求的关联锚（query_id）；单锚命中
        # 时 trace_id = 唯一链锚（多锚命中不猜，仍走 trace_ids 列表）。
        "correlation_id": query_id,
        "trace_id": trace_ids[0] if len(trace_ids) == 1 else None,
    }
    if not trace_ids:
        chain["counts"] = {}
        chain["message"] = (
            f"{query_id} 未解析到任何 trace_id（六 ID 方向均未命中；"
            "015 之前落库行 trace_id 为 NULL 不参与关联）"
        )
        return chain

    chain["agent_runs"] = q_all(
        [
            (
                f"SELECT a.run_id, a.task_id, a.agent_id, a.agent_version, a.state,"
                f" {col(src, 'agent_run', 'trace_id')}, a.created_at, a.started_at, a.finished_at"
                f" FROM {src}.agent_run a WHERE a.trace_id IN ({ph})",
                src,
            )
            for src in agent_schemas
        ],
        args,
        "created_at",
    )
    task_ids = [r["task_id"] for r in chain["agent_runs"] if r.get("task_id")]
    chain["tasks"] = []
    task_schemas = schemas_with("research_task")
    if task_ids and task_schemas:
        tph = ",".join("?" for _ in task_ids)
        chain["tasks"] = q_all(
            [
                (
                    f"SELECT task_id, title, objective, target, task_type, state,"
                    f" created_at, updated_at"
                    f" FROM {src}.research_task WHERE task_id IN ({tph})",
                    src,
                )
                for src in task_schemas
            ],
            tuple(task_ids),
            "task_id",
        )
    # 意图/上下文面（manifest 按 run.task_id 或 run_id 兜底）。
    chain["manifests"] = []
    chain["context_items"] = []
    man_schemas = schemas_with("context_manifest")
    anchors = list(dict.fromkeys(task_ids + [r["run_id"] for r in chain["agent_runs"]]))
    if anchors and man_schemas:
        aph = ",".join("?" for _ in anchors)
        chain["manifests"] = q_all(
            [
                (
                    f"SELECT manifest_id, task_id, intent_json, intent_status, created_at"
                    f" FROM {src}.context_manifest WHERE task_id IN ({aph})",
                    src,
                )
                for src in man_schemas
            ],
            tuple(anchors),
            "created_at",
        )
        manifest_ids = [m["manifest_id"] for m in chain["manifests"]]
        item_schemas = schemas_with("context_manifest_item")
        if manifest_ids and item_schemas:
            mph = ",".join("?" for _ in manifest_ids)
            chain["context_items"] = q_all(
                [
                    (
                        f"SELECT manifest_id, item_id, layer, kind, included, ref_id,"
                        f" source, title"
                        f" FROM {src}.context_manifest_item WHERE manifest_id IN ({mph})",
                        src,
                    )
                    for src in item_schemas
                ],
                tuple(manifest_ids),
                "item_id",
            )
    chain["tool_runs"] = q_all(
        [
            (
                f"SELECT run_id, tool_id, tool_version, task_id, workspace_id, submit_key,"
                f" status, error_category, {col(src, 'tool_run', 'trace_id')},"
                f" created_at, started_at, finished_at"
                f" FROM {src}.tool_run WHERE trace_id IN ({ph})",
                src,
            )
            for src in tool_schemas
        ],
        args,
        "created_at",
    )
    chain["artifacts"] = q_all(
        [
            (
                f"SELECT artifact_id, run_id, role, uri, sha256, size_bytes, mime_type,"
                f" {col(src, 'artifact', 'trace_id')}, created_at"
                f" FROM {src}.artifact WHERE trace_id IN ({ph})",
                src,
            )
            for src in art_schemas
        ],
        args,
        "created_at",
    )
    chain["evidence"] = q_all(
        [
            (
                f"SELECT evidence_id, evidence_type, storage_ref, content_hash, captured_at,"
                f" captured_by, task_ref, source_pk, {col(src, 'evidence', 'trace_id')},"
                f" created_at"
                f" FROM {src}.evidence WHERE trace_id IN ({ph})",
                src,
            )
            for src in ev_schemas
        ],
        args,
        "created_at",
    )
    chain["audit_events"] = q_all(
        [
            (
                f"SELECT event_id, ts, actor, action, entity_type, entity_id,"
                f" correlation_id, {col(src, 'audit_event', 'trace_id')}, source_system"
                f" FROM {src}.audit_event WHERE trace_id IN ({ph})",
                src,
            )
            for src in audit_schemas
        ],
        args,
        "event_id",
    )
    # P8-3：workflow_run 面入链（run_id 即链锚，trace_id/run_id 双向命中；
    # 018 六 ID 关联列按库水位条件出现，缺列库 NULL 占位不炸）。
    chain["workflow_runs"] = []
    if wf_schemas:
        chain["workflow_runs"] = q_all(
            [
                (
                    f"SELECT run_id, workflow_id, workflow_version, state,"
                    f" {_wf_col(conn, src, 'workflow_run', 'trace_id')},"
                    f" {_wf_col(conn, src, 'workflow_run', 'task_id')},"
                    f" {_wf_col(conn, src, 'workflow_run', 'agent_run_id')},"
                    f" {_wf_col(conn, src, 'workflow_run', 'tool_run_id')},"
                    f" {_wf_col(conn, src, 'workflow_run', 'artifact_id')},"
                    f" {_wf_col(conn, src, 'workflow_run', 'evidence_id')},"
                    f" created_at, started_at, finished_at"
                    f" FROM {src}.workflow_run"
                    f" WHERE trace_id IN ({ph}) OR run_id IN ({ph})",
                    src,
                )
                for src in wf_schemas
            ],
            args + args,
            "created_at",
        )
    chain["counts"] = {
        "tasks": len(chain["tasks"]),
        "agent_runs": len(chain["agent_runs"]),
        "manifests": len(chain["manifests"]),
        "context_items": len(chain["context_items"]),
        "tool_runs": len(chain["tool_runs"]),
        "artifacts": len(chain["artifacts"]),
        "evidence": len(chain["evidence"]),
        "audit_events": len(chain["audit_events"]),
        "workflow_runs": len(chain["workflow_runs"]),
    }
    return chain


# ---------------------------------------------------------------------------
# P7-7 Agent Studio 读面（SESSION-HANDOFF §11.4 接入点 4）：/api/agents 与
# /api/agent-runs，只读 SELECT 零 SQL 写；主库与 aux 双 schema 归并同款
# （handle_traces 的 schemas_with 逐库查询模式，缺表守卫不炸、不伪造）。
# ---------------------------------------------------------------------------

AGENT_RUN_STATES = ("QUEUED", "PAUSED", "RUNNING", "SUCCEEDED", "FAILED", "CANCELLED")

AGENT_RUN_LIST_COLUMNS = (
    "run_id, task_id, agent_id, agent_version, state, memory_refs_json,"
    " created_at, started_at, finished_at, updated_at"
)

# 013 三记忆表视图（P7-7 Agent Studio 第三区）：task / evidence / research
# 三族各自的列表列（content_json / refs_json 由 parse_json_or_none 解析）。
AGENT_MEMORY_TABLES = {
    "task": "agent_task_memory",
    "evidence": "agent_evidence_memory",
    "research": "agent_research_memory",
}


def schemas_with_table(conn: sqlite3.Connection, table: str) -> list[str]:
    """承载某张表的 schema 列表（主库优先、aux 兜底；与 handle_traces 内
    schemas_with 同拓扑，提为模块级供 Agent Studio 读面复用）。"""
    out = []
    if table_exists(conn, "main", table):
        out.append("main")
    if "aux" in attached_schemas(conn) and table_exists(conn, "aux", table):
        out.append("aux")
    return out


def select_merged(conn: sqlite3.Connection, statements: list[tuple[str, str]], params: tuple) -> list[dict]:
    """逐 schema 执行同一参数化 SELECT 后归并（_src 标注来源库）。
    单库 sqlite3.Error 视为该库水位不支持该查询，跳过不炸（与
    handle_traces.probe_all 同纪律）。"""
    rows: list[dict] = []
    for sql, src in statements:
        try:
            for row in conn.execute(sql, params).fetchall():
                item = dict(row)
                item["_src"] = src
                rows.append(item)
        except sqlite3.Error:
            continue
    return rows


def sort_by(rows: list[dict], column: str) -> list[dict]:
    """归并行 None 安全排序（升序列名或 'col DESC'）。"""
    desc = column.strip().upper().endswith(" DESC")
    key = column.split(" ")[0]
    rows.sort(
        key=lambda item: (item.get(key) is None, str(item.get(key) or "")),
        reverse=desc,
    )
    return rows


def agent_run_list_sql(conn: sqlite3.Connection, src: str) -> str:
    """agent_run 列表列：trace_id 列按库水位条件追加（015 之后开发副本含列），
    steps_count 子查询在 agent_run_step 同库在场时带上（缺表库不伪造计数）。"""
    cols = AGENT_RUN_LIST_COLUMNS
    if has_column(conn, src, "agent_run", "trace_id"):
        cols += ", trace_id"
    if table_exists(conn, src, "agent_run_step"):
        cols += f", (SELECT COUNT(*) FROM {src}.agent_run_step s WHERE s.run_id = a.run_id) AS steps_count"
    return f"SELECT {cols} FROM {src}.agent_run a"


def capability_refs_of(text: str | None) -> list:
    """capability_refs_json 解析（失败保留原文不静默丢）。"""
    value = parse_json_or_none(text)
    return value if isinstance(value, list) else []


def handle_agents(conn: sqlite3.Connection, params: dict) -> dict:
    """Agent 定义与能力绑定（P7-7 第一区读面）：agent_definition 列表；
    ?id=<agent_id> 详情 = 全版本定义（capability_refs 逐条对账 tool 注册面）
    + 该 agent 的 run 摘要（run_id/task_id/trace_id/steps_count 六 ID 关联
    字段）。主库与 aux 双 schema 归并，只读 SELECT 零 SQL 写。"""
    agent_id = clean_str(params, "id")
    limit = parse_limit(params)
    def_schemas = schemas_with_table(conn, "agent_definition")
    run_schemas = schemas_with_table(conn, "agent_run")

    def merged_definitions(where: str = "", args: tuple = ()) -> list[dict]:
        statements = [
            (
                f"SELECT agent_id, version, name, context_policy, capability_refs_json,"
                f" created_at, updated_at FROM {src}.agent_definition{where}"
                f" ORDER BY updated_at DESC, agent_id, version LIMIT ?",
                src,
            )
            for src in def_schemas
        ]
        merged: list[dict] = []
        seen: set[tuple[str, str]] = set()
        for item in select_merged(conn, statements, args + (limit,)):
            key = (item.get("agent_id") or "", item.get("version") or "")
            if key in seen:
                continue
            seen.add(key)
            item["context_policy"] = parse_json_or_none(item.pop("context_policy", None))
            item["capability_refs"] = capability_refs_of(item.pop("capability_refs_json", None))
            merged.append(item)
        return merged

    def merged_runs(where: str, args: tuple, cap: int) -> list[dict]:
        statements = [
            (f"{agent_run_list_sql(conn, src)}{where} ORDER BY created_at DESC LIMIT ?", src)
            for src in run_schemas
        ]
        merged: list[dict] = []
        seen: set[str] = set()
        for item in select_merged(conn, statements, args + (cap,)):
            run_key = item.get("run_id") or ""
            if run_key in seen:
                continue
            seen.add(run_key)
            item["memory_refs"] = parse_json_or_none(item.pop("memory_refs_json", None))
            merged.append(item)
        sort_by(merged, "created_at DESC")
        return merged

    def runs_summary(runs: list[dict]) -> dict:
        by_state: dict[str, int] = {}
        for item in runs:
            by_state[item.get("state") or "?"] = by_state.get(item.get("state") or "?", 0) + 1
        return {"by_state": by_state}

    if agent_id:
        # 详情：全版本定义 + 能力绑定对账（tool 注册面 lifecycle）+ run 摘要。
        definitions = [
            d
            for d in merged_definitions(" WHERE agent_id = ?", (agent_id,))
            if d.get("agent_id") == agent_id
        ]
        if not definitions:
            raise ApiError(404, "no_agent", f"Agent 定义不存在：{agent_id}")
        tool_schemas = schemas_with_table(conn, "tool")
        bindings: dict[str, dict] = {}
        for definition in definitions:
            for ref in definition["capability_refs"]:
                tool = ref.get("tool_id") if isinstance(ref, dict) else None
                if not tool or tool in bindings:
                    continue
                binding: dict = {"tool_id": tool, "version": ref.get("version")}
                if tool_schemas:
                    # 主库（ETL 全量注册面）优先，aux 开发副本回退（handle_tools
                    # 详情同款：契约工具暂存开发副本时主库可能没有该行）。
                    row = None
                    for tool_src in tool_schemas:
                        row = query_one(
                            conn,
                            f"SELECT name, lifecycle, governance FROM {tool_src}.tool"
                            f" WHERE tool_id = ?",
                            (tool,),
                        )
                        if row:
                            break
                    if row:
                        binding.update({"registered": True, **row})
                    else:
                        binding["registered"] = False
                else:
                    binding["registered"] = None
                bindings[tool] = binding
        runs = merged_runs(" WHERE a.agent_id = ?", (agent_id,), limit)
        summary = runs_summary(runs)
        return {
            "available": True,
            "agent_id": agent_id,
            "definitions": definitions,
            "tool_bindings": list(bindings.values()),
            "runs": runs,
            "runs_total": len(runs),
            "runs_by_state": summary["by_state"],
        }

    items = merged_definitions()
    runs = merged_runs("", (), MAX_LIMIT)
    summary = runs_summary(runs)
    return {
        "available": True,
        "total": len(items),
        "agents": len({item.get("agent_id") for item in items}),
        "returned": len(items),
        "limit": limit,
        "source": "+".join(def_schemas) if def_schemas else "missing",
        "items": items,
        "runs_total": len(runs),
        "runs_by_state": summary["by_state"],
        **(
            {}
            if def_schemas
            else {
                "message": "agent_definition 表不可读（主库/aux 均缺 006 迁移）："
                "Agent 定义面暂不可查（真实空态说明，非空数据）"
            }
        ),
    }


def handle_agent_runs(conn: sqlite3.Connection, params: dict) -> dict:
    """AgentRun 列表与详情（P7-7 第二/三区读面）：?id=<run_id> 详情 =
    agent_run 全行 + agent_run_step 步骤轨迹（seq 升序，014 权威面）
    + 013 三记忆表视图（task/evidence/research，run_id 归并）
    + agent_run_control 控制行（goal/retry 请求位）+ correlation 六 ID
    （trace_id/task_id/agent_run_id/tool_run_ids/artifact_ids/evidence_ids，
    tool_run_id 跳 RunDetail，其余跳 /traces）。主库与 aux 双 schema 归并，
    只读 SELECT 零 SQL 写。"""
    run_id = clean_str(params, "id")
    limit = parse_limit(params)
    run_schemas = schemas_with_table(conn, "agent_run")

    if run_id:
        run: dict | None = None
        run_src: str | None = None
        for src in run_schemas:
            found = query_one(conn, f"SELECT * FROM {src}.agent_run WHERE run_id = ?", (run_id,))
            if found is not None:
                run = dict(found)
                run_src = src
                break
        if run is None or run_src is None:
            raise ApiError(404, "no_agent_run", f"AgentRun 不存在：{run_id}")
        run["memory_refs"] = parse_json_or_none(run.pop("memory_refs_json", None))

        # 步骤轨迹（014 agent_run_step，seq 升序；跨库按 seq 去重归并）。
        step_schemas = schemas_with_table(conn, "agent_run_step")
        steps = select_merged(
            conn,
            [
                (
                    f"SELECT seq, round, tool_id, tool_version, tool_run_id, status,"
                    f" artifact_id, evidence_id, input_json, content_hash, summary,"
                    f" error, elapsed_ms, created_at, updated_at"
                    f" FROM {src}.agent_run_step WHERE run_id = ? ORDER BY seq ASC",
                    src,
                )
                for src in step_schemas
            ],
            (run_id,),
        )
        seen_seq: set[int] = set()
        unique_steps: list[dict] = []
        for step in steps:
            seq = step.get("seq")
            if seq in seen_seq:
                continue
            seen_seq.add(seq)
            step["input"] = parse_json_or_none(step.pop("input_json", None))
            unique_steps.append(step)
        sort_by(unique_steps, "seq")

        # 013 三记忆表视图（run_id 归并；content/refs JSON 列解析）。
        memories: dict[str, list[dict]] = {}
        for view, table in AGENT_MEMORY_TABLES.items():
            mem_schemas = schemas_with_table(conn, table)
            rows = select_merged(
                conn,
                [
                    (f"SELECT * FROM {src}.{table} WHERE run_id = ? ORDER BY created_at ASC", src)
                    for src in mem_schemas
                ],
                (run_id,),
            )
            for row in rows:
                if view == "research":
                    row["refs"] = parse_json_or_none(row.pop("refs_json", None))
                elif view != "evidence":
                    row["content"] = parse_json_or_none(row.pop("content_json", None))
                row.pop("content_json", None)
                row.pop("refs_json", None)
            sort_by(rows, "created_at")
            memories[view] = rows

        # 控制行（014 agent_run_control：goal/retry/协作请求位快照）。
        control: dict | None = None
        for src in schemas_with_table(conn, "agent_run_control"):
            control = query_one(
                conn,
                f"SELECT goal_text, retry_of_run_id, pause_requested_at, cancel_requested_at"
                f" FROM {src}.agent_run_control WHERE run_id = ?",
                (run_id,),
            )
            if control is not None:
                break

        # P7-5 六 ID 关联面（读面聚合，不写库）：tool_run_ids/artifact_ids/
        # evidence_ids 从步骤轨迹与 evidence memory 台账归并去重。
        def collect(column: str) -> list[str]:
            out: list[str] = []
            for step in unique_steps:
                value = step.get(column)
                if value and value not in out:
                    out.append(value)
            for row in memories["evidence"]:
                value = row.get(column)
                if value and value not in out:
                    out.append(value)
            return out

        trace_id = run.get("trace_id")
        correlation = {
            "trace_id": trace_id if trace_id else None,
            "task_id": run.get("task_id"),
            "agent_run_id": run_id,
            "tool_run_ids": collect("tool_run_id"),
            "artifact_ids": collect("artifact_id"),
            "evidence_ids": collect("evidence_id"),
        }
        return {
            "available": True,
            "source": run_src,
            "run": run,
            "steps": unique_steps,
            "memories": memories,
            "control": control,
            "correlation": correlation,
            # P9-3 对外契约：顶层 correlation_id = agent_run_id（AgentRun 的
            # 审计关联锚；trace 关联头由 HTTP 层按 correlation.trace_id 透出）。
            "correlation_id": run_id,
            "counts": {
                "steps": len(unique_steps),
                "task_memory": len(memories["task"]),
                "evidence_memory": len(memories["evidence"]),
                "research_memory": len(memories["research"]),
            },
        }

    state = check_enum(params, "state", AGENT_RUN_STATES)
    agent_filter = clean_str(params, "agent_id")
    where, args = [], []
    if state:
        where.append("a.state = ?")
        args.append(state)
    if agent_filter:
        where.append("a.agent_id = ?")
        args.append(agent_filter)
    where_sql = f" WHERE {' AND '.join(where)}" if where else ""
    statements = [
        (f"{agent_run_list_sql(conn, src)}{where_sql} ORDER BY a.created_at DESC LIMIT ?", src)
        for src in run_schemas
    ]
    merged: list[dict] = []
    seen: set[str] = set()
    for item in select_merged(conn, statements, tuple(args) + (limit,)):
        run_key = item.get("run_id") or ""
        if run_key in seen:
            continue
        seen.add(run_key)
        item["memory_refs"] = parse_json_or_none(item.pop("memory_refs_json", None))
        merged.append(item)
    sort_by(merged, "created_at DESC")
    total = len(merged)
    result = {
        "available": True,
        "source": "+".join(run_schemas) if run_schemas else "missing",
        "total": total,
        "returned": len(merged),
        "limit": limit,
        "filters": {"state": state, "agent_id": agent_filter},
        "items": merged,
        **(
            {}
            if run_schemas
            else {
                "message": "agent_run 表不可读（主库/aux 均缺 006 迁移）："
                "Agent 运行面暂不可查（真实空态说明，非空数据）"
            }
        ),
    }
    if total == 0 and run_schemas:
        result["message"] = "暂无 AgentRun：rp-agent run 端到端链未产生运行记录"
    return result


# ---------------------------------------------------------------------------
# P8-3 Workflow Studio 读面与写面：/api/workflows 与 /api/workflow-runs，
# 只读 SELECT 零 SQL 写（agent 读面同款 schemas_with 逐库查询模式，缺表
# 守卫不炸、不伪造）；写面 run/pause/resume/retry 一律经 subprocess 调
# rp-workflow CLI（run-request-v1 契约形状校验 + 生产库守卫镜像），本文件
# 对 workflow 面零直连 SQL 写（selftest 有静态断言）。
# ---------------------------------------------------------------------------

WORKFLOW_RUN_STATES = ("queued", "running", "paused", "succeeded", "failed", "cancelled")


def _wf_col(conn: sqlite3.Connection, src: str, table: str, column: str) -> str:
    """018 追加列按库水位条件出现（缺列库零贡献，不炸不伪造）。"""
    return column if has_column(conn, src, table, column) else f"NULL AS {column}"


def _wf_control_rows(conn: sqlite3.Connection, src: str, run_id: str) -> dict:
    """018 workflow_run_control 控制行投影（缺表库返回真实空说明形状）。"""
    if not table_exists(conn, src, "workflow_run_control"):
        return {}
    row = None
    try:
        row = conn.execute(
            f"SELECT pause_requested_at, cancel_requested_at, approval_node_id,"
            f" approval_request_json, approval_json, approval_updated_at"
            f" FROM {src}.workflow_run_control WHERE run_id = ?",
            (run_id,),
        ).fetchone()
    except sqlite3.Error:
        return {}
    if not row:
        return {}
    return {
        "pause_requested_at": row[0],
        "cancel_requested_at": row[1],
        "approval_node_id": row[2],
        "approval_request": parse_json_or_none(row[3]),
        "approval": parse_json_or_none(row[4]),
        "approval_updated_at": row[5],
        "awaiting_approval": bool(row[2]) and row[4] is None,
        "_src": src,
    }


def handle_workflows(conn: sqlite3.Connection, params: dict) -> dict:
    """Workflow 定义读面（P8-3 第一区）：workflow_definition 列表；?id= 详情 =
    全版本定义（版本管理历史逐版可查）+ 逐版 graph 形状（P8-4 历史回看渲染
    画布消费）。主库与 aux 双 schema 归并（platform*.db 停在 V1 无 016，
    守卫降级为真实空态说明）。"""
    workflow_id = clean_str(params, "id")
    limit = parse_limit(params)
    schemas = schemas_with_table(conn, "workflow_definition")
    merged: list[dict] = []
    seen: set[tuple[str, int]] = set()
    for src in schemas:
        where = " WHERE workflow_id = ?" if workflow_id else ""
        args: tuple = (workflow_id,) if workflow_id else ()
        try:
            rows = conn.execute(
                f"SELECT workflow_id, version, name, description, compat, node_count,"
                f" edge_count, input_schema_json, source, created_at,"
                f" definition_json"
                f" FROM {src}.workflow_definition{where}"
                f" ORDER BY workflow_id, version LIMIT ?",
                args + (limit,),
            ).fetchall()
        except sqlite3.Error:
            continue
        for row in rows:
            key = (row["workflow_id"], row["version"])
            if key in seen:
                continue
            seen.add(key)
            item = dict(row)
            item["_src"] = src
            merged.append(item)
    sort_by(merged, "created_at")
    for item in merged:
        item["input_schema"] = parse_json_or_none(item.pop("input_schema_json", None))
        # P8-4：逐版 graph 形状（definition_json 规范文本内的 graph 节点/边
        # 向量，历史回看按版本渲染只读画布）。?id= 详情随行返回；列表路径
        # 不带 payload（节点图可能数百 KB）。缺列库（无 definition_json 列）
        # 解析得 None，页面按空 graph 呈现不伪造。
        if workflow_id:
            parsed_doc = parse_json_or_none(item.pop("definition_json", None))
            item["graph"] = parsed_doc.get("graph") if isinstance(parsed_doc, dict) else None
        else:
            item.pop("definition_json", None)
    total_ids = len({item["workflow_id"] for item in merged})
    result = {
        "available": True,
        "source": "+".join(schemas) if schemas else "missing",
        "total_workflows": total_ids,
        "total_versions": len(merged),
        "items": merged,
        **(
            {}
            if schemas
            else {
                "message": "workflow_definition 表不可读（主库/aux 均缺 016 迁移）："
                "Workflow 定义面暂不可查（真实空态说明，非空数据）"
            }
        ),
    }
    if total_ids == 0 and schemas:
        result["message"] = "暂无 workflow 定义：rp-workflow put 端到端链未产生定义记录"
    return result


def handle_workflow_runs(conn: sqlite3.Connection, params: dict) -> dict:
    """WorkflowRun 读面（P8-3 第二区）：run 列表（六态 + 六 ID 关联列）；
    ?id= 详情 = run 头行 + 节点状态行 + 018 控制行（awaiting_approval 判定）
    + checkpoint 计数 + 六 ID 对账面。主库与 aux 双 schema 归并。"""
    run_id = clean_str(params, "id")
    limit = parse_limit(params)
    schemas = schemas_with_table(conn, "workflow_run")
    merged: list[dict] = []
    seen: set[str] = set()
    for src in schemas:
        where = " WHERE run_id = ?" if run_id else ""
        args: tuple = (run_id,) if run_id else ()
        try:
            rows = conn.execute(
                f"SELECT run_id, workflow_id, workflow_version, state, input_json,"
                f" output_json, error_json, created_at, started_at, finished_at,"
                f" {_wf_col(conn, src, 'workflow_run', 'trace_id')},"
                f" {_wf_col(conn, src, 'workflow_run', 'task_id')},"
                f" {_wf_col(conn, src, 'workflow_run', 'agent_run_id')},"
                f" {_wf_col(conn, src, 'workflow_run', 'tool_run_id')},"
                f" {_wf_col(conn, src, 'workflow_run', 'artifact_id')},"
                f" {_wf_col(conn, src, 'workflow_run', 'evidence_id')}"
                f" FROM {src}.workflow_run{where} ORDER BY created_at DESC, run_id LIMIT ?",
                args + (limit,),
            ).fetchall()
        except sqlite3.Error:
            continue
        for row in rows:
            key = row["run_id"]
            if key in seen:
                continue
            seen.add(key)
            item = dict(row)
            item["_src"] = src
            merged.append(item)
    sort_by(merged, "created_at DESC")
    if run_id and merged:
        item = merged[0]
        src = item["_src"]
        detail: dict = {"run": {k: v for k, v in item.items() if k != "input_json"}}
        detail["run"]["input"] = parse_json_or_none(item.get("input_json"))
        detail["run"]["output"] = parse_json_or_none(item.get("output_json"))
        detail["run"]["error"] = parse_json_or_none(item.get("error_json"))
        detail["nodes"] = []
        if table_exists(conn, src, "workflow_run_node"):
            try:
                for row in conn.execute(
                    f"SELECT node_id, node_kind, state, attempt, output_json, error_json,"
                    f" started_at, finished_at,"
                    f" {_wf_col(conn, src, 'workflow_run_node', 'tool_run_id')},"
                    f" {_wf_col(conn, src, 'workflow_run_node', 'agent_run_id')}"
                    f" FROM {src}.workflow_run_node WHERE run_id = ? ORDER BY node_id",
                    (run_id,),
                ).fetchall():
                    node = dict(row)
                    node["output"] = parse_json_or_none(node.pop("output_json", None))
                    node["error"] = parse_json_or_none(node.pop("error_json", None))
                    detail["nodes"].append(node)
            except sqlite3.Error:
                pass
        detail["checkpoint_seq"] = None
        if table_exists(conn, src, "workflow_checkpoint"):
            try:
                row = conn.execute(
                    f"SELECT MAX(seq) FROM {src}.workflow_checkpoint WHERE run_id = ?",
                    (run_id,),
                ).fetchone()
                detail["checkpoint_seq"] = row[0] if row else None
            except sqlite3.Error:
                pass
        detail["control"] = _wf_control_rows(conn, src, run_id)
        detail["correlation"] = {
            "trace_id": item.get("trace_id"),
            "task_id": item.get("task_id"),
            "agent_run_id": item.get("agent_run_id"),
            "tool_run_id": item.get("tool_run_id"),
            "artifact_id": item.get("artifact_id"),
            "evidence_id": item.get("evidence_id"),
        }
        # P9-3 对外契约：顶层 correlation_id = run_id（audit_event.correlation_id
        # 同锚语义，015）；trace 关联头（X-Trace-Id/X-Correlation-Id）由 HTTP
        # 层按 trace_response_headers 统一透出。
        detail["correlation_id"] = run_id
        # 015 trace 面双向对账入口：trace_id 即链锚（= run_id），全链经
        # /api/traces?id=<trace_id> 汇出。
        detail["trace_query"] = f"/api/traces?id={item.get('trace_id')}" if item.get("trace_id") else None
        return {
            "available": True,
            "source": "+".join(schemas),
            "query_id": run_id,
            **detail,
        }
    result = {
        "available": True,
        "source": "+".join(schemas) if schemas else "missing",
        "total": len(merged),
        "returned": len(merged),
        "limit": limit,
        "items": [{k: v for k, v in item.items() if k != "input_json"} for item in merged],
        **(
            {}
            if schemas
            else {
                "message": "workflow_run 表不可读（主库/aux 均缺 017 迁移）："
                "WorkflowRun 运行面暂不可查（真实空态说明，非空数据）"
            }
        ),
    }
    if len(merged) == 0 and schemas:
        result["message"] = "暂无 WorkflowRun：rp-workflow run 端到端链未产生运行记录"
    return result


# ---------------------------------------------------------------------------
# P8-3 workflow 写面（run/pause/resume/retry，经 rp-workflow 子进程）：
# 形状按 schemas/workflow/run-request-v1.schema.json 语义手工核对（stdlib-only
# 无 jsonschema 运行时）；run/resume/retry 异步承载（Popen 不等待，run_id
# 预生成钉定，状态经 GET /api/workflow-runs 轮询），pause 同步（请求位写入
# 即返回）。
# ---------------------------------------------------------------------------

WORKFLOW_WRITE_ACTIONS = ("pause", "resume", "retry")

# run id 形状（run-request-v1 同口径；domain::RunId 兼容 wfr- 前缀连字符词形，
# CLI 钉定 id 如 wfr-p83-agent 均合法）。
_RUN_ID_RE = re.compile(r"^wfr-[A-Za-z0-9._-]+$")


def _workflow_db_guard() -> str:
    """写面目标库守卫（镜像 rp-workflow CLI production_db_refused）：缺配置
    或指向 platform*.db 一律拒绝。"""
    if not WORKFLOW_DB_PATH:
        raise ApiError(503, "workflow_db_not_configured",
                       "写面不可用：未配置 RP_WORKFLOW_DB（workflow 开发库路径）")
    name = os.path.basename(WORKFLOW_DB_PATH)
    if name.startswith("platform"):
        raise ApiError(403, "production_db_refused",
                       "写面拒绝：RP_WORKFLOW_DB 指向 platform*.db（红线 ROADMAP §13）")
    return WORKFLOW_DB_PATH


def _generate_run_id() -> str:
    """run id 预生成（wfr- + 128bit 十六进制，domain::RunId 形状兼容）。"""
    import uuid
    return "wfr-" + uuid.uuid4().hex


def _spawn_workflow(args: list[str], log_name: str) -> dict:
    """异步承载 rp-workflow 子进程（stdout/stderr 落日志文件，不等待）。"""
    os.makedirs(WORKFLOW_LOG_DIR, exist_ok=True)
    log_path = os.path.join(WORKFLOW_LOG_DIR, log_name)
    try:
        with open(log_path, "ab", buffering=0) as log_fh:
            proc = subprocess.Popen(
                [WORKFLOW_BIN, *args],
                stdout=log_fh,
                stderr=log_fh,
                cwd=REPO_ROOT,
                start_new_session=True,
            )
    except OSError as exc:
        raise ApiError(502, "workflow_cmd_spawn_failed",
                       f"rp-workflow 启动失败（构建产物在位? {WORKFLOW_BIN}）：{exc}") from exc
    return {"pid": proc.pid, "log": log_path}


def _run_workflow_sync(args: list[str], timeout: int = WORKFLOW_SYNC_TIMEOUT_S) -> dict:
    """同步承载 rp-workflow 子进程（pause/put 用；JSON stdout 解析失败带原文）。
    同步超时口径 = WORKFLOW_SYNC_TIMEOUT_S（60s，超限 504 workflow_cmd_timeout）。"""
    try:
        proc = subprocess.run(
            [WORKFLOW_BIN, *args],
            capture_output=True, text=True, timeout=timeout,
            cwd=REPO_ROOT,
        )
    except subprocess.TimeoutExpired as exc:
        raise ApiError(504, "workflow_cmd_timeout",
                       f"rp-workflow 超时（{timeout}s）：{exc}") from exc
    except OSError as exc:
        raise ApiError(502, "workflow_cmd_spawn_failed", f"rp-workflow 启动失败：{exc}") from exc
    payload: dict = {}
    if proc.stdout.strip():
        try:
            payload = json.loads(proc.stdout.strip().splitlines()[-1])
        except json.JSONDecodeError:
            payload = {"stdout": proc.stdout[-800:]}
    if proc.returncode != 0:
        stderr_tail = (proc.stderr or "")[-800:]
        return {"ok": False, "returncode": proc.returncode, "stderr_tail": stderr_tail, **payload}
    return {"ok": True, "returncode": proc.returncode, **payload}


def workflow_put(body: dict) -> dict:
    """P8-4 画布保存（POST /api/workflows）：definition-v1 JSON 落临时文件，
    rp-workflow put 同步子进程四道门后追加一版定义（版本由存储侧分配 =
    现有最大版本 + 1），本进程零直连 SQL 写（与 run/pause/resume/retry 同
    一纪律）。CLI exit 0 = PUT；exit 1 = 静态校验拒绝（422 带 REJECTED 汇
    总）；exit 2 = 装载失败（400，Schema/JSON 违规）。临时文件即落即删。"""
    definition = body.get("definition")
    if not isinstance(definition, dict):
        raise ApiError(400, "definition_required",
                       "put 需要 definition（workflow/definition-v1 形状 JSON object）")
    # 最小形状核对（调用方第一道护栏；全量装载门 + 静态校验由 rp-workflow put 承担）
    if definition.get("schema") != "workflow/definition-v1":
        raise ApiError(400, "bad_definition_schema",
                       "definition.schema 必须为 \"workflow/definition-v1\"")
    workflow_id = (definition.get("workflow_id") or "").strip()
    if not workflow_id or len(workflow_id) > 128:
        raise ApiError(400, "bad_workflow_id", "definition.workflow_id 非空且不超过 128 字符")
    graph = definition.get("graph")
    if not isinstance(graph, dict) or not isinstance(graph.get("nodes"), list) \
            or not isinstance(graph.get("edges"), list):
        raise ApiError(400, "bad_graph", "definition.graph.nodes/edges 必须是数组（definition-v1）")
    db = _workflow_db_guard()
    os.makedirs(WORKFLOW_LOG_DIR, exist_ok=True)
    fd, tmp_path = tempfile.mkstemp(prefix="wf-def-", suffix=".json", dir=WORKFLOW_LOG_DIR)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            json.dump(definition, fh, ensure_ascii=False)
        out = _run_workflow_sync(["put", tmp_path, "--db", db])
    finally:
        try:
            os.unlink(tmp_path)
        except OSError:
            pass
    if out.get("ok"):
        return {"result": "put", "workflow_id": out.get("workflow_id"),
                "version": out.get("version"), "nodes": out.get("nodes"),
                "edges": out.get("edges"),
                "status_query": f"/api/workflows?id={out.get('workflow_id')}"}
    code = out.get("returncode")
    if code == 1:
        raise ApiError(422, "definition_rejected",
                       f"definition 静态校验拒绝（REJECTED）：{out.get('stderr_tail') or out}")
    raise ApiError(400, "definition_unparsable",
                   f"definition 装载失败（JSON/Schema 违规）：{out.get('stderr_tail') or out}")


def workflow_write(action: str, run_id: str | None, body: dict) -> dict:
    """workflow 写面统一入口（run/pause/resume/retry）：形状校验 → CLI 子进程。
    本进程零直连 SQL 写；非零退出 → 502 带错误 JSON（CLI stderr 尾部透传）。"""
    if not os.path.isfile(WORKFLOW_BIN):
        raise ApiError(502, "workflow_bin_missing",
                       f"rp-workflow 可执行缺失：{WORKFLOW_BIN}（先 cargo build --offline -p workflow-engine --bin rp-workflow）")
    db = _workflow_db_guard()
    common = ["--db", db]

    if action == "run":
        workflow_id = (body.get("workflow_id") or "").strip()
        if not workflow_id or len(workflow_id) > 128:
            raise ApiError(400, "workflow_id_required", "run 需要 workflow_id（非空 <=128）")
        inp = body.get("input")
        if inp is None:
            inp = {}
        if not isinstance(inp, dict):
            raise ApiError(400, "bad_input", "input 必须是 JSON object（run-request-v1）")
        args = ["run", workflow_id, "--input", json.dumps(inp, ensure_ascii=False)]
        version = body.get("version")
        if version is not None:
            if not isinstance(version, int) or version < 1:
                raise ApiError(400, "bad_version", "version 必须是 >= 1 的整数")
            args += ["--version", str(version)]
        run_id = (body.get("run_id") or "").strip() or _generate_run_id()
        if not _RUN_ID_RE.match(run_id):
            raise ApiError(400, "bad_run_id", "run_id 形状须为 wfr-<alnum>（run-request-v1）")
        args += ["--run-id", run_id, *common]
        spawned = _spawn_workflow(args, f"run-{run_id}.log")
        return {"result": "accepted", "action": "run", "run_id": run_id,
                "correlation_id": run_id, **spawned,
                "status_query": f"/api/workflow-runs?id={run_id}"}

    if not run_id or not run_id.startswith("wfr-"):
        raise ApiError(400, "run_id_required", f"{action} 需要 run_id（wfr- 形状）")

    if action == "pause":
        out = _run_workflow_sync(["pause", run_id, *common])
        if not out.get("ok"):
            raise ApiError(502, "workflow_cmd_failed",
                           f"rp-workflow pause 失败：{out.get('stderr_tail') or out}")
        return {"result": "pause-requested", "action": "pause", "run_id": run_id,
                "correlation_id": run_id, **out}

    if action == "resume":
        args = ["resume", run_id]
        inject = body.get("inject")
        if inject is not None:
            if not isinstance(inject, dict):
                raise ApiError(400, "bad_inject",
                               "inject 必须是 JSON object（原样经 resume --input 并入恢复态；"
                               "批准参数按 human 节点 params.key 包键，缺省 approval）")
            args += ["--input", json.dumps(inject, ensure_ascii=False)]
        args += common
        spawned = _spawn_workflow(args, f"resume-{run_id}.log")
        return {"result": "accepted", "action": "resume", "run_id": run_id,
                "correlation_id": run_id, **spawned,
                "status_query": f"/api/workflow-runs?id={run_id}"}

    if action == "retry":
        node_id = (body.get("node_id") or "").strip()
        if not node_id or len(node_id) > 64:
            raise ApiError(400, "node_id_required", "retry 需要 node_id（非空 <=64）")
        args = ["retry", run_id, "--node", node_id]
        new_run_id = (body.get("new_run_id") or "").strip() or _generate_run_id()
        if not _RUN_ID_RE.match(new_run_id):
            raise ApiError(400, "bad_new_run_id", "new_run_id 形状须为 wfr-<alnum>（run-request-v1）")
        args += ["--run-id", new_run_id, *common]
        spawned = _spawn_workflow(args, f"retry-{new_run_id}.log")
        return {"result": "accepted", "action": "retry", "run_id": run_id,
                "new_run_id": new_run_id, "correlation_id": new_run_id, **spawned,
                "status_query": f"/api/workflow-runs?id={new_run_id}"}

    raise ApiError(404, "no_route", f"未知 workflow 写动作：{action}")


API_ROUTES = {
    "/api/plugins": handle_plugins_list,
    "/api/plugins/trust-policy": handle_plugin_trust_policy,
    "/api/knowledge_sources": handle_knowledge_sources,
    "/api/stats": lambda conn, params: handle_stats(conn),
    "/api/resources": handle_resources,
    "/api/reviews": handle_reviews,
    "/api/tasks": handle_tasks,
    "/api/overview": handle_overview,
    "/api/knowledge": handle_knowledge,
    "/api/tools": handle_tools,
    "/api/mcp-catalog": handle_mcp_catalog,
    "/api/runs": handle_runs,
    "/api/runtime": handle_runtime,
    "/api/traces": handle_traces,
    "/api/agents": handle_agents,
    "/api/agent-runs": handle_agent_runs,
    "/api/workflows": handle_workflows,
    "/api/workflow-runs": handle_workflow_runs,
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

    def _audit_note(self, status: int, payload: dict | None = None,
                    rate_limited: bool = False) -> None:
        """记录本请求的审计要素（status/trace 锚/限频旗），do_* 收尾落 JSONL。"""
        self._audit_status = status
        self._audit_rate_limited = rate_limited
        if payload:
            headers = trace_response_headers(payload)
            self._audit_trace_id = headers.get("X-Trace-Id")
            self._audit_correlation_id = headers.get("X-Correlation-Id")

    def send_json(self, payload: dict, status: int = 200) -> None:
        body = json.dumps(payload, ensure_ascii=False, indent=2).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        for name, value in trace_response_headers(payload).items():
            self.send_header(name, value)
        self._audit_note(status, payload)
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
        if exc.status == 429:
            self.send_header("Retry-After", str(RATE_LIMIT_WINDOW_S))
        self._audit_note(exc.status, rate_limited=(exc.code == "rate_limited"))
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

    def _finish_audit(self, method: str, parsed_path: str, t0: float) -> None:
        """P9-3 对外访问审计收尾：/api/* 请求落一行 JSONL（缺省关闭）。"""
        if not API_AUDIT_LOG or not parsed_path.startswith("/api/"):
            return
        audit_api_access({
            "ts": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "event": "api_access",
            "method": method,
            "path": parsed_path,
            "remote": self.address_string(),
            "status": int(getattr(self, "_audit_status", 0) or 0),
            "duration_ms": int((time.monotonic() - t0) * 1000),
            "trace_id": getattr(self, "_audit_trace_id", None),
            "correlation_id": getattr(self, "_audit_correlation_id", None),
            "rate_limited": 1 if getattr(self, "_audit_rate_limited", False) else 0,
            "auth_enabled": 1 if AUTH_ENABLED else 0,
        })

    def do_GET(self) -> None:  # noqa: N802
        parsed = urlparse(self.path)
        params = parse_qs(parsed.query, keep_blank_values=True)
        t0 = time.monotonic()
        try:
            if parsed.path.startswith("/api/"):
                check_rate_limit()
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
        finally:
            self._finish_audit("GET", parsed.path, t0)

    def _reject_write(self) -> None:
        self.send_json(
            {"error": {"code": "readonly", "message": "只读工作台：该路径不提供写端点"}},
            status=405,
        )

    do_PUT = do_DELETE = do_PATCH = _reject_write

    def do_POST(self) -> None:  # noqa: N802
        """写面白名单（本文件对数据面零直连 SQL 写）：
        - /api/plugins/*（P6-8）：经 rp-plugin 子进程走 plugin-runtime 状态机
          与 audit-engine；
        - /api/workflow-runs（P8-3）：action=run 受理新 run；
        - /api/workflow-runs/<run_id>/{pause,resume,retry}（P8-3）：经
          rp-workflow 子进程走 workflow-engine 控制面与执行器；
        - /api/workflows（P8-4）：put 画布保存（definition-v1 经 rp-workflow
          put 子进程落库，版本存储侧递增）。
        其余路径保持只读 405。写端点强制 Bearer（鉴权关闭时明确拒绝写），
        鉴权四件套（require_auth / constant-time 比较 / 失败节流 / 写面
        强制鉴权）保持不变。P9-3 增量：全局限频先行（429 rate_limited +
        Retry-After），对外访问审计收尾落 JSONL。"""
        parsed = urlparse(self.path)
        t0 = time.monotonic()
        try:
            check_rate_limit()
            # P8-4 workflow 定义保存（画布图转 definition JSON 走版本递增）。
            if parsed.path == "/api/workflows":
                self.require_auth()
                if not AUTH_ENABLED:
                    raise ApiError(403, "auth_required_for_write",
                                   "workflow 写端点要求启用鉴权（配置 RP_AUTH_TOKEN 或令牌文件）")
                body = self._read_json_body()
                self.send_json(workflow_put(body))
                return
            # P8-3 workflow 写面（run 走集合路径，pause/resume/retry 走动态路径）。
            if parsed.path == "/api/workflow-runs":
                self.require_auth()
                if not AUTH_ENABLED:
                    raise ApiError(403, "auth_required_for_write",
                                   "workflow 写端点要求启用鉴权（配置 RP_AUTH_TOKEN 或令牌文件）")
                body = self._read_json_body()
                self.send_json(workflow_write("run", None, body))
                return
            parts = parsed.path.strip("/").split("/")
            # 形状一：/api/workflow-runs/<action>（run_id 在 body）。
            if len(parts) == 3 and parts[0] == "api" and parts[1] == "workflow-runs" \
                    and parts[2] in WORKFLOW_WRITE_ACTIONS:
                self.require_auth()
                if not AUTH_ENABLED:
                    raise ApiError(403, "auth_required_for_write",
                                   "workflow 写端点要求启用鉴权（配置 RP_AUTH_TOKEN 或令牌文件）")
                body = self._read_json_body()
                self.send_json(workflow_write(parts[2], body.get("run_id") or None, body))
                return
            # 形状二：/api/workflow-runs/<run_id>/<action>（REST 动态路径）。
            if len(parts) == 4 and parts[0] == "api" and parts[1] == "workflow-runs" \
                    and parts[3] in WORKFLOW_WRITE_ACTIONS:
                self.require_auth()
                if not AUTH_ENABLED:
                    raise ApiError(403, "auth_required_for_write",
                                   "workflow 写端点要求启用鉴权（配置 RP_AUTH_TOKEN 或令牌文件）")
                body = self._read_json_body()
                self.send_json(workflow_write(parts[3], unquote(parts[2]), body))
                return
            if not parsed.path.startswith("/api/plugins/"):
                raise ApiError(405, "readonly", "只读工作台：该路径不提供写端点")
            self.require_auth()
            if not AUTH_ENABLED:
                raise ApiError(403, "auth_required_for_write",
                               "插件写端点要求启用鉴权（配置 RP_AUTH_TOKEN 或令牌文件）")
            body = self._read_json_body()
            parts = parsed.path.strip("/").split("/")
            if len(parts) == 3 and parts[2] == "install":
                self.send_json(plugin_write("install", body.get("plugin_id") or "", body))
                return
            # P10-2：Manifest 只读预检（安装确认对话框的预览面，零写入）。
            if len(parts) == 3 and parts[2] == "manifest-inspect":
                self.send_json(plugin_manifest_inspect(body))
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
        finally:
            self._finish_audit("POST", parsed.path, t0)

    def _read_json_body(self) -> dict:
        """读 POST JSON 请求体（上限 1 MiB，防无界读）。"""
        length = int(self.headers.get("Content-Length") or 0)
        if length > 1_048_576:
            raise ApiError(413, "payload_too_large", "请求体超过 1 MiB 上限")
        raw = self.rfile.read(length).decode("utf-8") if length else "{}"
        try:
            return json.loads(raw) if raw else {}
        except json.JSONDecodeError:
            raise ApiError(400, "bad_json", "请求体不是合法 JSON")


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
