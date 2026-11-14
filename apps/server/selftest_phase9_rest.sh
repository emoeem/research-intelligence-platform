#!/usr/bin/env bash
# P9-3 REST 对外契约收口自验（ROADMAP §10.3 P9-3：REST API 完善 + CLI）：
#   trace 六 ID + correlation_id 透出（响应体 + X-Trace-Id/X-Correlation-Id
#   响应头）+ 全局限频（超限 429 + Retry-After）+ 对外访问审计 JSONL
#   （019 SQL 形状验证）+ POST 白名单外 404/405 守卫 + 契约文档端点清单与
#   rp_server.py ROUTES 逐条对账 + rp-* 11 bin 帮助面统一断言 +
#   生产库零写核对。输出 PASS/FAIL，全过 exit 0。
#
# 前置：仓库根运行（cargo 门禁按 CWD 发现 vendor 配置）；rp-tool/rp-workflow
# 由 cargo --offline 构建；临时库/临时审计文件落 /tmp；生产
# migrations/platform*.db 只读（URI mode=ro 指纹，红线 ROADMAP §13）。
# 端口守卫：SELFTEST_PORT / SELFTEST_PORT2（不与 7800 常驻工作台相撞）。
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO}"

PORT="${SELFTEST_PORT:-7823}"
PORT2="${SELFTEST_PORT2:-7824}"
TMP_DB="/tmp/rp_p93_rest_dev.db"
AUDIT="/tmp/rp_p93_rest_audit.jsonl"
AUDIT2="/tmp/rp_p93_rest_audit_ratelimit.jsonl"
TOKEN="selftest-$(python3 -c 'import secrets; print(secrets.token_hex(16))')"
LOG="$(mktemp /tmp/rp_p93_selftest.XXXX.log)"
LOG2="$(mktemp /tmp/rp_p93_selftest2.XXXX.log)"

PASS=0; FAIL=0
ck() { # ck <名> <期望> <实际>
  if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi
}
jget() { python3 -I -c "import sys,json; d=json.loads(sys.argv[1]); print(eval(sys.argv[2]))" "$1" "$2" 2>/dev/null || echo PARSE_FAIL; }
api() { curl -s -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}${1}"; }
api_code() { curl -s -o /dev/null -w "%{http_code}" -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}${1}"; }
api_code2() { curl -s -o /dev/null -w "%{http_code}" -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT2}${1}"; }

# 生产库指纹（user_version + 逐表行数，URI mode=ro 只读核对零写红线）。
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

cleanup() {
  [[ -n "${SRV1:-}" ]] && kill "${SRV1}" 2>/dev/null
  [[ -n "${SRV2:-}" ]] && kill "${SRV2}" 2>/dev/null
  [[ -n "${SRV1:-}" ]] && wait "${SRV1}" 2>/dev/null
  [[ -n "${SRV2:-}" ]] && wait "${SRV2}" 2>/dev/null
  rm -f "${TMP_DB}" "${TMP_DB}-wal" "${TMP_DB}-shm" "${AUDIT}" "${AUDIT2}" "${LOG}" "${LOG2}"
  rm -f /tmp/rp_p93_mig_a.db
}
trap cleanup EXIT

SERVER_PID=""; SERVER2_PID=""
echo "== 0. 构建（cargo --offline）"
cargo build --offline -q -p tool-registry --bin rp-tool || { echo "rp-tool build FAIL"; exit 1; }
cargo build --offline -q -p workflow-engine --bin rp-workflow || { echo "rp-workflow build FAIL"; exit 1; }
RP_TOOL="target/debug/rp-tool"
RP_WF="target/debug/rp-workflow"

echo "== 1. 生产库零写基线（user_version + 逐表行数指纹）"
PLAT_FP="$(db_fingerprint migrations/platform.db)"
DRY_FP=""
[[ -f migrations/platform-dryrun.db ]] && DRY_FP="$(db_fingerprint migrations/platform-dryrun.db)"
echo "[PASS] 生产库指纹已取（收尾复核一致）"

echo "== 2. 019 迁移验证（fresh 001+019 与 018 水位副本双路，platform*.db 不应用）"
MIG_RESULT="$(python3 -I <<'MIGEOF'
import sqlite3, sys, pathlib
mig = pathlib.Path("migrations")
c = sqlite3.connect("/tmp/rp_p93_mig_a.db")
c.executescript((mig / "001_init.sql").read_text())
c.executescript((mig / "019_external_api_audit.sql").read_text())
uv = c.execute("PRAGMA user_version").fetchone()[0]
cols = [r[1] for r in c.execute("PRAGMA table_info(external_api_audit)")]
expect_cols = ["id","ts","event","method","path","remote","status","duration_ms",
               "trace_id","correlation_id","rate_limited","auth_enabled","meta_json"]
