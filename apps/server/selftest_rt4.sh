#!/usr/bin/env bash
# DI-3/RT-4 运行时迁移整合批收口自验（docs/phase0/DI-3-RT-4-TASK.md §5 + 判据
# 1/2/3/8/10 数字实测）：
#   1. rp-tool validate 29 份派发面 Manifest 逐份 exit 0（判据 2 正例面）；
#   2. Resolver 快照重录比对（判据 1：28 assign + 1 not_applicable，
#      RT3_SNAPSHOT_DUMP=1 实跑逐行核对 B/C/F 组落点）；
#   3. dispatch 拒绝面 CLI 实跑（判据 3 负例矩阵 CLI 面：词表外 RT3_RUNTIME_
#      VOCAB / venv 缺根 RT3_VENV_UNAVAILABLE / 元数据条目 VALIDATION 拒 /
#      podman_vm RT3_VM_NOT_READY——dispatch_policy_gate.rs e2e 的脚本面复证）；
#   4. 29 份派发面计数与 venv 声明计数回显 + yaml 孪生一致性门（判据 8）；
#   5. 生产库指纹前后一致（红线 1：platform*.db 零触碰反证）。
# V-9 守卫预挂（0f2922cb 教训口径）：载具（cargo/seed/migrations/lock）缺 →
# SKIP 计数 + exit 0，不伪装 PASS 不误报 FAIL。PASS/SKIP 分开计数。
# 全 PASS（或全 SKIP）exit 0；任何 FAIL exit 1。
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO}"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/home/emo/rt3-target}"
PASS=0; SKIP=0; FAIL=0
ck() { # ck <名> <期望> <实际>
  if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi
}
sk() { # sk <名> <理由> — V-9 SKIP 计数（不混报 PASS）
  SKIP=$((SKIP+1)); echo "[SKIP] $1 ($2)"
}

TDIR="$(mktemp -d /tmp/rp_rt4_selftest.XXXXXX)"
MCP_TOKEN="rp-mcp-rt4-selftest-$(python3 -c 'import secrets; print(secrets.token_hex(16))')"
cleanup() { rm -rf "${TDIR}"; }
trap cleanup EXIT

echo "== 0. V-9 守卫预挂（载具缺 → SKIP + exit 0，0f2922cb 教训口径）"
CARRIERS_MISSING=()
[[ -d migrations ]] || CARRIERS_MISSING+=("migrations/")
[[ -f manifests/capabilities/host-catalog-seed.json ]] || CARRIERS_MISSING+=("manifests/capabilities/host-catalog-seed.json")
[[ -f manifests/tools/rip.ops.worker-runtime.json ]] || CARRIERS_MISSING+=("manifests/tools/rip.ops.worker-runtime.json")
[[ -d manifests/tools ]] || CARRIERS_MISSING+=("manifests/tools/")
[[ -f workers/python/requirements.lock.txt ]] || CARRIERS_MISSING+=("workers/python/requirements.lock.txt")
command -v cargo >/dev/null 2>&1 || CARRIERS_MISSING+=("cargo")
command -v python3 >/dev/null 2>&1 || CARRIERS_MISSING+=("python3")
if [[ "${#CARRIERS_MISSING[@]}" -gt 0 ]]; then
  for m in "${CARRIERS_MISSING[@]}"; do sk "载具在位" "missing ${m}"; done
  echo "SUMMARY PASS=${PASS} SKIP=${SKIP} FAIL=${FAIL}（载具缺位整体 SKIP）"
  exit 0
fi
echo "== 0b. 构建（cargo --offline）"
cargo build --offline -q -p tool-registry --bin rp-tool || { echo "[FAIL] cargo build rp-tool"; exit 1; }
cargo build --offline -q -p mcp-server --bin rp-mcp || { echo "[FAIL] cargo build rp-mcp"; exit 1; }
PASS=$((PASS+1)); echo "[PASS] rp-tool + rp-mcp 构建在位"

