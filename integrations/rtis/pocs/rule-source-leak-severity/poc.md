---
code: rule-source-leak-severity
title: Rule：JSP/堆栈/源码片段泄露的定级上限
kind: rule
category: info-leak
evidence_level: L4
verified_on: 2026-11-03
source: 实战（oauth.ezvizlife.com 14 行源码 + ELException）
tags: [JSP, Tomcat, 源码泄露, 堆栈, 异常页, 信息泄露, CVE-209, 定级上限, 风险可接受]
---

# JSP/堆栈/源码片段泄露的定级上限

## 现象

未认证访问 JSP 触发 500，Tomcat/Jasper 错误页打印 JSP 源码行号与代码片段；带参数触发 `ELException` 并打印另一段源码与完整调用栈。

## 判定

| 检查 | 方法 | 说明 |
|---|---|---|
| 是否无需认证 | 无 Cookie 直接请求 | 未认证即可触发 |
| 泄露多少 | 数源码行数、看是否含业务逻辑/密钥 | 本案例 14 行，无密钥 |
| 是否含凭据 | grep `password`/`secret`/`key` | 含则升级 |
| 是否稳定复现 | 两次请求对比 | 每次同样行号 = 稳定 |

## 定级

| 内容 | 上限 |
|---|---|
| 只有行号 + 少量源码片段 + 调用栈 | **Low** |
| 泄露完整源码文件 / 硬编码密钥 / 数据库连接串 | 可到 Medium–High |
| 泄露内容可直接用于构造下一跳利用 | 按链条整体定级，不是单独定 |

多数 SRC 低危 100 元档；也可能被归为「风险可接受」不记奖励。提交时不要把「信息泄露」包装成「源码泄露导致 RCE」。

## 反证条件

- 无 Cookie 返回 500 但错误页是通用 Whitelabel，无源码 ⇒ 不成立；
- 只有堆栈无源码行 ⇒ 更弱，可能不记分；
- 需要登录后才触发 ⇒ 不同类别。

## 关联

- `ezviz-oauth-jsp-error-source-leak`（本条对应的已验证 POC）
- `rule-cleartext-login-triage`（同站明文登录，独立一条）
