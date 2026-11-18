# RIP: Research Intelligence Platform（智能体入口）

任何智能体（zcode / Claude Code / Codex / Cursor / 其它）加载本仓库，按此文件行动。
当前状态与里程碑全量记录见 `docs/SESSION-HANDOFF.md`；平台总览见 `README.md`。

## 进门三件事

1. **先挂红队知识库**（本仓库自带，无任何外部依赖）：

   ```bash
   cd integrations/rtis && ./bootstrap.sh    # 重建 knowledge.db（不入 git）
   ```

   然后要么把它挂成 MCP（`rtis_mcp.mjs`，见 `integrations/rtis/README.md`），
   要么直接用命令行：

   ```bash
   node integrations/rtis/rtis_kb.mjs <<< '{"cmd":"search","q":"目标特征词","limit":10}'
   python3 integrations/rtis/rt.py poc search --q <组件名>
   ```

   纪律：动任何目标前先查库；命中就取全文照做；打完回填（`rt.py poc add/use`）。

2. **接任务先看交接**：`docs/SESSION-HANDOFF.md` 是唯一权威状态源（M0-M8 全程记录）。
3. **数据分级**：`knowledge.db`、`platform*.db`、凭据类证据一律不入 git（.gitignore 已配）。

## Runtime 原生优先与按需隔离（2026-11-18 起）

执行工具与平台服务采用 Native-first、Isolation-when-needed：Rust 平台服务与可信 Rust 工具优先原生运行；可信 Python Worker 优先使用项目级 venv + 既有 Worker Protocol；仅在依赖/系统库冲突、重型运行时、明确资源/文件/网络隔离需求或不可信代码风险成立时使用 rootless Podman；需要更强边界时使用 VM 内容器。容器不是默认运行方式，也不是 Cargo/CI/整个平台的构建环境。

- 原生运行不等于安全沙箱：仅允许审核过、Manifest 登记、二进制身份固定、参数 Schema 约束的工具；必须落实环境白名单、工作区边界、预算、超时、取消、进程回收、输出限制和统一审计。
- 不受信任工具不得通过 native_rust/python_worker/任意 CLI 绕过隔离策略；根据风险进入 podman_host 或 podman_vm。
- Python 重依赖不自动等于容器：可信且能在项目级虚拟环境稳定锁定的 Worker 可以原生运行；全局 pip 安装仍禁止。确需系统级依赖/隔离时再使用容器。
- **Host Software First**：优先复用宿主机已安装且身份/版本/能力经过验证的成熟软件（如 FFmpeg、LibreOffice、Tesseract、Poppler/MuPDF、Pandoc、ImageMagick）；候选清单不代表机器已安装，必须只读实测。缺失能力只报告，不擅自安装；解析恶意文件仍须按输入风险决定受限原生、Podman 或 VM。Host Capability Discovery 的安全规则与任务边界见 `docs/phase0/RT-0.5-HOST-CAPABILITY-DISCOVERY.md`。
- 所有 Runtime 复用 Tool Registry、Tool Engine、Runtime Gateway、ToolRun、Artifact、Evidence、Trace；禁止第二套执行/事实体系。
- Cargo build/test/Clippy 与 GitHub CI 默认原生执行。`target/` 是可再生缓存；Profile 优化先做 A/B，不得未经验证更改全局 RUSTFLAGS 或缩小 CI 测试范围。清理 target、Podman 镜像/容器/卷必须先核实归属和影响，禁止无差别 prune。
- 历史 `docs/phase0/TOOL-RUNTIME-ADR.md` 与 Phase 2 验收记录保留为历史事实；未来默认策略以 `docs/phase0/NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md` 为准。WSW-1 当前批次红线和工作树优先，不得借 Runtime 调整改写其 Cargo/迁移范围。

## 成熟库优先（硬约束，2026-11-14 起）

1. **通用基础能力默认先找成熟库**：协议、标准格式、解析/序列化、Schema 校验、认证、并发与异步运行时、网络传输、数据库、索引、压缩归档、文件类型识别、OCR 与文档解析等领域，实现前必须检索官方 SDK 与成熟维护库，评估通过即引入；只有产品差异化业务逻辑才自研。
2. **vendor 不在不是拒绝理由**：Rust 新依赖走增量 cargo vendor（cargo 已配 rsproxy 镜像；vendor/ 入库保 --offline；引入后干净环境离线构建与测试必须复验）。「vendor 里没有」「增加依赖」「需要 tokio」「需要一次受控联网」单独出现均不构成拒绝成熟库的理由，必须先做最小 feature 与依赖树分析。
3. **引入前必查**：维护状态、许可证、依赖树与 MSRV、feature 最小集、安全公告、目标协议/格式版本覆盖；可信 Python Worker 优先项目级 venv/锁文件原生运行，只有系统依赖冲突、重型运行时或明确隔离需求才进入 worker 容器；禁止全局 pip 与未经授权的系统级安装。
4. **手写通用组件必须先有例外 ADR**：候选库与拒绝理由、conformance 测试集、维护与迁移路径，经确认后才可合入；不为同一协议维护两套 parser/serializer/派发器。
5. **任务书强制小节**：成熟方案检索结果 / 候选比较 / 采用库与 feature / vendor 方案 / 自研边界 / 例外 ADR（仅当拒绝）/ 兼容性能安全回归验收；审阅与阶段验收把本节列为检查项。
6. **先例**：Phase 9 MCP 手写协议层判定不可长期保留（P0 tools/list 缺必需 inputSchema 且被测试固化；P1 initialize 版本回显无协商），rmcp 迁移为阻断性整改批次，审计全文 docs/phase9/MCP_MATURE_LIBRARY_AUDIT_2026-11-14.md。

## 仓库地图

| 路径 | 内容 |
|---|---|
| `integrations/rtis/` | 自包含红队知识栈（引擎/CLI/编排/MCP/125 技能/POC 落盘），2026-11-09 起本仓库为唯一活体 |
| `crates/` + `workers/` | Rust 核心 + worker（cargo 走 rsproxy 镜像，离线 vendor 可构建） |
| `migrations/` | ETL（RTIS/ERI 源库到 platform.db），快照在 `sources-snapshot/` |
| `apps/` `models/` `schemas/` | 应用层 / 数据模型 / schema |
| `legacy/` | ERI P0-P2 与 architecture-2.0 的历史交付（只读） |
| `docs/` | 里程碑报告 + SESSION-HANDOFF.md |

## 与 dsh 的关系（2026-11-09 解耦）

红队知识栈已从 `~/.dsh/redteam` 整体迁入 `integrations/rtis/`（默认路径全部自包含）。
`~/.dsh/redteam` 冻结不再更新；全量备份 `~/backups/dsh-redteam-20261109-094341.tar.zst`。
临时回旧布局：`RTIS_HOME=~/.dsh/redteam`。
