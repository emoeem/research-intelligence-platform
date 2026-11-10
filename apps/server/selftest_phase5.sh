#!/usr/bin/env bash
# Phase 5 只读 API 增量自验：起服务（临时令牌 + 临时端口），断言五端点数据
# 对账、aux 通道守卫降级、既有行为零回归。只读操作，不触碰任何源库。
# 输出每项 PASS/FAIL，全过 exit 0。
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "${HERE}/../.." && pwd)"
MIGRATIONS="${REPO}/migrations"
PORT="${SELFTEST_PORT:-7892}"
AUX_DB="${RP_AUX_DB_SELFTEST:-${MIGRATIONS}/dev-b5.db}"
TOKEN="selftest-$(python3 -c 'import secrets; print(secrets.token_hex(16))')"
LOG="$(mktemp /tmp/rp_p5_selftest.XXXX.log)"

PASS=0; FAIL=0
ck() { # ck <名> <期望> <实际>
  if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi
}
api() { # api <path> -> body
  curl -s -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}${1}"
}
api_code() { # api_code <path> [curl args...] -> http_code
  local path="$1"; shift
  curl -s -o /dev/null -w "%{http_code}" -H "Authorization: Bearer ${TOKEN}" "$@" "http://127.0.0.1:${PORT}${path}"
}
dbcount() { # dbcount <db> <table> -> 行数（表不存在输出 -1）
  python3 - "$1" "$2" <<'PYEOF'
import sqlite3, sys
conn = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
try:
    print(conn.execute(f"SELECT COUNT(*) FROM {sys.argv[2]}").fetchone()[0])
except sqlite3.Error:
    print(-1)
PYEOF
}
jget() { # jget <json> <python 表达式，变量 d> -> 值
  python3 -c "import sys,json; d=json.loads(sys.argv[1]); print(eval(sys.argv[2]))" "$1" "$2" 2>/dev/null || echo PARSE_FAIL
}

RP_PORT="${PORT}" RP_AUTH_TOKEN="${TOKEN}" RP_AUX_DB="${AUX_DB}" \
  python3 "${HERE}/rp_server.py" > "${LOG}" 2>&1 &
SRV=$!
trap 'kill ${SRV} 2>/dev/null; wait ${SRV} 2>/dev/null; rm -f "${LOG}"' EXIT
for _ in $(seq 1 30); do curl -s -o /dev/null "http://127.0.0.1:${PORT}/" && break; sleep 0.2; done

# ---- 1. 鉴权与只读语义不回归（增量端点同样受 Bearer 管控）----
code=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:${PORT}/api/overview")
ck "无令牌访问 /api/overview 返回 401" "401" "${code}"
code=$(api_code "/api/overview" -X POST)
ck "POST /api/overview 仍为 405（只读语义）" "405" "${code}"

# ---- 2. 主库 M 面对账：knowledge / tasks ----
body=$(api "/api/knowledge?limit=5")
ck "/api/knowledge total=库直查" "$(dbcount "${MIGRATIONS}/platform.db" knowledge)" "$(jget "${body}" "d['total']")"
ck "/api/knowledge available=true source=main" "True main" "$(jget "${body}" "d['available']") $(jget "${body}" "d['source']")"
body=$(api "/api/tasks?limit=1")
ck "/api/tasks total=库直查（42 存量）" "$(dbcount "${MIGRATIONS}/platform.db" research_task)" "$(jget "${body}" "d['total']")"

# ---- 3. 工具目录：列表主库注册面，详情 Schema 与 tool_version ----
body=$(api "/api/tools?limit=3")
ck "/api/tools total=库直查（20 注册面）" "$(dbcount "${MIGRATIONS}/platform.db" tool)" "$(jget "${body}" "d['total']")"
ck "/api/tools source=main" "main" "$(jget "${body}" "d['source']")"
detail=$(api "/api/tools?tool_id=nmap")
ck "/api/tools 详情含 tool/input_schema/output_schema/versions 字段" \
  "True True True True" \
  "$(jget "${detail}" "'tool' in d") $(jget "${detail}" "'input_schema' in d") $(jget "${detail}" "'output_schema' in d") $(jget "${detail}" "'versions' in d")"
