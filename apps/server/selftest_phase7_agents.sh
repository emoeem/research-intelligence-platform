#!/usr/bin/env bash
# P7-7 Agent Studio 读面自验（SESSION-HANDOFF §11.4 接入点 4）：真实 rp-agent
# CLI 端到端 run（真实 python_worker）+ rp-agent run-steps 轨迹读取面（014
# agent_run_step）+ 013 三记忆表夹具播种 + 真实 rp_server /api/agents 与
# /api/agent-runs?id= 响应断言（steps、correlation 六 ID、三记忆视图、控制行）
# + 写面静态守卫（handle_agents/handle_agent_runs 零 SQL 写、do_POST 白名单
# 不动：POST /api/agents 仍 405）+ 生产库零写核对（URI mode=ro 核对
# user_version 与逐表行数前后一致）。输出 PASS/FAIL，全过 exit 0。
#
# 前置：仓库根运行（cargo 门禁按 CWD 发现 vendor 配置）；rp-tool/rp-agent
# 由 cargo --offline 构建；临时库 /tmp，生产 migrations/platform*.db 只读
# （红线 ROADMAP §13）。013 三记忆行是夹具播种（生产口径该写入面来自
# agent_loop with_memory 可选接线，CLI run 未挂 Memory，读面自验先备行）。
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO}"

PORT="${SELFTEST_P77_PORT:-7898}"
TMP_DB="${RIP_P77_DB:-/tmp/rp_p77_agents_dev.db}"
ART_DIR="/tmp/rp_p77_artifacts"
REPORT="/tmp/rp_p77_report.json"
STEPS_JSON="/tmp/rp_p77_steps.json"
TOKEN="selftest-$(python3 -I -c 'import secrets; print(secrets.token_hex(16))')"
LOG="$(mktemp /tmp/rp_p77_selftest.XXXX.log)"

