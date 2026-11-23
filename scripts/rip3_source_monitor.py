#!/usr/bin/env python3
"""RIP3 Phase 7 source freshness/link/RTIS-reference monitor.

Read/write scope is limited to the RIP3 PostgreSQL monitoring ledger. RTIS SQLite
is opened read-only; POC bodies are never selected or copied. Requires psycopg3
and requests from the existing worker environment.
"""
from __future__ import annotations

import argparse
import hashlib
import http.client
import ipaddress
import json
import os
import ssl
import socket
import sqlite3
import sys
import time
import uuid
from datetime import datetime, timedelta, timezone
from pathlib import Path
from urllib.parse import urljoin, urlparse

import psycopg
def now() -> datetime:
    return datetime.now(timezone.utc)


class CaseInsensitiveHeaders(dict):
    def get(self, key, default=None):
        wanted = key.lower()
        for existing, value in self.items():
            if existing.lower() == wanted:
                return value
        return default


class PinnedResponse:
    def __init__(self, status_code: int, headers: dict[str, str], body: bytes):
        self.status_code = status_code
        self.headers = CaseInsensitiveHeaders(headers)
        self._body = body

    def close(self) -> None:
        return None

    def iter_content(self, chunk_size: int):
        for offset in range(0, len(self._body), chunk_size):
            yield self._body[offset:offset + chunk_size]


def resolve_public_url(url: str) -> tuple[str, int, str]:
    parsed = urlparse(url)
    if parsed.scheme not in ("http", "https") or not parsed.hostname:
        raise ValueError("only absolute HTTP(S) source URLs are supported")
    if parsed.username or parsed.password:
        raise ValueError("credentials embedded in source URL are forbidden")
    port = parsed.port or (443 if parsed.scheme == "https" else 80)
    addresses = {item[4][0] for item in socket.getaddrinfo(parsed.hostname, port, type=socket.SOCK_STREAM)}
    if not addresses:
        raise ValueError("source hostname did not resolve")
    for raw in addresses:
        address = ipaddress.ip_address(raw)
        if not address.is_global:
            raise ValueError(f"non-public source address rejected: {address}")
    return parsed.hostname, port, sorted(addresses)[0]


def validate_public_url(url: str) -> None:
    resolve_public_url(url)


class PinnedHTTPSConnection(http.client.HTTPConnection):
    def __init__(self, host: str, port: int, server_hostname: str, timeout: float):
        super().__init__(host, port, timeout=timeout)
        self.server_hostname = server_hostname

    def connect(self) -> None:
        sock = socket.create_connection((self.host, self.port), timeout=self.timeout)
        context = ssl.create_default_context()
        self.sock = context.wrap_socket(sock, server_hostname=self.server_hostname)


def pinned_request(
    method: str,
    url: str,
    headers: dict[str, str],
    timeout: float,
    resolved_ip: str,
    hostname: str,
    port: int,
) -> PinnedResponse:
    parsed = urlparse(url)
    target = parsed.path or "/"
    if parsed.query:
        target += "?" + parsed.query
    host_header = hostname
    if (parsed.scheme == "http" and port != 80) or (parsed.scheme == "https" and port != 443):
        host_header += f":{port}"
    request_headers = {
        "Host": host_header,
        "User-Agent": "RIP3-Source-Monitor/1.0",
        **{key: value for key, value in headers.items() if value is not None},
    }

    if parsed.scheme == "https":
        conn = PinnedHTTPSConnection(resolved_ip, port, hostname, timeout)
    else:
        conn = http.client.HTTPConnection(resolved_ip, port, timeout=timeout)
    try:
        conn.request(method, target, headers=request_headers)
        response = conn.getresponse()
        raw_headers = {key: value for key, value in response.getheaders()}
        body = b""
        if method != "HEAD":
            body = response.read(256 * 1024 + 1)
            if len(body) > 256 * 1024:
                raise ValueError("source response body exceeds 256 KiB")
        return PinnedResponse(response.status, raw_headers, body)
    finally:
        conn.close()


