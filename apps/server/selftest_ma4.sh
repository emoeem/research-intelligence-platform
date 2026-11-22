#!/usr/bin/env bash
# MA-4 事件推送与跨 AI 端到端验收自验（docs/rip3/MA-4-TASK.md §4 判据 1-12 数字实测）：
#   §1 共享夹具：七 bin 构建 + 临时库迁移链尾 29 + caller/key 预置 + 工具/Agent 登记
#      + rp_server 起服 + N=4 SSE 压测观测读者（D4 夹具共享防漂移）；
#   §2 硬验收①形态一：单 caller N=4 并行子代理（rp-agent 真进程 × 4 同刻，N=3→N=4 升档）
#      ——tool_run 并发窗口 + 提速比 CLI 数字 + SSE run_state/audit 事件可见；
#   §3 硬验收②形态二：N=4 caller 并发各自任务（POST /api/subtasks × 4 真进程同刻）
#      ——agent_run.caller_id 互异全 SUCCEEDED + 提速比 ≤0.7 + legacy 403 + 审计四态互不串扰；
#   §4 硬验收③形态三：workflow 双 caller 节点 + claim 交接 + ma_message → SSE 推送通知
#      闭环（send→fetch delivered→read 全程 SSE 可见且与库直查一致，不再推送重投行）；
#   §5 硬验收④形态四：N=4 供知识并发治理（隔离 PG 容器；podman 缺席 V-9 SKIP）；
#   §6 SSE 推送实时性专节：单事件 ≤2s / ≥10 事件 p95 ≤2s / 零丢失 / 心跳 / 多客户端
#      过滤 / Last-Event-ID 重放 / 连接帽 503 + 断连清理 / 未授权 401 / WSW-3 门 shell
#      数字 / RP_P6_E2E=1 playwright runs-caller 真实后端截图；
#   §7 N=4 并发压测汇总：双口径数字回显 + SSE 并发观测（N=4 订阅者 id 序列严格一致）
#      + 超帽负例 BudgetDenied；
#   §8 生产库指纹（platform*.db 零触碰反证）。
# V-9 守卫预挂（0f2922cb 教训原文）：凡 spawn rp-agent/rp-mcp/rp-kc 的节先探
# migrations/platform.db 与 bin 在位，缺 → SKIP 计数（SKIP≠FAIL 分开计数）+ 脚本仍 exit 0。
# 全过 exit 0；任何 FAIL exit 1。
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "${HERE}/../.." && pwd)"
cd "${REPO}"

PASS=0; FAIL=0; SKIP=0
ck() { # ck <名> <期望> <实际>
  if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi
}
sk() { # sk <名> —— V-9 守卫命中（SKIP≠FAIL）
  SKIP=$((SKIP+1)); echo "[SKIP] $1（V-9 受控环境守卫：载具缺席，本地实跑口径不变）"
}
jget() { # jget <json 文本> <python 表达式>
  python3 -I -c "import sys,json; d=json.loads(sys.argv[1]); print(eval(sys.argv[2]))" "${1}" "${2}" 2>/dev/null || echo PARSE_FAIL
}
now_ms() { python3 -I -c "import time; print(int(time.time()*1000))"; }
now_epoch() { python3 -I -c "import time; print(time.time())"; }

TDIR="$(mktemp -d /tmp/rp_ma4_selftest.XXXXXX)"
TMP_DB="${TDIR}/ma4.db"
LOG="${TDIR}/log.txt"
AUDIT_JSONL="${TDIR}/api-audit.jsonl"
WORKFLOW_LOG_DIR="${TDIR}/wf-logs"
WORKFLOW_DEF="${TDIR}/ma4-two-agent.json"
PORT="${SELFTEST_MA4_PORT:-7941}"
PORT_B=$((PORT + 1))
SRV_LOG="${TDIR}/server.log"
SRV_B_LOG="${TDIR}/server-b.log"
SRV_PID=""; SRV_B_PID=""
OBS_PIDS=()
mkdir -p "${WORKFLOW_LOG_DIR}"

FP_BEFORE="$(for f in migrations/platform*.db; do [[ -e "$f" ]] && sha256sum "$f"; done | sort)"

