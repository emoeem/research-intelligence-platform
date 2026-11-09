# 白牌包网-lobby-线路配置离线解密-pig4cloud-前端硬编码-aes-密钥-ecb-pkcs7

> 状态: `notes_only` — 目标/业务特定，通用脚本会失真。
> 本文件记录**真实技术细节与判定框架**，不是脚本。

## 主题

白牌包网 lobby 线路配置离线解密（Pig4Cloud 模板）


## 泄漏入口

```
<oss>/cocos/config_data.json
或页面内 window.LOBBY_SITE_CONFIG.INJECT_DATA.ossGetSiteUrlConfig.data.data
```

内容是 base64 的 AES 密文。

## 算法（逆向自 commonChunk bundle）

```js
key = window.atob("dGhhbmtzLH" + "BpZzRjbG91" + "ZA==")   // "thanks,pig4cloud"
iv  = Utf8.parse("5421698523" + "412578")                  // ECB 下不使用
Sn.ossDecrypt -> ot.AES.decrypt(data, key, {mode: ot.mode.ECB, padding: ot.pad.Pkcs7})
```

**关键点**：外层 `decrypt()` 默认是 CBC/ZeroPadding，但被 `ossDecrypt` 组装的
`{mode:ECB, padding:Pkcs7}` **覆盖**，实际生效的是 AES-128-ECB。这一点是能不能解开的
分界线 —— 按 CBC 解会得到乱码，然后误判"key 错了"。

## 配套脚本

`poc.py`（同目录）已实现：

```
./poc.py <本地json文件>
./poc.py https://oss.xxx.com/cocos/config_data.json
```

## 解密后看什么

| 字段 | 价值 |
|---|---|
| 线路列表（域名/IP） | 新资产，常含未上 CDN 的源站 |
| 支付通道 | 商户号、回调地址 |
| 客服/代理入口 | 后台地址，可能有未授权面 |
| signKey / aesKey | 可伪造请求签名 |

## 同族复用

`thanks,pig4cloud` 是 Pig4Cloud 脚手架自带的**默认 JWT 签名密钥**，很多基于它的项目
从来不改。所以这个 key 的命中率高得离谱 —— 解开一个站点后，应该：

1. 用同 key 试同族其他站点（同一套模板铺的白牌站）
2. 顺手试 Pig4Cloud 的默认 JWT：用这个 key 签一个 HS256 token，看能不能登进去

第 2 条是**独立的高危发现**，比配置泄露严重得多。报告里要单独立项。

## 常见误报

- 解出来是合法 JSON 但字段全是 null → key 对，但站点确实没配线路
- base64 里有换行/空格 → 要先去掉再解码
- 长度不是 4 的倍数 → 末尾 `=` 被截断，补上再试

## 落地清单

- [ ] 从页面或 OSS 拿密文
- [ ] 用 poc.py 解密
- [ ] 提取线路与密钥
- [ ] 试同族站点
- [ ] **单独测 Pig4Cloud 默认 JWT 密钥**


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
