#!/usr/bin/env bash
# P7-5 trace 统一关联自验（SESSION-HANDOFF §十追加项 1）：真实 rp-agent CLI
# 端到端 run（真实 python_worker：rip_cli_worker + rip_kb_search）+ 六 ID
# 全链落库断言 + rp-agent trace 双向关联查询（trace_id / evidence_id /
# task_id 三方向）+ 真实 rp_server /api 响应 trace 字段断言 + worker
# correlation 透传断言 + 生产库零写核对。输出 PASS/FAIL，全过 exit 0。
#
# 前置：仓库根运行（cargo 门禁按 CWD 发现 vendor 配置）；rp-tool/rp-agent
# 由 cargo --offline 构建；临时库 /tmp，生产 migrations/platform*.db 只读
# （URI mode=ro 核对 user_version 与行数不变，红线 ROADMAP §13）。
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO}"

PORT="${SELFTEST_P75_PORT:-7897}"
TMP_DB="${RIP_P75_DB:-/tmp/rp_p75_trace_dev.db}"
ART_DIR="/tmp/rp_p75_artifacts"
REPORT="/tmp/rp_p75_report.json"
TRACE_JSON="/tmp/rp_p75_chain.json"
TOKEN="selftest-$(python3 -c 'import secrets; print(secrets.token_hex(16))')"
LOG="$(mktemp /tmp/rp_p75_selftest.XXXX.log)"

