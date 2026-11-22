# MA-2 任务书：并发派发引擎（agent-engine 并行子任务预算化放开 + per-caller 并发/速率/预算帽 Rust 强制 + tool-engine FamilyLimiter per-caller 扩展 + caller 归属列 SQLite 027）

- 日期：2026-11-22
- 状态：任务书已定稿（2026-11-22 侦察后落稿；实现批次按本任务书执行，实施走 Agent 主会话路径）
- 依据：[MULTI-AGENT-ACCESS-PLAN.md](MULTI-AGENT-ACCESS-PLAN.md) §三.3「低并发纪律升级为预算化并发」（:36）、§四 MA-2 行（:46）、§六 禁止事项（:58-60）、§八 完成定义（:76）；[MA-DSH-REFERENCE.md](MA-DSH-REFERENCE.md) §一 R3/R5/R7（:15/:17/:19）+ 反面清单（:23-27）+ §三 设计输入（:41）；[MA-1-TASK.md](MA-1-TASK.md) §1 A8 归属列预留（:32）+ §2.2（:58）；[MA-1-BATCH-REPORT.md](MA-1-BATCH-REPORT.md) §八 MA-2 前置输出（:120 起）；侦察锚点 A1-A16 为 2026-11-22 本会话实测（当前 HEAD 行号逐一复核，MA-1 落地后 rp_server.py 已 4725 行，行号以本档为准）
- 前置：MA-1 已收口（git 0b950345..fdd593f6）：026 链头在位、rp-admin 五子命令、server 多 key→caller 校验（rp_server.py:209-248/:4340-4362）、per-caller 审计 JSONL caller 三态
- 设计裁决：本批特有裁决见 §0（不另立 ADR——设计增量可溯源至规划文档；如有分歧以规划文档为准并回改本档，MA-1-TASK 头注同款）

## 0. 设计裁决（本批特有，ADR 内嵌）

