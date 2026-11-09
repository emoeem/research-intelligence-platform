#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
wxapkg 全自动解包 + 敏感信息提取

不依赖 npm / unveilr：纯标准库实现，解决「机器上跑不了 node 工具」的场景。

格式（微信小程序包，官方无文档，以下是逆向得到的稳定结构）：

    offset 0x00  1 byte   0xBE
    0x01-0x04    4 bytes  info
    0x05-0x08    4 bytes  fileLength   （整个包的长度，大端）
    0x09-0x0C    4 bytes  dataLength   （data 段长度）
    0x0D         1 byte   0xED
    0x0E-...     ...      data 段
    ...          index 段  （紧接 data 段，位于文件末尾）

    index 段：
      4 bytes  文件数 n
      循环 n 次：
        4 bytes  nameLen
        nameLen  文件名（UTF-8，相对路径）
        4 bytes  offset（相对 data 段起点）
        4 bytes  size

坑：不同版本的 header 变体有好几种（部分包在 0x0E 处多 4 字节校验）。
    硬编码一个偏移会在某些包上解析出乱码。所以本脚本**自适应**：
    先试几个候选的 index 起点，用「所有文件名合法 + offset+size 不越界」
    来验证，第一个通过的才采用。

用法：
  ./wxapkg-unpack.py <xxx.wxapkg|包所在目录> [--out ./unpacked] [--scan]
示例：
  ./wxapkg-unpack.py ./pkg/ --out ./out --scan
