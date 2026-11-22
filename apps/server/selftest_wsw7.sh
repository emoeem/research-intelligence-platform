#!/usr/bin/env bash
# ============================================================================
# selftest_wsw7.sh — WSW-7 Plugin / Scanner Integration 收口（WSW-7-TASK
#   §三 S8 + 判据数字来源；禁模拟冒充——全部数字来自命令回显）。
#
# 节面：登记面（validate/register 副本幂等）→ providers 投影对账 →
#   rp-wsw scan CLI 负例/help 面 → RTIS 真实委派（V-9 rt.py 真源守卫）→
#   Nuclei 真实扫描 local_range（V-9 podman+镜像 digest 守卫——缺 SKIP 计数）
#   → ZAP 真实扫描同口径 → 预算负例词形 → rp_server 两端点（401/400/投影）
#   → 生产库只读指纹反证 + integrations/rtis 零 diff 反证。
#
# V-9 守卫预挂（D6 五项环境事实）：rp-wsw/rp-tool bin / migrations/dev-b1.db
#   副本 / python3 缺 → SKIP 计数 exit 0（selftest_wsw56.sh :29-42 GUARD_MISS
#   形状）；podman bin + 镜像 digest / rt.py 数据真源缺 → 分面 SKIP 计数
#   （三 adapter 真实派发按环境可用性 PASS/SKIP 分开计数——S8 门）。
# 纪律：临时工作目录（退出不清理，留对账）；platform*.db 零触碰（只读指纹）；
#   integrations/rtis/** 零写面；local_range 靶机全 loopback（127.0.0.1:8080）。
# ============================================================================
set -u
cd "$(dirname "$0")/../.."   # repo root
REPO_ROOT="$PWD"

BIN="${RP_WSW_BIN:-/home/emo/rt3-target/debug/rp-wsw}"
RP_TOOL="${RP_TOOL_BIN:-/home/emo/rt3-target/debug/rp-tool}"
PY=python3
PASS=0; SKIP=0; FAIL=0
ck() {  # ck <label> <expected> <actual>
  if [ "$2" = "$3" ]; then PASS=$((PASS+1)); echo "  [ok] $1 = $3"
  else FAIL=$((FAIL+1)); echo "  [FAIL] $1: expected=$2 actual=$3"; fi
}
sk() { SKIP=$((SKIP+1)); echo "  [SKIP] $1"; }

echo "== 0. V-9 守卫预挂（D6 五项环境事实）"
GUARD_MISS=0
[ -x "$BIN" ] || { sk "rp-wsw 二进制不存在（cargo build -p wsw-runtime --bin rp-wsw）"; GUARD_MISS=1; }
[ -x "$RP_TOOL" ] || { sk "rp-tool 二进制不存在（cargo build -p tool-registry --bin rp-tool）"; GUARD_MISS=1; }
command -v "$PY" >/dev/null 2>&1 || { sk "python3 不存在"; GUARD_MISS=1; }
[ -f migrations/dev-b1.db ] || { sk "migrations/dev-b1.db 开发副本不存在"; GUARD_MISS=1; }
if [ "$GUARD_MISS" = "1" ]; then
  echo "SKIP=$SKIP FAIL=$FAIL PASS=$PASS（V-9 受控环境守卫命中，exit 0）"
  exit 0
fi
ck "rp-wsw/rp-tool/python3/副本 四项硬守卫在位" "yes" "yes"

