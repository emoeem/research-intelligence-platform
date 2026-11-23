#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/.."
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/home/emo/rt3-target}"
export RP_TEST_PG_DSN="${RP_TEST_PG_DSN:-${RP_PG_DSN:-postgres://postgres:rip7p8test@127.0.0.1:29317/postgres}}"
export RP_PG_DSN="$RP_TEST_PG_DSN"

echo "== Phase 7/8 Rust format (intel-db scope; unrelated workspace files untouched) =="
cargo fmt -p intel-db -- --check

echo "== Phase 7 PostgreSQL integration =="
cargo test --offline -p intel-db --lib --test phase7_maintenance -- --test-threads=1

echo "== Phase 7 Clippy =="
cargo clippy --offline -p intel-db --all-targets -- -D warnings

echo "== Phase 7 Python worker tests =="
python3 -m unittest scripts.tests.test_rip3_source_monitor -v
python3 -m py_compile scripts/rip3_source_monitor.py scripts/rip3_phase8_scale_bench.py scripts/rip3_phase8_sqlite_compare.py

if ! pg_isready -d "$RP_TEST_PG_DSN" >/dev/null 2>&1; then
  echo "ERROR: PostgreSQL is required for Phase 7 acceptance (RP_TEST_PG_DSN unavailable)" >&2
  exit 1
fi
echo "== Phase 7 PostgreSQL source monitor smoke =="
RIP3_SOURCE_MONITOR_ALLOW_PRIVATE=1 python3 scripts/tests/phase7_source_monitor_pg_smoke.py
echo "== Phase 7 RTIS read-only POC reference smoke =="
python3 scripts/tests/phase7_rtis_monitor_smoke.py

if [[ "${RUN_PHASE8_SCALE_BENCH:-0}" == "1" ]]; then
  echo "== Phase 8 PostgreSQL 100k/1M benchmark =="
  bash scripts/tests/run_phase8_pg_full.sh
  echo "== Phase 8 SQLite WAL 100k/1M comparison =="
  python3 scripts/rip3_phase8_sqlite_compare.py --docs 100000 --chunks-per-doc 10 --queries 100 --workers 16 --output-dir docs/rip3/benchmarks
else
  echo "== Phase 8 benchmark artifact verification =="
  python3 - <<'PY'
import json
from pathlib import Path
root = Path("docs/rip3/benchmarks")
pg = sorted(root.glob("phase8-scale-*.json"), key=lambda p: p.stat().st_mtime, reverse=True)
sq = sorted(root.glob("phase8-sqlite-*.json"), key=lambda p: p.stat().st_mtime, reverse=True)
assert pg and sq, "missing Phase 8 PostgreSQL or SQLite benchmark report"
p, s = json.loads(pg[0].read_text()), json.loads(sq[0].read_text())
assert p["operations"]["counts"]["documents"] == 100_000
assert p["operations"]["counts"]["chunks"] == 1_000_000
assert p["operations"]["counts"]["ops_events"] == 4_000
assert p["operations"]["backup_restore"]["verified"] is True
assert s["operations"]["counts"]["documents"] == 100_000
assert s["operations"]["counts"]["chunks"] == 1_000_000
assert s["operations"]["backup_restore"]["verified"] is True
print(f"PostgreSQL report: {pg[0]}")
print(f"SQLite report: {sq[0]}")
PY
fi

echo "PHASE7_PHASE8_SELFTEST=PASS"