| # | 裁决 | 理由 |
|---|---|---|
| D1 | **027 归属列四表一次到位**：`research_task` / `workflow_run` / `agent_run` / `tool_run` 各 `ALTER TABLE … ADD COLUMN caller_id TEXT`（可空，NULL=历史行不回填，018:87-92 手法）+ 三索引（ix_research_task_caller / ix_workflow_run_caller / ix_agent_run_caller / ix_tool_run_caller）；归属列**不建硬 FK**（017:49-50 workflow_id 口径同源：SQLite 不建硬外键，存在性由受理面把关）；同文件落 `caller_budget` 配置表（D2，该表 caller_id 建真 FK→caller，026:52 api_key 先例）+ schema_version=27 登记行 + `PRAGMA user_version = 27`（026:68-74 先例）。**写面分工**：agent_run.caller_id / tool_run.caller_id 本批接线（D4）；workflow_run.caller_id 与 research_task.caller_id 本批**落列零写面**（写面随 MA-3 节点角色绑定与 RTIS 派发面接线——MA-1-TASK:58 承诺的是归属列结构，计划 :46 文本同） | 计划 :46「ResearchTask caller 归属字段（SQLite 027）」+ MA-1-TASK:32 A8「tool_run 归属列 = MA-2 027」；agent_run 必需：跨 run 并发帽记账 = COUNT agent_run WHERE caller_id AND state IN (QUEUED,RUNNING)；tool_run 必需：速率帽窗口计数数据源（D5.2）+ MA-1 A8 预留；四表一次 ALTER 避免 028/029 反复动迁移链 |
| D2 | **per-caller 预算配置 = `caller_budget` 表**（027 新增，不 ALTER 026 两表）：caller_id PK FK / `max_parallel_subtasks`（fan-out 并发帽，**0 = 禁并行**，缺行同）/ `max_concurrent_runs`（跨 run 受理并发帽，0 = 不限）/ `rate_window_ms`（默认 60000，>0）/ `rate_max_steps`（窗口速率帽，0 = 不限）/ `budget_max_steps_per_run`（caller 级单 run 总步数帽，0 = 沿用 LoopConfig.max_tool_calls 既有）/ updated_at。**缺行 = 现状零回归**（并行关、其余帽不限）；写入口唯一 = rp-admin（D6）。语义不对称是有意的：并行是新增放开面（最严=关）；并发/速率帽是新增限制面（缺省不限=既有 caller 行为零变化，显式 budget-set 才收紧） | 计划 :36「并行度由 per-caller 预算面强制」；026:5-6「确需 ALTER 既有表，另立 027+」虽许可，但独立配置表保持 026 两表行形状冻结（rp-admin caller-add/key-gen 零改动）；零回归优先 |
| D3 | **并行支路升级（双支路，串行路径逐字节不动）**：agent_loop.rs:610-617 告警位升级为真实 fork-join 支路，沿 node_parallel.rs:1-19 语义先例（声明面并发度、引擎不放大、每子步独立 tool_run/agent_run_step 行、写面经 Pool 串行化、确定性顺序合并）。**放行三与条件**：`allow_parallel_subtasks=true` && `caller_id=Some` && `caller_budget.max_parallel_subtasks ≥ 2`——任一不满足走既有串行路径（含既有 warning 留痕）。并发度 = min(plan_steps.len(), max_parallel_subtasks)，**帽以上步骤排队不并行（帽语义，非拒绝）**；每步仍过既有步闸（能力绑定闸/重复检测/步预算）；cancel/pause 请求在 fan-out 前与子步派发间隙消费；步记录按 plan 顺序落 agent_run_step（seq 确定性，014:99 UNIQUE(run_id,seq) 不乱序）。attempt 语义：agent 侧 fan-out 子步 = 引擎侧 tool_run/agent_run_step 行记账（dsh 弱点 3 超越点，MA-DSH-REFERENCE:26）；workflow_run_node.attempt（017:94）仅作语义参照，**零新列**；R3 转派/接管全语义属 MA-3 | 计划 :25 缺口行「并行开关 + 预算化放开路径（不是裸开）」+ :46；R5 预检面（:17）；失败语义：任一子步失败按既有 fail-fast（P7-2）收敛，主线程收齐终态后统一落 |
| D4 | **caller 传递通道（全 Rust 参数面，JSON 冻结契约零改动）**：①`RunRequest` 加 `caller_id: Option<String>`（agent_loop.rs:147-166 字段区；None = legacy/default，行为零变化）；②`AgentStore::create_run` 签名扩参 caller_id（store.rs:271，trace_id Option 同款手法 :268-271），INSERT 扩列（:293-296），`StoredRun` 加 caller_id 读列（:44-49 先例，:315 run()/:330 SELECT）；③retry 链继承源 run caller（control.rs:181-209，:203 trace 继承同款）；④`ToolCallSpec` 加 caller_id（tool_link.rs:77-93）经 `ToolLink::call` → tool-engine 新 API `submit_for_caller`（lib.rs:292 submit_correlated 邻位）写 tool_run.caller_id（:256-270 INSERT 扩列）；⑤rp-agent.rs Flags 加 `--caller-id`（:76-94 字段区，默认 None）。**caller 不进 ToolCallRequest JSON**（request.rs:132 未知字段拒绝 + D-TC-06；caller 是运行时维度，计划 :60 禁止事项 3） | MA-1 server 侧 caller 解析已就绪（rp_server.py:209-248），本批补 Rust 派发面通道；server 对 agent 只有读端点（selftest_phase7_agents.sh:5-6 POST 405），REST/MCP 派发面属 MA-3（计划 :47），本批 server 零 diff |
| D5 | **三帽强制点与拒绝语义**（全部 Rust 强制，对位计划 :46「Rust 强制」）：①**并发帽（fan-out 内）**：超帽步骤排队（D3），审计 GUARDRAIL `kind=budget_caller_parallel`（control.rs:59 词形，json payload kind 区分 :578/:672 既有形状）——不硬拒；②**速率帽**：窗口（rate_window_ms）内该 caller 步数 ≥ rate_max_steps → **fan-out/单步前整轮硬停**：finish_reason=`guardrail: caller rate budget` + GUARDRAIL `kind=budget_caller_rate`，超帽步骤不执行；窗口计数 = tool_run.caller_id 行 COUNT（027 数据源）+ fan-out 线程组内进程内计数复核（std AtomicUsize + Instant，零新依赖）；③**总预算**：规划步数 ≥ budget_max_steps_per_run（>0 时）→ GUARDRAIL `kind=budget_caller_total` 硬停；④**跨 run 并发帽**：受理面（create_run 前）COUNT agent_run RUNNING by caller ≥ max_concurrent_runs → **硬拒**：AgentError 新变体 `BudgetDenied(String)`（error.rs:6-35 additive，match 臂随实现补全）。拒绝词形沿用 GUARDRAIL（:59）与九类信封 category 词表内映射（policy.rs:509-518），禁发明词表外 category | 计划 :46 硬验收「超预算硬拒」需要三个可测拒绝点（速率/总量/受理）；并发帽排队与 D3 帽语义一致（帽是节流不是拒绝）；审计词形沿 control.rs:40-60 actions 常量纪律（防词形漂移） |
| D6 | **rp-admin 扩展四子命令**：`budget-set` / `budget-show` / `budget-clear`（caller_budget 表唯一写入口，MA-1 D4/D6 写权分治延续）+ `caller-revoke`（幂等，重复 revoke 非致命——MA-1-BATCH-REPORT §8.2「MA-2 待补登记：caller 级停用 CLI」兑现）。手写解析 + usage 更新 + JSON 输出 + 退出码 0/1/2 口径不变（rp-admin.rs:9-13/:42/:93-97 既有形态）。**api_key.last_used_at 维持零写入**：本批消费面（Rust CLI）直传 caller_id 不经 key 明文，server 写入违 MA-1 D4 红线 3（server 零直连 SQL 写）——显式登记留后续批（不静默，MA-1-BATCH-REPORT:135 待补项第二件据此裁决） | MA-1 §8.4 前置输出两件待补项逐一对账；caller 停用即 budget 行随之失效（校验面 join 判定语义沿用 D1 026 口径） |
| D7 | **tool-engine per-caller 范围**：`FamilyLimiter` 扩展 `acquire_for(family, caller_id, caller_cap)` / `try_acquire_for(…)`（podman_host.rs:131-196 邻位，双层判定：族内 default_limit 既有逻辑 :159/:180 **零改动** + caller 顶层帽：同 caller 跨族在飞容器数 ≤ caller_cap）；`ContainerCall` 加 `caller: Option<&str>` + `caller_cap: Option<usize>`（:219-236）；挂点 :759-761 改用 caller 感知 acquire（caller None → 现行为逐字节不变）。**`PodmanHostConfig::default().family_concurrency_default = 1` 不动**（:83；core_utils_e2e.rs:528-534 断言「不为验收提速调高」零改动通过）——per-caller 帽是独立顶层，全局族基线不缩放。**native_rust / python_worker 两通道 caller 级工具帽本批零实现**（两通道现无并发闸；计划 :46 点名「FamilyLimiter 扩展」；两通道 caller 维度由 agent-engine 三帽 + tool_run.caller_id 记账覆盖，通道级帽登记后续批） | 计划 :46 文本边界；core_utils_e2e.rs:532 ROADMAP §4.5 discipline 4 断言口径保持；帽值解析点 = 装配层（rp-agent/agent-engine 启动时读 caller_budget 构建内存映射，经 ToolLink 传入），tool-engine 不直查 caller_budget 表（分层纪律：adapter/引擎层不做 SQL 配置查询） |
| D8 | **R5 预检面落法（派发前预估 + 显式动作位）**：fan-out 前预估（任务数 / 并发帽 / 窗口余量）写 RunReport.warnings + audit_event（留痕面，实测数据非提示）；「显式确认」语义 = `--allow-parallel` CLI 旗标（唯一人工显式动作位，默认缺省即串行）——rp-agent 为非交互 CLI，不做交互式确认流；人工审批流沿既有 workflow approval 节点（018 workflow_run_control），agent fan-out 交互式审批登记 MA-3 派发契约 | R5（MA-DSH-REFERENCE:17）「fan-out 计划先出预估（任务数/并发帽/预算），显式确认才拉起——预算面强制」；对位 WSW「启动前显示预估」纪律；CLI 形态下旗标即显式确认的等价面 |
| D9 | **RT-3 constraints 词表本批零改动**：policy.rs:733-754 build_constraints 词表三族不动；caller 帽不走 constraints 词表（constraints 表达 Manifest 执行约束，caller 帽是运行编排约束，层不同；帽值来源 = caller_budget 表非声明面）。计划 :70「Resolver 输出 constraints 扩 caller 维度」**登记为 RT-3 后续批观察项，非 MA-2 必交付**；DecisionInput 全显式参数形状（:357-403，D10c 零环境读取）即无返工扩展位，本批不触碰 | 避免 RT-3 在跑面回归风险；本批 caller 约束经 caller_budget/审计面表达已闭环（硬验收不依赖 constraints 词形）；扩展位留白不动即「无返工」 |
| D10 | **R7 异构模型成员 + PG 侧本批零实现**：①MA-2「子代理」= agent run 内步骤级 fan-out（tool steps）+ 跨 run agent_run 并行（caller 维度）；run 级「不同模型成员并行」由不同 agent 定义（各自 capability_refs/模型绑定走既有 provider 面）各自起 run 达成；成员级异构绑定（provider/model/reasoning_effort per member，R7 :19）登记 MA-3 建队面输入；②migrations_pg 零改动（现只到 V0006，V0007 = MA-3 knowledge 来源，计划 :47；caller 列 PG 镜像登记后续批）；③MCP/REST 派发契约、SSE 推送、TaskClaim、知识供体面全部不进本批（计划 :47 MA-3/:48 MA-4 范围） | 计划 :46 MA-2 主要交付三件的字面边界；R7 的 run 级语义已被跨 run 并行覆盖，成员级规格面依赖 MA-3 建队契约才可测 |

