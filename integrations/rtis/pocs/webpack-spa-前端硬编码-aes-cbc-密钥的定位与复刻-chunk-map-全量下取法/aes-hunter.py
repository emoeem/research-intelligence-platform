#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
前端硬编码 AES 密钥定位器  (aes-hunter)

为什么写这个：
    这类站点的加密参数一定有一个 key/iv 在 bundle 里。人工去 20MB 的 chunk 里
    grep "key" 会被 8000 条命中淹掉。真正有效的做法不是「找 key」，而是
    「找能被解开的组合」——以密文为 oracle，反向确认哪一组 key/iv/mode 是对的。

    所以本脚本的方向是：先收集候选，再**用解密成功与否**筛，而不是靠肉眼读代码。

判定逻辑（为什么这样排）：
    1. 候选来源分四类，优先级从高到低：显式 key/iv 变量 > CryptoJS.parse >
       hex from() > 长度精确的裸字面量。前三类有上下文，误报低。
    2. 解密「成功」的定义很严：必须 PKCS7 padding 校验通过 **且** 明文可打印
       **且** 命中 JSON/查询串/中文任一特征。只看「不抛异常」会把随机数据
       当成明文（概率约 1/256 每字节，32 字节样本必然误报）。
    3. 每组都跑 ECB 和 CBC（IV=0 也试），因为很多站点把 CBC 当 ECB 用。

用法：
    ./poc.sh --url https://TARGET/            # 自动抓页面引用的 js
    ./poc.sh --dir ./chunks                   # 本地已下载的 chunk 目录
    ./poc.sh --file app.js --sample <B64>     # 手动指定

    --sample 可重复给多次；不给时从 js 里自动挑长 base64/hex 串（误报高，
    仅作提示，命中会标 [AUTO]）。
    --max-candidates N  候选上限（默认 400，防组合爆炸）

退出码：
    0 = 找到至少一组可用组合
    1 = 参数错误
    2 = 没有找到（不是错误，是否定结论）
