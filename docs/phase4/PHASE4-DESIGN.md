# Phase 4 阶段说明与实施设计：Context Broker（P4-0 交付物）

- 日期：2026-11-10
- 状态：已定稿（本阶段批次 P4-0 的交付物；批次内部实现细节按 ROADMAP §1.2 冻结纪律可在各批开工时微调，批次边界与完成判据见 §6）
- 定位：PHASE-ROADMAP §1.2 冻结纪律「每个 Phase 开工前按参考计划 §24 模板出阶段说明」在 Phase 4 的执行件。§24 模板出处：/home/emo/Downloads/RIP-完整项目实施计划.md 2296 行（「# 24. 每阶段交付模板」，本会话实测在位），十节为：目标、范围、非目标、现状、设计、实施批次、风险、验收、回滚、交付物。同款先例：docs/phase3/PHASE3-DESIGN.md（P3-0）。
- 执行基准：docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md（下称「计划」）§14 Phase 4（786 到 794 行，本会话实读）与 §8（425 到 446 行）；docs/phase0/CONTEXT-BROKER-DESIGN.md（下称「设计稿」，Phase 4 实施蓝本，ROADMAP §5.1 原文认定「其 §11 M4.0 到 M4.5 即本阶段执行序」）；docs/phase0/PHASE-ROADMAP.md（下称「ROADMAP」）§1.2、§1.4、§5；docs/phase3/PHASE3-DESIGN.md（下称「PHASE3-DESIGN」，批次纪律先例）。
- 红线遵守声明：本文为设计文档，纯文档批次，零功能新增（P3-0 同款先例）。撰写过程只新增本文档与 docs/README.md 的 Phase 4 登记行，未修改任何源文件；未写 migrations/ 下任何 platform*.db；未在宿主机安装任何依赖；未运行任何写库命令。文中「现状」结论逐条标注来源，本会话实测项见 §4.3。

---

## 1. 目标

把计划 §14 Phase 4（786 到 794 行）五条规划落成本阶段可验收的增量：交付新 crate `crates/context-broker`（计划 §13 crate 清单口径，设计稿 §10 引计划:719），实现 L0 到 L3 四级上下文装配（计划:437-443 层表）、意图识别与能力匹配、分项 Token 预算与降级驱逐、ContextManifest 三表持久化与引用追踪、内容寻址缓存与版本失效、工具说明三级加载（card/brief/full）、AssembledContext 输出契约。阶段验收三条（计划:794 原文）：不默认把整个工具目录或知识库正文塞入上下文；可解释选择了哪些知识和工具、为什么选择；工具输出通过 Artifact 引用进入上下文。

**本批（P4-0）目标**：落盘本阶段说明，冻结三处落地决议（§5.2）：其一，迁移号取 008（设计稿 §6.1 草案号 005 按 ROADMAP §1.4 让位规则让位）；其二，M4.1 目录源直连已交付的 tool-registry（设计稿 Mock 先行条件已消解）；其三，brief 生成落位 crates/context-broker 而非登记期预生成（与设计稿 §8 建议的差异记入 §5.3 差异对照，该表即 P4-6 实施差异文档的种子）。

## 2. 范围

### 2.1 允许修改或新增的落点（全部在 research-platform 仓库内，独立 commit，逐批可回滚）

| 落点 | 内容 | 批次 |
|---|---|---|
| `crates/context-broker/`（新增） | Broker crate 全部模块（设计稿 §10 目录树）：intent、policy、estimate、source 及 sources/ 六源、assemble、manifest、cache、error、rp-context CLI | P4-1 到 P4-5 |
| `crates/domain` | Context 域类型（IntentQuery、ContentRef 等，随 P4-1 落位） | P4-1 |
| `migrations/`（新增 1 个文件） | 008_context_broker.sql（§5.2 决议 1），只应用到开发库副本与测试内存库 | P4-1 |
| `docs/phase4/` | 本文档 + 各批次验收报告（参考计划 §24 批次验收报告模板）+ P4-6 实施差异文档与阶段验收 | 全程 |
| `docs/README.md` | 阶段工作目录登记（本批已登记 Phase 4 行） | P4-0 |

