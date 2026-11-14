# Phase 9 设计 · MCP 对外能力面（P9-1 落地形态，P9-2 追加章节见 §十，P9-3 追加章节见 §十一，P9-4 追加章节见 §十二）

| 项       | 值 |
| ---      | --- |
| 状态     | 生效中（P9-1 批次设计；P9-2 追加章节 §十；P9-3 追加章节 §十一；P9-4 追加章节 §十二，Resources/Prompts 并入该批交付，Phase 9 至此全段交付） |
| 日期     | 2026-11-14 |
| 适用版本 | Phase 8 收口（`8f0446b`）及之后 |
| 相关     | docs/phase0/PHASE-ROADMAP.md §10（Phase 9 批次表）、docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md §11:640-679（17 tool 与目录纪律）、docs/phase6/DUAL-MCP-BOUNDARY.md（双 MCP 边界）、apps/server/rp_server.py（handle_tools 同源口径基准）、crates/rp-mcp-parity（P9-4 载具）、[releases/RELEASE_2026-11-14_PHASE9.md](../releases/RELEASE_2026-11-14_PHASE9.md)（阶段总账） |

## 一、批次定位

Phase 9 五批（ROADMAP §10.3）：P9-1（本批，串行起点）落地 crates/mcp-server 的目录发现面；P9-2 接 rip.run_tool 端到端执行通道；P9-3 REST/CLI；P9-4 双通道一致性载具；P9-5 Resources 与 Prompts。P9-1 的完成判据（ROADMAP §10.3 原文）：stdio 握手 + tools/list 分页 + get_tool_schema 三交互跑通；初始化响应不含工具全文说明。

## 二、协议形态

- 传输：stdio NDJSON JSON-RPC 2.0，一行一请求，响应行写 stdout，日志一律 stderr（integrations/rtis/rtis_mcp.mjs:16-17 同款纪律）。
- 协议版本：MCP 2024-11-05 子集，方法面：initialize / notifications/initialized / ping / tools/list / tools/call / resources/list / prompts/list；子集外方法回 -32601。resources/prompts 为 P9-5 前置占位（如实空列表，不返回假目录）。
- 坏 JSON 行记 stderr 后跳过（不回 -32700，rtis_mcp.mjs:505 口径）；通知（无 id）一律静默。
- 实现形态参考 rtis_mcp.mjs 但代码独立：协议层薄（lib.rs 派发 + serde_json 信封），零新增外部依赖（vendor 实测有 serde_json 无 rmcp/tokio，引入即破坏 --offline 构建红线）。
- 双 MCP 边界（phase6/DUAL-MCP-BOUNDARY.md 已裁定）：本 crate 是平台 MCP 面（rip.* 前缀），kit 的 rtis_mcp.mjs（redteam_* 前缀）是 project_endpoint 接入面；命名空间隔离互不引用。

## 三、目录设计：能力面 + 注册面双段

tools/list 返回的组合目录分两段，固定次序：

1. 能力面前段（5 条，计划 §11:640-656 的 P9-1 子集，固定次序）：rip.list_tools（分页目录）、rip.get_tool_schema（按需 Schema）、rip.run_tool（执行入口，P9-2 接线）、rip.search_knowledge（知识检索面）、rip.get_run_status（运行查询面）。后两条与 run_tool 一样在 P9-1 为「目录条目 + 调用返回结构化 isError（wired_in=P9-2）」，不伪造执行结果；rip.list_tools 与 rip.get_tool_schema 为本批真正接通的两个交互。
2. 注册面段（tool 表逐行投影，MCP 名 = `rip.<tool_id>`）：与 Tool Center 同源的唯一 tool 表读面（catalog.rs），排序 `updated_at DESC, name COLLATE NOCASE ASC`，aux 增量合并。aux 行自带 rip.* 前缀（如 rip.kb.rtis）时映射后为 `rip.rip.kb.rtis`，保留全名不做剥名（机械可逆：去 `rip.` 前缀即 tool_id，对账无歧义）。

「初始化只暴露目录与分页发现」的落法（计划 §11:658 原文：不要在 MCP 初始化时把所有工具的完整说明暴露给模型）：

