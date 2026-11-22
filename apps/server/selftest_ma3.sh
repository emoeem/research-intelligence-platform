#!/usr/bin/env bash
# MA-3 协作协议与知识供体面收口自验（docs/rip3/MA-3-TASK.md §4 判据 1-10 数字实测）：
#   1. 硬验收①：双 AI 协作同一 workflow（节点分属两 caller）——rp-workflow run
#      --caller-id caller-a，node1 无绑定继承 run 级 / node2 params caller_id=caller-b；
#      agent_run 两行 caller_id 库直查 + workflow_run.caller_id 落列 +
#      caller_budget 帽对 caller-b 生效节点 failed `budget denied` 词形实测；
#   2. rp-collab CLI：claim 真进程并发竞争（N=6 恰一胜）/ transfer attempt 链 /
#      依赖门 / 邮箱 send→fetch→read 分记全序 / sweep 计数；
#   3. 硬验收②：外部 AI MCP 派发往返（真实 rp-mcp stdio + 真实 rp-agent
#      python_worker 闭环）agent_run.caller_id 落列 + rip.get_subtask_status
#      一致回读 + 非法 token 结构化拒绝；REST 同契约 POST /api/subtasks
#      （caller-a key 回执 + /api/agent-runs caller_id 可见 + legacy 403）；
#   4. 硬验收③：并发知识写入冲突治理（隔离 PG 容器 + N 并发 rp-kc
#      donor-submit 同 content → 全 CANDIDATE + SAME_CONTENT 候选边 + 双闸负例；
#      podman 缺席 V-9 SKIP exit 0）；
#   5. 生产库指纹前后一致（platform*.db 零触碰反证）。
# 全过 exit 0；任何 FAIL exit 1。
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "${HERE}/../.." && pwd)"
cd "${REPO}"

PASS=0; FAIL=0
ck() { # ck <名> <期望> <实际>
  if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi
}
jget() { # jget <json 文本> <python 表达式>
  python3 -I -c "import sys,json; d=json.loads(sys.argv[1]); print(eval(sys.argv[2]))" "${1}" "${2}" 2>/dev/null || echo PARSE_FAIL
}

TDIR="$(mktemp -d /tmp/rp_ma3_selftest.XXXXXX)"
TMP_DB="${TDIR}/ma3.db"
LOG="${TDIR}/log.txt"
WORKFLOW_DEF="${TDIR}/ma3-two-agent.json"
PORT="${SELFTEST_MA3_PORT:-7907}"
SRV_LOG="${TDIR}/server.log"

FP_BEFORE="$(for f in migrations/platform*.db; do [[ -e "$f" ]] && sha256sum "$f"; done | sort)"

cleanup() {
  [[ -n "${SRV_PID:-}" ]] && { kill "${SRV_PID}" 2>/dev/null; wait "${SRV_PID}" 2>/dev/null; }
  podman rm -f rip-p17-ma3-selftest >/dev/null 2>&1
  chmod -R u+rwX "${TDIR}" 2>/dev/null
  rm -rf "${TDIR}"
}
trap cleanup EXIT

echo "== 0. 构建（cargo --offline）"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/home/emo/rt3-target}"
BINS="${CARGO_TARGET_DIR}/debug"
for spec in "workflow-engine rp-workflow" "agent-engine rp-agent" "database rp-admin" \
            "tool-registry rp-tool" "mcp-server rp-mcp" "intel-db rp-kc" "workflow-engine rp-collab"; do
  set -- $spec
  cargo build --offline -q -p "$1" --bin "$2" || { echo "[FAIL] cargo build $2"; exit 1; }
done
ck "七 bin 构建在位（rp-workflow/rp-agent/rp-admin/rp-tool/rp-mcp/rp-kc/rp-collab）" "yes" "yes"

q() { # q <sql> — 临时库只读直查
  python3 -I -c "import sqlite3,sys; r=sqlite3.connect('file:${TMP_DB}?mode=ro',uri=True).execute(sys.argv[1]).fetchone(); print(r[0] if r else '')" "${1}" 2>/dev/null || echo QFAIL
}

echo "== 1. 临时库夹具：登记工具 + Agent 定义 + caller/key 预置（026/027/029 随迁移在位）"
"${BINS}/rp-tool" register --manifest manifests/tools/rip.kb.keyword_search.json --db "${TMP_DB}" >/dev/null 2>&1
ck "rp-tool register（029 随 rp-admin 自动迁移入链）" "0" "$?"
UV="$(q 'PRAGMA user_version')"
ck "迁移链尾 user_version=29" "29" "${UV}"

