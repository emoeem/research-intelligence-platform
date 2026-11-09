#!/usr/bin/env python3
# pip install --break-system-packages websocket-client
# 用法: python3 ws_client.py <JWT> [friend_uid]
import sys, json, ssl, time, websocket
# ---- 参数守卫（无参数必须给出人话，不抛 traceback、不朝真实目标发包）----
import sys as _sys, os as _os
if len(_sys.argv) < 2:
    _u = '<参数...>'
    print("用法: " + _os.path.basename(__file__) + " " + _u, file=_sys.stderr)
    raise SystemExit(1)
# ---- 守卫结束 ----


URL = "wss://chat-love.qingteng-inc.com/ws"      # 备: chat-love-bak / jqjrpwr / jymqwni / ubjcnqa / dtosxwt / nglhotr .afunapp.com/ws
HDRS = ["Origin: https://servicewechat.com",
        "Referer: https://servicewechat.com/wx788f5daad54eb854/922/page-frame.html",
        "User-Agent: Mozilla/5.0 (Linux; Android 16) MicroMessenger/8.0.79 MiniProgramEnv/android"]
TYPE = {"bind": 1, "heartbeat": 2}              # 其余命令一律 Data(3)

def ev(n):
    o = bytearray()
    while n >= 128: o.append((n & 0x7f) | 0x80); n >>= 7
    o.append(n & 0x7f); return bytes(o)

def enc(t, s):                                   # VTCLP 编码
    p = s.encode(); return b"\x01" + ev(t) + b"\x00" + ev(len(p)) + p

def dv(b, i):
    v = sh = 0
    while True:
        x = b[i]; i += 1; v |= (x & 0x7f) << sh
        if not x & 0x80: break
        sh += 7
    return v, i

def dec(raw):
    if isinstance(raw, str): return raw
    i = 1; t, i = dv(raw, i); i += 1; n, i = dv(raw, i)
    return raw[i:i+n].decode("utf-8", "replace")

class C:
    def __init__(self):
        self.pid = 0
        self.ws = websocket.create_connection(URL, header=HDRS, timeout=20,
                                             sslopt={"cert_reqs": ssl.CERT_NONE})
    def call(self, cmd, param=None, t=15):
        self.pid += 1
        self.ws.send_binary(enc(TYPE.get(cmd, 3),
            json.dumps({"param": param or {}, "packet_id": self.pid, "cmd": cmd}, separators=(",", ":"))))
        end = time.time() + t
        while time.time() < end:
            try:
                self.ws.settimeout(max(0.5, end - time.time())); raw = self.ws.recv()
            except Exception as e: return {"error": str(e)}
            j = json.loads(dec(raw))
            if j.get("push"): continue
            if j.get("packet_id") == self.pid: return j
        return {"error": "timeout"}

    jwt = sys.argv[1]; friend = int(sys.argv[2]) if len(sys.argv) > 2 else 0
    c = C()
    print("bind", c.call("bind", {"uid": 0, "token": jwt, "platform": 1,
                                  "version": "5.62.1", "device_id": ""}))   # param.uid 被忽略，随便填
    print("max_seq", c.call("sync_max_seq"))
    print("session_list", c.call("sync_session_list"))
    print("history", json.dumps(c.call("sync_history",
          {"friend": friend, "msg_seq": 9007199254740991, "limit": 100}), ensure_ascii=False)[:4000])
