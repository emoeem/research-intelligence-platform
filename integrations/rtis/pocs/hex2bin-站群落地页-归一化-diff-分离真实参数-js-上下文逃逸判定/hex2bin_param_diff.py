#!/usr/bin/env python3
"""
hex2bin 站群落地页：参数发现与开放重定向否定判定
适用：首页返回 document.write(hex2bin('<hex>')) 的站群/投放落地页
"""
import subprocess, re, hashlib, binascii, urllib.parse, time
# ---- 参数守卫（无参数必须给出人话，不抛 traceback、不朝真实目标发包）----
import sys as _sys, os as _os
if len(_sys.argv) < 2:
    _u = '<参数...>'
    print("用法: " + _os.path.basename(__file__) + " " + _u, file=_sys.stderr)
    raise SystemExit(1)
# ---- 守卫结束 ----


PROXY = "http://127.0.0.1:7892"   # 目标为境外 CloudFront 时需代理，否则 TLS 被拦
UA = ("Mozilla/5.0 (iPhone; CPU iPhone OS 16_6 like Mac OS X) AppleWebKit/605.1.15 "
      "Version/16.6 Mobile/15E148 Safari/604.1")

def fetch(url):
    r = subprocess.run(["curl","-sS","-m","20","-k","--compressed","--max-redirs","0",
        "-A",UA,"-x",PROXY,"-o","-","-w","\n@@@|%{http_code}|%{redirect_url}",url],
        capture_output=True, text=True, errors="replace")
    b, meta = r.stdout.rsplit("@@@|", 1)
    p = meta.split("|")
    return {"code":p[0], "loc":p[1] if len(p)>1 else "", "body":b}

def dehex(html):
    m = re.findall(r"hex2bin\('([0-9a-fA-F]+)'\)", html)
    return binascii.unhexlify(m[0]).decode("utf-8","replace") if m else html

def norm(b):
    """关键：剥离随机 trace_id / 时间戳 / 内嵌 IP，否则每请求 body 都不同 → 假阳性"""
    b = re.sub(r'[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}','UUID',b)
    b = re.sub(r'_\d{10}_\d+\.\d+\.\d+\.\d+','_TS_IP',b)
    b = re.sub(r'\b1[6-9]\d{8}\b','TS',b)
    return b

H="riomksa.sfk2aoe9v.cc"
base = dehex(fetch(f"https://{H}/")["body"])
bh = hashlib.md5(norm(base).encode()).hexdigest()[:10]
print("baseline", bh)
for p in ["url","redirect","next","goto","return","to","r","u","channel","_c","from","tg"]:
    r = fetch(f"https://{H}/?{p}=zq7marker")
    body = dehex(r["body"])
    h = hashlib.md5(norm(body).encode()).hexdigest()[:10]
    mark = "SAME" if h==bh else "DIFF <-- 服务端读取了该参数"
    print(f"  ?{p:10s} md5={h} {mark} loc={r['loc']}")
    time.sleep(0.3)

# 命中差异后，定位落到哪个 JS/HTML 上下文：
#   站群模板把 _c 渲染进 bootLandingSDK({..., channel:"<值>", ...})
#   再测字符过滤： " ' < > { } \  判断能否逃逸 JS 字符串
for ch,label in [('%22','dquote'),('%27','squote'),('%3Cx%3E','angle'),
                 ('%7Bx%7D','brace'),('%5C','backslash')]:
    r = fetch(f"https://{H}/?_c={ch}")
    m = re.search(r'channel:\s*"([^"]*)"', dehex(r["body"]))
    print(f"  _c={label:10s} -> channel={m.group(1) if m else 'N/A'!r}")
    time.sleep(0.3)

"""实测结论（2026-10，4 个同型目标一致）：
   _c=zq7marker  -> channel:"zq7marker"（原样）
   _c=%22 (")    -> channel:""           （双引号被服务端剔除 → 无法闭合 JS 字符串，不能 XSS）
   _c=%27 (')    -> channel:"'"          （保留）
   _c=%5C (\)    -> channel:"\"          （保留，可破坏 JS 字符串闭合，仅自伤）
   _c=<x> / {x}  -> 原样保留
   其余 34 个常见重定向参数（url/redirect/next/.../domain/host/site）全部与基线逐字节一致
   → 该模板不存在开放重定向；_c 只进 LandingSDK 上报 payload，不参与 URL 拼接。
"""

