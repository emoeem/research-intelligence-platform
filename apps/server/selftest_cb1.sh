#!/usr/bin/env bash
# CB-1 上下文折叠 CLI 实测（docs/rip3/CB-1-TASK.md §5 收口 / §4 判据 3 + §3 S7）：
#   1. 默认关 CLI 零足迹：不带 --transcript-fold 的 run → 报告 fold_events 空 +
#      run-folds 零折叠行（红线 4）；
#   2. 折叠开 CLI 实测（真实 python_worker 端到端，禁模拟冒充）：
#      --transcript-fold + 收紧参数面 run → 报告 fold_events ≥1 + run-folds 回查
#      折叠行 JSON（memory_id/rounds/digest/lifecycle）+ agent_run_step 权威面
#      全量行数 + 折叠前后 est 数字（进批次报告）；
#   3. rp-context cache-attrib 静态归因报告实测（assemble 落缓存后 entries/expired/
#      policy_mismatch 计数，判据 7 CLI 面）。
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

echo "== 0. 构建与二进制解析（cargo --offline；rt3-target 优先）"
RP_AGENT=""
RP_TOOL=""
RP_CONTEXT=""
for cand in "${CARGO_TARGET_DIR:-/home/emo/rt3-target}/debug/rp-agent" "${REPO}/target/debug/rp-agent"; do
  [[ -x "${cand}" ]] && RP_AGENT="${cand}" && break
done
[[ -z "${RP_AGENT}" ]] && cargo build --offline -q -p agent-engine --bin rp-agent \
  && RP_AGENT="${CARGO_TARGET_DIR:-/home/emo/rt3-target}/debug/rp-agent"
for cand in "${CARGO_TARGET_DIR:-/home/emo/rt3-target}/debug/rp-tool" "${REPO}/target/debug/rp-tool"; do
  [[ -x "${cand}" ]] && RP_TOOL="${cand}" && break
done
[[ -z "${RP_TOOL}" ]] && cargo build --offline -q -p tool-registry --bin rp-tool \
  && RP_TOOL="${CARGO_TARGET_DIR:-/home/emo/rt3-target}/debug/rp-tool"
for cand in "${CARGO_TARGET_DIR:-/home/emo/rt3-target}/debug/rp-context" "${REPO}/target/debug/rp-context"; do
  [[ -x "${cand}" ]] && RP_CONTEXT="${cand}" && break
done
[[ -z "${RP_CONTEXT}" ]] && cargo build --offline -q -p context-broker --bin rp-context \
  && RP_CONTEXT="${CARGO_TARGET_DIR:-/home/emo/rt3-target}/debug/rp-context"
[[ -x "${RP_AGENT}" && -x "${RP_TOOL}" && -x "${RP_CONTEXT}" ]] \
  && ck "rp-agent/rp-tool/rp-context 可用" "yes" "yes" \
  || ck "rp-agent/rp-tool/rp-context 可用" "yes" "no"

TMP_DB="$(mktemp /tmp/rp_cb1_selftest.XXXXXX.db)"
TMP_DATA="$(mktemp -d /tmp/rp_cb1_data.XXXXXX)"
ART_DIR="$(mktemp -d /tmp/rp_cb1_art.XXXXXX)"
OUT="$(mktemp /tmp/rp_cb1_out.XXXXXX)"
REPORT="$(mktemp /tmp/rp_cb1_report.XXXXXX)"
trap 'rm -f "${TMP_DB}" "${TMP_DB}-wal" "${TMP_DB}-shm" "${OUT}" "${REPORT}"; rm -rf "${TMP_DATA}" "${ART_DIR}"' EXIT
mkdir -p "${ART_DIR}" "${TMP_DATA}"

q() { # q <sql> — 临时库只读直查
  python3 -I -c "import sqlite3,sys; print(sqlite3.connect('file:${TMP_DB}?mode=ro',uri=True).execute(sys.argv[1]).fetchone()[0])" "${1}" 2>/dev/null || echo QFAIL
}

echo "== 1. 临时库夹具：登记工具 + Agent 定义 + research_task 行"
"${RP_TOOL}" register --manifest manifests/tools/rip.kb.keyword_search.json --db "${TMP_DB}" >/dev/null 2>&1
ck "rp-tool register rip.kb.keyword_search" "0" "$?"
python3 -I - "${TMP_DB}" <<'PYEOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
c.execute("UPDATE tool SET lifecycle = 'ENABLED' WHERE tool_id = 'rip.kb.keyword_search'")
c.commit(); c.close()
PYEOF
ck "夹具 tool 置 ENABLED（Broker 目录面可见口径）" "0" "$?"
"${RP_AGENT}" def-add --agent-id agent.cb1.selftest --version 0.1.0 \
  --name "CB-1 折叠自验 Agent" --caps "rip.kb.keyword_search" --db "${TMP_DB}" >/dev/null 2>&1