### 2.2 不修改

`apps/server` 既有端点行为；RTIS 本体（红线：只经 `rtis_kb.mjs` / `rt.py` Adapter 只读接入，设计稿 §1.2 原文）；`workers/python` 既有 Worker Protocol v1 协议；M0 到 M8 已交付能力（红线：不推翻）；`platform.db` 与 `platform-dryrun.db`（红线：全程不写，一切库操作走开发库副本与测试内存库）；既有九个 crate 的行为（复用一律走公开接口，设计稿 §12「不在 Broker 内重写检索或分词」原文）；`crates/tool-registry` 登记链路与数据面（§5.2 决议 3 的落位前提：brief 不做登记期预生成，登记面零改动）。

### 2.3 迁移声明（决议 1 落点）

本阶段预计新增 1 个迁移：008_context_broker.sql，承载设计稿 §6.1 三表（context_manifest、context_manifest_item、context_ref_log），DDL 以该草案为基，外键与索引在 P4-1 评审补全（计划 §12:706 要求唯一键、外键、索引、删除语义明确）。编号冻结依据见 §5.2 决议 1：本会话 `ls migrations/*.sql` 实测 001 到 007 在位（005_tool_contract、006_agent_data_plane、007_runtime_node 均已合入，合入批次见各阶段 RELEASE，与本批无关），下一可用号 008；P4-1 落库执行前再实查一次，若期间有新合入则顺延取当时下一可用号（ROADMAP §1.4 规则原文优先于本文件）。新迁移只应用开发库副本与测试内存库，幂等执行两次无差异（P4-1 判据），生产 `platform*.db` 全程不写。

## 3. 非目标

1. **不做 LLM 调用**：Broker 核心是确定性 Rust 代码，意图识别不用模型推理（设计稿 §1.2 原文）；自然语言理解增强属 Phase 7 Agent Engine。
2. **不实现向量检索轴**：search-engine 现状 FTS 加启发式混合，bge 轴未接通（设计稿 §1.2、ROADMAP §5.5 风险 5），预留接口位不落地，防止提前实现（计划 §15:872 口径）。
3. **不修改生产 RTIS 行为**：RTIS 知识栈只读 Adapter 接入（红线）；不把 knowledge.db 内容拷入平台库（设计稿 §12 原文）。
4. **不直连生产库、不绕过 Application API**：Web/MCP/CLI 的上下文能力一律走 Broker 公开接口（设计稿 §1.2）；不为 Broker 加任何直连 SQL 的handler 面。
5. **不改 Tool Registry 登记面**：不做登记期 brief 预生成（§5.2 决议 3），不新增登记字段、钩子或第二轮校验。
6. **不做 MCP/REST/CLI 对外能力面**：rp-context 是仓库内验收与运维 CLI（ROADMAP §5.4 第 2 组命令口径）；对外 CLI 入口属 Phase 9（ROADMAP §1.2 映射表第 13 行）。
7. **不接 memory_history 真实数据**：分项预算留位，任务记忆与历史摘要接入属 Phase 7（设计稿 §5.1 分项表 memory_history 行）。

## 4. 现状

### 4.1 代码与数据面（Phase 3 已收口，commit c9e7b23，本会话 git status 实测干净）