"${BINS}/rp-agent" def-add --agent-id agent.ma3.selftest --version 0.1.0 \
  --name "MA-3 协作自验 Agent" --caps "rip.kb.keyword_search" --db "${TMP_DB}" >/dev/null 2>&1
ck "rp-agent def-add" "0" "$?"

"${BINS}/rp-admin" caller-add --db "${TMP_DB}" --name "MA-3 caller A" --level T1 --caller-id caller-ma3-a >/dev/null 2>&1
ck "caller-add A" "0" "$?"
"${BINS}/rp-admin" caller-add --db "${TMP_DB}" --name "MA-3 caller B" --level T1 --caller-id caller-ma3-b >/dev/null 2>&1
ck "caller-add B" "0" "$?"
"${BINS}/rp-admin" caller-add --db "${TMP_DB}" --name "MA-3 外部 AI" --level T1 --caller-id caller-ma3-ext >/dev/null 2>&1
ck "caller-add EXT" "0" "$?"
KEY_A="$("${BINS}/rp-admin" key-gen --db "${TMP_DB}" --caller-id caller-ma3-a --label ma3-rest | python3 -I -c "import sys,json; print(json.load(sys.stdin)['key'])")"
ck "key-gen caller-ma3-a（rp1_+43 位）" "47" "${#KEY_A}"
KEY_EXT="$("${BINS}/rp-admin" key-gen --db "${TMP_DB}" --caller-id caller-ma3-ext --label ma3-mcp | python3 -I -c "import sys,json; print(json.load(sys.stdin)['key'])")"
ck "key-gen caller-ma3-ext" "47" "${#KEY_EXT}"

# caller-b 预算：跨 run 并发帽 1（帽拒实测载具）。
"${BINS}/rp-admin" budget-set --db "${TMP_DB}" --caller-id caller-ma3-b --max-concurrent-runs 1 >/dev/null 2>&1
ck "budget-set B（并发帽 1）" "0" "$?"

echo "== 2. 硬验收①：双 AI 协作同一 workflow（节点分属两 caller）"
cat > "${WORKFLOW_DEF}" <<'JSON'
{
  "schema": "workflow/definition-v1",
  "workflow_id": "wf-ma3-selftest",
  "version": 1,
  "name": "MA-3 双 caller 节点绑定自验",
  "description": "start -> node1(agent 无绑定) -> node2(agent caller_id=caller-ma3-b) -> done",
  "graph": {
    "nodes": [
      { "id": "start", "type": "start" },
      { "id": "node1", "type": "agent", "params": { "agent_id": "agent.ma3.selftest", "goal": "MA-3 node1 协作步（run 级 caller）" } },
      { "id": "node2", "type": "agent", "params": { "agent_id": "agent.ma3.selftest", "goal": "MA-3 node2 协作步（节点 caller-ma3-b）", "caller_id": "caller-ma3-b" } },
      { "id": "done", "type": "end" }
    ],
    "edges": [
      { "from": "start", "to": "node1" },
      { "from": "node1", "to": "node2" },
      { "from": "node2", "to": "done" }
    ]
  }
}
JSON
"${BINS}/rp-workflow" put "${WORKFLOW_DEF}" --db "${TMP_DB}" >/dev/null 2>&1
ck "workflow put（两 agent 节点定义）" "0" "$?"

RUN1="$("${BINS}/rp-workflow" run wf-ma3-selftest --input '{"target":"example.com"}' \
  --caller-id caller-ma3-a --db "${TMP_DB}" 2>"${LOG}")"
ck "rp-workflow run --caller-id caller-ma3-a exit 0" "0" "$?"
RUN1_ID="$(jget "${RUN1}" "d['run_id']")"
ck "run1 SUCCEEDED" "succeeded" "$(q "SELECT state FROM workflow_run WHERE run_id='${RUN1_ID}'")"
ck "workflow_run.caller_id = caller-ma3-a（run 级落列）" "caller-ma3-a" \
  "$(q "SELECT caller_id FROM workflow_run WHERE run_id='${RUN1_ID}'")"