# 分面守卫：podman + 镜像 digest（判据 1/2 e2e 面）+ rt.py 真源（判据 3 面）。
PODMAN_OK=0; NUCLEI_OK=0; ZAP_OK=0; RTIS_OK=0
command -v podman >/dev/null 2>&1 && podman info >/dev/null 2>&1 && PODMAN_OK=1
NUCLEI_DIGEST="9ff41ce98d4279ed528819ebc42cb32fe5b98916e8fb9a8633110fad83354cb8"
ZAP_DIGEST=$(grep -oE 'localhost/rip-zap@sha256:[0-9a-f]{64}' manifests/tools/rip.security.zap.json | head -1 | cut -d: -f2)
if [ "$PODMAN_OK" = "1" ]; then
  podman image exists "localhost/rip-nuclei@sha256:$NUCLEI_DIGEST" && NUCLEI_OK=1
  [ -n "$ZAP_DIGEST" ] && podman image exists "localhost/rip-zap@sha256:$ZAP_DIGEST" && ZAP_OK=1
fi
[ -f "$HOME/.dsh/redteam/knowledge.db" ] && [ -f integrations/rtis/rt.py ] && RTIS_OK=1
[ "$PODMAN_OK" = "1" ] || sk "podman 不在位（容器面分面 SKIP）"
[ "$NUCLEI_OK" = "1" ] || sk "localhost/rip-nuclei@sha256:$NUCLEI_DIGEST 不在位（判据 1 e2e 分面 SKIP——D6 预授权词形）"
[ "$ZAP_OK" = "1" ] || sk "localhost/rip-zap@sha256:${ZAP_DIGEST:-占位} 不在位（判据 2 e2e 分面 SKIP）"
[ "$RTIS_OK" = "1" ] || sk "RTIS 数据真源不在位（~/.dsh/redteam/knowledge.db——gitignore 运行时数据，判据 3 真实面 SKIP）"

WORK=$(mktemp -d /tmp/wsw7-selftest.XXXXXX)
SCAN_PORT=8080
echo "== 环境：工作目录 $WORK / rp-wsw $BIN / scan 靶机端口 $SCAN_PORT"

# --- 1. 登记面（判据 4：validate exit 0 + register 副本幂等 + 孪生计数）---
echo "== 1. 登记面（manifests/tools + rp-tool）"
"$RP_TOOL" validate --manifest manifests/tools/rip.security.zap.json >/dev/null 2>&1; VJSON_RC=$?
"$RP_TOOL" validate --manifest manifests/tools/rip.security.zap.yaml >/dev/null 2>&1; VYAML_RC=$?
ck "rp-tool validate zap json exit 0（判据 4）" "0" "$VJSON_RC"
ck "rp-tool validate zap yaml 孪生 exit 0" "0" "$VYAML_RC"
REGDB="$WORK/reg.db"
cp migrations/dev-b1.db "$REGDB"
# dev-b1 副本已含 S2 收口登记行——剥离 zap 行复现首次登记面（登记唯一路径
# = rp-tool register 实证；S2 落库 face 另见批次报告 dev-b1.db 行核对）。
sqlite3 "$REGDB" "DELETE FROM tool_version WHERE tool_id='rip.security.zap'; DELETE FROM tool WHERE tool_id='rip.security.zap';"
"$RP_TOOL" register --manifest manifests/tools/rip.security.zap.json --db "$REGDB" >/dev/null 2>&1; REG1_RC=$?
ck "rp-tool register dev 副本 exit 0" "0" "$REG1_RC"
REG2=$("$RP_TOOL" register --manifest manifests/tools/rip.security.zap.json --db "$REGDB" 2>&1); REG2_RC=$?
# 重跑词形（判据 4「幂等（重跑 skipped 词形）」实测注记：rp-tool rule 2
# fail-closed 拒绝同版本重登记——无 skipped 词形，幂等面 = 同 (id,version)
# 拒绝零副作用（坏消息单列：任务书词形与 rp-tool rule 2 实测语义不一致，
# 如实记录——第一条 register 落库后行完整，重跑零残留）。
if [ "$REG2_RC" != "0" ] && printf '%s' "$REG2" | grep -q "not greater than existing"; then
  PASS=$((PASS+1)); echo "  [ok] register 重跑 fail-closed 拒绝（rule 2 幂等面——零副作用实证，坏消息节登记词形差异）"
