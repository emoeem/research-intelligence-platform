# 更新说明 · 2026-11-14（Phase 9 第三批 P9-3：REST 对外契约化收口与 CLI 帮助面统一）

> Phase 9 第三批落地：REST 面第一次有了机器对账的对外契约（信封形状、ApiError 词表、鉴权四件套、trace 关联透出、全局限频、子进程超时口径、访问审计），CLI 面第一次有了统一帮助口径与参考手册（11 bin）。trace 六 ID 与 correlation_id 现在同时透出到响应体与响应头（X-Trace-Id / X-Correlation-Id）；超限请求 429 带 Retry-After；对外访问审计以 JSONL 流 + migrations/019 SQL 形状双形状落面。全部以 rp_server.py 既有 API 面为准，不另起第二套接口风格；生产 platform*.db 全程零写。

| 项         | 值 |
| ---        | --- |
| 状态       | 生效中（Phase 9 批次 3/4，P9-4 到 P9-5 未实施） |
| 日期       | 2026-11-14 |
| 适用版本   | Phase 9 P9-2 工作树及之后 |
| 阶段交付   | 本批工作树（提交与 push 短哈希按仓库惯例由批次收口回填，先例 `05ed5ee`/`f02d9b3`） |
| 数据库迁移 | 新增 migrations/019_external_api_audit.sql（external_api_audit 对外访问审计表，append-only + 三索引 + schema_version=19 + user_version 双写；fresh 001+019 与 018 水位开发副本双路实测，重放幂等；platform.db 与 platform-dryrun.db 不应用、全程零写：selftest 第 1/12 段指纹前后一致） |
| 自动化测试 | `cargo test --offline --workspace`：603 passed 0 failed（105 目标；P9-2 基线同值 603，迁移链尾断言随批演进 13..18 → 13..19）；`python3 -m unittest discover -s workers/python/tests`：211 OK 跳过 11（workers 零改动） |
| 静态检查   | `cargo clippy --offline --workspace --all-targets -- -D warnings`：exit 0 零告警（入库当日实测） |
| 相关       | [phase9/REST-API-CONTRACT.md](../phase9/REST-API-CONTRACT.md)（对外契约 + 端点总览机器对账）、[guides/CLI-REFERENCE.md](../guides/CLI-REFERENCE.md)（11 bin 参考手册）、[phase9/PHASE9-DESIGN.md §十一](../phase9/PHASE9-DESIGN.md)（设计追加章）、apps/server/selftest_phase9_rest.sh（33 断言）、apps/server/selftest_auth.sh（11 项回归） |

## 一、交付内容

### 1. trace 关联透出：响应体 + 响应头（SESSION-HANDOFF §十 Phase 9 追加项）

- 响应体：三类 run 详情（`/api/runs?run_id=`、`/api/workflow-runs?id=`、`/api/agent-runs?id=`）顶层新增 `correlation_id`（= 该 run 的审计关联锚 run_id，与 `audit_event.correlation_id` 同语义，rp_server.py 六 ID correlation 字段形状不动、向后兼容）；`/api/traces?id=` 新增顶层 `correlation_id`（= query_id）与 `trace_id`（单锚命中时 = 唯一链锚，多锚命中不猜仍走 `trace_ids` 列表）。
- 响应头：HTTP 层新增 `trace_response_headers`（rp_server.py:216）统一提取，`X-Trace-Id` 与 `X-Correlation-Id` 锚在场即发（取值顺序：顶层 trace_id > run.trace_id > correlation.trace_id；顶层 correlation_id > run.run_id）；列表页、空态说明、错误响应锚不在场不发头（负例有断言）。写面 run/resume/retry 受理响应带 `correlation_id`（retry = new_run_id）。
- 不发明新关联体系：锚语义全部沿用 015/018（trace_id 即链锚 = run_id，六 ID 同 ID 空间）。

### 2. 全局限频（超限 429）与子进程超时显式化

- 全局限频：`check_rate_limit`（rp_server.py:186）服务进程级滑动窗口，全局共享预算，只计 `/api/*`（GET 与 POST 都计，静态资源不计；先限频后鉴权，401 也计数，保护鉴权面）；`RP_RATE_LIMIT_RPM` 缺省 600 req/60s，`0` 显式关闭；超限 429 `{"error":{"code":"rate_limited",...}}` + `Retry-After: 60`；ThreadingHTTPServer 多线程记账加锁。
- 子进程超时常量化（行为零变化）：`PLUGIN_SUBPROCESS_TIMEOUT_S = 600`（rp_server.py:153，plugin 写面同步 run）与 `WORKFLOW_SYNC_TIMEOUT_S = 60`（rp_server.py:154，workflow pause/put 同步 run），超限 504 plugin_cmd_timeout / workflow_cmd_timeout；workflow run/resume/retry 保持异步 Popen 不等待（状态轮询 GET /api/workflow-runs?id=），不适用同步超时。