- initialize 响应 = protocolVersion 回显 + capabilities.tools.listChanged=false + serverInfo + 一行导航 instructions（95 字符实测），不含任何工具条目；
- tools/list 条目为摘要投影：注册面条目带 Tool Center 列表列（tool_id/kind/lifecycle/governance/permission_level/source_system/updated_at），无 description 无 inputSchema；能力面条目仅 name + tier；
- Schema 一律按需：rip.get_tool_schema 对注册面返回 tool 表 input_schema/output_schema 列（主库优先 aux 回退，同 handle_tools ?tool_id= 详情顺序；列 NULL 或坏 JSON 记 null，parse_json_or_none 口径），对能力面返回内置入参 Schema。

## 四、同源口径与两处已登记偏差

同源基准：apps/server/rp_server.py handle_tools（2026-11-14 读到的实现，行号见对齐点）。对齐点：

- 数据面守卫同 phase14_source（rp_server.py:231-241）：主库有 tool 表一律主库优先；缺表回退 aux；都无则真实空态说明（Unavailable，reason 词表同 rp_server.py:242-259：aux_db_not_configured / aux_db_missing / table_missing），不造假数据。
- 列投影同 TOOL_LIST_COLUMNS（rp_server.py:507）十二列。
- 过滤同款：keyword 按 name/tool_id LIKE 字面匹配（like_escape 转义 + %needle% 包裹，rp_server.py:181-183、704）；lifecycle/governance 词表白名单（rp_server.py:496-497，check_enum 口径），词表外回 -32602。
- 排序同款：updated_at DESC, name COLLATE NOCASE ASC。
- aux 增量合并语义同 handle_tools 注释（rp_server.py:669-670「aux 只补主库没有的行，不重复主库已有条目」）。

已登记偏差（对账在无过滤或页覆盖全量时两口径一致，selftest 实测 22=22 逐条一致）：

1. 去重集合：rp_server.py:737-747 与「本页已返回的 id 集合」去重（limit 截断页下存在跨页重复的理论缝隙）；MCP 读面与「主库全量 id 集合」去重（分页游标下无跨页重复）。
2. aux 段过滤：handle_tools 的 aux 段不过滤（keyword/lifecycle/governance 只作用于主库段）；MCP 读面对 aux 段同样生效（主库回退态下无差异）。

## 五、分页

- MCP tools/list 用协议自带游标：请求 params.cursor（缺省空串 = 首页）+ params.limit（1 到 200，缺省 20，边界同 rp_server.py parse_limit 的 MAX_LIMIT=200）；响应 nextCursor（形状 `v1-<offset>`，不透明游标，非法游标回 -32602），末页不带 nextCursor。
- 游标偏移作用于组合目录（能力面前段 + 注册面段），遍历不重不漏（tests/mcp_protocol.rs 分页用例 + selftest 第 6 段 limit=5 强制多页实测 6 页 27 条）。
- 能力面不受过滤参数影响（恒在前段）；过滤只作用于注册面段。rip.list_tools（tools/call）返回 handle_tools 口径信封（total/returned/limit/offset/filters/items/nextCursor），total 只计注册面行数。

## 六、握手鉴权（禁止匿名）

- 令牌来源：RP_MCP_TOKEN 环境变量（stdio MCP 服务器的凭据通道 = 宿主 spawn 时注入，先例 github-mcp-server 的 GITHUB_TOKEN 同款形态；与 rp_server 的 Bearer 头不可比，stdio 无请求头）。
- 拒绝行为：RP_MCP_TOKEN 未配置时进程不退出（保持可观测），所有带 id 请求一律 -32001 auth_required；stderr 显著告警。
- 可选出示：initialize params._meta.rp.auth 出示 `Bearer <token>` 时按常时比较校验（长度差早退 + 定长 XOR 折叠，不随匹配进度分支），不匹配 -32001 auth_mismatch；不出示不校验（一次性管道形态的 tools/list 单请求无需出示，ask 点名的自验命令原形即无出示）。
- 会话模型：无状态单请求（每次管道写入独立处理），判据三交互不依赖先发 initialize。

## 七、「MCP 工具面不出现 Tool Center 没有的能力」的落法

