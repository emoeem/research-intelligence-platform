---
name: hunter-recon
description: 奇安信鹰图 Hunter API 测绘：api-key header 认证、web 前缀语法、与 FOFA 语法对照表、积分配额纪律、限速客户端骨架
whenToUse: FOFA 配额耗尽或需要第二测绘源交叉验证时；SRC 铺资产面
role: recon
enabled: true
---

# 鹰图（Hunter）资产测绘

## 认证与接口

```
GET https://hunter.qianxin.com/openApi/search?api-key=<KEY>&search=<base64(查询)>&page=1&page_size=100&is_web=3
Header/参数：api-key（环境变量 $HUNTER_KEY，不落盘）
```

- `is_web`：1=web 资产 2=非 web 3=全部；响应 `code:200` 正常，`code:xxx + message` 报配额/语法
- 返回字段：`url/ip/port/domain/web_title/status_code(number)/company_name(关联单位)/number(总分页量)`

## 常用语法（web. 前缀为主）

```
web.title="后台管理"               web.header="X-Powered-By: ThinkPHP"
web.body="api-docs"               web.icon="hash值"
domain.suffix="target.com"        domain="target.com"
ip="1.2.3.0/24"                   ip.port="6379"
cert.subject="target"             web.tag="路由器"（产品标签）
```

## 与 FOFA 语法对照（互相切换用）

| 意图 | FOFA | Hunter |
|---|---|---|
| 域名后缀 | `domain="target.com"` | `domain.suffix="target.com"` |
| 网站标题 | `title="后台"` | `web.title="后台"` |
| HTTP 头 | `header="thinkphp"` | `web.header="..."` |
| body 关键字 | `body="keyword"` | `web.body="keyword"` |
| favicon | `icon_hash="xxx"` | `web.icon="xxx"` |
| 端口 | `port="6379"` | `ip.port="6379"` |

## 配额与纪律（对齐 fofa-recon 风格）

- 免费账户按**积分消耗**：查询消耗积分 = 返回条数相关，配额比 FOFA 紧张——**精确查询优先**（domain/cert 先收口），禁止裸 `web.body=` 宽语句
- 翻页 `page` 有积分加倍效应：默认只取前 1-2 页
- API 有 QPS 限制（约 1 次/秒量级）：串行执行，遇 `429`/配额报错退避，换 quake-recon/zoomeye-recon
- 结果落盘 `runs/hunter-<slug>.jsonl` 缓存，同查询不重拉

## 客户端骨架（复用 fofa-recon 的限速模式）

```python
import base64, os, urllib.request, time, json
KEY = os.environ["HUNTER_KEY"]
q = base64.b64encode('domain.suffix="target.com"'.encode()).decode()
url = f"https://hunter.qianxin.com/openApi/search?api-key={KEY}&search={q}&page=1&page_size=100&is_web=3"
data = json.load(urllib.request.urlopen(url))
# data['data']['arr'] 逐条落盘；data['code']!=200 时按 message 判断配额/限速并退避
```

## 产出约定

测绘结果并入 engagement 的 assets.db segment/asset 表；关联单位（company_name）字段用于站群归属判定（衔接包网站群经验）。

<!-- 改编自: 鹰图官方 API 文档要点 + 本地 fofa-recon.md 客户端风格, 2026-10-31 -->