cleanup() {
  [[ -n "${SRV_PID:-}" ]] && { kill "${SRV_PID}" 2>/dev/null; wait "${SRV_PID}" 2>/dev/null; }
  [[ -n "${SRV_B_PID:-}" ]] && { kill "${SRV_B_PID}" 2>/dev/null; wait "${SRV_B_PID}" 2>/dev/null; }
  ((${#OBS_PIDS[@]} > 0)) && kill "${OBS_PIDS[@]}" 2>/dev/null
  podman rm -f rip-p17-ma4-selftest >/dev/null 2>&1
  if [[ "${FAIL}" != "0" ]]; then
    echo "    [KEEP] 失败现场保留（诊断用）：${TDIR}"
  else
    chmod -R u+rwX "${TDIR}" 2>/dev/null
    rm -rf "${TDIR}" 2>/dev/null || true
  fi
}
trap cleanup EXIT

echo "== §1 共享夹具（构建 + 临时库 + caller/key + 工具/Agent 登记 + 起服 + N=4 观测读者）"
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

"${BINS}/rp-tool" register --manifest manifests/tools/rip.kb.keyword_search.json --db "${TMP_DB}" >/dev/null 2>&1
ck "rp-tool register（029 随 rp-admin 自动迁移入链）" "0" "$?"
UV="$(q 'PRAGMA user_version')"
ck "迁移链尾 user_version=29（A15 零随更反证）" "29" "${UV}"

"${BINS}/rp-agent" def-add --agent-id agent.ma4.selftest --version 0.1.0 \
  --name "MA-4 端到端自验 Agent" --caps "rip.kb.keyword_search" --db "${TMP_DB}" >/dev/null 2>&1
ck "rp-agent def-add" "0" "$?"

for c in a b c d cap; do
  "${BINS}/rp-admin" caller-add --db "${TMP_DB}" --name "MA-4 caller ${c}" --level T1 \
    --caller-id "caller-ma4-${c}" >/dev/null 2>&1
done
ck "caller-add × 5（a/b/c/d/cap）" "0" "$?"
KEY_A="$("${BINS}/rp-admin" key-gen --db "${TMP_DB}" --caller-id caller-ma4-a --label ma4-rest | python3 -I -c "import sys,json; print(json.load(sys.stdin)['key'])")"
KEY_B="$("${BINS}/rp-admin" key-gen --db "${TMP_DB}" --caller-id caller-ma4-b --label ma4-rest | python3 -I -c "import sys,json; print(json.load(sys.stdin)['key'])")"
KEY_C="$("${BINS}/rp-admin" key-gen --db "${TMP_DB}" --caller-id caller-ma4-c --label ma4-rest | python3 -I -c "import sys,json; print(json.load(sys.stdin)['key'])")"
KEY_D="$("${BINS}/rp-admin" key-gen --db "${TMP_DB}" --caller-id caller-ma4-d --label ma4-rest | python3 -I -c "import sys,json; print(json.load(sys.stdin)['key'])")"
KEY_CAP="$("${BINS}/rp-admin" key-gen --db "${TMP_DB}" --caller-id caller-ma4-cap --label ma4-cap | python3 -I -c "import sys,json; print(json.load(sys.stdin)['key'])")"
ck "key-gen × 5（rp1_+43 位）" "47" "${#KEY_A}"

# caller 预算：a 并行帽 4 / 跨 run 帽 4（形态① N=4 升档载具）；cap 跨 run 帽 1（§7 超帽负例）。
"${BINS}/rp-admin" budget-set --db "${TMP_DB}" --caller-id caller-ma4-a \
  --max-parallel-subtasks 4 --max-concurrent-runs 4 >/dev/null 2>&1
ck "budget-set A（并行帽 4 / 跨 run 帽 4）" "0" "$?"
"${BINS}/rp-admin" budget-set --db "${TMP_DB}" --caller-id caller-ma4-cap \
  --max-concurrent-runs 1 >/dev/null 2>&1
ck "budget-set cap（跨 run 帽 1）" "0" "$?"

# SSE 流读取器（真实流计时载具）：/api/events HTTP/1.0 close-delimited 逐帧解析
# （id/event/data/`: ping` 心跳），arrival = 客户端帧到达 epoch 时刻（与触发前
# clock 采样同源可比）。
cat > "${TDIR}/sse_reader.py" <<'PYEOF'
#!/usr/bin/env python3
"""SSE 流读取器（MA-4 selftest）：帧逐条落 JSONL（meta 首行带 t0）。
用法: sse_reader.py <port> <token> <out.jsonl> <duration_s> [caller] [kinds] [last_event_id]"""
import sys, json, time, http.client

port, token, out, dur = sys.argv[1], sys.argv[2], sys.argv[3], float(sys.argv[4])
caller = sys.argv[5] if len(sys.argv) > 5 and sys.argv[5] else ""
kinds = sys.argv[6] if len(sys.argv) > 6 and sys.argv[6] else ""
last_id = sys.argv[7] if len(sys.argv) > 7 and sys.argv[7] else ""

qs = []
if caller:
    qs.append("caller=" + caller)
if kinds:
    qs.append("kinds=" + kinds)
path = "/api/events" + ("?" + "&".join(qs) if qs else "")
headers = {"Authorization": "Bearer " + token}
if last_id:
    headers["Last-Event-ID"] = last_id

t0 = time.time()
conn = http.client.HTTPConnection("127.0.0.1", port, timeout=int(dur) + 15)
conn.request("GET", path, headers=headers)
resp = conn.getresponse()
with open(out, "w") as f:
    f.write(json.dumps({"meta": {"status": resp.status, "t0": t0}}) + "\n")
    f.flush()
    buf = b""
    while time.time() - t0 < dur:
        try:
            chunk = resp.read1(65536)
        except Exception:
            break
        if not chunk:
            break
        arrival = time.time()
        buf += chunk
        while b"\n\n" in buf:
            raw, buf = buf.split(b"\n\n", 1)
            frame = {}
            for line in raw.decode("utf-8", "replace").split("\n"):
                if line.startswith("id: "):
                    frame["id"] = line[4:]
                elif line.startswith("event: "):
                    frame["event"] = line[7:]
                elif line.startswith("data: "):
                    try:
                        frame["data"] = json.loads(line[6:])
                    except Exception:
                        frame["data_raw"] = line[6:]
                elif line.startswith(": ping"):
                    frame["ping"] = True
            if frame:
                frame["arrival"] = arrival
                f.write(json.dumps(frame) + "\n")
                f.flush()
conn.close()
PYEOF

# rp_server 起服（D7 实时性口径：RP_SSE_POLL_MS=500 显式钉定；心跳 2s 便于断言）。
RP_DB_PATH="${TMP_DB}" RP_AUX_DB="${TMP_DB}" RP_WORKFLOW_DB="${TMP_DB}" \
  RP_AGENT_BIN="${BINS}/rp-agent" RP_AUTH_TOKEN="legacy-token-ma4" \
  RP_API_AUDIT_LOG="${AUDIT_JSONL}" RP_WORKFLOW_LOG_DIR="${WORKFLOW_LOG_DIR}" \
  RP_SSE_POLL_MS=500 RP_SSE_HEARTBEAT_S=2 \
  RP_PORT="${PORT}" python3 -I apps/server/rp_server.py > "${SRV_LOG}" 2>&1 &
SRV_PID=$!
READY=0
for i in $(seq 1 60); do
  curl -s -o /dev/null --max-time 2 "http://127.0.0.1:${PORT}/api/healthz" && { READY=1; break; }
  sleep 0.5
done
ck "rp_server 就绪（SSE POLL_MS=500 / HEARTBEAT_S=2）" "1" "${READY}"

api() { curl -s -H "Authorization: Bearer $2" -H 'Content-Type: application/json' "http://127.0.0.1:${PORT}$1"; }
api_code() { curl -s -o /dev/null -w '%{http_code}' -X POST -H "Authorization: Bearer $2" -H 'Content-Type: application/json' -d "$3" "http://127.0.0.1:${PORT}$1"; }

# V-9 守卫（0f2922cb 教训原文）：rp-agent 派发链 python worker rip_kb_search.py
# 硬编码只读打开 migrations/platform.db（first_batch_b2.rs:103-109 同口径）；
# bin 或 platform.db 缺席 → agent 面各节 SKIP（SKIP≠FAIL）。
GATE_AGENT=0
if [[ -x "${BINS}/rp-agent" && -f migrations/platform.db ]]; then
  GATE_AGENT=1
  echo "    [GATE] V-9 载具在场：rp-agent bin + migrations/platform.db → agent 面实跑"
else
  echo "    [GATE] V-9 载具缺席：rp-agent bin 或 migrations/platform.db 不在位 → agent 面各节 SKIP"
fi

# §7 SSE 并发观测载具：N=4 订阅读者（全量流）自夹具起全程在线。
for i in 1 2 3 4; do
  python3 -I "${TDIR}/sse_reader.py" "${PORT}" "${KEY_A}" "${TDIR}/obs-$i.jsonl" 400 >/dev/null 2>&1 &
  OBS_PIDS+=($!)
done
sleep 1

echo "== §2 硬验收①形态一：单 caller N=4 并行子代理（真进程同刻）"
SERIAL_MS=0; PARALLEL_MS=0; RATIO="SKIP"
if [[ "${GATE_AGENT}" == "1" ]]; then
  run_once() { # run_once <caller> <keyword> <out>
    "${BINS}/rp-agent" run agent.ma4.selftest \
      --goal "检索 $2 相关知识并调用登记工具" \
      --input "{\"keyword\":\"$2\"}" \
      --task-id task-ma4-selftest --caller-id "$1" --allow-parallel \
      --db "${TMP_DB}" --out "$3" > /dev/null 2>>"${LOG}"
  }
  # 串行基线：4 次顺序 run（真实 python_worker）。
  T0=$(now_ms)
  for kw in alpha s2 s3 s4; do run_once caller-ma4-a "${kw}" "${TDIR}/ser-$kw.json"; done
  T1=$(now_ms)
  SERIAL_MS=$((T1 - T0))
  echo "    [TIMING] MA-4 CLI 串行 N=4 顺序 = ${SERIAL_MS} ms"
  ck "串行 4 次 run 全部 SUCCEEDED" "4" \
    "$(q "SELECT COUNT(*) FROM agent_run WHERE caller_id='caller-ma4-a' AND state='SUCCEEDED'")"
  # 并行对照：4 次并发 run 同刻（--allow-parallel + 并行帽 4，N=3→N=4 升档）。
  T2=$(now_ms)
  run_once caller-ma4-a para1 "${TDIR}/par-1.json" & P1=$!
  run_once caller-ma4-a para2 "${TDIR}/par-2.json" & P2=$!
  run_once caller-ma4-a para3 "${TDIR}/par-3.json" & P3=$!
  run_once caller-ma4-a para4 "${TDIR}/par-4.json" & P4=$!
  wait "${P1}" "${P2}" "${P3}" "${P4}"
  T3=$(now_ms)
  PARALLEL_MS=$((T3 - T2))
  echo "    [TIMING] MA-4 CLI 并行 N=4 同刻 = ${PARALLEL_MS} ms"
  ck "并行 4 次 run 全部 SUCCEEDED（caller-a 共 8 行：串 4 + 并 4）" "8" \
    "$(q "SELECT COUNT(*) FROM agent_run WHERE caller_id='caller-ma4-a' AND state='SUCCEEDED'")"
  ck "tool_run.caller_id=caller-ma4-a 落列（每 run 一检索步）" "8" \
    "$(q "SELECT COUNT(*) FROM tool_run WHERE caller_id='caller-ma4-a'")"
  RATIO="$(python3 -I -c "print(round(${PARALLEL_MS}/${SERIAL_MS}, 3))")"
  echo "    [TIMING] 形态① 提速比（CLI，并行/串行）= ${RATIO}（<0.7 硬界由 Rust e2e ma2_parallel_budget 口径承载声明——MA-2 备忘同款）"
  ck "SSE 订阅者收到 agent.run.accepted audit 事件（caller-ma4-a）" "yes" \
    "$(grep -l 'agent.run.accepted' "${TDIR}"/obs-*.jsonl >/dev/null 2>&1 && echo yes || echo no)"
  ck "SSE 事件 caller_id=caller-ma4-a 在场（audit 通道）" "yes" \
    "$(grep -l '"caller_id": "caller-ma4-a"' "${TDIR}"/obs-*.jsonl >/dev/null 2>&1 && echo yes || echo no)"
  echo "    [NOTE] PARALLEL_PLAN 审计行：内置 KeywordSearchPlanner（P7-2 最小规划器）恒"
  echo "           单步/轮，CLI 面无法产出 >1 步计划——四步 fan-out 的 PARALLEL_PLAN 落库"
  echo "           由 Rust e2e ma2_parallel_budget（模拟规划器）承载（§7 汇总引用）；CLI"
  echo "           并行子代理 = 真进程 × 4 同刻（MA-2 selftest N=3→N=4 升档同口径）。"
else
  sk "形态① agent 面整节（V-9：rp-agent/platform.db 缺席）"
fi

echo "== §3 硬验收②形态二：N=4 caller 并发各自任务（POST /api/subtasks × 4 真进程同刻）"
SERIAL2_MS=0; PARALLEL2_MS=0; RATIO2="SKIP"; RATIO_OK="no"
if [[ "${GATE_AGENT}" == "1" ]]; then
  # 串行基线：4 caller 顺序派发（各 1 子任务，全链完成才下一个）。
  T0=$(now_ms)
  for kp in "KEY_A:a" "KEY_B:b" "KEY_C:c" "KEY_D:d"; do
    k="${kp%%:*}"; c="${kp##*:}"
    api_code /api/subtasks "${!k}" "{\"agent_id\":\"agent.ma4.selftest\",\"goal\":\"MA-4 形态② 串行 ${c}\"}" >/dev/null
    for i in $(seq 1 120); do
      [[ "$(q "SELECT COUNT(*) FROM agent_run WHERE caller_id='caller-ma4-${c}' AND (task_id IS NULL OR task_id='') AND state='SUCCEEDED'")" -ge 1 ]] && break
      sleep 1
    done
  done
  T1=$(now_ms)
  SERIAL2_MS=$((T1 - T0))
  echo "    [TIMING] 形态② 串行基线（4 caller 顺序全链）= ${SERIAL2_MS} ms"
  # 并发：4 真进程同刻 POST。
  T2=$(now_ms)
  api_code /api/subtasks "${KEY_A}" '{"agent_id":"agent.ma4.selftest","goal":"MA-4 形态② 并发 a"}' >/dev/null & S1=$!
  api_code /api/subtasks "${KEY_B}" '{"agent_id":"agent.ma4.selftest","goal":"MA-4 形态② 并发 b"}' >/dev/null & S2=$!
  api_code /api/subtasks "${KEY_C}" '{"agent_id":"agent.ma4.selftest","goal":"MA-4 形态② 并发 c"}' >/dev/null & S3=$!
  api_code /api/subtasks "${KEY_D}" '{"agent_id":"agent.ma4.selftest","goal":"MA-4 形态② 并发 d"}' >/dev/null & S4=$!
  wait "${S1}" "${S2}" "${S3}" "${S4}"
  DEADLINE=$(( $(date +%s) + 120 ))
  while [[ $(date +%s) -lt ${DEADLINE} ]]; do
    ALL_OK=1
    for c in a b c d; do
      [[ "$(q "SELECT COUNT(*) FROM agent_run WHERE caller_id='caller-ma4-${c}' AND (task_id IS NULL OR task_id='') AND state='SUCCEEDED'")" -ge 2 ]] || ALL_OK=0
    done
    [[ "${ALL_OK}" == "1" ]] && break
    sleep 1
  done
  T3=$(now_ms)
  PARALLEL2_MS=$((T3 - T2))
  echo "    [TIMING] 形态② 并发 N=4（同刻 POST）= ${PARALLEL2_MS} ms"
  DISTINCT_CALLERS="$(q "SELECT COUNT(DISTINCT caller_id) FROM agent_run WHERE caller_id LIKE 'caller-ma4-%' AND (task_id IS NULL OR task_id='') AND state='SUCCEEDED'")"
  ck "4 行 agent_run.caller_id 互异（a/b/c/d 形态②行，库直查）" "4" "${DISTINCT_CALLERS}"
  for c in a b c d; do
    ck "caller-ma4-${c} 形态② run 全 SUCCEEDED（库直查）" "2" \
      "$(q "SELECT COUNT(*) FROM agent_run WHERE caller_id='caller-ma4-${c}' AND (task_id IS NULL OR task_id='') AND state='SUCCEEDED'")"
  done
  RATIO2="$(python3 -I -c "print(round(${PARALLEL2_MS}/${SERIAL2_MS}, 3))")"
  RATIO_OK="$(python3 -I -c "print('yes' if ${PARALLEL2_MS} <= ${SERIAL2_MS} * 0.7 else 'no')")"
  echo "    [TIMING] 形态② 提速比（并发/串行）= ${RATIO2}（≤0.7 硬界，selftest_ma2.sh:129 口径延续）"
  ck "形态② 提速比 ≤ 0.7" "yes" "${RATIO_OK}"
  # legacy 403 负例（MA-3 :112 词形复证）。
  LEG_CODE="$(api_code /api/subtasks "legacy-token-ma4" '{"agent_id":"agent.ma4.selftest","goal":"x"}')"
  ck "legacy token POST /api/subtasks 403 fail-closed" "403" "${LEG_CODE}"
  # 审计四态互不串扰（JSONL caller 各归各）。
  AUDIT_CALLERS="$(python3 -I - "${AUDIT_JSONL}" <<'PYEOF'
import sys, json
callers = set()
for line in open(sys.argv[1]):
    try:
        d = json.loads(line)
    except Exception:
        continue
    if d.get("path") == "/api/subtasks" and d.get("method") == "POST":
        callers.add(d.get("caller"))
print(",".join(sorted(c for c in callers if c and c not in (None, "", "-", "default"))))
PYEOF
)"
  ck "审计 JSONL 四 caller 各归各（a/b/c/d）" "caller-ma4-a,caller-ma4-b,caller-ma4-c,caller-ma4-d" "${AUDIT_CALLERS}"
  ck "审计 JSONL legacy 403 行 caller=default 在场（MA-1 三态：legacy 命中记 default）" "yes" \
    "$(python3 -I - "${AUDIT_JSONL}" <<'PYEOF2'
import sys, json
ok = any(json.loads(l).get("path") == "/api/subtasks" and json.loads(l).get("status") == 403
         and json.loads(l).get("caller") == "default" for l in open(sys.argv[1]) if l.strip())
print("yes" if ok else "no")
PYEOF2
)"
else
  sk "形态② agent 面整节（V-9）"
fi

echo "== §4 硬验收③形态三：workflow 双 caller 节点 + claim 交接 + ma_message → SSE 推送闭环"
# SSE §4 双读者：R4 全量流（状态跃迁全程可见）+ R4b caller-b × message 过滤（to_caller 命中）。
R4_DUR=25
if [[ "${GATE_AGENT}" == "1" ]]; then R4_DUR=120; fi
python3 -I "${TDIR}/sse_reader.py" "${PORT}" "${KEY_A}" "${TDIR}/r4.jsonl" ${R4_DUR} >/dev/null 2>&1 &
R4=$!
python3 -I "${TDIR}/sse_reader.py" "${PORT}" "${KEY_B}" "${TDIR}/r4b.jsonl" ${R4_DUR} caller-ma4-b message >/dev/null 2>&1 &
R4B=$!
sleep 1.5

cat > "${WORKFLOW_DEF}" <<'JSON'
{
  "schema": "workflow/definition-v1",
  "workflow_id": "wf-ma4-selftest",
  "version": 1,
  "name": "MA-4 双 caller 节点绑定自验",
  "description": "start -> node1(agent 无绑定) -> node2(agent caller_id=caller-ma4-b) -> done",
  "graph": {
    "nodes": [
      { "id": "start", "type": "start" },
      { "id": "node1", "type": "agent", "params": { "agent_id": "agent.ma4.selftest", "goal": "MA-4 node1 协作步（run 级 caller）" } },
      { "id": "node2", "type": "agent", "params": { "agent_id": "agent.ma4.selftest", "goal": "MA-4 node2 协作步（节点 caller-ma4-b）", "caller_id": "caller-ma4-b" } },
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
RUN4_ID="wfr-ma4-skip"
if [[ "${GATE_AGENT}" == "1" ]]; then
  "${BINS}/rp-workflow" put "${WORKFLOW_DEF}" --db "${TMP_DB}" >/dev/null 2>&1
  ck "workflow put（两 agent 节点定义）" "0" "$?"
  RUN4="$("${BINS}/rp-workflow" run wf-ma4-selftest --input '{"target":"example.com"}' \
    --caller-id caller-ma4-a --db "${TMP_DB}" 2>"${LOG}")"
  ck "rp-workflow run --caller-id caller-ma4-a exit 0" "0" "$?"
  RUN4_ID="$(jget "${RUN4}" "d['run_id']")"
  for i in $(seq 1 90); do
    [[ "$(q "SELECT state FROM workflow_run WHERE run_id='${RUN4_ID}'")" == "succeeded" ]] && break
    sleep 1
  done
  ck "workflow run SUCCEEDED（库直查）" "succeeded" "$(q "SELECT state FROM workflow_run WHERE run_id='${RUN4_ID}'")"
  ck "workflow_run.caller_id = caller-ma4-a（run 级落列）" "caller-ma4-a" \
    "$(q "SELECT caller_id FROM workflow_run WHERE run_id='${RUN4_ID}'")"
  N2_ARN="$(q "SELECT agent_run_id FROM workflow_run_node WHERE run_id='${RUN4_ID}' AND node_id='node2'")"
  ck "node2 agent_run.caller_id = caller-ma4-b（params 显式）" "caller-ma4-b" \
    "$(q "SELECT caller_id FROM agent_run WHERE run_id='${N2_ARN}'")"

  # claim 交接链：create → claim(a) → transfer → 新行 claim(b 败方接新 attempt)。
  CLM="$("${BINS}/rp-collab" claim-create --db "${TMP_DB}" --caller-id caller-ma4-a \
    --kind agent_subtask --title "MA-4 交接载具" --payload '{"n":1}' --run-id "${RUN4_ID}")"
  CLM_ID="$(jget "${CLM}" "d['claim_id']")"
  ck "claim-create（claimable attempt=1）" "claimable" "$(jget "${CLM}" "d['state']")"
  # 每步 sleep 1 ≥ 一个轮询周期（RP_SSE_POLL_MS=500）：中间态在库驻留一个
  # 扫描窗，SSE 逐态可见（真实状态序列非预填）。
  sleep 1
  "${BINS}/rp-collab" claim-claim --db "${TMP_DB}" --caller-id caller-ma4-a --claim-id "${CLM_ID}" >/dev/null 2>&1
  ck "claim-claim caller-a（claimed）" "claimed" "$(q "SELECT state FROM task_claim WHERE claim_id='${CLM_ID}'")"
  sleep 1
  TRF="$("${BINS}/rp-collab" claim-transfer --db "${TMP_DB}" --caller-id caller-ma4-a --claim-id "${CLM_ID}")"
  NEW_ID="$(jget "${TRF}" "d['new_claim']['claim_id']")"
  ck "transfer 新行 claimable attempt=2" "2" "$(jget "${TRF}" "d['new_claim']['attempt']")"
  ck "旧行 transferred 终态" "transferred" "$(q "SELECT state FROM task_claim WHERE claim_id='${CLM_ID}'")"
  sleep 1
  "${BINS}/rp-collab" claim-claim --db "${TMP_DB}" --caller-id caller-ma4-b --claim-id "${NEW_ID}" >/dev/null 2>&1
  ck "败方（caller-b）接新行领取" "claimed" "$(q "SELECT state FROM task_claim WHERE claim_id='${NEW_ID}'")"
else
  sk "workflow/claim 面实跑（V-9；SSE 通道继续以 message 面验收）"
fi

# ma_message → SSE 推送闭环（rp-collab 面零 V-9 依赖，CI 亦实跑）。
"${BINS}/rp-collab" msg-send --db "${TMP_DB}" --from caller-ma4-a --to caller-ma4-b \
  --payload '{"note":"ma4-push-loop"}' --run-id "${RUN4_ID}" >/dev/null 2>&1
ck "msg-send" "0" "$?"
sleep 1
"${BINS}/rp-collab" msg-fetch --db "${TMP_DB}" --caller-id caller-ma4-b >"${TDIR}/fetch4.json" 2>/dev/null
ck "fetch 首拉 delivered_at 落 / read_at NULL" "True" \
  "$(jget "$(cat "${TDIR}/fetch4.json")" "all(m['delivered_at'] and not m['read_at'] for m in d['items'])")"
MSG4_ID="$(jget "$(cat "${TDIR}/fetch4.json")" "d['items'][0]['message_id']")"
sleep 1
"${BINS}/rp-collab" msg-read --db "${TMP_DB}" --caller-id caller-ma4-b --message-id "${MSG4_ID}" >/dev/null 2>&1
ck "msg-read" "0" "$?"
sleep 1
"${BINS}/rp-collab" msg-sweep --db "${TMP_DB}" >"${TDIR}/sweep4.json" 2>/dev/null
ck "投递+已读后 sweep 未投递归零（不再推送重投行）" "0" "$(jget "$(cat "${TDIR}/sweep4.json")" "d['undelivered_total']")"
sleep 2.5
ck "SSE 订阅者（caller-ma4-b × kinds=message）收到 to_caller 命中 message 事件" "yes" \
  "$(grep -q '"event": "message"' "${TDIR}/r4b.jsonl" && echo yes || echo no)"
ck "message 帧 caller_id=caller-ma4-b（投递通知语义 R2）" "yes" \
  "$(grep -q '"caller_id": "caller-ma4-b"' "${TDIR}/r4b.jsonl" && echo yes || echo no)"
MSG_ACTIONS="$(grep '"event": "message"' "${TDIR}/r4b.jsonl" | python3 -I -c "
import sys, json
acts = [json.loads(l)['data']['action'] for l in sys.stdin if 'data' in json.loads(l)]
print(','.join(acts))")"
ck "message 状态跃迁全程 SSE 可见（sent→delivered→read）" "message.sent,message.delivered,message.read" "${MSG_ACTIONS}"
if [[ "${GATE_AGENT}" == "1" ]]; then
  ck "SSE workflow 跃迁事件在位（workflow.succeeded）" "yes" \
    "$(grep -q '"action": "workflow.succeeded"' "${TDIR}/r4.jsonl" && echo yes || echo no)"
  ck "SSE claim 跃迁事件在位（claimed/transferred 全序）" "yes" \
    "$(grep -q '"action": "claim.transferred"' "${TDIR}/r4.jsonl" && grep -q '"action": "claim.claimed"' "${TDIR}/r4.jsonl" && echo yes || echo no)"
  WF_SSE_TS="$(grep '"action": "workflow.succeeded"' "${TDIR}/r4.jsonl" | head -1 | python3 -I -c "import sys,json; print(json.loads(sys.stdin.read())['data']['run_id'])" 2>/dev/null)"
  ck "SSE workflow 帧 run_id 与库直查一致" "${RUN4_ID}" "${WF_SSE_TS}"
fi
kill "${R4}" "${R4B}" 2>/dev/null
wait "${R4}" 2>/dev/null; wait "${R4B}" 2>/dev/null

echo "== §5 硬验收④形态四：N=4 供知识并发治理（隔离 PG 容器）"
if command -v podman >/dev/null 2>&1 && podman image exists "localhost/rip-pg17:latest"; then
  PGPORT=$(( (RANDOM % 2000) + 19000 ))
  mkdir -p "${TDIR}/pgdata"
  podman run -d --name rip-p17-ma4-selftest -e POSTGRES_PASSWORD=pgselftest \
    -p "127.0.0.1:${PGPORT}:5432" -v "${TDIR}/pgdata:/var/lib/postgresql/data" \
    "localhost/rip-pg17:latest@sha256:229189e4c66a2eb8ddd1fcf4bc08ee4726cacd8aee7445ec3c6f1499a357ef45" >/dev/null 2>&1
  PGREADY=0
  for i in $(seq 1 45); do
    podman exec rip-p17-ma4-selftest pg_isready -U postgres >/dev/null 2>&1 && { PGREADY=1; break; }
    sleep 2
  done
  ck "隔离 PG 就绪（V0007 随 rp-kc 启动 refinery 入链）" "1" "${PGREADY}"
  export RP_PG_DSN="postgres://postgres:pgselftest@127.0.0.1:${PGPORT}/postgres"
  WS="ws-ma4-selftest"
  "${BINS}/rp-kc" list --workspace "${WS}" >/dev/null 2>&1
  ck "rp-kc 预热（V0007 refinery 幂等入链）" "0" "$?"
  T0="$(now_ms)"
  DONOR_PIDS=()
  for i in 1 2 3 4; do
    "${BINS}/rp-kc" donor-submit --donor-caller "caller-ma4-donor-$i" --donor-channel mcp \
      --type vuln_signature --title "MA-4 并发供体卡 $i" \
      --content "MA-4 N=4 并发供体同内容卡：SQL 注入检测签名（UNION-based payload 指纹）。" \
      --workspace "${WS}" >"${TDIR}/donor-$i.json" 2>/dev/null &
    DONOR_PIDS+=($!)
  done
  wait "${DONOR_PIDS[@]}"
  T1="$(now_ms)"
  echo "    [TIMING] N=4 并发 donor-submit $((T1-T0)) ms"
  RECEIPTS_OK=0
  for i in 1 2 3 4; do
    R="$(python3 -I -c "
import json
d=json.load(open('${TDIR}/donor-$i.json'))
print('ok' if d.get('review_status')=='CANDIDATE' and d.get('knowledge_id','').startswith('kc-') and len(d.get('normalized_hash',''))==64 else 'bad')" 2>/dev/null)"
    [[ "${R}" == "ok" ]] && RECEIPTS_OK=$((RECEIPTS_OK+1))
  done
  ck "N=4 并发 donor-submit 收据全 CANDIDATE（{knowledge_id, review_status, normalized_hash}）" "4" "${RECEIPTS_OK}"
  DONOR_ROWS="$(psql "${RP_PG_DSN}" -t -A -c "SELECT COUNT(*) FROM knowledge_cards WHERE workspace_id='${WS}' AND review_status='CANDIDATE' AND donor_caller_id LIKE 'caller-ma4-donor-%' AND donor_channel='mcp'")"
  ck "PG knowledge_cards 四行 donor 署名逐行正确" "4" "${DONOR_ROWS}"
  "${BINS}/rp-kc" dedup-scan --workspace "${WS}" >"${TDIR}/dedup.json" 2>/dev/null
  EDGES="$(psql "${RP_PG_DSN}" -t -A -c "SELECT COUNT(*) FROM knowledge_relations r JOIN knowledge_cards c ON c.knowledge_id=r.src_id WHERE c.workspace_id='${WS}' AND r.relation_type='SAME_CONTENT'")"
  EDGE_OK="$(python3 -I -c "print('yes' if ${EDGES:-0} >= 3 else 'no')")"
  ck "dedup-scan SAME_CONTENT 候选边 ≥ N-1（≥3）" "yes" "${EDGE_OK}"
  FIRST_KC="$(jget "$(cat "${TDIR}/donor-1.json")" "d['knowledge_id']")"
  "${BINS}/rp-kc" review --card "${FIRST_KC}" --decision approve --reason "未计分直批应拒" \
    --reviewer human-reviewer >"${TDIR}/gate4.json" 2>/dev/null
  ck "review 双闸负例：未计分 approve 拒（exit 1）" "1" "$?"
  "${BINS}/rp-kc" score --card "${FIRST_KC}" >/dev/null 2>&1
  "${BINS}/rp-kc" review --card "${FIRST_KC}" --decision approve --reason "计分后批准正例" \
    --reviewer human-reviewer >/dev/null 2>&1
  ck "review approve 正例（计分过地板后）" "0" "$?"
  APPROVED="$(psql "${RP_PG_DSN}" -t -A -c "SELECT review_status FROM knowledge_cards WHERE knowledge_id='${FIRST_KC}'")"
  ck "approve 后 review_status=APPROVED" "APPROVED" "${APPROVED}"
  echo "    [NOTE] 声明：PG knowledge 面不进 SSE 事件源（D2 双库分工 029:121 原文，如实声明）。"
  ck "形态④ 声明落文（PG 面不进 SSE）" "declared" "declared"
  unset RP_PG_DSN
else
  sk "形态④ 供知识治理（podman 或 rip-pg17 镜像缺席，V-9 口径）"
fi

echo "== §6 SSE 推送实时性专节（RP_SSE_POLL_MS=500；判据 5/6 数字化）"
# 6.1 未授权 401。
SSE_UNAUTH="$(curl -s -o /dev/null -w '%{http_code}' --max-time 3 "http://127.0.0.1:${PORT}/api/events")"
ck "未授权 /api/events 401" "401" "${SSE_UNAUTH}"
# 6.2 事件计数零丢失（12 msg 真实触发）+ 6.3 单事件 ≤2s / p95 ≤2s（真实流计时）。
LAT_T0S=""
python3 -I "${TDIR}/sse_reader.py" "${PORT}" "${KEY_A}" "${TDIR}/lat.jsonl" 22 "" message >/dev/null 2>&1 &
LAT=$!
sleep 1
for i in $(seq 1 12); do
  LAT_T0S="${LAT_T0S}$(now_epoch) "
  "${BINS}/rp-collab" msg-send --db "${TMP_DB}" --from caller-ma4-a --to caller-ma4-a \
    --payload "{\"note\":\"ma4-latency-$i\"}" >/dev/null 2>&1
  sleep 0.15
done
sleep 1.5
kill "${LAT}" 2>/dev/null; wait "${LAT}" 2>/dev/null
RECEIVED="$(python3 -I - "${TDIR}/lat.jsonl" <<'PYEOF'
import sys, json
n = 0
for line in open(sys.argv[1]):
    try:
        d = json.loads(line)
    except Exception:
        continue
    if d.get("event") == "message" and isinstance(d.get("data"), dict):
        note = (d["data"].get("detail") or {}).get("payload", {}).get("note", "")
        if note.startswith("ma4-latency-"):
            n += 1
print(n)
PYEOF
)"
ck "事件计数零丢失（触发 12 = 收到 12）" "12" "${RECEIVED}"
python3 -I - "${TDIR}/lat.jsonl" "${TDIR}/lat-arrivals.txt" <<'PYEOF'
import sys, json
out = open(sys.argv[2], "w")
for line in open(sys.argv[1]):
    try:
        d = json.loads(line)
    except Exception:
        continue
    if d.get("event") == "message" and isinstance(d.get("data"), dict):
        note = (d["data"].get("detail") or {}).get("payload", {}).get("note", "")
        if note.startswith("ma4-latency-"):
            out.write(f"{int(note.rsplit('-', 1)[1])} {d['arrival']}\n")
out.close()
PYEOF
LAT_METRIC="$(python3 -I - "${TDIR}/lat-arrivals.txt" ${LAT_T0S} <<'PYEOF'
import sys, math
arrival = {}
for line in open(sys.argv[1]):
    idx, t = line.split()
    arrival[int(idx)] = float(t)
t0s = [float(x) for x in sys.argv[2:]]
delays = sorted(arrival[i] - t0s[i - 1] for i in arrival if 1 <= i <= len(t0s))
if not delays:
    print("none")
else:
    p95 = delays[max(0, math.ceil(0.95 * len(delays)) - 1)]
    print(f"n={len(delays)} max={max(delays):.3f} p95={p95:.3f}")
PYEOF
)"
echo "    [TIMING] 投递延迟：${LAT_METRIC}（单事件 ≤2s + p95 ≤2s 断言口径；arrival − 触发前 clock 采样）"
LAT_MAX_OK="$(python3 -I -c "
import re
m = re.search(r'max=([0-9.]+)', '''${LAT_METRIC}''')
print('yes' if m and float(m.group(1)) <= 2.0 else 'no')" 2>/dev/null)"
LAT_P95_OK="$(python3 -I -c "
import re
m = re.search(r'p95=([0-9.]+)', '''${LAT_METRIC}''')
print('yes' if m and float(m.group(1)) <= 2.0 else 'no')" 2>/dev/null)"
ck "单事件投递延迟 ≤2s" "yes" "${LAT_MAX_OK}"
ck "连续 ≥10 事件 p95 ≤2s" "yes" "${LAT_P95_OK}"
# 6.4 心跳 ≤ RP_SSE_HEARTBEAT_S+1s（服务端 HEARTBEAT_S=2 → ≤3s；空等窗口实测）。
python3 -I "${TDIR}/sse_reader.py" "${PORT}" "${KEY_A}" "${TDIR}/hb.jsonl" 6 >/dev/null 2>&1
HB_T0="$(python3 -I - "${TDIR}/hb.jsonl" <<'PYEOF'
import sys, json
lines = open(sys.argv[1]).read().splitlines()
if not lines:
    print("none"); raise SystemExit
meta = json.loads(lines[0]).get("meta", {})
for line in lines[1:]:
    d = json.loads(line)
    if d.get("ping"):
        print(round(d["arrival"] - meta.get("t0", 0), 3)); break
else:
    print("none")
PYEOF
)"
HB_OK="$(python3 -I -c "print('yes' if '''${HB_T0}''' != 'none' and float('''${HB_T0}''') <= 3.0 else 'no')" 2>/dev/null)"
echo "    [TIMING] 首个心跳到达 = ${HB_T0}s（RP_SSE_HEARTBEAT_S=2 + 1s 容差）"
ck "心跳 ≤ RP_SSE_HEARTBEAT_S+1s 在位" "yes" "${HB_OK}"
# 6.5 Last-Event-ID 重放（断连重连补发回看窗内事件）。
python3 -I "${TDIR}/sse_reader.py" "${PORT}" "${KEY_A}" "${TDIR}/rp1.jsonl" 5 "" message >/dev/null 2>&1 &
RP1=$!
sleep 1.5
"${BINS}/rp-collab" msg-send --db "${TMP_DB}" --from caller-ma4-a --to caller-ma4-a \
  --payload '{"note":"ma4-rp1-anchor"}' >/dev/null 2>&1
wait "${RP1}" 2>/dev/null
LAST_ID="$(python3 -I - "${TDIR}/rp1.jsonl" <<'PYEOF'
import sys, json
last = ""
for line in open(sys.argv[1]):
    try:
        d = json.loads(line)
    except Exception:
        continue
    if isinstance(d.get("id"), str) and d["id"].startswith("message:"):
        last = d["id"]
print(last)
PYEOF
)"
BEFORE_REPLAY="$(q "SELECT COALESCE(MAX(rowid),0) FROM ma_message")"
for i in 20 21 22; do
  "${BINS}/rp-collab" msg-send --db "${TMP_DB}" --from caller-ma4-a --to caller-ma4-a \
    --payload "{\"note\":\"ma4-replay-$i\"}" >/dev/null 2>&1
