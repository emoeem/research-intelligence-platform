#!/usr/bin/env bash
# P1 Batch A/B acceptance probe. It reports fail/blocked honestly and never
# substitutes synthetic fixtures or an alternate parser image for acceptance.
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO"

PASS=0
FAIL=0
BLOCK=0

ck() {
  local name="$1"
  local actual="$2"
  local expected="$3"
  if [[ "$actual" == "$expected" ]]; then
    PASS=$((PASS + 1))
    echo "[PASS] $name"
  else
    FAIL=$((FAIL + 1))
    echo "[FAIL] $name (expected=$expected actual=$actual)"
  fi
}

blocked() {
  local name="$1"
  local detail="$2"
  BLOCK=$((BLOCK + 1))
  echo "[BLOCKED] $name: $detail"
}

PARSER_DIGEST="sha256:9a59eef52bf80aeb2562d9ca32cada455946c775fe92a0b0fb044a527dd8bf7a"

echo "== P1 Batch A: execution capability"
for tool in podman python3 pdftotext pdfinfo tesseract libreoffice pandoc; do
  if command -v "$tool" >/dev/null 2>&1; then
    echo "[PASS] host $tool: $(command -v "$tool")"
  else
    blocked "host $tool" "not installed"
  fi
done

if podman image inspect "localhost/rip-parser:latest@${PARSER_DIGEST}" >/dev/null 2>&1; then
  actual="$(podman image inspect "localhost/rip-parser:latest@${PARSER_DIGEST}" --format '{{.Digest}}')"
  ck "exact rip-parser image identity" "$actual" "$PARSER_DIGEST"
else
  blocked "exact rip-parser image identity" "localhost/rip-parser@${PARSER_DIGEST} is unknown"
fi

CORPUS_DIR="${P1_GOLDEN_CORPUS_DIR:-}"
if [[ -z "$CORPUS_DIR" || ! -d "$CORPUS_DIR" ]]; then
  blocked "real Golden Corpus" "set P1_GOLDEN_CORPUS_DIR to a source-reviewed, hashed corpus"
else
  count="$(find "$CORPUS_DIR" -type f | wc -l)"
  if [[ "$count" -ge 40 ]]; then
    ck "real Golden Corpus size" "$count" ">=40"
  else
    blocked "real Golden Corpus size" "found $count files; require at least 40"
  fi
  if [[ -f "$CORPUS_DIR/MANIFEST.json" ]]; then
    PASS=$((PASS + 1))
    echo "[PASS] Golden Corpus manifest present"
  else
    blocked "Golden Corpus manifest" "MANIFEST.json with sha256, license/source and expected structure is required"
  fi
fi

echo "== P1 parser pipeline local logic"
if cargo test --offline -p intel-db --test parser_pipeline -- --test-threads=1 >/tmp/rip-p1-parser.log 2>&1; then
  PASS=$((PASS + 1))
  echo "[PASS] parser pipeline test command"
else
  FAIL=$((FAIL + 1))
  echo "[FAIL] parser pipeline test command"
  tail -20 /tmp/rip-p1-parser.log
fi

echo "== P1 Batch B: isolated SQLite recovery"
if bash apps/server/selftest_phase10_backup.sh >/tmp/rip-p1-backup.log 2>&1; then
  PASS=$((PASS + 1))
  echo "[PASS] SQLite backup/restore/FTS/retention roundtrip"
else
  FAIL=$((FAIL + 1))
  echo "[FAIL] SQLite backup/restore/FTS/retention roundtrip"
  tail -30 /tmp/rip-p1-backup.log
fi

echo
echo "PASS=$PASS FAIL=$FAIL BLOCKED=$BLOCK"
if [[ "$FAIL" -gt 0 || "$BLOCK" -gt 0 ]]; then
  exit 1
fi