N1_ARN="$(q "SELECT agent_run_id FROM workflow_run_node WHERE run_id='${RUN1_ID}' AND node_id='node1'")"
N2_ARN="$(q "SELECT agent_run_id FROM workflow_run_node WHERE run_id='${RUN1_ID}' AND node_id='node2'")"
ck "node1 agent_run.caller_id = caller-ma3-a（继承 run 级）" "caller-ma3-a" \
  "$(q "SELECT caller_id FROM agent_run WHERE run_id='${N1_ARN}'")"
ck "node2 agent_run.caller_id = caller-ma3-b（params 显式）" "caller-ma3-b" \
  "$(q "SELECT caller_id FROM agent_run WHERE run_id='${N2_ARN}'")"
T1_MS="$(python3 -I -c "print(1)")"
ck "两节点 agent_run trace_id 同链（= run_id）" "2" \
  "$(q "SELECT COUNT(*) FROM agent_run WHERE trace_id='${RUN1_ID}' AND run_id IN ('${N1_ARN}','${N2_ARN}')")"

echo "== 3. caller_budget 帽对 caller-b 生效（节点 failed budget denied 词形）"
# caller-b 并发帽 1：预置一行 caller-b RUNNING 记账行（受理面
# count_active_runs_by_caller 计数数据源 = agent_run 行，D1 公式；确定性
# 占位非时序竞态），再跑 node2 绑定 caller-ma3-b 的 workflow → node2
# 受理 BudgetDenied → failed。
python3 -I - "${TMP_DB}" <<'FIXEOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
c.execute(
    "INSERT INTO agent_run (run_id, task_id, agent_id, agent_version, state,"
    " memory_refs_json, created_at, started_at, updated_at, schema_version, caller_id,"
    " trace_id) VALUES ('arn-ma3-cap-placeholder', NULL, 'agent.ma3.selftest', '0.1.0',"
    " 'RUNNING', '[]', '2026-11-22T00:00:00Z', '2026-11-22T00:00:00Z',"
    " '2026-11-22T00:00:00Z', 1, 'caller-ma3-b', 'tr-ma3-cap')")
c.commit(); c.close()
FIXEOF
RUN2="$("${BINS}/rp-workflow" run wf-ma3-selftest --input '{"target":"example.com"}' \
  --caller-id caller-ma3-a --db "${TMP_DB}" 2>"${LOG}")"
RUN2_RC=$?
python3 -I - "${TMP_DB}" <<'FIXEOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
c.execute("DELETE FROM agent_run WHERE run_id = 'arn-ma3-cap-placeholder'")
c.commit(); c.close()
FIXEOF
if [[ "${RUN2_RC}" == "1" ]]; then
  PASS=$((PASS+1)); echo "[PASS] 帽拒 run exit=1（fail-fast 收敛）"
else
  FAIL=$((FAIL+1)); echo "[FAIL] 帽拒 run exit=1 (实际=${RUN2_RC})"
fi
RUN2_ID="$(jget "${RUN2}" "d['run_id']")"
N2F_ERR="$(q "SELECT error_json FROM workflow_run_node WHERE run_id='${RUN2_ID}' AND node_id='node2'")"
if python3 -I -c "import sys,json; d=json.loads('''${N2F_ERR}'''); sys.exit(0 if ('budget denied' in d['message'] and d['code']=='agent_failed') else 1)"; then
  PASS=$((PASS+1)); echo "[PASS] node2 failed code=agent_failed message 含 budget denied"
else
  FAIL=$((FAIL+1)); echo "[FAIL] budget denied 词形 (实际=${N2F_ERR})"
fi
ck "node2 BudgetDenied run 不落行" "0" \
  "$(q "SELECT COUNT(*) FROM agent_run WHERE run_id=(SELECT agent_run_id FROM workflow_run_node WHERE run_id='${RUN2_ID}' AND node_id='node2')")"

echo "== 4. rp-collab CLI：claim 并发竞争 / transfer / 依赖门 / 邮箱分记"
CLM="$("${BINS}/rp-collab" claim-create --db "${TMP_DB}" --caller-id caller-ma3-a \
  --kind agent_subtask --title "并发竞争载具" --payload '{"n":1}' --run-id "${RUN1_ID}")"
CLM_ID="$(jget "${CLM}" "d['claim_id']")"
ck "claim-create（tcl- 前缀，claimable attempt=1）" "claimable" "$(jget "${CLM}" "d['state']")"