- `crates/` 九个：audit-engine、database、domain、quality-engine、resource-engine、runtime-gateway、search-engine、tool-engine、tool-registry（本会话 `ls crates/` 实测）；无 context-broker，本阶段新增（计划:719 已规划该 crate 名，设计稿 §10 口径）。
- `crates/tool-registry` 公开接口（本会话实读）：`Registry::discover()`（src/lib.rs:93，回 Vec&lt;ToolSummary&gt;，字段 tool_id/name/lifecycle/governance/permission_level，src/catalog.rs:13-24）、`Registry::query()`（src/lib.rs:98，回 ToolDetail：tool 行 + 全部版本行 + current_version）、`Registry::schemas()`（src/lib.rs:103，回 input/output JSON Schema）、`Registry::register()`（src/lib.rs:76）。Phase 1 验收「工具可发现」已交付，设计稿的 Registry 未落地前提不复存在（§5.2 决议 2）。
- `migrations/`：001 到 007 在位（本会话 `ls migrations/*.sql` 实测），下一可用号 008；context 三表当前无任何承载表。
- hash 能力先例：`crates/tool-registry/src/hash.rs` 文件在位（本会话 ls 实测）；其公开函数面是否可被 context-broker 复用，P4-1 接入时核对（待验证），缓存键与 content_hash 不另起炉灶。
- 质量门基线（Phase 3 收口实测，docs/releases/RELEASE_2026-11-10_PHASE3.md:9-10）：`cargo test --offline --workspace` 207 全绿、`cargo clippy -D warnings` 零告警、`cargo fmt --check` 通过、worker unittest 125 全绿（跳过 11）。本批无代码改动未重跑（§4.3 未执行项），P4-1 起每批必跑并对照该基线。

### 4.2 复用面（引用设计稿既有实测记录，本批未重跑）

设计稿 §12 复用对照表十项：search-engine 检索、resource-engine tokenizer 与 ALIASES 同步源、quality-engine 治理过滤与 VERIFIED 守卫、audit-engine correlation_id 链路、crates/database 迁移执行器（discover_migrations / migrate）、worker runtime 缓存原子写先例、RTIS kit（Adapter 不改本体）、ERI sidecar、Tool Manifest `context` 字段、三份只读查询脚本。其证据口径已在设计稿 §2 到 §7 逐条落档（架构盘点与工具盘点行号），Phase 4 直接继承，不重复审计；批次落地时以公开接口实际签名为准（逐项核对，不照抄文档签名）。

### 4.3 本会话复核记录（只读实跑，2026-11-10）

| 命令/动作 | 输出摘要 |
|---|---|
| `git log --oneline -1`（research-platform/） | `c9e7b23 RIP 2.0 Phase 3 首批 Tool Packs`；`git status --short` 空（干净） |
| `ls migrations/*.sql` | 001 到 007 七个文件，下一可用号 008 |
| `ls crates/` | 九个 crate，无 context-broker |
| `grep -n "pub fn\|pub struct\|pub trait" crates/tool-registry/src/lib.rs` | register（:76）、discover（:93）、query（:98）、schemas（:103）在位 |
| `grep -n "pub struct ToolSummary" -A 20 crates/tool-registry/src/catalog.rs` | 五字段投影（tool_id/name/lifecycle/governance/permission_level，:13-24） |
| `sed -n '784,795p' docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md` | Phase 4 五条规划与验收三条原文在位（786 标题、794 验收） |
| `sed -n '437,446p'` 同上 | L0 到 L3 层表（437-443）、Broker 职责行（444）、工具说明分层要求（446）原文在位 |
| `grep -n "^# 24\." /home/emo/Downloads/RIP-完整项目实施计划.md` | 2296 行「# 24. 每阶段交付模板」在位 |
| Read docs/phase0/CONTEXT-BROKER-DESIGN.md 全文（450 行） | :177 草案号 005、:267 登记期预生成建议、:350 Mock Registry 落位、:389-398 M4.0 到 M4.5、:400-406 验收映射，逐行定位核对 |
| `sed -n '293,296p'`、`sed -n '304,343p'` docs/phase0/PHASE-ROADMAP.md | §5.1 目标与验收三条、§5.3 批次表 P4-1 到 P4-6、§5.4 验收命令、§5.5 五条风险原文在位 |
| `grep -n "cargo test\|clippy" docs/releases/RELEASE_2026-11-10_PHASE3.md` | :9-10 质量门基线在位（207 全绿、零告警、fmt 通过） |

未执行项：未写任何数据库、未安装任何依赖、未修改任何源文件（本文档与 docs/README.md 登记行除外）、未运行 cargo/python 测试（纯设计批次，无代码改动）。

