# eval-packed-aes.js-解包-cryptojs-硬编码密钥解密站点配置-白牌包网-h5

> 状态: `notes_only` — 目标/业务特定，通用脚本会失真。
> 本文件记录**真实技术细节与判定框架**，不是脚本。

## 主题

eval 打包的 aes.js：还原 CryptoJS 硬编码密钥，离线解密站点配置


## 背景

白牌包网的 H5 站点常把配置（线路、支付通道、客服地址、加解密参数）加密后放在
CDN 上，前端用一段 `eval(function(p,a,c,k,e,d){...})` 打包的 JS 解密。这段 JS 里
硬编码了 key 与 iv。

## 还原步骤

1. **找到 eval 块**：`grep -o 'eval(function(p,a,c,k,e,d)' *.js`
2. **不用手工解**：把整个 `eval(...)` 替换成 `console.log(...)` 在浏览器里跑，
   或者用 node 直接执行（注意沙箱，不要在有凭证的环境跑）
3. **去 packer 壳**：`eval(function(p,a,c,k,e,d)` 是 Dean Edwards packer，
   解码后的代码仍然可能是混淆的（变量名 `_0xabc123`）
4. **定位密钥**：在还原后的代码里搜 `CryptoJS.AES`、`createDecipheriv`、
   `Utf8.parse(`、`mode.ECB`、`pad.Pkcs7`
5. **提取 key/iv**：常见形态
   - `Utf8.parse("16位字符串")` → 直接是 key 原文
   - `atob("base64字符串")` → base64 编码的 key
   - `"0123456789abcdef"` → hex
   - 分段拼接：`"dGhhbmtzLH" + "BpZzRjbG91" + "ZA=="` → 要拼起来再 atob

## 验证（必须做重加密复现）

```
pt = decrypt(ct)
assert encrypt(pt) == ct   # 逐字节相同才算确认
```

只解出"看起来像 JSON"的明文不够 —— PKCS7 填充有 1/256 概率蒙对。

## 白牌包网特有的三个坑

1. **同一套 key 全品牌复用**：解开一个品牌，其他品牌的配置也能解
   （这是最值钱的发现，报告里要写"影响同族 N 个站点"）
2. **ECB vs CBC 混淆**：外层函数名叫 `decrypt()` 默认 CBC，但内层组装的
   `{mode: ECB, padding: Pkcs7}` 会覆盖它。搞错模式解出来是乱码
3. **iv 可能全 0**：ECB 下 iv 不参与运算，但代码里仍传 `"0000000000000000"`

## 解密后看什么

| 字段 | 价值 |
|---|---|
| 线路列表 / 备用域名 | 新资产，且常含未上 CDN 的源站 |
| 支付通道配置 | 商户号（脱敏后写）、回调地址 |
| 客服 / 代理入口 | 后台地址，可能有未授权面 |
| 加密参数（signKey 等） | 可伪造请求签名 |
| oss 地址 / bucket 名 | 对象存储，可能可列举 |

## 落地清单

- [ ] 抓 CDN 上的 config.json（通常 `<oss>/cocos/config_data.json`）
- [ ] 还原 eval 块，提取 key/iv
- [ ] 重加密复现验证
- [ ] 解密配置，提取线路与密钥
- [ ] 用同 key 试同族其他站点，确认复用范围


---

## 为什么没有可执行脚本

这类条目的价值在「**针对特定目标的具体契约**」：错误码表、字段名、加密常量、
路由命名。把这些抽象成"通用脚本"，产出的就是一个装满占位符的空壳，
反而比这份说明更容易误导人。

需要自动化时，从中抽出的通用部分已经落到这些脚本里：
- `../crypto_data-加密网关判定法-*/crypto-gateway-probe.py`（判定加密形态）
- `../webpack-spa-前端硬编码-aes-cbc-密钥的定位与复刻-chunk-map-全量下取法/aes-hunter.py`（定位并验证 AES 密钥）
- `../appid-secret-泄露面静态扫描-*/scan-appsecret.py`（批量扫凭证）
- `../webpack-chunk-map-提全量按需-chunk-含-hash-批量抓取/chunk-harvest.py`（抓全量 bundle）

## 待办

- [ ] 若后续对同类目标再遇到一次，把重复出现的部分抽成脚本
- [ ] 每个错误码表补一次实测确认（不同版本会变）
