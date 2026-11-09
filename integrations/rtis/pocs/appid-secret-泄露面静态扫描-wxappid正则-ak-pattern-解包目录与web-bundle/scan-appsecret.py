#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
AppID / Secret / AK 泄露面静态扫描

用途：拿一个解包后的目录（wxapkg 解包结果、APK 解包结果、web dist），
      扫出所有「凭证形态」的字符串，并按置信度分级。

为什么不能只用一条正则：
  · AppSecret 没有固定前缀（32 位 hex），和 md5 完全同形
  · AK/SK 各家有前缀（AKID/ASIA/LTAI），但云厂商有几十家
  · 高熵字符串会产生海量噪音（webpack chunk 里的 hash 文件名就是 32 位 hex）

所以分三级：
  L1 CONFIRMED  带明确键名（app_secret|appSecret|secret_key = "..."）→ 几乎无假阳性
  L2 LIKELY     带厂商前缀（AKID/LTAI/AKIA）或 JWT 完整三段
  L3 WEAK       裸 32/40/64 位 hex —— 默认不输出，加 --aggressive 才显示

用法：
  ./scan-appsecret.py <目录或文件> [--aggressive] [--json out.json]
"""
import os
import re
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "_lib"))
import common as C  # noqa: E402

# ---- 规则表 --------------------------------------------------------------
# 键名规则：捕获「键名 = 引号包裹的值」。允许 : 和 = 两种分隔（js/json 与 ini/env）
KEY_RULES = [
    ("wx_appsecret", r'(?i)\b(app[_-]?secret|wxapp[_-]?secret|mp[_-]?secret)\b\s*[:=]\s*["\']([0-9a-f]{32})["\']'),
    ("wx_appid",     r'(?i)\b(app[_-]?id|appid|wxappid)\b\s*[:=]\s*["\'](wx[0-9a-f]{16})["\']'),
    ("generic_secret", r'(?i)\b([a-z_]*(?:secret|passwd|password|token)[a-z_]*)\b\s*[:=]\s*["\']([^"\']{12,80})["\']'),
    ("db_dsn",       r'(?i)\b(mysql|postgres|postgresql|mongodb|redis|amqp)://([^\s"\']{8,120})'),
    ("jdbc",         r'(?i)(jdbc:[a-z]+://[^\s"\']{8,120})'),
]

# 厂商前缀规则
PREFIX_RULES = [
    ("aliyun_ak",  r'\b(LTAI[A-Za-z0-9]{12,20})\b'),
    ("aliyun_ak2", r'\b(AKID[A-Za-z0-9]{12,30})\b'),
    ("aws_ak",     r'\b((?:AKIA|ASIA)[A-Z0-9]{16})\b'),
    ("tencent_ak", r'\b(AKID[A-Za-z0-9]{32})\b'),
    ("google_api", r'\b(AIza[0-9A-Za-z_-]{35})\b'),
    ("github_pat", r'\b(gh[pousr]_[A-Za-z0-9]{36,255})\b'),
    ("slack_tok",  r'\b(xox[baprs]-[A-Za-z0-9-]{10,80})\b'),
    ("stripe_sk",  r'\b(sk_live_[A-Za-z0-9]{20,99})\b'),
    ("openai_sk",  r'\b(sk-[A-Za-z0-9]{32,64})\b'),
    ("jwt",        r'\b(eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,})\b'),
    ("pem_priv",   r'(-----BEGIN (?:RSA |EC )?PRIVATE KEY-----)'),
]

# L3 裸高熵。默认关闭 —— 光 webpack 的 hash 文件名就能刷出几千条。
WEAK_RULE = r'\b([0-9a-f]{32}|[0-9a-f]{40}|[0-9a-f]{64})\b'

SCAN_EXT = {
    ".js", ".mjs", ".cjs", ".ts", ".json", ".vue", ".jsx", ".tsx",
    ".py", ".php", ".java", ".kt", ".go", ".rb", ".cs",
    ".yaml", ".yml", ".ini", ".conf", ".env", ".properties", ".xml",
    ".txt", ".md", ".html", ".htm", ".smali", ".plist", ".gradle", ".cfg",
}
SKIP_DIR = {".git", "node_modules", "__pycache__", ".idea", ".gradle", "build/intermediates"}
MAX_BYTES = 8 * 1024 * 1024   # 单文件上限，防止把整个 sqlite 当文本读


def iter_files(root):
    if os.path.isfile(root):
        yield root
        return
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIR and not d.startswith(".cache")]
        for fn in filenames:
            ext = os.path.splitext(fn)[1].lower()
            if ext in SCAN_EXT:
                yield os.path.join(dirpath, fn)


def scan_text(text, aggressive=False):
    """返回 [(level, kind, value, extra)]。value 做脱敏，只留首尾。"""
    hits = []
    for kind, pat in KEY_RULES:
        for m in re.finditer(pat, text):
            v = m.group(2) if m.lastindex and m.lastindex >= 2 else m.group(1)
            hits.append(("L1", kind, v, m.group(0)[:120]))
    for kind, pat in PREFIX_RULES:
        for m in re.finditer(pat, text):
            hits.append(("L2", kind, m.group(1), m.group(0)[:80]))
    if aggressive:
        for m in re.finditer(WEAK_RULE, text):
            hits.append(("L3", "hex_blob", m.group(1), m.group(0)))
    return hits


def mask(v: str) -> str:
    if len(v) <= 8:
        return v[:2] + "***"
    return v[:4] + "…" + v[-4:]


def main():
    if len(sys.argv) < 2:
        C.usage_die(__doc__.strip().split("用法：")[0] + "用法：\n  ./scan-appsecret.py <目录或文件> [--aggressive] [--json out.json]")

    target = sys.argv[1]
    aggressive = "--aggressive" in sys.argv
    out_json = None
    if "--json" in sys.argv:
        i = sys.argv.index("--json")
        out_json = sys.argv[i + 1] if i + 1 < len(sys.argv) else None

    if not os.path.exists(target):
        C.err(f"路径不存在: {target}")
        sys.exit(1)

    C.hdr("0. 扫描范围")
    files = list(iter_files(target))
    C.dim(f"目标: {target}")
    C.dim(f"待扫文件: {len(files)} 个（后缀过滤后）")
    if not files:
        C.warn("没有可扫文件。检查路径，或该目录只有二进制（.so/.dex/.apk 需先解包）")
        C.verdict("app-secret-scan", "NO_INPUT", "0 个文本文件")
        return

    C.hdr("1. 逐文件扫描")
    results = []
    scanned = 0
    for path in files:
        try:
            if os.path.getsize(path) > MAX_BYTES:
                C.dim(f"跳过（>8MB）: {os.path.relpath(path, target)}")
                continue
            with open(path, "r", encoding="utf-8", errors="ignore") as f:
                text = f.read()
        except OSError as e:
            C.warn(f"读失败 {path}: {e}")
            continue
        scanned += 1
        hits = scan_text(text, aggressive)
        for level, kind, value, ctx in hits:
            results.append({
                "level": level, "kind": kind, "value": value,
                "masked": mask(value), "file": path, "context": ctx,
            })

    C.dim(f"实际读取 {scanned} 个文件")

    C.hdr("2. L1 确认级（带明确键名）")
    l1 = [r for r in results if r["level"] == "L1"]
    seen = set()
    for r in l1:
        key = (r["kind"], r["value"])
        if key in seen:
            continue
        seen.add(key)
        C.warn(f"{r['kind']}: {r['masked']}  ← {os.path.relpath(r['file'], target)}")
        C.dim(f"    {r['context'][:100]}")
    if not l1:
        C.ok("无 L1 命中")

    C.hdr("3. L2 厂商前缀 / JWT / 私钥")
    l2 = [r for r in results if r["level"] == "L2"]
    seen2 = set()
    for r in l2:
        key = (r["kind"], r["value"])
        if key in seen2:
            continue
        seen2.add(key)
        C.warn(f"{r['kind']}: {r['masked']}  ← {os.path.relpath(r['file'], target)}")
    if not l2:
        C.ok("无 L2 命中")

    if aggressive:
        C.hdr("4. L3 裸高熵（噪音大，仅供人工筛）")
        l3 = [r for r in results if r["level"] == "L3"]
        C.dim(f"{len(l3)} 条，前 20 条：")
        for r in l3[:20]:
            C.dim(f"  {r['value'][:16]}…  ← {os.path.relpath(r['file'], target)}")

    C.hdr("结论")
    C.dim(f"L1={len(l1)} L2={len(l2)} L3={sum(1 for r in results if r['level']=='L3')}")
    C.err("")
    print("""必须验证，否则不算漏洞（这是本脚本和「grep 出个 32 位 hex 就报高危」的区别）：
  1) AppSecret 是否真的有效？对微信开放接口换一次 access_token：
       curl 'https://api.weixin.qq.com/cgi-bin/token?grant_type=client_credential&appid=APPID&secret=SECRET'
     返回 access_token = 有效；返回 40125 = secret 已失效
  2) 云 AK 是否有效？只做 Describe 类只读调用（绝不 List/Create/Delete）：
       aliyun sts GetCallerIdentity / aws sts get-caller-identity
  3) JWT 是否可伪造？拿 payload 里的 alg 判断；HS256 且密钥在包里 = 可签任意 token
  4) 数据库 DSN 只做 connect + SELECT 1，不读表
报告口径：写清「哪个文件、哪一行、什么凭证、验证结果」，不要只写「存在硬编码密钥」""")

    if l1 or l2:
        C.verdict("app-secret-scan", "FOUND", f"L1={len(l1)} L2={len(l2)}")
    else:
        C.verdict("app-secret-scan", "CLEAN", f"{scanned} 文件无凭证形态命中")

    if out_json:
        C.dump_json({"target": target, "scanned": scanned, "results": results}, out_json)


if __name__ == "__main__":
    sys.exit(C.main_guard(main)())