### 4.4 待验证项（随对应批次验证，不预先断言结论）

1. Token 估算常数未对真实 tokenizer 校准（设计稿 §13.1；ROADMAP §5.5 风险 1，P4-6 前用 models/bge-small-zh-v1.5/tokenizer.json 离线抽样校准）。
2. RTIS knowledge.db 实际条数口径差（设计稿 §13.3，README 14717 与 rtis.py 头注 14664，P4-3 实查一次）。
3. tool_execution 历史数据与 past_success 权重校准（设计稿 §13.4，Phase 4 恒为 0 只留通路）。
4. ContextSource trait 与 Registry 对接签名（设计稿 §13.2 显式待验证项；决议 2 消解其 Mock 前提，签名对齐在 P4-2 批内实测定稿）。
5. RTIS 源子进程调用超时与失败语义（设计稿 §13.6，P4-2/P4-3 实测，eri_adapter.py 三态回落为参照）。
6. brief 装配期生成的实测开销（决议 3 引入的新验证点，P4-2 用 cache-stats 前后对比落数，见 §5.4）。
7. tool-registry hash.rs 公开函数面的复用可行性（§4.1，P4-1 核对）。

## 5. 设计

### 5.1 总体形状与总则

装配流沿用设计稿全文（实施蓝本，ROADMAP §5.1 认定），本节只画骨架不复述：

~~~text
IntentQuery（确定性意图，设计稿 §4）
   │  归一化复用 resource-engine tokenizer + ALIASES
   ▼
ContextSource 候选检索（设计稿 §4.3：registry / knowledge / rtis / rulepack / artifact / plugin）
   │  统一打分 w1 到 w5 + 一票否决过滤 + matched_signals 留痕
   ▼
分项预算闸（设计稿 §5：l0 到 reserve_output 七分项 + 降级驱逐链）
   ▼
AssembledContext 签发（设计稿 §9：L3 不内联，expand 记账，citations 权威引用集）
   │
   ├── context_manifest / context_manifest_item / context_ref_log 三表留痕（设计稿 §6）
   └── broker-cache 内容寻址缓存 + 七类版本失效触发器（设计稿 §7）
~~~

总则四条（继承设计稿 §2 并加本阶段纪律）：

1. 按需加载、预算封顶、证据不丢、清单不可变（设计稿 §2 第 1 到 5 条原文执行）。
2. 复用优先：检索走 search-engine、分词与同义走 resource-engine、治理过滤走 quality-engine、事件走 audit-engine、迁移走 crates/database，全部公开接口，不在 Broker 内重写任何既有能力（设计稿 §12「不复用与不重复」段原文）。
3. 端口隔离：Tool Registry 与 Artifact 存储经 `source.rs` trait 接入（设计稿 §10 依赖方向），决策 2 的直连不破坏该 trait 边界。
4. 新依赖预计为零（Rust 标准库加 workspace 既有 crate 足够：sha256 能力有 tool-registry/src/hash.rs 先例）；确需新依赖时先评估维护状态，选型后增量 cargo vendor 保证 `--offline` 构建通过，并作为差异条目记入 P4-6 差异文档。

### 5.2 冻结决议（三处，本批核心产出）

#### 决议 1：迁移号取 008（设计稿 §6.1 草案号 005 让位）

- 设计稿原文：docs/phase0/CONTEXT-BROKER-DESIGN.md:177「-- 005_context_broker.sql（草案，纪律同既有迁移：IF NOT EXISTS，幂等，架构盘点:140）」。
- 让位规则：ROADMAP §1.4（86 到 88 行）冻结规则原文「迁移编号按合入顺序递增，先到先得；……设计文档中的编号让位于本规则」。该规则同时让位 ROADMAP 自己的预测号（§1.4 原文预计 006_context_broker.sql，其后 Phase 2/3 期间 006 与 007 已被 agent_data_plane 与 runtime_node 合入占用）。
- 实测依据：本会话 `ls migrations/*.sql` 得 001 到 007，下一可用号 008。
- 落位：Phase 4 迁移定名 008_context_broker.sql；P4-1 落库前再实查一次，以落库时点实测为准（PHASE3-DESIGN §2.4 同款纪律）。

