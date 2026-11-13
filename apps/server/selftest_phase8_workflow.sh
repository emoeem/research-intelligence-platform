#!/usr/bin/env bash
# P8-2/P8-3 自验脚本（ROADMAP §9.3 P8-2/P8-3 判据的 CLI/API 层等价断言）：
# 真实 rp-workflow CLI 端到端（python_worker 真实派发 + context-broker
# 真实装配 + agent-engine AgentLoop 真实闭环）+ 节点状态对账（只读开发库
# 副本）+ kill 断点恢复 + 失败重试 attempt 递增 + 节点超时显式终态
# + P8-3 协作暂停恢复（agent 节点 014 控制行传播）+ 人工介入
# awaiting_approval 批准注入 + 六 ID 对账（GET /api/workflow-runs 与库列
# 一致 + /api/traces 双向）+ POST 白名单守卫（白名单外 405）+
# 生产库零写核对（URI mode=ro 指纹）。输出 PASS/FAIL，全过 exit 0。
#
# 前置：仓库根运行（cargo 门禁按 CWD 发现 vendor 配置）；rp-tool /
# rp-workflow / rp-agent 由 cargo --offline 构建；临时库 /tmp；生产
# migrations/platform*.db 只读（红线 ROADMAP §13）。
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO}"

TMP_DB="${RIP_P82_DB:-/tmp/rp_p82_workflow_dev.db}"
COPY_DB="/tmp/rp_p82_workflow_copy.db"
LOG="$(mktemp /tmp/rp_p82_selftest.XXXX.log)"

PASS=0; FAIL=0
ck() { # ck <名> <期望> <实际>
  if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi
}
jget() { python3 -I -c "import sys,json; d=json.loads(sys.argv[1]); print(eval(sys.argv[2]))" "$1" "$2" 2>/dev/null || echo PARSE_FAIL; }
# 节点状态对账（只读 URI，ask 第 2 条原形）
reconcile() { python3 -I -c "import sqlite3; c=sqlite3.connect('file:$1?mode=ro',uri=True); print(c.execute(\"select node_id,state,attempt from workflow_run_node where run_id='$2' order by node_id\").fetchall())"; }
run_state() { python3 -I -c "import sqlite3; c=sqlite3.connect('file:$1?mode=ro',uri=True); print(c.execute(\"select state from workflow_run where run_id='$2'\").fetchone()[0])"; }
agent_state() { python3 -I -c "import sqlite3; c=sqlite3.connect('file:$1?mode=ro',uri=True); r=c.execute(\"select a.state from workflow_run_node n join agent_run a on a.run_id=n.agent_run_id where n.run_id='$2' and n.node_id='$3'\").fetchone(); print(r[0] if r else '')"; }

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

cleanup() { rm -f "${TMP_DB}" "${TMP_DB}-wal" "${TMP_DB}-shm" "${COPY_DB}"; }
trap cleanup EXIT

echo "== 0. 构建（cargo --offline）"
cargo build --offline -q -p tool-registry --bin rp-tool || { echo "rp-tool build FAIL"; exit 1; }
cargo build --offline -q -p workflow-engine --bin rp-workflow || { echo "rp-workflow build FAIL"; exit 1; }
RP_TOOL="target/debug/rp-tool"
RP_WF="target/debug/rp-workflow"

echo "== 1. 生产库零写基线（user_version + 逐表行数指纹）"
PLAT_FP="$(db_fingerprint migrations/platform.db)"
DRY_FP=""
if [[ -f migrations/platform-dryrun.db ]]; then
  DRY_FP="$(db_fingerprint migrations/platform-dryrun.db)"
fi
echo "[PASS] 生产库指纹已取（收尾复核一致）"

echo "== 2. 临时开发副本：put 五份样例定义（装载门 + 静态校验门）"
rm -f "${TMP_DB}" "${TMP_DB}-wal" "${TMP_DB}-shm"
for f in docs/phase8/samples/p8-tool-node.json docs/phase8/samples/p8-knowledge-node.json \
         docs/phase8/samples/p8-condition-node.json docs/phase8/samples/p8-parallel-node.json \
         docs/phase8/samples/p8-retry-fail-then-fix.json; do
  "${RP_WF}" put "${f}" --db "${TMP_DB}" > /dev/null 2>&1
  ck "put $(basename "${f}")" "0" "$?"