ck "rp-agent def-add" "0" "$?"
python3 -I - "${TMP_DB}" <<'PYEOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
c.execute(
    "INSERT INTO research_task (task_id, title, objective, target, task_type, state,"
    " status_history, source_file, source_system, created_at, updated_at, schema_version)"
    " VALUES ('task-cb1-selftest', 'CB-1 折叠自验任务', 'CLI 折叠实测',"
    " 'local://selftest', 'recon', 'running', '[]', 'selftest-fixture', 'rtis-tasks',"
    " '2026-11-22T00:00:00Z', '2026-11-22T00:00:00Z', 1)")
c.commit(); c.close()
PYEOF
ck "夹具 research_task 行落库" "0" "$?"

echo "== 2. 判据 5 CLI 守卫：默认关（无 --transcript-fold）零折叠足迹"
"${RP_AGENT}" run agent.cb1.selftest \
  --goal "检索 off-fold 相关知识并调用登记工具" \
  --input '{"keyword":"off-fold"}' \
  --task-id task-cb1-selftest \
  --db "${TMP_DB}" --artifacts-dir "${ART_DIR}" --out "${REPORT}" > /dev/null 2>&1
ck "默认关 run SUCCEEDED（exit 0）" "0" "$?"
ck "默认关报告 fold_events 空" "0" "$(jget "$(cat "${REPORT}")" "len(d['fold_events'])")"
RUN_OFF="$(jget "$(cat "${REPORT}")" "d['run_id']")"
"${RP_AGENT}" run-folds "${RUN_OFF}" --db "${TMP_DB}" --out "${OUT}" > /dev/null 2>&1
ck "run-folds 可执行（默认关 run）" "0" "$?"
ck "默认关 run-folds 零折叠行" "0" "$(jget "$(cat "${OUT}")" "d['folds_count']")"
ck "默认关 agent_run_step 权威面全量" "1" "$(q "SELECT COUNT(*) FROM agent_run_step WHERE run_id='${RUN_OFF}'")"

echo "== 3. 判据 3 CLI 实测：--transcript-fold 折叠开（真实 python_worker）"
"${RP_AGENT}" run agent.cb1.selftest \
  --goal "检索 on-fold 相关知识并调用登记工具" \
  --input '{"keyword":"on-fold"}' \
  --task-id task-cb1-selftest \
  --transcript-fold --fold-growth-tokens 1 --fold-min-interval-rounds 0 --fold-hot-window-rounds 0 \
  --db "${TMP_DB}" --artifacts-dir "${ART_DIR}" --out "${REPORT}" > /dev/null 2>&1
ck "折叠开 run SUCCEEDED（exit 0）" "0" "$?"
ck "报告 fold_events ≥1" "yes" "$(jget "$(cat "${REPORT}")" "'yes' if len(d['fold_events'])>=1 else 'no'")"
RUN_ON="$(jget "$(cat "${REPORT}")" "d['run_id']")"
STEPS_ON="$(jget "$(cat "${REPORT}")" "len(d['steps'])")"
ck "报告 steps 全量不缩水" "1" "${STEPS_ON}"
"${RP_AGENT}" run-folds "${RUN_ON}" --db "${TMP_DB}" --out "${OUT}" > /dev/null 2>&1
ck "run-folds 回查可执行" "0" "$?"
FOLDS_COUNT="$(jget "$(cat "${OUT}")" "d['folds_count']")"
ck "run-folds 折叠行 ≥1" "yes" "$(python3 -I -c "print('yes' if ${FOLDS_COUNT}>=1 else 'no')")"
ck "折叠行 lifecycle ACTIVE" "ACTIVE" "$(jget "$(cat "${OUT}")" "d['folds'][0]['lifecycle']")"
ck "折叠行 summary transcript-fold 词形" "yes" "$(jget "$(cat "${OUT}")" "'yes' if d['folds'][0]['summary'].startswith('transcript-fold:') else 'no'")"
ck "折叠行 digest 64 hex" "64" "$(jget "$(cat "${OUT}")" "len(d['folds'][0]['digest'])")"
ck "折叠行 rounds 落载荷" "[0, 0]" "$(jget "$(cat "${OUT}")" "str(d['folds'][0]['rounds'])")"
EST_BEFORE="$(jget "$(cat "${OUT}")" "d['folds'][0]['est_before']")"
EST_AFTER="$(jget "$(cat "${OUT}")" "d['folds'][0]['est_after']")"
ck "折叠 est 收缩（before > after）" "yes" "$(python3 -I -c "print('yes' if ${EST_BEFORE}>${EST_AFTER}>0 else 'no')")"
ck "agent_run_step 权威面全量（折叠零重写）" "1" "$(q "SELECT COUNT(*) FROM agent_run_step WHERE run_id='${RUN_ON}'")"
ck "agent_task_memory 折叠行落库（013 记忆面）" "1" "$(q "SELECT COUNT(*) FROM agent_task_memory WHERE run_id='${RUN_ON}' AND summary LIKE 'transcript-fold:%'")"
ck "agent_run_step 零折叠合成行（权威面纯净）" "0" "$(q "SELECT COUNT(*) FROM agent_run_step WHERE run_id='${RUN_ON}' AND tool_id='transcript:fold'")"
echo "CB-1 CLI 折叠数字（进批次报告）：run=${RUN_ON} est_before=${EST_BEFORE} est_after=${EST_AFTER} steps=${STEPS_ON} run-steps 字节=$(q "SELECT length(input_json)+length(summary)+length(coalesce(error,'')) FROM agent_run_step WHERE run_id='${RUN_ON}'" 2>/dev/null || echo NA)"