PASS=0; FAIL=0
ck() { # ck <名> <期望> <实际>
  if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi
}
ck_contains() {
  if [[ "$3" == *"$2"* ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (需含=$2 实际=${3:0:300})"; fi
}
jget() { python3 -I -c "import sys,json; d=json.loads(sys.argv[1]); print(eval(sys.argv[2]))" "$1" "$2" 2>/dev/null || echo PARSE_FAIL; }
api() { curl -s -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}${1}"; }
api_code() { curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}${1}"; }

# 生产库指纹：user_version + 逐表行数（URI mode=ro，只读核对零写红线）。
db_fingerprint() {
  python3 -I - "$1" <<'PYEOF'
import sqlite3, sys, json
c = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
uv = c.execute("PRAGMA user_version").fetchone()[0]
tables = sorted(r[0] for r in c.execute(
    "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'"))
counts = {t: c.execute(f"SELECT COUNT(*) FROM \"{t}\"").fetchone()[0] for t in tables}
print(json.dumps({"user_version": uv, "row_counts": counts}, sort_keys=True))
c.close()
PYEOF
}

cleanup() {
  [[ -n "${SERVER_PID:-}" ]] && kill "${SERVER_PID}" 2>/dev/null
  rm -f "${TMP_DB}" "${TMP_DB}-wal" "${TMP_DB}-shm" "${REPORT}" "${STEPS_JSON}"
}
trap cleanup EXIT

echo "== 0. 构建（cargo --offline）"
cargo build --offline -q -p tool-registry --bin rp-tool || { echo "rp-tool build FAIL"; exit 1; }
cargo build --offline -q -p agent-engine --bin rp-agent || { echo "rp-agent build FAIL"; exit 1; }
RP_TOOL="target/debug/rp-tool"
RP_AGENT="target/debug/rp-agent"

echo "== 1. 生产库零写基线（user_version + 逐表行数指纹）"
PLAT_FP="$(db_fingerprint migrations/platform.db)"
ck_contains "生产 platform.db user_version=1" '"user_version": 1' "${PLAT_FP}"
if [[ -f migrations/platform-dryrun.db ]]; then
  DRY_FP="$(db_fingerprint migrations/platform-dryrun.db)"
  PASS=$((PASS+1)); echo "[PASS] platform-dryrun.db 指纹已取（收尾复核）"
else
  DRY_FP=""
  PASS=$((PASS+1)); echo "[PASS] platform-dryrun.db 不存在（跳过，收尾一致）"
fi

echo "== 2. 临时开发副本：登记工具 + Agent 定义"
rm -f "${TMP_DB}" "${TMP_DB}-wal" "${TMP_DB}-shm"; rm -rf "${ART_DIR}"; mkdir -p "${ART_DIR}"
"${RP_TOOL}" register --manifest manifests/tools/rip.kb.keyword_search.json --db "${TMP_DB}" >/dev/null 2>&1
ck "rp-tool register rip.kb.keyword_search" "0" "$?"
"${RP_AGENT}" def-add --agent-id agent.trace.selftest --version 0.1.0 \
  --name "P7-7 Agent Studio 自验 Agent" --caps "rip.kb.keyword_search" --db "${TMP_DB}" >/dev/null 2>&1
ck "rp-agent def-add" "0" "$?"

# 任务面夹具：research_task 行（生产口径该行来自 M1 ETL；开发副本手工播种，
# agent-engine 只读关联不代写任务行）。
python3 -I - "${TMP_DB}" <<'PYEOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
c.execute(
    "INSERT INTO research_task (task_id, title, objective, target, task_type, state,"
    " status_history, source_file, source_system, created_at, updated_at, schema_version)"
    " VALUES ('task-p77-selftest', 'P7-7 Agent Studio 自验任务', 'AgentRun 读面端到端自验',"
    " 'local://selftest', 'recon', 'running', '[]', 'selftest-fixture', 'rtis-tasks',"
    " '2026-11-13T00:00:00Z', '2026-11-13T00:00:00Z', 1)")
c.commit(); c.close()
PYEOF
ck "夹具 research_task 行落库" "0" "$?"

echo "== 3. 端到端 run（真实 python_worker，显式 --trace-id/--task-id）"
"${RP_AGENT}" run agent.trace.selftest \
  --goal "检索 nmap 端口扫描相关知识并调用登记工具" \
  --input '{"keyword":"nmap"}' \
  --task-id task-p77-selftest --trace-id tr-p77-selftest \
  --db "${TMP_DB}" --artifacts-dir "${ART_DIR}" --out "${REPORT}" > /dev/null 2>"${LOG}"
RUN_RC=$?
ck "rp-agent run exit=0（SUCCEEDED）" "0" "${RUN_RC}"
[[ "${RUN_RC}" != "0" ]] && { echo "--- run stderr"; cat "${LOG}"; }
REPORT_TEXT="$(cat "${REPORT}")"
ck "报告 state=SUCCEEDED" "SUCCEEDED" "$(jget "${REPORT_TEXT}" "d['state']")"
RUN_ID="$(jget "${REPORT_TEXT}" "d['run_id']")"
TOOL_RUN_ID="$(jget "${REPORT_TEXT}" "d['steps'][0]['tool_run_id']")"
ARTIFACT_ID="$(jget "${REPORT_TEXT}" "d['steps'][0]['artifact_id']")"
EVIDENCE_ID="$(jget "${REPORT_TEXT}" "d['steps'][0]['evidence_id']")"

echo "== 4. rp-agent run-steps：014 轨迹读取面（复用 RunStateStore::steps）"
"${RP_AGENT}" run-steps "${RUN_ID}" --db "${TMP_DB}" --out "${STEPS_JSON}" > /dev/null 2>&1
ck "run-steps exit=0" "0" "$?"
STEPS_TEXT="$(cat "${STEPS_JSON}")"
ck "run-steps steps_count>=1" "True" "$(jget "${STEPS_TEXT}" "d['steps_count'] >= 1")"
ck "run-steps steps[0].seq=0" "0" "$(jget "${STEPS_TEXT}" "d['steps'][0]['seq']")"
ck "run-steps steps[0].tool_run_id=报告步骤一" "${TOOL_RUN_ID}" "$(jget "${STEPS_TEXT}" "d['steps'][0]['tool_run_id']")"
ck "run-steps steps[0].tool_id" "rip.kb.keyword_search" "$(jget "${STEPS_TEXT}" "d['steps'][0]['tool_id']")"
ck "run-steps steps[0].input 关键词在场" "nmap" "$(jget "${STEPS_TEXT}" "d['steps'][0]['input']['keyword']")"
"${RP_AGENT}" run-steps arn-no-such-run --db "${TMP_DB}" > /dev/null 2>&1
ck "run-steps 未知 run exit=1" "1" "$?"

echo "== 5. 013 三记忆表夹具播种（读面自验备行；引用真实 run/evidence/artifact）"
python3 -I - "${TMP_DB}" "${RUN_ID}" "${ARTIFACT_ID}" "${EVIDENCE_ID}" <<'PYEOF'
import sqlite3, sys
db, run_id, artifact_id, evidence_id = sys.argv[1:5]
c = sqlite3.connect(db)
row = c.execute(
    "SELECT content_hash, storage_ref FROM evidence WHERE evidence_id=?", (evidence_id,)
).fetchone()
assert row, f"evidence {evidence_id} 不存在（端到端 run 未登记证据）"
content_hash, storage_ref = row
c.execute(
    "INSERT INTO agent_task_memory (memory_id, run_id, task_id, round, content_json,"
    " summary, lifecycle, created_at, updated_at, schema_version)"
    " VALUES ('mem-t-p77-selftest', ?, 'task-p77-selftest', 0, '{}', '', 'ACTIVE',"
    " '2026-11-13T02:00:00Z', '2026-11-13T02:00:00Z', 1)", (run_id,))
c.execute(
    "INSERT INTO agent_evidence_memory (memory_id, run_id, evidence_id, artifact_id,"
    " content_hash, storage_ref, lifecycle, created_at, updated_at, schema_version)"
    " VALUES ('mem-e-p77-selftest', ?, ?, ?, ?, ?, 'ACTIVE',"
    " '2026-11-13T02:00:00Z', '2026-11-13T02:00:00Z', 1)",
    (run_id, evidence_id, artifact_id, content_hash, storage_ref))
c.execute(
    "INSERT INTO agent_research_memory (memory_id, run_id, task_id, agent_id, title,"
    " summary, content_json, refs_json, lifecycle, created_at, updated_at, schema_version)"
    " VALUES ('mem-r-p77-selftest', ?, 'task-p77-selftest', 'agent.trace.selftest',"
    " 'P7-7 自验研究成果', '端到端链路读面回链摘要', '{}', ?, 'ACTIVE',"
    " '2026-11-13T02:00:00Z', '2026-11-13T02:00:00Z', 1)",
    (run_id,
     f'[{{"ref_type":"artifact","ref_id":"{artifact_id}"}},{{"ref_type":"evidence","ref_id":"{evidence_id}"}}]'))
c.commit(); c.close()
print("OK")
PYEOF
ck "013 三记忆夹具行落库" "OK" "$(python3 -I - "${TMP_DB}" <<'PYEOF'
import sqlite3, sys
c = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
print("OK" if c.execute("SELECT COUNT(*) FROM agent_task_memory").fetchone()[0] >= 1
      and c.execute("SELECT COUNT(*) FROM agent_evidence_memory").fetchone()[0] >= 1
      and c.execute("SELECT COUNT(*) FROM agent_research_memory").fetchone()[0] >= 1
      else "BAD")
c.close()
PYEOF
)"

echo "== 6. 写面静态守卫（handle_agents/handle_agent_runs 零 SQL 写）"
STATIC_GUARD="$(python3 -I - <<'PYEOF'
import ast
src = open("apps/server/rp_server.py", encoding="utf-8").read()
tree = ast.parse(src)
found = [n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name in
         ("handle_agents", "handle_agent_runs")]
assert len(found) == 2, f"handlers missing: {[n.name for n in found]}"
bad = []
for fn in found:
    text = ast.get_source_segment(src, fn).lower()
    for kw in ("insert ", "update agent", "delete ", "create ", "drop ", "attach ", "replace into"):
        if kw in text:
            bad.append((fn.name, kw))
print("OK" if not bad else f"BAD {bad}")
PYEOF
)"
ck "两个 handler 纯 SELECT（ast 静态断言）" "OK" "${STATIC_GUARD}"

