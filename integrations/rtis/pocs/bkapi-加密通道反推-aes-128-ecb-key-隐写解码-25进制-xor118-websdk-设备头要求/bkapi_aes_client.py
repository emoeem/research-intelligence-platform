#!/usr/bin/env python3
"""破解 /bkapi AES-128-ECB 加密通道
1) key 隐写在主包：E_AES_API_KEY 常量经 Hn() 反混淆
   Hn(e,t=118,n=25): 以 chr(n+97)='z' 分割 -> int(part,25) ^ 118 -> chr
2) 请求体 = b64(AES-128-ECB-PKCS7( json{data,token,deviceId} ))
3) 响应同 key 解密
4) 必须带 WebSDK 生成的 4 个设备头：device_fingerprint / fp_version / sdk_version / sid
   缺则统一返回 {"status":"n","error":"安全性错误DD!","errorCode":2001}
"""
import json, time, base64, requests, urllib3
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
# ---- 参数守卫（无参数必须给出人话，不抛 traceback、不朝真实目标发包）----
import sys as _sys, os as _os
if len(_sys.argv) < 2:
    _u = '<参数...>'
    print("用法: " + _os.path.basename(__file__) + " " + _u, file=_sys.stderr)
    raise SystemExit(1)
# ---- 守卫结束 ----

urllib3.disable_warnings()

HOST  = "https://TARGET_HOST"
PROXY = {"http": "http://127.0.0.1:7899", "https": "http://127.0.0.1:7899"}  # 境外站需代理，且必须 HTTP/1.1
KEY   = b"664b25fb3e41c1bc"   # Hn("2ez2ez2gzkz2iz2hzgzkz2jzjz2gz2lzlz2lzkzl")

# --- 浏览器里取：await WebSDK.getCommonFields(["device_fingerprint","fp_version","sdk_version","sid"])
DEV = {"device_fingerprint": "FP", "fp_version": "3.0.0", "sdk_version": "1.1.8", "sid": "SID"}
TOKEN = "TOKEN_UID"        # localStorage bika_LSP 用 AES-ECB key="FLs3oA3p2a39Gnsm" 解密
DEVICE_ID = "DID"          # localStorage bika_DID 同 key 解密

def _ecb(key=KEY):
    return Cipher(algorithms.AES(key), modes.ECB())

def enc(obj, key=KEY):
    pt = json.dumps(obj, ensure_ascii=False, separators=(",", ":")).encode()
    p = 16 - len(pt) % 16
    pt += bytes([p]) * p
    e = _ecb(key).encryptor()
    return base64.b64encode(e.update(pt) + e.finalize()).decode()

def dec(raw, key=KEY):
    ct = base64.b64decode(raw)
    d = _ecb(key).decryptor()
    pt = d.update(ct) + d.finalize()
    return json.loads(pt[:-pt[-1]].decode())

def call(path, data=None, token=TOKEN, device_id=DEVICE_ID, headers=None):
    body = enc({"data": data if data is not None else "", "token": token, "deviceId": device_id})
    h = {"Content-Type": "text/plain", "deviceType": "h5",
         "time": time.strftime("%Y-%m-%d %H:%M:%S", time.gmtime(time.time() + 28800)),  # 服务端 +8h
         "version": "2.9.9", "Origin": HOST, "Referer": HOST + "/", **DEV}
    if headers: h.update(headers)
    r = requests.post(HOST + "/bkapi" + path, data=body, headers=h,
                      proxies=PROXY, verify=False, timeout=30)
    return r, (dec(r.text) if r.text else None)

    r, j = call("/system/info", {})
    print(json.dumps(j, ensure_ascii=False)[:2000])
