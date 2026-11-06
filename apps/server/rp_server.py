#!/usr/bin/env python3
"""RP Server: RIP M7 统一工作台（只读 + Token 鉴权）。

单文件 http.server 实现，Python stdlib-only：
- 静态页（apps/server/static/）
- 4 个只读 JSON API（sqlite3 URI mode=ro 直查生产库 platform.db）
- 无任何写端点，默认绑定 127.0.0.1（容器内通过 RP_HOST=0.0.0.0 覆盖）
- Bearer Token 鉴权：配置 RP_AUTH_TOKEN 或 RP_AUTH_TOKEN_FILE 后，
  所有 /api/* 请求必须携带有效令牌（constant-time 比较 + 失败节流）；
  未配置任何令牌时为本地开发模式（鉴权关闭，启动时显著告警）。

环境变量：
- RP_PORT            监听端口，默认 7800
- RP_HOST            监听地址，默认 127.0.0.1
- RP_DB_PATH         数据库路径，默认仓库内 migrations/platform.db
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
    return conn


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


API_ROUTES = {
    "/api/stats": lambda conn, params: handle_stats(conn),
    "/api/resources": handle_resources,
    "/api/reviews": handle_reviews,
    "/api/tasks": handle_tasks,
}


# ---------------------------------------------------------------------------
# HTTP layer
# ---------------------------------------------------------------------------


class RpHandler(BaseHTTPRequestHandler):
    server_version = "rp-server/0.2.0-m7-auth"

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