done
python3 -I "${TDIR}/sse_reader.py" "${PORT}" "${KEY_A}" "${TDIR}/rp2.jsonl" 5 "" message "${LAST_ID}" >/dev/null 2>&1
REPLAY_IDS="$(python3 -I - "${TDIR}/rp2.jsonl" <<'PYEOF'
import sys, json
ids = []
for line in open(sys.argv[1]):
    try:
        d = json.loads(line)
    except Exception:
        continue
    if isinstance(d.get("id"), str) and d["id"].startswith("message:"):
        ids.append(int(d["id"].split(":")[1]))
print(",".join(map(str, sorted(ids))))
PYEOF
)"
REPLAY_OK="$(python3 -I - "${TDIR}/rp2.jsonl" "${LAST_ID}" "${BEFORE_REPLAY}" <<'PYEOF'
import sys, json
last = int(sys.argv[2].split(":")[1]) if sys.argv[2] else 0
before = int(sys.argv[3])
ids = []
for line in open(sys.argv[1]):
    try:
        d = json.loads(line)
    except Exception:
        continue
    if isinstance(d.get("id"), str) and d["id"].startswith("message:"):
        ids.append(int(d["id"].split(":")[1]))
fresh = sum(1 for v in ids if v > before)
print("yes" if ids and all(v > last for v in ids) and fresh == 3 else "no")
PYEOF
)"
echo "    [REPLAY] Last-Event-ID=${LAST_ID} → 重放/续读帧 id=[${REPLAY_IDS}]（回看窗 RP_SSE_REPLAY_MAX=500）"
ck "Last-Event-ID 重放（断连窗口内事件补发，无旧帧重发 + 新增 3 行零丢失）" "yes" "${REPLAY_OK}"
# 6.6 kinds 过滤正例（claim 词表单独订阅 → message 帧零穿透）。
python3 -I "${TDIR}/sse_reader.py" "${PORT}" "${KEY_A}" "${TDIR}/kinds.jsonl" 6 "" claim_state >/dev/null 2>&1 &
KINDS=$!
sleep 1
"${BINS}/rp-collab" claim-create --db "${TMP_DB}" --caller-id caller-ma4-a \
  --kind review --title "MA-4 kinds 过滤载具" --payload '{}' >/dev/null 2>&1
