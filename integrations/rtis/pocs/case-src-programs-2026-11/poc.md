---
code: case-src-programs-2026-11
title: Case：9 家 SRC 范围登记表（2026-11-03）
kind: case
category: other
evidence_level: L4
verified_on: 2026-11-03
source: 实战（政策页逐家采集）
tags: [SRC, scope, 项目登记, 度小满, OPPO, 萤石, 嘀嗒, 乐信, 齐治, 原力金智, PayPal, Airbnb]
---

# 9 家 SRC 范围登记表（2026-11-03）

## 用途

下次开工先查这张表，不要重新读 9 家政策页。表里只记**可核验事实**（政策页 URL、抓取时间、scope 条目数、判定），不记推测。

## 登记表

| slug | 厂商 | 规则可读性 | scope 条目 | 判定 | 备注 |
|---|---|---|---:|---|---|
| duxiaoman | 度小满 DXMSRC | public | 26 | 可测（restricted） | 规则 PDF 需带 Referer 才返回 `status:0` |
| oppo | OPPO OSRC | public | 122 | 可测（restricted） | 无通配范围，禁止外推子域；拒收扫描器原始报告 |
| ezviz | 萤石 YSRC | public | 9 | 可测 | `ezvizlife.com` 主域名含子域名 |
| airbnb | Airbnb（HackerOne 全球） | public | 33 | 可测 | `*.airbnbpayments.com` 属 Higher Impact |
| dida | 嘀嗒 DIDASRC | login_required | 0 | **blocked** | 规则需实名+签协议 |
| lexin | 乐信 LXSRC | login_required | 0 | **blocked** | 同上 |
| qizhi | 齐治 QZSRC | login_required | 0 | **blocked** | 同上 |
| yuanlijinzhi | 原力金智 | login_required | 0 | **blocked** | 同上 |
| paypal-cn | PayPal 中国 PPSRC | login_required | 0 | **blocked** | 同上 |

## 关键区分

**Airbnb 全球 HackerOne ≠ AirSRC 中国。** AirSRC 中国（漏洞盒子）的资产范围只有 `*.airbnb.cn`、`zh.airbnb.com`、App、小程序；`airbnbpayments.com` 只在全球 HackerOne 项目里。把后者的报告投到前者，会因资产不在范围被拒。

## 2026-11-03 战果

- 可测 4 家、目标 8 个、confirmed 5 条（Airbnb 1、萤石 3、OPPO 1）、报告 12 份、提交 0 份。
- 5 家 blocked 的共同原因：范围/规则需 Vulbox 登录 + 实名验证。

## 关联

- `rule-src-scope-gate`
- `case-ezviz-oauth-cluster`
- `airbnb-payments-iframe-postmessage-origin-trust`
- `sso-gateway-401-host-injection-redirect-uri`