PASS=0; FAIL=0
ck() { # ck <名> <期望> <实际>
  if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi
}
ck_contains() {
  if [[ "$3" == *"$2"* ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (需含=$2 实际=${3:0:300})"; fi
}
jget() { python3 -c "import sys,json; d=json.loads(sys.argv[1]); print(eval(sys.argv[2]))" "$1" "$2" 2>/dev/null || echo PARSE_FAIL; }
api() { curl -s -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}${1}"; }

cleanup() {
  [[ -n "${SERVER_PID:-}" ]] && kill "${SERVER_PID}" 2>/dev/null
  rm -f "${TMP_DB}" "${TMP_DB}-wal" "${TMP_DB}-shm" "${REPORT}" "${TRACE_JSON}"
}
trap cleanup EXIT

echo "== 0. 构建（cargo --offline）"
cargo build --offline -q -p tool-registry --bin rp-tool || { echo "rp-tool build FAIL"; exit 1; }
cargo build --offline -q -p agent-engine --bin rp-agent || { echo "rp-agent build FAIL"; exit 1; }
RP_TOOL="target/debug/rp-tool"
RP_AGENT="target/debug/rp-agent"

echo "== 1. 生产库零写基线（user_version 快照）"
PLAT_UV="$(python3 -c "
import sqlite3
c = sqlite3.connect('file:migrations/platform.db?mode=ro', uri=True)
print(c.execute('PRAGMA user_version').fetchone()[0])")"
ck "生产 platform.db user_version 基线=1" "1" "${PLAT_UV}"

echo "== 2. 临时开发副本：登记工具 + Agent 定义"
rm -f "${TMP_DB}" "${TMP_DB}-wal" "${TMP_DB}-shm"; rm -rf "${ART_DIR}"; mkdir -p "${ART_DIR}"
"${RP_TOOL}" register --manifest manifests/tools/rip.kb.keyword_search.json --db "${TMP_DB}" >/dev/null 2>&1
ck "rp-tool register rip.kb.keyword_search" "0" "$?"
"${RP_AGENT}" def-add --agent-id agent.trace.selftest --version 0.1.0 \
  --name "P7-5 trace 自验 Agent" --caps "rip.kb.keyword_search" --db "${TMP_DB}" >/dev/null 2>&1
ck "rp-agent def-add" "0" "$?"

# 任务面夹具：research_task 行（生产口径该行来自 M1 ETL；开发副本手工播种，
# agent-engine 只读关联不代写任务行）。五类事件链判据要求任务段非空。
python3 - "${TMP_DB}" <<'PYEOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
c.execute(
    "INSERT INTO research_task (task_id, title, objective, target, task_type, state,"
    " status_history, source_file, source_system, created_at, updated_at, schema_version)"
    " VALUES ('task-p75-selftest', 'P7-5 trace 自验任务', '端到端链路自验', 'local://selftest',"
    " 'recon', 'running', '[]', 'selftest-fixture', 'rtis-tasks',"
    " '2026-11-13T00:00:00Z', '2026-11-13T00:00:00Z', 1)")
c.commit(); c.close()
PYEOF
ck "夹具 research_task 行落库" "0" "$?"

echo "== 3. 端到端 run（真实 python_worker，显式 --trace-id）"
"${RP_AGENT}" run agent.trace.selftest \
  --goal "检索 nmap 端口扫描相关知识并调用登记工具" \
  --input '{"keyword":"nmap"}' \
  --task-id task-p75-selftest --trace-id tr-p75-selftest \
  --db "${TMP_DB}" --artifacts-dir "${ART_DIR}" --out "${REPORT}" > /dev/null 2>"${LOG}"
RUN_RC=$?
ck "rp-agent run exit=0（SUCCEEDED）" "0" "${RUN_RC}"
[[ "${RUN_RC}" != "0" ]] && { echo "--- run stderr"; cat "${LOG}"; }
REPORT_TEXT="$(cat "${REPORT}")"
ck "报告 state=SUCCEEDED" "SUCCEEDED" "$(jget "${REPORT_TEXT}" "d['state']")"
ck "报告 trace_id=tr-p75-selftest" "tr-p75-selftest" "$(jget "${REPORT_TEXT}" "d['trace_id']")"
RUN_ID="$(jget "${REPORT_TEXT}" "d['run_id']")"
TOOL_RUN_ID="$(jget "${REPORT_TEXT}" "d['steps'][0]['tool_run_id']")"
ARTIFACT_ID="$(jget "${REPORT_TEXT}" "d['steps'][0]['artifact_id']")"
EVIDENCE_ID="$(jget "${REPORT_TEXT}" "d['steps'][0]['evidence_id']")"

echo "== 4. 五面同链落库核对（临时副本只读）"
PYCHK="$(python3 - "${TMP_DB}" "${RUN_ID}" "${TOOL_RUN_ID}" "${ARTIFACT_ID}" "${EVIDENCE_ID}" <<'PYEOF'
import sqlite3, sys
db, run_id, tool_run_id, artifact_id, evidence_id = sys.argv[1:6]
c = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
q = lambda sql: c.execute(sql).fetchone()[0]
checks = {
    "agent_run.trace_id": q(f"SELECT trace_id FROM agent_run WHERE run_id='{run_id}'"),
    "tool_run.trace_id": q(f"SELECT trace_id FROM tool_run WHERE run_id='{tool_run_id}'"),
    "artifact.trace_id": q(f"SELECT trace_id FROM artifact WHERE artifact_id='{artifact_id}'"),
    "evidence.trace_id": q(f"SELECT trace_id FROM evidence WHERE evidence_id='{evidence_id}'"),
    "audit_events_by_trace": q("SELECT COUNT(*) FROM audit_event WHERE trace_id='tr-p75-selftest'"),
}
c.close()
bad = {k: v for k, v in checks.items() if (k != "audit_events_by_trace" and v != "tr-p75-selftest")}
if not bad and checks["audit_events_by_trace"] < 3:
    bad["audit_events_by_trace"] = checks["audit_events_by_trace"]
print("OK" if not bad else f"BAD {bad}")
PYEOF
)"
ck "五面 trace_id 同链 + 审计事件 >=3" "OK" "${PYCHK}"

echo "== 5. rp-agent trace：trace_id / evidence_id 双向关联查询（ask 判据）"
"${RP_AGENT}" trace tr-p75-selftest --db "${TMP_DB}" --out "${TRACE_JSON}" > /dev/null 2>&1
ck "trace 子命令 exit=0" "0" "$?"
CHAIN="$(cat "${TRACE_JSON}")"
ck "方向1 trace_id 命中面" "['trace_id']" "$(jget "${CHAIN}" "d['matched_via']")"
ck "方向1 tool_runs=1" "1" "$(jget "${CHAIN}" "d['counts']['tool_runs']")"
ck "方向1 evidence=1" "1" "$(jget "${CHAIN}" "d['counts']['evidence']")"
ck "方向1 artifacts=1" "1" "$(jget "${CHAIN}" "d['counts']['artifacts']")"
ck "方向1 agent_runs=1" "1" "$(jget "${CHAIN}" "d['counts']['agent_runs']")"
MANIFESTS="$(jget "${CHAIN}" "d['counts']['manifests']")"
if [[ "${MANIFESTS}" -ge 1 ]]; then PASS=$((PASS+1)); echo "[PASS] 方向1 manifests>=1（意图/上下文面经 task_id 关联）"; else FAIL=$((FAIL+1)); echo "[FAIL] 方向1 manifests=${MANIFESTS}"; fi
ck "方向1 tasks=1（任务面，research_task 经 agent_run.task_id 关联）" "1" "$(jget "${CHAIN}" "d['counts']['tasks']")"
AUDITS="$(jget "${CHAIN}" "d['counts']['audit_events']")"
if [[ "${AUDITS}" -ge 3 ]]; then PASS=$((PASS+1)); echo "[PASS] 方向1 audit_events>=3"; else FAIL=$((FAIL+1)); echo "[FAIL] audit_events=${AUDITS}"; fi

"${RP_AGENT}" trace "${EVIDENCE_ID}" --db "${TMP_DB}" > /tmp/rp_p75_chain_ev.json 2>/dev/null
EV_CHAIN="$(cat /tmp/rp_p75_chain_ev.json)"
ck "方向2 evidence_id 命中面" "['evidence_id']" "$(jget "${EV_CHAIN}" "d['matched_via']")"
ck "方向2 同链 trace_ids" "['tr-p75-selftest']" "$(jget "${EV_CHAIN}" "d['trace_ids']")"
ck "方向2 同宽（counts 一致）" "$(jget "${CHAIN}" "d['counts']")" "$(jget "${EV_CHAIN}" "d['counts']")"

"${RP_AGENT}" trace task-p75-selftest --db "${TMP_DB}" > /tmp/rp_p75_chain_task.json 2>/dev/null
TASK_CHAIN="$(cat /tmp/rp_p75_chain_task.json)"
ck "附方向 task_id 命中面" "['task_id']" "$(jget "${TASK_CHAIN}" "d['matched_via']")"

echo "== 6. 真实 rp_server：/api 响应 trace 字段断言"
RP_AUX_DB="${TMP_DB}" RP_AUTH_TOKEN="${TOKEN}" RP_PORT="${PORT}" python3 -I apps/server/rp_server.py > "${LOG}" 2>&1 &
SERVER_PID=$!
for _ in $(seq 1 40); do
  curl -s -o /dev/null -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}/api/stats" && break
  sleep 0.3