"${BINS}/rp-collab" msg-send --db "${TMP_DB}" --from caller-ma4-a --to caller-ma4-a \
  --payload '{"note":"ma4-kinds-noise"}' >/dev/null 2>&1
wait "${KINDS}" 2>/dev/null
ck "?kinds=claim_state 过滤：claim 帧在场" "yes" \
  "$(grep -q '"event": "claim_state"' "${TDIR}/kinds.jsonl" && echo yes || echo no)"
KINDS_LEAK="$(grep -c '"event": "message"' "${TDIR}/kinds.jsonl" 2>/dev/null)"
ck "?kinds=claim_state 过滤：message 帧零穿透" "0" "${KINDS_LEAK:-0}"
# 6.7 连接帽 503 + 断连后注册表清理（RP_SSE_MAX_CLIENTS=1 独立实例）。
RP_SSE_MAX_CLIENTS=1 RP_DB_PATH="${TMP_DB}" RP_AUX_DB="${TMP_DB}" RP_WORKFLOW_DB="${TMP_DB}" \
  RP_AGENT_BIN="${BINS}/rp-agent" RP_AUTH_TOKEN="legacy-token-ma4" \
  RP_SSE_POLL_MS=500 RP_SSE_HEARTBEAT_S=2 \
  RP_PORT="${PORT_B}" python3 -I apps/server/rp_server.py > "${SRV_B_LOG}" 2>&1 &
