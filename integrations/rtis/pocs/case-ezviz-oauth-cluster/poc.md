---
code: case-ezviz-oauth-cluster
title: Case：EZVIZ oauth 登录入口三连（明文登录 / JSP 泄露 / Host 注入）
kind: case
category: info-leak
evidence_level: L3
verified_on: 2026-11-03
source: 实战（oauth.ezvizlife.com，YSRC）
tags: [EZVIZ, YSRC, oauth, 明文登录, JSP, Host 注入, 同一目标, 案例]
---

# EZVIZ oauth 登录入口三连

## 目标

`oauth.ezvizlife.com`（A `47.88.221.221`），EZVIZ 国际区 Spring Security OAuth2 授权服务器登录入口。链路：阿里云 SLB → nginx → Tomcat/Jasper → Spring Security + SiteMesh + Druid WebStatFilter。

范围依据：YSRC《萤石安全漏洞奖励计划 V2.0》域名表「`ezvizlife.com` 主域名，含其子域名」。

## 三条 finding（同一次评估产出）

| # | id | 问题 | 建议定级 | 关键证据 |
|---|---|---|---|---|
| 1 | 900021 | 80 端口明文提供登录页并接收凭据 POST | Medium | `http://oauth.ezvizlife.com/` 200；`/login.jsp` 200/4043；`POST /login.do` 302；`login.js:25` 提交 `hex_md5` |
| 2 | 900022 | 未认证异常页泄露 JSP 源码与调用栈 | Low | 无 Cookie `/login.jsp` 500，打印 `Cookie[] cookies = request.getCookies()` 与 line 25；带参 500 + `ELException` + line 112 |
| 3 | 900023 | Host 头注入导致登录跳转指向任意域 | Low / informational | `Host: evil.example` → `302 location: https://evil.example/login.jsp` |

## 为什么放在一个 Case 里

三条同源（同一登录入口），但**根因不同、定级不同、修复不同**。合并成一个 Case 是为了避免下次重复评估，而不是把它们打包成一个高危。

## 结论边界

- 未用真实账号完成登录 ⇒ 无账号接管证明；
- 未在真实浏览器验证 HSTS/Secure Cookie 行为 ⇒ 该点只作规范推论；
- Host 注入在浏览器侧不可达（不能设置 Host）⇒ 链未闭环。

## 关联

- `rule-cleartext-login-triage`
- `rule-source-leak-severity`
- `rule-src-scope-gate`