"""
import json
import os
import re
import struct
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "_lib"))
import common as C  # noqa: E402

MAGIC = 0xBE
# 候选的 index 段起点偏移（按实战出现频率排序）
IDX_START_CANDIDATES = [0x0E, 0x12, 0x0D, 0x16, 0x1A]


def u32(b, off):
    if off + 4 > len(b):
        raise ValueError("越界")
    return struct.unpack(">I", b[off:off + 4])[0]


def parse_index(buf, idx_start):
    """尝试从 idx_start 解析 index 段。返回 (files, data_start) 或 None。"""
    try:
        n = u32(buf, idx_start)
    except ValueError:
        return None
    if n <= 0 or n > 20000:      # 文件数合理性。超过 2 万几乎肯定是解析错了
        return None

    pos = idx_start + 4
    files = []
    for _ in range(n):
        try:
            nlen = u32(buf, pos)
        except ValueError:
            return None
        if nlen == 0 or nlen > 512 or pos + 4 + nlen + 8 > len(buf):
            return None
        pos += 4
        raw = buf[pos:pos + nlen]
        pos += nlen
        try:
            name = raw.decode("utf-8")
        except UnicodeDecodeError:
            return None
        # 文件名必须像路径：不含控制字符、不含 ../
        if not name or any(ord(ch) < 0x20 for ch in name) or ".." in name:
            return None
        try:
            off = u32(buf, pos); size = u32(buf, pos + 4)
        except ValueError:
            return None
        pos += 8
        files.append((name, off, size))

    if not files:
        return None
    # 验证：最大的 offset+size 必须落在 data 段内（data 段 = idx_start 之前的区域）
    max_end = max(o + s for _, o, s in files)
    if max_end > idx_start:
        return None
    # 验证：文件名里至少有一个 .js（小程序包必有）
    if not any(f[0].endswith((".js", ".json", ".html", ".wxml")) for f in files):
        return None
    return files, max_end


def unpack(path, out_dir):
    with open(path, "rb") as f:
        buf = f.read()
    if not buf or buf[0] != MAGIC:
        C.err(f"{os.path.basename(path)}: 不是 wxapkg（首字节 0x{buf[0]:02X} != 0xBE）")
        return None

    file_len = u32(buf, 5)
    if file_len != len(buf):
        C.dim(f"  header 声明长度 {file_len} != 实际 {len(buf)}（部分包如此，继续）")

    parsed = None
    used = None
    for cand in IDX_START_CANDIDATES:
        # index 段起点 = 文件尾 - indexLen，但我们不知道 indexLen，
        # 所以反过来：从候选 data 段末尾开始找。这里用「文件尾倒推」：
        # index 段长度 = 文件尾 - data 段末尾。先用 dataLength 算 data 末尾。
        data_len = u32(buf, 9)
        idx_guess = 0x0D + 1 + data_len
        for start in (idx_guess, cand, len(buf) - (len(buf) - idx_guess)):
            if start <= 0 or start >= len(buf):
                continue
            r = parse_index(buf, start)
            if r:
                parsed, used = r, start
                break
        if parsed:
            break

    if not parsed:
        # 最后兜底：全文件扫一遍，找能解析出合法 index 的位置
        for start in range(0x08, min(len(buf) - 8, 0x4000)):
            r = parse_index(buf, start)
            if r:
                parsed, used = r, start
                break

    if not parsed:
        C.err(f"{os.path.basename(path)}: index 段解析失败（格式可能是 wxapkg v2 或已加密）")
        return None

    files, _ = parsed
    C.ok(f"{os.path.basename(path)}: index 起点 0x{used:X}，{len(files)} 个文件")

    written = 0
    for name, off, size in files:
        data_start = 0x0D + 1   # data 段起点
        # 有些包 data 段起点随 header 变体偏移，用「offset 最大值」反推：
        # 若 off+size <= len(buf) - used，则 data_start 取上述默认值即可
        chunk = buf[data_start + off: data_start + off + size]
        if len(chunk) != size:
            C.warn(f"  截断: {name} 期望 {size} 实得 {len(chunk)}")
        dst = os.path.join(out_dir, name)
        os.makedirs(os.path.dirname(dst) or out_dir, exist_ok=True)
        with open(dst, "wb") as f:
            f.write(chunk)
        written += 1

    C.ok(f"  解出 {written} 个文件 → {out_dir}")
    return written


# ---- 敏感信息提取 --------------------------------------------------------
PATTERNS = [
    ("appid",      re.compile(r'wx[0-9a-f]{16}')),
    ("appsecret",  re.compile(r'(?i)\bapp[_-]?secret["\']?\s*[:=]\s*["\']([0-9a-f]{32})["\']')),
    ("api_base",   re.compile(r'https?://[a-zA-Z0-9._-]{4,60}(?:/[a-zA-Z0-9._/-]{0,80})?')),
    ("aes_key",    re.compile(r'(?i)\b(?:aes|encrypt)?_?key["\']?\s*[:=]\s*["\']([^"\']{8,64})["\']')),
    ("aes_iv",     re.compile(r'(?i)\biv["\']?\s*[:=]\s*["\']([^"\']{8,32})["\']')),
    ("token_url",  re.compile(r'(?i)"[^"]*(?:token|secret|key)[^"]*"\s*:\s*"[^"]{8,80}"')),
]


def scan(out_dir):
    C.hdr("敏感信息扫描")
    hits = {k: set() for k, _ in PATTERNS}
    filecount = 0
    for dirpath, _, filenames in os.walk(out_dir):
        for fn in filenames:
            if not fn.endswith((".js", ".json", ".wxml", ".html", ".txt", ".wxs")):
                continue
            p = os.path.join(dirpath, fn)
            try:
                if os.path.getsize(p) > 20 * 1024 * 1024:
                    continue
                with open(p, "r", encoding="utf-8", errors="ignore") as f:
                    txt = f.read()
            except OSError:
                continue
            filecount += 1
            for k, pat in PATTERNS:
                for m in pat.finditer(txt):
                    v = m.group(1) if m.groups() else m.group(0)
                    hits[k].add(v)

    C.dim(f"扫描 {filecount} 个文本文件")
    for k, vals in hits.items():
        if not vals:
            continue
        C.warn(f"{k}: {len(vals)} 个")
        for v in list(vals)[:8]:
            C.dim(f"   {v[:90]}")
        if len(vals) > 8:
            C.dim(f"   …还有 {len(vals)-8} 个")

    domains = set()
    for u in hits["api_base"]:
        m = re.match(r'https?://([^/:]+)', u)
        if m:
            domains.add(m.group(1))
    if domains:
        C.warn(f"涉及域名 {len(domains)} 个")
        for d in sorted(domains)[:20]:
            C.dim(f"   {d}")

    return hits


def main():
    if len(sys.argv) < 2:
        C.usage_die(__doc__.strip().split("用法：")[0] + "用法：\n  ./wxapkg-unpack.py <wxapkg文件或目录> [--out DIR] [--scan]\n示例：\n  ./wxapkg-unpack.py ./pkg/ --out ./out --scan")

    target = sys.argv[1]
    out_dir = "./unpacked"
    do_scan = "--scan" in sys.argv
    if "--out" in sys.argv:
        out_dir = sys.argv[sys.argv.index("--out") + 1]

    if not os.path.exists(target):
        C.err(f"路径不存在: {target}")
        sys.exit(1)

    pkgs = []
    if os.path.isfile(target):
        pkgs = [target]
    else:
        for dp, _, fns in os.walk(target):
            for fn in fns:
                if fn.endswith(".wxapkg"):
                    pkgs.append(os.path.join(dp, fn))

    if not pkgs:
        C.warn("没找到 .wxapkg 文件。微信客户端的包在：")
        C.dim("  Android: /data/data/com.tencent.mm/MicroMsg/<hash>/appbrand/pkg/")
        C.dim("  PC 微信:  WeChat Files/Applet/<appid>/<version>/")
        C.verdict("wxapkg-unpack", "NO_PKG", "0 个包")
        sys.exit(2)

    C.hdr(f"0. 发现 {len(pkgs)} 个包")
    total = 0
    for p in pkgs:
        sub = os.path.join(out_dir, os.path.splitext(os.path.basename(p))[0])
        os.makedirs(sub, exist_ok=True)
        n = unpack(p, sub)
        if n:
            total += n

    if do_scan and total:
        scan(out_dir)
    else:
        C.dim("加 --scan 触发敏感信息扫描")

    C.hdr("结论")
    C.dim(f"包 {len(pkgs)} 个 / 解出文件 {total} 个")
    print("""
解包后必看的四类文件：
  app-service.js        主逻辑，API 全集、加密实现、密钥都在这里
  __APP__.wxapkg        分包，常有业务核心
  app.json              pages 列表 → 完整页面路由（爬未授权页面的入口）
  project.config.json   开发者配置，偶含 appid + 测试环境地址

下一步：
  · API 全集：grep -ohE 'url:\\s*["'"'"'][^"'"'"']+' app-service.js | sort -u
  · 加密实现：grep -n 'CryptoJS\\|createCipher\\|aes' app-service.js
  · 若发现硬编码 key/iv，用 aes-hunter.py 验证是否能解出真实响应""")

    C.verdict("wxapkg-unpack", "DONE" if total else "FAILED", f"{len(pkgs)} 包 / {total} 文件")


if __name__ == "__main__":
    sys.exit(C.main_guard(main)())
