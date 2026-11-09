#!/usr/bin/env python3
r"""小程序 wxapkg 全自动解包与敏感信息提取流水线（模板，未针对任何真实目标验证）。

判据/流程（复现步骤）：
  1) 输入：一个或多个 .wxapkg 文件（获取路径见 miniprogram-recon 技能：
     安卓 adb 拉取 /data/data/com.tencent.mm/MicroMsg/<uhash>/appbrand/pkg/，
     PC 端 WeChatAppEx / xwechat_files 缓存目录，或抓包 dldir1.qq.com 下载通道）
  2) 解包主链：优先 unveilr（Rust，支持 PC 端加密包）：unveilr <pkg.wxapkg>
     未安装时打印安装命令（cargo install unveilr 或 npm 包 wxappUnpacker 镜像），
     并用本脚本内置的 wxapkg v1 头解析兜底（只能拆出原始文件段，不做 wxss/wxml 还原）
  3) 敏感信息提取（对解包输出目录递归 grep）：
     - appid:    wx[0-9a-fA-F]{16}
     - secret:   secret|appsecret|AppSecret 后跟 hex32
     - 云 AK:    LTAI[0-9A-Za-z]{12,28}（阿里云）/ AKID[0-9A-Za-z]{13,20}（腾讯云）
     - 加密 key: AES|CryptoJS|Crypto.enc|key = / setKey 等上下文行
     - 内网域名: (dev|test|uat|internal|intra|\.local|10\.|172\.(1[6-9]|2[0-9]|3[01])\.|192\.168\.) 的 host
     - 接口路径: https?:// 与 /api/、/gateway/ 前缀路径
  4) 产出：findings 清单（类别/文件/行号/命中片段），人工复核后回填 knowledge.db

用法：
  python3 extract.py <解包输出目录或wxapkg路径...> [-o findings.json]

纪律：
  - 仅用于授权评估的小程序；样本与解包产物保存在本地工作目录，不外传
  - 提取到的真实 secret/appid 在报告与 knowledge.db 中一律脱敏（wxXXXXXXXXXXXXXXXX）
"""
import argparse
import json
import os
import re
import subprocess
import sys

PATTERNS = {
    "appid": re.compile(r"\bwx[0-9a-fA-F]{16}\b"),
    "appsecret": re.compile(r"(?:app_?secret|AppSecret)[\"'\s:=]+([0-9a-f]{32})"),
    "aliyun-ak": re.compile(r"\bLTAI[0-9A-Za-z]{12,28}\b"),
    "tencent-ak": re.compile(r"\bAKID[0-9A-Za-z]{13,20}\b"),
    "crypto-key": re.compile(r"(?:AES|CryptoJS|enc\.Utf8\.parse|setKey|secretKey)[^\n]{0,80}"),
    "internal-host": re.compile(r"https?://[a-zA-Z0-9.-]*(?:dev|test|uat|internal|intra)[a-zA-Z0-9.-]*"),
    "private-ip": re.compile(r"https?://(?:10\.\d+\.\d+\.\d+|172\.(?:1[6-9]|2\d|3[01])\.\d+\.\d+|192\.168\.\d+\.\d+)"),
    "api-path": re.compile(r"[\"']/(?:api|gateway|v1|v2)/[A-Za-z0-9/_-]+[\"']"),
}


def try_unveilr(pkg: str) -> str | None:
    """有 unveilr 就用它解包，返回输出目录；没有就返回 None（由主流程兜底）。"""
    for cmd in (["unveilr", pkg], ["npx", "unveilr", pkg]):
        try:
            out = os.path.join(os.getcwd(), os.path.basename(pkg) + ".unpacked")
            r = subprocess.run(cmd + ["-o", out], capture_output=True, text=True, timeout=120)
            if r.returncode == 0 and os.path.isdir(out):
                return out
        except (OSError, subprocess.TimeoutExpired):
            continue
    return None


def raw_split(pkg: str, out: str) -> int:
    """纯 python 兜底：按 wxapkg v1 头拆出文件段（不做编译还原）。
    头格式：firstMark(1) unknown(4) indexInfoLen(4) bodyInfoLen(4) lastMark(1)
            fileCount(4) 然后 fileCount 组 [nameLen(4) name(nameLen) offset(4) size(4)]"""
    with open(pkg, "rb") as f:
        data = f.read()
    if len(data) < 22 or data[0] not in (0xBE, 0xED):
        print(f"[skip] {pkg}: 非标准 wxapkg 头（可能是 PC 加密包，需 unveilr）")
        return 0
    import struct

    idx = 14
    (count,) = struct.unpack(">I", data[idx : idx + 4])
    idx += 4
    os.makedirs(out, exist_ok=True)
    n = 0
    for _ in range(count):
        (nlen,) = struct.unpack(">I", data[idx : idx + 4])
        idx += 4
        name = data[idx : idx + nlen].decode("utf-8", "replace")
        idx += nlen
        (off, size) = struct.unpack(">II", data[idx : idx + 8])
        idx += 8
        p = os.path.join(out, name)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "wb") as w:
            w.write(data[off : off + size])
        n += 1
    return n


def scan_dir(root: str) -> list[dict]:
    hits = []
    for base, _dirs, files in os.walk(root):
        for fn in files:
            p = os.path.join(base, fn)
            if os.path.getsize(p) > 5 * 1024 * 1024:
                continue
            try:
                text = open(p, encoding="utf-8", errors="replace").read()
            except OSError:
                continue
            for cat, pat in PATTERNS.items():
                for m in pat.finditer(text):
                    line = text.count("\n", 0, m.start()) + 1
                    frag = text[max(0, m.start() - 20) : m.end() + 20].replace("\n", " ")
                    hits.append({"cat": cat, "file": p.replace(root, ".", 1), "line": line, "frag": frag[:160]})
    return hits


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("targets", nargs="+", help="wxapkg 文件或已解包目录")
    ap.add_argument("-o", "--out", default="findings.json")
    a = ap.parse_args()

    roots = []
    for t in a.targets:
        if os.path.isdir(t):
            roots.append(t)
        elif t.endswith(".wxapkg"):
            out = try_unveilr(t)
            if out:
                print(f"[unveilr] {t} -> {out}")
            else:
                out = t + ".raw"
                n = raw_split(t, out)
                print(f"[raw-split] {t} 拆出 {n} 个文件段 -> {out}（还原还原层需装 unveilr）")
            roots.append(out)
        else:
            sys.exit(f"输入不存在: {t}")

    findings = []
    for r in roots:
        findings.extend(scan_dir(r))
    # 按 cat 归并去重（同文件同行同类别只留一条）
    seen, uniq = set(), []
    for h in findings:
        k = (h["cat"], h["file"], h["line"])
        if k not in seen:
            seen.add(k)
            uniq.append(h)
    json.dump(uniq, open(a.out, "w"), ensure_ascii=False, indent=1)
    by = {}
    for h in uniq:
        by[h["cat"]] = by.get(h["cat"], 0) + 1
    print(f"共 {len(uniq)} 条命中 -> {a.out}")
    for k, v in sorted(by.items(), key=lambda x: -x[1]):
        print(f"  {k:<14} {v}")


if __name__ == "__main__":
    main()
