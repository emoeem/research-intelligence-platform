#!/usr/bin/env python3
"""小程序 WxEncryptVersion 响应加密降级：原样重放 Reqable 捕获记录，取明文响应。
用法: python3 wxenc_downgrade_replay.py rec_5883.json
"""
import json, ssl, http.client, sys
from urllib.parse import urlparse
# ---- 参数守卫（无参数必须给出人话，不抛 traceback、不朝真实目标发包）----
import sys as _sys, os as _os
if len(_sys.argv) < 2:
    _u = '用法: python3 wxenc_downgrade_replay.py rec_5883.json'
    print("用法: " + _os.path.basename(__file__) + " " + _u, file=_sys.stderr)
    raise SystemExit(1)
# ---- 守卫结束 ----


CTX = ssl._create_unverified_context()

def replay(rec):
    u = urlparse(rec['url'])
    path = u.path + ('?' + u.query if u.query else '')
    h = {}
    for x in rec['request']['headers']:
        lk = x['name'].lower()
        # 关键：丢掉 accept-encoding，让客户端发 identity —— 这是降级到明文的条件
        if lk in ('host', 'content-length', 'accept-encoding', 'connection'):
            continue
        h[x['name']] = x['value']
    b = rec['request'].get('body')
    body = b.get('text') if isinstance(b, dict) else (b if isinstance(b, str) else None)
    if body:
        h.setdefault('Content-Type', 'application/x-www-form-urlencoded')
    c = http.client.HTTPSConnection(u.hostname, timeout=40, context=CTX)
    c.request(rec['request']['method'], path, body=body, headers=h)
    r = c.getresponse()
    raw = r.read()
    ce = r.getheader('content-encoding')
    c.close()
    if ce == 'br':
        import brotli; raw = brotli.decompress(raw)
    elif ce == 'gzip':
        import gzip; raw = gzip.decompress(raw)
    return r.status, raw.decode('utf-8', 'replace')

if __name__ == '__main__':
    rec = json.load(open(sys.argv[1]))
    st, txt = replay(rec)
    enc = '&WxEncryptVersion' in txt
    print(f'HTTP {st}  len={len(txt)}  encrypted={enc}')
    print(txt[:2000])

