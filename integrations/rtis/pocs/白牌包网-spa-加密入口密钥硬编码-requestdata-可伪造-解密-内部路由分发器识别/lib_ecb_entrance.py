#!/usr/bin/env python3
import hashlib, base64, json
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
# ---- 参数守卫（无参数必须给出人话，不抛 traceback、不朝真实目标发包）----
import sys as _sys, os as _os
if len(_sys.argv) < 2:
    _u = '<参数...>'
    print("用法: " + _os.path.basename(__file__) + " " + _u, file=_sys.stderr)
    raise SystemExit(1)
# ---- 守卫结束 ----


def _key(a): return hashlib.md5(a.encode()).hexdigest().encode()
def _dec(k,d):
    c=Cipher(algorithms.AES(k),modes.ECB()).decryptor(); return c.update(d)+c.finalize()
def _enc(k,d):
    c=Cipher(algorithms.AES(k),modes.ECB()).encryptor(); return c.update(d)+c.finalize()

def encrypt(obj, alias):
    t=json.dumps(obj,separators=(',',':'),ensure_ascii=False).encode()
    p=16-len(t)%16
    return base64.b64encode(base64.b64encode(_enc(_key(alias), t+bytes([p])*p))).decode()

def decrypt(ct, alias):
    raw=base64.b64decode(base64.b64decode(ct).decode())
    pt=_dec(_key(alias), raw)
    pad=pt[-1]
    if 0<pad<17 and pt[-pad:]==bytes([pad])*pad: pt=pt[:-pad]
    return pt.decode('utf-8','replace')

# alias 取值：curl -sk https://TARGET/web/simple.php | grep -o "alias = '[^']*'"
# 调用：
#   rd = encrypt({"url":"entrance/page/article/getArticle.json",
#                 "params":json.dumps({"code":"about"})}, ALIAS)
#   curl -sk -H 'Host: HOST' -H 'Content-Type: application/x-www-form-urlencoded' \
#        --data "requestData=$rd" https://IP:PORT/entrance/api