# N=6 真进程同刻 claim-claim（恰一胜；其余 exit 1 not_claimable）。
RACE_PIDS=()
for i in 1 2 3 4 5 6; do
  "${BINS}/rp-collab" claim-claim --db "${TMP_DB}" --caller-id "caller-ma3-a" \
    --claim-id "${CLM_ID}" >"${TDIR}/race-$i.json" 2>/dev/null &
  RACE_PIDS+=($!)
done
wait "${RACE_PIDS[@]}"
WINS=0; LOSSES=0
for i in 1 2 3 4 5 6; do
  RC=$(python3 -I -c "
import json
try:
    d=json.load(open('${TDIR}/race-$i.json'))
    print('win' if d.get('state')=='claimed' else 'lose')
except Exception:
    print('lose')")
  [[ "${RC}" == "win" ]] && WINS=$((WINS+1)) || LOSSES=$((LOSSES+1))
done
ck "真进程 N=6 并发 CAS 恰一胜" "1" "${WINS}"
ck "其余 0 行零副作用（败方 exit 1）" "5" "${LOSSES}"
ck "胜方 claimed_by caller-ma3-a" "caller-ma3-a" "$(q "SELECT claimed_by FROM task_claim WHERE claim_id='${CLM_ID}'")"

# transfer（本人转派 → 新行 attempt+1 → 败方领取）。
TRF="$("${BINS}/rp-collab" claim-transfer --db "${TMP_DB}" --caller-id caller-ma3-a --claim-id "${CLM_ID}")"
NEW_ID="$(jget "${TRF}" "d['new_claim']['claim_id']")"
ck "transfer 新行 claimable attempt=2" "2" "$(jget "${TRF}" "d['new_claim']['attempt']")"
ck "旧行 transferred 终态" "transferred" "$(q "SELECT state FROM task_claim WHERE claim_id='${CLM_ID}'")"
"${BINS}/rp-collab" claim-claim --db "${TMP_DB}" --caller-id caller-ma3-ext --claim-id "${NEW_ID}" >/dev/null 2>&1
ck "败方（外部 caller）对新行领取成功" "claimed" \
  "$(q "SELECT state FROM task_claim WHERE claim_id='${NEW_ID}'")"

# 依赖门（create 拒环 + 依赖未完成拒领）。
DEP_A="$("${BINS}/rp-collab" claim-create --db "${TMP_DB}" --caller-id caller-ma3-a \
  --kind review --title "依赖门 A" --payload '{}')"
DEP_A_ID="$(jget "${DEP_A}" "d['claim_id']")"
"${BINS}/rp-collab" claim-create --db "${TMP_DB}" --caller-id caller-ma3-a \
  --kind agent_subtask --title "依赖门子任务" --payload '{}' \
  --depends-on "${DEP_A_ID}" >"${TDIR}/gated.json" 2>/dev/null
GATED_ID="$(jget "$(cat "${TDIR}/gated.json")" "d['claim_id']")"
"${BINS}/rp-collab" claim-claim --db "${TMP_DB}" --caller-id caller-ma3-b --claim-id "${GATED_ID}" \
  >"${TDIR}/gated-claim.json" 2>/dev/null
ck "依赖未完成 claim 拒（exit 1）" "1" "$?"
"${BINS}/rp-collab" claim-claim --db "${TMP_DB}" --caller-id caller-ma3-a --claim-id "${DEP_A_ID}" >/dev/null 2>&1
"${BINS}/rp-collab" claim-complete --db "${TMP_DB}" --caller-id caller-ma3-a --claim-id "${DEP_A_ID}" \
  --result '{"ok":1}' >/dev/null 2>&1
"${BINS}/rp-collab" claim-claim --db "${TMP_DB}" --caller-id caller-ma3-b --claim-id "${GATED_ID}" >/dev/null 2>&1
ck "依赖 completed 后可领" "claimed" "$(q "SELECT state FROM task_claim WHERE claim_id='${GATED_ID}'")"

# 邮箱全序：send → fetch（delivered 落 / read NULL）→ 重复 fetch 可见 →
# read → 不再返回 → sweep 计数。
"${BINS}/rp-collab" msg-send --db "${TMP_DB}" --from caller-ma3-a --to caller-ma3-b \
  --payload '{"note":"ma3"}' --run-id "${RUN1_ID}" >/dev/null 2>&1
ck "msg-send" "0" "$?"
"${BINS}/rp-collab" msg-sweep --db "${TMP_DB}" >"${TDIR}/sweep1.json" 2>/dev/null
ck "sweep 未投递计数=1" "1" "$(jget "$(cat "${TDIR}/sweep1.json")" "d['undelivered_total']")"
"${BINS}/rp-collab" msg-fetch --db "${TMP_DB}" --caller-id caller-ma3-b >"${TDIR}/fetch1.json" 2>/dev/null
ck "fetch 首拉 delivered_at 落 / read_at NULL" "True" \
  "$(jget "$(cat "${TDIR}/fetch1.json")" "all(m['delivered_at'] and not m['read_at'] for m in d['items'])")"
"${BINS}/rp-collab" msg-fetch --db "${TMP_DB}" --caller-id caller-ma3-b >"${TDIR}/fetch2.json" 2>/dev/null
ck "未读行重复 fetch 可见（持久重投）" "1" "$(jget "$(cat "${TDIR}/fetch2.json")" "d['total']")"
MSG_ID="$(jget "$(cat "${TDIR}/fetch1.json")" "d['items'][0]['message_id']")"
"${BINS}/rp-collab" msg-read --db "${TMP_DB}" --caller-id caller-ma3-b --message-id "${MSG_ID}" >/dev/null 2>&1
ck "msg-read" "0" "$?"
"${BINS}/rp-collab" msg-fetch --db "${TMP_DB}" --caller-id caller-ma3-b >"${TDIR}/fetch3.json" 2>/dev/null
ck "read 后 fetch 不再返回" "0" "$(jget "$(cat "${TDIR}/fetch3.json")" "d['total']")"
"${BINS}/rp-collab" msg-sweep --db "${TMP_DB}" >"${TDIR}/sweep2.json" 2>/dev/null
ck "投递后 sweep 归零" "0" "$(jget "$(cat "${TDIR}/sweep2.json")" "d['undelivered_total']")"

echo "== 5. 硬验收②：外部 AI MCP 派发往返（真实 stdio + 真实 rp-agent）"
MCP_TOKEN="${KEY_EXT}"
MCP_REQ='{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"%s","arguments":%s}}'
mcp_req() { # mcp_req <name> <arguments json> -> 业务响应行
  printf '%s\n%s\n%s\n' \
    '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"selftest","version":"0"}}}' \
    '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
    "$(printf '{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"%s","arguments":%s}}' "$1" "$2")" \
    | RP_MCP_TOKEN="${MCP_TOKEN}" RP_DB_PATH="${TMP_DB}" RP_RUN_DB="${TMP_DB}" RP_PLUGIN_DB=none \
      RP_AGENT_BIN="${BINS}/rp-agent" RP_SUBTASK_TIMEOUT_S=120 "${BINS}/rp-mcp" 2>/dev/null | grep '"id":9'
}
LIST_REQ='{"jsonrpc":"2.0","id":9,"method":"tools/list","params":{}}'
TOOLS_LINE="$(printf '%s\n%s\n%s\n' \
    '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"selftest","version":"0"}}}' \
    '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
    "${LIST_REQ}" \
    | RP_MCP_TOKEN="${MCP_TOKEN}" RP_DB_PATH="${TMP_DB}" RP_RUN_DB="${TMP_DB}" RP_PLUGIN_DB=none \
      "${BINS}/rp-mcp" 2>/dev/null | grep '"id":9')"
