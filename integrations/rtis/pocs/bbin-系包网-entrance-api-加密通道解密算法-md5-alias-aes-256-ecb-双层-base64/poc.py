#!/usr/bin/env python3
"""entrance/api 加密通道 加解密工具（BBIN 系白牌包网站群通用）
算法源自 /web/build/js/chunk-common.<hash>.js 模块 adb7：
  key   = Utf8.parse( MD5(window.alias) )   # WordArray.toString() -> 32 位小写 hex 字符串，当 ASCII 用
  ct    = AES-256-ECB(JSON.stringify(obj), key, PKCS7)
  inner = base64(ct)                        # crypto-js CipherParams.toString() 默认 OpenSSL formatter
  wire  = base64(inner)                     # Utf8.parse + Base64.stringify => 双层 base64
仅依赖 openssl CLI，无需 pycryptodome。
"""
import base64, hashlib, json, ssl, subprocess, urllib.parse, urllib.request
# ---- 参数守卫（无参数必须给出人话，不抛 traceback、不朝真实目标发包）----
import sys as _sys, os as _os
if len(_sys.argv) < 2:
    _u = '<参数...>'
    print("用法: " + _os.path.basename(__file__) + " " + _u, file=_sys.stderr)
    raise SystemExit(1)
# ---- 守卫结束 ----


HOST_HDR = "TARGET_HOST:PORT"
BASE     = "https://TARGET_IP:PORT"

def _key(alias):
    return hashlib.md5(alias.encode()).hexdigest().encode()   # 32 ASCII chars -> AES-256

def _aes(data, key, dec):
    r = subprocess.run(["openssl","enc","-aes-256-ecb","-K",key.hex(),"-nopad",
                        "-d" if dec else "-e"], input=data, capture_output=True)
    if r.returncode: raise RuntimeError(r.stderr.decode())
    return r.stdout

def decrypt(wire, alias):
    inner = base64.b64decode(wire.strip())
    pt = _aes(base64.b64decode(inner), _key(alias), True)
    p = pt[-1]
    if 1 <= p <= 16 and pt[-p:] == bytes([p])*p: pt = pt[:-p]
    return json.loads(pt)

def encrypt(obj, alias):
    pt = json.dumps(obj, separators=(",",":"), ensure_ascii=False).encode()
    p = 16 - len(pt) % 16
    ct = _aes(pt + bytes([p])*p, _key(alias), False)
    return base64.b64encode(base64.b64encode(ct)).decode()

CTX = ssl.create_default_context(); CTX.check_hostname = False; CTX.verify_mode = ssl.CERT_NONE

def call(obj, alias, path="/entrance/api"):
    body = urllib.parse.urlencode({"requestData": encrypt(obj, alias)}).encode()
    req = urllib.request.Request(BASE+path, data=body, method="POST", headers={
        "Host": HOST_HDR,
        "Content-Type": "application/x-www-form-urlencoded; charset=UTF-8"})
    with urllib.request.urlopen(req, context=CTX, timeout=40) as r:
        return decrypt(r.read().decode(), alias)

# 用法：先 GET 入口页取 window.alias
#   alias = MD5 的输入就是页面里那串 base64 原文（不要 b64decode！）
# 已知未授权只读路由：
#   entrance/common/basicinfocontroller/getInfo.json   全站配置 59KB
#   entrance/langi18n/list.json                        文案 101KB
#   web/config/common.json                             模板配置 1.2KB
#   entrance/page/casino/getCasino.json                游戏清单 18KB
print(call({"url":"entrance/common/basicinfocontroller/getInfo.json","params":"[]"}, "ALIAS_B64"))