done

RUNS_LIST="$(api "/api/runs?limit=5")"
ck_contains "/api/runs 列表响应含 trace 字段" '"trace_id": "tr-p75-selftest"' "${RUNS_LIST}"
RUN_DETAIL="$(api "/api/runs?run_id=${TOOL_RUN_ID}")"
ck "/api/runs 详情 correlation.trace_id" "tr-p75-selftest" "$(jget "${RUN_DETAIL}" "d['correlation']['trace_id']")"
ck "/api/runs 详情 correlation.tool_run_id" "${TOOL_RUN_ID}" "$(jget "${RUN_DETAIL}" "d['correlation']['tool_run_id']")"
ck "/api/runs 详情 correlation.artifact_ids" "['${ARTIFACT_ID}']" "$(jget "${RUN_DETAIL}" "d['correlation']['artifact_ids']")"
ck "/api/runs 详情 correlation.evidence_ids" "['${EVIDENCE_ID}']" "$(jget "${RUN_DETAIL}" "d['correlation']['evidence_ids']")"

API_TRACE="$(api "/api/traces?id=tr-p75-selftest")"
ck "/api/traces available" "True" "$(jget "${API_TRACE}" "d['available']")"
ck "/api/traces matched_via" "trace_id" "$(jget "${API_TRACE}" "d['matched_via']")"
ck "/api/traces tool_runs=1" "1" "$(jget "${API_TRACE}" "d['counts']['tool_runs']")"
ck "/api/traces evidence=1" "1" "$(jget "${API_TRACE}" "d['counts']['evidence']")"
API_TRACE_EV="$(api "/api/traces?id=${EVIDENCE_ID}")"
ck "/api/traces evidence_id 方向同链" "['tr-p75-selftest']" "$(jget "${API_TRACE_EV}" "d['trace_ids']")"

echo "== 7. worker correlation 透传（真实 python_worker 协议回路）"
WORKER_ECHO="$(python3 - <<'PYEOF'
import json, subprocess, sys, tempfile, os
with tempfile.TemporaryDirectory() as td:
    env = dict(os.environ, WORKER_CACHE_DIR=td)
    p = subprocess.Popen(
        ["python3", "-I", "scripts/rip_cli_worker.py", "scripts/rip_kb_search.py"],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        text=True, env=env, cwd=os.getcwd())
    def send(obj):
        p.stdin.write(json.dumps(obj) + "\n")
        p.stdin.flush()
        line = p.stdout.readline()
        return json.loads(line) if line else {}
    send({"jsonrpc": "2.0", "id": 0, "method": "initialize",
          "params": {"protocol_version": "1"}})
    send({"jsonrpc": "2.0", "id": 1, "method": "get_capabilities", "params": {}})
    corr = {"trace_id": "tr-p75-selftest", "task_id": "task-p75-selftest",
            "agent_run_id": None, "tool_run_id": "rn-p75-selftest",
            "artifact_id": None, "evidence_id": None}
    resp = send({"jsonrpc": "2.0", "id": 2, "method": "execute",
                 "params": {"task": {"op": "cli", "argv": ["nmap"]},
                            "budget": {"timeout_ms": 30000, "max_output_bytes": 1048576},
                            "idempotency_key": "p75-selftest-1",
                            "correlation": corr}})
    p.stdin.close(); p.wait(timeout=10)
    result = resp.get("result", {})
    echo = result.get("correlation")
    print("OK" if echo == corr else f"BAD echo={echo}")
PYEOF
)"
ck "worker 响应原样回传 params.correlation（六 ID）" "OK" "${WORKER_ECHO}"

echo "== 8. 生产库零写核对（收尾）"
PLAT_UV2="$(python3 -c "
import sqlite3
c = sqlite3.connect('file:migrations/platform.db?mode=ro', uri=True)
print(c.execute('PRAGMA user_version').fetchone()[0])")"
ck "生产 platform.db user_version 收尾不变" "${PLAT_UV}" "${PLAT_UV2}"

echo ""
echo "== P7-5 selftest 结果：PASS=${PASS} FAIL=${FAIL}"
[[ "${FAIL}" == "0" ]] && exit 0
echo "失败项见上方 [FAIL]；server 日志 ${LOG}"
exit 1
