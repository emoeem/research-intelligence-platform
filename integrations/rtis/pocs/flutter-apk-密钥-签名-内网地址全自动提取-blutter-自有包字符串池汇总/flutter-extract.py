#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Flutter APK：密钥 / 签名 / 内网地址提取

为什么单独一个脚本：Flutter 的 libapp.so 是 Dart AOT 快照，
  · strings 能出东西，但顺序杂乱、有大量噪音（Dart 运行时符号）
  · 真正的业务字符串在 **对象池（object pool）** 里，需要解析快照才能定位
  · blutter 是最强的工具，但要自己编译、依赖 CMake + Dart SDK

本脚本是**纯 Python 兜底**：
  1. 从 APK(zip) 里取出 lib/*/libapp.so 与 assets/flutter_assets/
  2. 用「可打印串 + 上下文窗口」的方式提取候选
  3. 按类别归一化：域名 / IP / 密钥形态 / 签名算法 / API 路径
  4. 与 Dart 运行时符号做差集（减掉噪音）

如果机器上有 blutter，脚本会提示用它做精确解析。

用法：
  ./flutter-extract.py <app.apk|app.aab> [--out ./flutter-out] [--blutter /path/to/blutter]
示例：
  ./flutter-extract.py ./target.apk --out ./fx
"""
import os
import re
import subprocess
import sys
import zipfile

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "_lib"))
import common as C  # noqa: E402

# Dart 运行时/框架自身的符号，用来做噪音减法
DART_NOISE = re.compile(
    r'^(?:dart:|package:flutter/|package:sky_engine|_?[Zz]one|_?[Ii]solate|'
    r'_[A-Za-z]+@[0-9]+|(?:get|set):|.*\.dart$|.*\.g\.dart$)')

# 分类规则
RULES = [
    ("ip_private", re.compile(rb'\b(?:10|172|192)\.(?:\d{1,3}\.){2}\d{1,3}\b')),
    ("ip_public",  re.compile(rb'\b(?:(?!10\.|127\.|0\.|169\.254\.|192\.168\.|172\.(?:1[6-9]|2\d|3[01])\.)\d{1,3}\.){3}\d{1,3}\b')),
    ("url",        re.compile(rb'https?://[A-Za-z0-9._~:/?#\[\]@!$&\'()*+,;=%-]{6,200}')),
    ("domain",     re.compile(rb'\b(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+(?:com|cn|net|org|io|top|xyz|vip|cc|co|me|app|dev|cloud|site|info|shop|live|tv|link)\b')),
    ("jwt",        re.compile(rb'eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}')),
    ("pem",        re.compile(rb'-----BEGIN [A-Z ]*PRIVATE KEY-----')),
    ("aws_ak",     re.compile(rb'\b(?:AKIA|ASIA)[A-Z0-9]{16}\b')),
    ("aliyun_ak",  re.compile(rb'\bLTAI[A-Za-z0-9]{12,20}\b')),
    ("hex32",      re.compile(rb'\b[0-9a-f]{32}\b')),
    ("apikey_kv",  re.compile(rb'(?i)(?:api[_-]?key|app[_-]?key|secret|sign[_-]?key|aes[_-]?key)["\']?\s*[:=]\s*["\']?([A-Za-z0-9+/=_-]{8,64})')),
    ("api_path",   re.compile(rb'/(?:api|v\d+|rest|graphql)/[A-Za-z0-9_/{}.-]{2,80}')),
    ("sign_algo",  re.compile(rb'(?i)(?:hmac[_-]?sha(?:1|256|512)|md5|sha1|sha256|rsa|aes[_-]?(?:cbc|ecb|gcm)|sm[234])')),
]

MAX_SO = 300 * 1024 * 1024


def find_blutter():
    for p in ("blutter", "/opt/blutter/blutter.py", os.path.expanduser("~/blutter/blutter.py")):
        if os.path.sep in p:
            if os.path.exists(p):
                return p
        else:
            from shutil import which
            w = which(p)
            if w:
                return w
    return None


def strings_of(blob: bytes, minlen=6):
    """提取可打印 ASCII 串。比 `strings` 多保留 UTF-8 中文（Flutter 中文串很多）。"""
    out = []
    cur = bytearray()
    for b in blob:
        if 0x20 <= b < 0x7F or b >= 0x80:
            cur.append(b)
        else:
            if len(cur) >= minlen:
                out.append(bytes(cur))
            cur = bytearray()
    if len(cur) >= minlen:
        out.append(bytes(cur))
    return out


def main():
    if len(sys.argv) < 2:
        C.usage_die(__doc__.strip().split("用法：")[0] + "用法：\n  ./flutter-extract.py <app.apk> [--out DIR]\n示例：\n  ./flutter-extract.py ./target.apk --out ./fx")

    apk = sys.argv[1]
    out_dir = "./flutter-out"
    if "--out" in sys.argv:
        out_dir = sys.argv[sys.argv.index("--out") + 1]

    if not os.path.isfile(apk):
        C.err(f"文件不存在: {apk}")
        sys.exit(1)
    os.makedirs(out_dir, exist_ok=True)

    C.hdr("0. 环境")
    blutter = find_blutter()
    if blutter:
        C.ok(f"检测到 blutter: {blutter}")
        C.dim("blutter 能做精确的 Dart 对象池解析，比本脚本的字符串扫描准得多")
    else:
        C.dim("未检测到 blutter，使用纯 Python 字符串扫描（噪音较多但可用）")

    C.hdr("1. 从 APK 取 libapp.so 与 flutter_assets")
    so_paths = []
    assets = []
    try:
        with zipfile.ZipFile(apk) as z:
            names = z.namelist()
            so_paths = [n for n in names if n.startswith("lib/") and n.endswith("libapp.so")]
            assets = [n for n in names if n.startswith("assets/flutter_assets/")]
            C.dim(f"libapp.so: {len(so_paths)} 个；flutter_assets: {len(assets)} 个")
            if not so_paths:
                C.warn("没有 libapp.so → 可能不是 Flutter 应用，或用了 AAB 分包")
                C.verdict("flutter-extract", "NOT_FLUTTER", "无 libapp.so")
                return 2

            blobs = []
            for n in so_paths:
                info = z.getinfo(n)
                if info.file_size > MAX_SO:
                    C.dim(f"跳过（>{MAX_SO//1024//1024}MB）: {n}")
                    continue
                C.dim(f"读取 {n} ({info.file_size/1024/1024:.1f} MB)")
                blobs.append((n, z.read(n)))

            # flutter_assets 里常有 AssetManifest.json / kernel_blob.bin
            kernel = [n for n in assets if n.endswith("kernel_blob.bin")]
            for n in kernel:
                info = z.getinfo(n)
                if info.file_size <= MAX_SO:
                    C.dim(f"读取 {n} ({info.file_size/1024/1024:.1f} MB)")
                    blobs.append((n, z.read(n)))
    except zipfile.BadZipFile:
        C.err("不是合法 zip/apk")
        sys.exit(1)

    C.hdr("2. 字符串提取与分类")
    allstr = []
    for name, blob in blobs:
        ss = strings_of(blob)
        C.dim(f"{os.path.basename(name)}: {len(ss)} 个字符串")
        allstr.extend(ss)

    C.dim(f"合计 {len(allstr)} 个字符串")

    hits = {}
    for s in allstr:
        for kind, pat in RULES:
            for m in pat.finditer(s):
                v = m.group(0) if not m.groups() else m.group(1)
                hits.setdefault(kind, set()).add(v)

    C.hdr("3. 结果")
    order = ["ip_private", "url", "domain", "jwt", "pem", "aws_ak", "aliyun_ak",
             "apikey_kv", "api_path", "sign_algo", "hex32"]
    for kind in order:
        vals = hits.get(kind, set())
        if not vals:
            continue
        # 噪音减法：去掉 Dart 框架自己的域名/路径
        clean = set()
        for v in vals:
            sv = v.decode("utf-8", "replace") if isinstance(v, bytes) else v
            if DART_NOISE.match(sv):
                continue
            if any(x in sv for x in ("dart.dev", "flutter.dev", "googleapis.com",
                                     "gstatic.com", "schema.org", "w3.org",
                                     "apache.org", "openssl.org", "chromium.org")):
                continue
            clean.add(sv)
        if not clean:
            continue
        C.warn(f"{kind}: {len(clean)} 条（原始 {len(vals)}）")
        for v in sorted(clean)[:15]:
            C.dim(f"   {v[:120]}")
        if len(clean) > 15:
            C.dim(f"   …还有 {len(clean)-15} 条")

    # 落盘
    dump = {k: sorted(v.decode("utf-8", "replace") if isinstance(v, bytes) else v
                      for v in vs) for k, vs in hits.items()}
    C.dump_json(dump, os.path.join(out_dir, "extracted.json"))

    C.hdr("结论")
    priv = hits.get("ip_private", set())
    if priv:
        C.warn(f"发现 {len(priv)} 个内网 IP —— 这是最有价值的产出")
        C.dim("内网 IP 说明该包是内网测试版泄漏出来的，可据此推断网段结构")
    if hits.get("pem") or hits.get("aws_ak") or hits.get("aliyun_ak"):
        C.warn("发现私钥 / 云 AK —— 立即单独存档，并做有效性验证（只做只读调用）")

    print("""
判读注意（这三条能挡掉大部分假阳性）：
  1) hex32 里 90% 是 Flutter 引擎自己的 hash，不是密钥。只有当它出现在
     apikey_kv 的上下文里才算候选
  2) domain 里大量是 pub.dev / storage.googleapis.com 这类依赖源，已做减法
  3) api_path 可能是 Dart 标准库的错误信息路径，逐条验证是否真的能请求

blutter 更彻底的做法（推荐在拿到 APK 后跑一次）：
  git clone https://github.com/worawit/blutter && cd blutter
  python3 blutter.py <解包后的 lib/arm64-v8a 目录> <输出目录>
  # 产出 asm/ 与 pp.txt，pp.txt 是对象池，业务字符串全在里面
""" if blutter is None else """
blutter 已就绪，建议：
  python3 blutter.py <解包目录>/lib/arm64-v8a <out>
然后对比 out/pp.txt 与本脚本的 extracted.json，blutter 的上下文更完整
""")

    total = sum(len(v) for v in hits.values())
    C.verdict("flutter-extract", "DONE" if total else "CLEAN",
              f"{total} 条命中 / {len(allstr)} 字符串")
    return 0


if __name__ == "__main__":
    sys.exit(C.main_guard(main)())
