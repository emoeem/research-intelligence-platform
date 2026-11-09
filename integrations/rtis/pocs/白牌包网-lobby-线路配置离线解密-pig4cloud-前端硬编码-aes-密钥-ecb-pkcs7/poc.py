#!/usr/bin/env python3
"""
白牌包网 lobby 线路配置离线解密（Pig4Cloud 前端硬编码密钥）
适用：data-template-type=lobby / dw-site 的静态站与 SSR 大厅
泄漏入口：<oss>/cocos/config_data.json  或  页面内 window.LOBBY_SITE_CONFIG.INJECT_DATA.ossGetSiteUrlConfig.data.data

算法（逆向自 commonChunk bundle）：
  key = window.atob("dGhhbmtzLH"+"BpZzRjbG91"+"ZA==")      = "thanks,pig4cloud"
  iv  = Utf8.parse("5421698523"+"412578")                  = "5421698523412578"  (ECB 下不使用)
  Sn.ossDecrypt -> ot.AES.decrypt(data, key, {mode: ot.mode.ECB, padding: ot.pad.Pkcs7})
  注意：外层 decrypt() 默认是 CBC/ZeroPadding，但被 ossDecrypt 组装的 {mode:ECB,padding:Pkcs7} 覆盖，
        实际生效的是 AES-128-ECB。这一点是能不能解开的关键。
"""
import sys, base64, json, urllib.parse

KEY = b"thanks,pig4cloud"
IV  = b"5421698523412578"

def decrypt(b64_text: str) -> dict:
    t = "".join(b64_text.split())
    t = t[: len(t) // 4 * 4]
    ct = base64.b64decode(t)
    from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
    dec = Cipher(algorithms.AES(KEY), modes.ECB()).decryptor()
    pt = dec.update(ct) + dec.finalize()
    pt = pt.rstrip(b"\x00")
    if pt and pt[-1] <= 16 and pt.endswith(bytes([pt[-1]]) * pt[-1]):
        pt = pt[: -pt[-1]]
    return json.loads(pt.decode("utf-8"))

def from_page(path: str) -> dict:
    d = open(path, "r", encoding="utf-8", errors="replace").read()
    import re
    m = re.search(r'ossGetSiteUrlConfig[^)]*?data\.data\)\s*\|\|\s*""', d)
    m2 = re.findall(r'atob\("([A-Za-z0-9+/=]{300,})@', d)
    for b in m2:
        try:
            obj = json.loads(urllib.parse.unquote(base64.b64decode(b).decode("utf-8", "replace")))
            s = obj.get("INJECT_DATA", {}).get("ossGetSiteUrlConfig", {}).get("data", {}).get("data")
            if s:
                return decrypt(s)
        except Exception:
            continue
    raise SystemExit("未在页面中找到可解密的配置")

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print(__doc__); sys.exit(1)
    src = sys.argv[1]
    if src.startswith("http"):
        import urllib.request, ssl
        ctx = ssl.create_default_context(); ctx.check_hostname = False; ctx.verify_mode = ssl.CERT_NONE
        raw = urllib.request.urlopen(src, timeout=20, context=ctx).read().decode("utf-8", "replace")
        out = decrypt(raw)
    elif src.endswith((".html", ".htm")):
        out = from_page(src)
    else:
        out = decrypt(open(src, encoding="utf-8", errors="replace").read())
    print(json.dumps(out, ensure_ascii=False, indent=1))
