#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
wxapkg 内 CryptoJS 硬编码 AES 密钥定位 + 字段级解密验证

和「grep 出一个 32 位 hex 就当密钥」的区别：本脚本必须通过**重加密复现**
才算命中——用找到的 key/iv 把明文加密回原密文，逐字节相同才认。

三步：
  1. 定位：在 app-service.js 里找 CryptoJS.AES.encrypt / .decrypt 调用点，
     向上回溯 20 行找 key/iv 的字面量赋值
  2. 提取：把 key/iv 归一化成 bytes，自动判断是 Hex 还是 Base64 还是 UTF-8 原文
  3. 验证：
       a) 对已知密文解密 → 结果是合法 UTF-8 且不含控制字符（严格的填充校验）
       b) 重加密复现 → encrypt(decrypt(c)) == c
     两条都过才算 CONFIRMED

填充校验的坑：PKCS7 只要末字节 n 满足 n<=16 且最后 n 字节都等于 n 就"合法"，
随机密文有 1/256 的概率蒙对。所以必须靠重加密复现做交叉验证。

用法：
  ./wxapkg-aes.py <解包目录> [--cipher <base64或hex密文>] [--json out.json]
示例：
  ./wxapkg-aes.py ./unpacked --cipher "U2FsdGVkX1+abc..."
