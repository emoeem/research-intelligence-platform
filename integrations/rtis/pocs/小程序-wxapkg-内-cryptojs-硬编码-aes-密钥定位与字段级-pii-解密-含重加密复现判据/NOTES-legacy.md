#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
小程序硬编码 AES 密钥定位 + 字段级 PII 解密 —— 通用流程

适用：小程序用 CryptoJS 对 SENSITIVE_FIELD 做字段级加密，密钥硬编码在包内。
不适合：crypto_data 整包加密通道若使用 wx.getUserCryptoManager()（微信平台
        每用户下发的 encryptKey/iv），静态密钥解不开，需另想办法。

=== 第 1 步：取出 wxapkg（设备需 root）===
adb shell 'su -c "find /data/data/com.tencent.mm/MicroMsg/appbrand/pkg -name \"*.wxapkg\""'
adb shell 'su -c "cp <path> /sdcard/x.wxapkg; chmod 644 /sdcard/x.wxapkg"'
adb pull /sdcard/x.wxapkg .

=== 第 2 步：解包（标准未加密包 0xBE 魔数）===
import struct, os
def unpack(path, outdir):
    d = open(path,'rb').read()
    assert d[0] == 0xBE, 'magic %02x（可能是加密包）' % d[0]
    i1, idx_len, body_len = struct.unpack('>III', d[1:13])
    assert d[13] == 0xED
    count = struct.unpack('>I', d[14:18])[0]
    p, files = 18, []
    for _ in range(count):
        nl = struct.unpack('>I', d[p:p+4])[0]; p += 4
        name = d[p:p+nl].decode('utf-8','replace'); p += nl
        off, size = struct.unpack('>II', d[p:p+8]); p += 8
        files.append((name, off, size))
    base = 18 + idx_len
    for name, off, size in files:
        dst = os.path.join(outdir, name.lstrip('/'))
        os.makedirs(os.path.dirname(dst) or outdir, exist_ok=True)
        open(dst,'wb').write(d[base+off: base+off+size])
    print('unpacked %d files' % len(files))

=== 第 3 步：定位密钥（关键：先找 CryptoJS 的 Utf8.parse 字面量）===
grep -ob 'enc\.Utf8\.parse("' app-service.js appservice.app.js | head
grep -oE 'enc\.Utf8\.parse\("[^"]{4,64}"\)' app-service.js | sort | uniq -c
# 典型形态（青藤之恋实测）：
#   t = CryptoJS.enc.Utf8.parse("C8T98mXcoiAKMini")   // key 16B
#   n = CryptoJS.enc.Utf8.parse("vpMaQ7OQAG3xMini")   // iv  16B 固定
#   AES.encrypt(data, t, {iv:n, mode:CBC, padding:Pkcs7})

=== 第 4 步：确认密钥服务于哪一层（别搞混）===
grep -o 'SENSITIVE_FIELD={[^;]*' app-service.js | head -c 2000     # 字段级加解密表
grep -o 'getUserCryptoManager[^;]*' app-service.js | head          # ← 若出现且有
grep -o 'cryptoInfo' app-service.js | wc -l                        #   cryptoInfo，说明整包通道
                                                                   #   用的是动态密钥
判据：
  · aes.encrypt(值) 调用时只传 1 个参数  → 落到静态 key/iv（字段级）
  · dynamicCryptoData() 里 cryptoInfo != null → 用微信用户动态密钥（整包）
  ⇒ 两者不可混用；静态密钥解不开 crypto_data 是正常的

=== 第 5 步：解密 + 重加密复现（比只解密强）===
import base64
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
KEY, IV = b'C8T98mXcoiAKMini', b'vpMaQ7OQAG3xMini'
def dec(b64):
    ct = base64.b64decode(b64)
    d = Cipher(algorithms.AES(KEY), modes.CBC(IV)).decryptor()
    raw = d.update(ct) + d.finalize(); n = raw[-1]
    return raw[:-n]
def enc(pt):
    p = 16 - len(pt) % 16; raw = pt + bytes([p])*p
    e = Cipher(algorithms.AES(KEY), modes.CBC(IV)).encryptor()
    return base64.b64encode(e.update(raw)+e.finalize()).decode()

pt = dec('o3Y9U7AoXIhM8uU0yLcT+g==')      # -> 真实姓名
assert enc(pt) == 'o3Y9U7AoXIhM8uU0yLcT+g=='   # ← 逐字节复现才算定死模式与 IV

=== 第 6 步：中文身份证校验（证明不是噪声）===
w = [7,9,10,5,8,4,2,1,6,3,7,9,10,5,8,4,2]; m = '10X98765432'
s = dec('yyYE1R2uAG9agJDgtexjjKs80sbuV8AaOSm3CEnImQA=').decode()
assert m[sum(int(s[i])*w[i] for i in range(17)) % 11] == s[17].upper()
