# M7 骨架报告：Unified UI Workbench（只读工作台，提前开工）

日期：2026-11-06 03:1x
状态：骨架完成，端到端自验通过（38/38 PASS），容器冒烟通过
范围口径：本报告只写实际执行事实。验证基于 `migrations/platform-dryrun.db`（M1 dryrun 快照，只读）。

## 1. 交付物

| 文件 | 说明 |
| --- | --- |
| `apps/server/rp_server.py` | 单文件 http.server，stdlib-only（http.server/sqlite3/json/urllib），4 个只读 JSON API + 静态页服务 |
| `apps/server/static/index.html` | 工作台骨架：Overview / Resources / Reviews / Tasks 四页签 |
| `apps/server/static/style.css` | 深色主题（#15141A 底，#CBA6F7 强调），1000px 断点折叠侧栏 |
| `apps/server/static/app.js` | fetch 四 API 渲染，加载/空/错误三态齐全，hash 路由，textContent 注入防 XSS |
| `apps/server/Containerfile` | python:3.12-slim，构建上下文 = apps/server，DB 以只读卷挂载 |
| `docs/m7-skeleton/*.png` | 四页签 UI 截图（chromium headless 1440x900） |

## 2. API 清单（全部 GET，无写端点）

| 端点 | 参数 | 返回 |
| --- | --- | --- |
| `GET /api/stats` | 无 | 全域计数：资源/知识/审核/任务/证据/别名/工具/Skill，等级、生命周期、验证状态、队列与任务状态分布，schema_version，最近 8 条审核 |
| `GET /api/resources` | `keyword`（LIKE 字面匹配 name/repo/identity/description/URL/owner，通配符已转义）、`grade`（A/B/C/D）、`limit`（1..200，默认 50） | total/returned/filters/items（含质量分、等级、原因、时间） |
| `GET /api/reviews` | `queue`（d5/license/mapping，缺省返回全部分组）、`limit`（1..200，默认 50） | 按 target_type 分组：每组 total、by_decision、items（review_id/target_id/decision/confidence/reason/reviewed_at） |
| `GET /api/tasks` | `limit`（1..200，默认 50） | total、by_state、by_type、items；表空时 `items: []` + message 提示（"暂无任务：research_task 为空，待 M1 全量数据接入后显示"） |

安全设计：

- 数据库连接：`sqlite3.connect("file:...?mode=ro", uri=True)` + `PRAGMA query_only = ON` 双保险，每请求短连接。
- 无写端点：POST/PUT/DELETE/PATCH 一律 405（响应 "只读工作台：本服务不提供写端点"）。
- 绑定 127.0.0.1（`RP_HOST` 默认 127.0.0.1，容器内通过环境变量覆盖为 0.0.0.0 以配合 -p 映射）。
- 路径穿越防护：percent-decode 后 posixpath.normpath，realpath 必须落在 static 根内，越界 404。
- SQL 全参数化，无字符串拼接用户输入；limit/grade/queue 白名单校验。
- 兜底异常不向客户端泄露堆栈；API 响应 `Cache-Control: no-store`，静态文件 `X-Content-Type-Options: nosniff`。

## 3. 端到端自验（实测）

启动：`RP_PORT=7800 python3 apps/server/rp_server.py`（后台，日志 /tmp/rp_server.log）。
验证脚本：curl 四 API + 直接读库对账 + 边界与安全用例，共 38 项，全部 PASS。

### 3.1 数字对账（API vs sqlite 直查）

| 指标 | API 返回 | 库直查 | 结论 |
| --- | --- | --- | --- |
| resources.total | 81 | 81 | 一致 |
| resources.by_grade | A4 / B28 / C44 / D5 | A4 / B28 / C44 / D5 | 一致 |
| knowledge.total | 46 | 46 | 一致 |
| knowledge.by_lifecycle | VERIFIED 37 / CANDIDATE 7 / DISCOVERED 2 | 同 | 一致 |
| knowledge.by_verified | verified 21 / unverified 8 / unknown 17 | 同 | 一致 |
| reviews.total | 165 | 165 | 一致 |
| reviews.by_queue | d5 68 / license 79 / mapping 18 | d5 68 / license 79 / mapping 18 | 一致 |
| tasks.total | 35 | 35 | 一致 |
| tasks.by_state | CREATED 24 / AUTHORIZATION_REVIEW 7 / COMPLETED 2 / AUTHORIZED_EXECUTION 1 / LOCAL_KNOWLEDGE_RETRIEVAL 1 | 同 | 一致 |
| evidence.total | 23（http_pair 21 / screenshot 2） | 同 | 一致 |
| entity_alias.total | 431 | 431 | 一致 |
| tools / skills | 0 / 0 | 0 / 0 | 一致 |

### 3.2 curl 输出摘要

