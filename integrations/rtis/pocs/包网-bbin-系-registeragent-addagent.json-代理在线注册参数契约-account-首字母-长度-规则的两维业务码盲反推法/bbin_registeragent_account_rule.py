#!/usr/bin/env python3
# -*- coding: utf-8 -*-
import json,ssl,urllib.parse,urllib.request,time
# ---- 参数守卫（无参数必须给出人话，不抛 traceback、不朝真实目标发包）----
import sys as _sys, os as _os
if len(_sys.argv) < 2:
    _u = '<参数...>'
    print("用法: " + _os.path.basename(__file__) + " " + _u, file=_sys.stderr)
    raise SystemExit(1)
# ---- 守卫结束 ----

IP="TARGET_IP:PORT"; HOST="TARGET_HOST:PORT"
C=ssl.create_default_context(); C.check_hostname=False; C.verify_mode=ssl.CERT_NONE
def add(account, pw="Aa123456"):
    b=urllib.parse.urlencode({"account":account,"password":pw,"passwd":pw}).encode()
    req=urllib.request.Request("https://"+IP+"/infe/rest/registeragent/addAgent.json",data=b,method="POST",
        headers={"Host":HOST,"Content-Type":"application/x-www-form-urlencoded","User-Agent":"Mozilla/5.0"})
    with urllib.request.urlopen(req,context=C,timeout=30) as r:
        j=json.loads(r.read().decode())
    return j.get("code"), j.get("message"), (j.get("data") or {}).get("errorParam")

# 业务码语义（qihang88 模板）
# 111032502 请输入代理账号        (account 空)
# 111032518 账号格式错误          (规则不匹配)
# 111032531 此账号已经有人使用了  (规则通过 且 该账号已存在)  ← 格式判定的正信号
# 111032519 密码须符合0~9及a~z    (account 已过 -> 进入密码层)  ← 格式判定的正信号
# 111032538 您输入的密码不相同 / 111032504 currency / 111032510 rmNum / 111032511 agree / 111032540 验证码失效
#
# 第 1 步：固定长度 8，逐字母扫首字符（a-z，每个只发 1 次）
for L in "abcdefghijklmnopqrstuvwxyz":
    code,msg,ep = add(L+"1234567")
    if code != 111032518:
        print("FIRST LETTER =", L, code, msg); break
    time.sleep(0.9)
# 实测结果：仅 'd' 通过（qihang88 模板：d = 代理）
#
# 第 2 步：固定首字符 d，扫长度定位区间
#   4 位 -> 111032518(太短) ; 5~12 位 -> 111032519(通过) ; 13/15 位 -> 111032518(太长)
#   => account = ^d[a-z0-9]{4,11}$
#
# 第 3 步：密码与其余字段（实测校验顺序）
#   password = ^[a-z0-9]{6,12}$   （配置里的 genericPasswordRegex 允许大写，但此接口实际拒收）
#   passwd 必须与 password 一致
#   currency = RMB
#   agree    = Y
#   rmNum    = 图形验证码 —— 卡点：/infe/verify/macpic 同 session 同 SR 连取两次返回不同图片
#              （图片随机渲染，与校验代码不对应），纯 HTTP 无法通过；mkcode 的 32hex 也非 md5(code)
#
# 结论：account 规则可稳定反推；能否真正注册取决于该模板的验证码实现是否正常。

