# Phase 0 交付物 7：Context Broker 设计（CONTEXT-BROKER-DESIGN）

日期：2026-11-09
性质：设计文档（Phase 0 产出，用于指导 Phase 4 实施），本次未修改任何源文件、未写任何数据库。
对应计划条目：`docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md` §16 第 7 条（CONTEXT-BROKER-DESIGN.md：知识、插件和工具的分层上下文设计，计划:891）；执行基准为同计划 §8「知识库与 Context Broker」（计划:425-446）与 §14 Phase 4（计划:786-794）。
输入文档：`docs/phase0/CURRENT-ARCHITECTURE-INVENTORY.md`（下称「架构盘点」）、`docs/phase0/TOOL-INVENTORY.md`（下称「工具盘点」）、`legacy/architecture-2.0/MODULE_BOUNDARIES.md` §8.1/8.2（本次实际读取）。
证据口径：对现状的描述均给出 `路径:行号` 或命令原文；设计提案本身标注「设计」；未能亲自验证的事项标注「待验证」。

结论先行：Context Broker 是 Agent 与知识库、工具库之间的唯一上下文装配层，核心机制是四层加载（L0 到 L3）加预算控制加清单留痕。Phase 4 落地为新 crate `crates/context-broker`（计划:719 已规划该 crate 名），复用 search-engine 检索、audit-engine 关联 ID、quality-engine 治理状态与 worker 缓存先例；对 RTIS 知识栈只经 Adapter 只读接入（工具盘点:161），不修改生产 RTIS 行为。装配结果是带引用追踪的结构化 `AssembledContext`，工具输出一律以 Artifact 引用进入上下文（计划:794 Phase 4 验收）。

---

## 1. 目标与边界

### 1.1 要解决的问题（来自计划原文）

- 架构原则明确要求：Context Broker 按需加载知识和工具说明，不把整个知识库或所有工具描述塞入每次上下文（计划:121）。
- §8 职责清单：识别任务意图、检索候选知识/插件/工具、排序、计算 Token 预算、去重压缩、保留来源引用，并防止不相关插件进入上下文（计划:444）。
- 知识库职责要求跨插件检索、来源追踪、版本追踪和引用（计划:430）。
- Phase 4 验收三条：不默认把整个工具目录或知识库正文塞入上下文；可解释选择了哪些知识和工具、为什么选择；工具输出通过 Artifact 引用进入上下文（计划:794）。
- 最终验收要求知识与工具说明按任务动态加载，控制 Token 消耗（计划:906）。

### 1.2 边界（Phase 4 做与不做）

做：

- 四层上下文定义、装配、清单（Manifest）落库与引用追踪。
- 意图识别（规则与词典驱动，确定性，可测试）与能力匹配打分。
- 分项 Token 预算、降级与驱逐策略。
- 工具说明三级分层加载（目录卡片、匹配摘要、按需 Schema）。
- 内容寻址缓存与版本失效。
- 给 Agent Engine 的输出契约。

不做（Phase 4 范围外，设计预留接口）：

- 不做 LLM 调用：Broker 核心是确定性 Rust 代码，意图识别不用模型推理，保证可测试可回放。意图理解的自然语言增强留到 Agent Engine（Phase 7）。
- 不实现向量检索轴：search-engine 当前只有 FTS 加启发式混合，向量在 Python embedding worker 侧二者未接通（架构盘点:76，§2.5 审计补充）。设计预留 `ContextSource` 的查询向量接口位，Phase 4 不实现（待验证：bge 轴接入后启用）。
- 不修改生产 RTIS 行为：RTIS 知识栈以 Tool Manifest 指向其现有 CLI/stdin 入口（`rt.py`、`rtis_kb.mjs`）的方式 Adapter 接入（工具盘点:161），Broker 只做进程编排与输出结构化。
- 不直接写生产库：红线「不写 migrations/ 下的 platform*.db」全程生效（架构盘点:144）；Broker 自有表走新增迁移文件，只应用到开发库副本。
- 不绕过 Application API 唯一入口原则（计划:117）：Web/MCP/CLI 的上下文能力都走 Broker 公开接口，不各自查库。现状 apps/server 的 handler 直连 SQL 是已记录的最大架构债（架构盘点:115，§4 处置结论），Phase 4 不加重它。

---

## 2. 总原则

