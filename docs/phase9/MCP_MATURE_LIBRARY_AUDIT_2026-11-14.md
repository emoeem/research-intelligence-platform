# RIP 成熟库优先硬约束与 Phase 9 MCP 实现审计报告

**日期：** 2026-11-14  
**审计对象：** `emoeem/research-intelligence-platform` 公开仓库 `main` 分支可见代码  
**审计性质：** 只读代码审阅 + 官方 MCP Rust SDK / 规范核对  
**本轮未执行：** 代码修改、数据库操作、Git 写入、提交或推送

---

## 1. 结论摘要

1. RIP 2.0 的整体推进方向和现有工程纪律仍然成立。用户提供的运行报告显示 Phase 7、Phase 8 已完成，Phase 9 批次 4/N 正在执行，Phase 10 和收尾闭环尚未开始。该进度数字来自运行报告，并非本次对本机活跃工作树的独立读取。
2. **MCP 当前手写协议层不应作为长期实现保留。建议在当前 Phase 9 批次 4 完成后，安排独立的 rmcp 迁移/兼容性整改批次。** 不应在活跃批次中途改动脚本或协议实现，也不应把暂缓中途修改解释为接受手写协议层。
3. MCP 当前实现有已核实的协议一致性问题：`tools/list` 返回的工具对象没有标准必需的 `inputSchema`；initialize 直接回显客户端请求的 `protocolVersion`，未执行版本协商；现有测试还明确断言 `inputSchema` 不得出现，固化了错误行为。
4. 当前仓库离线 vendor 纪律属实：`.cargo/config.toml` 将 crates.io 替换为 `vendor/`，而公开 `main` 的 `Cargo.lock` 和 `vendor/` 均没有 `rmcp`、`tokio` 等相关 crate。实施员识别到依赖不在 vendor 是正确的事实判断；把它当成永久拒绝成熟 SDK 的理由则不符合成熟库优先原则。
5. 从现在起，**协议、标准格式、解析、序列化、认证、并发、存储、压缩等通用基础能力默认必须先评估成熟维护库；手写实现必须先有书面例外决策。** “vendor 里没有”“会增加依赖”“离线构建暂时失败”本身不构成拒绝成熟库的充分理由。

## 2. RIP 2.0 当前进度：信息来源分层

### 2.1 用户提供的运行报告

| 阶段 | 报告状态 | 报告中的关键证据 |
|---|---|---|
| Phase 0–6 + SK 扩展 | 完成 | 历史提交 `c9e7b23`、`ace3c53`、`2bf8e9c` |
| Phase 7 Agent Engine | 完成 | `102cab5`；cargo 510 passed；Trace 六 ID、能力契约 Schema 化、浏览器实测 |
| Phase 8 Workflow Studio | 完成 | `8f0446b`；cargo 582 passed；Vitest 111；016–018 迁移；画布真实浏览器验证 |
| CI 硬门槛 | 全绿 | 报告称仓库史上首次全绿，#53，Rust/Python/Web 三 job |
| Phase 9 MCP/REST/CLI | 进行中 | 批次 4/N；批次 1–3 已落地，批次 4 活跃 |
| Phase 10 生态与发布 | 待开始 | Phase 9 洞口和 CI 绿门槛通过后放行 |
| 收尾闭环 | 待开始 | ChatGPT 审阅、GitHub 知识库补齐、实战校验 |

报告还称当前工作流 55 站中到第 16 站、58/59 步零失败、累计约 302M token，批次 4 的多个文件仍有活跃写入。这些是用户转交的运行快照，不代表本报告通过远程 GitHub 能独立确认本机未提交状态。

### 2.2 本次独立核验的远端仓库状态

本次通过 GitHub 只读接口核对到：

- 仓库：`emoeem/research-intelligence-platform`
- 默认分支：`main`
- 本次读取时远端 HEAD：`e2e97861949a8e23dd8aba49d1195f05184b01fc`
- 该提交为 Phase 9 批次 1 的 WIP 备份，提交信息称“新 crate 起步与 workspace 注册”。
- 公开 `main` 中已经存在 `crates/mcp-server/`，包含 `Cargo.toml`、`src/lib.rs`、`src/catalog.rs`、`src/bin/rp-mcp.rs` 和 `tests/mcp_protocol.rs`。