## 1. 现状锚点（2026-11-22 本会话实测，A1-A16）

| 锚点 | 事实 | 出处 |
|---|---|---|
| A1 并行开关现状 | `allow_parallel_subtasks`：声明注释「默认 false（低并发纪律，ROADMAP §8.5 风险 1）；false 时单轮多步骤一律串行执行并在报告留痕」；Default `false`（:89，「默认值即低并发纪律的最严面」:62）；单测固化 `default_config_disallows_parallel_subtasks`（:360-368，:362 断言） | crates/agent-engine/src/control.rs:76-78,89,62,358-368 |
| A2 唯一消费点 | `plan_steps.len() > 1 && !cfg.allow_parallel_subtasks` 只 push warning（"parallel subtasks disabled (low-concurrency policy): … serialized"），**不改变执行方式**；步骤在 `for step in plan_steps` 永远串行（:624）；全仓 grep `allow_parallel_subtasks` 仅此一处执行消费（其余 = 声明/默认/单测） | agent_loop.rs:610-617,624 |
| A3 AgentLoop 注入面 | AgentLoop 字段 `config: LoopConfig`（:262）/`control: RunControlConfig`（:263）；new() 用 `RunControlConfig::default()`（:286）；with_control() 注入（:292-295）；全仓 `with_control` 唯一调用点 = run_control_p7_4.rs:197（测试）——**rp-agent.rs 零调用，CLI 恒用最严缺省**且 Flags 无 caller/并行字段（:76-94） | agent_loop.rs:255-265,270-289,291-295；rp-agent.rs:76-94 |
| A4 预算面三件套 | max_rounds（GUARDRAIL kind=budget_rounds :578）；max_tool_calls（LoopConfig :43-48；round 级 :583-588 + step 级 :677-683）；max_elapsed_ms（0=不限，步间 :661-675，kind=budget_elapsed :672）。audit_emit 签名（action/run_id/trace_id/actor/detail :1189-1196）；GUARDRAIL 词形常量（control.rs:59）；actions 词表区 :41-60 | agent_loop.rs:570-581,583-588,661-675,677-683,1189-1196；control.rs:41-60 |
| A5 派发入口 | rp-agent Flags 全集（db/goal/task_id/input/budget/workspace/max_tool_calls 默认 4 :107/:136/rulepack/policy/caps/trace_id…）**无 caller 无并行字段**；migrate 先行（:183-186）；AgentLoop::new 六参构造（:639-649，无 with_control）；rp-server 对 agent 只有读端点、POST /api/agents 405，执行面在 Rust CLI | rp-agent.rs:76-94,183-186,639-649；apps/server/selftest_phase7_agents.sh:4-7 |
| A6 RunRequest/store/retry | RunRequest 字段区（agent_id/agent_version/goal/task_id/trace_id/run_id/input/planner…）无 caller（agent_loop.rs:147-166）；`create_run(&self, run, trace_id: Option<&str>)`（store.rs:271）INSERT agent_run 十列（:293-296）；StoredRun.trace_id Option 读列先例（:44-49，run() :315、SELECT :330）；retry 链继承 trace（control.rs:181-209，:203 `create_run(&replay, stored.trace_id.as_deref())`） | store.rs:271,293-296,44-49,315,330；agent_loop.rs:147-166；control.rs:181-209 |
| A7 AgentError 面库 | 八变体（Definition/DefinitionExists/RunNotFound/InvalidTransition/Plan/Broker/Tool/Storage）**无预算拒绝变体** | crates/agent-engine/src/error.rs:6-35 |
| A8 027 空位 | research_task 全列无 caller（002_tasks.sql:32-45）；workflow_run 全列无 caller（017_workflow_run.sql:44-70；:49-50「SQLite 不建硬外键，存在性由执行器受理面把关」口径）；agent_run 无 caller（006_agent_data_plane.sql:50-65）；tool_run 有 task_id/workspace_id/permission_ctx/budget_json（:48）无 caller（005_tool_contract.sql:34-65）；migrations 001-026 连续占用，**027 空位**；migrations_pg 只到 V0006 | migrations/002:32-45；017:44-70,49-50；006:50-65；005:34-65；ls migrations/ + migrations_pg/ 实测 |
| A9 可空列 ALTER 先例 | 018 六条 `ALTER TABLE workflow_run ADD COLUMN … TEXT`（:87-92）+ 索引（:94-95）+ workflow_run_node 两条（:100-101）；头注「SQLite 无 ADD COLUMN IF NOT EXISTS … ALTER 依赖迁移执行器 user_version 单次执行语义；同一库手工重复执行会报 duplicate column name，属预期守卫」（:50-53）；红线「只在开发库副本与测试内存库执行」（:55-56）；既有行不回填（:85） | migrations/018_workflow_control_correlation.sql:50-53,55-56,85,87-92,94-95,100-101 |
| A10 caller 域现状（MA-1 落地） | caller 表：caller_id CHECK `'caller-%'`（:38-39）/ permission_level T0-T2 CHECK（:41-42）/ revoked_at NULL=在用（:45-46）；api_key：key_hash sha256 hex 64 主键（:50-51）/ caller_id 真 FK（:52）/ last_used_at 登记列零写入方（:56-57）；文件头「确需 ALTER 既有表，另立 027+，禁改历史迁移」（:5-6）+「last_used_at 消费面留 MA-2 预算/限流批」（:23-24）；schema_version=26 + PRAGMA user_version=26（:68-74） | migrations/026_caller.sql:5-6,23-24,37-47,49-60,68-74 |
| A11 server caller 面 | `_resolve_caller_key`：sha256(请求 key)→api_key JOIN caller 吊销判定单行直查（mode=ro 零写）→compare_digest→返回 caller_id（:209-248）；require_auth caller 三态 default/caller_id/"-"（:4340-4362）；server 写面一律 subprocess 委派注释（:2540-2546）；server 零 caller 传递到 Rust 派发面（A5） | rp_server.py:209-248,4340-4362,2540-2546 |
| A12 FamilyLimiter 现状 | `FamilyLimiter { inner: Arc<LimiterInner> }`（:100-103）；LimiterInner { slots: Mutex<HashMap<String,Slot>>, default_limit } **单一全局值**（:105-108）；FamilyGuard Drop 统一释放（:115-129）；new(default_limit).max(1)（:133-140）；try_acquire 非阻塞（:143-146）；try_locked 锁内判定 `slot.active < inner.default_limit.max(1)`（:159）；acquire 阻塞 500ms Condvar 轮询（:172-195，判定 :180）；`family_concurrency_default = 1`（:68-69, :83，ADR §7 基线） | podman_host.rs:100-108,115-129,133-140,143-146,150-168,159,172-195,180,68-69,83 |
| A13 挂点与 API 面 | 生产路径唯一 acquire 点在 execute_container_via_engine 内：`let _family_guard = call.adapter.limiter.acquire(call.family);`（:759-761）；ContainerCall 全字段（engine/run_id/adapter/image/entrypoint/family :231/network）无 caller（:219-236）；`FamilyLimiter/FamilyGuard/ContainerCall/execute_container_via_engine` pub re-export（adapter/mod.rs:26-29） | podman_host.rs:759-761,219-236,231；adapter/mod.rs:26-29 |
| A14 tool-engine 受理/落库 | `submit`（lib.rs:161）/`submit_correlated`（lib.rs:292）；INSERT tool_run 十五列（lib.rs:256-270，无 caller）；Budget{timeout_seconds/max_output_bytes}（request.rs:42-47，解析 :171-185）；请求 JSON 未知字段拒绝「no secrets/host paths in ToolCallRequest, D-TC-06」（request.rs:132） | tool-engine/src/lib.rs:161,292,256-270；request.rs:42-47,130-133,171-185 |
| A15 RT-3 决策面 | DecisionInput 全显式参数形状（:357-403，has_budget :399-400，D10c 零环境读取）；build_constraints 词表三族（e2:* + catalog:* 直通 + permission_level，:733-754）无 caller 维度；RuntimeDecision.constraints :609（:593-614）；DerivedTarget/RejectOutcome 两支路均带 constraints（:616-629/:631-642）；decide 四返回路径（:1310 起：NotApplicable 短路 → R0-R5 Reject → Inconsistent → Assign；reject_decision 装配 :1581 起）；RejectCode 七码 :499-505 + 九类 category 映射 :509-518 | policy.rs:357-403,733-754,593-614,616-642,1310-1580,476-518 |
| A16 先例与测试面 | fork-join 先例：node_parallel.rs:1-19 口径（声明面出边并发度/每子节点独立线程独立行独立 checkpoint/写面 Pool 串行化）+ fork 派发 :48-60；测试：run_control_p7_4.rs:177-199 build_loop harness（:180 control 参数）、:364-369 e2e 用例；podman_host_budget.rs:242 起 family_limiter 并发基线单测；core_utils_e2e.rs:484-540（**:528-534「每工具族默认 1（不为验收提速调高）」断言**）；policy_resolver_matrix.rs / policy_resolver_manifests.rs 在位；**迁移链尾断言五文件**（migrations_agent_data_plane.rs:85,101,110；migrations_context_broker.rs:116,158；migrations_tool_contract.rs:115,158；backup_restore.rs:4,51；migration_dev_copy_p7_3.rs:76,93 + migration_idempotent.rs）现钉 026/26；selftest_auth.sh MA-1 多 key 块 #8（:71-80）既有零改动 | workflow-engine/src/exec/node_parallel.rs:1-19,48-60；tests 各处行号实测；Cargo.lock 399（grep 实测） |

