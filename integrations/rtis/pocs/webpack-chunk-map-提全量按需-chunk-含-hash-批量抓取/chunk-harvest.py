#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
从入口 bundle 反解全量 chunk 清单并批量抓取

为什么不能靠目录列举：
  · Vite/Webpack 的 chunk 名带 contenthash，目录列举拿不到清单
  · /assets/ 目录通常 403 或返回 SPA 首页
  · manifest.json / .vite/manifest.json 生产环境基本不给

唯一可靠的来源是**入口 bundle 自己**：
  Vite   → window.__vite__mapDeps = [...] 或 __vite__mapDeps([...]) 的依赖数组
  Webpack 5 → __webpack_require__.u = (chunkId) => "static/js/" + chunkId + "." + {...}[chunkId] + ".js"
  Webpack 4 → 同上但形如 "js/" + chunkId + "." + hash + ".chunk.js"
  Rspack/ESBuild → 类似，都有 chunk 名映射表

本脚本做四步：
  1. 抓入口 index.html，抠出所有 <script src> 与 <link rel=modulepreload>
  2. 从入口 bundle 里正则提取 chunk 名映射表 / 依赖数组
  3. 去重 + 补全路径，逐个 HEAD/GET 验证存在
  4. 全量下载到 --out 目录，并合并成一个文件供 grep

用法：
  ./chunk-harvest.py <BASE_URL> [--out /tmp/chunks] [--max 2000] [--sleep 0.1]
示例：
  ./chunk-harvest.py https://app.example.com
  ./chunk-harvest.py https://app.example.com --out ./chunks --max 3000