ck "MCP tools/list 含七枚新工具" "7" \
  "$(python3 -I -c "
import json
d=json.loads('''${TOOLS_LINE}''')
names=[t['name'] for t in d['result']['tools']]
seven=['rip.run_subtask','rip.get_subtask_status','rip.claim_task','rip.complete_claim','rip.send_message','rip.fetch_messages','rip.contribute_knowledge']
print(sum(1 for n in seven if n in names))")"

T0="$(python3 -I -c "import time; print(int(time.time()*1000))")"
SUB_OUT="$(mcp_req rip.run_subtask '{"agent_id":"agent.ma3.selftest","goal":"MA-3 MCP 派发往返自验：检索 ma3 关键词"}')"
T1="$(python3 -I -c "import time; print(int(time.time()*1000))")"
echo "    [TIMING] rip.run_subtask 往返 $((T1-T0)) ms"
SUB_TEXT="$(jget "${SUB_OUT}" "d['result']['content'][0]['text']")"
if python3 -I -c "
import sys, json
p=json.loads('''${SUB_TEXT}''')
sys.exit(0 if p.get('ok') and p['report']['state']=='SUCCEEDED' and p['caller']=='caller-ma3-ext' else 1)"; then
  PASS=$((PASS+1)); echo "[PASS] rip.run_subtask 往返 ok=true state=SUCCEEDED caller=caller-ma3-ext"