c.execute("INSERT INTO external_api_audit (ts,event,method,path,remote,status,duration_ms,trace_id,correlation_id,rate_limited,auth_enabled) VALUES ('2026-11-14T00:00:00Z','api_access','GET','/api/stats','127.0.0.1',200,5,NULL,'wfr-x',0,1)")
row = c.execute("SELECT ts,event,method,path,status,rate_limited,auth_enabled FROM external_api_audit").fetchone()
sv = c.execute("SELECT COUNT(*) FROM schema_version WHERE version=19").fetchone()[0]
idx = [r[0] for r in c.execute("SELECT name FROM sqlite_master WHERE type='index' AND name LIKE 'ix_external_api_audit%'")]
ok = (uv == 19 and cols == expect_cols and row[1] == 'api_access' and row[4] == 200
      and sv == 1 and len(idx) == 3)
# 幂等：重放零差异
c.executescript((mig / "019_external_api_audit.sql").read_text())
uv2 = c.execute("PRAGMA user_version").fetchone()[0]
rows2 = c.execute("SELECT COUNT(*) FROM external_api_audit").fetchone()[0]
ok = ok and uv2 == 19 and rows2 == 1
print("PASS" if ok else f"FAIL uv={uv} cols={cols} row={row} sv={sv} idx={idx} uv2={uv2} rows2={rows2}")
sys.exit(0 if ok else 1)
MIGEOF
)"
ck "019 fresh 库应用（表/列/索引/schema_version/user_version=19 + 重放幂等）" "PASS" "${MIG_RESULT}"

cp migrations/dev-p8.db /tmp/rp_p93_mig_b.db 2>/dev/null
if [[ -f /tmp/rp_p93_mig_b.db ]]; then
  python3 -I -c "
import sqlite3
c = sqlite3.connect('/tmp/rp_p93_mig_b.db')
c.executescript(open('migrations/019_external_api_audit.sql').read())
print(c.execute('PRAGMA user_version').fetchone()[0])
" > /tmp/rp_p93_mig_b_uv.txt
  ck "019 在 018 水位开发副本上增量应用（dev-p8 副本 → 19）" "19" "$(cat /tmp/rp_p93_mig_b_uv.txt)"
else
  ck "019 在 018 水位开发副本上增量应用（dev-p8 副本缺席按 V-9 跳过）" "SKIP" "SKIP"
fi

echo "== 3. workflow fixture（临时库：登记 + put + run 真实链）"
rm -f "${TMP_DB}" "${TMP_DB}-wal" "${TMP_DB}-shm"; rm -f "${AUDIT}" "${AUDIT2}"
"${RP_TOOL}" register --manifest manifests/tools/rip.kb.keyword_search.json --db "${TMP_DB}" >/dev/null 2>&1
ck "rp-tool register rip.kb.keyword_search" "0" "$?"
"${RP_WF}" put docs/phase8/samples/p8-tool-node.json --db "${TMP_DB}" >/dev/null 2>&1
ck "rp-workflow put p8-tool-node 样例" "0" "$?"
RUN_OUT="$("${RP_WF}" run wf-p8-tool --input '{"target":"example.com"}' --db "${TMP_DB}" 2>"${LOG}")"
ck "wf-p8-tool run exit=0（succeeded 终态）" "0" "$?"
RUN_ID="$(jget "${RUN_OUT}" "d['run_id']")"
RUN_TRACE="$(python3 -I -c "
import sqlite3
c = sqlite3.connect('file:${TMP_DB}?mode=ro', uri=True)
print(c.execute(\"SELECT trace_id FROM workflow_run WHERE run_id = ?\", ('${RUN_ID}',)).fetchone()[0])")"
ck "run trace_id = run_id（018 链锚惯例，库行核对）" "${RUN_ID}" "${RUN_TRACE}"

echo "== 4. 起服务 A（token + aux=fixture + 审计 JSONL + 默认限频）"
RP_PORT="${PORT}" RP_AUTH_TOKEN="${TOKEN}" RP_AUX_DB="${TMP_DB}" \
  RP_API_AUDIT_LOG="${AUDIT}" \
  python3 apps/server/rp_server.py > "${LOG}" 2>&1 &
SRV1=$!; SERVER_PID="${SRV1}"
for _ in $(seq 1 30); do curl -s -o /dev/null "http://127.0.0.1:${PORT}/" && break; sleep 0.2; done

# 鉴权快回归（全量口径归 selftest_auth.sh）
code=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:${PORT}/api/stats")
ck "无令牌 401（鉴权回归）" "401" "${code}"
code=$(api_code "/api/stats")
ck "正确令牌 200" "200" "${code}"

echo "== 5. 断言 1：workflow run 详情（trace 六 ID + correlation_id 体 + 双头）"
api "/api/workflow-runs?id=${RUN_ID}" > /tmp/rp93_wf_detail.json
BODY_KEYS="$(python3 -I - "${RUN_ID}" <<'PYEOF'
import json, sys
run_id = sys.argv[1]
d = json.load(open("/tmp/rp93_wf_detail.json", encoding="utf-8"))
corr = d.get("correlation", {})
keys = ["trace_id", "task_id", "agent_run_id", "tool_run_id", "artifact_id", "evidence_id"]
ok = (d.get("correlation_id") == run_id and d.get("query_id") == run_id
      and all(k in corr for k in keys))
print("yes" if ok else f"no correlation_id={d.get('correlation_id')} keys={[k for k in keys if k not in corr]}")
PYEOF
)"
ck "run 详情响应体含六 ID 关联字段与顶层 correlation_id（=run_id）" "yes" "${BODY_KEYS}"
HDRS="$(curl -si -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}/api/workflow-runs?id=${RUN_ID}" | tr -d '\r')"
ck "X-Trace-Id 头在场且 = trace_id" "X-Trace-Id: ${RUN_ID}" "$(echo "${HDRS}" | grep -i '^X-Trace-Id:' | head -1)"
ck "X-Correlation-Id 头在场且 = run_id" "X-Correlation-Id: ${RUN_ID}" "$(echo "${HDRS}" | grep -i '^X-Correlation-Id:' | head -1)"

