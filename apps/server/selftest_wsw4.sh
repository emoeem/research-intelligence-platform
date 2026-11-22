#!/usr/bin/env bash
# ============================================================================
# selftest_wsw4.sh — WSW-4 Web Fuzzer / Intruder 真实靶机实测收口
#   （WSW-4-TASK §三 S9 + 判据 5/7/8/9/10/11 数字来源；禁模拟冒充——PLAN
#   :436/:754：全部数字来自命令回显）
#
# 靶机 = python3 标准库 http.server 真实 HTTP 服务（宿主零安装，127.0.0.1
# 随机端口；REDLINES 红线 8）；发送面 = rp-wsw fuzz 子进程（Rust 引擎，
# 发送单点 sender::send_request）；History 闭环经 rp_server 真实 HTTP 链路
# curl 断言（判据 10）。
#
# 纪律：临时工作目录（退出不清理，留对账）；platform*.db 零触碰；web/** 零触碰。
# ============================================================================
set -u
cd "$(dirname "$0")/../.."   # repo root
REPO_ROOT="$PWD"

BIN="${RP_WSW_BIN:-/home/emo/rt3-target/debug/rp-wsw}"
PY=python3
PASS=0; FAIL=0
ck() {  # ck <label> <expected> <actual>
  if [ "$2" = "$3" ]; then PASS=$((PASS+1)); echo "  [ok] $1 = $3"
  else FAIL=$((FAIL+1)); echo "  [FAIL] $1: expected=$2 actual=$3"; fi
}

WORK=$(mktemp -d /tmp/wsw4-selftest.XXXXXX)
export RP_ARTIFACTS_DIR="$WORK/artifacts"
mkdir -p "$WORK/state" "$RP_ARTIFACTS_DIR"
export RP_WSW_STATE_DIR="$WORK/state"
WSW_STATE_DIR="$WORK/state"
DB="$WORK/wsw4.db"
echo "== 0. 环境：工作目录 $WORK / 二进制 $BIN"
[ -x "$BIN" ] || { echo "FATAL: rp-wsw 二进制不存在（cargo build -p wsw-runtime --bin rp-wsw）"; exit 1; }

# --- 迁移到 028 ---
cp migrations/dev-b1.db "$DB"
"$BIN" db migrate --db "$DB" >/dev/null 2>&1
UV=$(sqlite3 "file:$DB?mode=ro" "PRAGMA user_version")
ck "迁移水位 user_version=28" "28" "$UV"

# --- 真实 HTTP 靶机（python3 标准库；随机端口；请求计数器）---
cat > "$WORK/target.py" <<'EOF'
import http.server, socketserver, sys, threading, urllib.parse
HITS = {"n": 0}
LOCK = threading.Lock()
class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        p = urllib.parse.urlparse(self.path).path
        if p != "/hits":
            with LOCK:
                HITS["n"] += 1
        q = urllib.parse.parse_qs(urllib.parse.urlparse(self.path).query)
        if p == "/hits":
            body = str(HITS["n"]).encode()
            self.send_response(200); self.send_header("Content-Type","text/plain")
            self.send_header("Content-Length", str(len(body))); self.end_headers()
            self.wfile.write(body); return
        if p == "/limited":
            self.send_response(429); self.send_header("Content-Length","0"); self.end_headers(); return
        if p == "/slow":
            import time; time.sleep(3)
            body = b"slow-done"
            self.send_response(200); self.send_header("Content-Length", str(len(body))); self.end_headers()
            self.wfile.write(body); return
        # /fuzz?q=<v> → 200 echo v（marker 命中回读面）
        v = (q.get("q") or ["noq"])[0]
        body = v.encode()
        self.send_response(200); self.send_header("Content-Type","text/plain")
        self.send_header("Content-Length", str(len(body))); self.end_headers()
        self.wfile.write(body)