1. 按需加载，预算封顶：任何一层内容进入上下文前先过预算闸（计划:121、:444）。
2. 可解释：每一个被选中的知识、工具、插件都携带 `reason` 与命中信号，能回答「为什么是它」（计划:794）。
3. 可追溯：每个条目带来源、版本、内容哈希；引用通过 `(manifest_id, item_id)` 贯穿工具调用与证据（计划:430；工具调用对象含 `context_refs`，计划:219）。
4. 证据不丢：原始证据与 Artifact 不因压缩或驱逐消失，被驱逐的只是「正文副本」，引用与哈希保留（计划:474「原始证据不能因上下文压缩而丢失」）。
5. 不可变清单：Manifest 一经签发只追加引用事件，不回写；重新装配产生新 `manifest_id`。
6. 失效靠版本不靠猜：缓存键含版本与内容哈希，治理状态跃迁、索引重建、清单版本变化即失效（详见 §7）。
7. 审计旁路：Broker 的装配、展开、驱逐事件走 audit-engine `Emit`，写失败不影响装配（架构盘点:60，§2.3；MODULE_BOUNDARIES §8.2 第 4 条）。

---

## 3. 四级上下文：定义与加载时机

层框架取自计划 §8 表格（计划:437-443），逐层落到当前仓库的具体来源：

| 层级 | 内容（计划原文） | 加载时机（计划原文） | 本设计的具体来源与形态 |
|---|---|---|---|
| L0 | RIP 全局能力说明与核心规则 | 会话启动 | 规则包（rule pack）：版本化的 Markdown 与结构化规则文件，含工作区纪律、权限分级（架构盘点:152 提出的 T0 只读分级）、Artifact/Evidence 义务、红线（不装宿主机、不动 RTIS）。来源为随仓库分发的规则包目录，经 `rulepack.rs` 加载。不计入候选检索 |
| L1 | 插件、工具、知识包目录元数据 | 发现能力时 | 能力目录摘要：来自 Tool Registry 的 `list_tools` 类接口（Phase 1 验收「工具可发现」，计划:761）、插件 Registry 目录（Phase 6，先留接口）、知识源目录（platform 知识表经 search-engine、RTIS、ERI）。每条一行卡片（id、名称、族、一句话摘要、健康与状态），只给目录不给正文 |
| L2 | 匹配插件摘要、工具说明、知识目录 | 任务规划时 | 经 §4 意图匹配后的候选摘要集：知识条目摘要（标题、来源、等级、要点）、工具 brief（§8 定义的三级工具说明的中间层）。预算大头在这一层做取舍 |
| L3 | 完整文档、代码、证据、工具 Schema | 实际需要时 | 按需展开：完整知识正文、工具 input/output JSON Schema 与用法文档（对应 Tool Manifest `context.docs` 字段，计划:207）、Artifact 内容、Evidence 原文。Agent 通过 `expand` 接口显式请求，每次展开记账（§9） |

加载时机的平台触发点（设计）：

- L0：Agent 会话创建（Phase 7 Agent Engine 的 `Spawn`）或外部 AI 首次 MCP 握手时装配一次，会话内复用。
- L1：任务规划开始时装配一次目录摘要；MCP 场景对应 `rip.list_tools` 类分页目录（计划:645，且计划:658 明确 MCP 初始化不暴露完整工具说明）。
- L2：意图识别与匹配完成后、Agent 产出计划前装配。
- L3：Agent 决定调用某工具或引用某知识正文时逐条展开。

与现有资产的对应关系：L2/L3 的知识检索复用 search-engine 公开接口 `SearchEngine::search`（架构盘点:75，§2.5 公开接口实测）；中文分词与同义词扩展复用 resource-engine 公开 tokenizer 函数 `expand_keywords / tokenize / keyword_hits` 与 ALIASES 单一同步源（架构盘点:68，§2.4）；RTIS 知识经 `rtis_kb.mjs` stdin JSON 或 `rt.py poc search` 只读获取（工具盘点:159，运行方式实测记录）。

---

## 4. 任务意图识别与能力匹配流程

### 4.1 意图表示（设计）

意图对象 `IntentQuery` 为结构化槽位，不依赖自然语言理解：

~~~text
IntentQuery {
  text_normalized: String        // 归一化文本（复用 tokenize 口径）
  families: Vec<ToolFamily>      // 候选工具族，取值即计划 §4.2 的 13 族（计划:148-162）
  knowledge_intents: Vec<...>    // 检索/评估/来源核验/反知识核查（计划:159 Knowledge 族典型能力）
  target_slots: Vec<Slot>        // domain / ip / url / apk / binary / document / code_repo / none
  task_verbs: Vec<String>        // 收集/解析/检索/评估/比对/监控 等
  constraints: Vec<String>       // 只读、离线、超时上限等
  confidence: f32                // 0 到 1
}
~~~

### 4.2 流程（设计，五步全确定性）

