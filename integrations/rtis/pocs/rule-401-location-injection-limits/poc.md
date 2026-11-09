---
code: rule-401-location-injection-limits
title: Rule：401 响应 Location 头注入的可利用性上限
kind: rule
category: other
evidence_level: L4
verified_on: 2026-11-03
source: 实战（gcsm.oppoit.com）
tags: [401, Location, host-header, X-Forwarded-Host, Referer, OAuth, redirect_uri, 误判防护, 定级上限]
---

# 401 响应 Location 头注入的可利用性上限

## 现象

未认证请求受保护路径，网关回 `401`，并在 `Location` 里拼出 IdP 的 authorize URL；其中 `redirect_uri` / `state` 的 host 由 `X-Forwarded-Host` 或 `Referer` 派生，无白名单。

## 四道削弱条件（逐条实测，缺一不可）

| # | 条件 | 实测方法 | 结果示例 |
|---|---|---|---|
| 1 | 浏览器是否跟随 401 的 Location | 401 不是重定向状态码；带 `Sec-Fetch-Mode: navigate` 复测 | 仍 401，不跟随 |
| 2 | 响应是否可缓存（缓存投毒） | 同 URL 连发两次，比对 `trace_id`、看 `Age`/`X-Cache`/`Cache-Control` | trace_id 不同、无缓存头 |
| 3 | 跨域页面能否注入该头 | OPTIONS 预检看 `Access-Control-Allow-Headers` 是否含 `x-forwarded-host` | 不含，浏览器拦截 |
| 4 | 攻击者是否本就能拼出同一 URL | `client_id` / `redirect_uri` 是否都是 URL 参数 | 是，增量风险 ≈ 0 |

## 定级

- 四道全中（浏览器不跟随、不缓存、不可跨域注入、可直接手拼）⇒ **上限 Low**，可按 informational 提交。
- 若 IdP 未测试 `redirect_uri` 白名单（域外）⇒ 端到端授权码劫持**未证**，禁止写「账号接管」。
- 若响应可缓存 ⇒ 升级为缓存投毒，需重新评估。

## 对照实验（证明是注入而非固定配置）

同网关的 logout 端点若使用写死的 `service=`，则说明只有 401 处理器把请求头当可信输入。这一步是「注入」与「配置如此」的分界线。

## 反证条件

- 401 换成 302 ⇒ 浏览器会跟随，升级；
- 预检允许 `x-forwarded-host` ⇒ 跨域可注入，升级；
- IdP 接受任意 `redirect_uri` ⇒ 端到端成立，升级为 High。

## 关联

- `sso-gateway-401-host-injection-redirect-uri`（本条对应的已验证 POC）
- `rule-src-scope-gate`（IdP 是否在 scope 内，决定能不能继续测）