**重要范围限制：** 用户报告的批次 4 活跃工作树可能尚未推送，因此本次只能审阅远端 `main` 可见的 MCP 代码，不能声称已经审阅批次 4 本机未提交文件。批次 4 完成后必须对最终合并树重新执行本报告第 4 节的核查。

## 3. MCP 官方 SDK 与依赖现状

### 3.1 官方 SDK 存在且适用于当前场景

MCP 官方组织维护 Rust SDK，核心 crate 名为 `rmcp`。本次查询时官方文档显示 `rmcp 3.10.0`，SDK 提供 server 能力和 stdio transport；官方仓库说明 3.x 实现当前稳定协议，并兼容 2025-11-25 及更早版本。RIP 当前使用的 2024-11-05 属于其兼容范围。

- 官方仓库：<https://github.com/modelcontextprotocol/rust-sdk>
- 官方 crate 文档：<https://docs.rs/rmcp/latest/rmcp/>
- 官方 SDK 首页：<https://rust.sdk.modelcontextprotocol.io/>

### 3.2 当前依赖理由的事实部分

RIP 当前 `.cargo/config.toml` 配置：

```toml
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor"
```

公开 `main` 的 `Cargo.lock` 中，`mcp-server` 依赖仅有 `database`、`serde`、`serde_json`；没有 `rmcp` 或 `tokio`。公开 `vendor/` 也没有 `rmcp`、`tokio`、`tokio-util`、`futures`、`tracing` 等主要依赖。

所以“目前不能直接把 rmcp 加进依赖后仍期待现有 `cargo --offline` 立即成功”是正确的。但工程上正确的解决方案是受控获取依赖、锁定版本、增量 vendor，再恢复离线构建，而不是手写协议层替代 SDK。

### 3.3 推荐依赖策略

- 以官方发布的 `rmcp` 为首选，不直接跟踪不稳定 Git `main`。
- 对照项目 Rust toolchain/MSRV、目标平台、许可证、依赖树和安全公告后，选择并精确锁定版本；当前可把 3.10.0 作为评估基线，不应在未构建验证前将版本号视为无条件结论。
- 只启用 server + stdio 所需 feature；不要引入本项目暂时不用的 HTTP、OAuth、client、elicitation 等可选功能。
- 优先使用 `rmcp` 自带 stdio transport，不再自写 JSON-RPC/MCP 行协议、initialize 生命周期、版本协商、协议错误封装。
- 受控联网完成 `cargo fetch`/依赖审查/`cargo vendor`，更新 `Cargo.lock`、`vendor/`、许可证和依赖清单；之后验证 `cargo test --offline`、`cargo clippy --offline -- -D warnings`、容器构建及 CI 离线构建。

## 4. 当前 MCP 实现审计结果

### 4.1 已确认的问题

#### P0：`tools/list` 缺少必需的 `inputSchema`

远端 `main` 的 `crates/mcp-server/src/lib.rs` 中 `CatalogEntry` 结构只包含 `name`、`tier`、`tool_id`、`kind`、`lifecycle`、`governance`、`permission_level`、`source_system`、`updated_at` 等字段，没有 MCP 标准 Tool 对象要求的 `inputSchema`。

MCP 2024-11-05 的 Tool 定义中，`name` 和 `inputSchema` 是必需字段；`description` 是可选字段。因此：

- 初始化响应不携带工具全文可以继续保留；
- `tools/list` 必须返回符合 MCP schema 的 Tool 对象，包含 `name` 和 `inputSchema`；
- `description` 可以精简或省略；
- 分页负责控制一次发现返回的工具数量，不能用省略必需 Schema 的方式节省 Token；
- `rip.get_tool_schema` 可以保留为平台自定义的按需详情工具，但不能替代标准 `tools/list` 的 `inputSchema`。

当前 `tests/mcp_protocol.rs` 中 `tools_list_entries_are_summary_projection_without_full_text` 还断言每个条目都不得包含 `inputSchema`。这条测试应在迁移时改为标准一致性断言，而不是保留原测试预期。

#### P1：initialize 协议版本直接回显

