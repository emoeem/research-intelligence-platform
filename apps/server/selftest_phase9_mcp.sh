#!/usr/bin/env bash
# P9-1 自验脚本（ROADMAP §10.3 P9-1 判据的 CLI/API 层等价断言）：
# 真实 rp-mcp 二进制端到端（stdio NDJSON JSON-RPC 2.0）+ 无令牌握手被拒
# （RP_MCP_TOKEN 未配置 -> -32001）+ 初始化响应不含工具全文 + tools/list
# 分页游标遍历不重不漏 + 能力面在场（rip.run_tool / rip.search_knowledge /
# rip.get_run_status / rip.list_tools / rip.get_tool_schema）+ tools/list
# 注册面与 Tool Center（GET /api/tools，真实 rp_server 进程）逐条对账一致
# （同源：同一 tool 表读面，aux 增量合并同 handle_tools 口径）+ 按需 Schema
# 与 /api/tools?tool_id= 同源对账 + 未接线工具结构化 isError（不伪造成功）
# + 生产库零写核对（URI mode=ro 指纹）。输出 PASS/FAIL，全过 exit 0。
#
# 前置：仓库根运行（cargo 门禁按 CWD 发现 vendor 配置）；rp-mcp 由 cargo
# --offline 构建；临时端口默认 7800（ask 点名的 curl 形态，可 SELFTEST_PORT
# 覆盖）；生产 migrations/platform*.db 只读（红线 ROADMAP §13）。
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO}"

PORT="${SELFTEST_PORT:-7800}"
API_TOKEN="rp-api-selftest-$(python3 -c 'import secrets; print(secrets.token_hex(16))')"
MCP_TOKEN="rp-mcp-selftest-$(python3 -c 'import secrets; print(secrets.token_hex(16))')"
AUX_DB="${RIP_P91_AUX_DB:-migrations/dev-b5.db}"
SRV_LOG="$(mktemp /tmp/rp_p91_server.XXXX.log)"
MCP_BIN="target/debug/rp-mcp"
WORK="$(mktemp -d /tmp/rp_p91_selftest.XXXX)"
RUN_DB="$(mktemp /tmp/rp_p91_run.XXXX.db)"

PASS=0; FAIL=0
ck() { # ck <名> <期望> <实际>
  if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi
}

# 生产库指纹：user_version + 逐表行数（URI mode=ro，只读核对零写红线）。
db_fingerprint() {
  python3 -I - "$1" <<'FPEOF'
import sqlite3, sys, json
c = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
uv = c.execute("PRAGMA user_version").fetchone()[0]
tables = sorted(r[0] for r in c.execute(
    "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'"))
counts = {t: c.execute(f"SELECT COUNT(*) FROM \"{t}\"").fetchone()[0] for t in tables}
print(json.dumps({"user_version": uv, "row_counts": counts}, sort_keys=True))
c.close()
FPEOF
}

# 单请求进出：把请求行喂给 rp-mcp（带令牌），输出响应行。
mcp_req() { # mcp_req <请求行> -> 响应行
  RP_MCP_TOKEN="${MCP_TOKEN}" RP_DB_PATH="migrations/platform.db" RP_AUX_DB="${AUX_DB}" \
    RP_RUN_DB="${RUN_DB}" "${MCP_BIN}" <<< "$1" 2>/dev/null
}

cleanup() {
  kill "${SRV_PID}" 2>/dev/null; wait "${SRV_PID}" 2>/dev/null
  rm -f "${SRV_LOG}"; rm -rf "${WORK}"; rm -f "${RUN_DB}"
}
trap cleanup EXIT

echo "== 0. 构建（cargo --offline）与端口守卫"
cargo build --offline -q -p mcp-server --bin rp-mcp || { echo "rp-mcp build FAIL"; exit 1; }
if curl -s -o /dev/null --max-time 2 "http://127.0.0.1:${PORT}/api/healthz" 2>/dev/null \
   || curl -s -o /dev/null --max-time 2 "http://127.0.0.1:${PORT}/" 2>/dev/null; then
  echo "[FAIL] 端口 ${PORT} 已被占用（避免打到别人的服务，换 SELFTEST_PORT=<p> 重跑）"; exit 1