1. 归一化：文本小写化，按 resource-engine tokenizer 口径分词并做 ALIASES 同义扩展（架构盘点:68）。
2. 槽位抽取：词典与正则规则抽 target 类型（域名/IP/URL/APK/哈希样串等）与任务动词；规则包版本化，随 L0 一起管理。
3. 意图分类：规则打分映射到 13 工具族与知识意图的组合，输出 families 与 confidence。映射表（族、触发词、示例）是版本化配置而非硬编码。
4. 低置信回退：confidence 低于阈值（默认 0.5）时，装配结果标记 `intent_status="ambiguous"`，附 top3 候选意图，由 Agent 或用户确认后重新装配；Broker 不静默猜（对应计划:444「防止不相关插件进入上下文」的保守面）。
5. 审计：意图事件带 `correlation_id` 走 audit-engine `Emit`（架构盘点:60），使 Trace 能回放「任务、意图、上下文、工具、证据」链（架构盘点:60，§2.3 的关联骨干能力实测存在）。

### 4.3 能力匹配与排序（设计）

候选来自三类源，统一为 `ContextSource` trait（§10）：

- 工具：Registry 按族与标签查询（Phase 1 Registry 接口；Registry 未落地期间用 Mock 源，先例是 Phase 1 自己的 Mock Runner，计划:759）。
- 知识：platform 库知识面经 search-engine `SearchEngine::search`（架构盘点:75）；红队知识经 RTIS Adapter（`rtis_kb.mjs` / `rt.py`，只读，工具盘点:161）；ERI 来源核验经 `eri_adapter.py` 链路（工具盘点:183，T10）。
- 插件：Phase 6 前返回空集，接口保留。

统一打分（设计，权重进策略文件可配）：

~~~text
score = w1*family_match + w2*quality + w3*health + w4*recency + w5*past_success
默认：w1=0.40, w2=0.25, w3=0.20, w4=0.10, w5=0.05
~~~

- `quality`：知识用 QualityGrade A/B/C/D（架构盘点:46，§2.1 domain 实测）；工具用 Registry 健康与验证状态。
- `past_success`：取 tool_execution 历史成功率；该表当前 0 行空结构（架构盘点:218，§12 差距清单第 3 条），Phase 4 该项恒为 0，不影响排序，只留通路（待验证：首批 Run 数据积累后校准）。
- 过滤（一票否决，不可被分数翻盘）：
  - 治理轴 QUARANTINED / REJECTED / ARCHIVED 不可见（对齐 resource-engine resolve 的数据边界，架构盘点:67，§2.4）。
  - 工具/插件状态不在 ENABLED 之后的状态机段不进候选（状态机 DISCOVERED 到 ACTIVE，计划:413）。
  - 知识源命中「重复、低质量、过期、无来源、损坏、冲突、许可证未知」处置标记的不进候选（计划:431 知识库职责）。
- 可解释性字段：每个候选保留 `matched_signals[]`（命中的关键词、同义词、槽位、族匹配），装配后原样写入 Manifest item 的 `reason`（满足计划:794「可解释」验收）。

### 4.4 与知识层联动（对齐计划示例）

计划 §9 给出的「检索知识与历史案例、发现工具包、动态加载分析方法、对照案例」八步联动（计划:478-488）在 Phase 4 的落点：第 1、2、4 步由 Broker 承担（意图、候选、按需展开），第 3、5 步由 Agent 经 Tool Engine 执行，Broker 不执行工具。

---

## 5. Token 预算模型

### 5.1 预算来源与分项（设计）

总预算 `B` 取自 AgentDefinition 的 `context_policy` 字段（计划:699 数据模型行）或任务级覆盖；策略文件版本化（`policy_version`），默认值如下：

| 分项 | 占比或上限 | 内容 |
|---|---|---|
| l0_rules | min(2000 tokens, 5% B) | 规则包，永不驱逐 |
| l1_catalog | 余量的 8% | 能力目录卡片 |
| l2_knowledge | 余量的 25% | 知识候选摘要 |
| l2_tools | 余量的 10% | 工具 brief |
| l3_reserved | 余量的 27% | L3 按需展开的动态额度池 |
| memory_history | 余量的 15% | 任务记忆/历史摘要（Phase 7 接入，先留分项） |
| reserve_output | 余量的 15% | 输出保留，任何装配不得侵吞（硬下限 10%） |

分项间允许不挪用 reserve_output 的有条件借用（l3 空闲额度可借给 l2，反向不允许）；所有借用在 `budget_report` 中披露。

### 5.2 估算器（设计）

`est_tokens(text) = ceil(ascii_chars / 4 + cjk_chars * 1.0 + other_chars / 2)`。