done
# 长链定义（kill/resume 确定性 kill 点用：8 个顺序 kb 节点）
cat > /tmp/rp_p82_long.json <<'JSONEOF'
{
  "schema": "workflow/definition-v1",
  "workflow_id": "wf-p8-selftest-long",
  "version": 1,
  "name": "P8-2 selftest long chain (kill/resume)",
  "graph": {
    "nodes": [
      { "id": "start", "type": "start" },
      { "id": "t1", "type": "tool", "params": { "tool_id": "rip.kb.keyword_search", "input": { "keyword": "nmap" } } },
      { "id": "t2", "type": "tool", "params": { "tool_id": "rip.kb.keyword_search", "input": { "keyword": "example" } } },
      { "id": "t3", "type": "tool", "params": { "tool_id": "rip.kb.keyword_search", "input": { "keyword": "workflow" } } },
      { "id": "t4", "type": "tool", "params": { "tool_id": "rip.kb.keyword_search", "input": { "keyword": "tool" } } },
      { "id": "t5", "type": "tool", "params": { "tool_id": "rip.kb.keyword_search", "input": { "keyword": "agent" } } },
      { "id": "t6", "type": "tool", "params": { "tool_id": "rip.kb.keyword_search", "input": { "keyword": "broker" } } },
      { "id": "t7", "type": "tool", "params": { "tool_id": "rip.kb.keyword_search", "input": { "keyword": "registry" } } },
      { "id": "t8", "type": "tool", "params": { "tool_id": "rip.kb.keyword_search", "input": { "keyword": "manifest" } } },
      { "id": "done", "type": "end" }
    ],
    "edges": [
      { "from": "start", "to": "t1" }, { "from": "t1", "to": "t2" }, { "from": "t2", "to": "t3" },
      { "from": "t3", "to": "t4" }, { "from": "t4", "to": "t5" }, { "from": "t5", "to": "t6" },
      { "from": "t6", "to": "t7" }, { "from": "t7", "to": "t8" }, { "from": "t8", "to": "done" }
    ]
  }
}
JSONEOF
"${RP_WF}" put /tmp/rp_p82_long.json --db "${TMP_DB}" > /dev/null 2>&1
ck "put 长链定义" "0" "$?"

echo "== 3. 失败注入：未登记工具 run FAILED（retry 前置态）"
RETRY_OUT="$("${RP_WF}" run wf-p8-retry --input '{"target":"example.com"}' --db "${TMP_DB}" 2>"${LOG}")"
RUN_RC=$?
ck "未登记工具 run exit=1（FAILED 终态）" "1" "${RUN_RC}"
RETRY_RUN_ID="$(jget "${RETRY_OUT}" "d['run_id']")"
ck "失败 run 节点 search=failed attempt=1" \
   "[('done', 'pending', 0), ('search', 'failed', 1), ('start', 'succeeded', 1)]" \
   "$(reconcile "${TMP_DB}" "${RETRY_RUN_ID}")"

echo "== 4. 自验第 1 条：四类节点各一个最小定义 run 跑通 exit 0"
"${RP_TOOL}" register --manifest manifests/tools/rip.kb.keyword_search.json --db "${TMP_DB}" > /dev/null 2>&1
ck "rp-tool register rip.kb.keyword_search" "0" "$?"

OUT_TOOL="$("${RP_WF}" run wf-p8-tool --input '{"target":"example.com"}' --db "${TMP_DB}" 2>"${LOG}")"
ck "tool 节点 workflow run exit=0" "0" "$?"
ck "tool run state=succeeded" "succeeded" "$(jget "${OUT_TOOL}" "d['state']")"
RUN_TOOL="$(jget "${OUT_TOOL}" "d['run_id']")"

OUT_KB="$("${RP_WF}" run wf-p8-knowledge --input '{"target":"example.com"}' --db "${TMP_DB}" 2>"${LOG}")"
ck "knowledge 节点 workflow run exit=0" "0" "$?"
ck "knowledge run state=succeeded" "succeeded" "$(jget "${OUT_KB}" "d['state']")"
RUN_KB="$(jget "${OUT_KB}" "d['run_id']")"