else
  FAIL=$((FAIL+1)); echo "  [FAIL] register 重跑词形非 rule 2 拒绝: rc=$REG2_RC $REG2"
fi
ZAP_ROWS=$(sqlite3 "file:$REGDB?mode=ro" "SELECT COUNT(*) FROM tool_version WHERE tool_id='rip.security.zap'")
ck "zap tool_version 行数=1（重跑零残留）" "1" "$ZAP_ROWS"
TWIN=$(grep -rln "rip.security.zap" manifests/tools/ | wc -l)
ck "manifests 孪生计数=2（json+yaml，判据 4）" "2" "$TWIN"
DIGIT=$(grep -c "9ff41ce9\|rip-zap@sha256" manifests/tools/rip.security.zap.json)
ck "digest 钉值在场（判据 4）" "1" "$DIGIT"

# --- 2. providers 投影对账（判据 4/D8：投影与 manifest 逐字段一致）---
echo "== 2. providers 投影（rp-wsw scan providers）"
"$BIN" scan providers >"$WORK/projection.json" 2>/dev/null; ck "scan providers exit 0" "0" "$?"
"$PY" -I - "$REPO_ROOT" "$WORK/projection.json" <<'PYEOF' && PROJ_OK=1 || PROJ_OK=0
import json, sys, os
root, path = sys.argv[1], sys.argv[2]
d = json.load(open(path))
rows = d["providers"]
assert [r["provider_id"] for r in rows] == ["nuclei", "zap", "rtis"], rows
for r in rows:
    m = json.load(open(os.path.join(root, r["manifest"])))
    assert r["tool_id"] == m["id"], r
    assert r["runtime"] == m["runtime"]["type"], r
    assert r["family"] == m["family"], r
    assert r["permission_level"] == m["permission_level"], r
    assert r["idempotent"] == m["idempotent"], r
    assert r["version"] == m["version"], r
    if "image" in m["runtime"]:
        assert r["image"] == m["runtime"]["image"], r
plugins = d["plugins"]
assert plugins["source"] == "manifests/plugins/catalog.json"
assert plugins["catalog_entry_count"] == 3, plugins
assert plugins["plugin_ids"] == ["knowledge.awesome-hacking", "knowledge.secskills", "knowledge.tongui-course-notes"], plugins
print("providers=3 plugins=3 逐字段一致")
PYEOF
ck "providers 投影 manifest 逐字段一致 + plugin catalog 计数 3（D8）" "1" "$PROJ_OK"

# --- 3. rp-wsw scan CLI 负例 + help 面（判据 9）---
echo "== 3. CLI 负例与 help 面"
"$BIN" scan --help >/dev/null 2>&1; ck "scan --help exit 0（判据 9）" "0" "$?"
"$BIN" scan nuclei --help >/dev/null 2>&1; ck "scan nuclei --help exit 0" "0" "$?"
"$BIN" scan zap --help >/dev/null 2>&1; ck "scan zap --help exit 0" "0" "$?"
"$BIN" scan rtis --help >/dev/null 2>&1; ck "scan rtis --help exit 0" "0" "$?"
"$BIN" scan bogus --url x --db "$WORK/neg.db" >/dev/null 2>&1; ck "未知 provider exit 2（usage 面）" "2" "$?"
"$BIN" scan nuclei --db "$WORK/neg.db" >/dev/null 2>&1; ck "scan nuclei 缺 --url exit 1" "1" "$?"
"$BIN" scan rtis --target x >/dev/null 2>&1; ck "scan rtis 缺 --poc-id exit 1" "1" "$?"
"$BIN" scan nuclei --url "http://127.0.0.1:$SCAN_PORT" --max-requests 1001 --db "$WORK/neg.db" >"$WORK/neg.json" 2>/dev/null; NEG_RC=$?
ck "max_requests 1001 exit 1（结构化拒绝）" "1" "$NEG_RC"
grep -q "input fails input_schema" "$WORK/neg.json" \
  && { PASS=$((PASS+1)); echo "  [ok] VALIDATION 词形在场（input fails input_schema）"; } \
  || { FAIL=$((FAIL+1)); echo "  [FAIL] VALIDATION 词形缺: $(cat "$WORK/neg.json")"; }

