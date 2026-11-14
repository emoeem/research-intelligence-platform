# 更新说明 · 2026-11-14（Phase 9 第二批 P9-2：rip.run_tool 端到端执行通道）

> Phase 9 第二批落地：平台 MCP 第一次有了真实执行通道。外部 AI（与 rp-tool CLI）现在可以用 `rip.run_tool` 把工具调用送进 Tool Engine（与 Web Workflow Tool 节点同一引擎，不建第二套派发逻辑），拿回结构化结果、Artifact 与 Evidence 引用；`rip.get_run_status` / `rip.cancel_run`（本批新增能力面条目）承载运行查询与协作式取消。执行面全部落在运行库（RP_RUN_DB，gitignore 运行时数据），生产 platform*.db 全程零写。

| 项         | 值 |
| ---        | --- |
| 状态       | 生效中（Phase 9 批次 2/4，P9-3 到 P9-5 未实施） |
| 日期       | 2026-11-14 |
| 适用版本   | Phase 9 P9-1（`e2e9786`）及之后 |
| 阶段交付   | 本批工作树（提交与 push 短哈希按仓库惯例由批次收口回填，先例 `05ed5ee`/`f02d9b3`；P9-1 的 WIP 暂存提交先例 `e2e9786`） |
| 数据库迁移 | 零新增迁移（运行库按既有 001 到 018 全量迁移；下一可用号仍为 019）。生产 platform.db 与 platform-dryrun.db 全程零写：selftest 第 1/11 段指纹前后一致 + 本批实测 `user_version=1` 与 mtime 未变（11-07 00:53 / 11-06 03:52） |
| 自动化测试 | `cargo test --offline --workspace`：603 passed 0 failed（105 套件行；Phase 9 P9-1 基线 596 + 本批 run_tool_e2e 7 用例 + mcp_protocol 调整）；`python3 -m unittest discover -s workers/python/tests`：211 OK 跳过 11（workers 零改动） |
| 静态检查   | `cargo clippy --offline --workspace --all-targets -- -D warnings`：exit 0 零告警（入库当日实测） |
| 相关       | [guides/EXTERNAL-AI-QUICKSTART.md](../guides/EXTERNAL-AI-QUICKSTART.md)（外部 AI 使用示例，计划 §14 Phase 9 第 4 条）、docs/phase9/PHASE9-DESIGN.md §十（P9-2 追加章节）、apps/server/selftest_phase9_mcp.sh |

## 一、交付内容

### 1. crates/mcp-server/src/dispatch.rs（新增，执行通道唯一实现）

- `RunChannel`：运行库路径 + 迁移目录 + Manifest 目录 + Artifact 落盘根 + 容器工作区根 + runtime_node_id + working_dir 的执行通道配置（bin 层由环境变量装配）。
- `run_tool`：MCP arguments 整形为 TOOL-CONTRACT §4.1 ToolCallRequest（run_id 本面生成，形状同 workflow exec::node_tool 的 new_id("rn")）→ `tool_engine::Engine::submit`（`crates/tool-engine/src/lib.rs:157`，D-TC-05：结构、Registry 存在性、版本解析、预算帽、input_schema、T2 authorization_ref，拒绝不落行）→ 按 Manifest runtime.type 走两条既有驱动路径：
  - python_worker：`adapter::execute_via_engine`（真实 python 子进程，rp-workflow CLI 同款路径）；
  - podman_host / podman_vm：`runtime_gateway::Gateway::dispatch`（真实 CommandProbe::podman 心跳判定，探测失败落 WAITING_FOR_RUNNER 不 FAILED，计划 §5.6:336 原文语义）+ `adapter::execute_container_via_engine`（P3-3 全链同款）。
- 终态载荷：`agent_engine::evidence::EvidenceRegistrar`（P7-2 最小闭环同一实现，本批加 `with_face`，face=rp-mcp）登记 Artifact + Evidence（SUCCEEDED 取 result，失败取 error 对象），evidence id 回填信封 evidence_refs；返回 run_id / status / result / error / artifact_id / evidence_refs / content_hash / usage / environment / transcript。
- `run_status`：运行库只读投影（run 行 + artifacts + evidence 引用 + cancel_requested_at），run db 缺失如实报错不建库。
- `cancel_run`：`Engine::request_cancel` 同一实现（D-TC-04：仅 RUNNING 与 WAITING_FOR_RUNNER 受理；QUEUED 与终态 accepted=false）。
- `tool_schema`：运行库 tool 表 Schema 读面，供 `rip.get_tool_schema` 的 run_db 回退段（目录两库没有、运行库已登记的行按同一形状返回 source=run_db，只读不发明条目）。