- 这是近似值，未对照真实 tokenizer 校准（本会话未测量，待验证）。校准方式预留：用 `models/bge-small-zh-v1.5/tokenizer.json`（架构盘点:158，模型四文件实测存在）离线抽样对比后修正常数；或后续经 embedding worker 提供精确计数服务。
- 每个候选入库时预计算 `est_tokens`（摘要与正文分开算），装配期只做加法，不重复扫文本。

### 5.3 选择策略（设计，装配顺序即优先级）

1. 先装 L0（规则包，封顶内全量）。
2. L1 目录卡片按族聚合压缩（每族一行小结加超链接式 ref），超预算先截断低相关族并记录。
3. L2 候选按 score 降序装入：知识先装摘要形态；同 `content_hash` 只装一次（去重，计划:444「去重压缩」）；工具 brief 每条软上限约 120 tokens（brief 生成时超限即压缩，见 §8）。
4. L3 不预装正文；`l3_reserved` 只记账。`expand` 请求到达时按「当前剩余 l3 额度」放行或拒绝，拒绝时返回剩余额度与替代建议（读摘要）。
5. 驱逐顺序（超预算时）：score 最低的 L3 展开最先收回，其次 score 最低的 L2 知识摘要，再次 L2 工具 brief；L0 与 L1 目录摘要永不驱逐。每个被驱逐条目进 `budget_report.evicted[]`，附原因。
6. 降级链：L3 正文 > L2 摘要 > ref-only（只留引用与哈希）。降级不删除 Manifest item，只改 `included` 形态并记事件（证据引用不丢，对齐计划:474）。
7. 工具输出永不预装：Run 产出的正文以 Artifact 引用进上下文（计划:794 第三条验收），引用同样占极小额度的 ref 预算。

---

## 6. ContextManifest 与引用追踪

### 6.1 数据模型（设计草案，对齐计划 §12）

计划 §12 已给实体行：ContextManifest（task_id、selected_knowledge、selected_tools、budget，计划:703），并要求「未审计现有表结构前不要直接新增重复实体」（计划:706）。据此本设计只给草案，正式字段以 domain 审计后的迁移评审为准：

~~~sql
-- 005_context_broker.sql（草案，纪律同既有迁移：IF NOT EXISTS，幂等，架构盘点:140）
CREATE TABLE IF NOT EXISTS context_manifest (
  manifest_id    TEXT PRIMARY KEY,      -- ctxm-<ulid>
  task_id        TEXT,
  intent_json    TEXT,                  -- IntentQuery 快照
  policy_version TEXT,
  broker_version TEXT,
  budget_json    TEXT,                  -- 分项分配
  totals_json    TEXT,                  -- 分项实际占用、驱逐计数
  intent_status  TEXT,                  -- ok | ambiguous
  created_at     TEXT, schema_version TEXT
);
CREATE TABLE IF NOT EXISTS context_manifest_item (
  manifest_id  TEXT, item_id TEXT,
  layer        TEXT,                    -- L0|L1|L2|L3
  kind         TEXT,                    -- rule|tool|knowledge|plugin|artifact|schema|doc
  ref_id       TEXT, ref_version TEXT,
  source       TEXT,                    -- registry|platform-kb|rtis|eri|plugin:<id>|artifact-store
  title        TEXT, reason TEXT, matched_signals_json TEXT,
  est_tokens   INTEGER,
  included     TEXT,                    -- full|summary|ref-only|catalog-card
  content_hash TEXT,
  PRIMARY KEY (manifest_id, item_id)
);
CREATE TABLE IF NOT EXISTS context_ref_log (
  log_id INTEGER PRIMARY KEY AUTOINCREMENT,
  manifest_id TEXT, item_id TEXT,
  event TEXT,                            -- ASSEMBLED|EXPANDED|EVICTED|CITED_IN_TOOL_CALL|CITED_IN_EVIDENCE
  detail_json TEXT, created_at TEXT
);
~~~

纪律约束（全部继承自架构盘点 §6 实测的迁移纪律）：

- 只追加不更新：三张表无 UPDATE 路径；重装配是新 `manifest_id`。
- 幂等：`IF NOT EXISTS`，重复执行 no-op（对齐 001_init.sql:29-30 口径，架构盘点:140）。
- 应用目标：开发库副本与测试内存库；生产 `platform.db` 不写（红线，架构盘点:144）。迁移执行器复用 `crates/database` 的 `discover_migrations / migrate`（架构盘点:53，§2.2 公开接口实测）。
- 外键与索引在迁移评审时补全（计划:706 要求唯一键、外键、索引、删除语义明确）。

### 6.2 引用追踪闭环（设计）