echo "== 6. 断言 2/3：tool run 详情与 /api/traces 的关联透出"
TOOL_RUN_ID="$(python3 -I -c "import json; print(json.load(open('/tmp/rp93_wf_detail.json'))['correlation']['tool_run_id'])")"
api "/api/runs?run_id=${TOOL_RUN_ID}" > /tmp/rp93_run_detail.json
ck "tool run 详情 correlation_id = tool_run_id" "${TOOL_RUN_ID}" \
  "$(python3 -I -c "import json; print(json.load(open('/tmp/rp93_run_detail.json'))['correlation_id'])")"
ck "tool run 详情 X-Trace-Id/X-Correlation-Id 双头在场" "yes" \
  "$(curl -si -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}/api/runs?run_id=${TOOL_RUN_ID}" | tr -d '\r' | grep -ci '^X-Trace-Id:\|^X-Correlation-Id:' | awk '{print ($1>=2)?"yes":"no"}')"
api "/api/traces?id=${RUN_ID}" > /tmp/rp93_trace.json
ck "trace 链响应体顶层 trace_id/correlation_id（单锚命中）" "yes" \
  "$(python3 -I - "${RUN_ID}" <<'PYEOF'
import json, sys
d = json.load(open("/tmp/rp93_trace.json", encoding="utf-8"))
print("yes" if d.get("trace_id") == sys.argv[1] and d.get("correlation_id") == sys.argv[1] else f"no t={d.get('trace_id')} c={d.get('correlation_id')}")
PYEOF
)"
ck "trace 链 X-Trace-Id/X-Correlation-Id 双头在场" "yes" \
  "$(curl -si -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}/api/traces?id=${RUN_ID}" | tr -d '\r' | grep -ci '^X-Trace-Id:\|^X-Correlation-Id:' | awk '{print ($1>=2)?"yes":"no"}')"
ck "锚不在场的载荷不发关联头（列表页 /api/runs）" "no" \
  "$(curl -si -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}/api/runs" | tr -d '\r' | grep -ci '^X-Trace-Id:' | awk '{print ($1>=1)?"yes":"no"}')"

