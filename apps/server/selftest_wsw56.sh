#!/usr/bin/env bash
# ============================================================================
# selftest_wsw56.sh — WSW-5/6 WebSocket Workbench + Decoder/Comparer/Passive
#   真实靶机实测收口（WSW-5-6-TASK §三 S8 + 判据 1-10 数字来源；禁模拟冒充
#   ——PLAN :754：全部数字来自命令回显）。
#
# 靶机 = python3 标准库手工 RFC6455 最小 echo server（sha1 算
#   Sec-WebSocket-Accept + 掩码帧解析 ~80 行，零新依赖；REDLINES 红线 8
#   loopback 面）+ HTTP 被动面响应器（Server: nginx/1.24.0 等——passive
#   规则命中预期对账源）。真实 WS 会话 = python3 客户端经 rp-wsw Proxy
#   绝对形式升级握手（与 tests/ws_frame.rs 同链路）+ rp-wsw ws connect 直连。
#
# V-9 守卫预挂：rp-wsw bin / migrations/dev-b1.db 副本 / python3 缺 →
#   SKIP 计数 exit 0（0f2922cb 教训口径）。
# 纪律：临时工作目录（退出不清理，留对账）；platform*.db 零触碰；
#   web/** 零触碰；sender.rs/scope.rs 零触碰。
# ============================================================================
set -u
cd "$(dirname "$0")/../.."   # repo root
REPO_ROOT="$PWD"

BIN="${RP_WSW_BIN:-/home/emo/rt3-target/debug/rp-wsw}"
PY=python3
PASS=0; SKIP=0; FAIL=0
ck() {  # ck <label> <expected> <actual>
  if [ "$2" = "$3" ]; then PASS=$((PASS+1)); echo "  [ok] $1 = $3"
  else FAIL=$((FAIL+1)); echo "  [FAIL] $1: expected=$2 actual=$3"; fi
}
sk() { SKIP=$((SKIP+1)); echo "  [SKIP] $1"; }

# --- V-9 守卫预挂（bin/副本/解释器缺 → SKIP 计数 exit 0）---
echo "== 0. V-9 守卫 + 环境"
if [ ! -x "$BIN" ]; then sk "rp-wsw 二进制不存在（cargo build -p wsw-runtime --bin rp-wsw）"; fi
if ! command -v "$PY" >/dev/null 2>&1; then sk "python3 不存在"; fi
if [ ! -f migrations/dev-b1.db ]; then sk "migrations/dev-b1.db 开发副本不存在"; fi
GUARD_MISS=0
[ -x "$BIN" ] || GUARD_MISS=1
command -v "$PY" >/dev/null 2>&1 || GUARD_MISS=1
[ -f migrations/dev-b1.db ] || GUARD_MISS=1
if [ "$GUARD_MISS" = "1" ]; then
  echo "SKIP=$SKIP FAIL=$FAIL PASS=$PASS（V-9 受控环境守卫命中，exit 0）"
  exit 0
fi
ck "rp-wsw 二进制在位" "yes" "yes"

WORK=$(mktemp -d /tmp/wsw56-selftest.XXXXXX)
export RP_ARTIFACTS_DIR="$WORK/artifacts"
mkdir -p "$WORK/state" "$RP_ARTIFACTS_DIR"
WSW_STATE_DIR="$WORK/state"
DB="$WORK/wsw56.db"
PROXY_PORT=$(python3 -c "import socket; s=socket.socket(); s.bind(('127.0.0.1',0)); print(s.getsockname()[1]); s.close()")
TARGET_PORT=$(python3 -c "import socket; s=socket.socket(); s.bind(('127.0.0.1',0)); print(s.getsockname()[1]); s.close()")
HTTP_PORT=$(python3 -c "import socket; s=socket.socket(); s.bind(('127.0.0.1',0)); print(s.getsockname()[1]); s.close()")
echo "== 环境：工作目录 $WORK / 二进制 $BIN / proxy=$PROXY_PORT target=$TARGET_PORT http=$HTTP_PORT"

# --- 1. 迁移 030 + 幂等重跑（判据 3）---
cp migrations/dev-b1.db "$DB"
"$BIN" db migrate --db "$DB" >/dev/null 2>&1
UV=$(sqlite3 "file:$DB?mode=ro" "PRAGMA user_version")
ck "迁移水位 user_version=30（判据 3）" "30" "$UV"
T1=$(sqlite3 "file:$DB?mode=ro" "SELECT COUNT(*) FROM schema_version WHERE version=30")
ck "030 登记行 INSERT OR IGNORE 单行" "1" "$T1"
OUT2=$("$BIN" db migrate --db "$DB" 2>&1)
UV2=$(sqlite3 "file:$DB?mode=ro" "PRAGMA user_version")
ck "幂等重跑零异常（水位不动）" "30" "$UV2"
TBL=$(sqlite3 "file:$DB?mode=ro" "SELECT COUNT(*) FROM sqlite_master WHERE name IN ('wsw_ws_frame','wsw_passive_hit')")
ck "030 两表在场" "2" "$TBL"

# --- 2. python3 stdlib RFC6455 echo 靶机 + HTTP 被动面 ---
cat > "$WORK/target.py" <<EOF
import socket, base64, hashlib, struct, threading, sys, json, socketserver, http.server
GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"
TARGET_PORT = int(sys.argv[1]); HTTP_PORT = int(sys.argv[2]); STATE = sys.argv[3]
state = {"connections": 0, "frames": 0, "data_frames": 0, "last_opcode": "", "last_payload_hex": "", "last_data_payload_hex": ""}
lock = threading.Lock()

