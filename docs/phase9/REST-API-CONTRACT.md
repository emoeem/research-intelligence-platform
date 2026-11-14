# REST API 对外契约（rp_server · Phase 9 P9-3 收口）

| 项       | 值 |
| ---      | --- |
| 状态     | 生效中（P9-3 批次交付，P9-4 增 `/api/mcp-catalog` 一条只读路由；P9-4 双通道一致性载具与 Resources/Prompts 见 PHASE9-DESIGN §十二） |
| 日期     | 2026-11-14 |
| 适用版本 | Phase 9 P9-2 工作树及之后（本文件随 P9-3 收口） |
| 相关     | apps/server/rp_server.py（唯一实现源）、apps/server/selftest_auth.sh（鉴权 11 项）、apps/server/selftest_phase9_rest.sh（本契约机器对账）、migrations/019_external_api_audit.sql（审计 SQL 形状）、docs/guides/CLI-REFERENCE.md（CLI 面）、docs/phase9/PHASE9-DESIGN.md §十一 |

> 本文件是 rp_server.py 既有 API 面的对外契约化描述，不是新接口设计：不另起第二套风格，全部形状以服务实现为准。文档第 九 章端点清单与 rp_server.py `API_ROUTES`（rp_server.py:2643）及写面白名单逐条机器对账（selftest_phase9_rest.sh 第 10 段），文档发明端点或漂移即 selftest 红灯。

## 一、范围与定位

覆盖面：17 条只读路由（`API_ROUTES`；P9-4 起 16 条加 `/api/mcp-catalog`）+ 插件详情动态路径 + 白名单写面（plugins/workflows 两族，经子进程 CLI 落库，本文件零 SQL 写）。服务形态：单文件 stdlib http.server（ThreadingHTTPServer），默认绑定 `127.0.0.1:7800`（`RP_HOST`/`RP_PORT` 可覆盖），静态工作台页与 `/api/*` JSON 同进程承载。

三面同源关系（复用优先，不重复实现）：Web（本 REST 面）与 MCP（crates/mcp-server，rp-mcp）与 CLI（11 个 rp-* bin，见 docs/guides/CLI-REFERENCE.md）共用同一数据面与同一业务实现；MCP 目录的注册面与 `/api/tools` 同源（PHASE9-DESIGN §四），执行通道与 `/api/workflow-runs` 的 run 一样落 Tool Engine（P9-2 dispatch.rs 唯一实现）。

## 二、通用约定

- 编码：请求与响应体 UTF-8 JSON；响应头 `Content-Type: application/json; charset=utf-8`，`Cache-Control: no-store`。
- 分页：`limit` 参数 1..200（`parse_limit`，默认 50；越界 400 `bad_limit`）；部分端点另有 `?level=1|2|3`（插件三级加载）、`?offset=` 等，词表见各端点实现。
- 请求体：POST JSON 上限 1 MiB（超限 413 `payload_too_large`，rp_server.py:2901）；非法 JSON 400 `bad_json`。
- 成功信封（读面，两类）：
  - 列表信封：`available`、`source`（数据来源 schema 名）、`total`、`returned`、`limit`、`filters`、`items`（部分端点带 `nextCursor` 形状字段名不同，以实现为准）；
  - 详情信封：`available`、`source`、`query_id` 与详情体（run 详情为 `run`/`nodes`/`control`/`correlation` 等）。
- 真实空态说明：数据面不可读（缺库/缺表/迁移水位不足）时返回 `available: false` + `reason` 词表（aux_db_not_configured / aux_db_missing / table_missing / trace_tables_missing）+ 中文 `message`，并如实说明缺什么；不造假数据、不伪造空集（PHASE9-DESIGN §四同源口径）。
- 静态资源（工作台页）无需鉴权、不计限频、不落审计；`/api/*` 全部计。

## 三、鉴权口径（selftest_auth 11 项同源）