## 2. 范围

### 2.1 允许改动（落点清单）

| 落点 | 类型 | 内容 | 步骤 |
|---|---|---|---|
| `migrations/027_caller_attribution.sql` | 新增 | 四表 ALTER ADD caller_id（D1）+ 四索引 + caller_budget 表（D2，CHECK 域与时间 GLOB 口径沿 024/025/026）+ schema_version=27 登记行 + PRAGMA user_version=27 + 文件头纪律注释（018:50-53 ALTER 单次执行语义 + 026:5-6 027 预留声明对账 + MULTI-AGENT-ACCESS-PLAN.md:46 引用） | S2 |
| `crates/agent-engine/src/agent_loop.rs` | 修改 | RunRequest.caller_id；:610-617 双支路（串行路径逐字节不动）+ fan-out 执行器（std thread；帽排队；速率/总预算闸；GUARDRAIL kind=budget_caller_* 审计；预估留痕 warnings；StepRecord 按 plan 序落） | S3/S4 |
| `crates/agent-engine/src/control.rs` | 修改 | RunControlConfig 装配扩展（caller 预算面字段/结构体；**Default 值与既有六字段语义零改动**，:81-92）；actions 词表区按需加并发帽排队/预算硬拒常量（:41-60 词形纪律） | S4 |
| `crates/agent-engine/src/store.rs` | 修改 | create_run 扩参 caller_id + INSERT 扩列 + StoredRun.caller_id 读列（:271/:293-296/:44-49/:315/:330）；retry 继承面（RunControl 在 control.rs:203 侧传参） | S3 |
| `crates/agent-engine/src/error.rs` | 修改 | AgentError 加 `BudgetDenied(String)`（additive；全仓 match 臂补全） | S4 |
| `crates/agent-engine/src/tool_link.rs` | 修改 | ToolCallSpec.caller_id + ToolLink.call 传参（:77-93/:127-141）；帽值解析器注入面（builder，D7 分层纪律） | S3 |
| `crates/agent-engine/src/bin/rp-agent.rs` | 修改 | `--caller-id` / `--allow-parallel` flags（:76-94/:96-153）；caller_budget 装配读取（启动一次性，D7）；usage 文本更新（:60-66） | S3/S4 |
| `crates/tool-engine/src/adapter/podman_host.rs` | 修改 | FamilyLimiter.acquire_for/try_acquire_for（双层判定，:131-196 邻位）；ContainerCall.caller/caller_cap（:219-236）；挂点 :759-761 改造（caller None 路径逐字节不变） | S5 |
| `crates/tool-engine/src/lib.rs` | 修改 | submit_for_caller API（submit_correlated :292 邻位）+ INSERT tool_run 扩 caller_id 列（:256-270） | S5 |
| `crates/tool-engine/src/adapter/mod.rs` | 修改 | re-export 更新（:26-29 面新增符号） | S5 |
| `crates/database/src/bin/rp-admin.rs` | 修改 | budget-set/budget-show/budget-clear/caller-revoke 四子命令（D6；:9-13 usage/:42 Mode/:93-97 文本区） | S6 |
| 测试文件 | 修改/新增 | `crates/agent-engine/tests/run_control_p7_4.rs`（扩 build_loop 用例）+ 新 `crates/agent-engine/tests/ma2_parallel_budget.rs`（三硬验收 e2e）；`crates/tool-engine/tests/podman_host_budget.rs`（扩 acquire_for 双层判定用例）；迁移链尾断言五文件随 027 更新（A16 清单，只改预期值不改结构）；`apps/server/selftest_ma2.sh` 新增（CLI 实测三硬验收） | S6 |
| `docs/rip3/MA-2-BATCH-REPORT.md` + `docs/README.md` | 新增/additive | 批次验收报告（S7）+ 索引登记 | S7 |