"""
import os
import re
import sys
import time
import urllib.parse

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "_lib"))
import common as C  # noqa: E402

# chunk 名形态。三组都要，缺一组会漏：
#   A. 纯 hash 名          index.DM7ZXbcO.js          （Vite 默认）
#   B. 业务名+hash         vendor-react.a1b2c3d4.js   （Vite/Rollup 命名）
#   C. webpack chunkId+hash 12.4f8a9b.js / 12.4f8a9b.chunk.js
RE_CHUNK = [
    re.compile(r'["\']((?:[\w@.-]*/)?[\w.-]+\.[A-Za-z0-9_-]{8}\.js)["\']'),
    re.compile(r'["\']((?:[\w@.-]*/)?[A-Za-z0-9_-]{6,8}-[A-Za-z0-9_-]{8}\.js)["\']'),
    re.compile(r'["\']((?:[\w@.-]*/)?\d+\.[a-f0-9]{6,12}(?:\.chunk)?\.js)["\']'),
]
RE_SCRIPT = re.compile(r'<script[^>]+src=["\']([^"\']+)["\']', re.I)
RE_PRELOAD = re.compile(r'<link[^>]+(?:rel=["\']modulepreload["\'][^>]*href|href)=["\']([^"\']+\.js)["\']', re.I)


def resolve(base: str, ref: str) -> str:
    return urllib.parse.urljoin(base, ref)


def main():
    if len(sys.argv) < 2:
        C.usage_die(__doc__.strip().split("用法：")[0] + "用法：\n  ./chunk-harvest.py <BASE_URL> [--out DIR] [--max N] [--sleep S]\n示例：\n  ./chunk-harvest.py https://app.example.com --out ./chunks")

    base = sys.argv[1]
    out_dir = "/tmp/chunks"
    max_n = 2000
    sleep_s = 0.1
    for flag, cast in (("--out", str), ("--max", int), ("--sleep", float)):
        if flag in sys.argv:
            val = sys.argv[sys.argv.index(flag) + 1]
            if flag == "--out":
                out_dir = val
            elif flag == "--max":
                max_n = cast(val)
            else:
                sleep_s = cast(val)

    C.hdr("1. 抓入口页")
    idx = C.req(base)
    if not idx.ok:
        C.err(f"入口不可达: {idx.error or idx.status}")
        C.verdict("chunk-harvest", "DEAD", f"{idx.status} {idx.error}")
        sys.exit(1)
    C.ok(f"index {idx.status} {len(idx.body)}B")
    html = idx.body

    scripts = set(RE_SCRIPT.findall(html)) | set(RE_PRELOAD.findall(html))
    C.dim(f"入口 script/preload: {len(scripts)} 个")
    for s in list(scripts)[:5]:
        C.dim(f"  {s}")

    entries = [resolve(base, s) for s in scripts]
    if not entries:
        # 没有 script 标签时，赌一下默认入口
        for guess in ("/assets/index.js", "/static/js/main.js", "/js/app.js"):
            r = C.req(resolve(base, guess))
            if r.ok and r.status == 200:
                entries.append(resolve(base, guess))
                break

    if not entries:
        C.warn("入口页无 <script src>，可能靠 JS 动态注入，本脚本无法自动定位入口")
        C.verdict("chunk-harvest", "NO_ENTRY", "需手工指定入口 bundle URL")
        sys.exit(2)

    C.hdr("2. 从入口 bundle 反解 chunk 名")
    found = set()
    fetched = 0
    queue = list(entries)
    seen_url = set()
    while queue and fetched < 40:   # 入口只向前解析 40 层，避免爆
        u = queue.pop(0)
        if u in seen_url:
            continue
        seen_url.add(u)
        r = C.req(u)
        fetched += 1
        if not r.ok or r.status != 200:
            C.dim(f"  {u} → {r.status}")
            continue
        C.dim(f"  解析 {os.path.basename(u)} ({len(r.body)}B)")
        for pat in RE_CHUNK:
            for m in pat.finditer(r.body):
                name = m.group(1)
                if name.startswith(("http://", "https://", "//")):
                    continue
                found.add(name)
        # Vite 的 __vite__mapDeps 里是相对 './xxx.js'，上面正则已覆盖
        # 若发现新的「相对 import」，再入队一层（限制在 40 个内）
        for m in re.finditer(r'import\(["\'](\./[^"\']+\.js)["\']\)', r.body):
            queue.append(resolve(u, m.group(1)))

    C.ok(f"反解出 {len(found)} 个 chunk 名")
    if not found:
        C.warn("0 个 chunk 名。可能是 SSR 直出、或 chunk 名完全运行时拼装")
        C.verdict("chunk-harvest", "NO_CHUNKS", f"解析了 {fetched} 个入口")
        sys.exit(2)

    C.hdr("3. 补全路径并验证")
    # chunk 的真实路径通常与入口同目录
    entry_dir = os.path.dirname(urllib.parse.urlsplit(entries[0]).path) or "/"
    candidates = []
    for name in sorted(found):
        if name.startswith("/"):
            candidates.append(resolve(base, name))
        else:
            candidates.append(resolve(base, entry_dir.rstrip("/") + "/" + name))

    C.dim(f"候选 URL: {len(candidates)}")
    os.makedirs(out_dir, exist_ok=True)

    C.hdr("4. 下载")
    okn = 0
    total_bytes = 0
    manifest = []
    for i, u in enumerate(candidates[:max_n], 1):
        r = C.req(u)
        if r.ok and r.status == 200 and len(r.body) > 0:
            fn = os.path.join(out_dir, os.path.basename(urllib.parse.urlsplit(u).path) or f"c{i}.js")
            with open(fn, "wb") as f:
                f.write(r.body.encode("utf-8", "replace") if isinstance(r.body, str) else r.body)
            okn += 1
            total_bytes += len(r.body)
            manifest.append({"url": u, "file": fn, "bytes": len(r.body)})
        if i % 100 == 0:
            C.dim(f"  已处理 {i}/{min(len(candidates), max_n)}，成功 {okn}")
        time.sleep(sleep_s)

    C.ok(f"下载 {okn} 个，共 {total_bytes/1024/1024:.2f} MB -> {out_dir}")

    # 合并供 grep
    merged = os.path.join(out_dir, "_ALL.js")
    with open(merged, "w", encoding="utf-8", errors="ignore") as out:
        for m in manifest:
            try:
                with open(m["file"], "r", encoding="utf-8", errors="ignore") as f:
                    out.write(f.read())
                    out.write("\n/* ==== next chunk ==== */\n")
            except OSError:
                pass
    C.ok(f"合并文件: {merged} ({os.path.getsize(merged)/1024/1024:.2f} MB)")

    C.hdr("结论")
    C.dim(f"入口 {fetched} 个 / chunk 名 {len(found)} / 下载成功 {okn} / {total_bytes/1024/1024:.2f} MB")
    print(f"""
下一步（这些 grep 才有价值，逐个跑）：
  # 内部域名与 IP
  grep -ohE 'https?://[a-zA-Z0-9._-]{{4,60}}' {merged} | sort -u
  grep -ohE '\\b(10|172|192)\\.([0-9]{{1,3}}\\.){{2}}[0-9]{{1,3}}\\b' {merged} | sort -u
  # 硬编码密钥
  grep -ohE '(aes|secret|app_?key|api_?key|private_key)\\s*[:=]\\s*["'][^"']{{8,80}}["']' {merged} | sort -u
  # 未记录 API 路径
  grep -ohE '"/[a-zA-Z0-9_/-]{{3,60}}"' {merged} | sort -u > api-paths.txt
  # 数据库 / 中间件
  grep -ohiE '(jdbc:[a-z:]+//|mysql://|redis://|mongodb://)[^"\\s]{{5,100}}' {merged} | sort -u

注意：chunk 里出现的 API 路径不全是活的。很多是历史遗留或已下线，
必须先做存活判定（与随机路径比对），再列进报告。""")

    C.dump_json({"base": base, "entries": fetched, "names": len(found),
                 "downloaded": okn, "bytes": total_bytes, "merged": merged,
                 "files": manifest}, os.path.join(out_dir, "manifest.json"))

    C.verdict("chunk-harvest", "DONE" if okn else "EMPTY", f"{okn} chunk / {total_bytes/1024/1024:.1f}MB")


if __name__ == "__main__":
    sys.exit(C.main_guard(main)())