1. 令牌来源优先级：`RP_AUTH_TOKEN` 环境变量 > `RP_AUTH_TOKEN_FILE`（默认仓库根 `.rp-workbench-token`，权限非 600 启动告警）。
2. 校验：请求头 `Authorization: Bearer <token>`，`hmac.compare_digest` 常时比较（rp_server.py:2713-2721）。
3. 失败节流：比较失败统一 sleep 0.3s 再回（`AUTH_FAIL_DELAY`，rp_server.py:113），不区分「缺头」「错值」两种失败（不可枚举探测）。
4. 失败响应：401 + `WWW-Authenticate: Bearer`（rp_server.py:2706）+ `{"error":{"code":"unauthorized",...}}`。
5. 写端点强制鉴权：`AUTH_ENABLED` 为假（未配置任何令牌）时，全部 POST 一律 403 `auth_required_for_write`（不降级放行）；GET 在该模式下为本地开发模式可用（启动 stderr 显著告警）。
6. 静态页与登录流程无令牌可访问（selftest_auth 第 4 段口径）。

## 四、错误形状（ApiError）

统一形状（错误一律 JSON，无 HTML 错误页）：

```json
{"error": {"code": "<机器码>", "message": "<中文说明>"}}
```

状态码与机器码词表（实现出处 `raise ApiError`，rp_server.py 63 处；下表为对外高频面）：

| 状态码 | 机器码（词表） | 场景 |
| --- | --- | --- |
| 400 | bad_json / bad_limit / bad_level / bad_verified / bad_grade / missing_id / bad_input / bad_run_id / definition_required / bad_definition_schema / bad_workflow_id / bad_graph / definition_unparsable / no_route 之外的形状类拒绝 | 请求形状违规（完整词表以实现为准） |
| 401 | unauthorized | 缺少或无效令牌（携带 WWW-Authenticate: Bearer） |
| 403 | auth_required_for_write / production_db_refused | 写端点鉴权关闭；写面目标库指向 platform*.db（红线拒绝） |
| 404 | no_route / not_found / no_knowledge / no_tool / no_run / no_agent / no_agent_run / plugin_not_found | 路由或实体不存在 |
| 405 | readonly | POST/PUT/DELETE/PATCH 打白名单外路径 |
| 413 | payload_too_large | 请求体超 1 MiB |
| 422 | definition_rejected | workflow 定义静态校验拒绝（REJECTED 汇总透传） |
| 429 | rate_limited | 全局限频超限（带 Retry-After） |
| 500 | internal | 兜底（不含堆栈） |
| 502 | plugin_cmd_spawn_failed / workflow_cmd_spawn_failed / workflow_bin_missing / plugin_cmd_failed | 子进程启动失败或 CLI 拒绝 |
| 503 | db_missing / db_unavailable / plugin_db_missing / plugin_db_unavailable / workflow_db_not_configured | 数据面不可用（真实空态说明的强失败面） |
| 504 | plugin_cmd_timeout / workflow_cmd_timeout | 子进程超时（见 七） |

## 五、trace 关联透出（SESSION-HANDOFF §十 Phase 9 追加项）

响应体与响应头两处透出，锚语义与 015/018 trace 关联体系同 ID 空间：

1. 响应体：run 详情（`/api/runs?run_id=`、`/api/workflow-runs?id=`、`/api/agent-runs?id=`）顶层带 `correlation_id`（= 该 run 的审计关联锚 run_id，与 `audit_event.correlation_id` 同语义）；六 ID 关联字段保持既有 `correlation` 字段形状不移动（workflow run：`trace_id/task_id/agent_run_id/tool_run_id/artifact_id/evidence_id` 单数六键；tool run 与 agent run 的字段形状见各端点，`artifact_ids/evidence_ids` 为集合形态）。`/api/traces?id=` 详情顶层带 `correlation_id`（= 请求锚 query_id）与 `trace_id`（单锚命中时 = 唯一链锚，多锚命中不猜，仍走 `trace_ids` 列表）。
2. 响应头（HTTP 层 `trace_response_headers` 统一提取，rp_server.py:216）：`X-Trace-Id` 与 `X-Correlation-Id`，锚在场即发：

| 端点 | X-Trace-Id | X-Correlation-Id |
| --- | --- | --- |
| /api/runs?run_id=（详情） | run.trace_id（018 链锚，工作流驱动 = workflow run_id） | tool run_id |
| /api/workflow-runs?id=（详情） | run.trace_id（= run_id） | run_id |
| /api/agent-runs?id=（详情） | run.trace_id | agent run_id |
| /api/traces?id=（单锚命中） | 唯一链锚 | query_id |
| 列表页 / 空态说明 / 错误响应 | 不发 | 不发 |

3. 关联头同时落对外访问审计（`trace_id`/`correlation_id` 两列，见 八）。
4. 全链回查入口：`/api/traces?id=<任一关联 ID>` 六方向解析（015 词表：trace_id/task_id/agent_run_id/tool_run_id/artifact_id/evidence_id）。