def checked_request(method: str, url: str, headers: dict[str, str], timeout: float):
    current = url
    for _ in range(6):
        hostname, port, resolved_ip = resolve_public_url(current)
        response = pinned_request(method, current, headers, timeout, resolved_ip, hostname, port)
        if response.status_code in (301, 302, 303, 307, 308):
            location = response.headers.get("Location")
            response.close()
            if not location:
                raise ValueError("redirect response without Location")
            current = urljoin(current, location)
            continue
        return response
    raise ValueError("redirect limit exceeded")


def source_hash(url: str, headers: dict[str, str], timeout: float) -> tuple[int, str | None, str | None, str | None]:
    response = checked_request("HEAD", url, headers, timeout)
    if response.status_code in (405, 501) or (
        response.status_code == 200 and not response.headers.get("ETag")
        and not response.headers.get("Last-Modified")
    ):
        response.close()
        response = checked_request("GET", url, headers, timeout)
    status = response.status_code
    etag = response.headers.get("ETag")
    modified = response.headers.get("Last-Modified")
    digest = None
    if status == 200 and not etag and not modified:
        h = hashlib.sha256()
        read = 0
        for block in response.iter_content(8192):
            if not block:
                continue
            h.update(block)
            read += len(block)
            if read >= 256 * 1024:
                break
        digest = h.hexdigest()
    response.close()
    return status, etag, modified, digest


def init_sources(conn: psycopg.Connection, workspace: str | None) -> int:
    where = "WHERE source_type IN ('url','github') AND btrim(source_url) <> ''"
    params: tuple[object, ...] = ()
    if workspace:
        where += " AND workspace_id=%s"
        params = (workspace,)
    with conn.cursor() as cur:
        cur.execute(
            f"""INSERT INTO source_monitor_state(source_id,canonical_url,next_check_at)
                SELECT source_id,source_url,now() FROM source_records {where}
                ON CONFLICT(source_id) DO UPDATE SET
                    canonical_url=EXCLUDED.canonical_url,
                    previous_hash=source_monitor_state.observed_hash,
                    observed_hash=NULL,etag=NULL,last_modified=NULL,
                    monitor_state='PENDING',next_check_at=now(),
                    last_error='canonical source URL changed',updated_at=now()
                WHERE source_monitor_state.canonical_url IS DISTINCT FROM EXCLUDED.canonical_url""",
            params,
        )
        inserted_or_updated = cur.rowcount
        skipped_where = "s.source_type IN ('url','github') AND btrim(COALESCE(s.source_url,''))=''"
        skipped_params: tuple[object, ...] = ()
        if workspace:
            skipped_where += " AND s.workspace_id=%s"
            skipped_params = (workspace,)
        cur.execute(
            f"""UPDATE source_monitor_state m SET monitor_state='SKIPPED',
                next_check_at=NULL,last_error='source URL missing',updated_at=now()
                FROM source_records s WHERE s.source_id=m.source_id AND {skipped_where}
                AND m.monitor_state <> 'SKIPPED' RETURNING m.source_id""",
            skipped_params,
        )
        for (source_id,) in cur.fetchall():
            cur.execute(
                """INSERT INTO source_monitor_events
                   (event_id,source_id,outcome,detail)
                   VALUES (%s,%s,'SKIPPED','source URL missing')""",
                (f"sme-{uuid.uuid4().hex}", source_id),
            )
        return inserted_or_updated


