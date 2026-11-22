# CB-1 任务书：上下文折叠与装配稳定性（agent-engine 长跑转录折叠 B1 + 装配稳定性/缓存失效归因 B2 + 两级触发 B3——context-broker×agent-engine 结合部，零迁移）

- 日期：2026-11-22
- 状态：任务书已定稿（2026-11-22 侦察后落稿；实现批次按本任务书执行，实施走 Agent 主会话路径）
- 依据：[MA-DSH-REFERENCE.md](MA-DSH-REFERENCE.md) §6.2 设计输入（B1 :105 / B2 :106 / B3 :107 / B4 :108）+ §6.3 不借鉴面（:110-114）+ §6.4 落位批次 CB-1 立项行（:118）+ B4 登记观察池（:119）+ 任务书引用本页声明（:120）；[PHASE-ROADMAP.md](../phase0/PHASE-ROADMAP.md) §7 Phase 6（:407）/ §8 Phase 7（:466）领域归属；[MA-2-TASK.md](MA-2-TASK.md) §0 D2/D8 显式动作位先例 + MA-2 已收口现状（git d18b7dca）；侦察锚点 A1-A17 为 2026-11-22 本会话实测（MA-2 收口后 HEAD 行号逐一复核，行号以本档为准）
- 前置：MA-2 已收口（027 入链、caller_budget 面、with_control 装配在 rp-agent.rs:650-675）；028 迁移空位在但**本批零迁移**（D1）
- 设计裁决：本批特有裁决见 §0（不另立 ADR——设计增量可溯源至 MA-DSH-REFERENCE §6.2 与本档；如有分歧以规划文档为准并回改本档，MA-2-TASK 头注同款）

## 0. 设计裁决（本批特有，ADR 内嵌）