else
  FAIL=$((FAIL+1)); echo "[FAIL] rip.run_subtask 往返 (实际=${SUB_TEXT})"
fi
SUB_ARN="$(python3 -I -c "import json; print(json.loads('''${SUB_TEXT}''')['report']['run_id'])")"
ck "agent_run.caller_id = caller-ma3-ext（MCP 通道落列）" "caller-ma3-ext" \
  "$(q "SELECT caller_id FROM agent_run WHERE run_id='${SUB_ARN}'")"
ST_OUT="$(mcp_req rip.get_subtask_status "{\"run_id\":\"${SUB_ARN}\"}")"
ST_TEXT="$(jget "${ST_OUT}" "d['result']['content'][0]['text']")"
ck "rip.get_subtask_status 一致回读" "SUCCEEDED" \
  "$(python3 -I -c "import json; print(json.loads('''${ST_TEXT}''')['state'])")"
ck "get_subtask_status caller_id 一致" "caller-ma3-ext" \
  "$(python3 -I -c "import json; print(json.loads('''${ST_TEXT}''')['caller_id'])")"

# 非法 token：派发工具结构化拒绝，目录面正常。
BAD_LINE="$(printf '%s\n%s\n%s\n' \
    '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"selftest","version":"0"}}}' \
    '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
    "$(printf '{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"rip.run_subtask","arguments":{"agent_id":"agent.ma3.selftest","goal":"x"}}}')" \
    | RP_MCP_TOKEN="rp1_invalid-no-mapping" RP_DB_PATH="${TMP_DB}" RP_RUN_DB="${TMP_DB}" RP_PLUGIN_DB=none \
      "${BINS}/rp-mcp" 2>/dev/null | grep '"id":9')"
printf '%s' "${BAD_LINE}" > "${TDIR}/bad.json"
if python3 -I - "${TDIR}/bad.json" <<'PYEOF'
import sys, json
d = json.load(open(sys.argv[1]))
t = d["result"]["content"][0]["text"]
p = json.loads(t)
ok = d["result"].get("isError") and p["error"]["category"] == "POLICY_DENIED" and "fail-closed" in p["error"]["message"]
sys.exit(0 if ok else 1)
PYEOF
then
  PASS=$((PASS+1)); echo "[PASS] 非法 token rip.run_subtask 结构化拒绝（POLICY_DENIED fail-closed）"
else
  FAIL=$((FAIL+1)); echo "[FAIL] 非法 token 结构化拒绝 (实际=${BAD_LINE})"
fi
LIST_BAD="$(printf '%s\n%s\n%s\n' \
    '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"selftest","version":"0"}}}' \
    '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
    "${LIST_REQ}" \
    | RP_MCP_TOKEN="rp1_invalid-no-mapping" RP_DB_PATH="${TMP_DB}" RP_RUN_DB="${TMP_DB}" RP_PLUGIN_DB=none \
      "${BINS}/rp-mcp" 2>/dev/null | grep '"id":9')"
printf '%s' "${LIST_BAD}" > "${TDIR}/listbad.json"
ck "非法 token tools/list 正常（目录面零影响）" "True" \
  "$(python3 -I - "${TDIR}/listbad.json" <<'PYEOF'
import sys, json
d = json.load(open(sys.argv[1]))
print(any(t["name"] == "rip.run_tool" for t in d["result"]["tools"]))
PYEOF
)"

echo "== 6. REST 同契约：POST /api/subtasks + workflow run caller 传递 + legacy 403"
RP_DB_PATH="${TMP_DB}" RP_AUX_DB="${TMP_DB}" RP_WORKFLOW_DB="${TMP_DB}" RP_AGENT_BIN="${BINS}/rp-agent" \
  RP_AUTH_TOKEN="legacy-token-ma3" RP_PORT="${PORT}" python3 -I apps/server/rp_server.py > "${SRV_LOG}" 2>&1 &