### 2.2 不修改

- `crates/tool-registry/`（caller 不进 Registry——计划 :60 禁止事项 3）；`crates/tool-engine/src/policy.rs`（constraints 词表与决策逻辑零 diff——D9）；`crates/tool-engine/src/request.rs`（冻结 JSON 契约零 diff——D4）。
- `apps/server/rp_server.py`（caller 解析已就绪本批零 diff；MCP/REST 派发面 MA-3）；既有 selftest 脚本（selftest_auth.sh 既有用例含 MA-1 #8 块、selftest_phase6.sh、selftest_phase7_agents.sh 零改动——新增 selftest_ma2.sh 是新文件）。
- `migrations/` 既有 001-026 零 diff（026:5-6 纪律）；`migrations_pg/`（D10）；`schemas/`；`crates/workflow-engine/`（fork-join 先例只读参照，本批零 diff；workflow_run.caller_id 写面 MA-3）。
- `PodmanHostConfig::default().family_concurrency_default = 1` 与 core_utils_e2e.rs:528-534 断言（D7——不为验收提速调高）。
- RP_RATE_LIMIT_RPM 全局限频、api_key.last_used_at 写入面（D6 显式登记）、RT-3 constraints 词表（D9）、LoopConfig 既有字段语义与 LoopConfig::max_tool_calls 缺省 4（A5）。
- `Cargo.toml` 依赖面与 `Cargo.lock`（399 恒定——并行/速率面全 std：thread/Condvar/AtomicUsize/Instant，零新依赖）。
- 生产 `platform*.db`/`platform-dryrun.db`/`integrations/rtis/`（新迁移只在开发库副本与测试内存库执行，018:55-56 红线）。

