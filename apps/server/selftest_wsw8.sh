#!/usr/bin/env bash
# WSW-8 收口脚本（docs/rip3/wsw/WSW-8-TASK.md S8/D6）：
# V-9 五项守卫 + Chromium 探针链分面 + 真实 capture PASS/SKIP 分开计数 +
# reqwest 双检 + lock 基线钉 417 + vendor 完整性 + deny 双检 + 生产库只读
# 指纹 + 跨轨零 diff 反证。既有 selftest_wsw7.sh 形状先例（:23-38 守卫 /
# :279-282 指纹）。纪律：全部目标 = loopback fixture（自起，零外联）。
# 用法：RP_BROWSER_BIN=${RP_BROWSER_BIN:-} bash apps/server/selftest_wsw8.sh
set -u
cd "$(dirname "$0")/../.."   # repo root
REPO_ROOT="$PWD"

BIN="${RP_WSW_BIN:-/home/emo/rt3-target/debug/rp-wsw}"
BROWSER_BIN="${RP_WSW_BROWSER_BIN:-/home/emo/rt3-target/debug/rp-wsw-browser}"
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
[ -x "$BROWSER_BIN" ] || { sk "rp-wsw-browser 二进制不存在（cargo build -p wsw-runtime --bin rp-wsw-browser）"; GUARD_MISS=1; }
[ -x "$RP_TOOL" ] || { sk "rp-tool 二进制不存在（cargo build -p tool-registry --bin rp-tool）"; GUARD_MISS=1; }
command -v "$PY" >/dev/null 2>&1 || { sk "python3 不存在（loopback fixture 面）"; GUARD_MISS=1; }
[ -f migrations/dev-b1.db ] || { sk "migrations/dev-b1.db 开发副本不存在"; GUARD_MISS=1; }
if [ "$GUARD_MISS" = "1" ]; then
  echo "SKIP=$SKIP FAIL=$FAIL PASS=$PASS（V-9 受控环境守卫命中，exit 0）"
  exit 0
fi
ck "rp-wsw/rp-wsw-browser/rp-tool/python3/副本 五项硬守卫在位" "yes" "yes"

# Chromium 探针链五级（D2①；RP_BROWSER_BIN/PW_CHROMIUM env → ms-playwright
# 缓存 glob → /usr/sbin/chromium → /usr/bin/chromium——任一命中即 PASS，
# 全缺 → chromium_missing SKIP 计数，e2e 真捕获面分面 SKIP）。
CHROME_OK=0
CHROME_HIT=""
if [ -n "${RP_BROWSER_BIN:-}" ] && [ -x "${RP_BROWSER_BIN}" ]; then
  CHROME_OK=1; CHROME_HIT="RP_BROWSER_BIN=$RP_BROWSER_BIN"
elif [ -n "${PW_CHROMIUM:-}" ] && [ -x "${PW_CHROMIUM}" ]; then
  CHROME_OK=1; CHROME_HIT="PW_CHROMIUM=$PW_CHROMIUM"
else
  for cand in "$HOME"/.cache/ms-playwright/chromium-*/chrome-linux*/chromium /usr/sbin/chromium /usr/bin/chromium; do
    if [ -x "$cand" ]; then CHROME_OK=1; CHROME_HIT="$cand"; break; fi
  done
fi
if [ "$CHROME_OK" = "1" ]; then
  PASS=$((PASS+1)); echo "  [ok] Chromium 探针链命中：$CHROME_HIT"
else
  sk "Chromium 探针链五级全缺（chromium_missing——真捕获面分面 SKIP，CI runner 形态）"
fi

