#!/usr/bin/env python3
r"""公众号/小程序 appid-secret 泄露面静态扫描（模板，未针对任何真实目标验证）。

判据：对本地目录（小程序解包产物 / H5 web bundle / 前端源码 / GitHub clone 目录）做正则扫描：
  - appid:       \bwx[0-9a-f]{16}\b（公众号/小程序同一格式，按上下文 mptype 判）
  - appsecret:   (app_?secret|AppSecret|secret_key)\s*[:=]\s*["'][0-9a-f]{32}["']
  - 阿里云 AK:   \bLTAI[0-9A-Za-z]{12,28}\b（含 LTAI4F/LTAI5T 新前缀）
  - 腾讯云 AK:   \bAKID[0-9A-Za-z]{13,20}\b
  - 通用 key:    (api_?key|access_?key|access_?token)\s*[:=]\s*["'][A-Za-z0-9_-]{24,}["']
  - JS-SDK 签名: (signature|jsapi_ticket)\s*[:=] 上下文行（可伪造分享面，见 miniprogram-official-account 技能）
  高价值链路：appid + appsecret 同现 ⇒ 可换 access_token ⇒ 群发/客服/素材/用户信息接口全开（IP 白名单为空或含攻击者可控 IP 时）。

用法：
  python3 scan.py <目录> [-o leak-report.json]
  典型：python3 scan.py ./unpacked-miniapp ./dist-web

纪律：
  - 纯本地静态扫描，零网络行为；扫描对象须来自授权评估
  - 命中的真实 secret 在报告/knowledge.db 中脱敏（wxXXXXXXXXXXXXXXXX / secret=***）；验证 secret 有效性
    属于主动利用（换 token），必须确认授权后进行，且只调只读接口（如获取账户信息）证明危害
"""
import argparse
import json
import os
import re

PATTERNS = [
    ("appid",       re.compile(r"\bwx[0-9a-fA-F]{16}\b"), "微信 appid（小程序/公众号）"),
    ("appsecret",   re.compile(r"(?i:app_?secret|secret_?key)[\"'\s:={\[]+([0-9a-fA-F]{32})"), "appsecret 明文"),
    ("aliyun-ak",   re.compile(r"\bLTAI[0-9A-Za-z]{12,28}\b"), "阿里云 AccessKeyId"),
    ("tencent-ak",  re.compile(r"\bAKID[0-9A-Za-z]{13,20}\b"), "腾讯云 SecretId"),
    ("generic-key", re.compile(r"(?i:api_?key|access_?key|access_?token|client_?secret)[\"'\s:={\[]+[A-Za-z0-9_\-]{24,}"), "通用 key 形态"),
    ("jssdk-sign",  re.compile(r"(?:jsapi_ticket|signature)[\"'\s:={\[]+[A-Za-z0-9_\-]{8,}"), "JS-SDK 签名上下文"),
]


def scan(root: str):
    hits = []
    for base, _dirs, files in os.walk(root):
        for fn in files:
            p = os.path.join(base, fn)
            if os.path.getsize(p) > 10 * 1024 * 1024:
                continue
            try:
                text = open(p, encoding="utf-8", errors="replace").read()
            except OSError:
                continue
            for cat, pat, _desc in PATTERNS:
                for m in pat.finditer(text):
                    line = text.count("\n", 0, m.start()) + 1
                    frag = text[max(0, m.start() - 30): m.end() + 30].replace("\n", " ")
                    hits.append({"cat": cat, "file": os.path.relpath(p, root), "line": line, "frag": frag[:200]})
    return hits


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("dirs", nargs="+")
    ap.add_argument("-o", "--out", default="leak-report.json")
    a = ap.parse_args()

    report = {}
    total = 0
    for d in a.dirs:
        hits = scan(d)
        report[d] = hits
        total += len(hits)
        by = {}
        for h in hits:
            by[h["cat"]] = by.get(h["cat"], 0) + 1
        print(f"[{d}] {len(hits)} 条命中: {by}")
        # appid+secret 同文件同现 → 高价值告警
        appid_files = {h["file"] for h in hits if h["cat"] == "appid"}
        secret_files = {h["file"] for h in hits if h["cat"] == "appsecret"}
        pair = appid_files & secret_files
        if pair:
            print(f"  ⚠ appid+secret 同文件同现（可直接换 token 验证，先确认授权）: {sorted(pair)}")
    json.dump(report, open(a.out, "w"), ensure_ascii=False, indent=1)
    print(f"共 {total} 条 → {a.out}")


if __name__ == "__main__":
    main()