## 3. 实施步骤（S1→S7 串行，每步独立可验证）

- **S1 门禁基线核验**：Cargo.lock 399；`git status --short` 干净（仅本批文档）；`cargo test --offline -p agent-engine -p tool-engine -p database` 现状全绿记录；`bash apps/server/selftest_auth.sh` 全绿（含 MA-1 #8 多 key 块，判据 5 before 基线）；A1-A16 关键锚点函数名重定位复核（漂移则按函数名记录新行号）；rp-agent.rs:183 迁移先行确认（caller 列读写一律在 migrate 之后）。
- **S2 迁移 027**：`migrations/027_caller_attribution.sql` 落四 ALTER + 四索引 + caller_budget（D1/D2）；临时库实跑断言 user_version 升至 27 + 重跑零变更 + 手工重复执行报 duplicate column = 预期守卫（018:50-53 语义）；迁移链尾断言五文件随更（A16 清单，只改预期值与描述文本）。
- **S3 caller 归属与传递通道**：RunRequest.caller_id → store.create_run 扩参落 agent_run.caller_id → StoredRun 读列 → retry 继承（control.rs:203 同点传参）→ ToolCallSpec.caller_id → ToolLink.call → submit_for_caller 写 tool_run.caller_id → rp-agent `--caller-id`；**行为不变式：caller_id=None 全链路逐字节现状**（INSERT 列值 NULL、JSON 契约零 diff、现有测试零改动）。
- **S4 agent-engine 预算面与并行支路**：RunControlConfig 装配扩展（caller_budget 内存映射经 with_control/新 builder 注入，D7 分层）→ :610-617 双支路（放行三与条件 D3；串行路径零 diff）→ fan-out 执行器（帽排队/速率/总预算/cancel-pause 消费/fail-fast 收敛/plan 序落步）→ 受理面跨 run 并发帽（BudgetDenied，D5.4）→ GUARDRAIL kind=budget_caller_parallel/rate/total 审计 + 预估 warnings（D8）。
- **S5 tool-engine FamilyLimiter per-caller 扩展**：acquire_for/try_acquire_for 双层判定（族内既有逻辑 :159/:180 零改动）→ ContainerCall.caller/caller_cap → 挂点 :759-761 改造（None 路径不变）→ submit_for_caller + INSERT 扩列（与 S3 对接点，本步内联调）→ re-export 更新。
- **S6 rp-admin 四子命令 + 测试面**：budget-set/budget-show/budget-clear/caller-revoke（D6；JSON 输出 + 退出码口径 + revoke 幂等）；三硬验收 Rust e2e（判据 2/3/4）+ limiter 双层判定单测（判据 7）+ 链尾断言回归 + `apps/server/selftest_ma2.sh`（CLI 实测：rp-agent run N=3 计时对比 + caller 超预算拒收 + 双 caller 互不抢占，数字进批次报告）。
- **S7 收口**：§5 全部命令实跑（离线 + --test-threads=1 Rust 面）+ clippy -D warnings + deny 双检 + lock 399=399 + `git status` 复核只含 §2.1 落点 + 批次报告落盘 + docs/README.md 登记 + 显式 add → commit → push → CI watch 至绿。