echo "== 1. vendor 基线跃迁面（判据 11：lock 417 + reqwest 双检 + vendor 完整性）"
ck "Cargo.lock [[package]] = 417（S2 实测对账新基线钉）" "417" "$(grep -cE '^\[\[package\]\]' Cargo.lock)"
ck "lock 内 reqwest = 1 行（chromiumoxide 0.13.5 传递——基线钉）" "1" "$(grep -c 'name = "reqwest"' Cargo.lock)"
ck "workspace 直接依赖 reqwest = 0（硬门）" "0" "$(grep -rn '^reqwest' crates/*/Cargo.toml | wc -l)"
ck "lock 内 chromiumoxide = =0.9.1 钉面" "1" "$(grep -c 'name = "chromiumoxide"' Cargo.lock)"
# vendor 完整性门：cargo vendor --locked 复跑，tracked diff = 0（18 新目录
# 待显式 add 属工作树新增面，非 diff 漂移）。
VENDOR_OUT=$(cargo vendor vendor --locked >/dev/null 2>&1; echo "vendor_rc=$?")
VENDOR_TRACKED=$(git status --porcelain vendor/ | grep -c '^ M\|^M ') || VENDOR_TRACKED=0
VENDOR_UNTRACKED=$(git status --porcelain vendor/ | grep -c '^??') || VENDOR_UNTRACKED=0
ck "cargo vendor --locked 复跑 exit 0" "vendor_rc=0" "$VENDOR_OUT"
ck "vendor/ tracked 零 diff（完整性门）" "0" "$VENDOR_TRACKED"
ck "vendor/ 新增目录 = 18（16 新名 + 2 双版本目录）" "18" "$VENDOR_UNTRACKED"
ck "vendor 目录计数 = 392（374+18）" "392" "$(ls vendor | wc -l)"
ck ".cargo/config.toml 零 diff" "0" "$(git status --porcelain .cargo/ | wc -l)"

echo "== 2. deny 双检（D1④：deny.toml 零改动 + 许可/通告双绿）"
cargo deny check licenses >/dev/null 2>&1; LIC_RC=$?
cargo deny check advisories >/dev/null 2>&1; ADV_RC=$?
ck "cargo deny check licenses exit 0" "0" "$LIC_RC"
ck "cargo deny check advisories exit 0" "0" "$ADV_RC"
ck "deny.toml 零 diff（16 新包许可全在 allow 集）" "0" "$(git status --porcelain deny.toml | wc -l)"

# --- 3. 登记面（判据 8：validate + register 幂等 + 孪生 + digest 实测一致）---
echo "== 3. 登记面（rip.wsw.browser）"
"$RP_TOOL" validate --manifest manifests/tools/rip.wsw.browser.json >/dev/null 2>&1; VJSON_RC=$?
"$RP_TOOL" validate --manifest manifests/tools/rip.wsw.browser.yaml >/dev/null 2>&1; VYAML_RC=$?
ck "rp-tool validate json exit 0（判据 8）" "0" "$VJSON_RC"
ck "rp-tool validate yaml 孪生 exit 0" "0" "$VYAML_RC"
REGDB=$(mktemp -u /tmp/wsw8-reg.XXXXXX.db)
cp migrations/dev-b1.db "$REGDB"
sqlite3 "$REGDB" "DELETE FROM tool_version WHERE tool_id='rip.wsw.browser'; DELETE FROM tool WHERE tool_id='rip.wsw.browser';"
"$RP_TOOL" register --manifest manifests/tools/rip.wsw.browser.json --db "$REGDB" >/dev/null 2>&1; REG1_RC=$?
ck "rp-tool register 剥离行副本 exit 0" "0" "$REG1_RC"
REG2=$("$RP_TOOL" register --manifest manifests/tools/rip.wsw.browser.json --db "$REGDB" 2>&1); REG2_RC=$?
if [ "$REG2_RC" != "0" ] && printf '%s' "$REG2" | grep -q "not greater than existing"; then
  PASS=$((PASS+1)); echo "  [ok] register 重跑 rule 2 fail-closed 拒绝（幂等面——零副作用）"
else
  FAIL=$((FAIL+1)); echo "  [FAIL] register 重跑词形非 rule 2 拒绝: rc=$REG2_RC $REG2"
fi
TWIN=$(grep -rln "rip.wsw.browser" manifests/tools/ | wc -l)
ck "manifests 孪生计数=2（json+yaml，判据 8）" "2" "$TWIN"
# binary_sha256 与本地 release 构建实测一致（构建产物钉值先例——同 commit 纪律）。
RELEASE_BIN="${CARGO_TARGET_DIR:-target}/release/rp-wsw-browser"
[ "${CARGO_TARGET_DIR:-}" = "" ] && RELEASE_BIN="/home/emo/rt3-target/release/rp-wsw-browser"
if [ -x "$RELEASE_BIN" ]; then
  ACTUAL_SHA=$(sha256sum "$RELEASE_BIN" | cut -d' ' -f1)
  PINNED_SHA=$(grep -o '"binary_sha256": "[0-9a-f]*"' manifests/tools/rip.wsw.browser.json | cut -d'"' -f4)
  ck "binary_sha256 manifest = release 实测（判据 8）" "$ACTUAL_SHA" "$PINNED_SHA"