fi
echo "[PASS] rp-mcp 已构建，端口 ${PORT} 空闲"

echo "== 1. 生产库零写基线（user_version + 逐表行数指纹）"
PLAT_FP="$(db_fingerprint migrations/platform.db)"
DRY_FP="$(db_fingerprint migrations/platform-dryrun.db)"
echo "[PASS] 生产库指纹已取（收尾复核一致）"

echo "== 2. 起 rp_server（Bearer 令牌 + aux 挂载 ${AUX_DB}）"
RP_PORT="${PORT}" RP_HOST="127.0.0.1" RP_AUTH_TOKEN="${API_TOKEN}" \
  RP_DB_PATH="migrations/platform.db" RP_AUX_DB="${AUX_DB}" \
  python3 apps/server/rp_server.py > "${SRV_LOG}" 2>&1 &
SRV_PID=$!
for _ in $(seq 1 30); do
  curl -sf -H "Authorization: Bearer ${API_TOKEN}" "http://127.0.0.1:${PORT}/api/stats" >/dev/null 2>&1 && break
  sleep 0.2
done
# 就绪探针用 /api/stats（rp_server.py:2519 API_ROUTES 无 /api/healthz 路由）。
SRV_CODE=$(curl -s -o /dev/null -w "%{http_code}" -H "Authorization: Bearer ${API_TOKEN}" "http://127.0.0.1:${PORT}/api/stats")
ck "rp_server 就绪（/api/stats 200）" "200" "${SRV_CODE}"

echo "== 3. Tool Center 数据面：/api/tools items（同源对账基准面）"
curl -s -H "Authorization: Bearer ${API_TOKEN}" "http://127.0.0.1:${PORT}/api/tools?limit=200" > "${WORK}/api_tools.json"
curl -s -H "Authorization: Bearer ${API_TOKEN}" "http://127.0.0.1:${PORT}/api/tools?tool_id=rip.kb.rtis" > "${WORK}/api_schema.json"
API_TOTAL=$(python3 -I -c "import json; print(json.load(open('${WORK}/api_tools.json'))['total'])")
API_COUNT=$(python3 -I -c "import json; print(len(json.load(open('${WORK}/api_tools.json'))['items']))")
ck "/api/tools 主库+aux 合并条数一致（total=items）" "${API_TOTAL}" "${API_COUNT}"
ck "/api/tools 含 aux 增量行 rip.kb.rtis（22=20 主库+2 aux）" "22" "${API_TOTAL}"

echo "== 4. 无令牌握手被拒（RP_MCP_TOKEN 未配置 -> -32001）"
INIT_REQ='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"selftest","version":"0"}}}'
RESP=$(env -u RP_MCP_TOKEN RP_DB_PATH="migrations/platform.db" RP_AUX_DB="${AUX_DB}" RP_RUN_DB="${RUN_DB}" "${MCP_BIN}" <<< "${INIT_REQ}" 2>/dev/null)
CODE=$(python3 -I -c "import json,sys; print(json.loads(sys.argv[1])['error']['code'])" "${RESP}")
ck "无令牌 initialize 被拒（-32001 auth_required）" "-32001" "${CODE}"
RESP=$(env -u RP_MCP_TOKEN RP_DB_PATH="migrations/platform.db" RP_AUX_DB="${AUX_DB}" RP_RUN_DB="${RUN_DB}" "${MCP_BIN}" \
  <<< '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"cursor":""}}' 2>/dev/null)
CODE=$(python3 -I -c "import json,sys; print(json.loads(sys.argv[1])['error']['code'])" "${RESP}")
ck "无令牌 tools/list 被拒（-32001）" "-32001" "${CODE}"