def check_sources(conn: psycopg.Connection, workspace: str | None, timeout: float, interval_hours: int, limit: int) -> dict[str, int]:
    where = "(m.next_check_at IS NULL OR m.next_check_at <= now())"
    params: list[object] = []
    if workspace:
        where += " AND s.workspace_id=%s"
        params.append(workspace)
    params.append(limit)
    with conn.cursor() as cur:
        cur.execute(
            f"""SELECT m.source_id,m.canonical_url,m.etag,m.last_modified,m.observed_hash
                FROM source_monitor_state m JOIN source_records s USING(source_id)
                WHERE {where} ORDER BY m.next_check_at NULLS FIRST LIMIT %s""",
            tuple(params),
        )
        rows = cur.fetchall()
    counts = {"UNCHANGED": 0, "CHANGED": 0, "MISSING": 0, "ERROR": 0}
    for source_id, url, old_etag, old_modified, old_hash in rows:
        parsed = urlparse(url)
        outcome, status, etag, modified, digest, error = "ERROR", None, None, None, None, None
        try:
            if parsed.scheme not in ("http", "https") or not parsed.netloc:
                raise ValueError("unsupported or malformed URL")
            status, etag, modified, digest = source_hash(
                url, {"If-None-Match": old_etag, "If-Modified-Since": old_modified}, timeout
            )
            if status in (404, 410):
                outcome = "MISSING"
            elif status == 304:
                outcome = "UNCHANGED"
            elif status >= 400:
                outcome = "ERROR"
                error = f"HTTP {status}"
            else:
                comparable_old = old_hash
                comparable_new = digest
                changed = bool(old_etag and etag and old_etag != etag) or bool(old_modified and modified and old_modified != modified)
                changed = changed or bool(comparable_old and comparable_new and comparable_old != comparable_new)
                outcome = "CHANGED" if changed else "UNCHANGED"
                if digest is None and old_hash is None and old_etag is None and old_modified is None:
                    # A first successful HEAD is a baseline, not a change.
                    outcome = "UNCHANGED"
        except Exception as exc:  # network errors are recorded, not fatal to the batch
            error = f"{type(exc).__name__}: {exc}"[:1000]
        previous_hash = old_hash
        observed_hash = digest if digest is not None else old_hash
        next_check = now() + timedelta(hours=interval_hours)
        with conn.transaction():
            with conn.cursor() as cur:
                cur.execute(
                    """INSERT INTO source_monitor_events
                       (event_id,source_id,http_status,etag,last_modified,content_hash,previous_hash,outcome,detail)
                       VALUES (%s,%s,%s,%s,%s,%s,%s,%s,%s)""",
                    (f"sme-{uuid.uuid4().hex}", source_id, status, etag, modified, digest, previous_hash, outcome, error or ""),
                )
                cur.execute(
                    """UPDATE source_monitor_state SET etag=COALESCE(%s,etag),
                       last_modified=COALESCE(%s,last_modified),last_checked_at=now(),
                       http_status=%s,previous_hash=%s,observed_hash=%s,monitor_state=%s,
                       next_check_at=%s,last_error=%s,updated_at=now()
                       WHERE source_id=%s""",
                    (etag, modified, status, previous_hash, observed_hash, outcome, next_check, error, source_id),
                )
        counts[outcome] += 1
    return counts