- 向下：Agent 发起工具调用时，调用对象的 `context_refs` 填 `(manifest_id, item_id)` 列表（调用对象含 `context_refs` 是计划:219 原文）；Tool Run 记录沿用 `correlation_id` 关联（架构盘点:60）。
- 向上：Evidence 的 provenance 引用到 item（Artifact 与 Evidence 实体已在计划 §12 数据模型候选，计划:698、:704）。
- 回放：`Trace(correlation_id)` 现成能力按 event_id 回放链路（架构盘点:60），Phase 4 只需保证 Broker 事件都带同一 correlation_id。
- 完整性：每 item 的 `content_hash` 允许事后校验「当时引用的内容与现在库里的内容是否一致」，服务来源追踪与版本追踪（计划:430）。

---

## 7. 缓存与版本失效

### 7.1 缓存设计（设计）

- 存储位置：`<rip-data>/runtime/broker-cache/`，目录可配置不硬编码（目录先例与可配置要求来自计划:405-409）。
- 键：`sha256(kind, id, version, params, source_fingerprint)`，内容寻址。先例：worker runtime 的 `idempotency_key` 结果缓存用 `workers/.cache/<sha256>.json` 原子写（架构盘点:97，§3；工具盘点:19，worker_runtime.py:529-594 实测）。Broker 采用同构实现：临时文件加原子改名。
- 条目：`{key, kind, validation:{version, content_hash, source_fingerprint}, payload, created_at, policy_version}`。
- 分层 TTL（默认，可配）：L1 目录 10 分钟；L2 知识摘要 1 小时；工具 Schema 24 小时；RTIS 检索结果 10 分钟。TTL 只是兜底，主失效机制是 §7.2 的版本校验（读时比对 `validation` 字段，不一致即重建）。
- 不做负缓存；失败不落缓存。

### 7.2 失效触发器（设计，逐条对到已实测的版本信号）

| 触发源 | 版本信号 | 依据 |
|---|---|---|
| 工具清单变化 | tool manifest `version` 或镜像 digest 变化 | Manifest 字段实测于计划:180、:185 |
| 知识治理跃迁 | quality-engine 生命周期/治理轴状态变化，VERIFIED 行受守卫不可自动覆盖 | 架构盘点:81（§2.6 状态机与 VERIFIED 守卫实测） |
| 检索索引重建 | `INDEX_SCHEMA_VERSION` 变化或整库重灌（索引库全部表 `search_index_` 前缀、可删除重建） | 架构盘点:74（§2.5 实测） |
| RTIS 库重建 | `knowledge.db` 文件 mtime 与 size 变化（bootstrap.sh 从备份重建，库不入 git） | 工具盘点:155（28299264 字节实测）；架构盘点:123 |
| 插件禁用/卸载 | 插件状态机状态变化（禁用不删数据，卸载不删已入主库知识） | 计划:413-415 |
| 策略变化 | `policy_version` 变化使 L0 与预算相关缓存整体失效 | 本设计 §5.1 |
| 平台版本变化 | `broker_version` 变化（装配逻辑升级后旧缓存不信任） | 本设计 §9 |

### 7.3 与清单的关系（设计）

缓存只加速「读源」与「摘要生成」，不缓存装配结果本身；同一输入的装配结果确定性主要来自候选与策略版本，缓存命中与否不改变 Manifest 内容语义（`selection_hash` 字段记录所选集合指纹，便于排查）。历史 Manifest 永不受缓存失效影响（不可变原则，§2 第 5 条）。

---

## 8. 工具说明分层加载

计划依据：「工具说明也要分层。Agent 不需要每轮读取所有工具手册；先发现工具能力摘要，确定候选后再加载参数 Schema 和用法」（计划:446）；插件侧同构要求「只加载 Manifest 元数据；任务匹配后加载插件摘要；真正调用时才加载完整说明和 Schema」（计划:419-421）；MCP 侧「不要在 MCP 初始化时把所有工具的完整说明暴露给模型」（计划:658）。

三级定义（设计，字段来源对齐 Tool Manifest 示例计划:176-208）：

| 层 | 名称 | 内容 | 来源字段 | 大小约束 |
|---|---|---|---|---|
| tier 1（L1 用） | card | id、name、族标签、一句话状态（健康/版本） | manifest `id/name/kind/version` 与 Registry 健康态 | 每条约 30 tokens |
| tier 2（L2 用） | brief | `context.summary`（计划:206）+ input required 键名列表 + output 顶层键名列表 + 已知限制一行 | manifest `description`、`context.summary`、`input_schema.required`、`output_schema` 顶层键 | 每条约 120 tokens，生成时超限即压缩 |
| tier 3（L3 用） | full | 完整 input/output JSON Schema + `context.docs` 文档正文 + 资源要求与超时 | manifest `input_schema/output_schema/resources/context.docs` | 按需展开，计入 l3_reserved |

