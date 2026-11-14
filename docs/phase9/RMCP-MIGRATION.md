# rmcp 迁移整改（Phase 9 阻断性兼容性整改批次，2026-11-14）

> 本文件是审计 [MCP_MATURE_LIBRARY_AUDIT_2026-11-14.md](MCP_MATURE_LIBRARY_AUDIT_2026-11-14.md)
> §5 十项整改任务的选型记录与执行台账：手写 MCP 2024-11-05 子集协议层退役，
> 协议层（JSON-RPC 2.0 解析、initialize 生命周期、版本协商、错误信封、stdio
> I/O）交给 MCP 官方 Rust SDK rmcp，RIP 业务 handler（目录 / 执行 / 资源 /
> prompt）作为业务面接入 SDK。含审计 §6.6 要求的成熟库七小节。

| 项       | 值                                                             |
| -------- | --------------------------------------------------------------- |
| 状态     | 已实施（等待整批离线验收合入）                                   |
| 日期     | 2026-11-14                                                       |
| 采用库   | rmcp 3.10.0（crates.io 发布版，精确钉定 `=3.10.0`）               |
| feature  | `default-features = false, features = ["server", "transport-io"]` |
| 基线     | `ae73b92`（审计入库提交），实施在 main 工作树                     |
| 相关     | crates/mcp-server/、crates/rp-mcp-parity/、apps/server/selftest_phase9_mcp.sh、docs/phase9/rmcp-client-e2e/ |

## 一、成熟方案检索结果

- MCP 官方组织维护 Rust SDK：github.com/modelcontextprotocol/rust-sdk，crate
  名 `rmcp`，docs.rs/rmcp 与 rust.sdk.modelcontextprotocol.io 为官方文档面；
  本批经 crates.io API 实查：`rmcp` 最新稳定版 3.10.0（2026-11-12 发布），
  官方 SDK 持续活跃（3.1.4 至 3.10.0 在 2026-08 到 2026-11 间 12 个稳定版）。
- MCP 官方另有 Python / TypeScript SDK；RIP 服务面是 Rust，官方 Rust SDK 是
  唯一官方对口实现。
- 仓库内既有实现：手写 JSON-RPC 2.0 / MCP 子集协议层（原
  crates/mcp-server/src/lib.rs 协议段），审计判定不可长期保留（P0/P1 两条
  协议一致性问题 + 协议维护职责错位）；integrations/rtis/rtis_mcp.mjs 是
  kit 层接入面（redteam_* 命名空间），不是平台协议层，不在本批范围
  （红线：integrations/rtis/ 零修改，实测未触碰）。

## 二、官方 SDK / 库候选比较

| 候选                                   | 结论   | 理由（可复核）                                                         |
| -------------------------------------- | ------ | ---------------------------------------------------------------------- |
| rmcp 3.10.0（官方 Rust SDK，crates.io） | 采用   | 官方维护、协议版本面覆盖 2024-11-05 至 2025-11-25（vendor 源码 model.rs V_2024_11_05 至 V_2026_07_28 常量与协商实现 service/server.rs negotiate_protocol_version）、server + stdio 面可按 feature 最小化 |
| rmcp git main                          | 拒绝   | 审计明确「不直接跟踪不稳定 Git main」；crates.io 发布版已满足           |
| 继续手写协议层                          | 拒绝   | 审计 P0（tools/list 缺必需 inputSchema 被测试固化）/ P1（initialize 回显任意客户端版本无协商）均为手写层的结构性问题；协议兼容与安全边界职责不应由业务 crate 长期承担（例外 ADR 未获批，见 §六） |
| 轻量自研（继续子集化协议）              | 拒绝   | 同上；且 2025-03-26 / 2025-06-18 / 2025-11-25 三个协议版本的线形差异（_meta、outputSchema、SEP 编号）自研覆盖成本高于 SDK 接入 |
| 其他社区 MCP Rust 实现                  | 未采用 | 官方 SDK 存在即优先官方（成熟库优先纪律的第一候选面）                    |

## 三、采用库与 feature 选择

- 版本钉定：`rmcp = { version = "=3.10.0", default-features = false,
  features = ["server", "transport-io"] }`（workspace Cargo.toml + crates/
  mcp-server/Cargo.toml）。版本号经实际构建与测试验证（本批全部门禁跑在
  3.10.0 上），不是文档转抄。
