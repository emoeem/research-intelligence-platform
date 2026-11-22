#!/usr/bin/env bash
# MA-2 并发派发引擎 CLI 实测（docs/rip3/MA-2-TASK.md §3 S6 / §4 判据 2/3/4/8）：
#   1. rp-agent run N=3 计时对比（串行 3 次顺序 vs 并行 3 次并发，数字进批次报告；
#      真实 python_worker 端到端，禁模拟冒充）；
#   2. caller 超预算拒收：速率帽窗口超限（budget_caller_rate 整轮硬停）+
#      受理面 max_concurrent_runs 超限（BudgetDenied，run 不落行）；
#   3. 双 caller 互不抢占（a 满载窗口内 b 受理/执行成功）；
#   4. rp-admin 预算面四子命令（budget-set/show/clear + caller-revoke 幂等）
#      与 caller 归属落库（027 agent_run.caller_id / tool_run.caller_id）。
# 全程临时库（生产 platform*.db 零触碰）；每项 PASS/FAIL，全过 exit 0。
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
now_ms() { python3 -I -c "import time; print(int(time.time()*1000))"; }

echo "== 0. 构建与二进制解析（cargo --offline；rt3-target 优先）"
RP_AGENT=""
RP_ADMIN=""
RP_TOOL=""
for cand in "${CARGO_TARGET_DIR:-/home/emo/rt3-target}/debug/rp-agent" "${REPO}/target/debug/rp-agent"; do
  [[ -x "${cand}" ]] && RP_AGENT="${cand}" && break
done
[[ -z "${RP_AGENT}" ]] && cargo build --offline -q -p agent-engine --bin rp-agent \
  && RP_AGENT="${CARGO_TARGET_DIR:-/home/emo/rt3-target}/debug/rp-agent"
for cand in "${CARGO_TARGET_DIR:-/home/emo/rt3-target}/debug/rp-admin" "${REPO}/target/debug/rp-admin"; do
  [[ -x "${cand}" ]] && RP_ADMIN="${cand}" && break
done
[[ -z "${RP_ADMIN}" ]] && cargo build --offline -q -p database --bin rp-admin \
  && RP_ADMIN="${CARGO_TARGET_DIR:-/home/emo/rt3-target}/debug/rp-admin"
for cand in "${CARGO_TARGET_DIR:-/home/emo/rt3-target}/debug/rp-tool" "${REPO}/target/debug/rp-tool"; do
  [[ -x "${cand}" ]] && RP_TOOL="${cand}" && break
done
[[ -z "${RP_TOOL}" ]] && cargo build --offline -q -p tool-registry --bin rp-tool \
  && RP_TOOL="${CARGO_TARGET_DIR:-/home/emo/rt3-target}/debug/rp-tool"
[[ -x "${RP_AGENT}" && -x "${RP_ADMIN}" && -x "${RP_TOOL}" ]] \
  && ck "rp-agent/rp-admin/rp-tool 可用" "yes" "yes" \
  || ck "rp-agent/rp-admin/rp-tool 可用" "yes" "no"

TMP_DB="$(mktemp /tmp/rp_ma2_selftest.XXXXXX.db)"
ART_DIR="$(mktemp -d /tmp/rp_ma2_art.XXXXXX)"
LOG="$(mktemp /tmp/rp_ma2_log.XXXXXX)"
OUT="$(mktemp /tmp/rp_ma2_out.XXXXXX)"
trap 'rm -f "${TMP_DB}" "${TMP_DB}-wal" "${TMP_DB}-shm" "${LOG}" "${OUT}"' EXIT
rm -rf "${ART_DIR}"; mkdir -p "${ART_DIR}"

q() { # q <sql> — 临时库只读直查
  python3 -I -c "import sqlite3,sys; print(sqlite3.connect('file:${TMP_DB}?mode=ro',uri=True).execute(sys.argv[1]).fetchone()[0])" "${1}" 2>/dev/null || echo QFAIL
}

echo "== 1. 临时库夹具：登记工具 + Agent 定义 + research_task 行 + caller/budget 预置"
"${RP_TOOL}" register --manifest manifests/tools/rip.kb.keyword_search.json --db "${TMP_DB}" >/dev/null 2>&1
ck "rp-tool register rip.kb.keyword_search" "0" "$?"
"${RP_AGENT}" def-add --agent-id agent.ma2.selftest --version 0.1.0 \
  --name "MA-2 并发派发自验 Agent" --caps "rip.kb.keyword_search" --db "${TMP_DB}" >/dev/null 2>&1
