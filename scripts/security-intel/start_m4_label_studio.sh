#!/usr/bin/env bash
set -euo pipefail

ROOT="$HOME/.local/share/rip-m4-label-studio"
VENV="$HOME/.cache/rip-m4-label-studio-venv"

if [[ ! -x "$VENV/bin/label-studio" ]]; then
  echo "Label Studio 1.23.2 is not installed in $VENV" >&2
  echo "Install Python 3.13 with uv, then install scripts/security-intel/requirements-m4-review-ui.txt" >&2
  exit 2
fi

mkdir -p "$ROOT/data"
chmod 700 "$ROOT" "$ROOT/data"
if [[ -f "$ROOT/credentials.env" ]]; then
  set -a
  # Local-only owner account. This file is mode 0600 and must never enter Git.
  source "$ROOT/credentials.env"
  set +a
fi

export LABEL_STUDIO_BASE_DATA_DIR="$ROOT/data"
export LABEL_STUDIO_HOST=http://127.0.0.1:8062
export HOST=http://127.0.0.1:8062
export HF_HUB_DISABLE_TELEMETRY=1
export SENTRY_DSN=''
export FRONTEND_SENTRY_DSN=''
export SSRF_PROTECTION_ENABLED=true
export LATEST_VERSION_CHECK=false
export LABEL_STUDIO_ENABLE_LEGACY_API_TOKEN=false
export CSRF_TRUSTED_ORIGINS=http://127.0.0.1:8062

exec "$VENV/bin/label-studio" start \
  --data-dir "$ROOT/data" \
  --internal-host 127.0.0.1 \
  --port 8062 \
  --no-browser