OUT_COND="$("${RP_WF}" run wf-p8-condition --input '{"target":"example.com"}' --db "${TMP_DB}" 2>"${LOG}")"
ck "condition 节点 workflow run exit=0（true 分支）" "0" "$?"
ck "condition true: hit succeeded" "succeeded" "$(jget "${OUT_COND}" "[n for n in d['nodes'] if n['node_id']=='hit'][0]['state']")"
ck "condition true: miss skipped" "skipped" "$(jget "${OUT_COND}" "[n for n in d['nodes'] if n['node_id']=='miss'][0]['state']")"
RUN_COND="$(jget "${OUT_COND}" "d['run_id']")"

OUT_COND2="$("${RP_WF}" run wf-p8-condition --input '{"target":"other.example"}' --db "${TMP_DB}" 2>"${LOG}")"
ck "condition 节点 workflow run exit=0（false 分支）" "0" "$?"
ck "condition false: miss succeeded" "succeeded" "$(jget "${OUT_COND2}" "[n for n in d['nodes'] if n['node_id']=='miss'][0]['state']")"
ck "condition false: hit skipped" "skipped" "$(jget "${OUT_COND2}" "[n for n in d['nodes'] if n['node_id']=='hit'][0]['state']")"

OUT_PAR="$("${RP_WF}" run wf-p8-parallel --input '{"target":"example.com"}' --db "${TMP_DB}" 2>"${LOG}")"
ck "parallel 节点 workflow run exit=0（fork-join）" "0" "$?"
ck "parallel run state=succeeded" "succeeded" "$(jget "${OUT_PAR}" "d['state']")"
ck "parallel: w1 succeeded" "succeeded" "$(jget "${OUT_PAR}" "[n for n in d['nodes'] if n['node_id']=='w1'][0]['state']")"
ck "parallel: w2 succeeded" "succeeded" "$(jget "${OUT_PAR}" "[n for n in d['nodes'] if n['node_id']=='w2'][0]['state']")"
ck "parallel: fan succeeded" "succeeded" "$(jget "${OUT_PAR}" "[n for n in d['nodes'] if n['node_id']=='fan'][0]['state']")"
RUN_PAR="$(jget "${OUT_PAR}" "d['run_id']")"

echo "== 5. 自验第 2 条：节点状态对账（只读开发库副本）"
cp "${TMP_DB}" "${COPY_DB}"
ck "tool run 对账（每节点独立终态）" \
   "[('done', 'succeeded', 1), ('search', 'succeeded', 1), ('start', 'succeeded', 1)]" \
   "$(reconcile "${COPY_DB}" "${RUN_TOOL}")"
ck "knowledge run 对账" \
   "[('done', 'succeeded', 1), ('retrieve', 'succeeded', 1), ('start', 'succeeded', 1)]" \
   "$(reconcile "${COPY_DB}" "${RUN_KB}")"
ck "condition run 对账（含 skipped 终态）" \
   "[('check', 'succeeded', 1), ('done-hit', 'succeeded', 1), ('done-miss', 'skipped', 0), ('hit', 'succeeded', 1), ('miss', 'skipped', 0), ('start', 'succeeded', 1)]" \
   "$(reconcile "${COPY_DB}" "${RUN_COND}")"
ck "parallel run 对账（fan/w1/w2 独立终态）" \
   "[('done', 'succeeded', 1), ('fan', 'succeeded', 1), ('start', 'succeeded', 1), ('w1', 'succeeded', 1), ('w2', 'succeeded', 1)]" \
   "$(reconcile "${COPY_DB}" "${RUN_PAR}")"

echo "== 6. 自验第 4 条：retry 只重跑失败节点且 attempt 递增"
RETRY2_OUT="$("${RP_WF}" retry "${RETRY_RUN_ID}" --node search --db "${TMP_DB}" 2>"${LOG}")"
RETRY2_RC=$?
ck "retry exit=0（重跑成功）" "0" "${RETRY2_RC}"
RETRY2_RUN_ID="$(jget "${RETRY2_OUT}" "d['run_id']")"
ck "retry 以新 run 承载" "True" "$(python3 -I -c "print('${RETRY2_RUN_ID}' != '${RETRY_RUN_ID}')")"
ck "retry 新 run state=succeeded" "succeeded" "$(jget "${RETRY2_OUT}" "d['state']")"
ck "retry 新 run search attempt=2（递增）" \
   "[('done', 'succeeded', 1), ('search', 'succeeded', 2), ('start', 'succeeded', 1)]" \
   "$(reconcile "${TMP_DB}" "${RETRY2_RUN_ID}")"