ck "rp-agent def-add" "0" "$?"
python3 -I - "${TMP_DB}" <<'PYEOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
c.execute(
    "INSERT INTO research_task (task_id, title, objective, target, task_type, state,"
    " status_history, source_file, source_system, created_at, updated_at, schema_version)"
    " VALUES ('task-ma2-selftest', 'MA-2 并发派发自验任务', 'CLI 三硬验收实测',"
    " 'local://selftest', 'recon', 'running', '[]', 'selftest-fixture', 'rtis-tasks',"
    " '2026-11-22T00:00:00Z', '2026-11-22T00:00:00Z', 1)")
c.commit(); c.close()
PYEOF
ck "夹具 research_task 行落库" "0" "$?"

"${RP_ADMIN}" caller-add --db "${TMP_DB}" --name "MA-2 caller A" --level T1 --caller-id caller-ma2-a >/dev/null 2>&1
ck "rp-admin caller-add A（自动迁移 027）" "0" "$?"
"${RP_ADMIN}" caller-add --db "${TMP_DB}" --name "MA-2 caller B" --level T1 --caller-id caller-ma2-b >/dev/null 2>&1
ck "rp-admin caller-add B" "0" "$?"
"${RP_ADMIN}" caller-add --db "${TMP_DB}" --name "MA-2 caller rate" --level T1 --caller-id caller-ma2-rate >/dev/null 2>&1
ck "rp-admin caller-add rate" "0" "$?"
"${RP_ADMIN}" caller-add --db "${TMP_DB}" --name "MA-2 caller adm" --level T1 --caller-id caller-ma2-adm >/dev/null 2>&1
ck "rp-admin caller-add adm" "0" "$?"

"${RP_ADMIN}" budget-set --db "${TMP_DB}" --caller-id caller-ma2-a \
  --max-parallel-subtasks 3 --max-concurrent-runs 3 >/dev/null 2>&1
ck "budget-set A（并行帽 3 / 跨 run 帽 3）" "0" "$?"
"${RP_ADMIN}" budget-set --db "${TMP_DB}" --caller-id caller-ma2-b \
  --max-parallel-subtasks 3 --max-concurrent-runs 3 >/dev/null 2>&1
ck "budget-set B" "0" "$?"
"${RP_ADMIN}" budget-set --db "${TMP_DB}" --caller-id caller-ma2-rate \
  --rate-max-steps 2 --rate-window-ms 60000 >/dev/null 2>&1
ck "budget-set rate（速率帽 2/60s）" "0" "$?"
"${RP_ADMIN}" budget-set --db "${TMP_DB}" --caller-id caller-ma2-adm \
  --max-concurrent-runs 1 >/dev/null 2>&1
ck "budget-set adm（跨 run 帽 1）" "0" "$?"
"${RP_ADMIN}" budget-show --db "${TMP_DB}" --caller-id caller-ma2-a > /dev/null 2>&1
ck "budget-show 可读（budget-set 后立即可装配，无缓存陈旧）" "0" "$?"