- feature 最小集（vendor/rmcp-3.10.0/Cargo.toml 实查）：
  - `server` = transport-async-rw + schemars + pastey + uuid：server 面必需
    （schemars 为 server feature 的硬性传递要求，无法再裁剪）；
  - `transport-io` = transport-async-rw + tokio/io-std + tokio/net：stdio
    transport 必需；
  - `default-features = false` 显式排除 base64 与 macros（rmcp-macros 工具
    宏面，本仓 ServerHandler 为手写 trait impl 不用宏）；
  - 未启用：client、auth（jsonwebtoken / oauth2 / reqwest）、elicitation、
    transport-child-process、transport-sse-*、transport-streamable-http-*、
    skills-fs、local（审计红线：不引入 HTTP/OAuth/client/elicitation）。
- tokio：rmcp 运行时面直依赖（bin rp-mcp 的 async main 与 stdio 拉起），
  workspace 锁 1.x 最小 feature 子集（rt / rt-multi-thread / macros /
  io-std / io-util / sync / time）。
- MSRV：rmcp 3.10.0 rust_version 1.88（crates.io 版本元数据实查）；本仓
  workspace rust-version 1.99（本机 rustc 1.99.1 实测），满足。
- 许可证：rmcp Apache-2.0（crates.io 元数据）；传递依赖许可文件随 vendor
  目录入库（cargo vendor 保留 LICENSE-*），本仓 Apache/MIT 双许可兼容面。

## 四、vendor / 离线构建方案

- 受控联网口径（红线：仅 crates.io 与 rsproxy 拉依赖）：
  1. 临时把 .cargo/config.toml 从 vendored-sources 切到 rsproxy sparse
     镜像（备份原件）；
  2. 加入 rmcp / tokio 依赖后 `cargo fetch` 解析并下载；
  3. `cargo vendor vendor` 增量落库；
  4. 恢复 .cargo/config.toml 原 vendored 口径（逐字节还原）；
  5. 此后全部构建 / 测试 / 门禁 `--offline` 复跑，运行时零联网。
- Cargo.lock 差异（git show HEAD:Cargo.lock 逐包比对实测）：**纯增量**。
  新增 43 个外部 crate（tokio 1.54.1、tokio-util 0.7.20、futures 0.3.34 族、
  chrono 0.4.45、schemars 1.2.3、uuid 1.29.2、tracing 0.1.44、thiserror
  2.0.21、sha2 0.11.1 族、bytes 1.12.1、mio 1.2.5、socket2 0.6.5、pastey
  0.2.3、getrandom 0.4.3 等），既有 crate 零升级零移除；同名 crate 双版本
  共存（block-buffer 0.10.4+0.12.1、digest 0.10.7+0.11.3、sha2 0.10.9+
  0.11.1、getrandom 0.3.4+0.4.3、cpufeatures 0.2.17+0.3.1、crypto-common
  0.1.7+0.2.2、r-efi 5.3.0+6.0.0），vendor 目录以 `vendor/<name>-<旧版本>`
  承载旧版（实测 vendor/ 目录清单）。vendor 目录 114 -> 164 个 crate 目录。
- 离线复现：恢复 vendored 口径后 `cargo build --offline -p mcp-server` 通过；
  整批验收要求 `cargo clean` 后干净状态 `cargo test --offline --workspace`
  全绿（验收数见 §七）。

## 五、自研业务层边界（业务 handler 清单）

协议层退役（原 lib.rs 手写段全部删除）：JSON-RPC 2.0 消息解析、方法派发、
result/error 信封、通知静默、initialize 回显、常时令牌比较
（constant_time_eq）、`_meta.rp.auth` 出示校验、行写入器（write_line /
LineOutcome）。协议职责现由 rmcp 承载（vendor/rmcp-3.10.0/src/service/
server.rs 的握手状态机与 negotiate_protocol_version 实读为证）。

保留并接线的业务面（RIP 自有，零第二套派发 / 注册表）：