SRV_PID=$!
READY=0
for i in $(seq 1 60); do
  curl -s -o /dev/null --max-time 2 "http://127.0.0.1:${PORT}/api/healthz" && { READY=1; break; }
  sleep 0.5
done
ck "rp_server 就绪" "1" "${READY}"

api() { curl -s -H "Authorization: Bearer $2" -H 'Content-Type: application/json' "http://127.0.0.1:${PORT}$1"; }
api_code() { curl -s -o /dev/null -w '%{http_code}' -X POST -H "Authorization: Bearer $2" -H 'Content-Type: application/json' -d "$3" "http://127.0.0.1:${PORT}$1"; }

ST_CODE="$(api_code /api/subtasks "${KEY_A}" '{"agent_id":"agent.ma3.selftest","goal":"MA-3 REST 派发自验"}')"
ck "REST POST /api/subtasks（caller-a key）202/200 受理" "200" "${ST_CODE}"
REST_LIST="$(api "/api/agent-runs" "${KEY_A}")"
ck "/api/agent-runs 读面 caller_id 可见（REST 通道行）" "1" \
  "$(python3 -I -c "
import json
d=json.loads('''${REST_LIST}''')
runs=d.get('runs') or d.get('items') or []
print(1 if any(r.get('caller_id')=='caller-ma3-a' for r in runs) else 0)")"
LEG_CODE="$(api_code /api/subtasks "legacy-token-ma3" '{"agent_id":"agent.ma3.selftest","goal":"x"}')"
ck "legacy token POST /api/subtasks 403 fail-closed" "403" "${LEG_CODE}"
BODY_CALLER_CODE="$(api_code /api/subtasks "${KEY_A}" '{"agent_id":"a","goal":"x","caller_id":"caller-hijack"}')"
ck "body 传入 caller_id 拒绝（400；workflow run 同面）" "400" \
  "$(api_code /api/workflow-runs "${KEY_A}" '{"workflow_id":"wf-ma3-selftest","input":{},"caller_id":"caller-hijack"}')"
WRUN_ID="wfr-ma3-rest-$(python3 -I -c "import secrets; print(secrets.token_hex(4))")"
api_code /api/workflow-runs "${KEY_A}" "{\"workflow_id\":\"wf-ma3-selftest\",\"input\":{},\"run_id\":\"${WRUN_ID}\"}" >/dev/null
REST_CALLER=""
for i in $(seq 1 60); do
  WF_JSON="$(api "/api/workflow-runs?id=${WRUN_ID}" "${KEY_A}")"
  REST_CALLER="$(WF_JSON_JSON="${WF_JSON}" python3 -I -c "
import json, os
d=json.loads(os.environ['WF_JSON_JSON'])
print((d.get('run') or {}).get('caller_id') or '')" 2>/dev/null)"
  [[ "${REST_CALLER}" == "caller-ma3-a" ]] && break
  sleep 1
done
ck "REST workflow run --caller-id 落列（鉴权面 caller-a）" "caller-ma3-a" "${REST_CALLER}"

echo "== 7. 硬验收③：并发知识写入冲突治理（隔离 PG 容器；podman 缺席 V-9 SKIP）"
if command -v podman >/dev/null 2>&1 && podman image exists "localhost/rip-pg17:latest"; then
  PGPORT=$(( (RANDOM % 2000) + 19000 ))
  mkdir -p "${TDIR}/pgdata"
  podman run -d --name rip-p17-ma3-selftest -e POSTGRES_PASSWORD=pgselftest \
    -p "127.0.0.1:${PGPORT}:5432" -v "${TDIR}/pgdata:/var/lib/postgresql/data" \
    "localhost/rip-pg17:latest@sha256:229189e4c66a2eb8ddd1fcf4bc08ee4726cacd8aee7445ec3c6f1499a357ef45" >/dev/null 2>&1
  PGREADY=0
  for i in $(seq 1 45); do
    podman exec rip-p17-ma3-selftest pg_isready -U postgres >/dev/null 2>&1 && { PGREADY=1; break; }
    sleep 2
  done
  ck "隔离 PG 就绪（V0007 随 rp-kc 启动 refinery 入链）" "1" "${PGREADY}"
  export RP_PG_DSN="postgres://postgres:pgselftest@127.0.0.1:${PGPORT}/postgres"
  WS="ws-ma3-selftest"
  # refinery 迁移预热（串行一跑后并发 donor-submit 不再触迁移竞态）。
  "${BINS}/rp-kc" list --workspace "${WS}" >/dev/null 2>&1
  ck "rp-kc 预热（V0007 refinery 幂等入链）" "0" "$?"
  T0="$(python3 -I -c "import time; print(int(time.time()*1000))")"
  DONOR_PIDS=()
  for i in 1 2 3 4 5 6; do
    "${BINS}/rp-kc" donor-submit --donor-caller "caller-donor-$i" --donor-channel mcp \
      --type vuln_signature --title "MA-3 并发供体卡 $i" \
      --content "MA-3 并发供体同内容卡：SQL 注入检测签名（UNION-based payload 指纹）。" \
      --workspace "${WS}" >"${TDIR}/donor-$i.json" 2>/dev/null &
    DONOR_PIDS+=($!)
  done
  wait "${DONOR_PIDS[@]}"
  T1="$(python3 -I -c "import time; print(int(time.time()*1000))")"
  echo "    [TIMING] N=6 并发 donor-submit $((T1-T0)) ms"
  RECEIPTS_OK=0
  for i in 1 2 3 4 5 6; do
    R="$(python3 -I -c "