落地方式（设计）：

- tier 2 brief 在 Registry 登记入库时预生成并随 Manifest 存储装配期直接取，避免运行时解析 YAML/JSON Schema 的开销；生成器校验 required 键与 Schema 一致。
- 加载接口三件套：`tool_cards(filter)`、`tool_briefs(ids)`、`tool_schema(tool_id, version)`；第三个就是 L3 展开，走 `expand` 记账。
- MCP 映射：`rip.list_tools`（分页、分类，card 层）、`rip.get_tool_schema`（full 层，计划:645-646）；分页与按需发现在计划:658 已明文。
- Agent 可见性控制：Registry 维护的工具对 Agent 可见性标记（计划:249 Tool Center 条目）在 card 层过滤，不可见工具连目录都不出现。

---

## 9. 上下文装配输出契约（AssembledContext）

### 9.1 结构（设计）

Broker 对 Agent Engine（Phase 7）与 MCP 外部 AI（Phase 9）输出同一个结构化对象：

~~~json
{
  "schema_version": "1",
  "context_id": "ctx-<ulid>",
  "manifest_id": "ctxm-<ulid>",
  "task_id": "t-...",
  "created_at": "2026-11-09T00:00:00Z",
  "broker_version": "0.1.0",
  "policy_version": "policy-1",
  "intent": {
    "status": "ok",
    "families": ["Web / HTTP", "Knowledge"],
    "target_slots": ["domain"],
    "confidence": 0.82,
    "alternatives": []
  },
  "layers": {
    "l0": { "rule_pack": "rip-rules@1", "rules_ref": "rp-rules:1#core" },
    "l1": { "catalog_digest_ref": "catalog@snap-<id>", "counts": { "tools": 12, "knowledge_sources": 3, "plugins": 0 } },
    "l2": {
      "knowledge_summaries": [
        { "item_id": "ki-1", "ref_id": "kb:900008", "source": "rtis",
          "title": "...", "grade": "K3", "summary": "...",
          "content_hash": "sha256:...", "reason": "matched: kingdee; family=Knowledge" }
      ],
      "tool_briefs": [
        { "item_id": "tb-1", "tool_id": "rip.http.inspect", "version": "1.0.0",
          "summary_ref": "registry:brief:rip.http.inspect@1.0.0",
          "required_input": ["url"], "reason": "family=Web / HTTP; slot=domain" }
      ]
    },
    "l3_refs": []
  },
  "citations": [
    { "item_id": "ki-1", "source": "rtis", "ref_id": "kb:900008", "version": "...",
      "content_hash": "sha256:...", "uri": "rtis://poc/900008" }
  ],
  "budget_report": {
    "total_tokens": 8000,
    "allocated": { "l0": 400, "l1": 608, "l2_knowledge": 1900, "l2_tools": 760, "l3_reserved": 2052, "memory": 1140, "reserve_output": 1140 },
    "used": { "l0": 400, "l1": 512, "l2_knowledge": 1210, "l2_tools": 340, "l3_reserved": 0, "memory": 0, "reserve_output": 0 },
    "evicted": []
  },
  "warnings": [],
  "audit": { "correlation_id": "..." }
}
~~~

### 9.2 契约规则（设计）

1. L3 内容不内联：`l3_refs` 初始为空，Agent 用 `expand(manifest_id, item_id)` 逐条取正文或 Schema；每次展开写 `context_ref_log` 并从 `l3_reserved` 扣账，返回增量 tokens。
2. 摘要与引用分离：`l2` 内是摘要，`citations[]` 是权威引用集（source、version、content_hash、uri 齐全），任何下游（工具调用、证据、报告）只允许引用 citations 里的条目。
3. 不可变与可回放：装配结果一经签发不改；重装配出新 `context_id/manifest_id`。
4. 空结果也是合法结果：候选为空时返回带 `warnings` 的空装配（说明查了什么、为什么空），不抛错；`intent_status="ambiguous"` 时附 `alternatives` 等确认。
5. 大小上限：整个 AssembledContext 的内联部分必须落在 `budget_report.used` 之内，序列化后校验一次，超限即装配失败（fail fast，不允许静默超预算）。

---

## 10. crates/context-broker 模块设计

计划 §13 已把 `context-broker` 列为分批新增 crate 之一（计划:719），并要求每个新 crate 有清晰职责、公开接口、错误类型、测试和依赖方向（计划:728）。