`initialize_reply` 直接将客户端传入的 `protocolVersion` 写入结果，没有检查服务端实际支持哪些协议版本。版本协商应由 SDK 处理：支持客户端版本时按规范协商；不支持时按规范返回错误或服务端支持版本，不得假装支持任意客户端版本。

#### P1：自定义协议层承担了不该自己维护的职责

当前 `lib.rs` 手写 JSON-RPC 2.0/MCP 子集的消息解析、方法派发、响应 envelope、分页、错误码、通知静默处理、版本响应和常时令牌比较。即使功能范围小，也已经承担协议兼容性与安全边界维护责任。

这类基础协议代码属于成熟库优先规则的典型应用场景。产品专属的 Catalog 查询、Tool Center 映射、Engine/Gateway 派发应由 RIP 自己维护；标准协议行为则应交给 `rmcp`。

### 4.2 已经做对、迁移时必须保留的设计

- MCP 目录与 Tool Center 复用同一 `tool` 表读面，而不是另造工具清单。
- 主库优先、aux 增量补充、按 `tool_id` 去重等平台目录语义应保留。
- `catalog.rs` 经 `database::Pool` 只读打开数据库，不直接越过数据库抽象，不对生产库执行 SQL 写入。
- 工具实际执行应继续经过现有 Engine/Gateway，不在 MCP crate 内实现第二套工具派发。
- stdout 只输出协议消息、日志走 stderr 的 stdio 纪律应保留；迁移后由 SDK transport 承担协议 I/O。
- `rip.list_tools`、`rip.get_tool_schema` 等平台附加能力可以继续存在，但必须符合标准 Tool 描述和调用语义。
- 现有测试中对目录排序、过滤、主辅库合并、Schema 来源、未接线工具不伪造成功等业务行为的测试应迁移保留。

### 4.3 认证语义需要在迁移批次明确

当前实现要求进程环境变量 `RP_MCP_TOKEN` 必须存在；但当该变量存在时，`initialize` 没有强制要求客户端出示 `_meta.rp.auth`，只有客户端主动出示时才比较。这更接近“宿主进程必须配置令牌 + 可选校验自定义元数据”，不能简单描述为“客户端握手必需令牌认证”。

迁移时必须先明确安全边界：

- 若 stdio 子进程启动和环境变量注入本身就是授权边界，应明确写入部署文档，不要把它描述成客户端协议握手认证。
- 若要求客户端令牌认证，则必须设计与 MCP SDK 生命周期集成的明确校验方式，并测试缺失、错误、正确令牌；不能依靠可选自定义字段形成含糊策略。
- 不要自行发明 MCP 标准未定义的认证字段来替代适用的宿主/传输层安全机制。

## 5. MCP 整改批次计划

**时机：** Phase 9 当前批次 4 完成后，安排独立小批次；不在活跃批次中途修改脚本、Cargo 依赖或协议源文件。若批次 4 完成后 Phase 9 尚未最终验收，应优先在 Phase 9 RELEASE 前完成本批次；若确实影响既定封版节奏，应把它作为 Phase 9 后的明确阻断性兼容性整改，不得悄悄忽略。

### 实施任务

1. 先对最终工作树复核 MCP crate、Cargo.lock、vendor、测试和 Phase 9 设计文档；记录最终 HEAD 和工作树状态。
2. 形成 rmcp 选型记录：版本、MSRV、许可证、feature、传输支持、依赖树、维护状态、安全公告、与 2024-11-05/当前支持协议版本兼容情况。
3. 在隔离分支或受控批次内增量 vendor `rmcp` 最小依赖集合，保持从此可离线重建。
4. 将 `rp-mcp` 改为 rmcp server + stdio transport；RIP 的工具目录和调用处理器作为业务 handler 接入 SDK。
5. 将现有目录投影转换成标准 Tool 对象：每个 Tool 都有 `name` 与合法 `inputSchema`；description 可简洁；平台内部状态不应污染标准 Tool schema，可通过 RIP 自定义查询工具获取详细元数据。
6. 保留 Tool Center 同源、数据库只读、Engine/Gateway 单一派发、EvidenceRegistrar 原有实现和权限校验。
7. 重新定义 `RP_MCP_TOKEN` 安全边界与测试，不把宿主环境配置误称为协议认证。
8. 用真实 MCP 客户端做 stdio 端到端验收：initialize、版本协商、notifications/initialized、tools/list 多页、inputSchema 校验、tools/call 成功/失败、未知工具、通知与超时处理、stderr/stdout 分离、进程退出。
9. 增加协议 conformance 测试，覆盖 2024-11-05 和 SDK 实际支持的协议版本；测试不能通过手写期望固定非标准行为。
10. 完成离线验收：`cargo test --offline --workspace`、clippy、容器构建、Phase 9 MCP 自测脚本、实际客户端截图/日志与生产数据库零写指纹。