### 2. crates/mcp-server/src/lib.rs（协议层接线）

- 能力面 5 条扩为 6 条：`rip.cancel_run` 新增（计划 §11:651 原文条目，run/status/cancel 三面与 CLI 对称）；`rip.run_tool` / `rip.get_run_status` / `rip.cancel_run` 标记 wired 并接 RunChannel；`rip.search_knowledge` 仍未接线（结构化 isError 指明后续批次，不伪造）。
- 注册面直呼 `rip.<tool_id>` 改为定向说明（执行统一走 rip.run_tool 通道，目录名直呼不派发不伪造）。
- run_tool 入参 Schema 扩词表：tool_id / input 必填，task_id / authorization_ref / tool_version / workspace_id / budget 可选（D-TC-05 词表对齐）。
- initialize 导航 instructions 更新（点名执行与运行查询面，仍为一行、零工具全文）。

### 3. crates/tool-registry/src/bin/rp-tool.rs（CLI 执行面）

- `run --db <run_db> --tool <id> --input <json> [--task/--authorization-ref/--version/--workspace/--budget/--db-main/--manifest-dirs/--migrations-dir]`、`status --db <run_db> --run <run_id>`、`cancel --db <run_db> --run <run_id>` 三个对外子命令。
- 实现形态是最小 MCP 客户端：spawn rp-mcp 子进程（RP_MCP_BIN 可指路，缺省 target/debug/rp-mcp）做一次性 stdio JSON-RPC 交换，与外部 AI 走同一执行通道，不建第二套派发逻辑；一次性令牌经环境变量注入子进程（禁止匿名纪律由 MCP 握手鉴权承载）。validate/register/list/get 四个子命令原样保留（selftest_phase7/8 的 `-p tool-registry --bin rp-tool` 构建面不动）。

### 4. agent-engine evidence.rs（复用面微扩）

- `EvidenceRegistrar` 增加 `with_face`（captured_by 前缀与 provenance 首词）：缺省 `agent-engine` 与既有行为逐字节一致，MCP 面传 `rp-mcp`；登记逻辑与行形状零改动（复用优先，不另写第二套 INSERT）。

### 5. 测试与自验面

- `crates/mcp-server/tests/run_tool_e2e.rs`（新增 7 用例）：无令牌 run_tool 被拒（-32001）、T2 缺授权引用 POLICY_DENIED 且零 run 行（拒绝不落行）、T2 带授权引用走通真实 parsing_worker（库行对账 artifact/evidence + captured_by=rp-mcp:run_tool）、rip.doc.parse 全链（状态投影 + 终态取消拒绝 + run_db Schema 回退）、未知工具不落行、WAITING_FOR_RUNNER 取消受理 + cancel_requested_at 落行、容器工具无 podman 时 run 行在场（V-9 守卫：podman 在位环境跳过）。
- `tests/mcp_protocol.rs`（14 用例保留）：能力面六条、run face 未配置态、注册面直呼定向说明、缺 run_id -32602 等断言更新。
- `apps/server/selftest_phase9_mcp.sh`：能力面计数 5 改 6、第 10 段改为 P9-2 语义（隔离临时运行库中未登记工具如实 unknown tool 拒绝）、rp-mcp spawn 全部加 RP_RUN_DB 隔离（selftest 不写共享 rip-data 运行库）。

## 二、验收实测（入库当日，ROADMAP §10.4 验收第 1 条）