~~~text
crates/context-broker/
  src/lib.rs          职责声明 + re-export（crate 头惯例对齐现有六 crate，架构盘点:41）
  src/intent.rs       归一化、槽位、分类（纯函数，零 IO）
  src/policy.rs       预算策略加载与校验（版本化）
  src/estimate.rs     token 估算器（纯函数）
  src/source.rs       ContextSource trait + Candidate/Ref/Content 类型
  src/sources/rulepack.rs     L0 规则包源
  src/sources/registry.rs     工具目录源（Phase 1 Registry 接口；未落地期用 Mock，先例计划:759）
  src/sources/knowledge.rs    platform 知识源（经 search-engine）
  src/sources/rtis.rs         RTIS 只读 Adapter 源（子进程 rtis_kb.mjs / rt.py，超时与单行协议）
  src/sources/artifact.rs     Artifact/Evidence 引用源（Phase 2 产物存储）
  src/sources/plugin.rs       插件源（Phase 6 前空实现）
  src/assemble.rs     选择、去重、驱逐、AssembledContext 构造
  src/manifest.rs     三表持久化 + context_ref_log
  src/cache.rs        内容寻址缓存（原子写）
  src/error.rs        ContextBrokerError（对齐各 crate 自有错误类型惯例）
  src/bin/rp-context.rs  CLI：assemble / expand / manifest-show / cache-stats
  tests/              intent 纯函数、预算、装配、缓存失效、验收三测试
~~~

依赖方向（对齐 MODULE_BOUNDARIES §8.2，本次实际读取）：

- 依赖：`domain`、`database`、`audit-engine`（事件出口）、`search-engine`（知识检索；先例：search-engine 已依赖 resource-engine，engine 间公开接口依赖有实测先例，架构盘点:41）。
- 端口隔离：Tool Registry 与 Artifact 存储经 `source.rs` trait 接入，Phase 4 编译不硬依赖后续 crate；engine 之间不互读库，只走接口或事件（MODULE_BOUNDARIES §8.2 第 2 条）。
- 头部约束沿用 `#![forbid(unsafe_code)]` 与 `#![deny(missing_docs)]`（六 crate 现状，架构盘点:41）。

公开接口（设计签名，Phase 1 Registry 落稿后对齐，待验证）：

~~~rust
pub trait ContextSource {
    fn id(&self) -> &str;
    fn candidates(&self, q: &IntentQuery, limit: usize) -> Result<Vec<Candidate>>;
    fn load(&self, r: &ContentRef) -> Result<Content>;
}
pub struct Broker { /* sources, policy, cache, manifest store, audit */ }
impl Broker {
    pub fn assemble(&self, req: AssembleRequest) -> Result<AssembledContext>;
    pub fn expand(&self, manifest_id: &str, item_id: &str) -> Result<Expanded>;
    pub fn manifest(&self, manifest_id: &str) -> Result<ManifestView>;
}
~~~

---

## 11. 实施拆解（Phase 4 执行序）

依赖前提：Phase 1 的 Tool Contract 与 Registry（计划:753-761）提供目录与 Schema 接口；Phase 2/3 的 Run 与 Artifact 记录（计划:763-784）提供引用目标。Registry 未就绪的子步骤用 Mock 源先行（Mock 先例：计划:759）。

| 步骤 | 内容 | 完成判据 |
|---|---|---|
| M4.0 | 005 迁移草案（开发库）+ domain 内 Context 类型 + policy 加载 + estimator + 纯函数单测 | 迁移在开发库幂等执行两次无差异；estimator 有单测基线 |
| M4.1 | L0 规则包源 + L1 目录源（Mock Registry 起步）+ card/brief 生成器 | 12 个候选工具（工具盘点:193-204 总表 T01 起）可出目录卡片与 brief |
| M4.2 | intent 纯函数 + knowledge/rtis 只读源接入 + 打分与 matched_signals | 给定「kingdee HTTP 检查」类任务，rtis 源返回只读命中（命令口径同工具盘点:221 的 rt.py 冒烟） |
| M4.3 | assemble 全流程 + 三表落库 + expand 记账 + AssembledContext 契约校验 | 同一任务两次装配产生两个 manifest，selection_hash 一致可对账 |
| M4.4 | cache + 七类失效触发器 + CLI（rp-context） | 改一个工具版本号后 cache-stats 显示对应条目重建 |
| M4.5 | 验收三测试 + 文档（本设计补充实施差异） | 见下 |

Phase 4 验收到测试的映射（计划:794 三条）：

1. 「不默认塞入」测试：登记 12 个候选工具与 RTIS 全库（14717 条 POC 量级，工具盘点:155；架构盘点:129 标注两处口径有差，实际条数待验证），装配后断言上下文只含目录卡片与少量摘要，`used` 总量不超过预算。
2. 「可解释」测试：断言每个 item 的 `reason` 与 `matched_signals` 非空，Manifest 三表行数与装配报告一致。
3. 「Artifact 引用」测试：Mock Run 产出 Artifact 后，断言上下文中只有引用与哈希、无正文内联。

