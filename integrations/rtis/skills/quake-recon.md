---
name: quake-recon
description: 360 Quake API 测绘：X-QuakeToken 认证、service/web 搜索语法、与 FOFA 语法对照、积分体系与限速纪律
whenToUse: 多测绘源交叉验证；FOFA/Hunter 配额紧张时的第三源
role: recon
enabled: true
---

# 360 Quake 资产测绘

## 认证与接口

```
POST https://quake.360.net/api/v3/search/quake_service
Header: X-QuakeToken: <TOKEN>   Content-Type: application/json
Body: {"query":"domain:\"target.com\"","start":0,"limit":100,"ignore_cache":false}
```

- token 从环境变量 `$QUAKE_TOKEN` 读取
- 两个搜索端点：`quake_service`（端口/服务面）与 `quake_host`（主机聚合）；web 页面搜索用 `query:"domain:..."` 加 `include:"web.tag"` 过滤
- 响应：`code:0` 正常；`data[].{ip,port,hostname,service.name,service.http.title,service.http.server,location}`；`meta.pagination.total` 总量

## 常用语法

```
domain:"target.com"               hostname:"oa.target.com"
ip:"1.2.3.0/24"                   port:"6379"
response:"basic realm"            service:"redis"
app:"Nacos"                       title:"后台管理"
cert:"target"                     favicon:"hash"
country:"CN" AND province:"Shanghai"
```

## 与 FOFA 对照表

| 意图 | FOFA | Quake |
|---|---|---|
| 域名 | `domain="x.com"` | `domain:"x.com"` |
| 标题 | `title="后台"` | `title:"后台"` |
| 应用指纹 | `app="Nacos"` | `app:"Nacos"` |
| 响应体 | `body="keyword"` | `response:"keyword"` |
| 证书 | `cert="x"` | `cert:"x"` |
| favicon | `icon_hash="x"` | `favicon:"x"` |

## 配额与纪律

- 积分制：每月赠分 + 查询按条数扣分；`ignore_cache:true` 额外扣分——**非必要不刷缓存**
- `limit` 单页上限 100（部分档位 5000，按账户）；翻页用 `start`，只取前 1-2 页
- QPS 限制明显（约 1 req/s）：串行 + 遇积分不足报错（`code` 非 0）就切换 hunter-recon / zoomeye-recon
- 落盘缓存 `runs/quake-<slug>.jsonl`，同查询不重拉

## 产出约定

与 fofa-recon/hunter-recon 结果合并去重（url+ip+port 键）后入 assets.db；service.name 字段用于端口服务面（非 web）补充，是 FOFA 弱项。

<!-- 改编自: Quake 360 官方 API 文档要点（以官方文档为准）, 2026-10-31 -->