## 六、全局限频（超限 429）

- 形态：服务进程级滑动窗口（`check_rate_limit`，rp_server.py:186），全部客户端共享一个预算（全局口径），只计 `/api/*`（GET 与 POST 都计，静态资源不计），401 与 4xx 同样计数（先限频后鉴权，保护鉴权面）。
- 阈值：`RP_RATE_LIMIT_RPM`，默认 600 req/60s；`0` 显式关闭；窗口 60s（`RATE_LIMIT_WINDOW_S`）。
- 超限响应：429 `{"error":{"code":"rate_limited","message":"..."}}` + `Retry-After: 60` 头（rp_server.py:2707-2708）。
- 多线程安全：ThreadingHTTPServer 并发记账用锁串行（`threading.Lock`）。

## 七、子进程超时口径（显式常量）

| 写面 | 承载 | 超时 | 超限响应 |
| --- | --- | --- | --- |
| 插件写面（install/enable/disable/uninstall/upgrade/ingest/...） | 同步 `subprocess.run` rp-plugin CLI | 600s（`PLUGIN_SUBPROCESS_TIMEOUT_S`，rp_server.py:153） | 504 plugin_cmd_timeout |
| workflow pause / put（画布保存） | 同步 `subprocess.run` rp-workflow CLI | 60s（`WORKFLOW_SYNC_TIMEOUT_S`，rp_server.py:154） | 504 workflow_cmd_timeout |
| workflow run / resume / retry | 异步 `Popen`（不等待，`start_new_session`，stdout/stderr 落 `RP_WORKFLOW_LOG_DIR`） | 不适用同步超时；终态经 `GET /api/workflow-runs?id=<run_id>` 轮询 | run 头行六态（queued/running/paused/succeeded/failed/cancelled） |

写面纪律：plugins/workflows 写一律经子进程 CLI（plugin-runtime 状态机 / workflow-engine 装载门），本文件零直连 SQL 写（Phase 6 selftest 静态门禁：全文件不得含 `INSERT INTO` 等写语句）；写面目标库守卫镜像 CLI（指向 platform*.db 一律 403 `production_db_refused`）。

## 八、对外访问审计

- 写面：`RP_API_AUDIT_LOG` 指向 JSONL 文件时，每个 `/api/*` 请求在响应收尾追加一行 `api_access` 事件（`audit_api_access`，rp_server.py:202）；缺省关闭（零行为变化）。审计写入失败只告警不阻塞响应。
- 行形状（与 migrations/019_external_api_audit.sql 的 `external_api_audit` 表列一一对应）：`ts`（ISO8601 UTC）、`event`（= api_access）、`method`、`path`、`remote`、`status`、`duration_ms`、`trace_id`、`correlation_id`、`rate_limited`（0/1）、`auth_enabled`（0/1）。
- 表是审计流的 SQL 存储形状：rp_server 本身零 SQL 写（静态门禁），消费侧（ETL/装载或后续只读投影端点）按 019 列形状装载；019 只在开发库副本与测试库执行，platform*.db 不应用（零写红线）。

## 九、端点总览（与 ROUTES 逐条对账）

下表由 selftest_phase9_rest.sh 第 10 段机器对账：17 条 `API_ROUTES` 只读路由逐条在场且标 GET；动态路径 7 形状；写动作词表（`PLUGIN_WRITE_ACTIONS`/`WORKFLOW_WRITE_ACTIONS`，含 upgrade）与 rp_server.py 词表逐 token 一致；标记段内不得出现表外路径。