| 判据（ask/ROADMAP 原文） | 命令 | 结果 |
| --- | --- | --- |
| tools/list 分页目录含 rip.run_tool | 原形命令 + RP_MCP_TOKEN（P9-1 先例形态） | exit 0；能力面六条（新增 rip.cancel_run）+ 注册面 20 行主库投影；条目零 description 零 inputSchema；nextCursor=v1-20 |
| run_tool rip.http.inspect 返回结构化结果 | 原形命令 | run_id=rn-d3f30013acf2e0dd00000000；**本机无 podman**，Gateway 心跳探针失败 → status=WAITING_FOR_RUNNER（isError=false，不伪造结果），run 行在库（见下行对账）。容器全链（结构化结果 + body.bin Artifact）在 podman 在位环境由 web_research_e2e 承载 |
| 结构化结果 + artifact_id + evidence 引用（可实跑路径） | 同通道 rip.doc.parse（python_worker，纯本地解析）+ rtk.mcp 客户端八步 | run_id=rn-e99d6e50db6ec23600000000，SUCCEEDED，transcript QUEUED→PREPARING→RUNNING→SUCCEEDED，artifact_id=art-4e92987e4d7b86af31bc5432，evidence_refs=[ev-865c85279b36a17edc674527]；另一实跑（正文三节的 rn-10a9993a4107628300000000）同形状 |
| Run 在 tool_run 表可见（只读对账） | python3 -I sqlite3 URI mode=ro | tool_run 三行（WAITING_FOR_RUNNER / FAILED / SUCCEEDED 各一）、artifact 两行、evidence 两行（captured_by=rp-mcp:run_tool，source_pk=run_id）与 MCP 载荷逐条对上；artifact 落盘文件 sha256 = 行内 sha256 = 载荷 content_hash |
| rp-tool run --help 帮助面可读 | `./target/debug/rp-tool run --help` | 八行用法（run/status/cancel 全旗标）输出，exit 2 |
| run_tool_e2e 断言无令牌被拒 + T2 authorization_ref 保留 | `cargo test --offline -p mcp-server --test run_tool_e2e` | 7 passed 0 failed（含 -32001 与 POLICY_DENIED 零 run 行断言） |
| 生产库零写 | selftest 第 1/11 段指纹 + 手核 user_version/mtime | platform.db 与 platform-dryrun.db 前后一致（user_version=1，mtime 未变） |

## 三、已知取舍与机器差异

1. **容器工具在本机（无 podman）不能执行到底**：`rip.http.inspect` 调用链完整走到 WAITING_FOR_RUNNER（run 行在库，这是 §5.6:336 的正确语义而非失败），结构化结果须等容器 Runner。验收判据的「结构化结果 + artifact_id + evidence 引用」由 python_worker 工具实跑承载（引擎链完全同一条）。本机 2026-11-14 实测：podman 与 docker 均不在 PATH。
2. **rip.http.tls_inspect 存在 Phase 6 期遗留的登记件与 worker 不一致**：manifest output_schema 的 `error` 字段声明 `type: string`（描述却写「无失败为 null」），worker 成功路径回 `error: null`，经引擎 output_schema 强校验即 FAILED（EXECUTION，-32006）。该工具 P6 只做过 register + ping 未做过引擎执行，本批经执行通道首次暴露。属 Phase 6 样例工具缺陷，本批不改 workers/ 与其登记件（零改动红线），仅如实记录待 Phase 6 责任批次修复。
3. **CLI 执行面依赖 rp-mcp 二进制**：rp-tool run/status/cancel 是最小 MCP 客户端，rp-mcp 未构建时报 mcp_bin_missing 并给构建命令。这是 rp-tool 所在 crate（tool-registry）不能反向依赖 tool-engine（tool-engine 依赖 tool-registry，cargo 禁环）约束下的单一通道方案：CLI 与 MCP 共用 dispatch.rs 一个实现。
4. **运行库是执行面的独立存储**：目录读面（主库/aux）与运行库（RP_RUN_DB）分离，`get_tool_schema` 做了 run_db 回退段保证「目录没有、运行库有」的工具可按需取 Schema；tools/list 注册面仍只投影目录两库（Tool Center 同源口径不变）。外部 AI 完整走法见 quickstart 第三节。
5. **环境指纹 runtime_node_id 显示 host-cachyos**：python_worker 路径的 environment.runtime_node_id 由 tool-engine adapter 现有代码写入（常量），本批未改 tool-engine 既有行为；容器路径用 RP_RUNTIME_NODE_ID（缺省 host-local）。
6. web 面零改动（vitest/视觉门未跑，Phase 8 基线不变）；REST/CLI 同源（P9-3）本批未动。

## 四、回滚方式

单提交回滚：`git revert <本批提交哈希>` 整体撤销（dispatch.rs 与 run_tool_e2e.rs 为纯新增文件，lib.rs / rp-mcp.rs / rp-tool.rs / selftest / Cargo.toml / agent-engine evidence.rs 为小改）。运行库（rip-data/tool-runs.db）与 Artifact 落盘均为 gitignore 运行时数据，回滚无残留、无迁移回退需求；生产 platform*.db 本批零写。