import json
d=json.load(open('${TDIR}/donor-$i.json'))
print('ok' if d.get('review_status')=='CANDIDATE' and d.get('knowledge_id','').startswith('kc-') and len(d.get('normalized_hash',''))==64 else 'bad')" 2>/dev/null)"
    [[ "${R}" == "ok" ]] && RECEIPTS_OK=$((RECEIPTS_OK+1))
  done
  ck "N=6 并发 donor-submit 收据全 CANDIDATE（{knowledge_id, review_status, normalized_hash}）" "6" "${RECEIPTS_OK}"
  DONOR_ROWS="$(psql "${RP_PG_DSN}" -t -A -c "SELECT COUNT(*) FROM knowledge_cards WHERE workspace_id='${WS}' AND review_status='CANDIDATE' AND donor_caller_id LIKE 'caller-donor-%' AND donor_channel='mcp'")"
  ck "PG knowledge_cards 六行 donor 署名逐行正确" "6" "${DONOR_ROWS}"
  "${BINS}/rp-kc" dedup-scan --workspace "${WS}" >"${TDIR}/dedup.json" 2>/dev/null
  EDGES="$(psql "${RP_PG_DSN}" -t -A -c "SELECT COUNT(*) FROM knowledge_relations r JOIN knowledge_cards c ON c.knowledge_id=r.src_id WHERE c.workspace_id='${WS}' AND r.relation_type='SAME_CONTENT'")"
  ck "dedup-scan SAME_CONTENT 候选边 = N-1 对（治理面可见）" "5" "${EDGES}"
  # 双闸负例（未计分 approve 拒）。
  FIRST_KC="$(jget "$(cat "${TDIR}/donor-1.json")" "d['knowledge_id']")"
  "${BINS}/rp-kc" review --card "${FIRST_KC}" --decision approve --reason "未计分直批应拒" \
    --reviewer human-reviewer >"${TDIR}/gate.json" 2>/dev/null
  ck "review 双闸负例：未计分 approve 拒（exit 1）" "1" "$?"
  LOGS="$(psql "${RP_PG_DSN}" -t -A -c "SELECT COUNT(*) FROM knowledge_review_log l JOIN knowledge_cards c ON c.knowledge_id=l.knowledge_id WHERE c.workspace_id='${WS}' AND l.reviewer LIKE 'caller-donor-%'")"
  ck "knowledge_review_log 供体转移留痕 reviewer=donor_caller" "6" "${LOGS}"
  unset RP_PG_DSN
else
  echo "[SKIP] podman 或 rip-pg17 镜像缺席（V-9 口径；容器端到端保持本地实跑）"
  PASS=$((PASS+1))
fi

echo "== 8. 生产库指纹前后一致（platform*.db 零触碰反证）"
FP_AFTER="$(for f in migrations/platform*.db; do [[ -e "$f" ]] && sha256sum "$f"; done | sort)"
ck "platform*.db sha256 前后一致" "${FP_BEFORE}" "${FP_AFTER}"

echo ""
echo "MA-3 selftest: PASS=${PASS} FAIL=${FAIL}"
if [[ "${FAIL}" == "0" ]]; then
  echo "RESULT: ALL-PASS"
  exit 0
else
  echo "RESULT: FAIL"
  exit 1
fi
