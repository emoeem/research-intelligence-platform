#!/usr/bin/env python3
"""解密 bi_bm_{ag,hall} 远程配置（bundle 硬编码密钥，供应商通用）。

算法（从 /v3/js/plugins-core.*.js 还原）:
    key     = base64decode(ut.aes.ag | ut.aes.hall)   # 32B -> AES-256
    payload = base64decode(HTTP body)
    iv      = payload[:12]; ct = payload[12:]          # GCM ciphertext||tag
    plain   = AES-256-GCM(key, iv, ct)  -> JSON

定位密钥：
    curl -sk https://<ag-host>/v3/js/plugins-core.<hash>.js \
      | grep -o 'aes:{hall:"[^"]*",ag:"[^"]*"}'
    # 同时给出 config/agConfig 的 5 个 COS/AWS 镜像地址

用法：
    python3 decode_config.py ag            # 自动试镜像
    python3 decode_config.py hall
    python3 decode_config.py raw <file.b64> ag
"""
import base64, json, ssl, sys, urllib.request
from cryptography.hazmat.primitives.ciphers.aead import AESGCM
# ---- 参数守卫（无参数必须给出人话，不抛 traceback、不朝真实目标发包）----
import sys as _sys, os as _os
if len(_sys.argv) < 2:
    _u = '用法：'
    print("用法: " + _os.path.basename(__file__) + " " + _u, file=_sys.stderr)
    raise SystemExit(1)
# ---- 守卫结束 ----


KEYS = {
    "ag":   "FSBdf1f+ANJS36Y1ae9HR0CphIVoJozhDxv8LY+rUBQ=",
    "hall": "92FFmaDnZq1Eb5NGa8jYoTMilq0o1Sf/hTmgcVZlwpo=",
}
MIRRORS = {
 "ag": ["https://cfg-cdno-prod-cq-1304426969.cos.ap-chongqing.myqcloud.com/bi_bm_ag/config.json",
        "https://cfg-cdno-prod-gz-1304426969.cos.ap-guangzhou.myqcloud.com/bi_bm_ag/config.json"],
 "hall": ["https://cfg-cdno-prod-cq-1304426969.cos.ap-chongqing.myqcloud.com/bi_bm_hall/config.json",
          "https://cfg-cdno-prod-gz-1304426969.cos.ap-guangzhou.myqcloud.com/bi_bm_hall/config.json"],
}

def decrypt(body: bytes, which: str) -> dict:
    key = base64.b64decode(KEYS[which]); n = base64.b64decode(body.strip())
    return json.loads(AESGCM(key).decrypt(n[:12], n[12:], None))

def fetch(url: str) -> bytes:
    ctx = ssl.create_default_context(); ctx.check_hostname = False; ctx.verify_mode = ssl.CERT_NONE
    return urllib.request.urlopen(url, timeout=20, context=ctx).read()

    mode = sys.argv[1] if len(sys.argv) > 1 else "ag"
    if mode == "raw":
        print(json.dumps(decrypt(open(sys.argv[2], "rb").read(), sys.argv[3]), ensure_ascii=False, indent=1)); raise SystemExit(0)
    for url in MIRRORS[mode]:
        try:
            print("[OK]", url); print(json.dumps(decrypt(fetch(url), mode), ensure_ascii=False, indent=1)); break
        except Exception as e:
            print("[FAIL]", url, type(e).__name__, e)