echo "== 7. 断言 4：POST 白名单外 404/405 守卫"
ck "POST /api/stats（只读端点）→ 405" "405" "$(curl -s -o /dev/null -w '%{http_code}' -X POST -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}/api/stats")"
ck "POST /api/agents（白名单外）→ 405" "405" "$(curl -s -o /dev/null -w '%{http_code}' -X POST -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}/api/agents")"
ck "POST /api/nonexistent → 405" "405" "$(curl -s -o /dev/null -w '%{http_code}' -X POST -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}/api/nonexistent")"
ck "POST /api/plugins/<id>/unknown-action → 404" "404" "$(curl -s -o /dev/null -w '%{http_code}' -X POST -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}/api/plugins/whatever/unknown-action")"
ck "POST /api/workflow-runs/<id>/unknown-action → 405" "405" "$(curl -s -o /dev/null -w '%{http_code}' -X POST -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}/api/workflow-runs/${RUN_ID}/unknown-action")"
ck "GET /api/nonexistent → 404" "404" "$(api_code "/api/nonexistent")"

echo "== 8. 断言 5：对外访问审计 JSONL（019 表列形状）"
AUDIT_SHAPE="$(python3 -I - "${AUDIT}" <<'AUDEOF'
import json, sys
rows = [json.loads(l) for l in open(sys.argv[1], encoding="utf-8") if l.strip()]
keys = ["ts","event","method","path","remote","status","duration_ms",
        "trace_id","correlation_id","rate_limited","auth_enabled"]
ok = rows and all(all(k in r for k in keys) for r in rows)
wr = [r for r in rows if r["path"] == "/api/workflow-runs" and r["status"] == 200]
ok = ok and wr and wr[0]["trace_id"] is not None and wr[0]["rate_limited"] == 0
print("PASS" if ok else f"FAIL rows={len(rows)} wr={wr[:1]}")
AUDEOF
)"
ck "审计 JSONL 行形状 = 019 external_api_audit 列（api_access 事件）" "PASS" "${AUDIT_SHAPE}"

echo "== 9. 断言 6：全局限频（独立实例 RP_RATE_LIMIT_RPM=5 → 429）"
RP_PORT="${PORT2}" RP_AUTH_TOKEN="${TOKEN}" RP_RATE_LIMIT_RPM=5 \
  RP_API_AUDIT_LOG="${AUDIT2}" RP_AUX_DB="${TMP_DB}" \
  python3 apps/server/rp_server.py > "${LOG2}" 2>&1 &
SRV2=$!; SERVER2_PID="${SRV2}"
for _ in $(seq 1 30); do curl -s -o /dev/null "http://127.0.0.1:${PORT2}/" && break; sleep 0.2; done
CODES=""
for i in $(seq 1 8); do CODES="${CODES}$(api_code2 "/api/stats") "; done
ck "前 5 个请求 200，第 6 个起 429" "200 200 200 200 200 429 429 429 " "${CODES}"
RL_HDRS="$(curl -si -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT2}/api/stats" | tr -d '\r')"
ck "429 带 Retry-After 头" "yes" "$(echo "${RL_HDRS}" | grep -qi '^Retry-After:' && echo yes)"
ck "429 body code = rate_limited" "rate_limited" \
  "$(curl -s -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT2}/api/stats" | python3 -I -c "import sys,json; print(json.load(sys.stdin)['error']['code'])")"
ck "限频审计行 rate_limited=1 在场" "yes" \
  "$(python3 -I -c "
import json
rows = [json.loads(l) for l in open('${AUDIT2}', encoding='utf-8') if l.strip()]
print('yes' if any(r['rate_limited'] == 1 and r['status'] == 429 for r in rows) else 'no')
")"

echo "== 10. 契约文档端点清单与 rp_server.py ROUTES 逐条对账"
DOC_RESULT="$(python3 -I <<'DOCEOF'
import re, sys, pathlib

src = pathlib.Path("apps/server/rp_server.py").read_text()
doc = pathlib.Path("docs/phase9/REST-API-CONTRACT.md").read_text()

# rp_server.py API_ROUTES 键（词含连字符：agent-runs / workflow-runs）
block = re.search(r"API_ROUTES = \{(.*?)\n\}", src, re.S).group(1)
routes = sorted(set(re.findall(r'"(/api/[a-z0-9_-]+)":', block)))
assert len(routes) == 17, f"API_ROUTES 应 17 条只读路由（P9-4 起 16 条加 /api/mcp-catalog），实得 {len(routes)}: {routes}"

# 文档端点总览段（标记之间）
m = re.search(r"<!-- ENDPOINTS:BEGIN -->(.*?)<!-- ENDPOINTS:END -->", doc, re.S)
assert m, "文档缺 ENDPOINTS 标记段"
rows = re.findall(r"^\|\s*(GET|POST)\s*\|\s*(/api/[A-Za-z0-9_<>/-]+)\s*\|", m.group(1), re.M)
doc_get = {p for meth, p in rows if meth == "GET"}
doc_post = {p for meth, p in rows if meth == "POST"}