#### 决议 2：M4.1（P4-2 批）目录源直连已交付的 tool-registry，Mock 先行条件消解

- 设计稿原文两处：:350「src/sources/registry.rs 工具目录源（Phase 1 Registry 接口；未落地期用 Mock，先例计划:759）」；:108「Registry 未落地期间用 Mock 源」。
- 消解依据：Phase 1 已交付 crates/tool-registry（commit 962cfa9）；本会话实读公开接口 discover/query/schemas/register 在位（§4.1），「工具可发现」验收已达成，Mock 的存在理由（Registry 未落地）不复存在。
- 落位：P4-2 的 L1 目录源经 ContextSource trait 适配直连 Registry 公开接口；Mock 源降级为测试载具（确定性单测与负向用例用），不再是生产路径的过渡形态。

#### 决议 3：brief 生成落位 crates/context-broker（装配期生成加缓存），不做登记期预生成

- 设计稿原文：:267「tier 2 brief 在 Registry 登记入库时预生成并随 Manifest 存储装配期直接取，避免运行时解析 YAML/JSON Schema 的开销；生成器校验 required 键与 Schema 一致。」
- 落位决议：brief 生成器落 crates/context-broker，在 L2 装配路径内对入选工具现生成，产物进 broker-cache；Tool Registry 登记链路与数据面零改动。
- 理由四条：其一，登记面保持「只收 Manifest 事实」单一职责，不加展示性派生数据；其二，最小侵入，不改已交付 crate（Phase 1 冻结交付物，红线不推翻口径）；其三，历史 Manifest 不可变性（设计稿 §2 第 5 条）不受 brief 展示格式演进影响，登记期预生成则格式演进需回刷历史登记数据；其四，三级接口 tool_cards / tool_briefs / tool_schema 本就规划在 Broker 侧（设计稿:268），生成器与调用面同 crate 内聚。
- 设计稿关切的对冲：其预生成建议的动机是避免装配期解析开销；本决议以「登记数据面直取，不重解析 Manifest 文件全文 + 缓存命中零解析 + P4-2 实测落数」对冲（细节 §5.4）。实测否决时回退路径见 §7 风险 4 与 §9。
- 差异记档：本决议与设计稿 §8 的差异记入 §5.3 差异对照表（D-3 行），P4-6 汇编独立实施差异文档时为必收条目。

### 5.3 与设计稿差异对照（差异文档种子）

除下表三处，Phase 4 其余全部按设计稿原文执行；本表即 P4-6 实施差异文档的种子，P4-6 汇编时逐条复核并补执行期新发现差异。

| 编号 | 设计稿位置 | 设计稿口径 | Phase 4 落地决议 | 依据 |
|---|---|---|---|---|
| D-1 | §6.1（:177） | 迁移草案号 005_context_broker.sql | 008_context_broker.sql | ROADMAP §1.4 让位规则；本会话 `ls migrations/*.sql` 实测 001 到 007 |
| D-2 | §4.3（:108）、§10（:350） | Registry 未落地期用 Mock 源先行 | 目录源直连 tool-registry 公开接口，Mock 降为测试载具 | Phase 1 已交付；本会话实读 discover/query/schemas 在位（§4.1） |
| D-3 | §8（:267） | tier 2 brief 在 Registry 登记期预生成并随 Manifest 存储 | 装配期生成 + broker-cache 缓存，登记面零改动 | §5.2 决议 3 理由四条；ROADMAP §1.2 最小侵入纪律 |

### 5.4 决议 3 落位细节：brief 装配期生成