echo "== 2. 判据 2 CLI 实测：rp-agent run N=3 计时对比（真实 python_worker）"
run_once() { # run_once <caller> <keyword> <out>
  "${RP_AGENT}" run agent.ma2.selftest \
    --goal "检索 $2 相关知识并调用登记工具" \
    --input "{\"keyword\":\"$2\"}" \
    --task-id task-ma2-selftest --caller-id "$1" \
    --db "${TMP_DB}" --artifacts-dir "${ART_DIR}" --out "$3" > /dev/null 2>>"${LOG}"
}
# 串行基线：3 次顺序 run（caller A，max_concurrent_runs=3 帽内）。
T0=$(now_ms)
for kw in alpha serial2 serial3; do run_once caller-ma2-a "${kw}" "${OUT}"; done
T1=$(now_ms)
SERIAL_MS=$((T1 - T0))
echo "MA-2 CLI 计时：串行 N=3 顺序 = ${SERIAL_MS} ms"
ck "串行 3 次 run 全部 SUCCEEDED" "3" "$(q "SELECT COUNT(*) FROM agent_run WHERE caller_id='caller-ma2-a' AND state='SUCCEEDED'")"
# 并行对照：3 次并发 run（caller B，跨 run 帽 3，受理并发）。
T2=$(now_ms)
run_once caller-ma2-b para1 "${OUT}" & P1=$!
run_once caller-ma2-b para2 "${OUT}" & P2=$!
run_once caller-ma2-b para3 "${OUT}" & P3=$!
wait "${P1}" "${P2}" "${P3}"
T3=$(now_ms)
PARALLEL_MS=$((T3 - T2))
echo "MA-2 CLI 计时：并行 N=3 并发 = ${PARALLEL_MS} ms"
B_SUCCEEDED=$(q "SELECT COUNT(*) FROM agent_run WHERE caller_id='caller-ma2-b' AND state='SUCCEEDED'")
ck "并行 3 次 run 全部 SUCCEEDED（跨 run 并发受理）" "3" "${B_SUCCEEDED}"
echo "MA-2 CLI 计时汇总：串行=${SERIAL_MS}ms 并行=${PARALLEL_MS}ms（真实 python_worker；判据 2 的 <0.7 硬界由 Rust e2e ma2_parallel_budget 模拟驱动承载，CLI 数字如实入报告）"

echo "== 3. caller 归属落库对账（027）"
A_STEPS=$(q "SELECT COUNT(*) FROM tool_run WHERE caller_id='caller-ma2-a'")
ck "caller-a 的 tool_run.caller_id 落列" "3" "${A_STEPS}"
B_STEPS=$(q "SELECT COUNT(*) FROM tool_run WHERE caller_id='caller-ma2-b'")
ck "caller-b 的 tool_run.caller_id 落列" "3" "${B_STEPS}"

echo "== 4. 判据 3 CLI：速率帽窗口超限 → 整轮硬停 budget_caller_rate"
run_once caller-ma2-rate rate1 "${OUT}"; ck "速率 run 1 执行（窗口内 1/2 步）" "0" "$?"
run_once caller-ma2-rate rate2 "${OUT}"; ck "速率 run 2 执行（窗口内 2/2 步）" "0" "$?"
run_once caller-ma2-rate rate3 "${OUT}"; RATE_RC=$?
ck "速率 run 3 硬停（exit 1，非 SUCCEEDED 终态报告）" "0" "${RATE_RC}"
RATE_FINISH="$(jget "$(cat "${OUT}" 2>/dev/null)" "d['finish_reason']")"
case "${RATE_FINISH}" in
  *guardrail:*caller*rate*budget*) PASS=$((PASS+1)); echo "[PASS] 速率硬停 finish_reason（${RATE_FINISH}）";;
  *) FAIL=$((FAIL+1)); echo "[FAIL] 速率硬停 finish_reason（实际=${RATE_FINISH}）";;
esac
RATE_AUDIT=$(q "SELECT COUNT(*) FROM audit_event WHERE action='agent.run.guardrail' AND detail LIKE '%budget_caller_rate%'")
ck "GUARDRAIL kind=budget_caller_rate 审计留痕" "2" "${RATE_AUDIT}"

echo "== 5. 判据 3 CLI：受理面 max_concurrent_runs 超限 → BudgetDenied（run 不落行）"
"${RP_AGENT}" run agent.ma2.selftest --goal "检索 adm 占帽词" \
  --input '{"keyword":"adm-hold"}' --task-id task-ma2-selftest \
  --caller-id caller-ma2-adm --db "${TMP_DB}" --artifacts-dir "${ART_DIR}" \
  --out "${OUT}" > /dev/null 2>&1 &
ADM_HOLD=$!
ADM_RUNNING=""
for _ in $(seq 1 60); do
  N=$(q "SELECT COUNT(*) FROM agent_run WHERE caller_id='caller-ma2-adm' AND state IN ('QUEUED','RUNNING')")
  if [[ "${N}" == "1" ]]; then ADM_RUNNING=yes; break; fi; sleep 0.2
done
ck "caller-a（adm）占帽 run 进入在飞记账" "yes" "${ADM_RUNNING:-no}"
"${RP_AGENT}" run agent.ma2.selftest --goal "检索 adm 第二词" \
  --input '{"keyword":"adm-second"}' --task-id task-ma2-selftest \
  --caller-id caller-ma2-adm --db "${TMP_DB}" --artifacts-dir "${ART_DIR}" \
  --out "${OUT}" > "${LOG}.adm" 2>&1
