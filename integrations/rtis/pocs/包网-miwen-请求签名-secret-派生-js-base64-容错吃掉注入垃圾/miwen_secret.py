#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
miWen -> DECRYPT_SECRET 派生器（同一供应商，跨站通用）
=====================================================
算法来源（前端 bundle）：
  模�� e762 = js-base64 3.9.4（Base64.decode 会先剔除非法字符 → miWen 里的
  'AFSFSFN33sT'/'ASFDASFD'/'sfeAS' 之类注入垃圾被自动丢弃）

    var decoded = Base64.decode(site.miWen);      // 明文首 16 字符 = 密钥串
    var key16   = decoded.substr(0, 16);
    var rest    = decoded.substr(16);             // 再一层 base64 密文
    var plain   = CryptoJS.AES.decrypt(
                    rest,
                    CryptoJS.SHA256(key16),
                    { iv: CryptoJS.enc.Hex.parse('0000000000000000'),
                      mode: CryptoJS.mode.CBC, padding: CryptoJS.pad.Pkcs7 }
                  ).toString(CryptoJS.enc.Utf8);
    // 每 13 字符切一条 → DECRYPT_SECRET[]
    // 运行时： SECRET = DECRYPT_SECRET[ 时间片下标 + site.keyMove ]

用法：
    python3 miwen_secret.py miwen.txt
    python3 miwen_secret.py --miwen '<miWen 原串>'
"""
import re, sys, base64, hashlib, argparse
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
# ---- 参数守卫（无参数必须给出人话，不抛 traceback、不朝真实目标发包）----
import sys as _sys, os as _os
if len(_sys.argv) < 2:
    _u = '用法：'
    print("用法: " + _os.path.basename(__file__) + " " + _u, file=_sys.stderr)
    raise SystemExit(1)
# ---- 守卫结束 ----



def clean_b64(s: str) -> str:
    """复刻 js-base64 3.x 的容错：剔除 [A-Za-z0-9+/] 之外的字符"""
    c = re.sub(r'[^A-Za-z0-9+/]', '', s)
    return c + '=' * (-len(c) % 4)


def _b64_tolerant(s: str):
    """js-base64 对长度 %4==1 的串会给空结果；这里退一步裁到合法长度"""
    c = re.sub(r'[^A-Za-z0-9+/]', '', s)
    while len(c) % 4 == 1:
        c = c[:-1]
    return base64.b64decode(c + '=' * (-len(c) % 4))


def derive(miwen: str):
    try:
        raw = _b64_tolerant(miwen)
    except Exception:
        return None
    text = raw.decode('utf-8', 'ignore')
    key16, rest = text[:16], text[16:]
    if len(key16) != 16:
        return None
    try:
        ctr = _b64_tolerant(rest)
    except Exception:
        return None
    ctr = ctr[:len(ctr) // 16 * 16]
    if not ctr:
        return None
    key = hashlib.sha256(key16.encode()).digest()
    d = Cipher(algorithms.AES(key), modes.CBC(b'\x00' * 16)).decryptor()
    pt = d.update(ctr) + d.finalize()
    n = pt[-1]
    ok = 1 <= n <= 16 and pt[-n:] == bytes([n]) * n
    body = (pt[:-n] if ok else pt).decode('utf-8', 'ignore')
    secrets = [body[i:i + 13] for i in range(0, len(body), 13)]
    return {'key16': key16, 'padding_ok': ok, 'plain_len': len(body), 'secrets': secrets}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('file', nargs='?')
    ap.add_argument('--miwen')
    a = ap.parse_args()
    items = [a.miwen] if a.miwen else [l.strip() for l in open(a.file) if l.strip()]
    for i, w in enumerate(items, 1):
        r = derive(w)
        if not r:
            print(f"miWen#{i}: 解不开（头部不是 16 字符密钥串）")
            continue
        print(f"miWen#{i}: key16={r['key16']} padding_ok={r['padding_ok']} "
              f"plain={r['plain_len']}B SECRET={len(r['secrets'])} 条")
        print("  " + " | ".join(f"[{j}]{s}" for j, s in enumerate(r['secrets'])))


if __name__ == '__main__':
    main()

