#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
加密网关形态判定器  (crypto-gateway-probe)

为什么写这个：
    「站点参数加密了」不是一个结论，是一个岔路口。不同形态的下一步完全不同：
      · 硬编码 AES            → 去 bundle 找 key，恢复任意请求（aes-hunter）
      · RSA 混合              → 公钥只能加密，除非服务端把私钥也下发了
      · GCM + 随机 nonce      → 本路基本封死，转去别的通道
      · 不是加密只是编码      → 直接解，不需要逆向
    所以先判形态，能省掉大量无用功。本脚本全部结论来自**密文本身的结构**，
    零请求、零风险。

判定原理（为什么这几个指标有判别力）：
    1. len % 16：分组密码（AES/DES/SM4）必然对齐；流密码/编码不一定。
    2. 去重率：同一段明文重复请求，若密文每次都不同 → 有随机 IV/nonce；
       若完全相同 → 确定性（ECB 或固定 IV），**这是「可批量伪造」的强信号**。
    3. 跨长度共享首块：两组不同长度的密文前 16 字节相同 → ECB，或 CBC 固定 IV。
       这一条是决定性的，比看代码快。
    4. 定长前缀：每组密文都有一段长度固定、内容每次不同的前缀 → RSA 混合
       （典型 256B = RSA-2048 密文）。
    5. 长度差：len(密文) - len(明文) 恒定 → 固定开销（padding / nonce / tag）。

用法：
    ./poc.sh --samples samples.txt
    ./poc.sh --samples samples.txt --plain plain.txt      # 有明文对，判定更准
    ./poc.sh --pubkey 'MIIBIjANBgkq...'                   # 判公钥还是私钥
    ./poc.sh --config https://TARGET/config?appid=X       # 抓配置里的密钥字段

samples.txt：每行一个密文。支持 base64 / hex / 双层 urlencode 混排，
脚本自动识别并解码。空行和 # 注释忽略。