ck "/api/tools nmap 详情 tool_version 数=库直查" \
  "$(python3 -c "import sqlite3; print(sqlite3.connect('file:${MIGRATIONS}/platform.db?mode=ro',uri=True).execute(\"SELECT COUNT(*) FROM tool_version WHERE tool_id='nmap'\").fetchone()[0])")" \
  "$(jget "${detail}" "len(d['versions'])")"
detail=$(api "/api/tools?tool_id=rip.kb.rtis")
ck "/api/tools 契约工具详情回退 aux 且 input_schema 为真实 JSON Schema" \
  "aux dict" "$(jget "${detail}" "d['source']") $(jget "${detail}" "type(d['input_schema']).__name__")"
code=$(api_code "/api/tools?tool_id=no-such-tool")
ck "/api/tools 未知 tool_id 返回 404" "404" "${code}"

# ---- 4. Run 面：aux 通道 total 对账 + 详情 artifact 列表 ----
body=$(api "/api/runs")
ck "/api/runs total=aux 库直查" "$(dbcount "${AUX_DB}" tool_run)" "$(jget "${body}" "d['total']")"
ck "/api/runs source=aux" "aux" "$(jget "${body}" "d['source']")"
run_id=$(jget "${body}" "d['items'][0]['run_id']")
detail=$(api "/api/runs?run_id=${run_id}")
ck "/api/runs 详情 run_id 一致" "${run_id}" "$(jget "${detail}" "d['run']['run_id']")"
ck "/api/runs 详情含 artifacts 列表字段" "True" "$(jget "${detail}" "isinstance(d['artifacts'], list)")"
code=$(api_code "/api/runs?run_id=no-such-run")
ck "/api/runs 未知 run_id 返回 404" "404" "${code}"
code=$(api_code "/api/runs?status=BOGUS")
ck "/api/runs 非法 status 返回 400" "400" "${code}"

# ---- 5. Runtime 面：表在（空数据为真实空态）----
body=$(api "/api/runtime")
ck "/api/runtime available=true（表存在）" "True" "$(jget "${body}" "d['available']")"
ck "/api/runtime total=aux 库直查" "$(dbcount "${AUX_DB}" runtime_node)" "$(jget "${body}" "d['total']")"

# ---- 6. Overview 聚合 ----
body=$(api "/api/overview")
ck "/api/overview knowledge/tasks/tools/runs 对账" \
  "$(dbcount "${MIGRATIONS}/platform.db" knowledge) $(dbcount "${MIGRATIONS}/platform.db" research_task) True $(dbcount "${AUX_DB}" tool_run)" \
  "$(jget "${body}" "d['knowledge']['total']") $(jget "${body}" "d['tasks']['total']") $(jget "${body}" "d['tools']['available']") $(jget "${body}" "d['runs']['total']")"
ck "/api/overview aux 挂载状态上报 attached=true" "True" "$(jget "${body}" "d['database']['aux']['attached']")"

# ---- 7. 既有四端点不受 aux 挂载影响 ----
code=$(api_code "/api/stats")
ck "既有 /api/stats 仍 200" "200" "${code}"
code=$(api_code "/api/resources?limit=1")
ck "既有 /api/resources 仍 200" "200" "${code}"

# ---- 8. 无 RP_AUX_DB：守卫降级为真实空态说明，既有端点不受影响 ----
kill "${SRV}" 2>/dev/null; wait "${SRV}" 2>/dev/null
RP_PORT="${PORT}" RP_AUTH_TOKEN="${TOKEN}" python3 "${HERE}/rp_server.py" > "${LOG}" 2>&1 &
SRV=$!
sleep 0.6
body=$(api "/api/runs")
ck "无 RP_AUX_DB 时 /api/runs available=false" "False" "$(jget "${body}" "d['available']")"
ck "无 RP_AUX_DB 时 reason=aux_db_not_configured" "aux_db_not_configured" "$(jget "${body}" "d['reason']")"
ck "无 RP_AUX_DB 时 items 为空且非假数据" "0" "$(jget "${body}" "len(d['items'])")"
body=$(api "/api/runtime")
ck "无 RP_AUX_DB 时 /api/runtime 同样降级" "aux_db_not_configured" "$(jget "${body}" "d['reason']")"
body=$(api "/api/tools?limit=1")
ck "无 RP_AUX_DB 时 /api/tools 仍读主库注册面" "$(dbcount "${MIGRATIONS}/platform.db" tool)" "$(jget "${body}" "d['total']")"
code=$(api_code "/api/stats")
ck "无 RP_AUX_DB 时既有 /api/stats 仍 200" "200" "${code}"

echo "----"
echo "结果：PASS=${PASS} FAIL=${FAIL}"
[[ ${FAIL} -eq 0 ]]