- 生成时机：L2 装配路径内，对入选工具候选现生成 brief；输入取登记数据面（Registry `query()` / `schemas()` 或 manifests/tools/ 双份文件，取数路径属 P4-2 批内实现细节，按 ROADMAP §1.2 冻结纪律批内微调定稿），不重解析 Manifest 文件全文的解析面收敛到字段级取数。
- 压缩规则不变：约 120 tokens 软上限，生成超限即压缩（设计稿 §8 tier 2 行原文）；required 键与 Schema 一致性校验保留（设计稿:267 后半句，校验在生成器内做）。
- 开销对冲：brief 产物进 broker-cache，键含 tool_id、version、content_hash、policy_version（设计稿 §7.1 条目形状），命中即零生成；未命中走纯函数路径，P4-2 以 cache-stats 前后对比落数（§4.4 待验证 6）。
- 一致性收益：brief 展示格式演进只影响新装配，历史 Manifest 与其 content_hash 不受影响（设计稿 §2 第 5 条不可变原则）。
- 接口面不变：tool_cards / tool_briefs / tool_schema 三件套仍由 Broker 公开（设计稿:268），调用方无感；MCP 映射（rip.list_tools card 层、rip.get_tool_schema full 层，设计稿:269）不变，属 Phase 9 接线。

### 5.5 crate 结构与依赖方向

结构沿用设计稿 §10 目录树（src/intent.rs、policy.rs、estimate.rs、source.rs、sources/ 六源、assemble.rs、manifest.rs、cache.rs、error.rs、bin/rp-context.rs、tests/），两处按决议标注：sources/registry.rs 直连（决议 2），brief 生成器落 assemble 侧（决议 3，具体模块归属 P4-2 定稿）。依赖方向沿用设计稿 §10 两条：依赖 domain、database、audit-engine、search-engine（engine 间公开接口依赖有实测先例）；Tool Registry 与 Artifact 存储经 trait 端口接入，不硬编码内部实现。头部约束沿用 `#![forbid(unsafe_code)]` 与 `#![deny(missing_docs)]`（设计稿 §10 引六 crate 现状）。公开接口签名以设计稿 §10（ContextSource trait、Broker assemble/expand/manifest）为基，P4-2 对齐 Registry 实际签名后定稿（§4.4 待验证 4）。

## 6. 实施批次

本阶段共 7 批（P4-0 到 P4-6）；P4-0 即本文档。与 ROADMAP §5.3 草案的映射：P4-1 到 P4-6 一一对应，唯一差异是 P4-2 按决议 2 将「Mock Registry 起步」改为「直连 tool-registry」（差异 D-2）；与设计稿 §11 M4.0 到 M4.5 的映射为 P4-1 到 P4-6 对 M4.0 到 M4.5（ROADMAP §5.1 原文认定）。每批完成判据即该批验收命令通过并出批次验收报告（参考计划 §24 批次验收报告模板）。

| 批次 | 内容 | 最小可验证单元（完成判据） | 并行性 |
|---|---|---|---|
| P4-0（本批） | 阶段说明（本文档，§24 十节）；三决议冻结与差异对照落盘（§5.2、§5.3） | 十节齐备；零长横线；docs/README.md Phase 4 登记行；迁移声明落盘（§2.3） | 起点 |
| P4-1 | 008_context_broker.sql（开发库副本）+ domain Context 类型 + policy 加载 + token 估算器 + 纯函数单测 | 迁移在开发库副本幂等执行两次无差异；estimator 有单测基线（= 设计稿 M4.0 判据） | 【并行】车道 A 起点（ROADMAP §1.2） |
| P4-2 | L0 规则包源 + L1 目录源直连 tool-registry（决议 2，Mock 降为测试载具）+ card/brief 生成器（决议 3 落位） | 已登记工具可出目录卡片与 brief；brief 生成开销经 cache-stats 落数（= M4.1 判据 + 待验证 6） | 【并行】依赖 P4-1 |
| P4-3 | intent 纯函数 + knowledge/rtis 只读源接入 + 打分与 matched_signals | 给定「kingdee HTTP 检查」类任务，rtis 源返回只读命中（= M4.2 判据，命令口径同 TOOL-INVENTORY §8 的 rt.py 冒烟） | 【并行】依赖 P4-1 |
| P4-4 | assemble 全流程 + 三表落库 + expand 记账 + AssembledContext 契约校验 | 同一任务两次装配产生两个 manifest，selection_hash 一致可对账（= M4.3 判据） | 【串行】依赖 P4-2/P4-3 与 Phase 2/3 的 Run/Artifact 数据 |
| P4-5 | 内容寻址缓存 + 七类版本失效触发器 + rp-context CLI 四子命令（assemble/expand/manifest-show/cache-stats） | 改一个工具版本号后 cache-stats 显示对应条目重建（= M4.4 判据） | 【串行】依赖 P4-4 |
| P4-6 | 验收三测试（塞入/可解释/Artifact 引用）+ 实施差异文档汇编（§5.3 表为种子）+ 收口 RELEASE | 三测试进 CI（= M4.5 判据）；质量门全绿；差异文档覆盖全部实施差异 | 【串行】依赖 P4-5 |