读法：MCP 目录的注册面严格 1:1 投影 tool 表（同一读面，不发明条目）；能力面只收录计划 §11:640-656 中由既有平台能力面承载的 P9-1 子集（rip.list_tools/get_tool_schema 对应 Tool Center 的目录与 Schema 读面；rip.run_tool 对应 tool-engine 派发链；rip.search_knowledge 对应 knowledge 表检索面；rip.get_run_status 对应 tool_run 表），且三个执行/查询面在 P9-1 仅注册不执行。计划 §11 其余 rip.* 条目（plugins/agents/workflows/evidence 面）按批次归属 P9-2 到 P9-5，本批不出现。

## 八、验证面

- crates/mcp-server/tests/mcp_protocol.rs：14 用例（握手回显与零全文、无令牌被拒、出示令牌校验、分页游标不重不漏、limit/游标边界、摘要投影零说明、同源排序与库直查 SQL 逐条一致、aux 去重、过滤词表、按需 Schema（注册面/能力面/双 null/不存在）、未接线 isError、方法子集面、Unavailable 真实空态）。
- apps/server/selftest_phase9_mcp.sh：23 断言（真实 rp_server 进程 + 真实 rp-mcp 二进制端到端；tools/list 注册面与 /api/tools items 逐条对账；无令牌握手被拒；分页遍历；能力面在场；按需 Schema 与 /api/tools?tool_id= 同源；生产库零写指纹）。
- 本机 7800 端口常驻用户工作台 rp_server（PID 1173，只读 20 行主库面），ask 点名的 curl 对账命令按原文对活体服务执行通过（20 条逐条一致）；selftest 全量跑 SELFTEST_PORT=7811（脚本端口守卫避免打到别人的服务）。

## 九、已知取舍

- tools/list 条目无 description 无 inputSchema，偏离 MCP 2024-11-05 Tool 对象的完整形状（子集口径：Schema 按需是 P9-1 判据原文）；严格客户端需要 Schema 时先调 rip.get_tool_schema。
- P9-1 无会话保持（无状态单请求），initialize 后不维持授权态；P9-2 接执行通道时若引入长会话需重审此点。
- 能力面 Schema 为 crate 内置词表（capability_input_schema），不是 tool 表行；Tool Center 里没有这五行（能力面是平台协议面不是注册面），对账口径只对注册面逐条断言。
- aux 合并两处偏差见 §四；若 handle_tools 未来收口两处实现缝隙，本读面应反向对齐。

## 十、P9-2 追加：rip.run_tool 端到端执行通道（2026-11-14）

### 一、批次定位与派发纪律

P9-2 判据（ROADMAP §10.3 原文）：以最小 MCP 客户端模拟无工具外部 AI 完成一次 rip.http.inspect 调用；本批另按 ask 落 rp-tool run/status/cancel 对外子命令与 T2 authorization_ref 校验保留。派发纪律（ask 原文「与 Web Workflow Tool 节点同一引擎，不建第二套派发逻辑」）落法：`src/dispatch.rs` 只做三件事（请求整形、Engine 受理、既有适配器驱动），受理面就是 `tool_engine::Engine::submit`（crates/tool-engine/src/lib.rs:157，D-TC-05 全量校验原样生效），驱动面只有两条既有路径：

1. python_worker → `adapter::execute_via_engine`（真实 python 子进程，rp-workflow CLI 的 PythonWorkerToolDriver 同款装配口径）；
2. podman_host / podman_vm → `runtime_gateway::Gateway::dispatch`（真实 CommandProbe::podman 心跳判定，探测失败落 WAITING_FOR_RUNNER 不 FAILED，计划 §5.6:336 原文语义）→ `adapter::execute_container_via_engine`（P3-3 全链同款）。

### 二、数据面：运行库与生产库零写

- 执行通道的 tool_run / artifact / evidence / runtime_node 行全部落在运行库（RP_RUN_DB，缺省 `rip-data/tool-runs.db`，gitignore 运行时数据），按仓库 migrations 001 到 018 全量迁移（零新增迁移号）；生产 migrations/platform.db 是目录读面（只读），不在执行链内，零写红线延续 P9-1。
- 工具登记沿用既有 `rp-tool register --db <run_db>` 路径（SESSION-HANDOFF「添加新工具 = 一条可复制路径」）；dispatch 只做执行不做登记。
- 目录读面（主库/aux）与运行库分离：tools/list 注册面仍只投影目录两库（Tool Center 同源口径不变）；`rip.get_tool_schema` 增加 run_db 回退段（目录两库 NotFound 时读运行库 tool 表同一形状，source=run_db，只读不发明条目）。