echo "== 7. 真实 rp_server：/api/agents 与 /api/agent-runs 响应断言"
RP_AUX_DB="${TMP_DB}" RP_AUTH_TOKEN="${TOKEN}" RP_PORT="${PORT}" python3 -I apps/server/rp_server.py > "${LOG}" 2>&1 &
SERVER_PID=$!
for _ in $(seq 1 40); do
  curl -s -o /dev/null -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}/api/stats" && break
  sleep 0.3
done

AGENTS_LIST="$(api "/api/agents?limit=5")"
ck "/api/agents available" "True" "$(jget "${AGENTS_LIST}" "d['available']")"
ck "/api/agents 定义行 agent_id" "agent.trace.selftest" "$(jget "${AGENTS_LIST}" "d['items'][0]['agent_id']")"
ck "/api/agents capability_refs 工具在场" "rip.kb.keyword_search" "$(jget "${AGENTS_LIST}" "d['items'][0]['capability_refs'][0]['tool_id']")"
ck "/api/agents runs_total>=1" "True" "$(jget "${AGENTS_LIST}" "d['runs_total'] >= 1")"

AGENTS_DETAIL="$(api "/api/agents?id=agent.trace.selftest")"
ck "/api/agents?id= definitions>=1" "True" "$(jget "${AGENTS_DETAIL}" "len(d['definitions']) >= 1")"
ck "/api/agents?id= tool_bindings registered" "True" "$(jget "${AGENTS_DETAIL}" "d['tool_bindings'][0]['registered']")"
ck "/api/agents?id= runs[0].run_id=真实 run" "${RUN_ID}" "$(jget "${AGENTS_DETAIL}" "d['runs'][0]['run_id']")"
ck "/api/agents?id= runs[0].trace_id" "tr-p77-selftest" "$(jget "${AGENTS_DETAIL}" "d['runs'][0]['trace_id']")"
ck "/api/agents?id= runs[0].steps_count>=1" "True" "$(jget "${AGENTS_DETAIL}" "d['runs'][0]['steps_count'] >= 1")"