### 验收标准

- 协议解析、initialize 生命周期、版本协商、工具列表响应、协议错误和 stdio transport 由 rmcp 负责。
- 所有 `tools/list` 条目都符合 MCP Tool schema，具备必需 `inputSchema`。
- 不出现第二套 Tool Registry、第二套工具执行派发或生产数据库写入。
- 官方 SDK 与所有传递依赖已锁定并 vendor；全新环境能在断网状态通过要求的离线构建与测试。
- 至少一个独立真实 MCP 客户端完成 initialize → tools/list → tools/call 全链路验证。
- 安全、错误、兼容性和目录业务测试全部通过；旧测试中不符合规范的断言必须移除或改写。

## 6. 成熟库优先：项目级硬约束（可直接并入 AGENTS.md）

以下内容应作为所有规划员、实施员、审阅员和自动化工作流的强制规则。除非用户明确授权例外，不得绕过。

### 6.1 默认规则

**通用基础能力必须先找成熟库，不得默认手写。** 对协议、标准格式、解析器、序列化/反序列化、Schema 校验、认证、加密、并发、异步运行时、网络传输、数据库、索引、压缩、归档、文件类型识别、图像/PDF/OCR、Markdown/HTML/Office 解析等领域，实施前必须检索并评估官方 SDK、官方参考实现或成熟维护库。

产品差异化业务逻辑可以自研；协议和基础设施的通用实现优先交给成熟库。

### 6.2 先评估再实现

开始自研之前，必须至少核查：

1. 是否存在官方 SDK、官方 crate/package 或标准组织维护的参考实现；
2. 是否仍在维护：最近发布、提交活跃度、维护者和 issue/安全响应情况；
3. 是否覆盖目标协议版本、数据格式、平台和边界场景；
4. 许可证是否可接受，依赖树、MSRV、目标平台和构建方式是否兼容；
5. 安全公告、已知漏洞、供应链风险和可审计性；
6. 性能、内存、二进制体积、启动时间和实际功能需求；
7. vendor/离线构建、镜像部署和升级策略；
8. 与当前架构边界的集成成本，以及是否能通过 Adapter/trait 局部接入。

评估结论必须写入 ADR 或当前阶段设计文档，并给出可复核的仓库/文档链接、版本号、feature 和测试证据。不能只写“太重”“不想增加依赖”“目前 vendor 没有”。

### 6.3 离线 vendor 不能成为拒绝成熟库的挡箭牌

当成熟库未进入 vendor 时，必须评估受控引入方案，而不是直接改为手写实现：

1. 在独立依赖准备任务中允许受控联网；不得影响活跃阶段的工作树或离线运行批次。
2. 选择明确版本并检查许可证、依赖树、安全公告、MSRV 和 feature。
3. 更新 Cargo.lock/等价锁文件，增量执行 cargo vendor 或项目规定的 vendoring 工具。
4. 将依赖源、校验和、许可证、NOTICE、SBOM/依赖清单按项目规范入库。
5. 恢复 crates.io source replacement 和离线模式，执行干净环境 `--offline` 构建、测试和容器构建。
6. 后续 CI 和发布必须可离线、可复现；不允许运行时偷偷联网下载依赖。

**以下理由单独出现时均不足以否决成熟库：** “vendor 中没有”“新增若干传递依赖”“需要 Tokio/async runtime”“增加仓库体积”“需要一次受控联网下载”“SDK 默认 feature 太多”。必须先做最小 feature 和依赖树分析。

### 6.4 手写实现必须通过例外审批

只有在评估后确认无合适成熟库，或成熟库无法满足关键约束且适配成本明显高于有限自研时，才允许自研通用基础组件。必须先提交 ADR，至少记录：