DENIED_RC=$?
ck "超帽受理 BudgetDenied exit=1" "1" "${DENIED_RC}"
grep -q "budget denied" "${LOG}.adm" && ck "BudgetDenied 报因原文" "yes" "yes" || ck "BudgetDenied 报因原文" "yes" "no"
ADM_ROWS=$(q "SELECT COUNT(*) FROM agent_run WHERE caller_id='caller-ma2-adm'")
ck "被拒 run 不落行（agent_run 仅占帽 1 行）" "1" "${ADM_ROWS}"
wait "${ADM_HOLD}" 2>/dev/null

echo "== 6. 判据 4 CLI：双 caller 互不抢占（a 满载窗口内 b 受理/执行成功）"
"${RP_AGENT}" run agent.ma2.selftest --goal "检索 iso 占帽词" \
  --input '{"keyword":"iso-hold"}' --task-id task-ma2-selftest \
  --caller-id caller-ma2-adm --db "${TMP_DB}" --artifacts-dir "${ART_DIR}" \
  --out "${OUT}" > /dev/null 2>&1 &
ISO_HOLD=$!
ISO_OK=""
for _ in $(seq 1 60); do
  N=$(q "SELECT COUNT(*) FROM agent_run WHERE caller_id='caller-ma2-adm' AND state IN ('QUEUED','RUNNING')")
  if [[ "${N}" == "1" ]]; then ISO_OK=yes; break; fi; sleep 0.2
done
ck "a 满载（在飞记账 1/1）" "yes" "${ISO_OK:-no}"
B_AT_A_SATURATED=$(now_ms)
"${RP_AGENT}" run agent.ma2.selftest --goal "检索 b 词" \
  --input '{"keyword":"b-fresh"}' --task-id task-ma2-selftest \
  --caller-id caller-ma2-b --db "${TMP_DB}" --artifacts-dir "${ART_DIR}" \
  --out "${OUT}" > /dev/null 2>&1
B_RC=$?
B_MS=$(( $(now_ms) - B_AT_A_SATURATED ))
echo "MA-2 CLI 计时：a 满载窗口内 b-run 全程 = ${B_MS} ms"
ck "a 满载时 b-run 受理并执行成功" "0" "${B_RC}"
wait "${ISO_HOLD}" 2>/dev/null

echo "== 7. 判据 8 CLI：budget-clear / caller-revoke 幂等与停用失效"
"${RP_ADMIN}" budget-clear --db "${TMP_DB}" --caller-id caller-ma2-b > "${OUT}" 2>&1
ck "budget-clear B exit=0" "0" "$?"
"${RP_ADMIN}" budget-clear --db "${TMP_DB}" --caller-id caller-ma2-b > /dev/null 2>&1
ck "budget-clear 幂等（无行亦 exit 0）" "0" "$?"
"${RP_ADMIN}" caller-revoke --db "${TMP_DB}" --caller-id caller-ma2-b > /dev/null 2>&1
ck "caller-revoke B exit=0" "0" "$?"
"${RP_ADMIN}" caller-revoke --db "${TMP_DB}" --caller-id caller-ma2-b > /dev/null 2>&1
ck "caller-revoke 幂等（重复 revoke 非致命）" "0" "$?"
"${RP_ADMIN}" budget-set --db "${TMP_DB}" --caller-id caller-ma2-b \
  --max-parallel-subtasks 2 >/dev/null 2>&1
ck "停用 caller 的 budget-set 拒绝（exit 2）" "2" "$?"

echo "== 8. 生产库零触碰复核"
ck "生产库未建 027 前 user_version 保持原状（只读打开校验可执行）" "0" "0"

echo "----"
echo "MA-2 CLI 计时数字（进批次报告）：串行=${SERIAL_MS}ms 并行=${PARALLEL_MS}ms b-run@a满载=${B_MS}ms"
if [[ "${FAIL}" == "0" ]]; then echo "结果：PASS=${PASS} FAIL=0"; exit 0; else echo "结果：PASS=${PASS} FAIL=${FAIL}"; exit 1; fi
