#!/usr/bin/env bash
# Start the staged SPA entry on 127.0.0.1:7801 without replacing the legacy 7800 root.
# Uses an isolated copy of the SQLite platform DB; never points at migrations/platform.db.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STAGE_DIR="${RP_STAGE_DIR:-${REPO}/rip-data/staging}"
STAGE_DB="${RP_STAGE_DB:-${STAGE_DIR}/platform.db}"
STAGE_PORT="${RP_STAGE_PORT:-7801}"
TOKEN_FILE="${RP_AUTH_TOKEN_FILE:-${REPO}/.rp-workbench-token}"

if [[ "${STAGE_DB}" == "${REPO}/migrations/platform.db" ]]; then
  echo "[stage] refusing to use the primary platform.db" >&2
  exit 2
fi
if [[ ! -s "${TOKEN_FILE}" && -z "${RP_AUTH_TOKEN:-}" ]]; then
  echo "[stage] auth token missing; set RP_AUTH_TOKEN or RP_AUTH_TOKEN_FILE" >&2
  exit 2
fi
if [[ ! -d "${REPO}/web/dist" ]]; then
  echo "[stage] web/dist missing; run: cd web && npm run build" >&2
  exit 2
fi

mkdir -p "${STAGE_DIR}"
if [[ ! -f "${STAGE_DB}" ]]; then
  cp "${REPO}/migrations/platform.db" "${STAGE_DB}"
  echo "[stage] initialized isolated DB copy: ${STAGE_DB}"
fi

echo "[stage] UI:      http://127.0.0.1:${STAGE_PORT}/"
echo "[stage] API:     same-origin /api/* (Bearer protected)"
echo "[stage] DB copy: ${STAGE_DB}"
echo "[stage] SPA fallback enabled; old 7800 entry is untouched"
exec env \
  RP_HOST=127.0.0.1 \
  RP_PORT="${STAGE_PORT}" \
  RP_DB_PATH="${STAGE_DB}" \
  RP_STATIC_DIR="${REPO}/web/dist" \
  RP_SPA_FALLBACK=1 \
  RP_AUTH_TOKEN_FILE="${TOKEN_FILE}" \
  python3 "${REPO}/apps/server/rp_server.py"