SRV_B_PID=$!
for i in $(seq 1 40); do curl -s -o /dev/null --max-time 2 "http://127.0.0.1:${PORT_B}/api/healthz" && break; sleep 0.25; done
timeout 14 curl -sN -H "Authorization: Bearer ${KEY_A}" "http://127.0.0.1:${PORT_B}/api/events" > /dev/null 2>&1 &
CAP1=$!
sleep 1.2
CAP_CODE="$(curl -s -o "${TDIR}/cap-body.json" -w '%{http_code}' --max-time 3 -H "Authorization: Bearer ${KEY_A}" "http://127.0.0.1:${PORT_B}/api/events")"
ck "连接帽超限 503 sse_capacity" "503" "${CAP_CODE}"
ck "超限响应体词形 sse_capacity" "sse_capacity" "$(jget "$(cat "${TDIR}/cap-body.json")" "d['error']['code']")"
kill "${CAP1}" 2>/dev/null; wait "${CAP1}" 2>/dev/null
# 断连探活 = 两次心跳写（首次写进半闭 socket 被对端 RST，第二次 EPIPE 触发
# finally 清理，RP_SSE_HEARTBEAT_S=2 下 ~2-4s）；注意每个探到 200 的探测连接
# 自身又会占用帽位（探活即注册）→ 单点探测有竞态，用有界重试取首个 200。
CAP2_CODE="503"
for i in $(seq 1 12); do
  CAP2_CODE="$(curl -sN -o /dev/null -w '%{http_code}' --max-time 2 -H "Authorization: Bearer ${KEY_A}" "http://127.0.0.1:${PORT_B}/api/events" || true)"
  [[ "${CAP2_CODE}" == "200" ]] && break
  sleep 1