class S(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
srv = S(("127.0.0.1", 0), H)
with open(sys.argv[1], "w") as f:
    f.write(str(srv.server_address[1]))
srv.serve_forever()
EOF
"$PY" "$WORK/target.py" "$WORK/target.port" &
TARGET_PID=$!
for i in $(seq 1 50); do [ -s "$WORK/target.port" ] && break; sleep 0.1; done
PORT=$(cat "$WORK/target.port")
HITS_URL="http://127.0.0.1:$PORT/hits"
ck "靶机起在 127.0.0.1 随机端口" "yes" "$([ -n "$PORT" ] && echo yes)"
curl -s --max-time 3 "$HITS_URL" >/dev/null && ck "靶机可达（loopback）" "0" "$?"

# --- 会话行（scoped / deny 两面）---
NOW=$(date -u +%Y-%m-%dT%H:%M:%S.000Z)
sqlite3 "$DB" <<EOF
INSERT INTO wsw_session (session_id, title, scope_rules_json, out_of_scope_policy, created_at, updated_at)
VALUES ('wsw-ses-w4a', 'w4 scoped', '{"include":["127.0.0.1"],"allowed_ports":[$PORT],"allow_private_targets":true,"max_redirects":5}', 'blocked', '$NOW', '$NOW');
INSERT INTO wsw_session (session_id, title, scope_rules_json, out_of_scope_policy, created_at, updated_at)
VALUES ('wsw-ses-w4deny', 'w4 default deny', '{"include":[],"allow_private_targets":false}', 'blocked', '$NOW', '$NOW');
EOF

echo "== 1. Job A：marker echo 全链（§…§ 标记 + cartesian，6 payload）"
"$PY" - "$WORK/spec-a.json" "$PORT" <<'EOF'
import json, sys
port = sys.argv[2]
spec = {
  "request": {"method": "GET", "target": "/fuzz?q=§P§", "version": "http/1.1",
              "headers": [{"name": "Host", "value": f"127.0.0.1:{port}"},
                          {"name": "User-Agent", "value": "wsw4-selftest"}],
              "body": "", "trailers": [], "framing": "content_length"},
  "mode": "cartesian",
  "payloads": [{"type": "list", "values": ["alpha","bravo","charlie","delta","echo","foxtrot"]}],
  "budget": {"max_requests": 10, "max_concurrency": 2}
}
open(sys.argv[1], "w").write(json.dumps(spec))
EOF
HITS_A0=$(curl -s --max-time 3 "$HITS_URL")
A_OUT=$("$BIN" fuzz --db "$DB" --session-id wsw-ses-w4a --spec-json "$WORK/spec-a.json" 2>"$WORK/a.err")
T1=$(date +%s%N)
HITS_A1=$(curl -s --max-time 3 "$HITS_URL")
A_STATE=$(echo "$A_OUT" | "$PY" -c "import json,sys; print(json.load(sys.stdin)['state'])")
A_EST=$(echo "$A_OUT" | "$PY" -c "import json,sys; print(json.load(sys.stdin)['estimated_requests'])")
A_DISP=$(echo "$A_OUT" | "$PY" -c "import json,sys; print(json.load(sys.stdin)['dispatched'])")
A_RESP=$(echo "$A_OUT" | "$PY" -c "import json,sys; print(json.load(sys.stdin)['responded'])")
ck "Job A 终态 COMPLETED" "COMPLETED" "$A_STATE"
ck "Job A 估算 = 派发（判据 2）" "$A_EST" "$A_DISP"
ck "Job A 全 RESPONDED" "$A_DISP" "$A_RESP"
ck "Job A 靶机收到 6 请求（delta 口径）" "6" "$((HITS_A1 - HITS_A0))"
MARKER_MISS=0
for p in alpha bravo charlie delta echo foxtrot; do
  R=$(sqlite3 "file:$DB?mode=ro" "SELECT COUNT(*) FROM wsw_fuzz_case WHERE payload_json LIKE '%$p%' AND state='RESPONDED' AND response_sha256 IS NOT NULL")
  [ "$R" -ge 1 ] || MARKER_MISS=1
done
ck "Job A 六 payload 变体全落 RESPONDED 行" "0" "$MARKER_MISS"
EX_A=$(sqlite3 "file:$DB?mode=ro" "SELECT COUNT(*) FROM wsw_exchange WHERE source='fuzzer'")
ck "Job A exchange(source='fuzzer') 行 = 6" "6" "$EX_A"

echo "== 2. Job B：budget 截断（6 组合 max_requests=2 → BUDGET_EXHAUSTED）"
"$PY" - "$WORK/spec-b.json" "$PORT" <<'EOF'
import json, sys
port = sys.argv[2]
spec = {
  "request": {"method": "GET", "target": "/fuzz?q=§P§", "version": "http/1.1",
              "headers": [{"name": "Host", "value": f"127.0.0.1:{port}"}],
              "body": "", "trailers": [], "framing": "content_length"},
  "mode": "cartesian",
  "payloads": [{"type": "list", "values": ["b1","b2","b3","b4","b5","b6"]}],
  "budget": {"max_requests": 2, "max_concurrency": 2}
}
open(sys.argv[1], "w").write(json.dumps(spec))
EOF
B_OUT=$("$BIN" fuzz --db "$DB" --session-id wsw-ses-w4a --spec-json "$WORK/spec-b.json" 2>/dev/null)
ck "Job B 终态 BUDGET_EXHAUSTED（判据 5）" "BUDGET_EXHAUSTED" "$(echo "$B_OUT" | "$PY" -c "import json,sys; print(json.load(sys.stdin)['state'])")"
ck "Job B 派发 = 2（已发送 case 终态齐全）" "2" "$(echo "$B_OUT" | "$PY" -c "import json,sys; print(json.load(sys.stdin)['dispatched'])")"
ck "Job B RESPONDED = 2" "2" "$(echo "$B_OUT" | "$PY" -c "import json,sys; print(json.load(sys.stdin)['responded'])")"

echo "== 3. Job C：default deny 会话 → SKIPPED_SCOPE 零发送（判据 7）"
"$PY" - "$WORK/spec-c.json" "$PORT" <<'EOF'
import json, sys
port = sys.argv[2]
spec = {
  "request": {"method": "GET", "target": "/fuzz?q=§P§", "version": "http/1.1",
              "headers": [{"name": "Host", "value": f"127.0.0.1:{port}"}],
              "body": "", "trailers": [], "framing": "content_length"},
  "mode": "cartesian",
  "payloads": [{"type": "list", "values": ["c1","c2","c3"]}],
  "budget": {"max_requests": 10, "max_concurrency": 2}
}
open(sys.argv[1], "w").write(json.dumps(spec))
EOF
C_HITS0=$(curl -s --max-time 3 "$HITS_URL")
C_OUT=$("$BIN" fuzz --db "$DB" --session-id wsw-ses-w4deny --spec-json "$WORK/spec-c.json" 2>/dev/null)
ck "Job C 终态 COMPLETED（全部跳过）" "COMPLETED" "$(echo "$C_OUT" | "$PY" -c "import json,sys; print(json.load(sys.stdin)['state'])")"
ck "Job C SKIPPED_SCOPE = 3" "3" "$(echo "$C_OUT" | "$PY" -c "import json,sys; print(json.load(sys.stdin)['skipped_scope'])")"
ck "Job C 派发 = 0" "0" "$(echo "$C_OUT" | "$PY" -c "import json,sys; print(json.load(sys.stdin)['dispatched'])")"
HITS_AFTER=$(curl -s --max-time 3 "$HITS_URL")
ck "Job C 靶机请求计数不变（零发送实证，delta=0）" "$C_HITS0" "$HITS_AFTER"
C_ERR=$(sqlite3 "file:$DB?mode=ro" "SELECT DISTINCT error_code FROM wsw_fuzz_case WHERE job_id IN (SELECT job_id FROM wsw_fuzz_job WHERE session_id='wsw-ses-w4deny')")
ck "Job C error_code = scope 词形" "no_rules_default_deny" "$C_ERR"

echo "== 4. Job D：429 限流分类（判据 9）"
"$PY" - "$WORK/spec-d.json" "$PORT" <<'EOF'
import json, sys
port = sys.argv[2]
spec = {
  "request": {"method": "GET", "target": "/limited?d=§P§", "version": "http/1.1",
              "headers": [{"name": "Host", "value": f"127.0.0.1:{port}"}],
              "body": "", "trailers": [], "framing": "content_length"},
  "mode": "cartesian",
  "payloads": [{"type": "list", "values": ["x"]}],
  "budget": {"max_requests": 5, "max_concurrency": 1}
}
open(sys.argv[1], "w").write(json.dumps(spec))
EOF
D_OUT=$("$BIN" fuzz --db "$DB" --session-id wsw-ses-w4a --spec-json "$WORK/spec-d.json" 2>/dev/null)
ck "Job D 终态 COMPLETED" "COMPLETED" "$(echo "$D_OUT" | "$PY" -c "import json,sys; print(json.load(sys.stdin)['state'])")"
D_ERR=$(sqlite3 "file:$DB?mode=ro" "SELECT DISTINCT error_code FROM wsw_fuzz_case WHERE payload_json LIKE '%x%' AND response_status=429")
ck "Job D 429 → fuzz_rate_limited 词形" "fuzz_rate_limited" "$D_ERR"

echo "== 5. Job E：SIGTERM 取消（CANCELLING→CANCELLED + 取消延迟实测，判据 8/性能）"
"$PY" - "$WORK/spec-e.json" "$PORT" <<'EOF'
import json, sys
port = sys.argv[2]
spec = {
  "request": {"method": "GET", "target": "/slow?d=§P§", "version": "http/1.1",
              "headers": [{"name": "Host", "value": f"127.0.0.1:{port}"}],
              "body": "", "trailers": [], "framing": "content_length"},
  "mode": "cartesian",
  "payloads": [{"type": "list", "values": ["s1","s2","s3","s4"]}],
  "budget": {"max_requests": 10, "max_concurrency": 4,
             "min_request_interval_ms": 200}
}
open(sys.argv[1], "w").write(json.dumps(spec))
EOF
E_JID="wsw-fzj-e-$(date +%s%N)"
"$BIN" fuzz --db "$DB" --session-id wsw-ses-w4a --job-id "$E_JID" --spec-json "$WORK/spec-e.json" --pid-file "$WORK/e.pid" >"$WORK/e.out" 2>&1 &
E_PID=$!
sleep 1.2   # 等 1-2 case 在途
E_SIG0=$(date +%s%N)
kill -TERM "$E_PID" 2>/dev/null
wait "$E_PID"
E_SIG1=$(date +%s%N)
E_STATE=$(sqlite3 "file:$DB?mode=ro" "SELECT state FROM wsw_fuzz_job WHERE job_id='$E_JID'")
ck "Job E 终态 CANCELLED" "CANCELLED" "$E_STATE"
E_CANCEL_MS=$(( (E_SIG1 - E_SIG0) / 1000000 ))
echo "  [数字] 取消延迟（SIGTERM→进程退出）= ${E_CANCEL_MS} ms"
E_HANG=$(sqlite3 "file:$DB?mode=ro" "SELECT COUNT(*) FROM wsw_fuzz_case WHERE job_id='$E_JID' AND state IN ('PENDING','SENT')")
ck "Job E 取消后无悬挂 case 行" "0" "$E_HANG"

echo "== 6. Job F：SIGKILL 重启恢复防重派（判据 8）"
"$PY" - "$WORK/spec-f.json" "$PORT" <<'EOF'
import json, sys
port = sys.argv[2]
spec = {
  "request": {"method": "GET", "target": "/fuzz?q=§P§", "version": "http/1.1",
              "headers": [{"name": "Host", "value": f"127.0.0.1:{port}"}],
              "body": "", "trailers": [], "framing": "content_length"},
  "mode": "cartesian",
  "payloads": [{"type": "list", "values": ["f1","f2","f3","f4"]}],
  "budget": {"max_requests": 10, "max_concurrency": 1,
             "min_request_interval_ms": 250}
}
open(sys.argv[1], "w").write(json.dumps(spec))
EOF
F_JID="wsw-fzj-f-$(date +%s)"
"$BIN" fuzz --db "$DB" --session-id wsw-ses-w4a --job-id "$F_JID" --spec-json "$WORK/spec-f.json" >"$WORK/f1.out" 2>&1 &
F_PID=$!
# 等 /fuzz 收到 f1（首个 case 已 RESPONDED）再 SIGKILL。
for i in $(seq 1 100); do
  GOT=$(sqlite3 "file:$DB?mode=ro" "SELECT COUNT(*) FROM wsw_fuzz_case WHERE job_id='$F_JID' AND state='RESPONDED'")
  [ "$GOT" -ge 1 ] && break
  sleep 0.1
done
kill -9 "$F_PID" 2>/dev/null
wait "$F_PID" 2>/dev/null
F_KILLED_STATE=$(sqlite3 "file:$DB?mode=ro" "SELECT state FROM wsw_fuzz_job WHERE job_id='$F_JID'")
F_RESP_BEFORE=$(sqlite3 "file:$DB?mode=ro" "SELECT COUNT(*) FROM wsw_fuzz_case WHERE job_id='$F_JID' AND state='RESPONDED'")
echo "  [数字] kill 时 job 态 = $F_KILLED_STATE / 已 RESPONDED = $F_RESP_BEFORE"
"$BIN" fuzz --db "$DB" --session-id wsw-ses-w4a --job-id "$F_JID" --spec-json "$WORK/spec-f.json" >"$WORK/f2.out" 2>/dev/null
F_STATE=$(sqlite3 "file:$DB?mode=ro" "SELECT state FROM wsw_fuzz_job WHERE job_id='$F_JID'")
ck "Job F 恢复后 COMPLETED" "COMPLETED" "$F_STATE"
F_TOTAL=$(sqlite3 "file:$DB?mode=ro" "SELECT COUNT(*) FROM wsw_fuzz_case WHERE job_id='$F_JID'")
ck "Job F case 行总数 = 4（无重复行）" "4" "$F_TOTAL"
# 已终态 case 不重派：每个 payload 的 RESPONDED 行恰 1（靶机 echo 体唯一可对账）。
DUP=0
for p in f1 f2 f3 f4; do
  N=$(sqlite3 "file:$DB?mode=ro" "SELECT COUNT(*) FROM wsw_fuzz_case WHERE job_id='$F_JID' AND state='RESPONDED' AND payload_json LIKE '%$p%'")
  [ "$N" = "1" ] || DUP=$((DUP+1))
done
ck "Job F 各 payload 恰一次派发（判据 8 对账）" "0" "$DUP"

echo "== 7. 吞吐数字（Job A：dispatched / duration_ms）"
A_DUR=$(echo "$A_OUT" | "$PY" -c "import json,sys; print(json.load(sys.stdin)['duration_ms'])")
"$PY" -c "
print(f'  [数字] Job A 调度吞吐 = {$A_DISP / ($A_DUR / 1000):.1f} case/s（dispatched=$A_DISP, duration_ms=$A_DUR）')
"

echo "== 8. rp_server 四端点（真实 HTTP 链路，判据 10）"
SERVER_PORT=18844
RP_PORT=$SERVER_PORT RP_HOST=127.0.0.1 RP_AUTH_TOKEN=wsw4-selftest-token \
RP_WSW_DB="$DB" RP_WSW_BIN="$BIN" RP_WSW_STATE_DIR="$WSW_STATE_DIR" \
RP_ARTIFACTS_DIR="$RP_ARTIFACTS_DIR" \
  "$PY" apps/server/rp_server.py >"$WORK/server.log" 2>&1 &
SERVER_PID=$!
AUTH="Authorization: Bearer wsw4-selftest-token"
for i in $(seq 1 50); do curl -s -o /dev/null --max-time 2 "http://127.0.0.1:$SERVER_PORT/api/stats" && break; sleep 0.2; done

# POST create（slow job → 随后 cancel）
"$PY" - "$WORK/spec-g.json" "$PORT" <<'EOF'
import json, sys
port = sys.argv[2]
spec = {
  "request": {"method": "GET", "target": "/slow?d=§P§", "version": "http/1.1",
              "headers": [{"name": "Host", "value": f"127.0.0.1:{port}"}],
              "body": "", "trailers": [], "framing": "content_length"},
  "mode": "cartesian",
  "payloads": [{"type": "list", "values": ["g1","g2"]}],
  "budget": {"max_requests": 5, "max_concurrency": 1}
}
open(sys.argv[1], "w").write(json.dumps(spec))
EOF
SPEC_G=$(cat "$WORK/spec-g.json")
"$PY" - "$WORK/create-body.json" "$SPEC_G" <<'EOF'
import json, sys
json.dump({"session_id": "wsw-ses-w4a", "spec": json.loads(sys.argv[2])}, open(sys.argv[1], "w"))
EOF
CREATE=$(curl -s --max-time 5 -X POST -H "$AUTH" -H "Content-Type: application/json" \
  -d @"$WORK/create-body.json" "http://127.0.0.1:$SERVER_PORT/api/wsw/fuzz/jobs")
G_JID=$(echo "$CREATE" | "$PY" -c "import json,sys; print(json.load(sys.stdin).get('job_id',''))")
ck "POST /fuzz/jobs 回执 job_id 形状" "yes" "$([ "$G_JID" = "wsw-fzj-"* ] || [[ "$G_JID" == wsw-fzj-* ]] && echo yes)"
ck "POST /fuzz/jobs 回执 status_url" "yes" "$(echo "$CREATE" | grep -q "status_url" && echo yes)"

# job 行就绪轮询（spawn 异步面：子进程 spec 预检 + 行登记需数百 ms）。
READY=""
for i in $(seq 1 50); do
  DETAIL=$(curl -s --max-time 5 -H "$AUTH" "http://127.0.0.1:$SERVER_PORT/api/wsw/fuzz/jobs/$G_JID")
  echo "$DETAIL" | "$PY" -c "import json,sys; d=json.load(sys.stdin); sys.exit(0 if d.get('job_id')=='$G_JID' else 1)" 2>/dev/null && { READY=yes; break; }
  sleep 0.2
done
ck "POST spawn 后 job 行就绪（子进程登记）" "yes" "${READY:-no}"
# GET detail（job 运行中可见）
DETAIL=$(curl -s --max-time 5 -H "$AUTH" "http://127.0.0.1:$SERVER_PORT/api/wsw/fuzz/jobs/$G_JID")
ck "GET detail 含 job 行" "$G_JID" "$(echo "$DETAIL" | "$PY" -c "import json,sys; print(json.load(sys.stdin).get('job_id',''))" 2>/dev/null)"
ck "GET detail 含 case_counts" "yes" "$(echo "$DETAIL" | "$PY" -c "import json,sys; print('yes' if isinstance(json.load(sys.stdin).get('case_counts'), dict) else 'no')" 2>/dev/null)"

# POST cancel（SIGTERM → graceful CANCELLED）
sleep 0.8  # 等 case 在途
CANCEL=$(curl -s --max-time 10 -X POST -H "$AUTH" "http://127.0.0.1:$SERVER_PORT/api/wsw/fuzz/jobs/$G_JID/cancel")
ck "POST cancel cancelled=true" "True" "$(echo "$CANCEL" | "$PY" -c "import json,sys; print(json.load(sys.stdin).get('cancelled'))")"
sleep 0.5
G_STATE=$(sqlite3 "file:$DB?mode=ro" "SELECT state FROM wsw_fuzz_job WHERE job_id='$G_JID'")
ck "cancel 后 job 终态（经端点）" "CANCELLED" "$G_STATE"

# GET list
LIST=$(curl -s --max-time 5 -H "$AUTH" "http://127.0.0.1:$SERVER_PORT/api/wsw/fuzz/jobs?limit=100")
N_JOBS=$(echo "$LIST" | "$PY" -c "import json,sys; print(len(json.load(sys.stdin).get('jobs',[])))")
ck "GET list 含全部 job（>=6）" "yes" "$([ "$N_JOBS" -ge 6 ] && echo yes)"

# History 闭环（判据 10）：exchange(source=fuzzer) 经 /api/wsw/history 可查
HIST=$(curl -s --max-time 5 -H "$AUTH" "http://127.0.0.1:$SERVER_PORT/api/wsw/history?source=fuzzer&limit=100")
H_N=$(echo "$HIST" | "$PY" -c "import json,sys; print(len(json.load(sys.stdin).get('rows',[])))")
H_EXPECT=$(sqlite3 "file:$DB?mode=ro" "SELECT COUNT(*) FROM wsw_exchange WHERE source='fuzzer'")
ck "History API source=fuzzer 行数 = DB 对账" "$H_EXPECT" "$H_N"
EX_ID=$(sqlite3 "file:$DB?mode=ro" "SELECT exchange_id FROM wsw_fuzz_case WHERE payload_json LIKE '%alpha%' AND exchange_id IS NOT NULL LIMIT 1")
# 响应体按需（direction=response；D3 端点 body_b64 → CAS 逐字节回读断言）。
BODY_RESP=$(curl -s --max-time 5 -H "$AUTH" "http://127.0.0.1:$SERVER_PORT/api/wsw/history/$EX_ID/body?direction=response")
BODY_OK=$(echo "$BODY_RESP" | "$PY" -c "
import json,sys,base64
d = json.load(sys.stdin)
# CAS 存原始 wire（响应头+体）；echo 体在字节面回读可见（判据 10）。
body = base64.b64decode(d.get('body','') or b'')
print('yes' if b'alpha' in body else 'no-mismatch:' + repr(body[:40]))
" 2>/dev/null)
ck "body 按需端点 CAS 回读（echo 体 alpha 在场，判据 10）" "yes" "$BODY_OK"

kill "$SERVER_PID" 2>/dev/null
kill "$TARGET_PID" 2>/dev/null

echo "== 9. platform*.db 反证（生产库零触碰，红线 5）"
for f in migrations/platform.db migrations/platform-dryrun.db migrations/platform-fullbatch.db migrations/platform-governance.db migrations/platform-search.db; do
  [ -f "$f" ] || continue
  UV=$(sqlite3 "file:$f?mode=ro" "PRAGMA user_version")
  echo "  [数字] $f user_version=$UV"
done

echo "== 10. 汇总"
echo "PASS=$PASS FAIL=$FAIL"
echo "数字面：JobA 吞吐=$A_DISP/$((A_DUR))ms；取消延迟=${E_CANCEL_MS}ms；靶机端口=$PORT"
[ "$FAIL" = "0" ] && echo "SELFTEST_WSW4: ALL GREEN" || echo "SELFTEST_WSW4: FAILED"
exit "$([ "$FAIL" = "0" ] && echo 0 || echo 1)"