退出码：0 判定成功 / 1 参数错误 / 2 样本不足无法判定
"""
import argparse
import base64
import binascii
import json
import re
import sys
import urllib.parse

try:
    from cryptography.hazmat.primitives.serialization import (
        load_der_private_key, load_der_public_key, load_pem_private_key,
        load_pem_public_key,
    )
    HAVE_CRYPTO = True
except ImportError:
    HAVE_CRYPTO = False


# ---------------------------------------------------------------------------
# 解码：密文在真实流量里常被套多层
# ---------------------------------------------------------------------------
def decode_one(s):
    """返回 (bytes, 说明)。识别顺序：hex → base64 → 双层 urlencode 后重来。"""
    s0 = s.strip()
    if not s0 or s0.startswith("#"):
        return None, "skip"

    layers = []
    cur = s0
    # 最多剥 3 层 urlencode（客户端常见双层 quote(quote(v))）
    for _ in range(3):
        nxt = urllib.parse.unquote(cur)
        if nxt == cur:
            break
        layers.append("url")
        cur = nxt

    # hex？
    if re.fullmatch(r"[0-9a-fA-F]+", cur) and len(cur) % 2 == 0 and len(cur) >= 32:
        try:
            return bytes.fromhex(cur), "+".join(layers + ["hex"])
        except ValueError:
            pass

    # base64（补 padding）
    try:
        b = base64.b64decode(cur + "=" * (-len(cur) % 4), validate=False)
        if len(b) >= 16:
            return b, "+".join(layers + ["b64"])
    except (binascii.Error, ValueError):
        pass

    return None, "undecodable"


def entropy_like(b):
    """粗略的字节分布均匀度：0=全同字节，1=均匀。用于区分 RSA 段与填充。"""
    if not b:
        return 0.0
    from collections import Counter
    c = Counter(b)
    n = len(b)
    # 归一化香农熵 / log2(256)
    import math
    h = -sum((v / n) * math.log2(v / n) for v in c.values())
    return h / 8.0


# ---------------------------------------------------------------------------
# 判定
# ---------------------------------------------------------------------------
def analyze(blobs, plains):
    n = len(blobs)
    out = {"n": n, "notes": [], "verdict": None, "confidence": "low"}

    lens = [len(b) for b in blobs]
    uniq = {b for b in blobs}
    uniq_count = len(uniq)
    dup_rate = 1 - uniq_count / n
    is_deterministic = uniq_count == 1

    out["len_min"] = min(lens)
    out["len_max"] = max(lens)
    out["len_distinct"] = sorted(set(lens))
    out["dup_rate"] = dup_rate
    out["uniq_count"] = uniq_count
    out["entropy"] = [round(entropy_like(b), 3) for b in blobs[:5]]

    aligned = all(l % 16 == 0 for l in lens)
    out["aligned16"] = aligned

    # --- 跨块共享检测 ---
    first16 = {}
    for b in blobs:
        if len(b) >= 32:
            first16.setdefault(b[:16], 0)
            first16[b[:16]] += 1
    shared_first = max(first16.values()) if first16 else 0
    out["shared_first_block"] = shared_first

    # --- RSA 混合检测 ---
    # 判据：存在一个长度 c（1024/2048/3072/4096 bit 对应的密文长度），使得
    #   (a) 每个样本前 c 字节两两不同
    #   (b) 前 c 字节的字节熵 > 0.95（接近均匀随机）
    #   (c) 剩余部分 16 字节对齐（对称段）
    # 注意：单看密文无法 100% 区分「RSA 混合」与「长随机段 CBC」，
    # 所以这一条只给 SUSPECTED，定案需要 --plain 对。
    rsa_prefix = 0
    # 从大到小试：否则 128B 前缀永远先命中（它只是 256B 前缀的一部分），
    # 会把 RSA-2048 误报成 RSA-1024。
    cands = [512, 384, 256, 128]
    for c in cands:
        if not all(l > c + 15 for l in lens):
            continue
        prefixes = {b[:c] for b in blobs}
        if len(prefixes) != n:
            continue
        if not all((l - c) % 16 == 0 for l in lens):
            continue
        ents = [entropy_like(b[:c]) for b in blobs[:5]]
        # 实测：256 字节随机段的归一化熵约 0.90（不是 1.0，取样偏差决定的）；
        # 32 字节 AES 密文约 0.60。0.85 能分开这两档。
        if min(ents) > 0.85:
            rsa_prefix = c
            break
    out["rsa_prefix"] = rsa_prefix

    # --- 明密文长度关系 ---
    # 「固定开销 16」这个判据是错的：PKCS7 的开销 = 16 - (len(pt) % 16)，
    # 随明文长度变化。正确判据是「密文长度恰为明文按 16 向上取整 + 一个整块」。
    overhead = None
    pkcs7_aligned = False
    if plains and len(plains) == n:
        diffs = {len(b) - len(p) for b, p in zip(blobs, plains)}
        if len(diffs) == 1:
            overhead = diffs.pop()
        pkcs7_aligned = all(
            len(c) == (len(p) // 16 + 1) * 16 for c, p in zip(blobs, plains))
    out["overhead"] = overhead
    out["pkcs7_aligned"] = pkcs7_aligned

    # --- 收敛到结论 ---
    if not aligned:
        if dup_rate == 1.0:
            out["verdict"] = "DETERMINISTIC-NONBLOCK"
            out["notes"].append("长度非 16 对齐但完全确定 → 多半是编码/hash 而非分组密码")
        else:
            out["verdict"] = "STREAM-OR-ENCODING"
            out["notes"].append("长度非 16 对齐 → 流密码（RC4/ChaCha）或纯编码（base64/压缩）")
        out["confidence"] = "medium"
        return out

    if rsa_prefix:
        out["verdict"] = "RSA-HYBRID-SUSPECTED"
        out["notes"].append(
            f"检测到 {rsa_prefix}B 高熵且两两不同的前缀，剩余部分 16 字节对齐 "
            f"→ 疑似 RSA-{rsa_prefix*8} 段 + 对称段 混合封装")
        out["notes"].append(
            "单看密文无法与「长随机段 CBC」区分，需 --plain 明密文对定案")
        out["confidence"] = "medium"
        return out

    if is_deterministic:
        out["verdict"] = "DETERMINISTIC"
        out["notes"].append(
            f"{n} 组样本完全相同 → 同一明文产出同一密文 → ECB 或固定 IV + 静态 key")
        out["notes"].append("风险最高的一类：可直接重放，也可以离线伪造任意明文")
        out["confidence"] = "high"
        return out

    if shared_first >= 2:
        out["verdict"] = "ECB-OR-FIXED-IV"
        out["notes"].append(
            f"{shared_first}/{n} 组样本共享同一首 16 字节块，但整体不完全相同 "
            f"→ ECB，或 CBC 配固定 IV（明文首块相同）")
        out["notes"].append("这是可批量伪造的强信号：不需要每次抓新密文")
        out["confidence"] = "high"
        return out

    if out.get("pkcs7_aligned") and all(l % 16 == 0 for l in lens):
        out["verdict"] = "CBC-PKCS7-RANDOM-IV"
        out["notes"].append(
            f"密文长度 = 明文按 16 向上取整 + 一个整块（实测开销 {overhead}B），"
            f"且每次密文不同 → CBC + PKCS7 + 随机 IV")
        out["notes"].append("随机 IV 意味着无法用同一段密文重放，但 key 若在 bundle 里仍可自造")
        out["confidence"] = "medium"
        return out

    out["verdict"] = "AES-BLOCK-UNKNOWN"
    out["notes"].append("16 字节对齐、有随机性，但未命中更具体的形态特征")
    out["notes"].append("下一步：给 --plain 提供明密文对，或直接上 aes-hunter 找 key")
    out["confidence"] = "low"
    return out


# ---------------------------------------------------------------------------
# 密钥类型判定（公钥 vs 私钥）
# ---------------------------------------------------------------------------
def check_key(blob):
    if not HAVE_CRYPTO:
        return "需要 cryptography 库"
    try:
        raw = base64.b64decode(blob + "=" * (-len(blob) % 4))
    except Exception:
        return "不是合法 base64"
    for loader, kind in (
        (load_der_private_key, "DER-PrivateKey"),
        (load_der_public_key, "DER-PublicKey"),
    ):
        try:
            k = loader(raw, password=None)
            if kind.startswith("DER-Public"):
                pn = k.public_numbers()
                return (f"{kind}  {pn.n.bit_length()}bit e={pn.e}\n"
                        f"      只能加密，不能解密（{pn.n.bit_length()} 位模数）")
            return f"{kind}  ** 含私钥 ** 可直接解密全部流量"
        except Exception:
            continue
    # PEM
    for loader, kind in ((load_pem_private_key, "PEM-PrivateKey"),
                         (load_pem_public_key, "PEM-PublicKey")):
        try:
            k = loader(blob.encode(), password=None)
            return f"{kind}  {k.key_size}bit"
        except Exception:
            continue
    return "无法识别（可能不是标准 RSA/EC 密钥）"


def fetch_config(url):
    import urllib.request
    req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
    with urllib.request.urlopen(req, timeout=15) as r:
        return r.read().decode("utf-8", errors="replace")


def walk_keys(obj, path=""):
    """从配置 JSON 里捞出所有像密钥的字段"""
    hits = []
    if isinstance(obj, dict):
        for k, v in obj.items():
            p = f"{path}.{k}" if path else k
            if isinstance(v, str) and len(v) >= 16 and re.search(
                    r"(?i)key|secret|pub|priv|cert|salt", k):
                hits.append((p, v))
            else:
                hits.extend(walk_keys(v, p))
    elif isinstance(obj, list):
        for i, v in enumerate(obj):
            hits.extend(walk_keys(v, f"{path}[{i}]"))
    return hits


# ---------------------------------------------------------------------------

def main():
    ap = argparse.ArgumentParser(add_help=False)
    ap.add_argument("--samples")
    ap.add_argument("--plain")
    ap.add_argument("--pubkey")
    ap.add_argument("--config")
    ap.add_argument("-h", "--help", action="store_true")
    a = ap.parse_args()

    if a.help:
        print(__doc__)
        return 0
    if not (a.samples or a.pubkey or a.config):
        print("用法: ./poc.sh --samples FILE | --pubkey B64 | --config URL", file=sys.stderr)
        return 1

    rc = 0

    # ---- 密钥类型 ----
    if a.pubkey:
        print("== 密钥类型判定 ==")
        print("   " + check_key(a.pubkey))
        print()

    # ---- 配置抓取 ----
    if a.config:
        print("== 配置抓取 ==")
        try:
            txt = fetch_config(a.config)
            try:
                j = json.loads(txt)
                hits = walk_keys(j)
                if hits:
                    for p, v in hits[:20]:
                        print(f"   {p} = {v[:70]}{'...' if len(v) > 70 else ''}")
                    print()
                    if HAVE_CRYPTO:
                        for p, v in hits[:5]:
                            print(f"   [{p}] " + check_key(v))
                else:
                    print("   响应里没有疑似密钥字段")
                    print(f"   前 200 字节: {txt[:200]}")
            except json.JSONDecodeError:
                print(f"   非 JSON，前 200 字节: {txt[:200]}")
        except Exception as e:
            print(f"   [!] 抓取失败: {e}")
        print()

    # ---- 密文形态 ----
    if not a.samples:
        return rc

    raw = open(a.samples, encoding="utf-8", errors="ignore").read().splitlines()
    blobs, descs = [], []
    for line in raw:
        b, d = decode_one(line)
        if b:
            blobs.append(b)
            descs.append(d)

    if len(blobs) < 2:
        print(f"[-] 有效样本 {len(blobs)} 个，至少需要 2 个才能判形态", file=sys.stderr)
        return 2

    plains = []
    if a.plain:
        for line in open(a.plain, encoding="utf-8", errors="ignore"):
            if line.strip() and not line.startswith("#"):
                plains.append(line.rstrip("\n").encode())

    print(f"== 密文形态（{len(blobs)} 个样本）==")
    from collections import Counter
    print(f"   解码方式分布: {dict(Counter(descs))}")

    r = analyze(blobs, plains)
    print(f"   长度: min={r['len_min']} max={r['len_max']} 不同长度={len(r['len_distinct'])}")
    print(f"   16 字节对齐: {'是' if r['aligned16'] else '否'}")
    print(f"   去重后仅 {r['uniq_count']}/{r['n']} 种（重复率 {r['dup_rate']*100:.0f}%）")
    print(f"   共享首 16B 块的最大样本数: {r['shared_first_block']}")
    if r["rsa_prefix"]:
        print(f"   RSA 前缀: {r['rsa_prefix']} 字节")
    if r["overhead"] is not None:
        print(f"   固定开销: {r['overhead']} 字节")
    print(f"   字节熵(前5): {r['entropy']}")

    print(f"\n   判定: {r['verdict']}   置信度={r['confidence']}")
    for n_ in r["notes"]:
        print(f"     · {n_}")

    print("\n   下一步建议:")
    v = r["verdict"]
    if v in ("ECB-OR-FIXED-IV", "DETERMINISTIC", "DETERMINISTIC-NONBLOCK"):
        print("     判定为确定性加密 → 上 aes-hunter 找 bundle 里的 key，"
              "拿到的 key 可直接伪造任意请求")
    elif v == "RSA-HYBRID":
        print("     先用 --pubkey 确认服务端下发的是公钥还是私钥。"
              "是公钥则本路只能加密不能解密，转去其他通道（明文接口/内部 API）")
    elif v == "CBC-PKCS7-RANDOM-IV":
        print("     随机 IV 不影响伪造（IV 通常随密文一起传）。照样上 aes-hunter 找 key")
    elif v == "STREAM-OR-ENCODING":
        print("     先确认是不是纯编码：尝试 zlib / gzip 解开，"
              "或看是否有 'Salted__' 头（openssl enc 格式）")
    else:
        print("     形态未定 → 补 --plain 明密文对，或提供更多同接口样本")

    return rc


if __name__ == "__main__":
    sys.exit(main())