"""
import base64
import json
import os
import re
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "_lib"))
import common as C  # noqa: E402

# 没有 pycryptodome 也能干活：优先用它（快、稳），没有就报清楚缺什么
try:
    from Crypto.Cipher import AES  # type: ignore
    from Crypto.Util.Padding import pad, unpad  # type: ignore
    HAVE_CRYPTO = True
except ImportError:
    HAVE_CRYPTO = False

# CryptoJS 调用点
RE_AES_CALL = re.compile(
    r'(?:CryptoJS\.)?AES\.(?:encrypt|decrypt)\s*\(|'
    r'createCipheriv?|createDecipheriv?|'
    r'aesEncrypt|aesDecrypt|encryptData|decryptData',
    re.I)

# 键名 → 字面量
RE_KEY_ASSIGN = re.compile(
    r'(?i)\b(?:var|let|const)?\s*([A-Za-z_$][\w$]*(?:key|KEY|Key|iv|IV|Iv|secret|SECRET))\s*[:=]\s*'
    r'["\']([^"\']{8,64})["\']')

# CryptoJS 常见的直接内联：AES.encrypt(data, CryptoJS.enc.Utf8.parse("xxx"), ...)
RE_INLINE = re.compile(
    r'(?:Utf8|Hex|Base64)\.parse\s*\(\s*["\']([^"\']{8,64})["\']')


def to_bytes(s: str) -> bytes | None:
    """把字面量转成 bytes。顺序很重要：先按 Hex/Base64 试，都不行当 UTF-8。"""
    s = s.strip()
    if re.fullmatch(r'[0-9a-fA-F]{16}|[0-9a-fA-F]{24}|[0-9a-fA-F]{32}|[0-9a-fA-F]{48}|[0-9a-fA-F]{64}', s):
        try:
            b = bytes.fromhex(s)
            if len(b) in (16, 24, 32):
                return b
        except ValueError:
            pass
    if re.fullmatch(r'[A-Za-z0-9+/=_-]{16,88}', s):
        try:
            b = base64.b64decode(s.replace("-", "+").replace("_", "/") + "==", validate=False)
            if len(b) in (16, 24, 32):
                return b
        except Exception:
            pass
    b = s.encode()
    if len(b) in (16, 24, 32):
        return b
    return None


def pkcs7_unpad_checked(data: bytes) -> bytes | None:
    """严格 PKCS7 去填充。比 Crypto.Util 的 unpad 多一层「明文可打印」检查。"""
    if not data or len(data) % 16 != 0:
        return None
    n = data[-1]
    if n < 1 or n > 16:
        return None
    if data[-n:] != bytes([n]) * n:
        return None
    pt = data[:-n]
    if not pt:
        return None
    # 明文必须是合法 UTF-8 且不含除 \r\n\t 外的控制字符。
    # 这条是核心：随机数据蒙过填充的概率是 1/256，蒙过 UTF-8+可打印的概率极低。
    try:
        text = pt.decode("utf-8")
    except UnicodeDecodeError:
        return None
    for ch in text:
        if ord(ch) < 0x20 and ch not in "\r\n\t":
            return None
    return pt


def aes_dec(key: bytes, iv: bytes | None, ct: bytes, mode="cbc"):
    if not HAVE_CRYPTO:
        return None
    try:
        if mode == "cbc":
            if len(ct) % 16 != 0:
                return None
            c = AES.new(key, AES.MODE_CBC, iv or b"\x00" * 16)
            return pkcs7_unpad_checked(c.decrypt(ct))
        else:
            c = AES.new(key, AES.MODE_ECB)
            return pkcs7_unpad_checked(c.decrypt(ct))
    except Exception:
        return None


def aes_enc(key: bytes, iv: bytes | None, pt: bytes, mode="cbc"):
    if not HAVE_CRYPTO:
        return None
    try:
        if mode == "cbc":
            c = AES.new(key, AES.MODE_CBC, iv or b"\x00" * 16)
            return c.encrypt(pad(pt, 16))
        return AES.new(key, AES.MODE_ECB).encrypt(pad(pt, 16))
    except Exception:
        return None


def find_candidates(root):
    cands = []
    for dp, _, fns in os.walk(root):
        for fn in fns:
            if not fn.endswith((".js", ".wxs", ".json")):
                continue
            p = os.path.join(dp, fn)
            try:
                if os.path.getsize(p) > 30 * 1024 * 1024:
                    continue
                with open(p, "r", encoding="utf-8", errors="ignore") as f:
                    lines = f.readlines()
            except OSError:
                continue

            for i, line in enumerate(lines):
                if not RE_AES_CALL.search(line):
                    continue
                # 回溯 20 行找 key/iv
                ctx = "".join(lines[max(0, i - 20):i + 3])
                for m in RE_KEY_ASSIGN.finditer(ctx):
                    name, val = m.group(1), m.group(2)
                    b = to_bytes(val)
                    if b:
                        cands.append({"file": p, "line": i + 1, "name": name,
                                      "raw": val, "bytes": b.hex()})
                for m in RE_INLINE.finditer(ctx):
                    b = to_bytes(m.group(1))
                    if b:
                        cands.append({"file": p, "line": i + 1, "name": "inline",
                                      "raw": m.group(1), "bytes": b.hex()})
    # 去重
    seen = set()
    uniq = []
    for c in cands:
        k = (c["bytes"], c["file"], c["line"])
        if k in seen:
            continue
        seen.add(k)
        uniq.append(c)
    return uniq


def main():
    if len(sys.argv) < 2:
        C.usage_die(__doc__.strip().split("用法：")[0] + "用法：\n  ./wxapkg-aes.py <解包目录> [--cipher <密文>]\n示例：\n  ./wxapkg-aes.py ./unpacked --cipher \"U2FsdGVkX1...\"")

    root = sys.argv[1]
    cipher_text = ""
    if "--cipher" in sys.argv:
        cipher_text = sys.argv[sys.argv.index("--cipher") + 1]

    if not os.path.isdir(root):
        C.err(f"不是目录: {root}")
        sys.exit(1)

    C.hdr("0. 依赖检查")
    if HAVE_CRYPTO:
        C.ok("pycryptodome 可用")
    else:
        C.warn("缺 pycryptodome → 只能定位密钥，无法验证")
        C.dim("装法: pip install pycryptodome  （或 pacman -S python-pycryptodome）")

    C.hdr("1. 定位 CryptoJS AES 调用点与密钥")
    cands = find_candidates(root)
    if not cands:
        C.warn("未找到 AES 调用点 / 密钥字面量")
        print("""可能原因（按概率）：
  1) 密钥不在小程序包里，由后端下发（拦截 /api/config 之类的接口找 key）
  2) 用了非 CryptoJS 实现（自己写的 openssl native 插件，key 在 .so 里）
  3) 代码被压缩成单行，回溯 20 行拿不到上下文 —— 改用下面的手工方法：
     在 app-service.js 里搜 "AES" 前后各 500 字符：
       grep -o '.\\{0,500\\}AES\\.encrypt.\\{0,500\\}' app-service.js""")
        C.verdict("wxapkg-aes", "NO_KEY", "0 个候选")
        return 2

    C.ok(f"{len(cands)} 个候选（key/iv 字面量）")
    seen_b = set()
    uniq = []
    for c in cands:
        if c["bytes"] in seen_b:
            continue
        seen_b.add(c["bytes"])
        uniq.append(c)
    for c in uniq[:12]:
        C.dim(f"  {c['name']:<14} {c['raw'][:40]:<42} → {len(bytes.fromhex(c['bytes']))*8}bit  {os.path.relpath(c['file'], root)}:{c['line']}")

    if not HAVE_CRYPTO:
        C.verdict("wxapkg-aes", "LOCATED", f"{len(uniq)} 个密钥，未验证")
        return 0

    C.hdr("2. 验证")
    if not cipher_text:
        C.warn("未提供 --cipher，无法做解密+重加密复现")
        print("""拿到密文的方法：
  · 抓小程序的一次请求，把加密字段的值拷出来
  · 或者从包里找内置的示例数据（grep 一个 base64 长串）
  注意：CryptoJS 默认 encrypt 会输出 "Salted__" 开头的 base64，
        那说明用的是 passphrase 派生模式（EvpKDF），不是直接用 key/iv。
        本脚本只处理「直接用 key」的模式；Salted__ 开头需要额外做 EvpKDF 派生。""")
        C.verdict("wxapkg-aes", "LOCATED", f"{len(uniq)} 个密钥，缺密文")
        return 0

    # 密文归一化：支持 base64 与 hex
    ct = None
    if re.fullmatch(r'[0-9a-fA-F]+', cipher_text) and len(cipher_text) % 2 == 0:
        ct = bytes.fromhex(cipher_text)
        C.dim("密文格式: hex")
    else:
        try:
            ct = base64.b64decode(cipher_text + "==", validate=False)
            C.dim("密文格式: base64")
        except Exception:
            C.err("密文既不是 hex 也不是 base64")
            sys.exit(1)

    if ct[:8] == b"Salted__":
        C.warn("密文以 Salted__ 开头 → CryptoJS passphrase 模式（EvpKDF + MD5）")
        C.dim("本脚本不处理该模式。需要先用 passphrase + salt 派生 key/iv")
        C.verdict("wxapkg-aes", "SALTED_MODE", "需 EvpKDF 派生")
        return 3

    C.dim(f"密文长度 {len(ct)} 字节")

    confirmed = []
    for c in uniq:
        kb = bytes.fromhex(c["bytes"])
        if len(kb) not in (16, 24, 32):
            continue
        for mode in ("cbc", "ecb"):
            iv = None
            if mode == "cbc":
                # iv 优先用同名候选里含 iv 的
                ivcand = [x for x in uniq if "iv" in x["name"].lower() and len(bytes.fromhex(x["bytes"])) == 16]
                iv = bytes.fromhex(ivcand[0]["bytes"]) if ivcand else b"\x00" * 16
            pt = aes_dec(kb, iv, ct, mode)
            if pt is None:
                continue
            # 重加密复现
            rc = aes_enc(kb, iv, pt, mode)
            reproduced = (rc == ct)
            tag = "CONFIRMED" if reproduced else "PLAINTEXT_ONLY"
            C.ok(f"[{tag}] {mode.upper()} key={c['raw'][:20]}… iv={(iv.hex()[:16] if iv else 'none')}")
            C.dim(f"    明文: {pt[:120].decode('utf-8', 'replace')}")
            if reproduced:
                C.dim("    重加密复现: 通过（逐字节相同）")
                confirmed.append({"key": c["raw"], "key_hex": kb.hex(),
                                  "iv_hex": (iv.hex() if iv else ""), "mode": mode,
                                  "plaintext": pt.decode("utf-8", "replace")[:500]})
            else:
                C.warn("    重加密复现: 失败 → 可能是 key 对但 iv 错，或模式不对")

    C.hdr("结论")
    if confirmed:
        C.warn(f"{len(confirmed)} 组 key/iv 通过重加密复现 → 确认可用于伪造/解密")
        print("""
报告要写清的三件事：
  1) key/iv 出现在哪个文件哪一行（可复现定位）
  2) 解出的明文示例（脱敏，只留能证明危害的字段名，不要贴真实用户数据）
  3) 影响：能**解密任意响应** = 可读全部用户数据；能**加密任意请求** = 可伪造任意身份
     —— 后者更严重，因为它把「可读」变成了「可写」
修复建议：密钥由后端按会话签发（每次请求不同），前端只持有公钥做 RSA 加密""")
        C.verdict("wxapkg-aes", "CONFIRMED", f"{len(confirmed)} 组")
    else:
        C.ok("无 key 通过重加密复现")
        C.dim("常见原因：iv 不对（CryptoJS 常见 iv = key），或用了 GCM（有 tag）")
        C.verdict("wxapkg-aes", "NOT_CONFIRMED", "候选 key 均未复现")
    return 0


if __name__ == "__main__":
    sys.exit(C.main_guard(main)())
