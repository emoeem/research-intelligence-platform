## 一、格式证明（对比样本）
真实包 dl1.bin（后台下载 `权限节点管理.kdsmod`）：112164 字符 **ASCII base64** → 解码 **84123 字节高熵二进制**。
自造对照（loader 全部接受 base64 包裹）：

| 文件 | 内容 | 含义 |
|---|---|---|
| t1.kdsmod | `a:1:{s:11:"module_info";a:1:{s:5:"title";s:4:"pwn1";}}` | 裸 serialize（无 base64） |
| t2.kdsmod | `YToyOntzOjY6InZlbmRvciI7...` | base64(serialize()) |
| t3.kdsmod | `czo3OiJLRFMxMzM3Ijs=` | base64 的 `s:3:"KDS1337";` |
| t5.kdsmod | `Tzo4OiJzdGRDbGFzcyI6MTp7czoxOiJhIjtzOjE6ImIiO30=` | base64 的**对象** `O:8:"stdClass":1:{s:1:"a";s:1:"b";}` |

⇒ loader = `unserialize( decrypt( base64_decode($file) ) )`，解密**无条件执行**，因此必须产出合法密文。

## 二、密码学测量（python3）
```python
import base64,collections,math
raw=open('dl1.bin','rb').read()
d=base64.b64decode(raw)
print(len(raw),len(d),len(d)%16,len(d)%4,len(set(d)))
def ent(b):
    c=collections.Counter(b);n=len(b)
    return -sum((v/n)*math.log2(v/n) for v in c.values())
print(round(ent(d),4))                      # 7.9977
def ic(b):
    c=collections.Counter(b);n=len(b)
    return sum(v*(v-1) for v in c.values())/(n*(n-1))
print(round(ic(d),6))                       # 0.003907 = 随机
# 短周期密钥检测：位移 L=1..64 的同字节率，最高 0.0054 << 随机显著峰 → 无重复密钥
```

判定：
- `%16 != 0` ⇒ 排除 AES-CBC / AES-ECB（PKCS7）
- `%4  != 0` ⇒ 排除 XXTEA
- 熵 7.9977 + 无 gzip/zip/7z/bz2 魔数 ⇒ 排除 base64(gzinflate)
- IC≈随机 + 无 ≤64 周期 ⇒ 排除单字节/定长重复密钥异或
- ⇒ **流密码族：RC4 / AES-CTR / AES-CFB / AES-OFB / 自定义密钥流**

## 三、候选 key 否定结果（不要再重复跑）
161 个候选 key（`yunqikds`/`YunqiKDS`/`yunqi`/`kds`/`kdsmod`/`sms.qhinfo.net`/`qhinfo`/`qinghaiyunqi`/`yunqi2019..2021`/`kds123456`/`云起KDS`… 各自 raw + md5 + sha1 + sha256 的 hex/digest 形态）
× RC4 / AES-128·192·256-CTR·CFB·OFB
× IV ∈ {全零, 密文前16, 密文末16, key 左填充}
→ **0 命中**（判据：解密前 2–4 字节是否属 `a: s: O: i: b: d: C: PK 1f8b <?ph [ {`）
⇒ key 是授权/站点绑定密钥，非产品名派生。

## 四、key 生成式候选清单（拿到任一原值即可一步验证）
1. `config`/`system_config` 表 `app_key` / `secret` / `license` / `auth_code`
2. `application/config.php`、`application/extra/*.php` 硬编码常量
3. 后台「系统信息 / 授权 / 版本」页的**授权码 / 序列号 / 站点ID**
4. 现场已知常量名：`yunqikds_page_token`、`upload_auth`（若 key=md5(值) 直接可验）
5. 组合：`md5(域名)`、`md5(授权码)`、`md5(授权码.域名)`、`substr(md5(...),0,16|32)`
6. `install.lock` / `data/install/` 安装写入值

验证脚本（拿到候选 key 一步出结果）：
```python
from cryptography.hazmat.primitives.ciphers import Cipher,algorithms,modes
import base64
ct=base64.b64decode(open('dl1.bin','rb').read())
for k in [KEY]:
    for kl in (16,24,32):
        if len(k)!=kl: continue
        for iv in (b'\x00'*16,):
            p=Cipher(algorithms.AES(k),modes.CTR(iv)).encryptor().update(ct[:16])
            print(kl,p[:8], p.startswith((b'a:',b's:',b'O:')))
```

## 五、旁路：IV 复用（比找 key 便宜）
再下载**另一个**模块取第二份密文 C2：
- 若 `C1[:16] == C2[:16]` ⇒ IV/nonce 常量 ⇒ 密钥流复用 ⇒ `P1 XOR P2 = C1 XOR C2`，用 PHP 序列化已知 crib（`a:1:{s:11:"module_info";`）crib-drag 还原明文，并可伪任意明文：`C' = P' XOR C1 XOR P1`。
- 若不同 ⇒ IV 随机，回找 key。

## 六、诚实边界
只有一份密文且无 key 时，RC4/CTR 族**密码学上不可解**，不要尝试离线爆破。