| # | 裁决 | 理由 |
|---|---|---|
| D1 | **折叠产物落 agent_task_memory（013 既有表），零新迁移，028 不立项**：每次折叠事件经 `TaskMemoryStore::append`（memory/task.rs:46）追加一行 Task Memory（memory_id 沿 `mem-t-` 生成器，round = 折叠覆盖到的轮次，content_json = 折叠出处载荷：折叠轮区间/折入步数/est_tokens 前后值/折前段 canonical JSON 的 sha256；summary = 确定性摘要词形 `transcript-fold:rounds={a}-{b};steps={N};tools={…};artifacts={…}`，无时间戳无进程内计数）。agent_run_step（014:79-99）保持只追加轨迹权威面**零重写零回填**；摘要可引用回查走既有 `TaskMemoryStore::list_by_run`（task.rs:134）+ rp-agent 新增 `run-folds` 子命令。**折叠函数本身纯确定性**（同转录前缀 → 同折叠视图/digest），resume 由重算兜底、持久化行作回查与漂移检测面 | 014:30-31 明文两表分工「agent_run_step 是控制面轨迹权威面……Task Memory 是记忆面（压缩/驱逐生命周期），两表独立演进」——折叠产物按语义归属记忆面；不建新表 = 不触十五文件链尾钉面（A16 实测清单）、不动迁移链（红线 2）；artifact 表 FK→tool_run（005:79）不收非工具产物，不走 artifact；「不建第二事实源」红线满足：折叠是记忆面派生视图，非轨迹改写 |
| D2 | **B3 两级触发 = 增长阈值软收缩 + 轮次间隔防抖，挂 agent_loop 轮界；既有硬护栏即兜底零改动**：`FoldConfig` 结构落 control.rs（`enabled: bool` 默认 **false**；`fold_growth_tokens: u64` 默认 12000——自上次折叠起转录 est_tokens 增量阈值；`fold_min_interval_rounds: usize` 默认 4——两次折叠最小轮距；`fold_hot_window_rounds: usize` 默认 2——最近 N 轮永不折叠热窗）；`RunControlConfig` 扩字段（Default 既有七字段值零改动，control.rs:114-125）。轮界判定（agent_loop.rs:673 `'rounds` 循环内、控制消费之后护栏判定之前）：增量 ≥ 阈值 **且** 轮距 ≥ 间隔 → 对热窗外最旧段软折叠；任一不满足不折叠（防抖负例可测）。**硬兜底 = 既有 max_rounds（:712-722）/ max_tool_calls（:773-779）/ caller 三闸（:726-729 起）逐字节不动**——折叠只缩规划器可见面，预算记账数真实步数（红线 5）。rp-agent `--transcript-fold` 为唯一显式动作位（默认关，MA-2 `--allow-parallel` D8 先例同款） | MA-DSH-REFERENCE:107 B3「增长间隔软提示 + preflight 硬兜底」映射：软 = 阈值触发折叠，硬 = 既有护栏不变；默认关 = 改写规划视图面按显式开启（折叠改变 planner 所见，默认开会威胁判据 5「既有测试零改动通过」的确定性——MA-2 allow_parallel_subtasks 默认 false 的最严缺省纪律同源 control.rs 注释） |
| D3 | **折叠挂点 = PlanContext 构造点（agent_loop.rs:780-789），折叠只改规划器可见投影**：`steps: Vec<StepRecord>` 本体（报告 :226 / 落库 :1823 / executed_keys / seen_hashes / resume 续接 :555）保持全量逐字节；`PlanContext.history`（:77）传入折叠投影——已折叠段替换为**每段一条合成 StepRecord**（tool_id=`transcript:fold`、tool_version=折叠版本常量、tool_run_id=空串〔SUBMIT_REJECTED 空串先例 014:73〕、status=`SUCCEEDED`〔ToolRunStatus 词形内〕、summary=D1 确定性摘要、input_json/三 ID=None/elapsed_ms=0）；未折叠热窗段原样透传。`Planner` trait 签名零改动（agent_loop.rs:111-114）。**轮界跨度表内建**：fresh 路径轮循环记录 round→steps 区间；resume 路径由 `StoredStep.round` 重建（state.rs:293 SELECT 已含 round 列）——StepRecord 结构体零改动 | B1 设计输入（:105）「transcript 按增长间隔折叠为分层摘要」；折叠语义只对「发给规划器的」生效，对位 billion-context「只缩发给模型的、从不改写客户端本地历史」（MA-DSH-REFERENCE:103 可逆性行）；合成行词形全部落在既有词表内（status ToolRunStatus 词形、tool_run_id 空串先例），不发明新词值 |
| D4 | **B2 装配稳定性 = 三断言 + reason 序确定性化**：①**同输入同装配**——同 AssembleRequest 两次 assemble：selection_hash 相等（既有确定性锚点 assemble.rs:365-389 指纹排序+sha256）+ L0 规则列表与 L1 catalog:digest（:238-241）载荷逐字节相等；②**最小差异重装配**——候选集仅增一条知识候选：装配 diff = L2 知识桶增量，L0/L1 前缀逐字节不变（对位 :106「摘要小区间写入保持前缀完整」）；③**同 session 相邻请求**——broker 无 session 词形（A9 grep 零命中），session 映射 = 同 task_id 的 run 链（fresh → pause → resume 同 goal_text 二次装配 agent_loop.rs:612-627 / retry 链同定义重放），e2e 断言相邻两次装配 selection_hash 相等。**已知非确定面修复**：`reason()` 零信号退化形态内嵌进程级 `SELECT_SEQ`（assemble.rs:856-864，:823 AtomicU64）→ 本批改为**装配内 installed 序号**（确定性），两形态口径文档（acceptance_explainable.rs:159-161）原文不动，序号语义=装配内选择序（跨请求可复现） | B2 设计输入（:106）「L0/L1 层确定性排序 + 最小差异重装配（同 session 相邻请求前缀不变量）」；排序既有确定性在位（:190-195 score 降序 + item_id 稳定序）无需改动，本批是**断言固化 + 唯一非确定面点清**；reason 序号语义变更登记 §8 前置输出（explain 下游消费者对账输入） |
| D5 | **缓存失效归因 = `get_with_attribution` additive API + 归因三出口**：cache.rs 新增 `CacheMissReason` 六值 enum（EntryMissing / VersionMismatch / ContentHashMismatch / FingerprintMismatch / PolicyVersionMismatch / TtlExpired）+ `get_with_attribution(…)` 返回 `(Option<String>, CacheMissReason)`——validation 三元组 :199 整体比对拆解为逐字段归因；**既有 `get`（:177-207）签名与行为零 diff**（薄包装丢弃 reason）。归因三出口：①`BrokerCache` 内嵌 std AtomicU64 六归因桶（Arc 共享跨源），`attribution_stats()` 快照 accessor；②rp-context 新增 `cache-attrib` 子命令（目录级静态归因报告：entries / 按 TTL 类过期计数 / policy_version 不匹配计数，JSON 输出，cache-stats :283 邻位）；③七类失效触发器映射表（cache.rs:17-28）逐触发器 reason 单测固化。**归因不进 AssembledContext.warnings**（上下文载荷抖动会破坏 D4 前缀不变量口径） | B2 设计输入（:106）「缓存失效归因报告（对位 /acp-cache 按 TTL/模型/密钥归因）」；现有 get 返回裸 Option 无归因（:177-207 实测）；计数器走 std 零依赖（红线 3）；warnings 通道已有源降级 warning（assemble.rs:186）语义承载「源不可用」，缓存 miss 是正常路径不注入上下文 |
| D6 | **est_tokens 单点复用**：折叠阈值计量、折叠前后 token 记账、折叠摘要 sizing 全走 `context_broker::est_tokens`（estimate.rs:76-81：TokenizerAdapter 在位走真实计数、缺席回落保守公式，单点承载）；agent-engine 已经依赖 context-broker（Cargo.toml），折叠计量经既有依赖面取用。CB-1 **不新增 tokenizer 面、不改 estimate.rs/tokenizer.rs**（P4-3 R2 6.6 既有面零 diff） | 词表侦察（A13）：est_tokens/TokenizerAdapter 是 P4-3 批落的真实计数面（tokenizer.rs:1-14 装载口径），CB-1 直接复用不重写不混淆；tokenizer 在位/缺席只影响数值不改变触发判定结构（adapter 进程内 OnceLock 单例 tokenizer.rs:22） |
| D7 | **零新依赖**：折叠/触发/归因全 std（Vec/AtomicU64/Instant/Ordering）+ 两 crate 既有 serde/serde_json；sha256 复用 tool-registry::hash（agent-engine 既有依赖）；Cargo.lock 399 恒定（S1/S7 双点复核）；agent-engine/context-broker 两 Cargo.toml 零 diff（agent-engine 无 tokio 全 std 同步面，折叠在同线程轮界执行，无并发原语新增需求） | agent-engine Cargo.toml 依赖面实测（A14）；折叠执行点在轮界主线程（agent_loop.rs:673 循环体内），无跨线程共享需求；MA-2 先例（std AtomicUsize + Instant 零新依赖）同款 |
| D8 | **边界（B4/workflow/PG/server 零进本批）**：B4 血缘回溯登记 Phase 6 检索面观察池（MA-DSH-REFERENCE:119，本批零实现）；workflow-engine 零 diff（node_agent.rs:87 AgentNodeDriver 端口注入 AgentLoop——agent 节点自动受益折叠面，无需改动）；migrations_pg 零改动；rp_server.py 零 diff（agent 只有读端点，折叠读面走 CLI）；WSW 面 / RTIS 面 / manifest 三表（008:44/:66/:97）/ context_citation_evidence（011:87）零 diff | MA-DSH-REFERENCE:118 CB-1 范围字面边界 = B1+B2+B3；:108 B4「登记不进 CB-1 必交付」；workflow agent 节点经端口复用 AgentLoop（node_agent.rs:2 端口注入注释），折叠落在端口之内即两侧同时受益，改 workflow 面 = 越界 |