echo "== 5. 握手（initialize）：只暴露目录与分页发现，不含工具全文"
RESP=$(mcp_req "${INIT_REQ}")
HAS_RESULT=$(python3 -I -c "import json,sys; print('result' in json.loads(sys.argv[1]))" "${RESP}")
ck "带令牌 initialize 返回 result" "True" "${HAS_RESULT}"
python3 -I - "${RESP}" > "${WORK}/init_checks.txt" <<'INITEOF'
import json, sys
r = json.loads(sys.argv[1])["result"]
ok1 = r["protocolVersion"] == "2024-11-05"
ok2 = r["capabilities"]["tools"]["listChanged"] is False
ok3 = r["serverInfo"]["name"] == "rp-mcp"
text = json.dumps(r)
ok4 = '"description"' not in text and '"inputSchema"' not in text and '"tier"' not in text
ok5 = len(r["instructions"]) < 200 and "tools/list" in r["instructions"]
print("OK" if all([ok1, ok2, ok3, ok4, ok5]) else f"BAD {ok1} {ok2} {ok3} {ok4} {ok5}")
INITEOF
INIT_STATE=$(cat "${WORK}/init_checks.txt")
ck "initialize 协议回显/能力位/serverInfo/零工具全文/一行导航" "OK" "${INIT_STATE}"

echo "== 6. tools/list 分页游标遍历（limit=5 强制多页，不重不漏）"
: > "${WORK}/mcp_all_tools.jsonl"
CURSOR=""
PAGES=0
while :; do
  if [[ -z "${CURSOR}" ]]; then
    RESP=$(mcp_req '{"jsonrpc":"2.0","id":9,"method":"tools/list","params":{"cursor":"","limit":5}}')
  else
    RESP=$(mcp_req "{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"tools/list\",\"params\":{\"cursor\":\"${CURSOR}\",\"limit\":5}}")
  fi
  PAGES=$((PAGES+1))
  python3 -I - "${RESP}" "${WORK}/mcp_all_tools.jsonl" <<'WALKEOF'
import json, sys
r = json.loads(sys.argv[1])["result"]
with open(sys.argv[2], "a") as fh:
    for t in r["tools"]:
        fh.write(json.dumps(t, ensure_ascii=False) + "\n")
print(r.get("nextCursor", ""))
WALKEOF
  NEXT=$(python3 -I - "${RESP}" <<'NEXTEOF'
import json, sys
print(json.loads(sys.argv[1])["result"].get("nextCursor", ""))
NEXTEOF
)
  [[ -z "${NEXT}" ]] && break
  CURSOR="${NEXT}"
  [[ ${PAGES} -gt 50 ]] && { echo "[FAIL] 分页游标失灵（超过 50 页）"; break; }
done
MCP_ENTRY_COUNT=$(wc -l < "${WORK}/mcp_all_tools.jsonl" | tr -d ' ')
ck "分页遍历条数 = 能力面 6 + 注册面 ${API_COUNT}" "$((6 + API_COUNT))" "${MCP_ENTRY_COUNT}"
ck "分页页数 > 1（真分页，limit=5）" "True" "$([[ ${PAGES} -gt 1 ]] && echo True || echo False)"
DUP=$(sort "${WORK}/mcp_all_tools.jsonl" | uniq -d | wc -l | tr -d ' ')
ck "遍历无重复条目" "0" "${DUP}"
NO_TEXT=$(python3 -I - "${WORK}/mcp_all_tools.jsonl" <<'TEXTEOF'
import json, sys
bad = 0
for line in open(sys.argv[1]):
    t = json.loads(line)
    if "description" in t or "inputSchema" in t:
        bad += 1
print(bad)
TEXTEOF
)
ck "目录条目零完整说明零 Schema（摘要投影，按需获取）" "0" "${NO_TEXT}"

echo "== 7. 能力面在场（rip.run_tool 与知识检索/运行查询目录面）"
for cap in rip.run_tool rip.list_tools rip.get_tool_schema rip.search_knowledge rip.get_run_status; do
  HIT=$(grep -c "\"name\": \"${cap}\"" "${WORK}/mcp_all_tools.jsonl" || true)
  ck "能力面条目在场：${cap}" "1" "${HIT}"
done

echo "== 8. 注册面逐条对账（tools/list 注册面 <-> /api/tools items，同源）"
python3 -I - "${WORK}/mcp_all_tools.jsonl" "${WORK}/api_tools.json" > "${WORK}/recon.txt" <<'RECONEOF'
import json, sys
mcp_rows = [json.loads(l) for l in open(sys.argv[1]) if json.loads(l).get("tier") == "registry"]
api_rows = json.load(open(sys.argv[2]))["items"]
if len(mcp_rows) != len(api_rows):
    print(f"COUNT_MISMATCH mcp={len(mcp_rows)} api={len(api_rows)}"); raise SystemExit
