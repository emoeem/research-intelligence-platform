#!/usr/bin/env python3
"""Automated Workflow checkpoint recovery drill using an isolated SQLite database."""
from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WF_BIN = ROOT / "target/debug/rp-workflow"
TOOL_BIN = ROOT / "target/debug/rp-tool"
SAMPLE = ROOT / "workflows/parse-pipeline-v1.json"
DOC_PARSE_MANIFEST = ROOT / "manifests/tools/rip.doc.parse.json"


def run(args: list[str]) -> dict:
    proc = subprocess.run(
        args,
        cwd=ROOT,
        capture_output=True,
        text=True,
        timeout=30,
    )
    if proc.returncode != 0:
        return {"ok": False, "exit": proc.returncode,
                "stdout": proc.stdout[-2000:], "stderr": proc.stderr[-2000:]}
    try:
        return {"ok": True, "payload": json.loads(proc.stdout)}
    except json.JSONDecodeError:
        return {"ok": False, "exit": proc.returncode,
                "stdout": proc.stdout[-2000:], "stderr": "stdout was not JSON"}


def main() -> int:
    work = Path(tempfile.mkdtemp(prefix="rip-workflow-recovery-"))
    db = work / "workflow.db"
    try:
        registered = run([
            str(TOOL_BIN),
            "register",
            "--manifest", str(DOC_PARSE_MANIFEST),
            "--db", str(db),
        ])
        if not registered["ok"]:
            print(json.dumps({"status": "blocked", "gate": "tool_register",
                             "detail": registered}, ensure_ascii=False))
            return 1

        put = run([str(WF_BIN), "put", str(SAMPLE), "--db", str(db)])
        if not put["ok"]:
            print(json.dumps({"status": "blocked", "gate": "put", "detail": put},
                             ensure_ascii=False))
            return 1

        clean = work / "clean.md"
        clean.write_text("# Recovery Drill\n\nA deterministic paragraph.\n",
                         encoding="utf-8")
        started = run([
            str(WF_BIN), "run", "wf-parse-pipeline-v1",
            "--input", json.dumps({
                "container_path": "/task/in/clean.md",
                "input_host_files": {"clean.md": str(clean)},
            }),
            "--db", str(db),
        ])
        if not started["ok"]:
            print(json.dumps({"status": "blocked", "gate": "run", "detail": started},
                             ensure_ascii=False))
            return 1

        payload = started["payload"]
        run_id = payload.get("run_id")
        if not run_id:
            print(json.dumps({"status": "blocked", "gate": "run_id_missing"},
                             ensure_ascii=False))
            return 1

        # Simulate process/service restart by closing this subprocess and opening
        # a fresh CLI status call against the persisted SQLite run.
        status = run([str(WF_BIN), "status", run_id, "--db", str(db)])
        result = {
            "status": "pass" if status["ok"] else "blocked",
            "run_id": run_id,
            "restart_status": status,
            "database": str(db),
        }
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return 0 if status["ok"] else 1
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main())