RUNS_LIST="$(api "/api/agent-runs?limit=5")"
ck "/api/agent-runs 列表含真实 run" "${RUN_ID}" "$(jget "${RUNS_LIST}" "d['items'][0]['run_id']")"
ck "/api/agent-runs 列表 trace_id 字段" "tr-p77-selftest" "$(jget "${RUNS_LIST}" "d['items'][0]['trace_id']")"
ck "/api/agent-runs 列表 state=SUCCEEDED" "SUCCEEDED" "$(jget "${RUNS_LIST}" "d['items'][0]['state']")"

RUN_DETAIL="$(api "/api/agent-runs?id=${RUN_ID}")"
ck "详情 run.state=SUCCEEDED" "SUCCEEDED" "$(jget "${RUN_DETAIL}" "d['run']['state']")"
ck "详情 steps>=1" "True" "$(jget "${RUN_DETAIL}" "d['counts']['steps'] >= 1")"
ck "详情 steps[0].seq=0" "0" "$(jget "${RUN_DETAIL}" "d['steps'][0]['seq']")"
ck "详情 steps[0].tool_run_id" "${TOOL_RUN_ID}" "$(jget "${RUN_DETAIL}" "d['steps'][0]['tool_run_id']")"
ck "详情 correlation.trace_id" "tr-p77-selftest" "$(jget "${RUN_DETAIL}" "d['correlation']['trace_id']")"
ck "详情 correlation.task_id" "task-p77-selftest" "$(jget "${RUN_DETAIL}" "d['correlation']['task_id']")"
ck "详情 correlation.agent_run_id" "${RUN_ID}" "$(jget "${RUN_DETAIL}" "d['correlation']['agent_run_id']")"
ck "详情 correlation.tool_run_ids" "['${TOOL_RUN_ID}']" "$(jget "${RUN_DETAIL}" "d['correlation']['tool_run_ids']")"
ck "详情 correlation.artifact_ids" "['${ARTIFACT_ID}']" "$(jget "${RUN_DETAIL}" "d['correlation']['artifact_ids']")"
ck "详情 correlation.evidence_ids" "['${EVIDENCE_ID}']" "$(jget "${RUN_DETAIL}" "d['correlation']['evidence_ids']")"
ck "详情 memories.task>=1（013 夹具）" "True" "$(jget "${RUN_DETAIL}" "d['counts']['task_memory'] >= 1")"
ck "详情 memories.evidence>=1（013 夹具）" "True" "$(jget "${RUN_DETAIL}" "d['counts']['evidence_memory'] >= 1")"
ck "详情 memories.research>=1（013 夹具）" "True" "$(jget "${RUN_DETAIL}" "d['counts']['research_memory'] >= 1")"
ck "详情 memories.research[0].title" "P7-7 自验研究成果" "$(jget "${RUN_DETAIL}" "d['memories']['research'][0]['title']")"
ck "详情 control.goal_text 在场" "True" "$(jget "${RUN_DETAIL}" "bool((d['control'] or {}).get('goal_text'))")"

ck "未知 run 404 no_agent_run" "404" "$(api_code "/api/agent-runs?id=arn-no-such-run")"
ck_contains "404 错误码 no_agent_run" '"code": "no_agent_run"' "$(api "/api/agent-runs?id=arn-no-such-run")"
ck "未知 agent 404 no_agent" "404" "$(api_code "/api/agents?id=agent-no-such")"

echo "== 8. do_POST 白名单不动（/api/agents 与 /api/agent-runs 仍只读 405）"
ck "POST /api/agents 405" "405" "$(curl -s -o /dev/null -w '%{http_code}' -X POST -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}/api/agents")"
ck "POST /api/agent-runs 405" "405" "$(curl -s -o /dev/null -w '%{http_code}' -X POST -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}/api/agent-runs")"

echo "== 9. 生产库零写核对（收尾指纹一致）"
ck "生产 platform.db 指纹收尾一致" "${PLAT_FP}" "$(db_fingerprint migrations/platform.db)"
if [[ -n "${DRY_FP}" ]]; then
  ck "生产 platform-dryrun.db 指纹收尾一致" "${DRY_FP}" "$(db_fingerprint migrations/platform-dryrun.db)"
fi

echo ""
echo "== P7-7 selftest 结果：PASS=${PASS} FAIL=${FAIL}"
[[ "${FAIL}" == "0" ]] && exit 0
echo "失败项见上方 [FAIL]；server 日志 ${LOG}"
exit 1