### 三、Artifact 与 Evidence 引用（P7-2 同一登记实现）

终态（SUCCEEDED 与 FAILED/TIMED_OUT/CANCELLED 都登记）经 `agent_engine::EvidenceRegistrar::register_tool_result` 落 Artifact（role=primary，结果快照落 `<RP_ARTIFACTS_DIR>/<run_id>/`）+ Evidence（type=log，storage_ref 指向 Artifact uri，source_pk=run_id）双行，同 sha256 可互相对账；evidence id 回填信封 evidence_refs 后随 MCP 载荷返回。本批对 EvidenceRegistrar 只加 `with_face`（captured_by 前缀与 provenance 首词，缺省 agent-engine 逐字节不变，MCP 面传 rp-mcp），登记逻辑零复制。与容器信封内 `artifacts`（如 body.bin）的关系：两套引用并存，信封 artifacts 是工具产物本体，登记 artifact 是结果快照（P7-2 语义），均有 evidence 可复核。

### 四、CLI 面（rp-tool run/status/cancel）与依赖方向的裁定

rp-tool 所在 crate（tool-registry）不能反向依赖 tool-engine（tool-engine 依赖 tool-registry，cargo 禁环），而 Phase 7/8 selftest 以 `-p tool-registry --bin rp-tool` 构建面为由要求 rp-tool 留在原 crate。裁定：三个执行子命令实现为最小 MCP 客户端（spawn rp-mcp 子进程做一次性 stdio JSON-RPC 交换），与外部 AI 走同一执行通道（dispatch.rs 唯一实现），零第二套派发逻辑；一次性令牌经环境变量注入子进程，禁止匿名纪律由 MCP 握手鉴权承载。validate/register/list/get 四个子命令原样保留。

### 五、能力面扩量与状态

能力面 5 条扩为 6 条（新增 rip.cancel_run，计划 §11:651 原文条目）；rip.run_tool / rip.get_run_status / rip.cancel_run 三条 wired，rip.search_knowledge 仍未接线（后续批次）。无状态单请求模型延续：每次 tools/call 独立开运行库连接（migrate 幂等），执行完即释放；长会话仍不存在。

### 六、机器差异口径与已暴露的遗留缺陷

- 容器工具完整回路需要 podman 与 digest 钉死验收镜像（Phase 3 交付物，运行时数据 ADR §3.2）；无 podman 机器（2026-11-14 执行机实测 podman/docker 均不在 PATH）上 rip.http.inspect 如实停在 WAITING_FOR_RUNNER 且 run 行在库，「结构化结果 + artifact + evidence」判据由 python_worker 工具（rip.doc.parse）同通道实跑承载，容器全链断言仍由 web_research_e2e 在受控环境承载（V-9 口径）。
- 首次经引擎执行暴露 Phase 6 样例工具 rip.http.tls_inspect 的登记件/worker 不一致（output_schema.error 声明 string，worker 成功路径回 null → 引擎强校验 FAILED）：P6 当时只 register + ping 未引擎执行。本批不改 workers/ 与其登记件，待 Phase 6 责任批次修复。

### 七、验证面

- crates/mcp-server/tests/run_tool_e2e.rs：7 用例（无令牌 -32001、T2 缺授权 POLICY_DENIED 且零 run 行、T2 带授权真实 worker 全链、doc.parse 全链 + 状态投影 + run_db Schema 回退、未知工具不落行、WAITING 取消受理 + cancel_requested_at、容器工具 run 行在场（V-9 守卫））。
- crates/mcp-server/tests/mcp_protocol.rs：14 用例（能力面六条、run face 未配置态、注册面直呼定向说明、缺 run_id -32602 等更新）。
- apps/server/selftest_phase9_mcp.sh：23 断言（能力面计数 6 + 注册面对账、第 10 段改 P9-2 语义：隔离临时运行库中未登记工具如实 unknown tool 拒绝、rp-mcp spawn 全部 RP_RUN_DB 隔离不写共享 rip-data）。