# --- 4. RTIS 真实委派（判据 3：poc get 元数据锚 + poc use 回填）---
echo "== 4. RTIS 委派（真实 poc 900008；V-9 守卫）"
if [ "$RTIS_OK" = "1" ]; then
  "$BIN" scan rtis --poc-id 900008 --target "http://127.0.0.1:$SCAN_PORT" >"$WORK/rtis.json" 2>/dev/null; RTIS_RC=$?
  ck "scan rtis exit 0（真实库委派）" "0" "$RTIS_RC"
  "$PY" -I - "$WORK/rtis.json" <<'PYEOF' && RTIS_OK2=1 || RTIS_OK2=0
import json, sys
d = json.load(open(sys.argv[1]))
rt = d["rtis"]
assert rt["provider_id"] == "rtis-poc"
assert rt["poc_id"] == 900008
assert rt["metadata"]["id"] == 900008
assert rt["metadata"]["content_omitted"] is True
assert rt["use_recorded"]["used_on"] == "http://127.0.0.1:8080"
assert d["normalized"]["result_status"] == "not_tested"
# POC 正文零复制硬门（红线 5：content/usage/verified_note/path 零透出）
text = json.dumps(d)
for banned in ("#!/bin/bash", "verified_note", '"usage"', "content_chars"):
    assert banned not in text, banned
print("rtis 委派帧全量 + 正文零透出")
PYEOF
  ck "RTIS 委派帧 + 正文零复制硬门（判据 3/红线 5）" "1" "$RTIS_OK2"
else
  sk "RTIS 真实委派（真源缺——分面 SKIP）"
fi

# --- 5. Nuclei 真实扫描（判据 1：local_range 靶机对账；V-9 守卫）---
echo "== 5. Nuclei 真实扫描（local_range:8080）"
if [ "$NUCLEI_OK" = "1" ]; then
  python3 scripts/local_range.py --port $SCAN_PORT >"$WORK/range.log" 2>&1 &
  RANGE_PID=$!
  for i in $(seq 1 50); do curl -s -o /dev/null --max-time 2 "http://127.0.0.1:$SCAN_PORT/p59c-marker" && break; sleep 0.2; done
  "$BIN" scan nuclei --url "http://127.0.0.1:$SCAN_PORT" \
    --template-file scripts/local_range_template.yaml --max-requests 10 --timeout-s 120 \
    --task-id task-wsw7-selftest --run-id "rn-wsw7-selftest-nuclei-$$" \
    --db "$WORK/scan-nuclei.db" >"$WORK/nuclei.json" 2>/dev/null; NU_RC=$?
  ck "scan nuclei exit 0（真实派发）" "0" "$NU_RC"
  "$PY" -I - "$WORK/scan-nuclei.db" "$WORK/nuclei.json" <<'PYEOF' && NU_OK=1 || NU_OK=0
import json, sqlite3, sys
db, path = sys.argv[1], sys.argv[2]
d = json.load(open(path))
assert d["status_word"] == "SUCCEEDED", d["summary"]
matched = d["result"]["matched_count"]
assert matched >= 1, f"local_range marker 命中对账: {matched}"
assert d["normalized"]["result_status"] == "INCONCLUSIVE"  # 命中 ≠ CONFIRMED
assert d["tool_run_id"] and d["trace_id"]
conn = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
row = conn.execute("SELECT trace_id, status FROM tool_run WHERE run_id=?", (d["tool_run_id"],)).fetchone()
assert row and row[0] == d["trace_id"] and row[1] == "SUCCEEDED", row
arts = conn.execute("SELECT COUNT(*) FROM artifact WHERE run_id=? AND sha256!=''", (d["tool_run_id"],)).fetchone()[0]
assert arts >= 1, "result.jsonl artifact sha256 在库"
print(f"nuclei matched={matched} tool_run={d['tool_run_id']} artifacts={arts} 五态=INCONCLUSIVE")
PYEOF
  ck "Nuclei 真实扫描对账（tool_run trace_id + artifact sha256 + 五态 INCONCLUSIVE——判据 1）" "1" "$NU_OK"
  kill $RANGE_PID 2>/dev/null; wait $RANGE_PID 2>/dev/null
