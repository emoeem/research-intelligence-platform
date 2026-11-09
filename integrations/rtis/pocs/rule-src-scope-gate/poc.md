---
code: rule-src-scope-gate
title: SRC 范围闸门：政策不可读、范围为空、需实名账号一律 blocked
kind: rule
category: other
evidence_level: L4
verified_on: 2026-11-03
source: 实战（9 家 SRC 政策采集，5 家被挡住）
tags: [SRC, scope, 授权, 政策页, blocked, 范围闸门, 合规边界]
---

# SRC 范围闸门

## 为什么需要它

2026-11-03 对 9 家 SRC 做政策采集，**5 家（嘀嗒、乐信、齐治、原力金智、PayPal 中国）** 的范围与规则必须登录漏洞盒子并完成实名/手机/微信验证才能读。公开接口 `project/profile` 返回 `assets.content=""`、`rules=""`，公告详情返回 `{"code":301001,"msg":"暂无权限"}`。

如果在这 5 家上「凭线索猜 scope」开始测试，产出的是无授权测试，报告也不可能被接收。**范围闸门存在的意义：把「读不到范围」和「范围允许测」分开。**

## 判定顺序（4 步，任何一步失败即 blocked）

| # | 检查 | 方法 | 失败判定 |
|---|---|---|---|
| 1 | 政策页是否可匿名读 | `curl -sS -o /dev/null -w '%{http_code}' <规则页>`；看正文是否含资产表 | 登录墙 / 302 到登录 / 正文为空 ⇒ `rules_access=login_required` |
| 2 | in-scope 清单是否非空 | 政策页或官方资产表里是否有域名/通配/App/API 条目 | 只有「包括但不限于」而无具体资产 ⇒ 范围未公开 |
| 3 | 平台是否允许测试 | 政策是否明确禁止自动化扫描 / 是否要求实名 | `automated_scanning=prohibited` 时只能手工单请求 |
| 4 | 提交是否需要账号 | 提交入口是否要求登录/实名 | 需要账号 ≠ 不能测，但需要委托人提供账号 |

## 三种「看起来像可测」的假信号

| 信号 | 为什么不是授权 |
|---|---|
| 厂商官网有「安全响应中心」链接 | 只是入口，不是范围 |
| `security.txt` 有 `Contact:` | 只是联系方式，不等于允许测试 |
| 第三方转载的「某 SRC 范围」 | 可能是旧版或被修改，必须回到官方页核对 |

## 本次实测结论（2026-11-03）

| 厂商 | rules_access | scope 条目 | 判定 |
|---|---|---|---:|
| 度小满 DXMSRC | public | 26 | 可测 |
| OPPO OSRC | public | 122 | 可测（restricted） |
| 萤石 YSRC | public | 9 | 可测 |
| Airbnb（HackerOne 全球） | public | 33 | 可测 |
| 嘀嗒 DIDASRC | login_required | 0 | **blocked** |
| 乐信 LXSRC | login_required | 0 | **blocked** |
| 齐治 QZSRC | login_required | 0 | **blocked** |
| 原力金智 | login_required | 0 | **blocked** |
| PayPal 中国 PPSRC | login_required | 0 | **blocked** |

## 反证条件

- 政策页 302 到登录，但**公开 API 返回非空 scope** ⇒ 范围实际可读，可测；
- 范围为空，但厂商另有公开的资产表（如 OPPO 指向腾讯文档） ⇒ 以资产表为准，可测；
- 规则写着「禁止自动化」，但**手工单请求**仍被允许 ⇒ 降级为手工测试，不是 blocked。

## 不能得出的结论

- ❌ 读不到范围 ⇒ 这个厂商没有 SRC（可能只是要登录）
- ❌ 范围里有「包括但不限于」⇒ 可以外推任意子域（官方口径通常仍需审核）
- ❌ 在 A 项目可测的资产，在 B 项目也可测（**AirSRC 中国 scope 只有 `*.airbnb.cn`、`zh.airbnb.com`、App、小程序；`airbnbpayments.com` 属于 Airbnb 全球 HackerOne 项目，不属于 AirSRC**）

## 关联

- `rule-401-location-injection-limits`（定级前先确认它在不在 scope）
- `case-src-programs-2026-11`（9 家状态登记）