else
  sk "release rp-wsw-browser 不在位（sha 对账分面 SKIP——cargo build --release 后复跑）"
fi
rm -f "$REGDB"

# --- 4. 真实 capture（判据 1/2/3：loopback fixture + 真实 Chromium）---
echo "== 4. 真实 capture（loopback fixture；PASS/SKIP 分开计数）"
if [ "$CHROME_OK" = "1" ]; then
  WORK=$(mktemp -d /tmp/wsw8-selftest.XXXXXX)
  mkdir -p "$WORK/www"
  cat > "$WORK/www/fixture.html" <<EOF
<!DOCTYPE html><html><head><title>WSW8 selftest Fixture</title></head>
<body><h1 id="marker">wsw8-selftest-marker-4e7c</h1><p>loopback fixture body text</p>
<img src="pixel.png"><script src="app.js"></script></body></html>
EOF
  echo "x" > "$WORK/www/pixel.png"; echo "console.log(1);" > "$WORK/www/app.js"
  FIX_PORT=0
  "$PY" -I - "$WORK/www" <<'PYEOF' > "$WORK/port.txt" &
import http.server, socketserver, functools, sys, os
os.chdir(sys.argv[1])
with socketserver.TCPServer(("127.0.0.1", 0), functools.partial(http.server.SimpleHTTPRequestHandler)) as httpd:
    print(httpd.server_address[1], flush=True)
    httpd.serve_forever()
PYEOF
  SERVER_PID=$!
  sleep 1
  FIX_PORT=$(cat "$WORK/port.txt" | tr -d '[:space:]')
  CAPDB="$WORK/capture.db"
  CAP=$("$BIN" browser capture --db "$CAPDB" --url "http://127.0.0.1:$FIX_PORT/fixture.html" \
        --mode viewport --settle-ms 500 --task-id task-selftest-1 --trace-id tr-wsw8-selftest-1 2>/dev/null)
  CAP_RC=$?
  ck "rp-wsw browser capture exit 0（判据 1）" "0" "$CAP_RC"
  "$PY" -I - "$CAPDB" "$WORK" <<'PYEOF' && CAP_OK=1 || CAP_OK=0
import json, sqlite3, hashlib, os, sys
db, work = sys.argv[1], sys.argv[2]
conn = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
row = conn.execute("SELECT run_id, status, task_id FROM tool_run WHERE trace_id='tr-wsw8-selftest-1'").fetchone()
assert row is not None, "tool_run 行缺"
run_id, status, task_id = row
assert status == "SUCCEEDED", status
art = conn.execute("SELECT uri, sha256, size_bytes, trace_id FROM artifact WHERE run_id=? AND uri LIKE '%/out/screenshot.png'", (run_id,)).fetchone()
assert art is not None, "screenshot artifact 行缺"
uri, sha, size, art_trace = art
path = uri[len("file://"):]
blob = open(path, "rb").read()
assert blob[:8] == b"\x89PNG\r\n\x1a\n", "PNG magic 89504e47"
assert hashlib.sha256(blob).hexdigest() == sha, "sha256 与文件实测一致"
assert len(blob) == size, "size_bytes 与 stat 一致"
ev = conn.execute("SELECT evidence_type, content_hash, storage_ref, source_pk, task_ref, trace_id, lifecycle FROM evidence WHERE source_pk=?", (run_id,)).fetchone()
assert ev is not None, "evidence 行缺"
etype, ehash, eref, epk, etask, etrace, lifecycle = ev
assert etype == "screenshot", etype
assert ehash == sha and eref == uri and epk == run_id and etask == task_id and etrace == art_trace, ev
assert lifecycle == "RAW", lifecycle
out_dir = uri[len("file://"):].rsplit("/", 1)[0]
dom = json.load(open(out_dir + "/dom.json"))
assert "strings" in dom and "documents" in dom, sorted(dom.keys())
assert any("wsw8-selftest-marker-4e7c" in s for s in dom["strings"]), "strings marker 缺"
page = open(out_dir + "/page.txt").read()
assert "wsw8-selftest-marker-4e7c" in page and "loopback fixture body text" in page, "page.txt 投影缺"
net = json.load(open(out_dir + "/network.json"))
assert len(net["requests"]) >= 3, net["requests"]
meta = json.load(open(out_dir + "/meta.json"))
for k in ["current_url","final_url","title","captured_at","viewport","dpr","browser_version","load_status","trace_id"]:
    assert k in meta, k