- `GET /api/resources?limit=200`：`total=81 returned=81`。
- `GET /api/resources?keyword=security`：`total=38`（与库 LIKE 对账一致），`grade=A` 组合过滤返回均为 B 级。
- `GET /api/reviews?queue=license&limit=5`：仅 license 组，`total=79 returned=5`。
- `GET /api/tasks`：35 条全返回，by_state 对账一致。
- 错误态：`limit=0/999/abc` 400 bad_limit；`grade=X` 400 bad_grade；`queue=nope` 400 bad_queue；`/api/nothing` 404 no_route。
- 静态与安全：`/` 200 且含 "Beta：只读工作台" 与四页签 DOM；style.css/app.js 200 且 Content-Type 正确；四种穿越变体（`/../migrations/001_init.sql`、`..%2f..%2fmigrations%2f001_init.sql`、`%2e%2e/%2e%2e/etc/passwd`、`/static/../../rp_server.py`）全部 404 且响应体不含目标文件内容；`POST /api/stats` 405。
- LIKE 通配符字面化：`keyword=%25%5F`（即 `%_`）返回 0 命中不报错。

### 3.3 UI 截图（chromium headless，127.0.0.1:7800）

- `docs/m7-skeleton/overview.png`：计数卡（81/46/165/35/23/431/0）+ 等级、知识生命周期、队列、任务状态分布 + 最近审核列表
- `docs/m7-skeleton/resources.png`：搜索框 + 等级过滤 + limit，表格按质量分降序（PayloadsAllTheThings A 93 分居首）
- `docs/m7-skeleton/reviews.png`：队列 chips（全部/D5/License/Mapping）+ D5 组 68 条与 decision 徽章
- `docs/m7-skeleton/tasks.png`：状态 chips + 任务表格（状态徽章、类型、标题、目标、ID、时间）

四页签加载/空/错误态齐备：加载中占位、错误框带重试按钮、空态给提示文案。响应式 1000px 断点：侧栏折叠为顶部导航（CSS 媒体查询，未做移动端真机验证，见限制）。

## 4. 容器验证（podman，可选部分，已执行）

```
podman build -t rp-workbench:m7-skel apps/server          # 成功，localhost/rp-workbench:m7-skel
podman run --rm -d --name rp-workbench-smoke \
  -p 127.0.0.1:7810:7800 \
  -v .../migrations/platform-dryrun.db:/data/platform-dryrun.db:ro \
  rp-workbench:m7-skel
```

冒烟结果：`/` 200；stats 返回 resources 81 / knowledge 46 / reviews 165 / tasks 35；`grade=A` 4；`queue=d5` 68；tasks 35，与宿主机直跑一致（ro 挂载生效）。
清理：`podman rm -f rp-workbench-smoke` + `podman rmi rp-workbench:m7-skel` 完成，镜像及中间层已删除，`podman images -f dangling=true` 为空（未触碰其他工作流的既有镜像）。
注：容器内 `RP_HOST=0.0.0.0` 仅为端口映射所需；宿主机直跑默认仍绑定 127.0.0.1。

## 5. 与计划书 §M7 的关系

- 本骨架对应 M7 信息架构中的 Overview / Resource Center / Review Queue 三块 + Task 泳道的只读视图，用于提前验证信息架构、深色设计语言与 API 消费方式。
- 计划书 UI 原则第 1 条 "UI 只消费稳定 API" 已按骨架口径执行；GPUI 与 Tauri+React 的 PoC 决策仍按计划留待 M7 正式阶段，本骨架的 HTML/JS 版本不做技术路线预设。

## 6. 已知限制

1. 只读：无任何写端点，审核通过/拒绝、触发 sync/enrich 等操作不在本骨架范围（§6.6 列出的操作类需求待正式 M7）。
2. 无鉴权：以绑定 127.0.0.1 缓解（本机使用）；容器模式暴露面取决于 `-p` 参数与宿主防火墙，接入网络前必须补鉴权。
3. 数据为 M1 dryrun 快照：81 资源 / 46 知识 / 165 审核 / 35 任务是 dryrun 口径；真实数据接入待 M1 全量跑批后由 API 自然反映（API 无需改动）。
4. 工具/Skill/证据中心页签未做：tool、skill 表当前 0 行，API 仅在 stats 中暴露计数；实体视图与关系图（任务↔证据↔知识↔资源↔工具链路）待正式 M7。
5. 分页只有 limit 上限 200，无 offset/cursor；搜索为 LIKE 全表扫，数据量大后需 FTS。
6. 响应式经 1000px 断点的 headless 截图验证（1440px 桌面），未做窄屏真机与无障碍审查。
7. 服务日志为进程 stdout，未落盘、无访问控制审计（audit_event 表已存在但骨架未消费写审计，只读服务不产生审计事件）。

## 7. 复现步骤

```
cd /home/emo/code/pensting/research-platform
RP_PORT=7800 python3 apps/server/rp_server.py &
curl -s http://127.0.0.1:7800/api/stats | python3 -m json.tool
# 浏览器打开 http://127.0.0.1:7800/
```
