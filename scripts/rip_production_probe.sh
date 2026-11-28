#!/usr/bin/env bash
# Read-only production entry probe. Requires RP_PRODUCTION_URL.
# 可选 RP_PRODUCTION_CACERT：私有 CA 场景给 curl 提供信任锚（PB-007 本地
# TLS 切换演练用；缺省走系统信任库）。另可选 RP_PROBE_PORT 覆盖 :443 缺省
# （非标端口演练用）。零写操作，不携带凭据。
set -euo pipefail

url="${RP_PRODUCTION_URL:-}"
cacert="${RP_PRODUCTION_CACERT:-}"
probe_port="${RP_PROBE_PORT:-443}"
probe_token="${RP_PROBE_TOKEN:-}"
if [[ -z "$url" ]]; then
  echo "[BLOCKED] RP_PRODUCTION_URL is unset"
  exit 1
fi
if [[ "$url" != https://* ]]; then
  echo "[BLOCKED] production URL must use HTTPS"
  exit 1
fi

CURL_TRUST=()
if [[ -n "$cacert" ]]; then
  if [[ ! -f "$cacert" ]]; then
    echo "[BLOCKED] RP_PRODUCTION_CACERT does not exist: $cacert"
    exit 1
  fi
  CURL_TRUST=(--cacert "$cacert")
fi
# 鉴权部署的可选只读探针凭据（GET 与写同面鉴权；token 值永不回显）。
CURL_AUTH=()
if [[ -n "$probe_token" ]]; then
  CURL_AUTH=(-H "Authorization: Bearer ${probe_token}")
fi

host="${url#https://}"
host="${host%%/*}"
host="${host%%:*}"

headers="$(mktemp)"
body="$(mktemp)"
trap 'rm -f "$headers" "$body"' EXIT

if ! curl "${CURL_TRUST[@]}" "${CURL_AUTH[@]}" -fsS --max-time 15 -D "$headers" -o "$body" "$url/api/overview"; then
  echo "[FAIL] /api/overview did not return success"
  exit 1
fi

echo "[PASS] /api/overview returned success"
if [[ -n "$cacert" ]]; then
  curl "${CURL_TRUST[@]}" "${CURL_AUTH[@]}" -fsS --max-time 15 "$url" -o /dev/null
else
  curl -kfsS --max-time 15 -H "Authorization: Bearer ${probe_token}" "$url" -o /dev/null
fi
echo "[PASS] production root returned success"

openssl s_client -connect "${host}:${probe_port}" -servername "${host}" </dev/null 2>/dev/null |
  openssl x509 -noout -subject -issuer -dates

echo "PRODUCTION_PROBE=PASS"