view = conn.execute("SELECT DISTINCT entity_type FROM v_trace_chain WHERE trace_id='tr-wsw8-selftest-1'").fetchall()
kinds = {v[0] for v in view}
assert {"tool_run", "artifact", "evidence"} <= kinds, kinds
print(f"capture 实拍对账：run={run_id} png={size}B strings={len(dom['strings'])} net_rows={len(net['requests'])} 六ID 同链")
PYEOF
  ck "真实 capture 全链对账（PNG magic/sha256/dom strings/六 ID/015 视图）" "1" "$CAP_OK"
  # 负例：file:// scheme 词表拒绝（exit 1 + 词形回传）。
  BAD=$("$BIN" browser capture --db "$CAPDB" --url "file:///etc/passwd" 2>&1); BAD_RC=$?
  if [ "$BAD_RC" != "0" ] && printf '%s' "$BAD" | grep -q "scheme 词表外"; then
    PASS=$((PASS+1)); echo "  [ok] file:// scheme 词表拒绝负例（判据 7）"
  else
    FAIL=$((FAIL+1)); echo "  [FAIL] file:// 负例词形: rc=$BAD_RC $BAD"
  fi
  # 负例：scope 元数据 host 拒绝（deny-never-allow）。
  SCOPE=$("$BIN" browser capture --db "$CAPDB" --url "http://169.254.169.254/latest/meta-data/" \
          --scope-rules-json '{"include":["example.com"]}' 2>&1); SCOPE_RC=$?
  if [ "$SCOPE_RC" != "0" ] && printf '%s' "$SCOPE" | grep -q "metadata_endpoint"; then
    PASS=$((PASS+1)); echo "  [ok] scope 元数据 host 拒绝负例（判据 7）"
  else
    FAIL=$((FAIL+1)); echo "  [FAIL] scope 负例词形: rc=$SCOPE_RC $SCOPE"
  fi
  kill $SERVER_PID 2>/dev/null; wait $SERVER_PID 2>/dev/null
  rm -rf "$WORK"
else
  sk "真实 capture 全链对账（chromium_missing 分面 SKIP）"
  sk "file:// scheme 负例（chromium_missing 分面 SKIP——词表拒绝面在 e2e 测试承载）"
  sk "scope 元数据负例（chromium_missing 分面 SKIP——闸面在 e2e 测试承载）"
fi

# --- 5. 生产库只读指纹 + 跨轨零 diff 反证（红线 5/判据 11）---
echo "== 5. 生产库只读指纹 + 跨轨零 diff（红线 5/判据 11）"
for f in migrations/platform.db migrations/platform-dryrun.db migrations/platform-fullbatch.db migrations/platform-governance.db migrations/platform-search.db; do
  [ -f "$f" ] || continue
  UV=$(sqlite3 "file:$f?mode=ro" "PRAGMA user_version" 2>/dev/null)
  echo "  [数字] $f user_version=$UV"
done
ck "platform.db 生产库水位未被触碰" "1" "$(sqlite3 "file:migrations/platform.db?mode=ro" "PRAGMA user_version" 2>/dev/null)"
RTIS_DIFF=$(git status --porcelain -- integrations/rtis/ | wc -l)
ck "integrations/rtis 零 diff（红线 5）" "0" "$RTIS_DIFF"
CROSS=$(git status --porcelain | grep -c "web/\|sender.rs\|scope.rs\|listener\|persist\|fuzz/\|tool-engine/src\|mcp-server/src\|migrations/\|integrations/rtis\|deny.toml\|.cargo/config")
ck "跨轨零 diff（web/sender/scope/listener/persist/fuzz/tool-engine src/mcp-server src/migrations/rtis/deny.toml/.cargo）" "0" "$CROSS"
ck "migrations 零 diff（D5 零迁移）" "0" "$(git status --porcelain migrations/ | grep -v '^?? dev-b' | wc -l)"

echo "== 6. 汇总"
echo "PASS=$PASS SKIP=$SKIP FAIL=$FAIL"
[ "$FAIL" = "0" ] && exit 0 || exit 1