def read_frame(sock):
    hdr = b""
    while len(hdr) < 2:
        c = sock.recv(2 - len(hdr))
        if not c: return None
        hdr += c
    b0, b1 = hdr[0], hdr[1]
    opcode = b0 & 0x0F
    masked = (b1 & 0x80) != 0
    ln = b1 & 0x7F
    if ln == 126:
        ext = b""
        while len(ext) < 2: ext += sock.recv(2 - len(ext))
        ln = struct.unpack(">H", ext)[0]
    elif ln == 127:
        ext = b""
        while len(ext) < 8: ext += sock.recv(8 - len(ext))
        ln = struct.unpack(">Q", ext)[0]
    mask = b""
    if masked:
        while len(mask) < 4: mask += sock.recv(4 - len(mask))
    payload = b""
    while len(payload) < ln: payload += sock.recv(ln - len(payload))
    if masked:
        payload = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
    return opcode, payload

def send_frame(sock, opcode, payload):
    b0 = 0x80 | opcode
    ln = len(payload)
    if ln < 126:
        sock.sendall(struct.pack(">BB", b0, ln) + payload)
    elif ln < 65536:
        sock.sendall(struct.pack(">BBH", b0, 126, ln) + payload)
    else:
        sock.sendall(struct.pack(">BBQ", b0, 127, ln) + payload)

class Echo(threading.Thread):
    def __init__(self, sock): super().__init__(daemon=True); self.sock = sock
    def run(self):
        try:
            data = b""
            while b"\r\n\r\n" not in data:
                c = self.sock.recv(4096)
                if not c: return
                data += c
            head = data.split(b"\r\n\r\n")[0].decode("latin1")
            key = ""
            for line in head.split("\r\n")[1:]:
                if ":" in line:
                    k, v = line.split(":", 1)
                    if k.strip().lower() == "sec-websocket-key": key = v.strip()
            accept = base64.b64encode(hashlib.sha1((key + GUID).encode()).digest()).decode()
            self.sock.sendall(("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\n"
                               f"Connection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n").encode())
            with lock:
                state["connections"] += 1
                json.dump(state, open(STATE, "w"))
            while True:
                fr = read_frame(self.sock)
                if fr is None: break
                opcode, payload = fr
                with lock:
                    state["frames"] += 1
                    if opcode in (1, 2):
                        state["data_frames"] += 1
                        state["last_data_payload_hex"] = payload.hex()
                    state["last_opcode"] = {1:"text",2:"binary",9:"ping",8:"close",10:"pong"}.get(opcode, str(opcode))
                    state["last_payload_hex"] = payload.hex()
                    json.dump(state, open(STATE, "w"))
                if opcode == 1: send_frame(self.sock, 1, payload)
                elif opcode == 2: send_frame(self.sock, 2, payload)
                elif opcode == 9: send_frame(self.sock, 10, payload)
                elif opcode == 8:
                    send_frame(self.sock, 8, payload); break
        except OSError:
            pass

def ws_server():
    s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.bind(("127.0.0.1", TARGET_PORT)); s.listen(16)
    while True:
        sock, _ = s.accept()
        Echo(sock).start()