def check_rtis_refs(
    conn: psycopg.Connection, db_path: str, run_id: str, workspace: str
) -> dict[str, int]:
    """Read RTIS POC identity/verification timestamps only via immutable SQLite URI."""
    path = Path(db_path).expanduser().resolve()
    if not path.is_file():
        return {"checked": 0, "missing": 0, "stale": 0, "unverified": 0, "skipped": 1}
    uri = f"file:{path.as_posix()}?mode=ro&immutable=1"
    try:
        ro = sqlite3.connect(uri, uri=True)
        tables = {r[0] for r in ro.execute("SELECT name FROM sqlite_master WHERE type='table'")}
        if "poc" not in tables:
            ro.close()
            return {"checked": 0, "missing": 0, "stale": 0, "unverified": 0, "skipped": 1}
        columns = {r[1] for r in ro.execute("PRAGMA table_info(poc)")}
        selected = ["id"]
        date_col = next((c for c in ("last_verified_at", "updated_at", "created_at") if c in columns), None)
        if date_col:
            selected.append(date_col)
        verified_col = "verified" if "verified" in columns else None
        if verified_col:
            selected.append(verified_col)
        rows = ro.execute("SELECT " + ",".join('"' + c + '"' for c in selected) + " FROM poc").fetchall()
        ro.close()
    except sqlite3.Error:
        return {"checked": 0, "missing": 0, "stale": 0, "unverified": 0, "skipped": 1}
    by_id: dict[str, tuple[object, object]] = {}
    for row in rows:
        by_id[str(row[0])] = (row[1] if date_col else None, row[-1] if verified_col else None)
    with conn.cursor() as cur:
        cur.execute(
            """SELECT f.task_id,f.finding_id,f.rtis_ref
               FROM findings f JOIN research_tasks t USING(task_id)
               WHERE t.workspace_id=%s AND f.rtis_ref IS NOT NULL
                 AND f.rtis_ref LIKE 'rtis://poc/%%'""",
            (workspace,),
        )
        refs = cur.fetchall()
    counts = {"checked": len(refs), "missing": 0, "stale": 0, "unverified": 0, "skipped": 0}
    cutoff = now() - timedelta(days=365)
    with conn.transaction():
        with conn.cursor() as cur:
            for task_id, finding_id, ref in refs:
                poc_id = ref.removeprefix("rtis://poc/")
                if not poc_id.isdigit():
                    continue
                issues = []
                if poc_id not in by_id:
                    counts["missing"] += 1
                    issues.append(("RTIS_POC_REFERENCE_MISSING", {"task_id": task_id, "rtis_ref": ref}))
                else:
                    date_value, verified_value = by_id[poc_id]
                    if verified_col and str(verified_value).strip().lower() not in ("1", "true", "yes", "verified"):
                        counts["unverified"] += 1
                        issues.append(("RTIS_POC_UNVERIFIED", {"task_id": task_id, "rtis_ref": ref}))
                    if date_col and date_value:
                        try:
                            if isinstance(date_value, (int, float)):
                                checked_at = datetime.fromtimestamp(date_value, timezone.utc)
                            else:
                                checked_at = datetime.fromisoformat(str(date_value).replace("Z", "+00:00"))
                                if checked_at.tzinfo is None:
                                    checked_at = checked_at.replace(tzinfo=timezone.utc)
                            if checked_at < cutoff:
                                counts["stale"] += 1
                                issues.append(("RTIS_POC_STALE", {
                                    "task_id": task_id, "rtis_ref": ref,
                                    "date_field": date_col, "date": str(date_value),
                                }))
                        except (ValueError, OSError, OverflowError):
                            pass
                for code, detail in issues:
                    cur.execute(
                        """INSERT INTO maintenance_findings
                           (finding_row_id,run_id,workspace_id,subject_type,subject_id,check_code,severity,detail)
                           VALUES (%s,%s,%s,'finding',%s,%s,'WARN',%s)""",
                        (f"mfind-{uuid.uuid4().hex}", run_id, workspace, finding_id, code, json.dumps(detail)),
                    )
    return counts


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workspace")
    parser.add_argument("--timeout", type=float, default=12.0)
    parser.add_argument("--interval-hours", type=int, default=24)
    parser.add_argument("--limit", type=int, default=500)
    parser.add_argument("--rtis-db", default=os.environ.get("RTIS_KNOWLEDGE_DB", ""))
    parser.add_argument("--dsn", default=os.environ.get("RP_PG_DSN", ""))
    args = parser.parse_args()
    if not args.dsn:
        parser.error("set RP_PG_DSN or pass --dsn")
    if args.timeout <= 0 or args.interval_hours < 1 or args.limit < 1:
        parser.error("timeout, interval-hours and limit must be positive")
    workspace = args.workspace or "default"
    run_id = f"mrun-{uuid.uuid4().hex}"
    with psycopg.connect(args.dsn) as conn:
        with conn.transaction():
            conn.execute(
                "INSERT INTO maintenance_runs(run_id,workspace_id) VALUES (%s,%s)",
                (run_id, workspace),
            )
        try:
            initialized = init_sources(conn, workspace)
            source_results = check_sources(conn, workspace, args.timeout, args.interval_hours, args.limit)
            rtis_results = (
                check_rtis_refs(conn, args.rtis_db, run_id, workspace)
                if args.rtis_db else {"checked": 0, "missing": 0, "stale": 0, "unverified": 0, "skipped": 1}
            )
            summary = {"initialized": initialized, "sources": source_results, "rtis_refs": rtis_results}
            with conn.transaction():
                conn.execute(
                    "UPDATE maintenance_runs SET finished_at=now(),status='COMPLETED',summary=%s WHERE run_id=%s",
                    (json.dumps(summary), run_id),
                )
        except Exception as exc:
            with conn.transaction():
                conn.execute(
                    "UPDATE maintenance_runs SET finished_at=now(),status='FAILED',summary=%s WHERE run_id=%s",
                    (json.dumps({"error": f"{type(exc).__name__}: {exc}"}), run_id),
                )
            raise
    print(json.dumps({"run_id": run_id, "initialized": initialized, "sources": source_results, "rtis_refs": rtis_results}, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
