#!/usr/bin/env python3
"""Read-only adapter from a bounded keyword request to rp-search-unified."""
from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TIMEOUT_SECONDS = 60


def main() -> int:
    if len(sys.argv) not in (3, 4):
        print("usage: rip_unified_search.py <keyword> <limit> [include_candidates=true|false]", file=sys.stderr)
        return 64
    keyword = sys.argv[1].strip()
    if not keyword or len(keyword) > 200:
        print("keyword must contain 1..=200 characters", file=sys.stderr)
        return 64
    try:
        limit = int(sys.argv[2])
    except ValueError:
        print("limit must be an integer", file=sys.stderr)
        return 64
    if not 1 <= limit <= 20:
        print("limit must be between 1 and 20", file=sys.stderr)
        return 64
    include_candidates = len(sys.argv) == 4 and sys.argv[3].strip().lower() == "true"
    if len(sys.argv) == 4 and sys.argv[3].strip().lower() not in {"true", "false"}:
        print("include_candidates must be true or false", file=sys.stderr)
        return 64

    binary = Path(os.environ.get("RP_SEARCH_UNIFIED_BIN", ROOT / "target/debug/rp-search-unified"))
    index_db = Path(os.environ.get("RP_UNIFIED_SEARCH_INDEX_DB", ROOT / "rip-data/derived/curated-poc-search.db"))
    run_db = Path(os.environ.get("RP_SEARCH_UNIFIED_RUN_DB", ROOT / "rip-data/tool-runs.db"))
    workspace = os.environ.get("RP_SEARCH_WORKSPACE", "default")
    if not binary.is_file():
        print(f"rp-search-unified binary not found: {binary}", file=sys.stderr)
        return 69
    if not index_db.is_file():
        print(f"unified search index not found: {index_db}", file=sys.stderr)
        return 69

    argv = [
        str(binary), "--query", keyword, "--workspace", workspace,
        "--limit", str(limit), "--index-db", str(index_db), "--run-db", str(run_db),
        "--manifest-dir", str(ROOT / "containerfiles/rip-embed"),
    ]
    if include_candidates:
        argv.extend(["--sqlite-filters", "include-candidates=true"])
    try:
        proc = subprocess.run(
            argv, cwd=ROOT, capture_output=True, text=True,
            timeout=TIMEOUT_SECONDS, check=False,
        )
    except subprocess.TimeoutExpired:
        print("unified search timed out", file=sys.stderr)
        return 124
    if proc.returncode != 0:
        detail = proc.stderr[-2000:] or proc.stdout[-2000:]
        print(detail or "rp-search-unified failed without diagnostic output", file=sys.stderr)
        return proc.returncode or 1
    # Fail closed on malformed/partial output; the Rust adapter validates it again.
    import json
    try:
        payload = json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        print(f"rp-search-unified returned invalid JSON: {exc}", file=sys.stderr)
        return 65
    if not isinstance(payload, dict) or not isinstance(payload.get("hits"), list) or not isinstance(payload.get("channels"), dict):
        print("rp-search-unified response misses hits/channels", file=sys.stderr)
        return 65
    print(json.dumps(payload, ensure_ascii=False, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