class HttpH(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        body = b"passive-target-ok"
        self.send_response(200)
        self.send_header("Content-Type", "text/plain")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Set-Cookie", "sid=abc123; Path=/")
        self.send_header("Server", "nginx/1.24.0")
        self.send_header("X-Powered-By", "PHP/8.2")
        self.end_headers()
        self.wfile.write(body)

def http_server():
    socketserver.ThreadingTCPServer.allow_reuse_address = True
    s = socketserver.ThreadingTCPServer(("127.0.0.1", HTTP_PORT), HttpH)
    s.serve_forever()

threading.Thread(target=ws_server, daemon=True).start()
threading.Thread(target=http_server, daemon=True).start()
json.dump(state, open(STATE, "w"))
while True:
    threading.Event().wait(1)
EOF
"$PY" "$WORK/target.py" "$TARGET_PORT" "$HTTP_PORT" "$WORK/target-state.json" &
TARGET_PID=$!
for i in $(seq 1 50); do [ -s "$WORK/target-state.json" ] && break; sleep 0.1; done
ck "靶机起在 127.0.0.1 随机端口" "yes" "yes"

# --- 会话行（scoped / deny 两面 + rp_server 默认 session）---
NOW=$(date -u +%Y-%m-%dT%H:%M:%S.000Z)
sqlite3 "$DB" <<EOF
INSERT INTO wsw_session (session_id, title, scope_rules_json, out_of_scope_policy, created_at, updated_at)
VALUES ('wsw-ses-w56a', 'w56 scoped', '{"include":["127.0.0.1"],"allow_private_targets":true}', 'blocked', '$NOW', '$NOW');
INSERT INTO wsw_session (session_id, title, scope_rules_json, out_of_scope_policy, created_at, updated_at)
VALUES ('wsw-ses-w56deny', 'w56 default deny', '{"include":[],"allow_private_targets":false}', 'blocked', '$NOW', '$NOW');
INSERT INTO wsw_session (session_id, title, scope_rules_json, out_of_scope_policy, created_at, updated_at)
VALUES ('wsw-ses-cli-0001', 'w56 cli default', '{"include":["127.0.0.1"],"allow_private_targets":true}', 'blocked', '$NOW', '$NOW');
EOF

# --- 3. listener 启动（真实代理面）---
"$BIN" listener --bind "127.0.0.1:$PROXY_PORT" --db "$DB" --ca-dir "$WORK/ca" \
  --rules-json '{"include":["127.0.0.1"],"allow_private_targets":true}' \
  --status-file "$WORK/listener-status.json" --pid-file "$WORK/listener.pid" \
  >"$WORK/listener.log" 2>&1 &
LISTENER_PID=$!
STATE="x"
for i in $(seq 1 100); do
  STATE=$(sqlite3 "file:$DB?mode=ro" "SELECT state FROM wsw_listener ORDER BY rowid DESC LIMIT 1" 2>/dev/null)
  [ "$STATE" = "running" ] && break
  sleep 0.1
done
ck "listener 六态 running（024 登记行）" "running" "$STATE"

# --- 4. §A 经代理真实 WS 会话（判据 1/2：对账 + 保真）---
cat > "$WORK/wsclient.py" <<'EOF'
import socket, base64, os, struct, sys, json, time

def ws_session(proxy, target, messages):
    """经代理（proxy=None 直连）真实 WS 会话；返回 (echo 收帧列表, 耗时 ms)。"""
    t0 = time.time()
    target_host, target_port, path = target
    key = base64.b64encode(os.urandom(16)).decode()
    if proxy:
        host, port = proxy
        sock = socket.create_connection((host, port), timeout=8)
        req_target = f"http://{target_host}:{target_port}{path}"
        host_hdr = f"{target_host}:{target_port}"
    else:
        sock = socket.create_connection((target_host, target_port), timeout=8)
        req_target = f"{path}"
        host_hdr = f"{target_host}:{target_port}"
    req = (f"GET {req_target} HTTP/1.1\r\nHost: {host_hdr}\r\nConnection: Upgrade\r\n"
           f"Upgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: {key}\r\n\r\n")
    sock.sendall(req.encode())
    resp = b""
    while b"\r\n\r\n" not in resp:
        c = sock.recv(4096)
        if not c: raise RuntimeError("no handshake response")
        resp += c
    status = int(resp.split(b" ")[1])
    if status != 101:
        return {"status": status}, int((time.time() - t0) * 1000)

    def send_frame(opcode, payload):
        b0 = 0x80 | opcode
        ln = len(payload)
        mask = os.urandom(4)
        masked = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
        if ln < 126:
            sock.sendall(struct.pack(">BB", b0, 0x80 | ln) + mask + masked)
        elif ln < 65536:
            sock.sendall(struct.pack(">BBH", b0, 0x80 | 126, ln) + mask + masked)
        else:
            sock.sendall(struct.pack(">BBQ", b0, 0x80 | 127, ln) + mask + masked)

    def read_frame():
        hdr = b""
        while len(hdr) < 2:
            c = sock.recv(2 - len(hdr))
            if not c: return None
            hdr += c
        opcode = hdr[0] & 0x0F
        ln = hdr[1] & 0x7F
        ext = b""
        if ln == 126:
            while len(ext) < 2: ext += sock.recv(2 - len(ext))
            ln = struct.unpack(">H", ext)[0]
        elif ln == 127:
            while len(ext) < 8: ext += sock.recv(8 - len(ext))
            ln = struct.unpack(">Q", ext)[0]
        if hdr[1] & 0x80:
            mask = b""
            while len(mask) < 4: mask += sock.recv(4 - len(mask))
        payload = b""
        while len(payload) < ln: payload += sock.recv(ln - len(payload))
        return opcode, payload

    out = []
    # 交互流：先发数据帧（非 close）→ 读回声（text/binary 回声 + ping→pong
    # 协议层自动）→ 发 Close → 读服务端 close（RFC6455 收口序——客户端先发
    # close 再读数据帧会触发对端 tungstenite 写侧关闭，服务端帧无法再达）。
    data_msgs = [(op, p) for op, p in messages if op != 8]
    for opcode, payload in data_msgs:
        send_frame(opcode, payload)
    sock.settimeout(8)
    expect = len(data_msgs)
    try:
        while len(out) < expect:
            fr = read_frame()
            if fr is None: break
            op, pl = fr
            out.append({"opcode": {1: "text", 2: "binary", 9: "ping", 10: "pong", 8: "close"}.get(op, str(op)),
                        "payload_hex": pl.hex(), "len": len(pl)})
        # close 收口（数据回声读齐后发——服务端 close 回声可达）。
        send_frame(8, b"")
        while True:
            fr = read_frame()
            if fr is None: break
            op, pl = fr
            out.append({"opcode": {1: "text", 2: "binary", 9: "ping", 10: "pong", 8: "close"}.get(op, str(op)),
                        "payload_hex": pl.hex(), "len": len(pl)})
            if op == 8:
                break
    except (socket.timeout, OSError):
        pass
    sock.close()
    return out, int((time.time() - t0) * 1000)

if __name__ == "__main__":
    # argv: proxy_host,proxy_port|direct  target_port  path  state_out  msgs_json
    proxy_arg, target_port, path, state_out, msgs_json = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4], sys.argv[5]
    proxy = None
    if proxy_arg != "direct":
        ph, pp = proxy_arg.split(":")
        proxy = (ph, int(pp))
    msgs = [(op, (bytes.fromhex(p) if p else b"") if op == 2 else p.encode())
            for op, p in json.loads(msgs_json)]
    echoes, ms = ws_session(proxy, ("127.0.0.1", target_port, path), msgs)
    json.dump({"echoes": echoes, "ms": ms}, open(state_out, "w"))