<!-- ENDPOINTS:BEGIN -->
| 方法 | 路径 | 说明 | 鉴权 |
| --- | --- | --- | --- |
| GET | /api/plugins | 插件目录（L1 元数据卡片，?level=1|2|3） | Bearer |
| GET | /api/plugins/<plugin_id> | 插件详情（动态路径，?level=1|2|3 三级按需加载） | Bearer |
| GET | /api/knowledge_sources | 知识来源面 | Bearer |
| GET | /api/stats | M 面计数（evidence/knowledge 等） | Bearer |
| GET | /api/resources | 资源库读面 | Bearer |
| GET | /api/reviews | 评审队列读面 | Bearer |
| GET | /api/tasks | 任务列表 | Bearer |
| GET | /api/overview | Overview 聚合 | Bearer |
| GET | /api/knowledge | 知识库列表（keyword 过滤） | Bearer |
| GET | /api/tools | 工具目录（Tool Center 同源投影） | Bearer |
| GET | /api/mcp-catalog | MCP 目录同步状态（P9-4：目录条数 + 读时对账结果，Tool Center MCP 状态卡数据面） | Bearer |
| GET | /api/runs | 工具运行列表；?run_id= 详情（correlation_id + 关联头） | Bearer |
| GET | /api/runtime | 运行节点读面 | Bearer |
| GET | /api/traces | 统一关联查询 ?id=<六 ID 之一>（单锚 trace_id/correlation_id + 关联头） | Bearer |
| GET | /api/agents | Agent 定义读面 | Bearer |
| GET | /api/agent-runs | AgentRun 列表；?id= 详情（correlation_id + 关联头） | Bearer |
| GET | /api/workflows | Workflow 定义读面；?id= 全版本 + graph | Bearer |
| GET | /api/workflow-runs | WorkflowRun 读面；?id= 详情（六 ID correlation + correlation_id + 关联头） | Bearer |
| POST | /api/plugins/install | 插件安装（经 rp-plugin 子进程） | Bearer（写面强制） |
| POST | /api/plugins/<plugin_id>/<action> | 插件写面：action ∈ enable, disable, uninstall, ingest, upgrade | Bearer（写面强制） |
| POST | /api/workflows | 画布保存（definition-v1，版本存储侧递增；put 同步 60s 超时） | Bearer（写面强制） |
| POST | /api/workflow-runs | 受理新 run（run_id 预生成，异步 Popen；action=run） | Bearer（写面强制） |
| POST | /api/workflow-runs/<action> | 控制面形状一：action ∈ pause, resume, retry（run_id 在 body） | Bearer（写面强制） |
| POST | /api/workflow-runs/<run_id>/<action> | 控制面形状二（REST 动态路径）：action ∈ pause, resume, retry | Bearer（写面强制） |
<!-- ENDPOINTS:END -->

动态词表来源：插件写动作 `enable/disable/uninstall/ingest` 加 `upgrade`（rp_server.py `PLUGIN_WRITE_ACTIONS` + upgrade 特例）；workflow 控制动作 `pause/resume/retry`（`WORKFLOW_WRITE_ACTIONS`）。词表外动作 404 `no_route`；白名单外 POST 一律 405 `readonly`。

## 十、写面响应语义

- run 受理：`{"result":"accepted","action":"run","run_id":"wfr-...","correlation_id":"<run_id>","pid":...,"log":...,"status_query":"/api/workflow-runs?id=..."}`（受理即返回，run 异步执行）。
- retry：`new_run_id` 承载新 run，`correlation_id` = 新 run_id。
- put：`{"result":"put","workflow_id":...,"version":<存储侧递增>,"nodes":...,"edges":...,"status_query":...}`。
- pause：同步返回 `result=pause-requested`（请求位 consume-on-observe，018 控制行）。
- 关联头：写面 run/resume/retry 响应带 `correlation_id` 顶层键，HTTP 层按同一规则透出 `X-Correlation-Id`（新受理 run 的 run_id）。

## 十一、对账口径与已知边界

- 端点机器对账：selftest_phase9_rest.sh 第 10 段（API_ROUTES 16 键、GET 17 行、POST 6 形状、动作词表逐 token、零发明端点五重断言）。
- 鉴权 11 项回归：apps/server/selftest_auth.sh（Bearer + 常时比较 + 失败节流 + WWW-Authenticate + 只读语义 + 路径穿越 + 开发模式告警）。
- 已知边界（如实登记，不是缺口伪装成完整）：CLI 面（11 bin 帮助面统一口径与逐 bin 参考）见 docs/guides/CLI-REFERENCE.md，本文件不重复；MCP 面契约（握手鉴权、目录分页、isError 语义）见 docs/phase9/PHASE9-DESIGN.md；REST 面当前不提供 run_tool 直执端点（执行通道 = POST /api/workflow-runs 受理 workflow 或外部 AI 经 MCP rip.run_tool），P9-4 一致性载具将双通道对账。
- 本文件与实现漂移即债：改 rp_server.py 路由/词表/信封时必须同步更新本文件与 selftest 对账段（DOC-STANDARD 三道门禁不扫 markdown，对账由 selftest 承担）。
