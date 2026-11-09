---
code: method-postmessage-origin-audit
title: Method：支付/敏感 iframe 的 postMessage 信任边界审计法
kind: method
category: other
evidence_level: L4
verified_on: 2026-11-03
source: 实战（iframes.airbnbpayments.com）
tags: [postMessage, iframe, origin, targetOrigin, 支付, 卡表单, 跨源, 信任边界, 中间人, 凭据]
---

# 支付/敏感 iframe 的 postMessage 信任边界审计法

## 一句话

如果 iframe 的 `targetOrigin` 和入站 `event.origin` 比较值**都来自 URL 参数**，且白名单校验的返回值被丢弃，那么任何第三方源都满足比较条件——这是跨源信任边界缺陷，不是普通 CORS 误配。

## 五步审计

| # | 步骤 | 命令/方法 | 命中特征 |
|---|---|---|---|
| 1 | 找消息总线 | `grep -oE '.{0,120}postMessage\(.{0,160}' bundle.js` | 出现 `targetOrigin` / `this.origin` / `"*"` |
| 2 | 找 origin 解析 | `grep -nE 'location\.search' bundle.js` | 从 query 取 `origin` 参数 |
| 3 | 看校验返回值 | 读解析函数上下文 | `validate(x), x` 或 `void validate(x)` —— 求值不分支 = 洞 |
| 4 | 看白名单正则 | 找 `getUrlRegExp` / `new RegExp` | `\airbnb\.com` 少转义一个点 ⇒ `evilairbnb.com` 命中 |
| 5 | 浏览器正/负对照 | 本地起 `127.0.0.1:PORT` 页面嵌入 iframe | 正例收到消息，负对照（用官方源）0 条 |

## 判定矩阵

| 现象 | 结论 |
|---|---|
| `origin=` 指向第三方源，父页收到 `iframeReady`/`changeHeight` | targetOrigin 由参数决定 |
| 父页发控制消息，1 ms 内收到回包 | 入站 origin 校验由参数决定 |
| `origin=` 改成官方域名，0 条消息 | 负对照成立，不是浏览器噪声 |
| `origin=*` 仍广播 | 白名单完全缺失 |
| iframe 顶层 DOM 含真实 `<input name="credit-card-number">` | 卡表单可被任意源嵌入 |

## 危害上限（必须说清）

- 能证明：任意第三方源可嵌入表单、收发消息、拿到卡元数据（bin/bin8/lastFour/exp）。
- **不能**直接证明：可用受害卡完成扣款。需要攻击者自有 `clientToken`/`merchantId` 把 nonce 变成可扣款令牌——这一步没测就不能写「盗刷」。
- 起效前提还包括页面无 `X-Frame-Options` / CSP `frame-ancestors`。

## 反证条件

- 服务端按 `origin` 返回不同 body ⇒ 服务端也参与过滤，需要重新评估；
- 正例与负对照都收到消息 ⇒ 是浏览器扩展或测试环境影响，不是目标缺陷；
- 校验函数实际 `if (!validate(x)) return;` ⇒ 校验生效，不是洞。

## 关联

- `airbnb-payments-iframe-postmessage-origin-trust`（本方法的案例）
- `rule-src-scope-gate`（先确认资产在哪个项目的 scope）