dynamic_get = {"/api/plugins/<plugin_id>"}
expected_post = {
    "/api/plugins/install",
    "/api/plugins/<plugin_id>/<action>",
    "/api/workflows",
    "/api/workflow-runs",
    "/api/workflow-runs/<action>",
    "/api/workflow-runs/<run_id>/<action>",
}
problems = []
for r in routes:
    if r not in doc_get:
        problems.append(f"API_ROUTES {r} 不在文档 GET 行")
if doc_get != set(routes) | dynamic_get:
    problems.append(f"文档 GET 集合不一致: doc={sorted(doc_get)} expect={sorted(set(routes)|dynamic_get)}")
if doc_post != expected_post:
    problems.append(f"文档 POST 集合不一致: doc={sorted(doc_post)} expect={sorted(expected_post)}")
if len(doc_get) != 18:
    problems.append(f"文档 GET 行应 18（17 静态 + 1 动态详情），实得 {len(doc_get)}")

# 动作词表对账（rp_server.py 词表 ↔ 文档行内词表）
plug = re.search(r"PLUGIN_WRITE_ACTIONS = \(([^)]*)\)", src).group(1)
plug_words = sorted(set(re.findall(r'"([a-z-]+)"', plug))) + ["upgrade"]
wf = re.search(r"WORKFLOW_WRITE_ACTIONS = \(([^)]*)\)", src).group(1)
wf_words = sorted(set(re.findall(r'"([a-z-]+)"', wf)))
doc_line = lambda p: next((ln for ln in m.group(1).splitlines() if f"| {p} |" in ln), "")
for token in plug_words:
    if token not in doc_line("/api/plugins/<plugin_id>/<action>"):
        problems.append(f"插件写动作 {token} 不在文档行")
for token in wf_words:
    if token not in doc_line("/api/workflow-runs/<action>") or token not in doc_line("/api/workflow-runs/<run_id>/<action>"):
        problems.append(f"workflow 写动作 {token} 不在文档行")

# 文档不得发明端点（总览段内出现的 /api/ 路径全部 ∈ 已知集合）
all_doc_paths = set(re.findall(r"(/api/[A-Za-z0-9_<>/-]+)", m.group(1)))
known = set(routes) | dynamic_get | expected_post
extra = all_doc_paths - known
if extra:
    problems.append(f"文档发明端点: {sorted(extra)}")

if problems:
    for p in problems:
        print("FAIL:", p)
    sys.exit(1)
print("PASS")
DOCEOF
)"
ck "契约文档端点清单与 ROUTES/写面白名单/动作词表逐条一致" "PASS" "${DOC_RESULT}"

echo "== 11. rp-* 11 bin 帮助面统一（--help stdout exit 0；误用 stderr exit 2）"
BINS=(rp-agent rp-context rp-knowledge-fetch rp-plugin rp-research rp-resolve rp-review rp-search-index rp-tool rp-tool-health rp-workflow)
ck "CLI bin 计数 = 11（rp-mcp 为协议面不计）" "11" "${#BINS[@]}"
HELP_OK="yes"
for b in "${BINS[@]}"; do
  ./target/debug/$b --help >/tmp/rp93_help.out 2>/dev/null; rc=$?
  first="$(head -1 /tmp/rp93_help.out)"
  [[ $rc -eq 0 && "$first" == "usage: $b"* ]] || { HELP_OK="no($b rc=$rc first=$first)"; break; }
done
ck "11 bin --help：exit 0 + stdout 首行 usage: rp-<bin>" "yes" "${HELP_OK}"
MISUSE_OK="yes"
for b in "${BINS[@]}"; do
  ./target/debug/$b >/dev/null 2>/tmp/rp93_help.err; rc=$?
  [[ $rc -eq 2 ]] || { MISUSE_OK="no($b rc=$rc)"; break; }
done
ck "11 bin 无参误用：exit 2" "yes" "${MISUSE_OK}"

echo "== 12. 生产库零写复核（指纹一致）"
ck "platform.db 零写" "${PLAT_FP}" "$(db_fingerprint migrations/platform.db)"
if [[ -n "${DRY_FP}" ]]; then
  ck "platform-dryrun.db 零写" "${DRY_FP}" "$(db_fingerprint migrations/platform-dryrun.db)"
else
  ck "platform-dryrun.db 零写（缺席跳过）" "SKIP" "SKIP"
fi

echo "----"
echo "结果：PASS=${PASS} FAIL=${FAIL}"
[[ ${FAIL} -eq 0 ]]
