#!/usr/bin/env python3
"""PostgreSQL integration smoke for Phase 7 ETag change and dead-link monitoring."""
from __future__ import annotations
import json, os, subprocess, threading, uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import psycopg

class Handler(BaseHTTPRequestHandler):
    status = 200
    etag = '"v1"'
    def do_HEAD(self):
        self.send_response(self.status)
        if self.status == 200:
            self.send_header("ETag", self.etag)
        self.end_headers()
    def do_GET(self):
        self.do_HEAD()
    def log_message(self, *_args):
        pass

root = Path(__file__).resolve().parents[2]
dsn = os.environ.get("RP_TEST_PG_DSN", "postgres://postgres:rip7p8test@127.0.0.1:29317/postgres")
tag = uuid.uuid4().hex
workspace = f"ws-monitor-{tag}"
source_id = f"src-monitor-{tag}"
content_hash = tag + "0" * (64-len(tag))
server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
thread = threading.Thread(target=server.serve_forever, daemon=True)
thread.start()
url = f"http://127.0.0.1:{server.server_port}/source"

def run_worker():
    env = os.environ.copy()
    env["RP_PG_DSN"] = dsn
    env["RIP3_SOURCE_MONITOR_ALLOW_PRIVATE"] = "1"
    result = subprocess.run(
        ["python3", str(root / "scripts/rip3_source_monitor.py"), "--workspace", workspace],
        cwd=root, env=env, text=True, capture_output=True, check=True,
    )
    return json.loads(result.stdout)

try:
    with psycopg.connect(dsn) as conn:
        conn.execute(
            "INSERT INTO content_objects(content_hash,size_bytes,mime_type,storage_uri,ref_count) VALUES (%s,0,'text/plain',%s,1)",
            (content_hash, f"cas://{content_hash}"),
        )
        conn.execute(
            "INSERT INTO source_records(source_id,content_hash,source_type,source_url,workspace_id) VALUES (%s,%s,'url',%s,%s)",
            (source_id, content_hash, url, workspace),
        )

    first = run_worker()
    with psycopg.connect(dsn) as conn:
        state = conn.execute("SELECT monitor_state,etag FROM source_monitor_state WHERE source_id=%s", (source_id,)).fetchone()
        assert state == ("UNCHANGED", '"v1"'), state

    Handler.etag = '"v2"'
    with psycopg.connect(dsn) as conn:
        conn.execute("UPDATE source_monitor_state SET next_check_at=now()-interval '1 second' WHERE source_id=%s", (source_id,))
    second = run_worker()
    with psycopg.connect(dsn) as conn:
        state = conn.execute("SELECT monitor_state,etag FROM source_monitor_state WHERE source_id=%s", (source_id,)).fetchone()
        assert state == ("CHANGED", '"v2"'), state

    Handler.status = 404
    with psycopg.connect(dsn) as conn:
        conn.execute("UPDATE source_monitor_state SET next_check_at=now()-interval '1 second' WHERE source_id=%s", (source_id,))
    third = run_worker()
    Handler.status = 200
    Handler.etag = '"v3"'
    new_url = url + "/rotated"
    with psycopg.connect(dsn) as conn:
        conn.execute("UPDATE source_records SET source_url=%s WHERE source_id=%s", (new_url, source_id))
    fourth = run_worker()
    with psycopg.connect(dsn) as conn:
        state = conn.execute("SELECT monitor_state,http_status,etag,canonical_url FROM source_monitor_state WHERE source_id=%s", (source_id,)).fetchone()
        assert state == ("UNCHANGED", 200, '"v3"', new_url), state
        events = conn.execute("SELECT outcome FROM source_monitor_events WHERE source_id=%s ORDER BY checked_at", (source_id,)).fetchall()
        assert [x[0] for x in events] == ["UNCHANGED", "CHANGED", "MISSING", "UNCHANGED"], events
    print(json.dumps({"first": first["sources"], "second": second["sources"], "third": third["sources"], "fourth": fourth["sources"], "events": [x[0] for x in events]}, indent=2))
    print("PHASE7_SOURCE_MONITOR_PG_SMOKE=PASS")
finally:
    server.shutdown()
    thread.join(timeout=2)
    with psycopg.connect(dsn) as conn:
        conn.execute("DELETE FROM source_monitor_events WHERE source_id=%s", (source_id,))
        conn.execute("DELETE FROM source_monitor_state WHERE source_id=%s", (source_id,))
        conn.execute("DELETE FROM source_records WHERE source_id=%s", (source_id,))
        conn.execute("DELETE FROM content_objects WHERE content_hash=%s", (content_hash,))