"""
import argparse
import base64
import itertools
import os
import re
import sys
import urllib.request

# ---------------------------------------------------------------------------
# 候选提取
# ---------------------------------------------------------------------------

# 1) 显式变量：key/iv/secret 后面跟一个字符串
RE_NAMED = re.compile(
    r"""(?ix)
    \b(aes[_-]?)?(key|iv|secret|aeskey|aes[_-]?iv|enckey|enc[_-]?key|skey|siv)\b
    \s*[:=]\s*
    ["'`]([^"'`\n]{8,64})["'`]
    """
)

# 2) CryptoJS.enc.Utf8.parse("...")  /  parse("...", "hex")
RE_PARSE = re.compile(
    r"""CryptoJS\.enc\.(Utf8|Latin1|Hex)\.parse\(\s*["']([^"']{8,80})["']\s*\)"""
)

# 3) from("...", "hex") / Buffer.from("...","hex")
RE_FROM_HEX = re.compile(
    r"""(?:from|\bparse)\(\s*["']([0-9a-fA-F]{16,64})["']\s*,\s*["']hex["']"""
)

# 3b) key/iv 用 Buffer.from(...) / Uint8Array.from(...) 包一层的写法。
# 这是最常见的一种，漏掉它会导致「key 找到了、iv 找不到」，进而用全 0 IV 解出乱码。
RE_BUF_FROM = re.compile(
    r"""(?isx)
    \b([A-Za-z_$][\w$]{0,20})
    \s*[:=]\s*
    (?:Buffer|Uint8Array|[A-Za-z_$][\w$]*)\s*\.\s*from\(
    \s*["']([^"'\n]{8,80})["']
    (?:\s*,\s*["'](\w+)["'])?
    """
)

# 4) 裸字面量：长度恰为 16/24/32 的可打印 ASCII（最后兜底，噪声大）
RE_BARE = re.compile(r"""["']([ -~]{16}|[ -~]{24}|[ -~]{32})["']""")

# 密文样本：长 base64
RE_B64 = re.compile(r"""["']([A-Za-z0-9+/]{32,}={0,2})["']""")

# 页面里的 script src
RE_SCRIPT = re.compile(r"""<script[^>]+src=["']([^"']+\.js[^"']*)["']""", re.I)


def fetch(url, timeout=15):
    req = urllib.request.Request(url, headers={
        "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) "
                      "AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
    })
    with urllib.request.urlopen(req, timeout=timeout) as r:
        raw = r.read()
    for enc in ("utf-8", "latin-1"):
        try:
            return raw.decode(enc)
        except UnicodeDecodeError:
            continue
    return ""


def collect_from_url(base, limit=60):
    """抓首页 → 拿 script src → 抓 js。返回 [(name, text)]"""
    base = base.rstrip("/")
    out = []
    try:
        html = fetch(base + "/")
    except Exception as e:
        print(f"[!] 抓首页失败: {e}", file=sys.stderr)
        return out
    srcs = []
    for s in RE_SCRIPT.findall(html):
        if s.startswith("//"):
            s = "https:" + s
        elif s.startswith("/"):
            s = base + s
        elif not s.startswith("http"):
            s = base + "/" + s
        srcs.append(s)
    # 去重保序
    seen = set()
    srcs = [x for x in srcs if not (x in seen or seen.add(x))]
    print(f"[*] 页面引用 {len(srcs)} 个 js，抓前 {min(limit, len(srcs))} 个")
    for u in srcs[:limit]:
        try:
            out.append((u.rsplit("/", 1)[-1], fetch(u)))
        except Exception as e:
            print(f"[!] {u}: {e}", file=sys.stderr)
    return out


def collect_from_dir(d):
    out = []
    for root, _, files in os.walk(d):
        for f in files:
            if not f.endswith((".js", ".mjs", ".json", ".html")):
                continue
            p = os.path.join(root, f)
            try:
                out.append((f, open(p, encoding="utf-8", errors="ignore").read()))
            except Exception as e:
                print(f"[!] {p}: {e}", file=sys.stderr)
    return out


def extract_candidates(blobs):
    """返回 (keys, ivs)，每个元素是 (value, kind, source)"""
    keys, ivs = [], []
    kseen, iseen = set(), set()

    def add(bucket, seen, val, kind, src):
        if val in seen or len(val) < 8:
            return
        seen.add(val)
        bucket.append((val, kind, src))

    for name, text in blobs:
        for m in RE_NAMED.finditer(text):
            kw = (m.group(2) or "").lower()
            val = m.group(3)
            if kw.startswith("iv") or kw in ("siv",):
                add(ivs, iseen, val, "named-iv", name)
            else:
                add(keys, kseen, val, "named-key", name)
        for m in RE_PARSE.finditer(text):
            enc, val = m.group(1), m.group(2)
            kind = "parse-hex" if enc == "Hex" else "parse-utf8"
            # parse 出来的多半是 key；iv 也会走 parse，靠长度和命名无法区分，
            # 两份都放，组合时自然筛掉错的
            add(keys, kseen, val, kind, name)
            add(ivs, iseen, val, kind + "-maybe-iv", name)
        for m in RE_FROM_HEX.finditer(text):
            add(keys, kseen, m.group(1), "from-hex", name)
        for m in RE_BUF_FROM.finditer(text):
            var, val, enc = m.group(1), m.group(2), (m.group(3) or "utf8")
            kind = f"buffer-from/{enc}"
            if re.match(r"(?i)^(iv|.*_iv|.*iv$|siv|nonce)", var):
                add(ivs, iseen, val, kind, name)
            elif re.match(r"(?i)^(key|.*key$|skey|secret|aeskey)", var):
                add(keys, kseen, val, kind, name)
            else:
                # 变量名看不出用途：两份都放，让解密去筛
                add(keys, kseen, val, kind, name)
                add(ivs, iseen, val, kind + "-maybe-iv", name)
        for m in RE_BARE.finditer(text):
            v = m.group(1)
            # 纯数字/纯符号的跳过，几乎不可能是 key
            if sum(c.isalnum() for c in v) < len(v) * 0.6:
                continue
            add(keys, kseen, v, "bare", name)
    return keys, ivs


def harvest_samples(blobs):
    out = []
    for _, text in blobs:
        for m in RE_B64.finditer(text):
            s = m.group(1)
            if len(s) >= 32 and len(s) % 4 == 0:
                out.append(s)
    # 去重，最长优先（长串更可能是真密文）
    return sorted(set(out), key=len, reverse=True)


# ---------------------------------------------------------------------------
# 解密判定
# ---------------------------------------------------------------------------

try:
    from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
    HAVE_CRYPTO = True
except ImportError:
    HAVE_CRYPTO = False


def decrypt_blob(data, key, iv, mode):
    if not HAVE_CRYPTO:
        return None
    try:
        if mode == "ecb":
            c = Cipher(algorithms.AES(key), modes.ECB())
        else:
            c = Cipher(algorithms.AES(key), modes.CBC(iv))
        d = c.decryptor()
        return d.update(data) + d.finalize()
    except Exception:
        return None


def unpad_pkcs7(b):
    if not b:
        return None
    n = b[-1]
    if n < 1 or n > 16 or n > len(b):
        return None
    if b[-n:] != bytes([n]) * n:
        return None
    return b[:-n]


def looks_like_plaintext(b):
    """严格判据：可打印比例 + 无控制字符 + 结构特征，任一不满足就不算命中。

    为什么必须这么严：
      - 只用「pkcs7 不抛异常」当判据，随机数据有 1/256 概率通过，32 字节样本
        必然误报；我第一版就是只看了可打印比例，结果用全 0 IV 解出来的乱码
        被当成了明文（前 16 字节是控制字符，照样通过了）。
      - 所以加一条硬闸：除 \r\n\t 外不允许出现任何控制字符。真正的明文
        （JSON / 查询串 / 中文）不会有。"""
    if not b or len(b) < 4:
        return False
    try:
        s = b.decode("utf-8")
    except UnicodeDecodeError:
        return False
    for ch in s:
        if ch in "\r\n\t":
            continue
        if not ch.isprintable():
            return False
    printable = sum(1 for ch in s if ch.isprintable() or ch in "\r\n\t")
    if printable < len(s) * 0.95:
        return False
    # 结构特征：JSON / 查询串 / 中文 / 常见业务字段
    if re.search(r'^\s*[{\[]|[{}"\[\]]|=\s*[^&\s]+&|\b(uid|token|data|code|msg|'
                 r'username|password|phone|mobile|id|type|status|sign)\b', s, re.I):
        return True
    if re.search(r'[\u4e00-\u9fff]', s):
        return True
    return False


def to_bytes_pair(val):
    """一个候选值可能有两种解释：utf8 原文 / hex 解码。两种都要试，
    因为 parse(x) 与 from(x,"hex") 在源码里长得像。"""
    outs = []
    b = val.encode("utf-8", errors="ignore")
    outs.append(("utf8", b))
    if re.fullmatch(r"[0-9a-fA-F]+", val) and len(val) % 2 == 0:
        try:
            outs.append(("hex", bytes.fromhex(val)))
        except ValueError:
            pass
    try:
        outs.append(("b64", base64.b64decode(val + "=" * (-len(val) % 4))))
    except Exception:
        pass
    return outs


def normalize_key(b, want):
    """把候选字节裁/补到合法密钥长度。不裁就没法试，裁了要标出来。"""
    if len(b) < want:
        return None
    if len(b) > want:
        return b[:want]
    return b


def try_all(sample_b64, keys, ivs, max_candidates):
    hits = []
    try:
        data = base64.b64decode(sample_b64 + "=" * (-len(sample_b64) % 4))
    except Exception:
        return hits
    if len(data) % 16 != 0 or len(data) < 16:
        # 不是分组密码密文（可能是 RSA/GCM/双层编码）
        return hits

    tried = 0
    kbs = []
    for val, kind, src in keys[:max_candidates]:
        for how, b in to_bytes_pair(val):
            for klen in (16, 24, 32):
                kb = normalize_key(b, klen)
                if kb:
                    kbs.append((kb, klen, f"{kind}/{how}", val, src))
    kbs = kbs[:max_candidates]

    # zero-iv 只用于 ECB 等价物和「真 IV 没提取到」的兜底，排在最后，
    # 避免它抢在真实 IV 前面报一个「看起来解开了」的假命中
    ivbs = []
    for val, kind, src in ivs[:max_candidates]:
        for how, b in to_bytes_pair(val):
            if len(b) >= 16:
                ivbs.append((b[:16], f"{kind}/{how}", val))
    ivbs.append((b"\x00" * 16, "zero-iv", ""))

    for kb, klen, kdesc, kval, ksrc in kbs:
        for mode in ("cbc", "ecb"):
            # ECB 不吃 IV，只试一次；否则同一个 key 会按 IV 数量重复报 N 次
            cur_ivs = ivbs if mode == "cbc" else [(b"\x00" * 16, "n/a", "")]
            for iv, idesc, ival in cur_ivs:
                tried += 1
                out = decrypt_blob(data, kb, iv, mode)
                if out is None:
                    continue
                plain = unpad_pkcs7(out)
                cand = plain if plain is not None else out
                if looks_like_plaintext(cand):
                    hits.append({
                        "key": kval, "key_bytes": kb, "klen": klen,
                        "key_src": ksrc, "key_kind": kdesc,
                        "iv": ival, "iv_kind": idesc, "mode": mode,
                        "plain": cand.decode("utf-8", errors="replace")[:200],
                        "padded": plain is not None,
                    })
    print(f"[*] 尝试组合 {tried} 次", file=sys.stderr)
    return hits


# ---------------------------------------------------------------------------

def main():
    ap = argparse.ArgumentParser(add_help=False)
    ap.add_argument("--url")
    ap.add_argument("--dir")
    ap.add_argument("--file")
    ap.add_argument("--sample", action="append", default=[])
    ap.add_argument("--max-candidates", type=int, default=400)
    ap.add_argument("-h", "--help", action="store_true")
    a = ap.parse_args()

    if a.help:
        print(__doc__)
        return 0
    if not (a.url or a.dir or a.file):
        print("用法: ./poc.sh --url URL | --dir DIR | --file F [--sample B64]...", file=sys.stderr)
        return 1

    if not HAVE_CRYPTO:
        print("[!] 缺少 cryptography：pip install --user cryptography", file=sys.stderr)
        return 1

    blobs = []
    if a.url:
        blobs += collect_from_url(a.url)
    if a.dir:
        blobs += collect_from_dir(a.dir)
    if a.file:
        blobs.append((os.path.basename(a.file),
                      open(a.file, encoding="utf-8", errors="ignore").read()))

    if not blobs:
        print("[-] 没有拿到任何 JS 内容", file=sys.stderr)
        return 2

    total = sum(len(t) for _, t in blobs)
    print(f"[*] 语料 {len(blobs)} 个文件 / {total:,} 字节")

    keys, ivs = extract_candidates(blobs)
    print(f"[*] 候选 key {len(keys)} 个 / 候选 iv {len(ivs)} 个")

    samples = list(a.sample)
    auto = False
    if not samples:
        samples = harvest_samples(blobs)[:20]
        auto = True
        if samples:
            print(f"[!] 未给 --sample，自动挑了 {len(samples)} 个长串（误报高，命中标 [AUTO]）")

    if not samples:
        print("[-] 没有密文样本，无法用解密筛候选。给 --sample <base64密文>")
        print(f"[*] 候选 key 前 10：{[k[0] for k in keys[:10]]}")
        return 2

    any_hit = 0
    seen_sig = set()
    for s in samples:
        hits = try_all(s, keys, ivs, a.max_candidates)
        # 同一 (key,iv,mode) 只报一次；真实 iv 排在 zero-iv 前面
        hits.sort(key=lambda h: (h["iv_kind"] == "zero-iv", not h["padded"]))
        shown = 0
        for h in hits:
            sig = (h["key"], h["iv"], h["mode"], h["plain"][:40])
            if sig in seen_sig:
                continue
            seen_sig.add(sig)
            if shown >= 5:
                continue
            shown += 1
            any_hit += 1
            tag = "[AUTO]" if auto else "[HIT]"
            print(f"\n{tag} sample={s[:24]}...")
            print(f"      key = {h['key']!r}  ({h['klen']}B {h['key_kind']} from {h['key_src']})")
            print(f"      iv  = {h['iv']!r}  ({h['iv_kind']})")
            print(f"      mode= AES-{h['klen']*8}-{h['mode'].upper()}  "
                  f"padding={'pkcs7' if h['padded'] else 'none'}")
            print(f"      明文= {h['plain']!r}")

    if not any_hit:
        print("\n[-] 未找到可解组合。")
        print("    可能原因（按概率排）：")
        print("      1. key 不在本次抓到的文件里（chunk 未全量下取，试 --dir 全量）")
        print("      2. key 由运行时拼接/派生（如 MD5(deviceId)），不是字面量")
        print("      3. 密文样本不对：可能双层 urlencode / 前面还有 base64 一次")
        print("      4. 不是 AES：RSA / SM4 / 国密，换 crypto-gateway-probe 判形态")
        return 2

    print(f"\n[+] 共找到 {any_hit} 组可用组合（见上）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