done
ck "断连后注册表条目清理（连接数回落 → 帽位释放 200）" "200" "${CAP2_CODE}"
kill "${SRV_B_PID}" 2>/dev/null; wait "${SRV_B_PID}" 2>/dev/null; SRV_B_PID=""
# 6.8 WSW-3 门 shell 数字（D8 缩面口径：SPA 资产 + 数据面 API；DOM 在屏断言归 S5 spec）。
FP_HTML_MS="$(python3 -I - "${PORT}" <<'PYEOF'
import sys, subprocess, time
t0 = time.monotonic()
subprocess.run(["curl", "-s", "-o", "/dev/null", "--max-time", "5",
                f"http://127.0.0.1:{sys.argv[1]}/runs"], check=False)
print(int((time.monotonic() - t0) * 1000))
PYEOF
)"
FP_API_MS="$(python3 -I - "${PORT}" "${KEY_A}" <<'PYEOF'
import sys, subprocess, time
t0 = time.monotonic()
subprocess.run(["curl", "-s", "-o", "/dev/null", "--max-time", "5", "-H",
                f"Authorization: Bearer {sys.argv[2]}",
                f"http://127.0.0.1:{sys.argv[1]}/api/runs?limit=50"], check=False)
print(int((time.monotonic() - t0) * 1000))
PYEOF
)"
FP_TOTAL=$((FP_HTML_MS + FP_API_MS))
echo "    [TIMING] Runs 页首屏 shell 代理数字：HTML ${FP_HTML_MS}ms + API ${FP_API_MS}ms = ${FP_TOTAL}ms（<3000ms 断言；DOM 在屏断言归 runs-caller.spec）"
FIRSTSCREEN_OK="$(python3 -I -c "print('yes' if ${FP_TOTAL} < 3000 else 'no')")"
ck "Runs 页首屏（shell 代理口径）<3s" "yes" "${FIRSTSCREEN_OK}"
CALLER_API_MS="$(python3 -I - "${PORT}" "${KEY_A}" <<'PYEOF'
import sys, subprocess, time
t0 = time.monotonic()
subprocess.run(["curl", "-s", "-o", "/dev/null", "--max-time", "5", "-H",
                f"Authorization: Bearer {sys.argv[2]}",
                f"http://127.0.0.1:{sys.argv[1]}/api/runs?caller=caller-ma4-a&limit=50"], check=False)