## 1. 现状锚点（2026-11-22 本会话实测，A1-A17）

| 锚点 | 事实 | 出处 |
|---|---|---|
| A1 设计输入 | §6.2 四条借鉴落位表：B1 转录折叠（:105）/ B2 装配稳定性+缓存归因（:106）/ B3 两级触发（:107）/ B4 血缘登记观察池（:108）；CB-1 立项行「B1+B2+B3，context-broker×agent-engine 结合部，Phase 6/7 域」（:118）；不借鉴面「压缩面以库/模块形式落 agent-engine 侧」（:111） | docs/rip3/MA-DSH-REFERENCE.md:101-120 |
| A2 装配九步流程 | 一次 assemble：意图分类 → 预算分配 → 全源候选收集（源错误降级 warning）→ 五权重打分+否决 → 分桶选择（L0 全量→L1 单项→L2 降序装入→L3 引用）→ citations 分离 → selection_hash → 契约校验 fail fast → 三表落库；`Broker::assemble` 入口 :132；模块头注释 :1-23 | crates/context-broker/src/assemble.rs:1-23,132 |
| A3 排序与指纹确定性 | 分桶排序「分数降序，同分按 item_id 稳定序」（:190-195）；selection_hash 指纹 = (source,ref_id,ref_version,content_hash,included,est) 六元组 `fingerprint.sort()` 后 sha256，「同输入一致可对账」（:365-389，:15 模块注）；**已知非确定面**：`SELECT_SEQ: AtomicU64`（:823）进程级计数进 `reason()` 零信号退化词形 `source-order:{seq}+score`（:856-864，InstalledItem::summary :826 / ref_only :849 同源），不进 selection_hash 指纹 | assemble.rs:190-195,365-389,823,849,856-864 |
| A4 分层与预算 | L0 规则封顶内全量永不驱逐（:203-221）；L1 单项 catalog:digest = `catalog@sources:…;tools=N;…`（:223-271，digest 构造 :238-241）；L2 知识/工具桶降序装入 + content_hash 去重 + ref-only 降级 + 驱逐（:273-332，install_into_bucket :867 起）；L3 Artifact 引用 ref-only 上限 L3_REF_CAP=10（:54，:334-350）；citations 与摘要分离（:352-363）；契约校验 fail fast「序列化后 est_tokens 超总量即装配失败」（:510-522）；三表落库只追加（:523-605，SK-1 引用证据固化 :573-605） | assemble.rs:203-350,510-605 |
| A5 BrokerCache 形状 | 键 = `sha256(kind,id,version,params,source_fingerprint)`（:117-127）；条目 validation 三元组 version/content_hash/source_fingerprint（:48-56）读时逐字段比对（:199）+ policy_version 比对 + TTL 兜底（文件 mtime，:203-205/:237-250）；分层 TTL 四常量：catalog 600s / knowledge 3600s / schema 86400s / rtis 600s（:36-45）；put 原子写 tmp+rename（:135-173）；**「只加速读源与摘要生成，不缓存装配结果本身」（:14-15）**；七类失效触发器→validation 映射表（:17-28）；`get` 返回裸 Option 无归因（:177-207）；消费点：assemble.rs:672-686（TTL_SCHEMA）+ sources/registry.rs:20 | crates/context-broker/src/cache.rs:14-28,36-45,117-127,135-173,177-207,199,237-250 |
| A6 逐请求全新装配证据 | agent 路径：每 drive 装配一次（agent_loop.rs:612-627）+ ambiguous 确认回路重装配（broker_link.rs:92/:112）；workflow 路径：node_knowledge 每节点执行装配（node_knowledge.rs:62）；CLI 路径：rp-context assemble 逐次（rp-context.rs:194）；性能基线口径「每次迭代独立任务文本……测冷装配全路径」（assemble_bench.rs:5-9，P10-3 基线 assemble_ms 1.863 见 MA-DSH-REFERENCE:88 表头）；全仓 `Broker::new` 生产调用点四处（rp-agent.rs:231 / rp-context.rs:162 / rp-workflow.rs:1085 / node_knowledge.rs:158） | broker_link.rs:92,112；agent_loop.rs:612-627；node_knowledge.rs:62；rp-context.rs:194；assemble_bench.rs:5-9 |
| A7 转录增长面 | `drive()`（run/resume 共用执行尾 :598）：`let mut steps: Vec<StepRecord> = p.history.clone()`（:662）run 内全量累积；每轮构造 `PlanContext { history: &steps, … }`（:780-789，:785）——**规划器每轮可见全量转录，无截断无折叠**；`PlanContext.history: &'a [StepRecord]`（:77）；StepRecord 十字段（:117-139）；DriveParams.history Vec 续接（:254）+ resume 从 `persisted.iter().map(…record.clone())` 重建（:555） | agent_loop.rs:66-84,117-139,239-267,598,662,780-789 |
| A8 既有护栏 | max_rounds：RunControlConfig 默认 8（control.rs:118），轮界判定 + GUARDRAIL `budget_rounds`（agent_loop.rs:712-722）；max_tool_calls：LoopConfig 默认 4（:53/:60），轮界 `steps.len() >=` 判定（:773-779）；max_elapsed_ms / stall / drift / min_evidence_hint 既有七字段（control.rs:93-125 Default :114-125）；MA-2 caller 三闸在位（agent_loop.rs:605-610 解析 + :726-729 起总预算闸）；GUARDRAIL 词形常量（control.rs:59）+ actions 词表区（:41-60） | control.rs:41-60,93-125,114-125,118；agent_loop.rs:712-722,726-729,773-779 |
| A9 broker 无 session 词形 | context-broker src 全文 grep `session` 零命中；AssembleRequest 四字段 task_id/text/correlation_id/trace_id（assemble.rs:57-70）——「同 session 相邻请求」须映射到同 task_id run 链（D4.③） | assemble.rs:57-70；grep 实测 |
| A10 步轨迹持久化 | `RunStateStore::record_step` INSERT agent_run_step 十七列（state.rs:230-270，:244-248）；全量轨迹读面 `steps()` 按 seq 升序 SELECT 含 round 列（:273-296，:293）；DDL：UNIQUE(run_id,seq)（014:99）+ input_json 入参快照 + summary/error/elapsed_ms（014:79-97）；轨迹口径「agent_run_step 是持久化轨迹的权威面……Task Memory 是记忆面（压缩/驱逐生命周期），两表独立演进，互不引用」（014:25-31）；seq 分配 = steps.len()（串行 :982）/ steps.len()+ordinal（并行 :1466） | state.rs:230-270,273-296；migrations/014:22-31,69-99；agent_loop.rs:982,1466,1823 |
| A11 Memory 面与接线 | TaskMemoryStore：append（task.rs:46）/ get（:80）/ list_by_run（:134）/ compress_run（:155，ACTIVE→COMPRESSED content_json 置空 summary 留存）/ evict_run（:188，墓碑）；agent_task_memory DDL：memory_id/run_id FK/round/content_json/summary/lifecycle CHECK 三值（013:53-66）；MemoryStore 三存储统一入口（memory/mod.rs:30-53，task() accessor :46）；**agent_loop 的 memory 是 Option 可选接线**（agent_loop.rs:363 `memory: Option<MemoryStore>`，:386 None，:398 with_memory）；**rp-agent CLI 零 with_memory 调用**（grep 实测）；record_step_memory 失败降级 warning 不反压先例（:1832-1836 区） | memory/task.rs:46,134,155,188；memory/mod.rs:30-53；013:53-66；agent_loop.rs:363,386,398,1832-1836 |
| A12 结合部形状 | BrokerLink 是 agent×broker 唯一结合面：assemble_confirmed 含 ambiguous 确认回路（:85-130）/ expand 透传（:134-136）/ manifest 透传（:139-141）；「本层不读任何库表（Agent 禁自查库）」（:12-13 边界注）；**结合部折叠/压缩现状：两 crate src 全文 grep `fold\|折叠` 零命中；`compress\|压缩` 仅 agent-engine/src/memory/ 三 store（013 面）**——B1 是纯新增面 | broker_link.rs:59-142；grep 实测（A11 词表） |
| A13 词表占用 | `fold/折叠`：两 crate 零命中（词形空闲）；`compress/压缩`：agent-engine memory 面占用（TaskMemoryStore::compress_run task.rs:155 / EvidenceMemoryStore::compress_run evidence.rs:241 / ResearchMemoryStore::compress research.rs:352，lifecycle ACTIVE→COMPRESSED→EVICTED 词值 013:60-62）+ 契约「原始证据不能因上下文压缩而丢失」verify_run_memory_integrity（memory/mod.rs:17-18）；`summary`：StepRecord.summary（agent_loop.rs:134）/ KnowledgeSummary（assemble.rs:971）/ L2 摘要载荷——折叠摘要词形须带 `transcript-fold:` 前缀自别（D1）；est_tokens 单点（estimate.rs:76-81）+ TokenizerAdapter P4-3 面（tokenizer.rs:19-22,25-58,62-78，DEFAULT_TOKENIZER_DIR :19，EMBEDDING_TOKENIZER_DIR env 优先） | memory/task.rs:155；memory/evidence.rs:241；memory/research.rs:352；memory/mod.rs:17-18；estimate.rs:76-81；tokenizer.rs:19-22,62-78；grep 实测 |
| A14 依赖面 | agent-engine Cargo.toml：domain/database/resource-engine/tool-registry/tool-engine/context-broker/audit-engine/serde/serde_json，**无 tokio 无 async**（std 同步引擎）；context-broker Cargo.toml：domain/serde/serde_json/toml/tool-registry/resource-engine/search-engine/audit-engine/database/tokenizers 0.21.4（default-features=false）；Cargo.lock `[[package]]` 计数 399（grep 实测） | crates/agent-engine/Cargo.toml；crates/context-broker/Cargo.toml；Cargo.lock 399 |
| A15 领域归属 | Phase 6 = Plugin Runtime 与 Plugin Center（PHASE-ROADMAP.md:407）；Phase 7 = Agent Engine（:466，:476 Phase 6 软依赖）；P8-3 Agent 节点长任务执行（:535）；workflow agent 节点经 AgentNodeDriver 端口注入 AgentLoop（node_agent.rs:2/:87，agent_steps 投影 :189/:198）——折叠落 AgentLoop 内即 agent 直跑与 workflow Agent 节点两侧同时受益 | docs/phase0/PHASE-ROADMAP.md:407,466,476,535；workflow-engine/src/exec/node_agent.rs:2,87,189 |
| A16 测试面与链尾 | context-broker tests 十一文件（acceptance_artifact_ref/acceptance_explainable/acceptance_no_stuffing/assemble_bench/assemble_contract/cache_invalidation/catalog_sources/intent_pure/policy_estimate/rtis_source/tokenizer_calibration）；agent-engine tests 七文件（closed_loop_p7_2/ma2_parallel_budget/memory_contract_p7_3/migration_dev_copy_p7_3/migration_idempotent/run_control_p7_4/trace_correlation_p7_5）；cache_invalidation.rs 逐触发器用例（:128/:156/:177/:200/:223/:246 起）；acceptance_explainable reason 两形态口径文档（:159-161）；**迁移链尾钉面 = 十五文件**（实测 grep 逐一复核，钉 027/27 于断言或链尾水位：migrations_agent_data_plane.rs:101/:110 / migrations_context_broker.rs:116/:159 / migrations_tool_contract.rs:115/:159 / migrations_runtime_node.rs / backup_restore.rs / migration_idempotent.rs:87 `assert_eq!(user_version, 27)` / migration_dev_copy_p7_3.rs:76/:85/:93 / ma2_parallel_budget.rs 头注 001..027 / workflow migration_replay.rs:147 / rp-mcp-parity caller_auth.rs / plugin-runtime lifecycle_full_path.rs / wsw-message cas_e2e.rs:128-178 / wsw-runtime exchange_persist.rs:3/:85 / wsw-runtime intercept_queue.rs:47 / apps/server/selftest_ma2.sh:9/:80/:131/:210）——若 028 落地十五文件随更，**本批零迁移则十五文件零随更**（红线 2） | crates/*/tests 与 apps/server 实测逐一复核（grep `027` 全量 + 逐文件断言形态确认） |
| A17 MA-2 遗产接入点 | rp-agent.rs:650-675：`load_caller_budgets(&pool)` + `AgentLoop::new(…).with_control(RunControlConfig { caller_budgets, ..RunControlConfig::default() })`（结构体更新语法——FoldConfig 新字段经 ..default() 零改动兼容）；control.rs:133 `load_caller_budgets` 装配层读取面先例（引擎不直查配置表分层纪律）；Flags 区 :76-94（caller_id :91 / allow_parallel :92 邻位加 --transcript-fold）；run-steps 子命令轨迹读取面先例（:497 区） | rp-agent.rs:76-94,650-675,497；control.rs:133 |

## 2. 范围

### 2.1 允许改动（落点清单）

| 落点 | 类型 | 内容 | 步骤 |
|---|---|---|---|
| `crates/context-broker/src/assemble.rs` | 修改 | `reason()` 零信号退化词形序号源改装配内 installed 序号（D4；两形态口径与 acceptance_explainable.rs:159-161 文档不变）；**其余零 diff**（排序/指纹/L0-L3 流程/契约校验/三表落库全不动） | S2 |
| `crates/context-broker/src/cache.rs` | 修改 | `CacheMissReason` 六值 enum + `get_with_attribution` additive API（validation :199 整体比对拆逐字段归因）+ 六归因桶 AtomicU64 + `attribution_stats()` 快照；既有 `get`/`put`/`cache_key`/`stats` 签名与行为零 diff（get 改薄包装） | S2 |
| `crates/context-broker/src/sources/registry.rs` | 修改 | brief 生成缓存消费点切 `get_with_attribution`（归因计数自动入桶；读取行为逐字节不变） | S2 |
| `crates/context-broker/src/bin/rp-context.rs` | 修改 | 新增 `cache-attrib` 子命令（目录级静态归因 JSON：entries/expired 按 TTL 类/policy_mismatch；usage 文本 :37-42 更新；cache-stats :283 邻位） | S2 |
| `crates/agent-engine/src/control.rs` | 修改 | `FoldConfig` 结构（四字段 + Default 最严缺省）+ `RunControlConfig` 扩 `fold: FoldConfig` 字段（Default 既有七字段值零改动，:114-125）；词表区按需加折叠审计常量（:41-60 词形纪律） | S3 |
| `crates/agent-engine/src/agent_loop.rs` | 修改 | 折叠纯函数（steps + 轮界跨度表 → 折叠段/摘要/est 前后值，D3）+ 轮界跨度表内建（fresh 轮循环记录 / resume 由 StoredStep.round 重建）+ 轮界触发判定（D2，:673 循环内控制消费后）+ PlanContext 投影（:780-789 改传折叠视图）+ RunReport 加 `fold_events` additive 字段（:198-236 区）+ 折叠行落 agent_task_memory（经 TaskMemoryStore，失败降级 warning 先例 :1832 同款）+ resume 折叠漂移检测（重算 vs 持久化行不一致 → warning） | S3/S4 |
| `crates/agent-engine/src/bin/rp-agent.rs` | 修改 | `--transcript-fold` 旗标（默认关）+ `run-folds <run_id>` 子命令（TaskMemoryStore::list_by_run 过滤折叠行输出 JSON；run-steps :497 区先例）+ usage 文本更新（:60-66） | S5 |
| 测试文件 | 新增 | `crates/context-broker/tests/assemble_stability.rs`（D4 三断言）+ `crates/context-broker/tests/cache_attribution.rs`（六 reason 逐触发器 + 既有 get 行为不变负例）+ `crates/agent-engine/tests/cb1_transcript_fold.rs`（B1/B3 硬验收 e2e，ma2_parallel_budget.rs:1-17 harness 形态同款：内存库 + 真实 migrations 001..027 + 注入驱动）+ `apps/server/selftest_cb1.sh`（CLI 折叠实测：长跑折叠 + run-folds 回查，§5 收口） | S2/S6 |
| `docs/rip3/CB-1-BATCH-REPORT.md` + `docs/README.md` | 新增/additive | 批次验收报告（S7）+ 索引登记 | S7 |

### 2.2 不修改

- `migrations/` 001-027 全部零 diff（**028 不立项**，D1）；`migrations_pg/`；`schemas/`；十五文件链尾钉面零随更（A16 清单）。
- `crates/context-broker/src/estimate.rs` / `tokenizer.rs`（P4-3 est_tokens/TokenizerAdapter 既有面零 diff——D6）；`manifest.rs` 三表形状与只追加语义；`policy.rs` BudgetPolicy 七分项；`intent.rs` / `score.rs` 打分与权重；`error.rs` 词形（归因走返回值不新增错误变体）。
- `crates/agent-engine/src/state.rs`（record_step/steps 读写面零 diff——轨迹权威面不触碰，红线 1）；`store.rs`（agent_run 读写零 diff）；`tool_link.rs` / `evidence.rs` / `trace.rs`；`memory/` 三 store 与 mod.rs（折叠行经既有 append/list_by_run 公开 API，零 diff）；`LoopConfig` 既有字段与缺省 4（agent_loop.rs:53/:60）。
- `crates/workflow-engine/` 全 crate（node_agent 经端口自动受益，D8）；`crates/tool-engine/` / `crates/tool-registry/` / `apps/server/rp_server.py`（agent 读端点零 diff）；既有 selftest 脚本（selftest_phase7_agents.sh / selftest_ma2.sh 等）零改动。
- AssembledContext 序列化形状（schema_version="1" 冻结；fold 不进 context 载荷——D5 warnings 豁免同理）；ToolCallRequest JSON 冻结契约（MA-2 红线 3 延续）。
- `Cargo.toml` 依赖面与 `Cargo.lock`（399 恒定——D7）。
- 生产 `platform*.db` / `platform-dryrun.db` / `integrations/rtis/`（测试一律内存库 + 真实 migrations 目录，014:35-36 红线口径）。

## 3. 实施步骤（S1→S7 串行，每步独立可验证）

- **S1 门禁基线核验**：Cargo.lock 399；`git status --short` 干净（仅本批文档）；`cargo test --offline --workspace` 现状全绿记录（2026-11-22 新纪律：全量一次作基线）；A1-A17 关键锚点函数名重定位复核（漂移则按函数名记录新行号）；确认折叠默认关路径现状零回归基线（run_control/closed_loop/ma2_parallel_budget/memory_contract 全绿名单入报告）。
- **S2 context-broker 确定性与归因面**：reason 序确定性化（D4）→ `CacheMissReason` + `get_with_attribution` + 归因桶 + 快照 accessor（D5）→ registry 消费点切换（行为不变）→ rp-context `cache-attrib` 子命令 → `assemble_stability.rs` 三断言 + `cache_attribution.rs` 六 reason 单测落盘；**行为不变式：既有 get 调用方零改动全绿（cache_invalidation.rs 十一用例 + acceptance_* 五件零 diff 通过）**。
- **S3 折叠函数与 FoldConfig**：control.rs FoldConfig（Default 最严缺省，D2）→ agent_loop.rs 折叠纯函数（steps+跨度表→折叠视图/摘要/est 前后值；合成 StepRecord 词形 D3）+ 单测（同前缀同产物 / 热窗不折 / 词形断言）；本步不接线轮界，纯函数可独立验证。
- **S4 轮界接线与投影**：轮界跨度表（fresh 记录 / resume 重建）→ :673 循环内两级触发判定（控制消费之后、预算判定之前）→ PlanContext 投影切换（:780-789）→ RunReport.fold_events additive → 折叠行落 agent_task_memory（TaskMemoryStore，失败降级 warning）→ resume 重算 + 漂移检测；**行为不变式：fold.enabled=false 全链路逐字节现状**（跨度表记录为旁路零行为影响）。
- **S5 rp-agent CLI**：`--transcript-fold`（缺省关；开启时若无迁移面/内存库异常降级 warning 不炸 run）+ `run-folds` 子命令 + usage 更新；退出码 0/1/2 口径不变（rp-agent 既有 P9-3 形态）。
- **S6 e2e 硬验收测试面**：`cb1_transcript_fold.rs` 落判据 1-5 全部用例（长跑折叠 / 确定性 / 回查 / 两级触发负例 / 默认关零回归守卫）；判据 6-7 的 broker 侧用例已在 S2 落盘，本步联调补漏。
- **S7 收口**：§5 全部命令实跑（含 `cargo test --offline --workspace` 全量一次）+ clippy -D warnings + deny 双检 + lock 399=399 + `git status` 复核只含 §2.1 落点 + 批次报告落盘 + docs/README.md 登记 + 显式 add → commit → push → CI watch 至绿。

## 4. 完成标准（十项判据）

| # | 判据 | 验收 |
|---|---|---|
| 1 | **B1 硬验收①：长跑折叠实测** | e2e：N 轮模拟驱动 run（--transcript-fold 开 + 阈值/间隔配置），触发折叠 ≥2 次；折叠后规划器可见 history 长度 < 全量步数（合成折叠行替代）；`RunReport.steps` 全量不缩水；agent_run_step 行数 = 全量步数（权威面零重写实测）；数字（折叠前/后 planner 可见 est_tokens）入批次报告，禁模拟冒充 |
| 2 | **B1 判据②：折叠确定性** | 同转录前缀两次折叠（fresh 路径 vs resume 重算路径）折叠视图逐字节一致（digest sha256 相等）；持久化折叠行 vs 重算不一致 → warning 留痕（漂移检测负例可触发） |
| 3 | **B1 判据③：摘要可引用回查** | 折叠后 `rp-agent run-folds <run_id>` 输出折叠行 JSON（memory_id/round/summary digest/lifecycle）；TaskMemoryStore::list_by_run 可读；折叠行 compress_run 后 summary 词形留存（013 生命周期语义兼容实测） |
| 4 | **B3 判据④：两级触发** | 阈值未到不折叠（负例）；阈值到而轮距未到不折叠（防抖负例）；两者齐备触发软折叠；热窗段永不折叠断言；折叠开启下既有硬护栏（max_rounds/max_tool_calls）行为不变（负例回归：预算数真实步数不因折叠放宽） |
| 5 | 默认关闭零回归 | FoldConfig::default().enabled=false；不开启时全链路逐字节现状：既有全部测试零改动通过（context-broker 十一文件 + agent-engine 七文件含 ma2_parallel_budget 三硬验收）；RunControlConfig::default() 既有七字段值零改动（control.rs:114-125 断言） |
| 6 | **B2 判据⑥：前缀不变量** | 同输入两次 assemble：selection_hash 相等 + L0/L1 载荷逐字节相等；候选集单增一条 → diff 仅 L2 增量；同 task_id run 链相邻装配（fresh→resume 同 goal_text）selection_hash 相等；reason 序确定性化后 acceptance_explainable.rs 零改动通过 |
| 7 | **B2 判据⑦：缓存失效归因** | 六类 CacheMissReason 各有触发单测（七触发器映射表 cache.rs:17-28 逐条对得上）；既有 get/put 行为零 diff（cache_invalidation.rs 十一用例零改动通过）；rp-context cache-attrib JSON 输出（entries/expired/policy_mismatch 计数口径） |
| 8 | est_tokens 单点 | 折叠计量全走 context_broker::est_tokens；tokenizer 在位/缺席两口径仅数值差异、触发判定结构一致（estimate.rs:76-81 单点承载断言；estimate.rs/tokenizer.rs 零 diff） |
| 9 | 门禁全绿 | `cargo test --offline --workspace` 全量一次通过（2026-11-22 新纪律）+ agent-engine/context-broker 定向 `--test-threads=1` + clippy -D warnings + deny 双检 + Cargo.lock 399 恒定 + git status 只含 §2.1 落点 |
| 10 | 零迁移与权威面守卫 | 无 028（migrations/ ls 实测 001-027）；十五文件链尾钉面零随更全绿；agent_run_step / context_manifest / context_manifest_item / context_ref_log 四表 DDL 零 diff；折叠行只出现在 agent_task_memory（013） |

## 5. 测试面（收口命令）

```bash
# Rust 全量（2026-11-22 新纪律：清单式门禁对全量 CI 有覆盖差，workspace 一次必须跑）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace
# 本批两 crate 定向（串行口径）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline -p agent-engine -- --test-threads=1
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline -p context-broker -- --test-threads=1
CARGO_TARGET_DIR=/home/emo/rt3-target cargo clippy -p agent-engine -p context-broker --all-targets --offline -- -D warnings
# deny / lock
cargo deny check advisories && cargo deny check licenses
grep -c '^\[\[package\]\]' Cargo.lock   # 399
# server 面（既有，零改动回归）
bash apps/server/selftest_phase7_agents.sh   # agent 读端点回归（折叠读面不进 server）
bash apps/server/selftest_ma2.sh             # MA-2 既有 CLI 三硬验收回归（零改动通过）
# 本批新增：CLI 折叠实测（长跑折叠 + run-folds 回查，数字入批次报告）
bash apps/server/selftest_cb1.sh
```

## 6. 红线（八条）

1. **不建第二事实源**：折叠产物只落 agent_task_memory（013 记忆面）；agent_run_step（014 控制面轨迹权威面）零重写零回填零截断；折叠是记忆面派生视图不是轨迹改写（014:25-31 两表分工原文）；禁折叠独立建库建 schema。
2. **零迁移**：028 不立项；migrations/ 001-027 零 diff；十五文件链尾钉面零随更（A16 清单：migrations_agent_data_plane / migrations_context_broker / migrations_tool_contract / backup_restore / migration_dev_copy_p7_3 / migration_idempotent / ma2_parallel_budget / migrations_runtime_node / workflow migration_replay / rp-mcp-parity caller_auth / plugin-runtime lifecycle_full_path / wsw-message cas_e2e / wsw-runtime exchange_persist+intercept_queue / selftest_ma2.sh——现钉 027/27）；migrations_pg 零 diff。若实施中发现确需 028（本任务书判定不需要），**停批上报重开裁决**，不得自行落迁移。
3. **零新依赖**：Cargo.lock 399 恒定；折叠/触发/归因全 std（Vec/AtomicU64/Instant）+ 既有 serde/serde_json；sha256 走 tool-registry::hash 既有依赖；两 crate Cargo.toml 零 diff；S1/S7 双点复核。
4. **折叠默认关**：FoldConfig::default().enabled=false；未显式开启全链路逐字节现状；RunControlConfig::default() 既有七字段值与语义零改动（control.rs:114-125；MA-2 红线 1 同款最严缺省纪律）。
5. **折叠只改规划器可见面**：RunReport.steps / agent_run_step / executed_keys / seen_hashes / 重复检测与停滞检测输入全量不缩水；预算记账数真实步数（折叠不放宽任何护栏——D2 硬兜底即既有护栏）；RunReport 新增仅 additive 字段 fold_events。
6. **词表纪律**：折叠词形 `transcript-fold` / `fold_events` / `CacheMissReason` 六值；禁占用 compress/evict（013 lifecycle 词形 ACTIVE/COMPRESSED/EVICTED 与 TaskMemoryStore::compress_run/evict_run）/est_tokens/summary 既有词形（折叠摘要以 `transcript-fold:` 前缀自别）；审计词形沿 control.rs:41-60 词表区；status/tool_run_id 合成词形落既有词表内（D3）。
7. **生产库零触碰 + 边界**：测试一律内存库/临时库；platform*.db / platform-dryrun.db / integrations/rtis 零 diff（014:35-36 红线）；B4 血缘（MA-DSH-REFERENCE:119 观察池）/ workflow-engine / rp_server.py / WSW 面 / RTIS 面 / PG 面全部不进本批（D8；越界即停批上报）。
8. **契约冻结**：AssembledContext schema_version="1" 与序列化形状零 diff（折叠不进 context 载荷）；manifest 三表只追加语义零 diff；BrokerCache 既有 get/put/cache_key/stats 公开签名零 diff（归因走 additive get_with_attribution）；ToolCallRequest JSON 冻结契约零 diff（MA-2 红线 3 延续）。

## 7. 回滚

- 单 commit 交付；`git revert` 整批撤销。**零迁移** → 无列残留无表残留无链尾随更，revert 即完全回滚（相对 MA-2 的 027 残留面，本批回滚面为零）。
- 折叠默认关使 revert 前行为即现状：fold.enabled=false 全链路逐字节现状（判据 5）；revert 后 agent_task_memory 中已落折叠行残留（ACTIVE 生命周期行，013 压缩/驱逐生命周期可正常处理，无害；登记说明不清理）。
- context-broker 侧 reason 序变更随 revert 恢复 SELECT_SEQ 口径（历史 manifest 行 reason 原文不动——只追加语义天然免回填）；get_with_attribution 纯 additive，revert 后零残留。

## 8. 交付物

1. agent-engine 折叠面：control.rs FoldConfig + agent_loop.rs 折叠纯函数/轮界两级触发/PlanContext 投影/RunReport.fold_events/resume 漂移检测 + rp-agent `--transcript-fold` 与 `run-folds` 子命令。
2. context-broker 稳定性与归因面：assemble.rs reason 序确定性化 + cache.rs CacheMissReason/get_with_attribution/归因桶/attribution_stats + registry 消费点切换 + rp-context `cache-attrib` 子命令。
3. 测试：`cb1_transcript_fold.rs`（B1/B3 硬验收 e2e，判据 1-5）+ `assemble_stability.rs`（判据 6）+ `cache_attribution.rs`（判据 7）+ `apps/server/selftest_cb1.sh` CLI 实测（判据逐项有实测数字）。
4. `docs/rip3/CB-1-BATCH-REPORT.md`（命令原文 + 数字实测 + 坏消息单列）+ docs/README.md 登记。
5. **MA-3 / Phase 6 前置输出登记**：①折叠事件词形与 FoldEvent/折叠行 content_json JSON 形状（Phase 6 检索面沿父链回查的折叠摘要引用输入——B4 血缘观察池对接面）；②CacheMissReason 六词形与 cache-attrib JSON 输出形状（provider 侧 prompt-cache 归因面参考，对位 B2 /acp-cache）；③reason 序语义变更说明（装配内选择序替代进程累计序——explain 下游消费者对账输入）；④折叠阈值/间隔/热窗参数面备忘（MA-3 子代理嵌套 run 的转录面继承输入）。
