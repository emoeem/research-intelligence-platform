# 更新说明 · 2026-11-14（Phase 9 首批 P9-1：平台 MCP server 目录发现面）

> Phase 9 第一批落地：平台第一次有了自己的 MCP 入口（bin rp-mcp，stdio NDJSON JSON-RPC 2.0，MCP 2024-11-05 子集）。外部 AI 现在可以经 tools/list 分页翻看平台工具目录（能力面五个 rip.* 条目 + 注册面逐行投影 Tool Center 同一 tool 表，aux 增量合并同 handle_tools 口径），并用 rip.get_tool_schema 按需取 Schema；初始化响应与目录条目都不携带工具完整说明（计划 §11:658 纪律）。握手鉴权要求 RP_MCP_TOKEN，无令牌一律 -32001 拒绝。执行通道（rip.run_tool）本批只注册目录条目，调用返回结构化 isError 指明 P9-2 接线，不伪造成功。

| 项         | 值 |
| ---        | --- |
| 状态       | 生效中（Phase 9 批次 1/4，P9-2 到 P9-5 未实施） |
| 日期       | 2026-11-14 |
| 适用版本   | Phase 8 收口（`8f0446b`）及之后 |
| 阶段交付   | 本批提交（内容：crates/mcp-server 全新 crate 四文件 + workspace 注册 + selftest 脚本 + 两份文档；git 短哈希按仓库回填惯例由后续 docs 批次补记，先例 `05ed5ee`/`f02d9b3`） |
| 数据库迁移 | 零新增迁移（本批为只读 MCP 面，不建表不写库）。下一可用迁移号从 migrations/ 实际清单确认为 019（现存 001 到 018）。生产 platform.db 与 platform-dryrun.db 全程零写：selftest 前后逐表行数 + user_version 指纹一致（selftest 第 1/11 段实测） |
| 自动化测试 | `cargo test --offline -p mcp-server`：14 passed 0 failed（tests/mcp_protocol.rs）；`cargo test --offline --workspace`：596 passed 0 failed（Phase 8 基线 582 + 本批 14）；`python3 -m unittest discover -s workers/python/tests`：211 OK 跳过 11（本批无 workers/python 改动，入库当日复测确认无回归） |
| 静态检查   | `cargo clippy --offline --workspace --all-targets -- -D warnings`：0 error（exit 0，入库当日实测） |
| 相关       | [phase9/PHASE9-DESIGN.md](../phase9/PHASE9-DESIGN.md)（本批设计稿，含同源口径对齐点与两处已登记偏差）、docs/phase0/PHASE-ROADMAP.md §10、apps/server/selftest_phase9_mcp.sh |

## 一、交付内容

### 1. crates/mcp-server（bin rp-mcp）

- `src/catalog.rs`：工具目录读面，与 Tool Center 同源（同一 tool 表读面）。数据面守卫同 phase14_source（主库优先，缺表回退 aux，都无则真实空态说明）；列投影同 TOOL_LIST_COLUMNS 十二列；keyword/lifecycle/governance 过滤与排序（updated_at DESC, name COLLATE NOCASE ASC）同 handle_tools；aux 增量合并只补主库没有的行。两库均 SQLITE_OPEN_READ_ONLY（database::Pool readonly_file 复用），零 SQL 写。
- `src/lib.rs`：JSON-RPC 2.0 + MCP 2024-11-05 子集协议层。initialize 只暴露目录与分页发现（一行导航 instructions，95 字符，零工具全文）；tools/list 分页（不透明游标 `v1-<offset>`，limit 1 到 200 缺省 20）；tools/call 接通 rip.list_tools 与 rip.get_tool_schema，rip.run_tool / rip.search_knowledge / rip.get_run_status 为目录条目，调用返回结构化 isError（wired_in=P9-2）；resources/prompts 空占位（P9-5）；子集外方法 -32601。握手鉴权：RP_MCP_TOKEN 未配置一律 -32001；initialize 可选出示 _meta.rp.auth，出示必常时比较校验。
- `src/bin/rp-mcp.rs`：stdio 主循环（stdout 只允许 JSON 行，日志走 stderr），环境变量 RP_MCP_TOKEN / RP_DB_PATH（默认 migrations/platform.db）/ RP_AUX_DB；EOF 退出 0。
- 形态参考 integrations/rtis/rtis_mcp.mjs（NDJSON 纪律、isError 结果形态）但代码独立；双 MCP 命名空间隔离（平台 rip.*，kit redteam_*）。
- 零新增外部依赖：serde/serde_json/database 均为 workspace 既有条目，vendor/ 与 Cargo.lock 无新 package（复用优先纪律；vendor 实测无 rmcp/tokio，协议层为薄手写派发）。