## 十一、P9-3 追加：REST 对外契约化收口与 CLI 帮助面（2026-11-14）

### 一、批次定位与纪律

P9-3 判据（ROADMAP §10.3 原文）：同一业务操作（列工具、查 Schema、跑工具、查 Run）REST 与 CLI 各走通且结果一致；本批落契约面（文档 + 机器对账 + trace 透出 + 限频 + 审计 + 帮助面统一），双通道同断言载具归 P9-4。以 rp_server.py 既有 API 面为准（16 条只读路由 API_ROUTES + plugins/workflows 白名单写面），不另起第二套接口风格；全部形状以实现为准，契约文档 docs/phase9/REST-API-CONTRACT.md 与 rp_server.py ROUTES 逐条机器对账（selftest_phase9_rest.sh 第 10 段：API_ROUTES 16 键、GET 17 行、POST 6 形状、PLUGIN_WRITE_ACTIONS/WORKFLOW_WRITE_ACTIONS 词表逐 token、零发明端点五重断言）。

### 二、trace 关联透出（SESSION-HANDOFF §十 Phase 9 追加项）

- 响应体：三类 run 详情（/api/runs?run_id=、/api/workflow-runs?id=、/api/agent-runs?id=）顶层新增 `correlation_id`（= run 的审计关联锚 run_id，audit_event.correlation_id 同语义）；六 ID 关联字段保持既有 `correlation` 字段形状不移动（向后兼容，前端零改动）。/api/traces?id= 新增顶层 `correlation_id`（= query_id）与 `trace_id`（单锚命中时 = 唯一链锚；多锚命中不猜，仍走 trace_ids 列表）。
- 响应头：HTTP 层 `trace_response_headers`（rp_server.py:216）统一提取，`X-Trace-Id` 与 `X-Correlation-Id` 锚在场即发（取值顺序：顶层 trace_id > run.trace_id > correlation.trace_id；顶层 correlation_id > run.run_id）；列表页 / 空态说明 / 错误响应锚不在场不发头。写面 run/resume/retry 受理响应带 `correlation_id`（retry = new_run_id）。
- 不发明新关联体系：锚语义全部沿用 015/018（trace_id 即链锚 = run_id，六 ID 同 ID 空间）。

### 三、全局限频与子进程超时显式化