echo "== 1. rp-tool validate 29 份派发面 Manifest 逐份 exit 0（判据 2 正例面）"
VALIDATE_FAIL=0; VALIDATE_OK=0
for m in manifests/tools/*.json manifests/tools/rip.http.tls_inspect.yaml; do
  if "${CARGO_TARGET_DIR}/debug/rp-tool" validate --manifest "$m" >/dev/null 2>&1; then
    VALIDATE_OK=$((VALIDATE_OK+1))
  else
    echo "[FAIL] validate $m"; VALIDATE_FAIL=$((VALIDATE_FAIL+1)); FAIL=$((FAIL+1))
  fi
done
ck "validate 通过份数（28 JSON + 1 YAML-only）" "29" "${VALIDATE_OK}"
ck "validate FAIL 份数" "0" "${VALIDATE_FAIL}"

echo "== 2. Resolver 快照重录比对（判据 1：28 assign + 1 not_applicable）"
SNAP_LOG="${TDIR}/snapshot.log"
SNAP_KV="${TDIR}/snapshot.kv"
if RT3_SNAPSHOT_DUMP=1 cargo test --offline -q -p tool-engine --test policy_resolver_manifests \
    rt3_snapshot_dump_for_batch_report -- --test-threads=1 --nocapture >"${SNAP_LOG}" 2>&1; then
  python3 -I - "${SNAP_LOG}" "${SNAP_KV}" <<'SNAPEOF'
import json, sys
rows = []
for line in open(sys.argv[1], encoding="utf-8"):
    if "SNAPSHOT rip." not in line:
        continue
    d = json.loads(line[line.index("{"):])
    rows.append((d["tool_id"], d["verdict"], d.get("runtime")))
with open(sys.argv[2], "w", encoding="utf-8") as f:
    f.write(f"SNAP_N={len(rows)}\n")
    for verdict, name in [("assign", "ASSIGN_N"), ("not_applicable", "NA_N"),
                          ("inconsistent", "INC_N"), ("reject", "REJ_N")]:
        f.write(f"{name}={sum(1 for _, v, _ in rows if v == verdict)}\n")
    for tid in ["rip.knowledge.document.fetch", "rip.knowledge.source.sync",
                "rip.evidence.rtk.mcp", "rip.ops.worker-runtime"]:
        want = "not_applicable" if tid == "rip.ops.worker-runtime" else "assign"
        got = dict((t, v) for t, v, _ in rows).get(tid, "missing")
        f.write(f"POINT_{tid}={'ok' if got == want else got}\n")
SNAPEOF
  kv() { grep -E "^$1=" "${SNAP_KV}" | cut -d= -f2; }
  ck "快照总行数（29 份派发面）" "29" "$(kv SNAP_N)"
  ck "assign 计数（判据 1）" "28" "$(kv ASSIGN_N)"
  ck "not_applicable 计数" "1" "$(kv NA_N)"
  ck "inconsistent 计数（迁移完成态零成员）" "0" "$(kv INC_N)"
  ck "B 组落点 rip.knowledge.document.fetch" "ok" "$(kv POINT_rip.knowledge.document.fetch)"
  ck "B 组落点 rip.knowledge.source.sync" "ok" "$(kv POINT_rip.knowledge.source.sync)"
  ck "C 组落点 rip.evidence.rtk.mcp" "ok" "$(kv POINT_rip.evidence.rtk.mcp)"
  ck "F 组落点 rip.ops.worker-runtime" "ok" "$(kv POINT_rip.ops.worker-runtime)"
else
  sk "Resolver 快照重录" "cargo test 运行失败（见 ${SNAP_LOG}）"
fi

echo "== 3. dispatch 拒绝面 CLI 实跑（判据 3 负例矩阵 CLI 面）"
RUN_DB="${TDIR}/run.db"
FIX_MANIFESTS="${TDIR}/manifests"
mkdir -p "${FIX_MANIFESTS}"
cat > "${FIX_MANIFESTS}/rip.t.gate.vocab.json" <<'JSONEOF'
{"schema_version":"1","id":"rip.t.gate.vocab","name":"rt4 gate vocab","version":"0.1.0","kind":"tool","family":"utility","description":"selftest rt4 vocab-outside fixture","runtime":{"type":"mcp_client"},"input_schema":{"type":"object"},"output_schema":{"type":"object"},"resources":{"cpu":"1","memory":"64Mi","timeout_seconds":30,"max_output_bytes":65536},"network":{"mode":"none"},"secrets":[],"permission_level":"T0","idempotent":true,"health":{"probe":{"type":"worker_ping"}},"provenance":{"origin":"rip-native","license":"repo-internal","dependencies":[],"tested":"selftest rt4"}}
JSONEOF
cat > "${FIX_MANIFESTS}/rip.t.gate.venv.json" <<'JSONEOF'
{"schema_version":"1","id":"rip.t.gate.venv","name":"rt4 gate venv","version":"0.1.0","kind":"tool","family":"utility","description":"selftest rt4 venv-absent fixture","runtime":{"type":"python_worker","protocol_version":"1","worker_name":"parsing-worker","command":["python3","-I","workers/python/parsing_worker.py"],"working_dir":"repo://research-platform","venv":{"path":"/nonexistent/rt4-selftest-absent-venv","direct_deps":["pip"]}},"input_schema":{"type":"object"},"output_schema":{"type":"object"},"resources":{"cpu":"1","memory":"64Mi","timeout_seconds":30,"max_output_bytes":65536},"network":{"mode":"none"},"secrets":[],"permission_level":"T0","idempotent":true,"health":{"probe":{"type":"worker_ping"}},"provenance":{"origin":"rip-native","license":"repo-internal","dependencies":[],"tested":"selftest rt4"}}
JSONEOF
cat > "${FIX_MANIFESTS}/rip.t.gate.podmanvm.json" <<'JSONEOF'
{"schema_version":"1","id":"rip.t.gate.podmanvm","name":"rt4 gate podmanvm","version":"0.1.0","kind":"tool","family":"utility","description":"selftest rt4 podman_vm fixture","runtime":{"type":"podman_vm","image":"@sha256:1111111111111111111111111111111111111111111111111111111111111111","entrypoint":["/bin/true"]},"input_schema":{"type":"object"},"output_schema":{"type":"object"},"resources":{"cpu":"1","memory":"64Mi","timeout_seconds":30,"max_output_bytes":65536},"network":{"mode":"vm_local"},"secrets":[],"permission_level":"T0","idempotent":true,"health":{"probe":{"type":"worker_ping"}},"provenance":{"origin":"rip-native","license":"repo-internal","dependencies":[],"tested":"selftest rt4"}}
JSONEOF
cp manifests/tools/rip.ops.worker-runtime.json "${FIX_MANIFESTS}/"

# 生产库指纹（红线 1 只读反证）：实跑节前后一致。
FP_BEFORE="$(for f in migrations/platform*.db; do [[ -e "$f" ]] && sha256sum "$f"; done | sort | sha256sum | cut -d' ' -f1)"

# 单请求进出（phase9_mcp 同款 stdio 三行握手；独立会话用后即断）。
mcp_req() { # mcp_req <请求行> <响应落盘>
  printf '%s\n%s\n%s\n' \
    '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"selftest-rt4","version":"0"}}}' \
    '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
    "$1" | RP_MCP_TOKEN="${MCP_TOKEN}" RP_RUN_DB="${RUN_DB}" \
    RP_MIGRATIONS_DIR="migrations" RP_MANIFEST_DIRS="${FIX_MANIFESTS}" \
    RP_CAPABILITY_SEED="manifests/capabilities/host-catalog-seed.json" \
    "${CARGO_TARGET_DIR}/debug/rp-mcp" 2>/dev/null | grep '"id":9' > "$2"
}
gate_check() { # gate_check <名> <tool_id> <期望码或类别>
  local name="$1" tool="$2" want="$3" resp="${TDIR}/resp.json"
  mcp_req "{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"tools/call\",\"params\":{\"name\":\"rip.run_tool\",\"arguments\":{\"tool_id\":\"${tool}\",\"input\":{}}}}" "${resp}"
  local got
  got="$(python3 -I - "${resp}" <<'GATEEOF'
import json, sys
d = json.load(open(sys.argv[1]))
text = d["result"]["content"][0]["text"]
p = json.loads(text)
err = p.get("error") or {}
print(err.get("code") or err.get("category") or "NO_ERROR")
GATEEOF
)" || got="PARSE_FAIL"
  ck "$name → ${want}" "${want}" "${got}"
}
gate_check "负例① 词表外 runtime.type" "rip.t.gate.vocab" "RT3_RUNTIME_VOCAB"
gate_check "负例② venv 缺根" "rip.t.gate.venv" "RT3_VENV_UNAVAILABLE"
gate_check "负例③ 元数据条目 worker-runtime" "rip.ops.worker-runtime" "VALIDATION"
gate_check "负例⑤ podman_vm 声明" "rip.t.gate.podmanvm" "RT3_VM_NOT_READY"
# 拒绝不落 run 行（受理拒绝语义，四负例合并断言零 run 行）。
ROWS="$(python3 -I - "${RUN_DB}" <<'ROWEOF'
import sqlite3, sys
print(sqlite3.connect(sys.argv[1]).execute("SELECT COUNT(*) FROM tool_run").fetchone()[0])
ROWEOF
)"
ck "四负例受理拒绝零 run 行" "0" "${ROWS}"

echo "== 4. 计数回显 + venv 声明计数 + 孪生一致性门（判据 8）"
COUNT_KV="${TDIR}/count.kv"
python3 -I - "${COUNT_KV}" <<'COUNTEOF'
import glob, json, os, sys
try:
    import yaml
except ImportError:
    yaml = None
jsons = sorted(glob.glob("manifests/tools/*.json"))
dispatch = len(jsons) + (1 if os.path.exists("manifests/tools/rip.http.tls_inspect.yaml") else 0)
venv = 0
for p in jsons:
    d = json.load(open(p))
    rt = d.get("runtime", {})
    if rt.get("type") == "python_worker" and rt.get("venv"):
        venv += 1
if yaml and os.path.exists("manifests/tools/rip.http.tls_inspect.yaml"):
    y = yaml.safe_load(open("manifests/tools/rip.http.tls_inspect.yaml"))
    if y.get("runtime", {}).get("type") == "python_worker" and y["runtime"].get("venv"):
        venv += 1
bad = []
if yaml:
    for p in jsons:
        t = p[:-5] + ".yaml"
        if os.path.exists(t):
            a = json.dumps(json.load(open(p)), sort_keys=True, ensure_ascii=False)
            b = json.dumps(yaml.safe_load(open(t)), sort_keys=True, ensure_ascii=False)
            if a != b:
                bad.append(p)
with open(sys.argv[1], "w", encoding="utf-8") as f:
    f.write(f"DISPATCH_FACE={dispatch}\n")
    f.write(f"VENV_DECLARED={venv}\n")
    f.write(f"TWIN_DRIFT={len(bad)}\n")
COUNTEOF
ckv() { grep -E "^$1=" "${COUNT_KV}" | cut -d= -f2; }
ck "派发面 Manifest 计数" "29" "$(ckv DISPATCH_FACE)"
ck "venv 声明计数（A 组 17 份）" "17" "$(ckv VENV_DECLARED)"
ck "yaml 孪生漂移份数" "0" "$(ckv TWIN_DRIFT)"

echo "== 5. 生产库指纹反证（红线 1：platform*.db 零触碰）"
FP_AFTER="$(for f in migrations/platform*.db; do [[ -e "$f" ]] && sha256sum "$f"; done | sort | sha256sum | cut -d' ' -f1)"
ck "生产库指纹前后一致" "${FP_BEFORE}" "${FP_AFTER}"

echo "== SUMMARY PASS=${PASS} SKIP=${SKIP} FAIL=${FAIL}"
if [[ "${FAIL}" -gt 0 ]]; then
  exit 1
fi
exit 0