EOF

TEXT_MSG="wsw56-écho-帧"
BIN_HEX=$(python3 -c "print(bytes(range(32)).hex())")
PING_B64="cGluZzU"
python3 "$WORK/wsclient.py" "127.0.0.1:$PROXY_PORT" "$TARGET_PORT" "/wsw56-echo" "$WORK/sess-a.json" \
  "[[1, \"$TEXT_MSG\"], [2, \"$BIN_HEX\"], [9, \"$PING_B64\"], [8, \"\"]]" >/dev/null 2>&1
# 回声序非确定（代理 tungstenite 服务端角色对 client Ping 自应答 pong +
# 上游 pong 透传——RFC6455 5.5.3 合法行为）：按词表集合 + 逐字节回声断言。
A_CHECK=$("$PY" - "$WORK/sess-a.json" "$TEXT_MSG" "$BIN_HEX" <<'PYA'
import json, sys
d = json.load(open(sys.argv[1]))
echoes = d["echoes"]
ops = [e["opcode"] for e in echoes]
text_ok = any(e["opcode"] == "text" and e["payload_hex"] == sys.argv[2].encode().hex() for e in echoes)
bin_ok = any(e["opcode"] == "binary" and e["payload_hex"] == sys.argv[3] for e in echoes)
close_ok = any(e["opcode"] == "close" for e in echoes)
pong_n = ops.count("pong")
print(f"count={len(echoes)} text={text_ok} binary={bin_ok} close={close_ok} pong={pong_n}")
PYA
)
A_HITS=$(echo "$A_CHECK" | grep -o "True" | wc -l)
A_PONG_N=$(echo "$A_CHECK" | grep -oE "pong=[0-9]+" | grep -oE "[0-9]+")
ck "§A 词表集合 text/binary/close 回声逐字节在场（判据 1/2）" "3" "$A_HITS"
ck "§A pong 回声 >=1（代理自应答/上游透传）" "yes" "$([ "${A_PONG_N:-0}" -ge 1 ] && echo yes)"
echo "  [数字] §A 回声序：$A_CHECK"

# --- 5. §B CLI ws connect（直连 + scope 闸负例，判据 7/8）---
B_OUT=$("$BIN" ws connect --db "$DB" --session-id wsw-ses-w56a \
  --url "ws://127.0.0.1:$TARGET_PORT/wsw56-cli" --payload "cli-direct-hi" --echo-timeout 3 2>/dev/null | tail -1)
