#!/usr/bin/env python3
"""Disposable workspace-scoped RTIS reference smoke; never reads POC bodies."""
from __future__ import annotations

import json
import os
import sqlite3
import subprocess
import uuid
from pathlib import Path

import psycopg

root = Path(__file__).resolve().parents[2]
fixture = Path("/tmp/rip3-rtis-readonly-fixture.db")
fixture.unlink(missing_ok=True)
db = sqlite3.connect(fixture)
db.execute("CREATE TABLE poc(id INTEGER PRIMARY KEY, verified INTEGER, updated_at TEXT, content TEXT)")
db.execute("INSERT INTO poc VALUES(42,0,'2024-01-01T00:00:00Z','DO NOT READ THIS BODY')")
db.commit()
db.close()

env = os.environ.copy()
dsn = env.get("RP_TEST_PG_DSN", "postgres://postgres:rip7p8test@127.0.0.1:29317/postgres")
env["RP_PG_DSN"] = dsn
env["RTIS_KNOWLEDGE_DB"] = str(fixture)
tag = uuid.uuid4().hex
workspace = f"ws-rtis-smoke-{tag}"
task_id = f"task-rtis-smoke-{tag}"
finding_id = f"find-rtis-smoke-{tag}"

try:
    with psycopg.connect(dsn) as conn:
        conn.execute(
            "INSERT INTO research_tasks(task_id,workspace_id) VALUES (%s,%s)",
            (task_id, workspace),
        )
        conn.execute(
            """INSERT INTO findings
               (task_id,finding_id,rtis_ref,title,severity,result_state,finding_snapshot,evidence_refs)
               VALUES (%s,%s,%s,%s,%s,%s,%s,%s)""",
            (task_id, finding_id, "rtis://poc/42", "RTIS monitor fixture", "low", "confirmed", "{}", "[]"),
        )

    proc = subprocess.run(
        ["python3", str(root / "scripts/rip3_source_monitor.py"), "--workspace", workspace, "--rtis-db", str(fixture)],
        cwd=root, env=env, text=True, capture_output=True,
    )
    print(proc.stdout)
    if proc.returncode:
        print(proc.stderr)
        raise SystemExit(proc.returncode)
    result = json.loads(proc.stdout)
    assert result["rtis_refs"] == {"checked": 1, "missing": 0, "stale": 1, "unverified": 1, "skipped": 0}, result

    # A different workspace must not see this finding or emit a maintenance row for it.
    other = subprocess.run(
        ["python3", str(root / "scripts/rip3_source_monitor.py"), "--workspace", f"ws-empty-{tag}", "--rtis-db", str(fixture)],
        cwd=root, env=env, text=True, capture_output=True, check=True,
    )
    other_result = json.loads(other.stdout)
    assert other_result["rtis_refs"]["checked"] == 0, other_result
    print("PHASE7_RTIS_READONLY_WORKSPACE_SMOKE=PASS")
finally:
    with psycopg.connect(dsn) as conn:
        conn.execute("DELETE FROM maintenance_findings WHERE workspace_id=%s", (workspace,))
        conn.execute("DELETE FROM maintenance_runs WHERE workspace_id=%s", (workspace,))
        conn.execute("DELETE FROM findings WHERE task_id=%s", (task_id,))
        conn.execute("DELETE FROM research_tasks WHERE task_id=%s", (task_id,))
        conn.execute("DELETE FROM maintenance_findings WHERE workspace_id=%s", (f"ws-empty-{tag}",))
        conn.execute("DELETE FROM maintenance_runs WHERE workspace_id=%s", (f"ws-empty-{tag}",))
    fixture.unlink(missing_ok=True)