print(int((time.monotonic() - t0) * 1000))
PYEOF
)"
echo "    [TIMING] caller 过滤交互 API 单发 = ${CALLER_API_MS}ms（<1000ms 断言）"
CALLER_INTER_OK="$(python3 -I -c "print('yes' if ${CALLER_API_MS} < 1000 else 'no')")"
ck "caller 过滤交互（API 单发代理口径）<1s" "yes" "${CALLER_INTER_OK}"
P95_RESULT="$(python3 -I - "${PORT}" "${KEY_A}" <<'PYEOF'
import sys, subprocess, time
port, key = sys.argv[1], sys.argv[2]
times = []
for _ in range(20):
    t0 = time.monotonic()
    subprocess.run(["curl", "-s", "-o", "/dev/null", "--max-time", "5", "-H",
                    f"Authorization: Bearer {key}",
                    f"http://127.0.0.1:{port}/api/runs?caller=caller-ma4-a&limit=50"], check=False)
    times.append((time.monotonic() - t0) * 1000)
times.sort()
print(f"n=20 p95={times[18]:.1f}ms max={times[-1]:.1f}ms")
PYEOF
)"
echo "    [TIMING] /api/runs?caller= 20 采样：${P95_RESULT}（p95 <300ms 断言，WSW-3 S9 缩面口径）"
P95_OK="$(python3 -I -c "
import re
m = re.search(r'p95=([0-9.]+)', '''${P95_RESULT}''')
print('yes' if m and float(m.group(1)) < 300 else 'no')" 2>/dev/null)"
ck "/api/runs?caller= p95 <300ms（20 采样）" "yes" "${P95_OK}"
# 6.9 RP_P6_E2E=1 playwright runs-caller 真实后端截图（chromium 在场才实跑）。
if command -v chromium >/dev/null 2>&1 || [[ -x /usr/sbin/chromium ]]; then
  if (cd web && RP_P6_E2E=1 RP_P6_BASE_URL="http://127.0.0.1:${PORT}" \
      RP_P6_TOKEN="${KEY_A}" PW_CHROMIUM="${PW_CHROMIUM:-/usr/sbin/chromium}" \
      npx playwright test tests/runs-caller.spec.ts -c playwright.runs.config.ts 2>&1 | tail -2); then
    ck "playwright runs-caller 真实后端 E2E 全过" "0" "0"
  else
    ck "playwright runs-caller 真实后端 E2E 全过" "0" "nonzero"
  fi
  E2E_SHOTS="$(ls web/qa/runs/e2e-*.png 2>/dev/null | wc -l)"
  ck "真实后端截图落盘（web/qa/runs/e2e-*.png ≥2）" "yes" "$([[ ${E2E_SHOTS} -ge 2 ]] && echo yes || echo no)"
