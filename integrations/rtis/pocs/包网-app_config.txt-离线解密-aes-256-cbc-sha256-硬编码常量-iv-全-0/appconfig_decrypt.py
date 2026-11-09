#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
白牌包网(供应商 cg / "cgwss3" 一系) app_config.txt 离线解密器
================================================================
算法来源（前端 bundle 模块 29e6，导出 a = _0x1a10ee）：

    function decrypt(cipherBase64) {
      return CryptoJS.AES.decrypt(
        cipherBase64.toString(),
        CryptoJS.SHA256(state.sdkmin.aesKeyyunxin),   // 32 字节 → AES-256
        { iv:   CryptoJS.enc.Hex.parse('0000000000000000'),  // 16 个 hex 字符 = 8 字节
                                                            // CryptoJS 取 iv.words[0..3]
                                                            // 越界为 0 → 实际 IV = 16 字节全 0
          mode: CryptoJS.mode.CBC,
          padding: CryptoJS.pad.Pkcs7 }
      ).toString(CryptoJS.enc.Utf8);
    }

硬编码密钥：aesKeyyunxin = '57feaa48112bc0e7'（bundle 字符串表 0x144d，全站同一常量）

用法：
    python3 appconfig_decrypt.py <URL 或本地文件> [更多...]
    python3 appconfig_decrypt.py --key 57feaa48112bc0e7 --file acfg.txt --raw
"""
import re, sys, json, base64, hashlib, ssl, argparse, urllib.request
# ---- 参数守卫（无参数必须给出人话，不抛 traceback、不朝真实目标发包）----
import sys as _sys, os as _os
if len(_sys.argv) < 2:
    _u = '用法：'
    print("用法: " + _os.path.basename(__file__) + " " + _u, file=_sys.stderr)
    raise SystemExit(1)
# ---- 守卫结束 ----


try:
    from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
except ImportError:
    sys.exit("需要 cryptography:  pip install cryptography")

DEFAULT_KEY = '57feaa48112bc0e7'
CIPHER_RE = re.compile(r'\{"config":"(.*?)"\}', re.S)


def decrypt_config(cfg_text: str, keystr: str = DEFAULT_KEY) -> str | None:
    """cfg_text = app_config.txt 全文（JSONP 包装）; 返回解密后的 JSON 字符串"""
    m = CIPHER_RE.search(cfg_text)
    if not m:
        return None
    ct = base64.b64decode(m.group(1).replace('\\/', '/'))
    key = hashlib.sha256(keystr.encode()).digest()
    d = Cipher(algorithms.AES(key), modes.CBC(b'\x00' * 16)).decryptor()
    pt = d.update(ct) + d.finalize()
    n = pt[-1]
    if 1 <= n <= 16 and pt[-n:] == bytes([n]) * n:
        pt = pt[:-n]
    try:
        return pt.decode('utf-8')
    except UnicodeDecodeError:
        return None


def fetch(url: str) -> str:
    ctx = ssl.create_default_context()
    ctx.check_hostname = False
    ctx.verify_mode = ssl.CERT_NONE
    req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
    return urllib.request.urlopen(req, timeout=15, context=ctx).read().decode('utf-8', 'replace')


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('targets', nargs='*', help='URL 或本地文件路径')
    ap.add_argument('--key', default=DEFAULT_KEY)
    ap.add_argument('--raw', action='store_true', help='只输出解密后的原文')
    a = ap.parse_args()

    for t in a.targets:
        body = open(t, encoding='utf-8', errors='replace').read() if '://' not in t else fetch(t)
        pt = decrypt_config(body, a.key)
        if not pt:
            print(f"[FAIL] {t}", file=sys.stderr)
            continue
        if a.raw:
            print(pt)
            continue
        print(f"[OK] {t}")
        try:
            o = json.loads(pt)['root']['result']
            print(json.dumps(o, ensure_ascii=False, indent=1)[:2000])
        except Exception:
            print(pt[:2000])


if __name__ == '__main__':
    main()