| 业务面       | 位置                        | 说明                                                                     |
| ------------ | --------------------------- | ------------------------------------------------------------------------ |
| 目录读面     | crates/mcp-server/src/catalog.rs | Tool Center 同一 tool 表读面（主库优先 aux 回退、handle_tools 排序、十二列投影、只读 SQLITE_OPEN_READ_ONLY 零 SQL 写）；本批新增 input_schema 列直读（list_registry_with_schema）供 P0 标准 Tool 对象 |
| 执行通道     | crates/mcp-server/src/dispatch.rs | rip.run_tool 经 tool-engine Engine 受理（D-TC-05）与既有适配器派发（python_worker execute_via_engine / podman Gateway::dispatch），EvidenceRegistrar（P7-2 同一实现）登记；零改动 |
| 资源面       | crates/mcp-server/src/resources.rs | rip:// 七 resource（静态四 + 模板三），零改动                            |
| prompt 面    | crates/mcp-server/src/prompts.rs  | prompt 三条；本批仅更正剧本第 1 步文本（tools/list 现含 inputSchema）     |
| SDK 接入层   | crates/mcp-server/src/service.rs  | 新增：`impl rmcp::ServerHandler for McpServer`（tools/list / tools/call / resources / prompts 五面映射 + 错误码映射 -32602/-32603/-32601）；阻塞面（sqlite 与工具执行）经 spawn_blocking 隔离 async 运行时（协作式取消 D-TC-04 执行期仍可受理） |
| 进程入口     | crates/mcp-server/src/bin/rp-mcp.rs | 新写：RP_MCP_TOKEN fail-closed 门 + 环境变量装配 + rmcp serve（transport stdio）+ EOF 收尾 |

连带修正（非红线 crate）：crates/tool-registry/src/bin/rp-tool.rs（P9-2 最小
MCP 客户端）补 initialize 握手（id=0）+ notifications/initialized 前置，
因为 rmcp 服务首条请求必须为 initialize（vendor 源码
serve_server_with_ct_inner 对非 initialize 首请求回
ExpectedInitializeError 断连）；CLI 输出形状不变（仍取 id=1 业务响应）。

## 六、例外 ADR

无。本批采用成熟库，不触发「拒绝成熟库需例外 ADR」条款；手写协议层按审计
结论退役，而非保留。

## 七、兼容性、性能、安全和回归验收

### 协议行为变更清单（新协议行为，测试与文档同步）

1. **P0 tools/list 标准 Tool 对象**：每条含必需 `name` 与 `inputSchema`
   （注册面 input_schema 列直读，列 NULL / 坏 JSON / 非 object 回退宽松
   `{"type":"object","properties":{}}`，真实校验仍在执行期由 tool-engine 按
   Manifest 承担）；分页只控量不省必需 Schema；旧测试
   `tools_list_entries_are_summary_projection_without_full_text`（断言
   inputSchema 不得出现）改写为标准一致性断言
   `tools_list_entries_are_standard_tool_objects`。
2. **P0 元数据承载面迁移**：标准 Tool 对象不再携带平台 tier/tool_id/kind 等
   摘要列（审计「平台内部状态不污染标准 Tool schema」）；对账面移到
   rip.list_tools（handle_tools 口径信封，十二列与 REST /api/tools 同源），
   selftest §8 与 parity op1 同步改写。
3. **P1 版本协商**：rmcp 承载，支持列表 2024-11-05 / 2025-03-26 /
   2025-06-18 / 2025-11-25（2026-07-28 起 initialize 握手被 per-request
   _meta 取代（SEP-2567），无握手可答，不在列表）；列表内协商回显，列表外
   回落服务端声明版本 2025-11-25（vendor negotiate_protocol_version 实读）。
4. **tools/list 标准分页**：唯一入参 cursor（MCP 2024-11-05 标准形状），
   页大小服务端固定 20；keyword/lifecycle/governance 过滤与 limit 参数移到
   rip.list_tools（MCP 标准分页无过滤参数；过滤能力不丢失，承载面更标准）。
5. **RP_MCP_TOKEN 安全边界重定义**（审计 §4.3）：stdio 子进程的环境变量
   注入即授权边界（宿主 spawn 时注入非空令牌 = 授权本实例对外服务）；缺失 /
   空白令牌 fail-closed 拒启（exit 2，不进入服务循环）；`_meta.rp.auth`
   自定义出示校验退役（不发明 MCP 标准外认证字段）。三态实测：缺失 -> exit
   2（mcp_protocol.rs::token_boundary_three_states + selftest §4）、空白 ->
   exit 2、正确注入 -> 全链服务。