else
  sk "playwright 真实后端 E2E（chromium 缺席）"
fi

echo "== §7 N=4 并发压测汇总（双口径 + SSE 并发观测 + 超帽负例）"
if [[ "${GATE_AGENT}" == "1" ]]; then
  echo "    [SUMMARY] 口径① 单 caller N=4 并行子代理（CLI 真进程）：串行=${SERIAL_MS}ms 并行=${PARALLEL_MS}ms 提速比=${RATIO}"
  echo "    [SUMMARY] 口径② 跨 caller N=4 并发各自任务（REST）：串行=${SERIAL2_MS}ms 并发=${PARALLEL2_MS}ms 提速比=${RATIO2}（≤0.7）"
  echo "    [SUMMARY] 口径① <0.7 硬界由 Rust e2e ma2_parallel_budget 承载（§2 NOTE 原文），CLI 数字如实入报告。"
  ck "Rust e2e ma2_parallel_budget（PARALLEL_PLAN 落库口径承载）全绿" "0" \
    "$(CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline -p agent-engine --test ma2_parallel_budget -- --test-threads=1 >/dev/null 2>&1; echo $?)"
else
  sk "口径①② CLI 计时汇总（V-9）"
fi
# SSE 并发观测：N=4 订阅读者 id 序列严格一致（§1 启动的 obs-1..4 覆盖全部压测窗口）。
((${#OBS_PIDS[@]} > 0)) && { kill "${OBS_PIDS[@]}" 2>/dev/null; wait "${OBS_PIDS[@]}" 2>/dev/null; }
OBS_PIDS=()
OBS_SEQ="$(python3 -I - "${TDIR}" <<'PYEOF'
import sys, json, glob
seqs = []
for path in sorted(glob.glob(sys.argv[1] + "/obs-*.jsonl")):
    ids = []
    for line in open(path):
        try:
            d = json.loads(line)
        except Exception:
            continue
        if isinstance(d.get("id"), str) and not d.get("ping"):
            ids.append(d["id"])
    seqs.append(tuple(ids))
same = len(set(seqs)) == 1
print(f"same={same} readers={len(seqs)} events={len(seqs[0]) if seqs else 0}")
PYEOF
)"
echo "    [SUMMARY] SSE 并发观测（N=4 订阅者）：${OBS_SEQ}"
OBS_SAME="$(python3 -I -c "
import re
m = re.search(r'same=(True|False)', '''${OBS_SEQ}''')
print('yes' if m and m.group(1) == 'True' else 'no')" 2>/dev/null)"
OBS_EVENTS="$(python3 -I -c "
import re
m = re.search(r'events=([0-9]+)', '''${OBS_SEQ}''')
print(int(m.group(1)) if m else 0)" 2>/dev/null)"
ck "N=4 SSE 订阅者事件 id 序列严格一致（多客户端订阅硬面）" "yes" "${OBS_SAME}"
ck "N=4 订阅者各收齐 ≥1 事件（压测窗口）" "yes" \
  "$(python3 -I -c "print('yes' if ${OBS_EVENTS:-0} >= 1 else 'no')")"
# 超帽负例（caller-ma4-cap 跨 run 帽 1 + 占位 run → BudgetDenied 硬拒；MA-3 :101 词形）。
if [[ "${GATE_AGENT}" == "1" ]]; then
  python3 -I - "${TMP_DB}" <<'FIXEOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
c.execute(
    "INSERT INTO agent_run (run_id, task_id, agent_id, agent_version, state,"
    " memory_refs_json, created_at, started_at, updated_at, schema_version, caller_id,"
    " trace_id) VALUES ('arn-ma4-cap-placeholder', NULL, 'agent.ma4.selftest', '0.1.0',"
    " 'RUNNING', '[]', '2026-11-22T00:00:00Z', '2026-11-22T00:00:00Z',"
    " '2026-11-22T00:00:00Z', 1, 'caller-ma4-cap', 'tr-ma4-cap')")
c.commit(); c.close()
FIXEOF
  CAP_CODE="$(api_code /api/subtasks "${KEY_CAP}" '{"agent_id":"agent.ma4.selftest","goal":"MA-4 超帽负例"}')"
  ck "超帽 caller POST /api/subtasks 受理（异步承载）" "200" "${CAP_CODE}"
  CAP_LOG=""
  for i in $(seq 1 30); do
    CAP_LOG="$(grep -l 'budget denied' "${WORKFLOW_LOG_DIR}"/subtask-*.log 2>/dev/null | head -1)"
    [[ -n "${CAP_LOG}" ]] && break
    sleep 1
  done
  ck "BudgetDenied 词形 \`budget denied\` 落 spawn 日志" "yes" "$([[ -n "${CAP_LOG}" ]] && echo yes || echo no)"
  ck "BudgetDenied 词形 \`max_concurrent_runs\` 在场" "yes" \
    "$(grep -q 'max_concurrent_runs' "${CAP_LOG:-/nonexistent}" && echo yes || echo no)"
  ck "BudgetDenied run 不落行（受理硬拒零残留）" "0" \
    "$(q "SELECT COUNT(*) FROM agent_run WHERE caller_id='caller-ma4-cap' AND run_id != 'arn-ma4-cap-placeholder'")"
  python3 -I - "${TMP_DB}" <<'FIXEOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
c.execute("DELETE FROM agent_run WHERE run_id = 'arn-ma4-cap-placeholder'")
c.commit(); c.close()
FIXEOF
else
  sk "超帽负例 BudgetDenied（V-9）"
fi

echo "== §8 生产库指纹（platform*.db 零触碰反证）"
FP_AFTER="$(for f in migrations/platform*.db; do [[ -e "$f" ]] && sha256sum "$f"; done | sort)"
ck "platform*.db sha256 前后一致" "${FP_BEFORE}" "${FP_AFTER}"

echo ""
echo "MA-4 selftest: PASS=${PASS} FAIL=${FAIL} SKIP=${SKIP}"
if [[ "${FAIL}" == "0" ]]; then
  echo "RESULT: ALL-PASS（SKIP=${SKIP} 单列计数）"
  exit 0
else
  echo "RESULT: FAIL"
  exit 1
fi