### 3. 对外访问审计（JSONL 流 + 019 SQL 形状）

- rp_server：`RP_API_AUDIT_LOG` 指向 JSONL 文件时每个 `/api/*` 请求响应收尾追加一行 `api_access` 事件（ts/method/path/remote/status/duration_ms/trace_id/correlation_id/rate_limited/auth_enabled，`audit_api_access` rp_server.py:202）；缺省关闭零行为变化；写入失败只告警不阻塞响应。rp_server 零 SQL 写纪律不动摇（Phase 6 selftest_phase6.sh 全文件静态门禁原样通过）。
- migrations/019_external_api_audit.sql：`external_api_audit` append-only 表（13 列 + rate_limited/auth_enabled CHECK + 三索引），列与 JSONL 行一一对应，供消费侧（ETL/装载/后续只读投影端点）装载；019 只在开发库副本与测试库执行。
- 迁移链尾断言随批演进（先例 P8-3 落 018 同款）：migration_dev_copy_p7_3（13..19 + 终版 19）、migration_idempotent（全链 1..19）、migrations_agent_data_plane / migrations_context_broker / migrations_runtime_node / migrations_tool_contract（discover 全链 + 链尾 019 + user_version 19）、plugin-runtime lifecycle_full_path（009 自钉复位重放 10 枚）、workflow-engine migration_replay（replay 版本 19）。

### 4. CLI 帮助面统一（11 bin）与参考手册

- 统一口径：`--help`/`-h`（argv 任意位置）= 用法全文 stdout + exit 0；误用（无参/未知子命令/缺必填旗标）= 用法全文 stderr + exit 2；骨架统一（`usage: rp-<bin>` 首行 / subcommands 或 commands 分节 / exit 节 / help 节）。
- 逐 bin 收口：rp-agent / rp-workflow / rp-context / rp-research / rp-tool / rp-plugin / rp-knowledge-fetch / rp-tool-health 新增 --help 双路径（usage_text() + usage() 拆分，误用路径退出码逐 bin 保持不变）；rp-review / rp-search-index 无参从「打印帮助 exit 0」改为误用路径（stderr exit 2）；rp-resolve 错误路径补 usage 同出；rp-plugin 单行巨型 usage 重排为分节骨架（词表零增减）。子命令语义、旗标、成功/失败退出码零变化。
- docs/guides/CLI-REFERENCE.md：帮助面统一口径 + 12 bin 速览表（11 CLI + rp-mcp 协议面注记）+ 逐 bin 参考（子命令、数据面与 --db 缺省值、exit 语义、可复制例子，rp-resolve/rp-review 两例本机实跑验证）+ 维护纪律（新增 bin 必须同批更新手册与 selftest BINS）。

### 5. 契约文档与机器对账

- docs/phase9/REST-API-CONTRACT.md：范围与三面同源关系、通用约定（编码/分页/1 MiB 体上限/两类成功信封/真实空态说明）、鉴权四件套、ApiError 状态码与机器码词表、trace 透出规则表、全局限频、子进程超时口径、审计、端点总览（`<!-- ENDPOINTS:BEGIN/END -->` 标记段机器解析）、写面响应语义、对账纪律。
- 机器对账（selftest_phase9_rest.sh 第 10 段，五重断言）：API_ROUTES 16 键逐条在文档 GET 行；文档 GET 集合 == 16 静态 + 1 动态（/api/plugins/<plugin_id>）；文档 POST 集合 == 6 白名单形状；PLUGIN_WRITE_ACTIONS（+upgrade）与 WORKFLOW_WRITE_ACTIONS 词表从 rp_server.py 正则提取后与文档行逐 token 一致；标记段零发明端点。

### 6. apps/server/selftest_phase9_rest.sh（新增，33 断言 12 段）

覆盖：019 迁移 fresh（001+019）与 018 水位副本（dev-p8 副本拷贝）双路 + 重放幂等；workflow 真实 fixture（rp-tool register + rp-workflow put + run wf-p8-tool 真实链，run trace_id = run_id 库行核对）；run 详情响应体六 ID 关联字段 + 顶层 correlation_id + `curl -si` 实测 X-Trace-Id/X-Correlation-Id 双头；tool run 详情与 /api/traces 关联透出 + 锚不在场不发头负例；POST 白名单外 404/405 五例（只读端点 405、白名单外 405、未知路径 405、插件未知动作 404、workflow 未知动作 405）；审计 JSONL 行形状 = 019 列；限频独立实例（RP_RATE_LIMIT_RPM=5）实测前 5 请求 200 后 3 请求 429 + Retry-After + body code + 审计 rate_limited=1 行；契约文档五重对账；11 bin 帮助面统一断言（计数 + --help exit 0 首行 + 无参 exit 2）；生产库零写指纹复核。两连跑全 PASS。