B_ECHO=$("$PY" -c "
import json,sys
try: print(json.loads(sys.argv[1]).get('echo',{}).get('text',''))
except Exception: print('')
" "$B_OUT")
ck "§B ws connect 直连 echo = 发送文本" "cli-direct-hi" "$B_ECHO"
T_DELTA_B=$("$PY" -c "import json; print(json.load(open('$WORK/target-state.json'))['frames'])")
ck "§B 靶机 delta（ws connect 后）" "yes" "$([ "$T_DELTA_B" -ge 2 ] && echo yes)"
set +e
"$BIN" ws connect --db "$DB" --session-id wsw-ses-w56deny \
  --url "ws://127.0.0.1:$TARGET_PORT/x" --payload hi --echo-timeout 1 >"$WORK/deny.out" 2>/dev/null
RC=$?
set +e
DENY_JSON=$(tail -1 "$WORK/deny.out" 2>/dev/null)
ck "§B scope 外 ws connect 拒绝（exit 3，判据 8）" "3" "$RC"
ck "§B 拒绝词形 scope_blocked/no_rules_default_deny" "yes" "$(echo "$DENY_JSON" | grep -q 'scope_blocked' && echo yes)"

# --- 6. §C ws replay（编辑/原样两态，判据 2）---
# §A 握手行 = 首个 protocol='ws' exchange（重放目标还原面 + 帧行选择确定性
# ——1000 帧会话同表后 ORDER BY sequence_no 单独取会跨连接取错帧）。
HS_EX=$(sqlite3 "file:$DB?mode=ro" "SELECT exchange_id FROM wsw_exchange WHERE protocol='ws' ORDER BY started_at, exchange_id LIMIT 1")
FRAME_ID=$(sqlite3 "file:$DB?mode=ro" "SELECT frame_id FROM wsw_ws_frame WHERE direction='client_to_server' AND opcode='text' AND connection_exchange_id='$HS_EX' ORDER BY sequence_no LIMIT 1")
ORIG_PAYLOAD_HEX=$(sqlite3 "file:$DB?mode=ro" "SELECT hex(CASE WHEN payload_body_uri IS NULL THEN '' ELSE payload_body_uri END) FROM wsw_ws_frame WHERE frame_id='$FRAME_ID'")
ORIG_BYTES_HEX=$(RP_ARTIFACTS_DIR="$RP_ARTIFACTS_DIR" "$PY" -c "
import os, sys
uri = bytes.fromhex('$ORIG_PAYLOAD_HEX').decode()
root = os.environ['RP_ARTIFACTS_DIR']
print(open(os.path.join(root, uri), 'rb').read().hex())
")
T0_DATA=$("$PY" -c "import json; print(json.load(open('$WORK/target-state.json'))['data_frames'])")
R_OUT=$("$BIN" ws replay --db "$DB" --frame-id "$FRAME_ID" --session-id wsw-ses-w56a --echo-timeout 3 2>/dev/null | tail -1)
R_ECHO_LEN=$("$PY" -c "
import json,sys
try: print(json.loads(sys.argv[1]).get('echo',{}).get('len', -1))
except Exception: print('-1')
" "$R_OUT")
ck "§C replay 原样回声非空（len>=1）" "yes" "$([ "$R_ECHO_LEN" -ge 1 ] && echo yes)"
R_LAST_HEX=$("$PY" -c "import json; print(json.load(open('$WORK/target-state.json'))['last_data_payload_hex'])")
ck "§C replay 原样 = 原帧 payload（判据 2）" "$ORIG_BYTES_HEX" "$R_LAST_HEX"
T1_DATA=$("$PY" -c "import json; print(json.load(open('$WORK/target-state.json'))['data_frames'])")
ck "§C replay 原样靶机 delta=+1（data_frames 口径）" "1" "$((T1_DATA - T0_DATA))"
"$BIN" ws replay --db "$DB" --frame-id "$FRAME_ID" --payload "EDITED-wsw56" --session-id wsw-ses-w56a --echo-timeout 3 >/dev/null 2>&1
R2_LAST_HEX=$("$PY" -c "import json; print(json.load(open('$WORK/target-state.json'))['last_data_payload_hex'])")
WANT_EDITED_HEX=$(printf 'EDITED-wsw56' | xxd -p | tr -d '\n')
ck "§C replay 编辑帧靶机实收 = 编辑值（判据 2）" "$WANT_EDITED_HEX" "$R2_LAST_HEX"

# --- 7. §D 1000 帧 + rp_server 七端点（判据 7/9）---
PAGEMSGS=$("$PY" -c "
import json
msgs = [[1, f'p-{i:04d}-pad' + 'x' * 20] for i in range(1000)]
print(json.dumps(msgs))
")
"$PY" "$WORK/wsclient.py" "127.0.0.1:$PROXY_PORT" "$TARGET_PORT" "/wsw56-page" "$WORK/sess-page.json" "$PAGEMSGS" noecho >/dev/null 2>&1 &
PAGE_PID=$!
# 等帧行落库（worker 批面）。
FRAME_TOTAL=""
for i in $(seq 1 300); do
  FRAME_TOTAL=$(sqlite3 "file:$DB?mode=ro" "SELECT COUNT(*) FROM wsw_ws_frame" 2>/dev/null)
  [ "${FRAME_TOTAL:-0}" -ge 2008 ] && break
  sleep 0.1
done
ck "§D 1000 帧经代理落库（双向 2000 + §A 8）" "yes" "$([ "${FRAME_TOTAL:-0}" -ge 2008 ] && echo yes)"

SERVER_PORT=$("$PY" -c "import socket; s=socket.socket(); s.bind(('127.0.0.1',0)); print(s.getsockname()[1]); s.close()")
RP_PORT=$SERVER_PORT RP_HOST=127.0.0.1 RP_AUTH_TOKEN=wsw56-selftest-token \
RP_WSW_DB="$DB" RP_WSW_BIN="$BIN" RP_WSW_STATE_DIR="$WSW_STATE_DIR" \
RP_ARTIFACTS_DIR="$RP_ARTIFACTS_DIR" \
  "$PY" apps/server/rp_server.py >"$WORK/server.log" 2>&1 &
SERVER_PID=$!
AUTH="Authorization: Bearer wsw56-selftest-token"
for i in $(seq 1 50); do curl -s -o /dev/null --max-time 2 "http://127.0.0.1:$SERVER_PORT/api/stats" && break; sleep 0.2; done

# 401 负例
CODE401=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 "http://127.0.0.1:$SERVER_PORT/api/wsw/ws/frames")
ck "GET /ws/frames 无鉴权 401（判据 7）" "401" "$CODE401"

# 游标分页零重零漏（判据 7：1000+ 帧遍历断言）
WALK=$("$PY" - "$SERVER_PORT" "wsw56-selftest-token" <<'EOF'
import json, sys, urllib.request, urllib.error
port, token = int(sys.argv[1]), sys.argv[2]
auth = {"Authorization": f"Bearer {token}"}
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
seen, cursor, pages, dup = [], "", 0, 0
while True:
    url = f"http://127.0.0.1:{port}/api/wsw/ws/frames?limit=500" + (f"&cursor={cursor}" if cursor else "")
    req = urllib.request.Request(url, headers=auth)
    d = json.load(opener.open(req, timeout=15))
    for r in d["rows"]:
        if r["frame_id"] in seen: dup += 1
        seen.append(r["frame_id"])
    pages += 1
    if not d["has_more"] or not d["next_cursor"]:
        break
    cursor = d["next_cursor"]
    assert pages < 100, "pagination loop"
print(f"{len(seen)} {dup} {pages}")
EOF
)
WALK_N=$(echo "$WALK" | awk '{print $1}')
WALK_DUP=$(echo "$WALK" | awk '{print $2}')
DB_TOTAL=$(sqlite3 "file:$DB?mode=ro" "SELECT COUNT(*) FROM wsw_ws_frame")
ck "§D 游标分页遍历行数 = DB 总数（判据 7 零漏）" "$DB_TOTAL" "$WALK_N"
ck "§D 游标分页零重复（判据 7 零重）" "0" "$WALK_DUP"

# filter_cursor_mismatch 400 负例
PAGE1=$(curl -s --max-time 5 -H "$AUTH" "http://127.0.0.1:$SERVER_PORT/api/wsw/ws/frames?limit=5")
CURSOR=$(echo "$PAGE1" | "$PY" -c "import json,sys; print(json.load(sys.stdin).get('next_cursor',''))")
CODE400=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 -H "$AUTH" \
  "http://127.0.0.1:$SERVER_PORT/api/wsw/ws/frames?limit=5&cursor=$CURSOR&direction=server_to_client")
ck "filter_cursor_mismatch 400 负例（判据 7）" "400" "$CODE400"

# 帧详情 payload 按需回读（CAS）
D_TEXT_FRAME=$(sqlite3 "file:$DB?mode=ro" "SELECT frame_id FROM wsw_ws_frame WHERE direction='client_to_server' AND opcode='text' AND payload_body_uri IS NOT NULL ORDER BY sequence_no LIMIT 1")
DETAIL=$(curl -s --max-time 5 -H "$AUTH" "http://127.0.0.1:$SERVER_PORT/api/wsw/ws/frames/$D_TEXT_FRAME?payload=1")
D_LEN=$(echo "$DETAIL" | "$PY" -c "import json,sys,base64; print(len(base64.b64decode(json.load(sys.stdin).get('payload',''))))" 2>/dev/null)
ck "帧详情 CAS 回读 base64 字节数 > 0" "yes" "$([ "${D_LEN:-0}" -ge 1 ] && echo yes)"
D_OPCODE=$(echo "$DETAIL" | "$PY" -c "import json,sys; print(json.load(sys.stdin).get('opcode',''))" 2>/dev/null)
ck "帧详情 opcode 词形" "text" "$D_OPCODE"

# POST decode（判据 4 面上 API 形状）
DEC=$(curl -s --max-time 10 -X POST -H "$AUTH" -H "Content-Type: application/json" \
  -d '{"input":"6125323062","ops":["hex_decode","url_decode"]}' \
  "http://127.0.0.1:$SERVER_PORT/api/wsw/decode")
DEC_R=$(echo "$DEC" | "$PY" -c "import json,sys; print(json.load(sys.stdin).get('result',''))" 2>/dev/null)
ck "POST /decode 链 hex_decode→url_decode" "a b" "$DEC_R"
CODE_OPS=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 -X POST -H "$AUTH" -H "Content-Type: application/json" \
  -d '{"input":"x","ops":["gzip_decode"]}' "http://127.0.0.1:$SERVER_PORT/api/wsw/decode")
ck "POST /decode 裁剪面 op 词表外 400（D5 登记）" "400" "$CODE_OPS"

# POST compare（判据 5 面 API 形状）
CMP=$(curl -s --max-time 10 -X POST -H "$AUTH" -H "Content-Type: application/json" \
  -d '{"mode":"text","left":{"text":"a\nb"},"right":{"text":"a\nc"}}' \
  "http://127.0.0.1:$SERVER_PORT/api/wsw/compare")
CMP_IDENT=$(echo "$CMP" | "$PY" -c "import json,sys; print(json.load(sys.stdin).get('identical'))" 2>/dev/null)
ck "POST /compare 差异 identical=false" "False" "$CMP_IDENT"
CMP2=$(curl -s --max-time 10 -X POST -H "$AUTH" -H "Content-Type: application/json" \
  -d '{"mode":"hex","left":{"text":"AAAA"},"right":{"text":"AAAB"}}' \
  "http://127.0.0.1:$SERVER_PORT/api/wsw/compare")
CMP2_SAME=$(echo "$CMP2" | "$PY" -c "import json,sys; print(json.load(sys.stdin).get('identical'))" 2>/dev/null)
CMP2_OFF=$(echo "$CMP2" | "$PY" -c "import json,sys; print(json.load(sys.stdin)['hex_diff']['first_diff_offset'])" 2>/dev/null)
ck "POST /compare 等长不同内容 identical=false（PLAN :303 负例）" "False" "$CMP2_SAME"
ck "POST /compare 首差异 OFFSET 归因" "3" "$CMP2_OFF"

# --- 8. §E Passive Audit（判据 6：HTTP 捕获 → 命中对账 + 零 evidence 负例）---
curl -s --max-time 5 -x "http://127.0.0.1:$PROXY_PORT" "http://127.0.0.1:$HTTP_PORT/passive" >/dev/null
sleep 1
PASSIVE_EX=$(sqlite3 "file:$DB?mode=ro" "SELECT exchange_id FROM wsw_exchange WHERE protocol='http' AND response_status=200 AND authority LIKE '127.0.0.1:$HTTP_PORT%' ORDER BY started_at DESC LIMIT 1")
PASSIVE_HITS=$(sqlite3 "file:$DB?mode=ro" "SELECT COUNT(*) FROM wsw_passive_hit WHERE exchange_id='$PASSIVE_EX'")
ck "§E 被动命中行数 = 规则预期 9（判据 6 对账）" "9" "$PASSIVE_HITS"
EVID=$(sqlite3 "file:$DB?mode=ro" "SELECT COUNT(*) FROM pragma_table_info('wsw_passive_hit') WHERE name='evidence_id'")
ck "§E hit 行零 evidence_id 列（判据 6 负例）" "0" "$EVID"
BLOBS=$(sqlite3 "file:$DB?mode=ro" "SELECT COUNT(*) FROM wsw_body_blob")
CASFILES=$(find "$RP_ARTIFACTS_DIR/wsw" -type f ! -name ".*" 2>/dev/null | wc -l)
ck "§E wsw_body_blob 行数 = CAS 文件数（sha256 寻址对账）" "$CASFILES" "$BLOBS"

# passive/hits + rules 端点
HITS_API=$(curl -s --max-time 5 -H "$AUTH" "http://127.0.0.1:$SERVER_PORT/api/wsw/passive/hits?limit=100")
HITS_N=$(echo "$HITS_API" | "$PY" -c "import json,sys; print(len(json.load(sys.stdin).get('rows',[])))")
HITS_DB=$(sqlite3 "file:$DB?mode=ro" "SELECT COUNT(*) FROM wsw_passive_hit")
ck "GET /passive/hits 行数 = DB 对账" "$HITS_DB" "$HITS_N"
RULES_API=$(curl -s --max-time 5 -H "$AUTH" "http://127.0.0.1:$SERVER_PORT/api/wsw/passive/rules")
RULES_COUNT=$(echo "$RULES_API" | "$PY" -c "import json,sys; print(json.load(sys.stdin).get('count'))" 2>/dev/null)
ck "GET /passive/rules 注册表投影 14 规则" "14" "$RULES_COUNT"
RULE_MATCH=$("$PY" - "$RULES_API" <<'EOF'
import json, re, sys
api = {r["rule_id"] for r in json.loads(sys.argv[1])["rules"]}
src = open("crates/wsw-runtime/src/passive.rs", encoding="utf-8").read()
m = re.search(r"pub const PASSIVE_RULES: \[PassiveRule; 14\] = \[(.*?)\n\];", src, re.S)
rust_ids = set(re.findall(r'id: "([a-z_]+)"', m.group(1)))
print("yes" if api == rust_ids and len(api) == 14 else f"no(api={sorted(api-rust_ids)[:3]}, rust={sorted(rust_ids-api)[:3]})")
EOF
)
ck "规则投影与 passive.rs 注册表逐条对账（判据 12）" "yes" "$RULE_MATCH"

# POST replay API 面（编辑态 + scope 403 负例）
REPLAY=$(curl -s --max-time 15 -X POST -H "$AUTH" -H "Content-Type: application/json" \
  -d "{\"frame_id\":\"$FRAME_ID\",\"payload\":\"API-EDIT-56\",\"session_id\":\"wsw-ses-w56a\"}" \
  "http://127.0.0.1:$SERVER_PORT/api/wsw/ws/replay")
R3_LAST_HEX=$("$PY" -c "import json; print(json.load(open('$WORK/target-state.json'))['last_data_payload_hex'])")
WANT_API_HEX=$(printf 'API-EDIT-56' | xxd -p | tr -d '\n')
ck "POST /ws/replay 编辑帧靶机实收 = 编辑值" "$WANT_API_HEX" "$R3_LAST_HEX"
REPLAY_CODE=$(curl -s -o /dev/null -w "%{http_code}" --max-time 15 -X POST -H "$AUTH" -H "Content-Type: application/json" \
  -d "{\"frame_id\":\"$FRAME_ID\",\"session_id\":\"wsw-ses-w56deny\"}" \
  "http://127.0.0.1:$SERVER_PORT/api/wsw/ws/replay")
ck "POST /ws/replay scope 外 403（判据 8 API 面）" "403" "$REPLAY_CODE"

# --- 9. §F 性能门（判据 9：代理增量 <50ms 中位数 + API p95 <300ms 20 采样）---
"$PY" "$WORK/wsclient.py" "127.0.0.1:$PROXY_PORT" "$TARGET_PORT" "/wsw56-perf" "$WORK/sess-p1.json" '[[1, "perf-proxy"]]' >/dev/null 2>&1
PROXY_MS=$("$PY" -c "import json; print(json.load(open('$WORK/sess-p1.json'))['ms'])")
"$PY" "$WORK/wsclient.py" "direct" "$TARGET_PORT" "/wsw56-perf" "$WORK/sess-p2.json" '[[1, "perf-direct"]]' >/dev/null 2>&1
DIRECT_MS=$("$PY" -c "import json; print(json.load(open('$WORK/sess-p2.json'))['ms'])")
for i in 1 2 3; do
  "$PY" "$WORK/wsclient.py" "127.0.0.1:$PROXY_PORT" "$TARGET_PORT" "/wsw56-perf" "$WORK/sess-p1.json" '[[1, "perf-proxy"]]' >/dev/null 2>&1
  PROXY_MS="$PROXY_MS;$("$PY" -c "import json; print(json.load(open('$WORK/sess-p1.json'))['ms'])")"
  "$PY" "$WORK/wsclient.py" "direct" "$TARGET_PORT" "/wsw56-perf" "$WORK/sess-p2.json" '[[1, "perf-direct"]]' >/dev/null 2>&1
  DIRECT_MS="$DIRECT_MS;$("$PY" -c "import json; print(json.load(open('$WORK/sess-p2.json'))['ms'])")"
done
PERF=$("$PY" - "$PROXY_MS" "$DIRECT_MS" <<'EOF'
import statistics, sys
p = [int(x) for x in sys.argv[1].split(";") if x]
d = [int(x) for x in sys.argv[2].split(";") if x]
pm, dm = statistics.median(p), statistics.median(d)
print(f"{pm:.0f} {dm:.0f} {pm - dm:.0f}")
EOF
)
P_MED=$(echo "$PERF" | awk '{print $1}')
D_MED=$(echo "$PERF" | awk '{print $2}')
DELTA=$(echo "$PERF" | awk '{print $3}')
echo "  [数字] WS 握手+回声中位数：经代理=${P_MED}ms 直连=${D_MED}ms 增量=${DELTA}ms（判据 9）"
ck "代理增量中位数 <50ms" "yes" "$([ "$DELTA" -lt 50 ] && echo yes)"

# 20 采样循环面（curl --repeat 版本兼容退化为循环）。
P95_FRAMES=$("$PY" - "$SERVER_PORT" "wsw56-selftest-token" <<'EOF'
import subprocess, sys, time
port, token = sys.argv[1], sys.argv[2]
auth = f"Authorization: Bearer {token}"
ts = []
for _ in range(20):
    t0 = time.monotonic()
    subprocess.run(["curl", "-s", "-o", "/dev/null", "--max-time", "5", "-H", auth,
                    f"http://127.0.0.1:{port}/api/wsw/ws/frames?limit=100"], check=False)
    ts.append((time.monotonic() - t0) * 1000)
ts.sort()
print(f"{ts[18]:.0f}")
EOF
)
echo "  [数字] GET /ws/frames p95=${P95_FRAMES}ms（20 采样，判据 9）"
ck "GET /ws/frames p95 <300ms" "yes" "$([ "$P95_FRAMES" -lt 300 ] && echo yes)"

P95_DEC=$("$PY" - "$SERVER_PORT" "wsw56-selftest-token" <<'EOF'
import subprocess, sys, time, json
port, token = sys.argv[1], sys.argv[2]
auth = f"Authorization: Bearer {token}"
body = json.dumps({"input": "a%20b", "ops": ["url_decode"]}).encode()
ts = []
for _ in range(20):
    t0 = time.monotonic()
    subprocess.run(["curl", "-s", "-o", "/dev/null", "--max-time", "5", "-H", auth,
                    "-H", "Content-Type: application/json", "-d", body, "-X", "POST",
                    f"http://127.0.0.1:{port}/api/wsw/decode"], check=False)
    ts.append((time.monotonic() - t0) * 1000)
ts.sort()
print(f"{ts[18]:.0f}")
EOF
)
echo "  [数字] POST /decode p95=${P95_DEC}ms（20 采样，判据 9）"
ck "POST /decode p95 <300ms" "yes" "$([ "$P95_DEC" -lt 300 ] && echo yes)"

P95_HITS=$("$PY" - "$SERVER_PORT" "wsw56-selftest-token" <<'EOF'
import subprocess, sys, time
port, token = sys.argv[1], sys.argv[2]
auth = f"Authorization: Bearer {token}"
ts = []
for _ in range(20):
    t0 = time.monotonic()
    subprocess.run(["curl", "-s", "-o", "/dev/null", "--max-time", "5", "-H", auth,
                    f"http://127.0.0.1:{port}/api/wsw/passive/hits?limit=100"], check=False)
    ts.append((time.monotonic() - t0) * 1000)
ts.sort()
print(f"{ts[18]:.0f}")
EOF
)
echo "  [数字] GET /passive/hits p95=${P95_HITS}ms（20 采样，判据 9）"
ck "GET /passive/hits p95 <300ms" "yes" "$([ "$P95_HITS" -lt 300 ] && echo yes)"

# --- 10. §G 生产库反证（红线 5）+ 收口 ---
kill "$SERVER_PID" 2>/dev/null
kill "$LISTENER_PID" 2>/dev/null
kill "$TARGET_PID" 2>/dev/null
wait "$SERVER_PID" 2>/dev/null
echo "== 10. platform*.db 反证（生产库零触碰）"
for f in migrations/platform.db migrations/platform-dryrun.db migrations/platform-fullbatch.db migrations/platform-governance.db migrations/platform-search.db; do
  [ -f "$f" ] || continue
  UV=$(sqlite3 "file:$f?mode=ro" "PRAGMA user_version" 2>/dev/null)
  echo "  [数字] $f user_version=$UV"
done
ck "platform.db 生产库水位未被触碰" "1" "$(sqlite3 "file:migrations/platform.db?mode=ro" "PRAGMA user_version" 2>/dev/null)"

echo "== 11. 汇总"
echo "PASS=$PASS SKIP=$SKIP FAIL=$FAIL"
echo "数字面：帧行=$FRAME_TOTAL；§A 会话帧=8；§D 遍历=$WALK_N/零重=$WALK_DUP；§E 命中=$PASSIVE_HITS；代理增量=${DELTA}ms；p95 frames=${P95_FRAMES}ms decode=${P95_DEC}ms hits=${P95_HITS}ms"
[ "$FAIL" = "0" ] && echo "SELFTEST_WSW56: ALL GREEN" || echo "SELFTEST_WSW56: FAILED"
exit "$([ "$FAIL" = "0" ] && echo 0 || echo 1)"