验收三测试到计划:794 三条的映射沿用设计稿 §11 末三条（不默认塞入、可解释、Artifact 引用），本文件不复述其测试设计；质量门进既有 CI（`cargo test --offline --workspace` 与 clippy `-D warnings`，设计稿 §11 质量门原文），不新增独立标准。

## 7. 风险

1. **Token 估算常数未校准**（继承 ROADMAP §5.5 风险 1）：预算分项在校准前只有相对意义。缓解：P4-6 前用 bge tokenizer.json 离线抽样校准（设计稿 §5.2 预留路径）；校准只修常数不改预算结构。
2. **RTIS 条数口径差**（继承 ROADMAP §5.5 风险 2）：影响「不默认塞入」测试的规模口径。缓解：P4-3 实查一次并落批次报告。
3. **past_success 权重通路无真实数据**（继承 ROADMAP §5.5 风险 3）：Phase 4 恒为 0。缓解：只留通路不参与排序效果评估，首批 Run 积累后回看，不误判为排序缺陷。
4. **决议 3 的装配期开销**：设计稿 §8 预生成建议的本意是规避装配期解析开销（:267 原文关切），改为装配期生成引入新风险面。缓解：登记数据面字段级直取（不重解析全文）+ 缓存命中零生成 + P4-2 实测落数；若实测不可接受，按差异文档评审恢复登记期预生成（§9 回退），不静默改口径。
5. **直连 Registry 的耦合面**（决议 2 引入）：Registry 公开接口演进会波及 Broker。缓解：ContextSource trait 端口隔离保留（§5.1 总则 3），Registry 适配器单点适配；P4-2 定稿签名后即冻结，后续 Registry 演进走适配器内变更。
6. **范围蔓延**：向量轴不实现、memory_history 不接数据、对外 CLI 不做（§3 第 2、6、7 条）；实现类改动越出 §2.1 落点表即视为越界，验收报告记录并回滚（PHASE3-DESIGN §7 风险 5 同款纪律）。
7. **RTIS 子进程频控**：本阶段无容器面，宿主风险集中在 rtis 源调用。缓解：调用间隔不低于 1s、超时与三态回落按设计稿 §13.6 参照 eri_adapter.py 先例（P4-2/P4-3 实测）。

## 8. 验收

阶段验收命令（P4-6 收口时全跑；各批次判据命令随批次验收报告落盘）。尖括号为占位符，执行时按实际值替换。

~~~bash
cd /home/emo/code/pensting/research-platform

# 0. 文档自检（P4-0，本批已跑，见 §4.3 与批次报告）
grep -c '^## ' docs/phase4/PHASE4-DESIGN.md            # 期望 >= 10（§24 十节）
grep -P '[\x{2014}\x{2013}]' docs/phase4/PHASE4-DESIGN.md; echo "dash_grep_exit=$?"   # 期望退出码 1（零长横线）
grep -rP '\x{2014}|\x{2013}' docs/ ; echo "docs_dash_exit=$?"                        # 期望 1（DOC-STANDARD 提交前检查）