for m, a in zip(mcp_rows, api_rows):
    if m["name"] != f"rip.{a['tool_id']}":
        print(f"NAME_MISMATCH {m['name']} vs rip.{a['tool_id']}"); raise SystemExit
    for field in ("kind", "lifecycle", "governance", "permission_level", "source_system", "updated_at"):
        mv = m.get(field); av = a.get(field)
        if (mv or None) != (av or None):
            print(f"FIELD_MISMATCH {a['tool_id']}.{field}: mcp={mv} api={av}"); raise SystemExit
print("OK")
RECONEOF
RECON=$(cat "${WORK}/recon.txt")
ck "注册面与 /api/tools 逐条一致（顺序 + 六摘要列同源）" "OK" "${RECON}"

echo "== 9. 按需 Schema 同源（rip.get_tool_schema <-> /api/tools?tool_id=）"
RESP=$(mcp_req '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"rip.get_tool_schema","arguments":{"tool_id":"rip.kb.rtis"}}}')
python3 -I - "${RESP}" "${WORK}/api_schema.json" > "${WORK}/schema_cmp.txt" <<'SCEOF'
import json, sys
r = json.loads(sys.argv[1])
payload = json.loads(r["result"]["content"][0]["text"])
api = json.load(open(sys.argv[2]))
# handle_tools 详情的解析后 Schema 在响应顶层（"tool" 内是该列原文）。
ok = payload["input_schema"] == api.get("input_schema") and \
     payload["output_schema"] == api.get("output_schema")
print("OK" if ok and isinstance(payload["input_schema"], dict) else f"BAD payload={payload} api={api.get('input_schema')}")
SCEOF
SCHEMA_CMP=$(cat "${WORK}/schema_cmp.txt")
ck "rip.kb.rtis 的 Schema 与 /api/tools?tool_id= 逐字段一致（同一 tool 表列）" "OK" "${SCHEMA_CMP}"
RESP=$(mcp_req '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"rip.get_tool_schema","arguments":{"tool_id":"nmap"}}}')
NULLSCHEMA=$(python3 -I - "${RESP}" <<'NULLEOF'
import json, sys
p = json.loads(json.loads(sys.argv[1])["result"]["content"][0]["text"])
print("OK" if p["input_schema"] is None and p["output_schema"] is None and p["source"] == "main" else "BAD")
NULLEOF
)
ck "主库无 Schema 行如实返回双 null（不造假）" "OK" "${NULLSCHEMA}"

echo "== 10. run_tool 执行通道（P9-2）：未登记工具如实提交拒绝（D-TC-05，不落行）"
RESP=$(mcp_req '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"rip.run_tool","arguments":{"tool_id":"rip.kb.rtis","input":{}}}}')
ISERR=$(python3 -I - "${RESP}" <<'ERREOF'
import json, sys
r = json.loads(sys.argv[1])["result"]
t = json.loads(r["content"][0]["text"])
ok = (
    r["isError"] is True
    and t.get("ok") is False
    and "unknown tool" in str(t.get("error", {}).get("message", ""))
)
print("OK" if ok else f"BAD payload={t}")
ERREOF
)
ck "rip.run_tool 未登记工具结构化 isError（unknown tool，拒绝不落行）" "OK" "${ISERR}"
RESP=$(mcp_req '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"rip.nothere","arguments":{}}}')
CODE=$(python3 -I -c "import json,sys; print(json.loads(sys.argv[1])['error']['code'])" "${RESP}")
ck "目录外工具 -32602" "-32602" "${CODE}"

echo "== 11. 生产库零写复核（指纹前后一致）"
ck "platform.db 指纹不变" "${PLAT_FP}" "$(db_fingerprint migrations/platform.db)"
ck "platform-dryrun.db 指纹不变" "${DRY_FP}" "$(db_fingerprint migrations/platform-dryrun.db)"

echo ""
echo "PASS=${PASS} FAIL=${FAIL}"
if [[ ${FAIL} -eq 0 ]]; then
  echo "selftest_phase9_mcp: ALL PASS"
  exit 0
else
  echo "selftest_phase9_mcp: FAILED"
  exit 1
fi
