#!/usr/bin/env bash
set -euo pipefail
cd /media/emo/code/pensting/research-platform
export RP_BENCH_DSN="${RP_TEST_PG_DSN:-postgres://postgres:rip7p8test@127.0.0.1:29317/postgres}"
python3 scripts/rip3_phase8_scale_bench.py \
  --docs 100000 --chunks-per-doc 10 --queries 100 --workers 16 \
  --output-dir docs/rip3/benchmarks
