---
code: rule-cleartext-login-triage
title: Rule：明文 HTTP 登录页与凭据 POST 的定级边界
kind: rule
category: info-leak
evidence_level: L4
verified_on: 2026-11-03
source: 实战（oauth.ezvizlife.com）
tags: [明文, 中间人, MITM, 凭据, 传输, CWE-319, HSTS, 登录, 定级边界, 密码]
---

# 明文 HTTP 登录页与凭据 POST 的定级边界

## 现象

80 端口不跳 HTTPS，直接 `200` 返回完整登录表单；表单 `action` 是相对路径，浏览器默认向 HTTP 提交；服务端在明文端口接收凭据 POST 并进入登录处理链。

## 判定链（5 步，缺一步就降级）

| # | 检查 | 方法 | 判据 |
|---|---|---|---|
| 1 | 80 端口是否跳转 | `curl -sS -D - -o /dev/null http://T/` | 无 `Location`、无 301/308 |
| 2 | 是否提供完整登录表单 | 取 `/login` 页面 | 含 `action=`、`name="username"`、`name="password"` |
| 3 | 表单提交目标是否也是 HTTP | 看 `action` 是相对路径还是绝对 https | 相对路径 ⇒ 跟随当前协议 |
| 4 | 服务端是否接收明文 POST | `POST http://T/login` | 进入登录处理链（302 到错误页 / 200），不是 400/403 |
| 5 | 传输的是明文还是可重放摘要 | 读前端 JS | `md5(pass)` 可直接重放；PBKDF2+nonce 则不能 |

## 定级

| 条件 | 上限 |
|---|---|
| 需攻击者处于网络路径（MITM/恶意热点/ARP），无真实登录成功证明 | **Medium** |
| 只有根路径 200、无登录表单 | Low / informational |
| 有真实账号在明文通道登录成功并可重放 | 可论证 High，但需要真账号与受控网络 |
| 传输的是加盐 KDF + 一次性 nonce | 降级，摘要不可重放 |

**禁止写法**：「账号接管」——除非真的用这套凭据登录成功并证明会话被夺。

## HSTS 陷阱

响应带 `Strict-Transport-Security` **不代表**首次访问受保护。按 RFC 6797 §8.1，UA 必须忽略经非安全传输收到的 STS。这一条只能作为规范推论，未在真实浏览器验证时不要写成实测结论。

## 反证条件

- 80 端口 301/308 到 https ⇒ 不成立；
- 登录表单 `action` 是绝对 `https://` ⇒ 凭据不落明文；
- 服务端对明文 POST 返回 400/426 ⇒ 拒绝明文，不成立。

## 关联

- `ezviz-oauth-http-plaintext-login`（本条对应的已验证 POC）
- `rule-source-leak-severity`（同站常伴随堆栈/源码泄露，分开定级）