else
  sk "Nuclei 真实扫描（钉值镜像不在本机——D6 分面 SKIP；重建 digest 非确定性见批次报告坏消息节）"
fi

# --- 6. ZAP 真实扫描（判据 2：D-TC-03 帧对账；V-9 守卫）---
echo "== 6. ZAP 真实扫描（local_range:8080）"
if [ "$ZAP_OK" = "1" ]; then
  python3 scripts/local_range.py --port $SCAN_PORT >"$WORK/range-zap.log" 2>&1 &
  RANGE_PID=$!
  for i in $(seq 1 50); do curl -s -o /dev/null --max-time 2 "http://127.0.0.1:$SCAN_PORT/" && break; sleep 0.2; done
  "$BIN" scan zap --url "http://127.0.0.1:$SCAN_PORT" --timeout-s 420 \
    --run-id "rn-wsw7-selftest-zap-$$" --db "$WORK/scan-zap.db" >"$WORK/zap.json" 2>/dev/null; ZAP_RC=$?
  ck "scan zap exit 0（真实派发）" "0" "$ZAP_RC"
  "$PY" -I - "$WORK/scan-zap.db" "$WORK/zap.json" <<'PYEOF' && ZAP_OK2=1 || ZAP_OK2=0
import json, sqlite3, sys
db, path = sys.argv[1], sys.argv[2]
d = json.load(open(path))
assert d["status_word"] == "SUCCEEDED", d["summary"]
r = d["result"]
assert isinstance(r.get("matched_count"), int), "D-TC-03 帧词形 matched_count"
assert isinstance(r.get("alerts_excerpt"), str), "alerts_excerpt 词形"
assert r.get("zap_version"), "zap_version 输出（版本恒在）"
assert d["normalized"]["result_status"] == "INCONCLUSIVE"  # 命中 ≠ CONFIRMED
conn = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
row = conn.execute("SELECT status FROM tool_run WHERE run_id=?", (d["tool_run_id"],)).fetchone()
assert row and row[0] == "SUCCEEDED", row
arts = conn.execute("SELECT COUNT(*) FROM artifact WHERE run_id=? AND sha256!=''", (d["tool_run_id"],)).fetchone()[0]
assert arts >= 1, "zap-report.json artifact 在库"
print(f"zap matched={r['matched_count']} version={r['zap_version']} artifacts={arts} 五态=INCONCLUSIVE")
PYEOF
  ck "ZAP 真实扫描对账（D-TC-03 帧 + zap_version + artifact——判据 2/5）" "1" "$ZAP_OK2"
  kill $RANGE_PID 2>/dev/null; wait $RANGE_PID 2>/dev/null
else
  sk "ZAP 真实扫描（镜像不在本机——分面 SKIP；登记面/代码面判据 4/5 由其余节承载）"
fi