## 4. 完成标准（九项判据）

| # | 判据 | 验收 |
|---|---|---|
| 1 | 027 迁移纯增量幂等 | migrate 首跑 user_version=27；重跑零变更；四 ALTER + caller_budget 全部落列；单行 ALTER 守卫语义保持；回滚 = git revert + 库残留可空列/空表（无害，登记 drop 说明） |
| 2 | **硬验收①：单 caller N=3 并行提速实测** | e2e：三步各 ~300ms 模拟驱动的同 caller run，并行帽=3 配置 vs 串行基线：并行总耗时 < 串行 × 0.7，且驱动侧并发观测峰值 ≥ 2；CLI 实测（selftest_ma2.sh）数字（串行/并行 ms）入批次报告，禁模拟冒充（计划 :76） |
| 3 | **硬验收②：超预算硬拒** | 三个拒绝点各有实测：速率帽窗口超限 → 整轮硬停 + GUARDRAIL kind=budget_caller_rate；总预算步数到位 → GUARDRAIL kind=budget_caller_total；受理面 max_concurrent_runs 超限 → BudgetDenied 硬拒（run 不落行）；全部留审计原文 |
| 4 | **硬验收③：双 caller 互不抢占** | caller-a 满载（并行 run 进行中且并发帽占满）时：caller-b 新 run 受理/执行不受影响（时延粗断言）；a 第三 run BudgetDenied 而 b 同刻受理成功；tool-engine 层 a 的 caller 顶层帽不占 b 的族槽位（acquire_for 单测断言） |
| 5 | legacy/default 零回归 | caller_id=None 或 caller_budget 缺行 → 串行 + 既有 warning + 既有护栏逐字节现状；既有全部自验脚本零改动通过（selftest_auth.sh 既有 11 例 + #8 块 / selftest_phase6.sh / selftest_phase7_agents.sh） |
| 6 | caller 落库与继承 | agent_run.caller_id / tool_run.caller_id 正确落列（027）；retry 新 run 继承源 run caller；trace 继承（015 面）不回归（trace_correlation_p7_5.rs 零改动通过） |
| 7 | tool-engine per-caller 帽 | acquire_for 双层判定单测：族内满 → None；caller 顶层满 → None；**另一 caller 同族可进**；ContainerCall.caller=None 行为不变；族基线断言（core_utils_e2e.rs:528-534）零改动通过 |
| 8 | rp-admin 预算面完整 | budget-set/show/clear + caller-revoke JSON 输出 + 退出码 0/1/2 口径；budget-set 后 CLI 立即可装配（无缓存陈旧）；caller-revoke 幂等且名下 budget 失效随停用语义 |
| 9 | 门禁全绿 | 迁移链尾断言五文件随 027 更新后全绿；clippy -D warnings；deny 双检；Cargo.lock 399 恒定；git status 只含 §2.1 落点 |

