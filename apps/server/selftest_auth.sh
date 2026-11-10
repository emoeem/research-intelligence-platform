#!/usr/bin/env bash
# M7 鉴权自验：起服务（临时令牌 + 临时端口），断言鉴权/只读/数据对账/静态安全。
# 输出每项 PASS/FAIL，全过 exit 0。只读操作，不触碰源库。
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "${HERE}/../.." && pwd)"
PORT="${SELFTEST_PORT:-7891}"
TOKEN="selftest-$(python3 -c 'import secrets; print(secrets.token_hex(16))')"
LOG="$(mktemp /tmp/rp_selftest.XXXX.log)"

PASS=0; FAIL=0
ck() { # ck <名> <期望> <实际>
  if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi
}

RP_PORT="${PORT}" RP_AUTH_TOKEN="${TOKEN}" python3 "${HERE}/rp_server.py" > "${LOG}" 2>&1 &
SRV=$!
trap 'kill ${SRV} 2>/dev/null; wait ${SRV} 2>/dev/null; rm -f "${LOG}"' EXIT
for _ in $(seq 1 30); do curl -sf "http://127.0.0.1:${PORT}/api/healthz" >/dev/null 2>&1 && break; curl -s -o /dev/null "http://127.0.0.1:${PORT}/" && break; sleep 0.2; done

# 1. 无令牌 → 401 + WWW-Authenticate
code=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:${PORT}/api/stats")
ck "无令牌访问 API 返回 401" "401" "${code}"
www=$(curl -si "http://127.0.0.1:${PORT}/api/stats" | grep -i "^WWW-Authenticate:" | tr -d "\r")
ck "401 携带 WWW-Authenticate: Bearer" "WWW-Authenticate: Bearer" "${www}"

# 2. 错误令牌 → 401
code=$(curl -s -o /dev/null -w "%{http_code}" -H "Authorization: Bearer wrong-token" "http://127.0.0.1:${PORT}/api/stats")
ck "错误令牌返回 401" "401" "${code}"

# 3. 正确令牌 → 200，且数字与库直查一致
body=$(curl -s -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}/api/stats")
api_ev=$(echo "${body}" | python3 -c "import sys,json; print(json.load(sys.stdin).get('evidence',{}).get('total','MISSING'))" 2>/dev/null || echo PARSE_FAIL)
db_ev=$(python3 -c "import sqlite3; print(sqlite3.connect('file:${REPO}/migrations/platform.db?mode=ro',uri=True).execute('SELECT COUNT(*) FROM evidence').fetchone()[0])")
ck "evidence 总数 API=库直查（fullbatch=700）" "${db_ev}" "${api_ev}"
code=$(curl -s -o /dev/null -w "%{http_code}" -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}/api/tasks")
ck "正确令牌访问 tasks 返回 200" "200" "${code}"

# 4. 静态页无需令牌（登录流程可用）
code=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:${PORT}/login.html")
ck "登录页无令牌可访问" "200" "${code}"
code=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:${PORT}/")
ck "首页静态资源无令牌可访问" "200" "${code}"

# 5. 只读语义保持
code=$(curl -s -o /dev/null -w "%{http_code}" -X POST -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}/api/stats")
ck "POST 仍为 405" "405" "${code}"

# 6. 路径穿越防护保持
code=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:${PORT}/../../etc/passwd")
ck "路径穿越仍被拒" "404" "${code}"

# 7. 鉴权关闭模式（开发态）：清空令牌来源重启
# 2026-11-10 修正：仓库根 .rp-workbench-token 存在时，原「只清环境变量」的
# 重启仍会从默认令牌文件加载令牌（401），永远进不了无鉴权分支；显式把
# RP_AUTH_TOKEN_FILE 指到空文件才真正清空来源。测试架修复，服务端行为不变。
kill "${SRV}" 2>/dev/null; wait "${SRV}" 2>/dev/null
EMPTY_TOKEN_FILE="$(mktemp /tmp/rp_empty_token.XXXX)"
trap 'kill ${SRV} 2>/dev/null; wait ${SRV} 2>/dev/null; rm -f "${LOG}" "${EMPTY_TOKEN_FILE}"' EXIT
RP_PORT="${PORT}" RP_DB_PATH="${REPO}/migrations/platform.db" \
  RP_AUTH_TOKEN="" RP_AUTH_TOKEN_FILE="${EMPTY_TOKEN_FILE}" \
  python3 "${HERE}/rp_server.py" > "${LOG}" 2>&1 &
SRV=$!
sleep 0.6
code=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:${PORT}/api/stats")
ck "无令牌配置时为开发模式（API 可用）" "200" "${code}"
grep -q "鉴权未启用" "${LOG}" && ck "开发模式启动输出显著告警" "yes" "yes" || ck "开发模式启动输出显著告警" "yes" "no"

echo "----"
echo "结果：PASS=${PASS} FAIL=${FAIL}"
[[ ${FAIL} -eq 0 ]]