## 二、验收实测（入库当日）

| 判据（ask 原文） | 命令 | 结果 |
| --- | --- | --- |
| selftest_auth.sh 11 项全 PASS exit 0（既有口径回归） | `bash apps/server/selftest_auth.sh` | PASS=11 FAIL=0（鉴权四件套 + 只读语义 + 路径穿越 + 开发模式告警全保持） |
| selftest_phase9_rest.sh：run 详情响应体含六 ID 与 correlation_id | 脚本第 5 段 | PASS（correlation 六键全在场 + 顶层 correlation_id = run_id） |
| curl -si 实测 X-Trace-Id/X-Correlation-Id 头在场 | 脚本第 5/6 段 | PASS（workflow run 详情双头 = run_id；tool run 详情 X-Correlation-Id = tool_run_id；/api/traces 单锚双头；列表页不发头负例 PASS） |
| 超限请求 429 | 脚本第 9 段（RP_RATE_LIMIT_RPM=5 独立实例） | 5×200 后 429；Retry-After 头在场；body code=rate_limited；审计 rate_limited=1 |
| POST 白名单外路径仍 404/405 | 脚本第 7 段 | POST /api/stats → 405、/api/agents → 405、未知路径 → 405、插件未知动作 → 404、workflow 未知动作 → 405（全 PASS） |
| 契约文档端点清单与 ROUTES 逐条对账 | 脚本第 10 段 | PASS（16 键 + GET 17 + POST 6 + 词表逐 token + 零发明端点） |
| cargo test --offline --workspace | 原形命令 | 603 passed 0 failed（105 目标；P9-2 基线同值） |
| cargo clippy --offline --workspace --all-targets -- -D warnings | 原形命令 | exit 0 零告警 |
| workers unittest 回归 | `python3 -m unittest discover -s workers/python/tests` | 211 OK 跳过 11（workers 零改动） |
| 生产库零写 | selftest 第 1/12 段指纹 | platform.db 与 platform-dryrun.db 前后一致（零写红线保持） |

## 三、已知取舍与机器差异

1. **审计写面是 JSONL 流，019 表不是 rp_server 的直写目标**：rp_server 零 SQL 写是 Phase 6 静态门禁（selftest_phase6.sh 全文件 grep）与写面纪律的既有裁定，本批不改（改门禁 accommodative 即属推翻已验收能力）；external_api_audit 表定位为审计流的 SQL 存储形状，消费侧装载（ETL/后续只读投影端点）归后续批次。缺省关闭（RP_API_AUDIT_LOG 不配不落盘）。
2. **全局限频取全局预算而非按 IP**：ask 原文「全局限频（超限 429）」按字面落为服务进程级全局滑动窗口；本地单人工作台形态下按 IP 分桶收益有限且稀释保护强度。如需按客户端分桶，RP_RATE_LIMIT_RPM 之外另立词表属后续批次。
3. **rp-tool run --help 退出码 2 → 0**：P9-2 发布注记记录的 exit 2 为快照事实（DOC-STANDARD 不回改旧 RELEASE），本批统一口径后为 exit 0；CLI-REFERENCE.md §一.5 与本节为权威口径。
4. **REST 面不提供 run_tool 直执端点**：执行通道 = POST /api/workflow-runs 受理 workflow（异步）或外部 AI 经 MCP rip.run_tool（P9-2 通道）；「列工具、查 Schema、跑工具、查 Run」REST 与 CLI 各走通且结果一致的双通道同断言载具归 P9-4（本批判据是契约面收口，ROADMAP §10.3 P9-3 完成判据的 REST 侧对账已由 tools/runs 端点与 MCP 注册面同源口径承载，PHASE9-DESIGN §四）。
5. **本机环境**：执行机（2026-11-14）无 podman/docker（P9-2 同款），本批不涉容器路径；web 面零改动（vitest/视觉门未跑，Phase 8 基线不变）。

## 四、回滚方式

单提交回滚：`git revert <本批提交哈希>` 整体撤销（019 迁移文件、REST-API-CONTRACT.md、CLI-REFERENCE.md、selftest_phase9_rest.sh 为纯新增文件；rp_server.py / 11 个 bin / 六个迁移链尾断言测试为小改）。019 不在生产库落过（platform*.db 零写），回滚无迁移回退需求；审计 JSONL 与 rip-data 均为 gitignore 运行时数据。
