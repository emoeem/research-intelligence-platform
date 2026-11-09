---
name: zoomeye-recon
description: ZoomEye API v2 测绘：APIKEY 认证、host/search 语法（v2 前缀语义）、与 FOFA 语法对照、配额纪律与三家测绘源切换策略
whenToUse: 第四测绘源；历史数据/主机聚合视角与前三家互补
role: recon
enabled: true
---

# ZoomEye 资产测绘

## 认证与接口（API v2）

```
GET https://api.zoomeye.org/host/search?query=<q>&page=1
Header: API-KEY: <KEY>   （环境变量 $ZOOMEYE_KEY）
```

- `host/search`：主机维度（聚合 ip 的多端口结果）；`web/search`：web 页面维度
- 响应 `status:200`；`matches[].{ip,portinfo.port,portinfo.service,site,hostname,ipinfo.location}`；`available` 总量
- v2 语法在字段后直接写值，无需等号引号均可（`domain:x.com` 与 `domain:"x.com"` 等价）

## 常用语法

```
hostname:"oa.target.com"          site:"target.com"
ip:"1.2.3.0/24"                   port:6379
service:redis                     app:"Nacos"        (app 前缀在 v2 为 app:"name")
title:"后台管理"                   header:"thinkphp"
cert.subject:"target"             favicon:"hash"
after:"2026-01-01"                （时间过滤，ZoomEye 特色：查新数据）
before:"2025-01-01"               （时间过滤：查历史遗留资产——打"下线未关"资产利器）
```

## 与 FOFA 对照表

| 意图 | FOFA | ZoomEye v2 |
|---|---|---|
| 域名 | `domain="x.com"` | `domain:x.com` 或 `site:x.com` |
| 主机名 | `host="oa.x.com"` | `hostname:"oa.x.com"` |
| 标题 | `title="x"` | `title:"x"` |
| 应用 | `app="Nacos"` | `app:"Nacos"` |
| favicon | `icon_hash="x"` | `favicon:"x"` |
| 端口 | `port="6379"` | `port:6379` |

## 配额与纪律

- 积分制：查询与翻页都消耗；免费档总量有限——精确查询、只翻 1-2 页
- 独有价值：`after/before` 时间过滤查**历史暴露资产**（已下线但未关的服务、旧版本组件），与其他三家互补
- QPS 低（免费档更低）：串行执行，`429` 退避；三家都紧时按 hunter→quake→zoomeye 顺序轮换
- 落盘缓存 `runs/zoomeye-<slug>.jsonl`

## 产出约定

主机聚合视角结果入 assets.db 时按 ip 聚合（一个 ip 多 port/service 摊开成 service 行）；`after/before` 挖到的"遗产资产"单独标记（往往是高价值弱口令面）。

<!-- 改编自: ZoomEye 官方 API v2 文档要点（以官方文档为准）, 2026-10-31 -->