## 5. 测试面（收口命令）

```bash
# Rust 面（agent/tool/database 三 crate + 链尾断言）
cargo test --offline -p agent-engine -- --test-threads=1
cargo test --offline -p tool-engine -- --test-threads=1
cargo test --offline -p database -- --test-threads=1
cargo clippy -p agent-engine -p tool-engine -p database --all-targets --offline -- -D warnings
# deny / lock
cargo deny check advisories && cargo deny check licenses
grep -c '^\[\[package\]\]' Cargo.lock   # 399
# server 面（既有，零改动回归）
bash apps/server/selftest_auth.sh        # 既有 11 例 + MA-1 #8 多 key 块
bash apps/server/selftest_phase6.sh      # 零直连 SQL 写门禁（既有）
bash apps/server/selftest_phase7_agents.sh   # agent 读端点回归（POST 405 语义不动）
# 本批新增：CLI 三硬验收实测（N=3 计时对比 / 超预算硬拒 / 双 caller 互不抢占）
bash apps/server/selftest_ma2.sh
```

## 6. 红线（八条）

1. **禁止裸开并行**：并行支路放行三与条件（D3）——allow_parallel_subtasks=true 而 caller 预算面不齐（无 caller_id / 无 budget 行 / 帽<2）一律仍串行；`RunControlConfig::default()` 六字段值与语义零改动（control.rs:81-92；计划 :59 禁止事项 2「一切并发经预算面」）。
2. **族基线不动**：`family_concurrency_default = 1` 保持，per-caller 帽是独立顶层不缩放族基线（core_utils_e2e.rs:528-534 断言零改动通过；ROADMAP §4.5 discipline 4）。
3. **JSON 冻结契约零改动 + caller 不进 Registry**：caller 走 Rust 参数不进 ToolCallRequest JSON（request.rs:132 D-TC-06）；crates/tool-registry 零 diff（计划 :60 禁止事项 3——caller 是运行时维度）。
4. **不建第二事实源**：027 是 platform.db 同库纯增量四列一表；禁 per-caller 分库分 schema（计划 :58 禁止事项 1——租户隔离=行级 caller 字段）。
5. **历史迁移禁改**：001-026 零 diff；027 零既有列/CHECK 域改动（026:5-6 纪律；018:50-53 ALTER 单次执行语义）；链尾断言五文件只改预期值不改结构。
6. **零新依赖**：Cargo.lock 399 恒定；并行 = std thread/Condvar/AtomicUsize，速率窗 = std Instant；S1/S7 双点复核。
7. **生产库零触碰**：测试一律临时库/内存库；platform*.db / platform-dryrun.db / integrations/rtis 零 diff（018:55-56 红线）。
8. **边界纪律**：TaskClaim/邮箱消息/成员建队/知识供体面/MCP-REST 派发契约/SSE 推送/workflow_run·research_task caller 写面/PG caller 镜像/native·python 通道级 caller 帽/RT-3 constraints caller 维度/api_key.last_used_at 写入全部不进本批（D6/D9/D10；越界即停批上报）。

## 7. 回滚

- 单 commit 交付；`git revert` 整批撤销。027 对已迁移库残留四可空 NULL 列 + caller_budget 空表（无害；回滚说明登记 drop 语句——SQLite DROP COLUMN 为 3.35+ 特性且禁改历史迁移，残留不清理）。
- 代码面零回归设计使 revert 风险低：caller_id=None 全链路逐字节现状；并行支路缺 caller 面自动落串行；caller_budget 查询失败降级串行并记一次 warn（rp_server.py:236-239 降级告警先例同源），不炸 run。
- tool-engine 侧 acquire_for 纯新增 + 挂点改造保留 None 直通路径；rp-admin 新子命令纯新增，revert 后零残留。

## 8. 交付物

1. `migrations/027_caller_attribution.sql`（四表 caller_id + caller_budget）+ agent-engine caller 通道与并行支路（agent_loop/control/store/error/tool_link/rp-agent）+ tool-engine per-caller 扩展（podman_host/lib.rs/adapter mod）+ rp-admin 四子命令。
2. 测试：三硬验收 e2e（`ma2_parallel_budget.rs`）+ FamilyLimiter 双层判定单测 + 迁移链尾五文件随更 + `apps/server/selftest_ma2.sh` CLI 实测（判据逐项有实测数字）。
3. `docs/rip3/MA-2-BATCH-REPORT.md`（命令原文 + 数字实测 + 坏消息单列）+ docs/README.md 登记。
4. MA-3 前置输出：workflow_run.caller_id / research_task.caller_id 落列零写面声明（节点角色绑定与 RTIS 派发面接线输入）+ caller_budget 配置形状备忘（成员建队预算传递输入）+ R7 成员模型可配、RT-3 constraints caller 维度、native/python 通道级帽三项观察项登记。