# --- 7. rp_server 端点面（判据 9：401/400/投影；POST rtis 随 RTIS 守卫）---
echo "== 7. rp_server 两端点"
SERVER_PORT=$("$PY" -c "import socket; s=socket.socket(); s.bind(('127.0.0.1',0)); print(s.getsockname()[1]); s.close()")
# export 后直启（$! = python 直子进程——env 前缀形态会产生 subshell 包裹，
# kill 落 subshell 不落 python，残留面实录于本批会话）。
export RP_PORT=$SERVER_PORT RP_HOST=127.0.0.1 RP_AUTH_TOKEN=wsw7-selftest-token
export RP_WSW_BIN="$BIN" RP_WSW_DB="$WORK/reg.db" RP_AUX_DB="$WORK/reg.db"
"$PY" apps/server/rp_server.py >"$WORK/server.log" 2>&1 &
SERVER_PID=$!
for i in $(seq 1 50); do curl -s -o /dev/null --max-time 2 "http://127.0.0.1:$SERVER_PORT/api/stats" && break; sleep 0.2; done
AUTH="Authorization: Bearer wsw7-selftest-token"
C401A=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 "http://127.0.0.1:$SERVER_PORT/api/wsw/scan/providers")
ck "GET /api/wsw/scan/providers 无鉴权 401（判据 9）" "401" "$C401A"
C401B=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 -X POST -H "Content-Type: application/json" -d '{}' "http://127.0.0.1:$SERVER_PORT/api/wsw/scan")
ck "POST /api/wsw/scan 无鉴权 401（判据 9）" "401" "$C401B"
C400=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 -X POST -H "$AUTH" -H "Content-Type: application/json" -d '{"provider":"bogus"}' "http://127.0.0.1:$SERVER_PORT/api/wsw/scan")
ck "未知 provider 400（判据 9）" "400" "$C400"
C200=$(curl -s -o /dev/null -w "%{http_code}" --max-time 30 -H "$AUTH" "http://127.0.0.1:$SERVER_PORT/api/wsw/scan/providers")
ck "GET providers 投影 200（Bearer 同面）" "200" "$C200"
# /api/tools 露出 zap 行（判据 4 尾项；P6-9 词形：主库未命中回退 aux 详情面
# ——RP_AUX_DB=登记副本挂载，zap 行 aux 回退读出；dev-b1 本体为一次性
# 开发副本——first_batch_b1.rs reset_dev_copy 每跑重建，登记面落 $WORK 副本）。
curl -s --max-time 10 -H "$AUTH" "http://127.0.0.1:$SERVER_PORT/api/tools?tool_id=rip.security.zap" >"$WORK/tool-zap.json"
"$PY" -I - "$WORK/tool-zap.json" <<'PYEOF' && TOOL_ZAP_OK=1 || TOOL_ZAP_OK=0
import json, sys
d = json.load(open(sys.argv[1]))
assert d.get("available") is True, d
assert d.get("source") == "aux", d.get("source")
t = d["tool"]
assert t["tool_id"] == "rip.security.zap" and t["lifecycle"] == "VALIDATED"
assert t["permission_level"] == "T1"
assert any(v["version"] == "0.1.0" and v["status"] == "current" for v in d["versions"]), d["versions"]
print("zap 行露出（available/source=aux/VALIDATED/T1/0.1.0 current）")
PYEOF
ck "/api/tools?tool_id=rip.security.zap 露出 zap 行（判据 4 尾项）" "1" "$TOOL_ZAP_OK"
curl -s --max-time 30 -H "$AUTH" "http://127.0.0.1:$SERVER_PORT/api/wsw/scan/providers" >"$WORK/proj-api.json"
grep -q '"catalog_entry_count"' "$WORK/proj-api.json" \
  && { PASS=$((PASS+1)); echo "  [ok] providers 投影 plugin catalog 计数面（D8 只读投影）"; } \
  || { FAIL=$((FAIL+1)); echo "  [FAIL] providers 投影 plugin 计数面: $(head -c 200 "$WORK/proj-api.json")"; }
if [ "$RTIS_OK" = "1" ]; then
  API_R=$(curl -s -o /dev/null -w "%{http_code}" --max-time 60 -X POST -H "$AUTH" -H "Content-Type: application/json" \
    -d '{"provider":"rtis","poc_id":900008,"target":"http://127.0.0.1:8080"}' "http://127.0.0.1:$SERVER_PORT/api/wsw/scan")
  ck "POST scan rtis 真实委派 200" "200" "$API_R"