echo "== 4. 判据 7 CLI：rp-context cache-attrib 静态归因报告"
"${RP_CONTEXT}" assemble --task-id task-cb1-selftest --db "${TMP_DB}" --data-dir "${TMP_DATA}" > /dev/null 2>&1
ck "rp-context assemble 可执行" "0" "$?"
# 缓存目录夹具：真实 CacheEntry 磁盘形状（cache.rs serde 词面）两条——
# 新鲜条目 policy-1 + 回溯 30 天的 policy-2 条目（mtime os.utime 回溯）。
# 归因扫描面验收 = entries / 按 TTL 类过期（类间嵌套）/ policy_version 不匹配。
python3 -I - "${TMP_DATA}/runtime/broker-cache" <<'PYEOF'
import json, os, sys, time
cache_dir = sys.argv[1]
os.makedirs(cache_dir, exist_ok=True)
def entry(key, policy, payload):
    doc = {
        "key": key, "kind": "brief",
        "validation": {"version": "1.0.0", "content_hash": "aa" * 32,
                       "source_fingerprint": "broker=0.1.0"},
        "payload": payload, "created_at": "2026-11-22T00:00:00Z",
        "policy_version": policy,
    }
    path = os.path.join(cache_dir, f"{key}.json")
    with open(path, "w") as f:
        json.dump(doc, f)
    return path
fresh = entry("cb1-fixture-fresh", "policy-1", "fresh payload")
old = entry("cb1-fixture-old", "policy-2", "stale payload")
month_ago = time.time() - 30 * 86400
os.utime(old, (month_ago, month_ago))
print("fixtures: 2")
PYEOF
ck "缓存目录夹具落盘（真实 CacheEntry 形状 ×2）" "0" "$?"
"${RP_CONTEXT}" cache-attrib --data-dir "${TMP_DATA}" --policy-version policy-1 > "${OUT}" 2>/dev/null
CACHE_RC=$?
ck "cache-attrib 可执行（exit 0）" "0" "${CACHE_RC}"
ck "cache-attrib entries=2" "2" "$(jget "$(cat "${OUT}")" "d['entries']")"
ck "cache-attrib 30 天条目四类 TTL 全过（嵌套口径）" "1" "$(jget "$(cat "${OUT}")" "d['expired']['schema']")"
ck "cache-attrib 30 天条目 catalog 类过期" "1" "$(jget "$(cat "${OUT}")" "d['expired']['catalog']")"
ck "cache-attrib 新鲜条目 schema 类不过期" "1" "$(jget "$(cat "${OUT}")" "1 if d['expired']['schema']==1 and d['expired']['knowledge']==1 else 0")"
ck "cache-attrib policy_mismatch=1（policy-2 条目对 policy-1 口径）" "1" "$(jget "$(cat "${OUT}")" "d['policy_mismatch']")"
ck "cache-attrib JSON 口径（expired 四类 + policy_mismatch）" "yes" "$(jget "$(cat "${OUT}")" "'yes' if set(d['expired'].keys())=={'catalog','knowledge','schema','rtis'} and 'policy_mismatch' in d else 'no'")"

echo "----"
echo "CB-1 CLI 折叠实测数字（进批次报告）：折叠 est ${EST_BEFORE}->${EST_AFTER}；折叠行 ${FOLDS_COUNT} 条（run ${RUN_ON}）"
if [[ "${FAIL}" == "0" ]]; then echo "结果：PASS=${PASS} FAIL=0"; exit 0; else echo "结果：PASS=${PASS} FAIL=${FAIL}"; exit 1; fi