6. **坏 JSON 行静默跳过保留**：rmcp transport 对不可解析行不回 -32700
   （vendor 自带测试 receive_ignores_parse_error，issue #938），与旧
   rtis_mcp.mjs 纪律一致（conformance 断言以 SDK 实测行为为准，不固化手写
   期望）。
7. stdout 只出协议消息 / 日志走 stderr 纪律不变（rmcp transport 承担
   stdout；e2e 第 9 步实测 stdout 9 行全 JSON、stderr 含 ready 零 JSON 泄漏）。

### 实测验收（本机实跑，受控环境；数字与命令一一对应）

| 门禁 | 命令 | 结果 |
| ---- | ---- | ---- |
| 干净态全量测试 | `cargo clean` 后 `cargo test --offline --workspace` | **605 passed / 0 failed**（108 个测试段；与基线 608 的差 = mcp-server 测试改写后用例数变化，逐段全绿） |
| crate 测试 | `cargo test --offline -p mcp-server` | mcp_protocol 11 passed + run_tool_e2e 7 passed（真实 python worker 全链） |
| 一致性载具 | `cargo test --offline -p rp-mcp-parity` | 5 passed（op1/op2/op3/op4/op5 全过，spawn 真实 rp-mcp） |
| 静态检查 | `cargo clippy --offline --workspace --all-targets -- -D warnings` | 0 error 0 warning（needless_borrow 10 处与 doc 缩进 1 处为批次内修复项） |
| Python worker 门禁 | `workers/python` 下 `python3 -m unittest discover -s tests` | 211 tests OK（skipped=11），与 Phase 9 基线一致（Rust 侧迁移零影响） |
| 自验脚本 | `SELFTEST_PORT=7832 / 7833 bash apps/server/selftest_phase9_mcp.sh` | PASS=26 FAIL=0，ALL PASS（两轮；7800 被其他进程占用按脚本既有守卫换端口） |
| 真实客户端 e2e | `node docs/phase9/rmcp-client-e2e/rmcp_client_e2e.mjs` | 10 步全过 exit 0（留证见 rmcp-client-e2e/RESULT.md） |
| rp-tool 客户端全链 | `rp-tool register` + `rp-tool run --tool rip.doc.parse`（临时运行库） | run SUCCEEDED + run_id + evidence_refs（补握手后 CLI 输出形状不变） |
| 生产库零写 | selftest §1/§11 URI mode=ro 指纹（user_version + 逐表行数） | platform.db 与 platform-dryrun.db 前后指纹一致 |
| 离线构建 | `cargo build --offline -p mcp-server`（vendored 口径恢复后） | 通过 |

### 安全公告核对（OSV 版本级实查，2026-11-14）

对锁定版本逐一查询 api.osv.dev：rmcp 3.10.0、tokio 1.54.1、tokio-util
0.7.20、tracing 0.1.44、sha2 0.11.1、chrono 0.4.45、futures 0.3.34、uuid
1.29.2、schemars 1.2.3、thiserror 2.0.21、bytes 1.12.1、mio 1.2.5、
getrandom 0.4.3、serde 1.0.229、serde_json 1.0.154、libc 0.2.182、indexmap
2.14.2 均 **0 条已知通告**。历史通告（tokio RUSTSEC-2021-0072 /
2021-0124 / 2023-0001 / 2023-0005 / 2025-0023 / 2026-0384 / 2026-0386、
tracing RUSTSEC-2023-0078、sha2 RUSTSEC-2021-0100、chrono
RUSTSEC-2020-0159）只影响旧版本，锁定版本均在修复线之后（OSV 版本级查询
返回空为证）。

### 已知取舍

- schemars / uuid / chrono 为 rmcp server feature 的硬性传递依赖，无法再
  裁剪；vendor 体积代价实测入库（本批 vendor 增量约 +50 crate 目录）。
- tools/list 不再接受非标准过滤参数（标准面收敛）；依赖旧行为的调用方改走
  rip.list_tools（词表不变）。
- 协议版本面从「只声明 2024-11-05」扩为「2024-11-05 至 2025-11-25 协商」，
  服务端业务面仍是原子集（tools/resources/prompts，无 elicitation /
  completion / logging / sampling / tasks 能力声明）。
- 本机 7800 端口被其他进程占用时 selftest 需 `SELFTEST_PORT=<p>` 重跑
  （脚本既有守卫，本批实测触发一次，未改脚本语义）。