### 2. 目录双段设计与 rip.* 命名空间

tools/list 组合目录 = 能力面前段 5 条（rip.list_tools / rip.get_tool_schema / rip.run_tool / rip.search_knowledge / rip.get_run_status，计划 §11:640-656 的 P9-1 子集）+ 注册面段（tool 表逐行，MCP 名 = `rip.<tool_id>`）。注册面严格 1:1 投影（不发明条目），能力面只收录既有平台能力面（tool 表 / tool_run 表 / knowledge 表）承载的子集，agents/workflows/plugins/evidence 面按批次归属 P9-2 到 P9-5。「MCP 工具面不出现 Tool Center 没有的能力」按此读法落（详见设计稿 §七）。

### 3. selftest 与登记

- apps/server/selftest_phase9_mcp.sh：23 断言，含 ask 判据三件（tools/list 注册面与 /api/tools items 逐条对账一致、无令牌握手被拒、分页遍历），另加 initialize 零全文、能力面在场、按需 Schema 与 /api/tools?tool_id= 逐字段同源、未接线 isError、生产库零写指纹。默认端口 7800（ask 点名 curl 形态），占用时有守卫提示换 SELFTEST_PORT。
- docs/phase9/PHASE9-DESIGN.md：P9-1 设计稿（协议形态、目录双段、同源对齐点与两处已登记偏差、分页、鉴权模型、验证面、已知取舍）。

## 二、验收实测（入库当日）

| 判据（ask/ROADMAP 原文） | 命令 | 结果 |
| --- | --- | --- |
| tools/list 分页目录含 rip.run_tool 与知识检索/运行查询目录面，不含工具完整说明 | `RP_MCP_TOKEN=<t> sh -c 'echo {"...tools/list..."} \| cargo run --offline -p mcp-server --bin rp-mcp'` | exit 0；目录含 rip.run_tool / rip.search_knowledge / rip.get_run_status 五能力条目 + 注册面投影；条目零 description 零 inputSchema；nextCursor=v1-20（真分页） |
| 初始化响应不含工具全文 | 同通道 initialize | protocolVersion 回显 + serverInfo rp-mcp@0.2.0 + 95 字符导航 instructions；响应无 description/inputSchema/tier 字段 |
| cargo test --offline -p mcp-server 全绿 | 同名命令 | 14 passed 0 failed；workspace 全量 596 passed 0 failed |
| tools/list 工具集与 /api/tools items 逐条对账一致（同源） | selftest 第 8 段（SELFTEST_PORT=7811，aux=dev-b5.db，22 行）+ ask 原形 curl 对活体 7800 服务（20 行） | 两口径均逐条一致（顺序 + tool_id 映射 + kind/lifecycle/governance/permission_level/source_system/updated_at 六列同值）；selftest PASS=23 FAIL=0 exit 0 |
| 无令牌握手被拒 | 无 RP_MCP_TOKEN 喂 initialize | `{"error":{"code":-32001,"message":"auth_required: RP_MCP_TOKEN 未配置，禁止匿名握手..."}}` |
| 生产库零写 | selftest 第 1/11 段指纹 | platform.db 与 platform-dryrun.db user_version + 逐表行数前后一致 |

## 三、已知取舍

- tools/list 条目偏离 MCP 2024-11-05 Tool 完整形状（无 description 无 inputSchema）：按需 Schema 是 P9-1 判据原文；严格客户端需先调 rip.get_tool_schema。
- 本机 7800 被用户常驻工作台 rp_server（PID 1173，只读主库 20 行面）占用，selftest 全量在 SELFTEST_PORT=7811 实跑；ask 原形 curl 对账按原文对该活体服务执行通过。
- rip.run_tool / rip.search_knowledge / rip.get_run_status 与全部注册面工具在 P9-1 为已通告未接线（调用返回 isError wired_in=P9-2），执行通道是 P9-2 的判据，本批不伪造。
- aux 合并对 handle_tools 有两处已登记偏差（全量 id 集合去重、aux 段同过滤），无过滤或页覆盖全量时两口径一致，登记于设计稿 §四。
- web 面零改动（未跑 vitest/视觉门，Phase 8 基线不变）；workers/python 零改动（unittest 211 OK 复测确认无回归）。

## 四、回滚方式

单提交回滚：`git revert <本批提交哈希>` 即可整体撤销（新增 crate 四文件 + workspace members 一行 + selftest + 两份文档；零迁移零生产库写，无数据面回退需求）。crates/mcp-server 目录与 apps/server/selftest_phase9_mcp.sh 为纯新增文件，Cargo.toml 仅增 workspace members 一行 `crates/mcp-server`，回滚无残留。