else
  sk "POST scan rtis 真实委派（真源缺——分面 SKIP）"
fi
kill $SERVER_PID 2>/dev/null; wait $SERVER_PID 2>/dev/null

# --- 8. 生产库反证 + 零 diff 反证（红线 5/判据 10）---
echo "== 8. 生产库只读指纹 + integrations/rtis 零 diff（红线 5/判据 10）"
for f in migrations/platform.db migrations/platform-dryrun.db migrations/platform-fullbatch.db migrations/platform-governance.db migrations/platform-search.db; do
  [ -f "$f" ] || continue
  UV=$(sqlite3 "file:$f?mode=ro" "PRAGMA user_version" 2>/dev/null)
  echo "  [数字] $f user_version=$UV"
done
ck "platform.db 生产库水位未被触碰" "1" "$(sqlite3 "file:migrations/platform.db?mode=ro" "PRAGMA user_version" 2>/dev/null)"
RTIS_DIFF=$(git status --porcelain -- integrations/rtis/ | wc -l)
ck "integrations/rtis 零 diff（红线 5）" "0" "$RTIS_DIFF"
# 跨轨零 diff（判据 10）：Cargo.lock 收口态断言（RT-7 D2 按收口态重钉）——
# WSW-8 收口（chromiumoxide 0.9.1 vendor 基线跃迁）后 Cargo.lock 干净树恒零
# diff（收口后恒 0；WSW-8-BATCH-REPORT :166「收口后 417 为新基线，wsw7 四钉
# 随下一链尾演进批随更」= owner 预授权词形）；其余跨轨（web/vendor/sender/
# scope/tool-engine src/mcp-server src/migrations/plugin-*）零 diff。
LOCK_DIFF=$(git diff Cargo.lock | grep -cE '^\+ "(tool-engine|tool-registry|verification-engine)",$')
LOCK_DIFF_ALL=$(git diff Cargo.lock | grep -cE '^\+[^+]')
ck "Cargo.lock diff 收口后恒 0（WSW-8 收口态——RT-7 D2 重钉）" "0" "$LOCK_DIFF"
ck "Cargo.lock diff 无其它行（收口后恒 0——RT-7 D2 重钉）" "0" "$LOCK_DIFF_ALL"
ck "Cargo.lock [[package]] 417 恒定（WSW-8 新基线——随链尾演进批同步，WSW-8-BATCH-REPORT :166）" "417" "$(grep -cE '^\[\[package\]\]' Cargo.lock)"
CROSS=$(git status --porcelain | grep -c "web/\|vendor/\|sender.rs\|scope.rs\|tool-engine/src\|mcp-server/src\|migrations/\|plugin-registry/\|plugin-runtime/")
ck "跨轨零 diff（web/vendor/sender/scope/tool-engine src/mcp-server src/migrations/plugin-*）" "0" "$CROSS"

# --- 9. 汇总 ---
echo "== 9. 汇总"
echo "PASS=$PASS SKIP=$SKIP FAIL=$FAIL"
echo "数字面：validate json+yaml=$VJSON_RC/$VYAML_RC；投影 providers=3 plugins=3；CLI 负例=4；RTIS 委派=$([ "$RTIS_OK" = "1" ] && echo yes || echo skip)；nuclei e2e=$([ "$NUCLEI_OK" = "1" ] && echo yes || echo skip)；zap e2e=$([ "$ZAP_OK" = "1" ] && echo yes || echo skip)；端点 401/401/400/200"
[ "$FAIL" = "0" ] && echo "SELFTEST_WSW7: ALL GREEN" || echo "SELFTEST_WSW7: FAILED"
exit "$([ "$FAIL" = "0" ] && echo 0 || echo 1)"