# 1. 质量门（与 CI 同命令 + fmt；P4-1 起每批必跑，基线对照 RELEASE_2026-11-10_PHASE3.md:9-10）
cargo test --offline --workspace
cargo clippy --offline --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
python3 -m unittest discover -s tests -v

# 2. 装配端到端（ROADMAP §5.4 同款，rp-context 四子命令）
cargo run --offline -p context-broker --bin rp-context -- assemble --task-id t-acceptance
cargo run --offline -p context-broker --bin rp-context -- manifest-show <manifest_id>
cargo run --offline -p context-broker --bin rp-context -- cache-stats

# 3. 验收第 1 条「不默认塞入」对账（只读查开发库副本，mode=ro，不触生产库）
python3 -I -c "import sqlite3; c=sqlite3.connect('file:<开发库副本>?mode=ro',uri=True); print(c.execute('select layer,kind,sum(est_tokens) from context_manifest_item group by layer,kind').fetchall())"
# 期望：无 L3 正文内联行，L1 为目录卡片，used 总量在 budget_report 内

# 4. 验收第 3 条「Artifact 引用」对账（只读）
python3 -I -c "import sqlite3; c=sqlite3.connect('file:<开发库副本>?mode=ro',uri=True); print(c.execute(\"select event,count(*) from context_ref_log group by event\").fetchall())"
# 期望：CITED_IN_TOOL_CALL 事件存在且引用指向 artifact

# 5. 宿主机零安装对照（基线延续 Phase 3 收口值，全程 diff 为空）
pacman -Q | wc -l
~~~

计划 §14 Phase 4 验收口径在本阶段的达成形式：三条验收（计划:794）各由 §6 P4-6 的验收三测试与第 3、4 组对账命令落证，逐条入 P4-6 阶段验收报告。

## 9. 回滚

- **代码**：每批次独立 commit，`git revert <批次 commit>` 即整体撤销该批；crates/context-broker 为整目录新增，revert 即整体移除；不触及既有九 crate 与 M0 到 M8 交付物（红线），既有行为由三道门回归守护（§8 第 1 组）。
- **迁移**：008 只应用开发库副本与测试内存库；回退走副本回退或 drop 三表（DDL 幂等，重放无差异）；生产 `platform*.db` 全程零写入，无生产回滚面（ROADMAP §1.4 口径）。
- **文档**：本文档与 docs/README.md 登记行随 P4-0 commit 回滚恢复原状；后续批次验收报告各自独立回滚。
- **决议级回退**：决议 3 若被 P4-2 实测否决（§7 风险 4），按差异文档评审恢复设计稿 §8 登记期预生成路径，该回退本身作为新差异条目记档，不静默；决议 1 编号若遇并行合入冲突，按 ROADMAP §1.4 顺延取当时下一可用号，三表 DDL 不变。

## 10. 交付物

- **代码**：`crates/context-broker/`（P4-1 到 P4-5 分批成形，模块目录见设计稿 §10 与本文件 §5.5）；`crates/domain` Context 类型。预计新增迁移 1 个（008，§2.3 声明）、新增 crate 1 个、新增第三方依赖 0 个（§5.1 总则 4）。
- **文档**：本文档（`docs/phase4/PHASE4-DESIGN.md`）；P4-1 到 P4-6 各批次验收报告（`docs/phase4/` 下，参考计划 §24 批次验收报告模板，含命令原文与输出摘要）；P4-6 实施差异文档（§5.3 表为种子）与阶段更新记录（`docs/releases/`，DOC-STANDARD §四 格式）；docs/README.md Phase 4 登记行（本批已落）。
- **测试**：intent 与 estimator 纯函数单测、迁移幂等用例、装配端到端、缓存失效用例、验收三测试（计划:794 映射，设计稿 §11 末三条）。
- **数据**：无生产库写入（红线）；开发库副本上的 context 三表结构行与验收测试行；broker-cache 为运行时数据（目录可配置，设计稿 §7.1），不进 git。