质量门：进既有 CI（rust job 的 `cargo test --offline --workspace` 与 clippy `-D warnings`，架构盘点:170 实测的 CI 命令），不新增独立标准。

---

## 12. 与现有资产的复用对照

| 现有资产 | 复用点 | 依据 |
|---|---|---|
| search-engine | L2/L3 知识检索、索引重建信号 | 架构盘点:73-76（§2.5 实测） |
| resource-engine tokenizer | 意图归一化与同义扩展 | 架构盘点:68（§2.4 实测） |
| quality-engine | 知识治理过滤与失效信号（VERIFIED 守卫） | 架构盘点:81（§2.6 实测） |
| audit-engine | Emit/Trace、correlation_id 链路骨干 | 架构盘点:60-62（§2.3 实测） |
| crates/database | 迁移执行器、连接池 | 架构盘点:53（§2.2 实测） |
| worker runtime 缓存先例 | broker-cache 的内容寻址与原子写 | 架构盘点:97；工具盘点:19 |
| RTIS kit | 红队知识源（Adapter，不改本体） | 工具盘点:161（复用建议原文） |
| ERI sidecar | 来源核验知识源（经 eri_adapter.py） | 工具盘点:183（T10） |
| Tool Manifest `context` 字段 | 工具三层说明的数据来源 | 计划:205-207 |
| 三份只读查询脚本 | L1 知识源目录的天然成员（T0 只读分级） | 架构盘点:152（§7 审计补充） |

不复用与不重复：不用 rp_server.py 的直连 SQL 模式做 Broker 数据访问（架构盘点:115 已判定为架构债）；不在 Broker 内重写检索或分词（全部走公开接口）；不把 knowledge.db 内容拷入平台库（知识库本体是内容资产，按架构盘点:128 的裁定作为知识源接入）。

---

## 13. 待验证清单

1. Token 估算常数未对真实 tokenizer 校准（本会话未测量）。
2. `ContextSource` 与 Registry 的对接签名以 Phase 1 TOOL-CONTRACT-DRAFT 落稿为准（该文件写作时点尚未存在：`ls docs/phase0/` 实测仅三文件）。
3. RTIS knowledge.db 实际条数（README 14717 与 rtis.py 头注 14664 两处口径有差，架构盘点:129），影响 §11 M4.5 测试一的规模口径。
4. tool_execution 历史 0 行，`past_success` 权重通路待真实 Run 数据校准（架构盘点:218）。
5. 向量检索轴接入时点与接口（bge worker 与 search-engine 未接通，架构盘点:76），Phase 4 预留不实现。
6. RTIS 源子进程调用的超时与失败语义需在 M4.2 实测（eri_adapter.py 的 5s 超时与三态回落是可参照的既有实现，架构盘点:124）。

## 14. 验证记录（本会话实际执行）

| 动作 | 命令/方式 | 结果 |
|---|---|---|
| 仓库基线 | `git log -1 --oneline`（research-platform/） | `022a717 rtis: 红队知识栈从 ~/.dsh/redteam 解耦迁入 integrations/rtis（自包含 kit）`；`git status --short` 显示未跟踪 `docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md` 与 `docs/phase0/` |
| 计划结构 | `grep -n "^#" docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md` | §8 在 425 行起，Phase 4 在 786 行起，§12 数据模型、§13 crate 清单定位确认 |
| 计划精读 | Read 计划 127-256、374-447、448-492、634-732、734-832、834-926 行 | §3/§4/§7/§8/§9/§11/§12/§13/§14/§15/§16/§17/§18 原文取得 |
| 架构盘点 | 全文 Read（224 行） | §2 各 crate 接口、§3 worker 缓存、§5 RTIS、§6 迁移纪律、§12 差距清单 |
| 工具盘点 | 全文 Read（238 行） | T01-T12 总表、RTIS Adapter 纪律、验证记录 |
| 模块边界 | `sed -n '/## 8.1/,/## 8.2/p'` 与 `/## 8.2/,/## 8.3/p`（legacy/architecture-2.0/MODULE_BOUNDARIES.md） | §8.1 十二模块表、§8.2 五条依赖规则取得；§8.1 无 context-broker 行，该 crate 名出自计划:719 |
| phase0 现状 | `ls docs/phase0/` | 仅 CURRENT-ARCHITECTURE-INVENTORY.md、TELEGRAM-ADMIN-UI-REUSE.md、TOOL-INVENTORY.md 三文件 |
| 测试执行 | 无（设计文档，无代码改动） | 未运行任何测试，本会话未跑 cargo/python 测试 |