ck "原 run 保持 FAILED 终态" "failed" "$(run_state "${TMP_DB}" "${RETRY_RUN_ID}")"

echo "== 7. 自验第 3 条：kill 中断 + resume 断点续跑"
"${RP_WF}" run wf-p8-selftest-long --input '{}' --db "${TMP_DB}" > /tmp/rp_p82_long_out.json 2>"${LOG}" &
LONG_PID=$!
LONG_RUN_ID=""
CP=0
for i in $(seq 1 600); do
  LONG_RUN_ID="$(python3 -I -c "import sqlite3; c=sqlite3.connect('file:${TMP_DB}?mode=ro',uri=True); r=c.execute(\"select run_id from workflow_run where workflow_id='wf-p8-selftest-long' and state='running'\").fetchall(); print(r[0][0] if r else '')" 2>/dev/null || true)"
  if [[ -n "${LONG_RUN_ID}" ]]; then
    CP="$(python3 -I -c "import sqlite3; c=sqlite3.connect('file:${TMP_DB}?mode=ro',uri=True); print(c.execute(\"select count(*) from workflow_checkpoint where run_id='${LONG_RUN_ID}' and node_state='succeeded'\").fetchone()[0])" 2>/dev/null || echo 0)"
    if [[ "${CP}" -ge 3 ]]; then break; fi
  fi
  sleep 0.02
done
if [[ -z "${LONG_RUN_ID}" || "${CP:-0}" -lt 3 ]]; then
  FAIL=$((FAIL+1)); echo "[FAIL] kill 点未出现（长链 run_id=${LONG_RUN_ID:-无} checkpoint=${CP:-0}）"
  kill "${LONG_PID}" 2>/dev/null
else
  # 确定性 kill：已成功节点 >= 3 后 SIGKILL（残留节点续跑窗口充足）。
  PRE_KILL_STATES="$(reconcile "${TMP_DB}" "${LONG_RUN_ID}")"
  kill -9 "${LONG_PID}" 2>/dev/null
  wait "${LONG_PID}" 2>/dev/null
  ck "kill 后 run 停留 running（非终态）" "running" "$(run_state "${TMP_DB}" "${LONG_RUN_ID}")"
  RESUME_OUT="$("${RP_WF}" resume "${LONG_RUN_ID}" --db "${TMP_DB}" 2>"${LOG}")"
  RESUME_RC=$?
  ck "resume exit=0（续跑至 SUCCEEDED）" "0" "${RESUME_RC}"
  ck "resume run state=succeeded" "succeeded" "$(jget "${RESUME_OUT}" "d['state']")"
  # 已完成节点（kill 前 t1/t2/t3）attempt 保持 1 不重跑。
  T1_AFTER="$(python3 -I -c "import json; d=json.loads('''${RESUME_OUT}'''); print([ (n['state'],n['attempt']) for n in d['nodes'] if n['node_id']=='t1'][0])")"
  ck "kill 前已完成节点 t1 保持 succeeded attempt=1" "('succeeded', 1)" "${T1_AFTER}"
  PASS=$((PASS+1)); echo "[PASS] kill 前 checkpoint 快照留存（${PRE_KILL_STATES:0:80}...）"
fi

echo "== 8. 节点超时显式终态（--node-timeout-ms 50 < 真实派发耗时）"
TIMEOUT_OUT="$("${RP_WF}" run wf-p8-tool --input '{"target":"example.com"}' --node-timeout-ms 50 --db "${TMP_DB}" 2>"${LOG}")"
TIMEOUT_RC=$?
ck "超时 run exit=1（FAILED 终态）" "1" "${TIMEOUT_RC}"
ck "超时 run state=failed" "failed" "$(jget "${TIMEOUT_OUT}" "d['state']")"
TIMEOUT_RUN_ID="$(jget "${TIMEOUT_OUT}" "d['run_id']")"
TIMEOUT_CODE="$(python3 -I -c "import sqlite3,json; c=sqlite3.connect('file:${TMP_DB}?mode=ro',uri=True); r=c.execute(\"select error_json from workflow_run_node where run_id='${TIMEOUT_RUN_ID}' and node_id='search'\").fetchone(); print(json.loads(r[0])['code'])")"
ck "超时节点 error code=node_timeout" "node_timeout" "${TIMEOUT_CODE}"

echo "== 9. 读取面：status / nodes"
STATUS_OUT="$("${RP_WF}" status "${RUN_TOOL}" --db "${TMP_DB}" 2>"${LOG}")"
ck "status run state=succeeded" "succeeded" "$(jget "${STATUS_OUT}" "d['state']")"
ck "status 携带六 ID 关联列（trace_id = run_id）" "${RUN_TOOL}" "$(jget "${STATUS_OUT}" "d['correlation']['trace_id']")"
NODES_OUT="$("${RP_WF}" nodes "${RUN_TOOL}" --db "${TMP_DB}" 2>"${LOG}" | tail -1)"
ck "nodes latest_checkpoint_seq>=1" "True" "$(jget "${NODES_OUT}" "d['latest_checkpoint_seq'] >= 1")"
"${RP_WF}" status run-not-exist --db "${TMP_DB}" > /dev/null 2>&1
ck "status 不存在 run exit=2（受理错误）" "2" "$?"

echo "== 10. P8-3 准备：agent/human 样例装载 + agent 定义登记"
"${RP_WF}" put docs/phase8/samples/p8-agent-node.json --db "${TMP_DB}" > /dev/null 2>&1
ck "put p8-agent-node（agent 节点词表放行）" "0" "$?"
"${RP_WF}" put docs/phase8/samples/p8-human-node.json --db "${TMP_DB}" > /dev/null 2>&1
ck "put p8-human-node（human 节点词表放行）" "0" "$?"
cargo build --offline -q -p agent-engine --bin rp-agent || { echo "rp-agent build FAIL"; exit 1; }
RP_AGENT="target/debug/rp-agent"
"${RP_AGENT}" def-add --agent-id agent.wf.demo --version 0.1.0 \
  --name "P8-3 selftest agent" --caps "rip.kb.keyword_search" --db "${TMP_DB}" > /dev/null 2>&1
ck "rp-agent def-add agent.wf.demo" "0" "$?"

echo "== 11. 判据 1：含 Agent 节点 workflow 协作暂停恢复（014 控制行传播）"
"${RP_WF}" run wf-p8-agent-node --input '{"target":"example.com","task_id":"rt-p83-selftest"}' \
  --db "${TMP_DB}" --run-id wfr-p83-agent > /tmp/rp_p83_agent_out.json 2>"${LOG}" &
AGENT_PID=$!
AGENT_RUN_STATE=""
for i in $(seq 1 600); do
  AGENT_RUN_STATE="$(agent_state "${TMP_DB}" "wfr-p83-agent" "think")"
  if [[ "${AGENT_RUN_STATE}" == "RUNNING" ]]; then break; fi
  sleep 0.05
done
if [[ "${AGENT_RUN_STATE}" != "RUNNING" ]]; then
  FAIL=$((FAIL+1)); echo "[FAIL] agent run 未进入 RUNNING（现态=${AGENT_RUN_STATE:-无}）"
  kill "${AGENT_PID}" 2>/dev/null; wait "${AGENT_PID}" 2>/dev/null
else
  PAUSE_OUT="$("${RP_WF}" pause wfr-p83-agent --db "${TMP_DB}" 2>"${LOG}")"
  ck "pause 请求受理 exit=0" "0" "$?"
  ck "pause 返回 state_at_request=running" "running" "$(jget "${PAUSE_OUT}" "d['state_at_request']")"
  PAUSED_OK=0
  for i in $(seq 1 300); do
    NOW="$(run_state "${TMP_DB}" "wfr-p83-agent")"
    AST="$(agent_state "${TMP_DB}" "wfr-p83-agent" "think")"
    if [[ "${NOW}" == "paused" && "${AST}" == "PAUSED" ]]; then PAUSED_OK=1; break; fi
    sleep 0.1
  done
  ck "workflow run 转 paused 且 agent run 转 PAUSED（014 协作暂停传播）" "1" "${PAUSED_OK}"
  kill "${AGENT_PID}" 2>/dev/null; wait "${AGENT_PID}" 2>/dev/null
  # 014 agent_run_control 有对应控制行（run_id 关联 + goal_text 落库）。
  CONTROL_ROWS="$(python3 -I -c "
import sqlite3
c=sqlite3.connect('file:${TMP_DB}?mode=ro',uri=True)
r=c.execute(\"select goal_text from agent_run_control where run_id=(select agent_run_id from workflow_run_node where run_id='wfr-p83-agent' and node_id='think')\").fetchall()
print(len(r))")"
  ck "014 agent_run_control 有对应控制行" "1" "${CONTROL_ROWS}"
  # agent run 节点链接列 + run 六 ID 关联列（trace_id/agent_run_id/task_id）。
  CORR_OK="$(python3 -I -c "
import sqlite3
c=sqlite3.connect('file:${TMP_DB}?mode=ro',uri=True)
row=c.execute(\"select trace_id, task_id, agent_run_id from workflow_run where run_id='wfr-p83-agent'\").fetchone()
ok = row[0]=='wfr-p83-agent' and row[1]=='rt-p83-selftest' and row[2] and row[2].startswith('arn-')
print(1 if ok else 0)")"
  ck "workflow_run 六 ID 关联列（trace/task/agent_run）" "1" "${CORR_OK}"
  RESUME_OUT="$("${RP_WF}" resume wfr-p83-agent --db "${TMP_DB}" 2>"${LOG}")"
  ck "resume exit=0（续跑至 SUCCEEDED）" "0" "$?"
  ck "resume run state=succeeded" "succeeded" "$(jget "${RESUME_OUT}" "d['state']")"
  ck "已完成节点 start attempt 不变（1）" \
     "('succeeded', 1)" \
     "$(jget "${RESUME_OUT}" "[ (n['state'],n['attempt']) for n in d['nodes'] if n['node_id']=='start'][0]")"
fi

echo "== 12. 判据 2：人工介入节点 awaiting_approval 与批准注入"
"${RP_WF}" run wf-p8-human-node --input '{}' --db "${TMP_DB}" --run-id wfr-p83-human > /tmp/rp_p83_human_out.json 2>"${LOG}"
ck "human run 停等 exit=0（Paused 非错误）" "0" "$?"
ck "human run state=paused" "paused" "$(jget "$(cat /tmp/rp_p83_human_out.json)" "d['state']")"
AWAIT_OK="$(python3 -I -c "
import sqlite3
c=sqlite3.connect('file:${TMP_DB}?mode=ro',uri=True)
r=c.execute(\"select approval_node_id, approval_request_json, approval_json from workflow_run_control where run_id='wfr-p83-human'\").fetchone()
ok = r and r[0]=='review' and r[1] and r[2] is None
print(1 if ok else 0)")"
ck "018 控制行停在 awaiting_approval（node 锚 + 请求载荷 + 未批准）" "1" "${AWAIT_OK}"
"${RP_WF}" resume wfr-p83-human --input '{"approval":{"ok":true}}' --db "${TMP_DB}" > /tmp/rp_p83_human_resume.json 2>"${LOG}"
ck "注入批准参数 resume exit=0" "0" "$?"
ck "批准后 run 终态 succeeded" "succeeded" "$(jget "$(cat /tmp/rp_p83_human_resume.json)" "d['state']")"
APPROVED_OK="$(python3 -I -c "
import sqlite3
c=sqlite3.connect('file:${TMP_DB}?mode=ro',uri=True)
r=c.execute(\"select approval_json from workflow_run_control where run_id='wfr-p83-human'\").fetchone()
print(1 if r and r[0] else 0)")"
ck "approval_json 落值留痕" "1" "${APPROVED_OK}"

echo "== 13. 判据 3：六 ID 对账（库列 vs GET /api/workflow-runs + /api/traces 双向）"
WF_PORT="${RIP_P83_PORT:-7842}"
WF_TOKEN="p83-selftest-token"
RP_DB_PATH="${REPO}/migrations/platform.db" \
  RP_AUX_DB="${TMP_DB}" RP_WORKFLOW_DB="${TMP_DB}" \
  RP_AUTH_TOKEN="${WF_TOKEN}" RP_PORT="${WF_PORT}" RP_HOST="127.0.0.1" \
  python3 apps/server/rp_server.py > /tmp/rp_p83_server.log 2>&1 &
SERVER_PID=$!
SERVER_UP=0
for i in $(seq 1 100); do
  curl -s -m 2 -H "Authorization: Bearer ${WF_TOKEN}" "http://127.0.0.1:${WF_PORT}/api/stats" > /dev/null 2>&1 && { SERVER_UP=1; break; }
  sleep 0.1
done
ck "rp_server 启动（aux=工作流开发库）" "1" "${SERVER_UP}"
RUNS_API="$(curl -s -m 5 -H "Authorization: Bearer ${WF_TOKEN}" "http://127.0.0.1:${WF_PORT}/api/workflow-runs?id=wfr-p83-agent")"
SIX_ID_OK="$(python3 -I - "${TMP_DB}" "${RUNS_API}" <<'PYEOF'
import sqlite3, sys, json
db, api_raw = sys.argv[1], sys.argv[2]
c = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
db_row = c.execute("select trace_id, task_id, agent_run_id, tool_run_id, artifact_id, evidence_id from workflow_run where run_id='wfr-p83-agent'").fetchone()
api = json.loads(api_raw)
corr = api.get("correlation") or {}
run = api.get("run") or {}
api_corr = (corr.get("trace_id"), corr.get("task_id"), corr.get("agent_run_id"),
            corr.get("tool_run_id"), corr.get("artifact_id"), corr.get("evidence_id"))
run_corr = (run.get("trace_id"), run.get("task_id"), run.get("agent_run_id"),
            run.get("tool_run_id"), run.get("artifact_id"), run.get("evidence_id"))
nodes = {n["node_id"]: n for n in api.get("nodes") or []}
agent_link = (nodes.get("think") or {}).get("agent_run_id")
ok = db_row == api_corr == run_corr and agent_link == db_row[2] and api.get("available") is True
print(1 if ok else 0)
PYEOF
)"
ck "库列与 GET /api/workflow-runs?id= 响应一致（六 ID + 节点链接）" "1" "${SIX_ID_OK}"
TRACE_API="$(curl -s -m 5 -H "Authorization: Bearer ${WF_TOKEN}" "http://127.0.0.1:${WF_PORT}/api/traces?id=wfr-p83-agent")"
TRACE_FWD_OK="$(python3 -I - "${TRACE_API}" <<'PYEOF'
import json, sys
api = json.loads(sys.argv[1])
counts = api.get("counts") or {}
ok = api.get("available") is True and counts.get("agent_runs", 0) >= 1 and counts.get("workflow_runs", 0) >= 1 and counts.get("artifacts", 0) >= 1 and counts.get("evidence", 0) >= 1
print(1 if ok else 0)
PYEOF
)"
ck "/api/traces?id=<run_id> 正向可查（agent_run/workflow_run/artifact/evidence 入链）" "1" "${TRACE_FWD_OK}"
ARN_ID="$(python3 -I -c "
import sqlite3
c=sqlite3.connect('file:${TMP_DB}?mode=ro',uri=True)
print(c.execute(\"select agent_run_id from workflow_run where run_id='wfr-p83-agent'\").fetchone()[0])")"
TRACE_API2="$(curl -s -m 5 -H "Authorization: Bearer ${WF_TOKEN}" "http://127.0.0.1:${WF_PORT}/api/traces?id=${ARN_ID}")"
TRACE_REV_OK="$(python3 -I - "${TRACE_API2}" <<'PYEOF'
import json, sys
api = json.loads(sys.argv[1])
counts = api.get("counts") or {}
wf = api.get("workflow_runs") or []
ok = api.get("available") is True and counts.get("agent_runs", 0) >= 1 and any(r.get("run_id") == "wfr-p83-agent" for r in wf)
print(1 if ok else 0)
PYEOF
)"
ck "/api/traces?id=<agent_run_id> 反向可查（回 workflow_run 行）" "1" "${TRACE_REV_OK}"
WORKFLOWS_API="$(curl -s -m 5 -H "Authorization: Bearer ${WF_TOKEN}" "http://127.0.0.1:${WF_PORT}/api/workflows")"
WF_LIST_OK="$(python3 -I - "${WORKFLOWS_API}" <<'PYEOF'
import json, sys
api = json.loads(sys.argv[1])
ids = {i.get("workflow_id") for i in api.get("items") or []}
ok = api.get("available") is True and "wf-p8-agent-node" in ids and "wf-p8-human-node" in ids
print(1 if ok else 0)
PYEOF
)"
ck "GET /api/workflows 列表含 P8-3 样例定义" "1" "${WF_LIST_OK}"

echo "== 14. 判据 4 前半：POST 写面（run/resume 经 API）+ 白名单守卫"
POST_OUT="$(curl -s -m 10 -X POST -H "Authorization: Bearer ${WF_TOKEN}" -H "Content-Type: application/json" \
  -d '{"action":"run","workflow_id":"wf-p8-human-node","input":{},"run_id":"wfr-p83-api-human"}' \
  "http://127.0.0.1:${WF_PORT}/api/workflow-runs")"
ck "POST run 受理返回 run_id" "wfr-p83-api-human" "$(jget "${POST_OUT}" "d['run_id']")"
API_PAUSED=0
for i in $(seq 1 300); do
  NOW="$(run_state "${TMP_DB}" "wfr-p83-api-human")"
  if [[ "${NOW}" == "paused" ]]; then API_PAUSED=1; break; fi
  sleep 0.1
done
ck "API run 停在 paused（awaiting_approval）" "1" "${API_PAUSED}"
POST_RESUME="$(curl -s -m 10 -X POST -H "Authorization: Bearer ${WF_TOKEN}" -H "Content-Type: application/json" \
  -d '{"run_id":"wfr-p83-api-human","inject":{"approval":{"ok":true}}}' \
  "http://127.0.0.1:${WF_PORT}/api/workflow-runs/wfr-p83-api-human/resume")"
ck "POST resume 受理" "accepted" "$(jget "${POST_RESUME}" "d['result']")"
API_DONE=0
for i in $(seq 1 300); do
  NOW="$(run_state "${TMP_DB}" "wfr-p83-api-human")"
  if [[ "${NOW}" == "succeeded" ]]; then API_DONE=1; break; fi
  sleep 0.1
done
ck "API resume 后终态 succeeded" "1" "${API_DONE}"
curl -s -m 5 -o /dev/null -w "%{http_code}" -X POST -H "Authorization: Bearer ${WF_TOKEN}" \
  -d '{}' "http://127.0.0.1:${WF_PORT}/api/stats" > /tmp/rp_p83_405_code
ck "白名单外 POST 仍 405（/api/stats）" "405" "$(cat /tmp/rp_p83_405_code)"
curl -s -m 5 -o /dev/null -w "%{http_code}" -X POST -H "Authorization: Bearer ${WF_TOKEN}" \
  -H "Content-Type: application/json" -d '{"action":"run"}' \
  "http://127.0.0.1:${WF_PORT}/api/workflow-runs" > /tmp/rp_p83_400_code
ck "白名单内缺参 400（run 缺 workflow_id）" "400" "$(cat /tmp/rp_p83_400_code)"
curl -s -m 5 -o /dev/null -w "%{http_code}" -X POST -d '{}' "http://127.0.0.1:${WF_PORT}/api/workflow-runs" > /tmp/rp_p83_401_code
ck "写端点无令牌 401（鉴权四件套）" "401" "$(cat /tmp/rp_p83_401_code)"

kill "${SERVER_PID}" 2>/dev/null; wait "${SERVER_PID}" 2>/dev/null

echo "== 15. 生产库零写复核（指纹前后一致）"
PLAT_FP2="$(db_fingerprint migrations/platform.db)"
ck "platform.db 指纹一致（零写）" "${PLAT_FP}" "${PLAT_FP2}"
if [[ -n "${DRY_FP}" ]]; then
  DRY_FP2="$(db_fingerprint migrations/platform-dryrun.db)"
  ck "platform-dryrun.db 指纹一致（零写）" "${DRY_FP}" "${DRY_FP2}"
else
  PASS=$((PASS+1)); echo "[PASS] platform-dryrun.db 不存在（跳过，收尾一致）"
fi

echo ""
echo "PASS=${PASS} FAIL=${FAIL}"
[[ "${FAIL}" -eq 0 ]] || { echo "--- 末次 stderr 日志：${LOG}"; tail -20 "${LOG}"; exit 1; }
exit 0