- 全局限频：服务进程级滑动窗口（`check_rate_limit`，rp_server.py:186），全局共享预算，只计 /api/*（GET+POST，先限频后鉴权，401 也计）；`RP_RATE_LIMIT_RPM` 缺省 600 req/60s，0 关闭；超限 429 `rate_limited` + `Retry-After: 60`；多线程记账用锁（ThreadingHTTPServer 并发安全）。缺省值不动既有 selftest（auth 11 项约 15 请求）。
- 子进程超时口径显式常量：`PLUGIN_SUBPROCESS_TIMEOUT_S = 600`（rp_server.py:153，plugin 写面同步 run）与 `WORKFLOW_SYNC_TIMEOUT_S = 60`（rp_server.py:154，workflow pause/put 同步 run），超限 504 plugin_cmd_timeout / workflow_cmd_timeout；workflow run/resume/retry 保持异步 Popen（不等待，状态轮询 GET /api/workflow-runs?id=），不适用同步超时。行为零变化，只是常量化 + 契约化。

### 四、对外访问审计（019 + JSONL 双形状）

- 写面是 rp_server 的 JSONL 审计流（`RP_API_AUDIT_LOG`，缺省关闭）：每个 /api/* 请求响应收尾落一行 `api_access` 事件（ts/method/path/remote/status/duration_ms/trace_id/correlation_id/rate_limited/auth_enabled）；写入失败只告警不阻塞。
- SQL 形状是 migrations/019_external_api_audit.sql 的 `external_api_audit` 表（append-only，列与 JSONL 行一一对应 + 三索引 + schema_version=19）：rp_server 本身零 SQL 写（Phase 6 selftest_phase6.sh:213 全文件静态门禁不动摇），本表供消费侧（ETL/装载/后续只读投影）按列形状装载；019 只在开发库副本与测试库执行，platform*.db 不应用。
- 迁移链尾演进 13..18 → 13..19：六处批次归属的链尾断言随批更新（migration_dev_copy_p7_3 / migration_idempotent / migrations_agent_data_plane / migrations_context_broker / migrations_runtime_node / migrations_tool_contract / lifecycle_full_path 重放计数 / workflow migration_replay），先例 P8-3 落 018 时同款更新。

### 五、CLI 帮助面统一（11 bin）

- 统一口径：`--help`/`-h`（argv 任意位置）= 用法全文到 stdout + exit 0；误用（无参/未知子命令/缺必填）= 用法全文到 stderr + exit 2；骨架统一（usage 首行 / subcommands 或 commands / exit 节 / help 节）。rp-mcp 是协议面不入 CLI 口径（无 --help，stdio JSON-RPC）。
- 收口前差异（如实登记）：8 个 bin 此前无 --help；rp-review/rp-search-index 无参与 --help 同路径（exit 0），现无参归误用；rp-resolve 错误路径补 usage；rp-plugin 单行巨型 usage 重排为分节骨架；P9-2 注记的 rp-tool run --help exit 2 → 0（快照不回改，本批 RELEASE 登记）。
- 逐 bin 参考：docs/guides/CLI-REFERENCE.md（速览表 + 逐 bin 子命令/数据面/exit 语义/可复制例子，例子实测可跑），机器断言在 selftest_phase9_rest.sh 第 11 段（11 bin 计数 + --help exit 0 首行 + 无参 exit 2）。

### 六、验证面

- apps/server/selftest_phase9_rest.sh：33 断言 12 段（019 迁移 fresh+018 副本双路；workflow 真实 fixture（register + put + run）→ run 详情六 ID 关联字段 + correlation_id + X-Trace-Id/X-Correlation-Id 双头；tool run 详情与 /api/traces 关联透出 + 锚不在场不发头负例；POST 白名单外 404/405 五例；审计 JSONL 行形状 = 019 列；限频独立实例 RP_RATE_LIMIT_RPM=5 实测 5×200 后 429 + Retry-After + body code + 审计 rate_limited=1；契约文档五重对账；11 bin 帮助面；生产库指纹零写）。
- 回归：selftest_auth.sh 11/11（鉴权四件套不动）；cargo test --offline --workspace 603 passed 0 failed（105 目标，P9-2 基线同值，链尾断言随批演进）；clippy --workspace --all-targets -D warnings 0 error。

## 十二、P9-4 追加：双通道一致性载具、rip:// 资源面与 prompt 面、MCP 状态卡（2026-11-14）

### 一、一致性载具（ROADMAP §10.4 验收第 2 条 + §10.5 风险 1）

- 载具 crate 名按 ROADMAP 预留定稿为 `rp-mcp-parity`（占位 lib + tests/parity.rs 五用例），验收命令 `cargo test --offline -p rp-mcp-parity` 原形可跑（0.88s 实测），并进 .github/workflows/ci.yml 常驻 parity job（风险 1 原文：载具必须是常驻 CI 门禁而不是一次性验收）。
- 通道口径：REST = 真实 rp_server.py HTTP 子进程（std TcpStream 薄客户端，vendor 无 HTTP client crate）；MCP = `McpServer::handle_line` 协议面（bin rp-mcp stdio 循环同一入口，stdio 循环由 mcp-server tests 与 selftest_phase9_mcp.sh 真实二进制覆盖）。两通道同一临时主库（001 水位，platform.db 同款）+ 同一临时运行库（全量迁移 + Registry::register，rp-tool register 库面同款），生产 platform*.db 零接触。
- 五操作断言面：列工具（total/id 序列/逐条核心字段 + /api/mcp-catalog 能力面词表 wire 对照）、查 Schema（input/output JSON 逐字节）、跑工具（REST tool 节点 output == MCP envelope result 全等；链锚 workflow = wfr-id、MCP = 自锚定 run-id）、查 Run（两通道互读对方 run，correlation/evidence/artifact 关联一致；未知 run 两通道同一不存在语义）、查 trace（/api/traces 与 rip://runs/<run_id> 同链锚同 evidence 集）。
- 前置守卫：python3 在位（V-9）；rp-workflow 二进制缺位时载具内 ensure_bin 按 cargo build --offline 自动构建（CI 与新机器开箱可跑）。

### 二、rip:// 资源面与 prompt 面（计划 §11:662-668，P9-5 内容并入本批）

- 七 resource：静态四件进 resources/list（workspace/current、knowledge/catalog、plugins/catalog、tools/catalog），模板三件进 resources/templates/list（tasks/{task_id}、runs/{run_id}、evidence/{evidence_id}），MCP 2024-11-05 协议口径不把模板伪装成具体资源；resources/read 词表外 -32602、实体不存在 -32602、数据面不可读 -32603（reason 词表同目录读面）。
- 读面复用：rip://runs/<run_id> 与 rip.get_run_status 同一 RunChannel::run_status 投影；rip://tools/catalog 与 rip.list_tools 注册面同一 list_registry；目录四件数据面不可读返回 available:false + reason 诚实空态载荷（rp_server 读面口径）。rip://evidence/<id> 附 storage_ref 指向的 Artifact 快照文本内联（64 KiB 上限，超出截断标注）。
- prompt 三条：rip.tool_run_playbook（八步剧本）/ rip.run_audit_review（审计链复核）/ rip.knowledge_brief（知识简报取材）；prompts/get 未知 prompt 与缺必填参数 -32602。initialize capabilities 增 resources（subscribe=false）与 prompts，instructions 保持一行导航。
- 新增 RP_PLUGIN_DB 环境变量（缺省 migrations/dev-p6.db，rp_server 同默认；=none 关闭；缺席诚实空态）。

### 三、MCP 通道审计链自锚定（行为演进）

- rip.run_tool 受理改走 `Engine::submit_correlated(&request, Some(run_id))`（tool_run.trace_id = run_id）且 Registration.trace_id = Some(run_id)（artifact/evidence 行同链）。此前 MCP 通道 trace_id 全 NULL，/api/traces 无法反查；自锚定后两通道运行都能经同一 trace 面回查（载具 op5 断言面）。惯例依据：015/018「run 即链锚」与 workflow tool 节点同一 submit_correlated 入口；T2 校验、词表、拒绝不落行零变化。P9-2 RELEASE 的 trace_id: None 快照不回改，本节为权威口径。

### 四、/api/mcp-catalog 与 Tool Center MCP 状态卡

- rp_server 新增 GET /api/mcp-catalog（API_ROUTES 16→17，契约文档 ENDPOINTS 段与 selftest 第 10 段对账断言随批演进 17/18）：catalog_count（与 MCP tools/list 注册面 total 同一读面，handle_tools 合并逻辑抽为 _tool_catalog_rows 共用实现）+ capability_tools（mcp-server lib CAPABILITY_TOOLS 六条同源镜像）+ 读时对账结果（注册面投影完整性 + 词表校验，drift 如实列出）。rp_server 只读纪律不落状态行，「最后对账结果」= 本次读取时点对账（reconciled_at），跨通道 wire 级对账由 rp-mcp-parity CI 常驻承载（端点 note 明示定位）。
- web：ToolCenterPage McpCatalogCard 四态（对账正常 / 对账漂移 N 条逐条展示 / 数据面不可读诚实空态 / 读取失败降级说明明示「目录本身可用」）+ 载荷形状防御；api.ts 增 McpCatalogStatus 与 fetchMcpCatalog。测试三件：mcp-smoke.test.tsx（vitest 4 用例）、mcp-status.spec.ts + playwright.mcp.config.ts（双模式：route-mock 三态 / RP_P9_E2E=1 真实后端，P6-8 同款 route 转发链路）、视觉门 /tools 页补 /api/mcp-catalog stub（55 用例全绿）。
- 浏览器实测证据：docs/phase9/browser-e2e/（真实后端 4 截图 + route-mock 三态 3 截图 + README 复跑命令；真实 Chromium 用户缓存 chromium-1234 注入，零系统级安装）。

### 五、验证面（P9-4 增量）

- crates/rp-mcp-parity/tests/parity.rs：5 用例（op1 到 op5）。
- crates/mcp-server/tests/mcp_protocol.rs：method_dispatch 用例更新（resources/list 4 件、templates 3 件、prompts 3 条、词表外 -32602、resources/subscribe -32601）。
- apps/server/selftest_phase9_rest.sh：路由计数断言随批演进（17 条）。
- 门禁实测数字与浏览器截图证据见 docs/releases/RELEASE_2026-11-14_PHASE9.md §三。

## 十三、rmcp 迁移整改批次追记（2026-11-14，审计 §5 十项落地）

> 外部审计（[MCP_MATURE_LIBRARY_AUDIT_2026-11-14.md](MCP_MATURE_LIBRARY_AUDIT_2026-11-14.md)）判定手写协议层不可长期保留，用户定案「按照这个方案」。本节记协议形态的批次内演进，选型与七小节台账见 [RMCP-MIGRATION.md](RMCP-MIGRATION.md)。

### 一、协议层交接边界

- 退役：JSON-RPC 2.0 解析、方法派发、result/error 信封、通知静默、initialize 回显、常时令牌比较、`_meta.rp.auth` 出示校验（手写段全部删除）。
- 接管：rmcp 3.10.0（`default-features=false, features=["server","transport-io"]`，crates.io 发布版精确钉定）承载 initialize 握手状态机、协议版本协商、协议错误信封、通知分发与 stdio transport；tokio 为运行时面直依赖（workspace 最小 feature 子集）。
- 保留：catalog / dispatch / resources / prompts 四个业务模块零重写，新增 `service.rs`（`impl rmcp::ServerHandler for McpServer`）做业务面映射；stdout 只出协议消息、日志走 stderr 纪律不变。

### 二、协议形态演进（与 §二/§五/§六 的差异）

- P0：tools/list 每条为标准 Tool 对象（必需 name + inputSchema，注册面 input_schema 列直读，缺省宽松 object 形态；分页只控量不省必需 Schema）；标准对象不再携带 tier/tool_id 等平台摘要列，对账面移到 rip.list_tools 信封（十二列同源不变）。
- P1：initialize 版本协商由 rmcp 承载，支持列表 2024-11-05 至 2025-11-25（2026-07-28 无握手面不在列表）；列表内协商回显、列表外回落服务端声明版本 2025-11-25，不回显任意客户端版本。
- tools/list 收敛为标准分页（唯一入参 cursor，页大小服务端固定 20）；keyword/lifecycle/governance 过滤与 limit 承载面移到 rip.list_tools（能力不丢失）。
- RP_MCP_TOKEN 重定义：宿主环境变量注入即授权边界（缺失/空白 fail-closed exit 2 拒启），不实现不发明 MCP 标准外认证字段（§六 的 -32001 三段语义退役，三态实测保留：缺失/错误形态/正确注入）。
- tools/call 阻塞面（sqlite 与工具执行）经 spawn_blocking 隔离 async 运行时，run 执行期 cancel 仍可并发受理（D-TC-04 不退化）。
- 连带：rp-tool（P9-2 最小客户端）补 initialize（id=0）+ notifications/initialized 前置（rmcp 状态机首请求必须 initialize），CLI 输出形状不变；剧本 prompt 第 1 步文本同步更正。

### 三、验证面（本批增量）

- crates/mcp-server/tests/ 重写为 spawn 真实 rp-mcp 二进制（tests/common/mod.rs 驱动，CARGO_BIN_EXE）：mcp_protocol.rs 11 用例（三态令牌 / 协商 / P0 标准对象 / 分页 / 同源 / 按需 Schema / 派发面 / conformance）；run_tool_e2e.rs 7 用例（执行链与取消语义原判据保留）。
- rp-mcp-parity：MCP 通道改 spawn 真实 rp-mcp（Mutex 串行驱动），op1 能力面对账改为目录前段固定次序 + 标准 Tool 形状断言，op2 到 op5 逻辑不变。
- selftest_phase9_mcp.sh：mcp_req 会话化（initialize 前置）、§4 三态令牌、§5 协商、§6 标准 Tool 对象断言、§8 对账改走 rip.list_tools；26/26 全过。
- 真实客户端 e2e：docs/phase9/rmcp-client-e2e/（独立 Node 客户端 10 步全过，会话日志留证）。
