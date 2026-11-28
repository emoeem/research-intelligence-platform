#!/usr/bin/env bash
# PB-007 HTTPS 切换演练（本地 TLS 全链，零碰生产文件）：
#   一次性主库克隆（rp_server HTTP）→ stdlib TLS 终结反代（自签证书）→
#   rip_production_probe 在原生 TLS 下探活（/api/overview + 根 + 证书）→
#   probe 拒绝 http:// 与缺 URL（BLOCKED 边界）→ 生产 platform*.db 指纹红线。
# 证书为自签演练物（openssl，SAN=127.0.0.1），不装系统信任，经
# RP_PRODUCTION_CACERT 注入探针。
# 全过 exit 0；任何 FAIL exit 1。
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO}"

PASS=0; FAIL=0
ck() { if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi; }

PROBE="scripts/rip_production_probe.sh"
command -v openssl >/dev/null 2>&1 || { echo "SKIP: openssl unavailable"; exit 0; }
command -v curl >/dev/null 2>&1 || { echo "SKIP: curl unavailable"; exit 0; }

WORK="$(mktemp -d /tmp/rp_https.XXXX)"
trap 'pkill -f "rp_server.py.*rp-https-drill" 2>/dev/null; pkill -f "https_proxy_probe.py" 2>/dev/null; if [[ -n ${RP_KEEP_WORK:-} ]]; then echo "[keep] WORK=${WORK}"; else chmod -R u+rwX "${WORK}" 2>/dev/null; rm -rf "${WORK}"; fi' EXIT
FP_BEFORE=$(for f in migrations/platform*.db; do [[ -e "$f" ]] && sha256sum "$f" | awk '{print substr($1,1,16) }'; done | sort | sha256sum | awk '{print $1}')

echo "== 1. 一次性主库克隆 + 证书"
cp migrations/platform.db "${WORK}/drill.db" 2>/dev/null || { echo "[FAIL] platform.db 克隆缺源"; exit 1; }
openssl req -x509 -newkey rsa:2048 -nodes -days 2 \
  -subj "/CN=127.0.0.1" -addext "subjectAltName=IP:127.0.0.1" \
  -keyout "${WORK}/tls.key" -out "${WORK}/tls.crt" >/dev/null 2>&1
ck "自签证书生成" "0" "$?"

echo "== 2. rp_server（HTTP 上游，一次性库）"
HTTP_PORT=$(( 28500 + RANDOM % 800 ))
TLS_PORT=$(( HTTP_PORT + 1 ))
RP_PORT="$HTTP_PORT" RP_DB_PATH="${WORK}/drill.db" RP_AUTH_TOKEN="https-drill-token" \
  python3 -I apps/server/rp_server.py > "${WORK}/server.log" 2>&1 &
SRV_PID=$!
sleep 3
curl -fsS -H "Authorization: Bearer https-drill-token" "http://127.0.0.1:${HTTP_PORT}/api/overview" >/dev/null 2>&1
ck "HTTP 上游 /api/overview 可用" "0" "$?"

echo "== 3. TLS 终结反代（stdlib 夹具）"
python3 -I apps/server/https_proxy_probe.py \
  --listen-port "$TLS_PORT" --cert "${WORK}/tls.crt" --key "${WORK}/tls.key" \
  --upstream-port "$HTTP_PORT" > "${WORK}/proxy.log" 2>&1 &
PRX_PID=$!
sleep 2
curl -fsS --cacert "${WORK}/tls.crt" "https://127.0.0.1:${TLS_PORT}/api/overview" -H "Authorization: Bearer https-drill-token" >/dev/null 2>&1
ck "HTTPS 反代链路可达（curl + CA 锚）" "0" "$?"

echo "== 4. rip_production_probe 原生 TLS 探活"
RP_PRODUCTION_URL="https://127.0.0.1:${TLS_PORT}" \
RP_PRODUCTION_CACERT="${WORK}/tls.crt" \
RP_PROBE_PORT="$TLS_PORT" \
RP_PROBE_TOKEN="https-drill-token" \
bash "$PROBE" > "${WORK}/probe.out" 2>&1
RC=$?
ck "probe exit 0" "0" "$RC"
grep -q "PRODUCTION_PROBE=PASS" "${WORK}/probe.out" && ck "PRODUCTION_PROBE=PASS 文案" "0" "0" || ck "PRODUCTION_PROBE=PASS 文案" "0" "1"
grep -q "/api/overview returned success" "${WORK}/probe.out" && ck "探针覆盖 /api/overview" "0" "0" || ck "探针覆盖 /api/overview" "0" "1"
grep -qE "subject|issuer|notBefore" "${WORK}/probe.out" && ck "探针透出证书主体" "0" "0" || ck "探针透出证书主体" "0" "1"

echo "== 5. probe 边界（缺 URL / http:// 必须拒绝）"
env -u RP_PRODUCTION_URL bash "$PROBE" > "${WORK}/p1.out" 2>&1
ck "缺 URL exit 1" "1" "$?"
RP_PRODUCTION_URL="http://127.0.0.1:${HTTP_PORT}" bash "$PROBE" > "${WORK}/p2.out" 2>&1
ck "http:// exit 1" "1" "$?"

kill "$SRV_PID" "$PRX_PID" 2>/dev/null
FP_AFTER=$(for f in migrations/platform*.db; do [[ -e "$f" ]] && sha256sum "$f" | awk '{print substr($1,1,16) }'; done | sort | sha256sum | awk '{print $1}')
ck "migrations/platform*.db 指纹零写一致" "$FP_BEFORE" "$FP_AFTER"

echo "== 自验汇总: PASS=$PASS FAIL=$FAIL"
[[ "$FAIL" -eq 0 ]] || exit 1