- 搜索过的官方 SDK 和候选库；
- 每个候选被拒绝的具体、可验证理由；
- 自研范围和明确不支持的功能；
- 对应标准版本及 conformance 测试集；
- 威胁模型、安全审查和错误处理策略；
- 维护责任人、升级/兼容策略和未来迁移路径；
- 何时重新评估成熟库。

未完成 ADR 与 conformance 测试，不得把自研协议层、解析器或基础设施代码合入主线。

### 6.5 禁止重复造轮子与重复事实源

- 不重复实现现有项目已具备的功能；先搜索仓库、依赖、上游项目和已交付模块。
- 不为同一数据建立第二个主事实源；以现有领域系统为权威，通过 Adapter/映射复用。
- 不为同一协议同时维护两套 parser/serializer/dispatcher，除非有明确兼容迁移计划和退役日期。
- 不因为某个库的 API 不完全符合业务需求就 fork 或重写整个基础层；优先薄适配器、wrapper 或 feature 组合。
- 不以自研代码更少、依赖更少作为唯一决策标准；必须综合协议正确性、维护成本、安全性、可升级性和总生命周期成本。

### 6.6 执行前强制输出

任何新增协议、解析器、格式支持、运行时、存储引擎、认证机制或通用基础设施的任务书，必须包含以下小节：

- **成熟方案检索结果**
- **官方 SDK / 库候选比较**
- **采用库及 feature 选择**
- **vendor / 离线构建方案**
- **自研业务层边界**
- **例外 ADR（仅当拒绝成熟库时）**
- **兼容性、性能、安全和回归验收**

实施员不得跳过该评估直接编码；审阅员必须把是否遵守本规则列为 CI/阶段验收检查项。

## 7. RIP 后续阶段的应用范围

本硬约束适用于 Phase 9 剩余批次的新增通用能力、Phase 10、2.0 收尾审阅，以及之后 Document Intelligence / Knowledge Governance / Research Workflow 等全部新阶段。

但它不授权任何代理中途打断当前 Phase 9 批次 4，也不授权擅自修改活跃工作树。对当前 MCP 的处理顺序是：

1. 当前批次 4 按原计划完成并通过原有门禁；
2. 记录当前 MCP 协议层手写的事实、风险和待迁移项；
3. 安排独立 rmcp 迁移批次；
4. 将该批次的协议一致性和离线 vendor 验收纳入 Phase 9 正式收口条件，或作为有明确阻断状态的紧邻后续批次；
5. Phase 10 规划和所有后续任务书强制应用本成熟库优先规则。

## 8. 当前决策

**决策：当前 Phase 9 批次 4 不动；批次完成后迁移到官方 `rmcp`，不接受手写 MCP 协议层作为长期架构。**

保留的是 RIP 自己的业务集成：Tool Center 同源目录、只读 Catalog、Engine/Gateway 派发、EvidenceRegistrar 和权限策略；替换的是手写协议层与传输层。

本报告仅形成审计结论和可执行约束文本。本轮没有修改仓库、没有更新 AGENTS.md、没有改动工作流，也没有执行提交或推送。将第 6 节纳入项目 `AGENTS.md`、规划员提示词和实施员提示词时，应在活跃批次完成后的独立文档批次中进行。

---

## 参考链接

- [RIP `crates/mcp-server/Cargo.toml`](https://github.com/emoeem/research-intelligence-platform/blob/main/crates/mcp-server/Cargo.toml)
- [RIP MCP protocol implementation](https://github.com/emoeem/research-intelligence-platform/blob/main/crates/mcp-server/src/lib.rs)
- [RIP MCP catalog implementation](https://github.com/emoeem/research-intelligence-platform/blob/main/crates/mcp-server/src/catalog.rs)
- [RIP MCP protocol tests](https://github.com/emoeem/research-intelligence-platform/blob/main/crates/mcp-server/tests/mcp_protocol.rs)
- [RIP offline Cargo source configuration](https://github.com/emoeem/research-intelligence-platform/blob/main/.cargo/config.toml)
- [Official MCP Rust SDK](https://github.com/modelcontextprotocol/rust-sdk)
- [Official `rmcp` docs](https://docs.rs/rmcp/latest/rmcp/)
- [MCP 2024-11-05 schema](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/schema/2024-11-05/schema.ts)
