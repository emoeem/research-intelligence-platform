# 白牌包网-spa-加密入口密钥硬编码-requestdata-可伪造-解密-内部路由分发器识别

> 状态: `notes_only` — 目标/业务特定，通用脚本会失真。
> 本文件记录**真实技术细节与判定框架**，不是脚本。

## 主题

白牌包网 SPA：requestData 加密入口的密钥硬编码与内部路由分发器识别


## 结构

```
POST /api/dispatch
{"requestData": "<加密串>"}
```

服务端解密后得到 `{ path: "member/getInfo", data: {...} }`，再路由到具体 handler。

## 为什么这是高价值目标

1. **一个入口覆盖全部内部方法** —— 解开信封就拿到整个 API 面
2. **前端路由限制形同虚设** —— 前端不渲染的页面，接口照样能调
3. **密钥硬编码在前端** —— 必然可提取

## 识别是否是"分发器"

判据（三选二即可）：
- 所有业务请求都打到同一个 URL（除了静态资源）
- 请求体只有一两个字段（`requestData` / `data` / `payload`）
- 响应体有统一的 `code` / `msg` / `data` 三层结构
- 前端 bundle 里有 `path:` + `data:` 的组装逻辑

## 密钥提取

在 bundle 里搜：
```
grep -oE 'Utf8\.parse\("[^"]{16,32}"\)' *.js
grep -oE '(aes|AES)[Kk]ey\s*[:=]\s*"[^"]+"' *.js
grep -oE 'atob\("[A-Za-z0-9+/=]{16,}"\)' *.js
```

常见形态：
- `CryptoJS.AES.encrypt(JSON.stringify(p), key, {mode:ECB, padding:Pkcs7})`
- 自定义 `enc()` 函数，包一层 try/catch（失败时原样返回，容易误判为"没加密"）

## 伪造后的攻击面

解开信封后，你能做的：

| 动作 | 说明 |
|---|---|
| 枚举 path | 从 bundle 提候选，测哪个返回业务错而非"path 不存在" |
| 改 data | 改 uid / agentId / hallId 测越权 |
| 去掉签名字段 | 很多实现只对 requestData 加密，不验签 → 直接改明文重加密即可 |
| 降级 | 部分实现支持明文（`requestData` 传 JSON 字符串），试试不加密 |

## 常见误报

- 加密是 base64 而非 AES → 直接解开就是明文，别当加密
- 每次密文不同但明文相同 → 有随机 padding 或 ts 拼在明文里
- 解开后 code=404 "path not found" → 分发器存在，但 path 名不对

## 落地清单

- [ ] 确认是分发器（三选二）
- [ ] 提取 key/iv
- [ ] 重加密复现
- [ ] 从 bundle 提 path 候选
- [ ] 测 path 枚举（限速，命中 20 个就停）
- [ ] 测 data 字段越权


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
