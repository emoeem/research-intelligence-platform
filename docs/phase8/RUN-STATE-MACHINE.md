# Workflow 运行状态机与执行引擎（P8-2 设计稿）

> 本文件是 Phase 8 P8-2 批次（执行引擎与非智能节点 + 运行状态持久化）的设计稿与口径登记。上游：docs/phase8/WORKFLOW-DSL.md（P8-1 定义面）、docs/phase0/PHASE-ROADMAP.md §9.3 P8-2、计划 §14:828-830（Tool/Knowledge/Condition/Parallel 节点 + 运行、暂停、恢复、失败重试）。代码面：crates/workflow-engine/src/{exec/, state.rs}、migrations/017_workflow_run.sql、crates/workflow-engine/src/bin/rp-workflow.rs（run/status/nodes/resume/retry/cancel 子命令）。

| 项 | 值 |
| --- | --- |
| 状态 | 生效中（批次 P8-2，第 2/4 批） |
| 日期 | 2026-11-13 |
| 相关 | docs/phase8/WORKFLOW-DSL.md、docs/phase0/PHASE-ROADMAP.md §9、apps/server/selftest_phase8_workflow.sh、docs/releases/RELEASE_2026-11-13_PHASE8-P8-2.md |

## 一、结论先行

1. 运行持久化落 017 三张新表：`workflow_run`（run 头行，六态词域）、`workflow_run_node`（节点状态行，五态词域 + attempt 计数）、`workflow_checkpoint`（节点终态即持久化的全量快照，只追加）。词值全部沿用 P8-1 冻结在 domain::workflow 的两张词表（`WorkflowRunState` 六值、`WorkflowNodeRunState` 五值），不新增第三处词表；CHECK 域与 domain 词值一一对应。
2. 执行引擎 Rust 自研（零执行框架引入），LangGraph/Temporal 思路落地为：每个节点到达终态写一行 checkpoint 快照，恢复从 MAX(seq) 快照回放，已完成节点不重跑。
3. 四类非智能节点执行器：Tool 走 tool-engine 派发链（submit D-TC-05 受理 + ToolRun 状态机 + 信封，python_worker 真实派发按 Manifest runtime 块解析，运行面经 `ToolRunDriver` 端口注入）；Knowledge 走 context-broker `Broker::assemble`（rp-agent 同款全源装配）；Condition 是纯函数谓词（expression 为一份 JSON Schema 文档，对合并后的 run state 求值，复用 jsonschema 0.56 既有依赖）；Parallel 是 fork-join 协调器（branch 扇出并发执行，join 全 succeeded 才收敛）。
4. 失败重试以新 run 承载：P8-1 冻结的跃迁表里 failed 是终态不迁出，retry 读失败 run 的最新 checkpoint 新开一个 run（钉同一 (workflow_id, workflow_version) 与 input），只把目标失败节点回 pending，其余节点状态与 attempt 原样继承；重跑时该节点 attempt 在原值上 +1。先例 agent_engine RunControl::retry。
5. 节点超时是显式终态：看门狗线程 recv_timeout，超时节点落 failed（error code = `node_timeout`）；超时后残留的执行线程若后续完成，其终态写入被乐观守卫（UPDATE WHERE state = 'running'）静默丢弃，不与 retry/resume 竞态双写。

## 二、状态机词表与跃迁（冻结面）

run 六态（domain::WorkflowRunState，017 CHECK 域词值）：

| 态 | 含义 | 可迁出 |
| --- | --- | --- |
| queued | 已受理未启动 | running / cancelled |
| running | 执行中 | paused / succeeded / failed / cancelled |
| paused | 协作暂停（P8-3 续接） | running / cancelled |
| succeeded | 成功终态 | 无 |
| failed | 失败终态（重试以新 run 承载） | 无 |
| cancelled | 取消终态 | 无 |

节点五态（domain::WorkflowNodeRunState）：pending（未到达）→ running（attempt +1 时进入）→ succeeded / failed；skipped 为条件分支未选中级联的终态。attempt 语义：已开始的执行次数，0 = 未开始；只在进入 running 时 +1。

落库即由 `RunStore::transition_run` / `node_start` / `node_finish` / `node_skip` 把关：全部走乐观守卫（UPDATE ... WHERE state = 预期态），命中 0 行即读回现态报 `WorkflowError::IllegalTransition` / `NodeNotStartable`。协作式 cancel（CLI `cancel` 子命令）对 queued/running/paused 直改 cancelled；正在执行的进程下一波读回 cancelled 即止（运行中节点不抢占，协作语义与 ToolRun D-TC-04 同源）。

## 三、checkpoint 与恢复语义

1. 快照形状：`{"nodes": {node_id: {state, attempt, output, error}}}`，由 `RunStore::snapshot_of` 从 workflow_run_node 全量投影生成；`record_checkpoint(run_id, node_id)` 在节点每到达一个终态（succeeded/failed/skipped）后追加一行（seq 自增单调，含触发节点与触发时终态）。
2. resume（同 run_id）：接受 running（kill 后）/ paused / queued 三种非终态；读 MAX(seq) 快照，快照内终态节点整体回放（state/attempt/output/error 原样），快照内非终态条目与快照外节点复位 pending 且 attempt 保留（kill 前已 node_start 的行计数不因快照滞后丢失）；随后从「input + 已 succeeded 节点输出按 node_id 升序浅合并」的恢复态续跑。
3. kill 模拟（selftest 第 7 节）：长链 run 到「已成功节点 checkpoint >= 3」即 SIGKILL；run 行停留 running，resume 后已成功节点保持 succeeded attempt=1（不重跑），其余续跑至 succeeded。
4. 恢复态合并顺序与正向完成序的差异：正向按完成序覆盖键冲突，恢复按 node_id 升序覆盖；两序在同键冲突场景可能产生不同 state（已知限制，见「七、已知取舍」第 4 条）。

## 四、四类节点执行器口径

### 4.1 Tool（exec/node_tool.rs）

- params：`tool_id`（必填，validate.rs 词表）、`input`（可选 object，字符串叶值 `$/` 前缀按 run state 的 RFC 6901 JSON Pointer 解析，缺省 = 整个 state）、`version` / `workspace_id`（缺省 ws-workflow）/ `task_id` / `authorization_ref` / `budget`（可选透传）。
- 派发链全走 tool-engine 公开接口：`Engine::submit_correlated`（D-TC-05 受理，拒绝不落 run 行；trace_id = workflow run id，015 链路关联）→ `ToolRunDriver::drive` 端口（CLI 装 `PythonWorkerToolDriver`：按 tool_run 行回查 tool_id，逐工具从 Manifest runtime 块解析 python_worker 适配器，argv 固定清单不经 shell，rp-agent resolve_python_worker 同款；测试装 Mock 注入）→ 信封校验交付。
- 输出：SUCCEEDED 信封的 result（null 或 object）；非 SUCCEEDED 终态 → 节点 failed，code = `tool_<status 小写>`（tool_failed / tool_timed_out / tool_cancelled）或 `tool_submit_rejected` / `tool_drive_failed`。
- 幂等 submit_key：`wf:<run_id>:<node_id>:a<attempt>`（同一 run 同一节点同一次 attempt 恒同键，跨 attempt 变键）。

### 4.2 Knowledge（exec/node_knowledge.rs）

- params：`query`（必填，支持 `$/` 引用）、`actor`（可选，缺省 rp-workflow）。
- 执行 = `Broker::assemble(AssembleRequest { task_id: run_id, text: query, correlation_id: run_id, trace_id: None, actor })`；装配副作用（manifest 只追加 + audit 事件 + 引用证据固化）由 Broker 内部完成，本层零 SQL 不自查库（P7-3 判据同源）。
- 输出投影：`{query, context_id, manifest_id, intent_status, confidence, knowledge_count, knowledge: [{item_id, ref_id, source, title, grade, summary 截 160 字}], tool_brief_count, warnings}`（摘要形态，正文永不预装）。
- Broker 未注入时 fail-closed（code = `broker_unavailable`）；装配错误按 ContextBrokerError 原文 failed，不静默放空集装成功。
- 为支撑 Parallel 子分支线程内装配，`ContextSource` 端口补 `Send + Sync` 上界（既有六实现均持 Pool/路径载体，行为面零改动，context-broker 唯一本批改动点）。

### 4.3 Condition（exec/node_condition.rs）

- params：`expression`（必填）为一份 JSON Schema 文档（draft 2020-12 子集）；`$/` 引用先解析再编译。
- 谓词 = `jsonschema::validator_for(expression).is_valid(state)`：通过 = true，否则 false。零解释器零随机性零 IO（同 state 恒同结果），表达力由 JSON Schema 承载（const / enum / required / 类型 / 数值约束）。
- 输出 `{"condition_result": bool}` 并入 state；branch 出边携带 when 布尔字面量（受理门强制），`condition_result == edge.when` 的边选中，其余边死边（下游 skip 级联）。
- 坏谓词受理期即拒（accept_definition 编译门），运行期不再出现 invalid schema。

### 4.4 Parallel（exec/node_parallel.rs）

- parallel 节点是 fork-join 协调器，不是计算节点：进入 running（attempt +1）后，全部 branch 出边目标（fan 子节点）各自派发独立线程并发执行（每子节点独立 attempt / node row / checkpoint / 看门狗超时），join 收齐全部子节点终态；全 succeeded → parallel succeeded（输出 = 子输出按子节点 id 升序浅合并），任一 failed → parallel failed（code = `parallel_child_failed`，载荷列出失败子节点），run 随 fail-fast 终止。
- 受理门限定：fan 子节点只能是 tool/knowledge/condition，且入度恰为 1（该 fan 边；diamond 入口会绕过 fork 语义，fail-closed）；parallel 出边必须全 branch 且不带 when；嵌套 parallel（子节点还是 parallel）拒绝。
- 并发纪律：fan 并发度 = branch 出边数（画布声明面，引擎不自动放大）；子节点写面经 database Pool 串行化（rusqlite 连接互斥），无共享可变状态。

## 五、run 循环（exec/mod.rs drive_run）

1. 波次推进：每轮读回 run 现态（协作 cancel 即止）与全节点行；先做 skip 判定（pending 节点全部入边均为死边，即条件分支未选中级联 → skipped + checkpoint），再取 ready 节点（node_id 升序首个；start 节点无入边即 ready；其余要求任一入边 Selected）。
2. 单节点执行：`node_start`（attempt +1）→ 派发线程（engine/driver/broker 全 Clone/Arc）→ 看门狗 `recv_timeout(node_timeout)` → 终态落列 + checkpoint → 输出浅合并进 state（object 才可并，非 object fail-closed code = `output_not_object`）。
3. fail-fast：任一节点 failed（含 parallel join 失败）即 run failed（error 载荷 {run_id, node_id, code, message}）；后续 pending 节点留在 pending 不标 skipped（skipped 专属分支未选中语义）。
4. 无进展 fail-closed：无 ready 无 skip 且仍有 pending 时落 failed（code = `no_progress`，理论不可达：静态校验禁环，防御面兜底），另有 4n+16 轮迭代守卫。
5. 受理门（accept_definition，start/resume/retry 共用）：compat 必须 graph-v1（legacy 参考行永不执行）；含 agent 节点拒绝（P8-3）；input 必须 JSON object 且合 input_schema（若声明，jsonschema 校验）；branch 边形状（见 4.3/4.4）；condition 必须有 branch 出边且 expression 可编译。

## 六、CLI 命令形与 exit 约定

| 命令 | 作用 | exit |
| --- | --- | --- |
| `run <workflow_id> --input '<json>' [--version <n>] [--node-timeout-ms <n>]` | 受理门后执行到终态 | SUCCEEDED = 0；FAILED = 1；受理拒绝 = 2 |
| `status <run_id>` | run 头行投影（六态 + 输入/输出/错误载荷 + 各态节点计数） | ok = 0；不存在 = 2 |
| `nodes <run_id>` | 节点状态行（五态 + attempt + 输出/错误）+ 最新 checkpoint seq | 同上 |
| `resume <run_id>` | 断点恢复（已完成节点不重跑） | 同 run |
| `retry <run_id> --node <node_id>` | 失败重试（新 run 承载，只重跑目标节点） | 同 run |
| `cancel <run_id>` | 协作式取消（queued/running/paused → cancelled） | cancelled = 0；终态 run 拒绝 = 1 |

默认 `--db migrations/dev-p8.db`；platform*.db 前缀守卫与 rp-agent 同款；`--rulepack-dir` / `--manifest-dirs` 缺省 rp-agent 同款（rulepacks/rip-rules、manifests/tools、containerfiles 两目录）。

## 七、已知取舍

1. retry 以新 run 承载而非复用 run_id：P8-1 冻结跃迁表 failed 为终态（`Failed.can_transition(Running) == false` 且有单测钉住），改表属推翻已验收能力（红线）；新 run 继承 (workflow_id, workflow_version) / input / checkpoint 节点状态，CLI 输出 `new run_id`，对账按新 run_id 查。P8-3 长任务语义若需同 run 续跑，需先在 domain 层扩跃迁表并过测试基线。
2. 超时残留线程不 kill：派发线程持有 engine/driver/broker 的 Arc，超时后放任其自然结束（python_worker 有自身 worker 级 timeout_ms 兜底）；其终态写入被乐观守卫丢弃。进程退出随宿主回收。换来的是：超时路径零 unsafe、零进程管理复杂度；代价是超时后短暂的无谓计算。
3. attempt 在 resume 场景可能 +1：被 kill 的 running 节点行已 node_start（attempt 已计 1），resume 复位 pending 保留计数，重跑再 +1。已完成节点严格不变（ask 判据只约束已完成节点）。
4. 恢复态合并序（node_id 升序）与正向完成序不同：同键冲突时恢复态的覆盖者可能与首次执行不同。当前四类节点输出键天然按节点语义隔离（query / keyword / condition_result），冲突面为零；若未来节点输出键交叉，需在画布层约束（P8-4）。
5. 单节点波次推进（每轮执行一个 ready 节点）而非批量并发：确定性优先（同图同输入恒同轨迹），并发只在 Parallel fan 面存在（该处的并发是语义要求的）。规模上限：Schema 门已限 256 节点 / 1024 边。
6. run 级事件不单独落审计表：节点轨迹在 workflow_run_node + workflow_checkpoint，工具面在 tool_run（trace_id = run_id 可关联）与 audit_event（Broker 装配事件 correlation_id = run_id），不新增第四处轨迹面；回放语义 P8-3 与 audit trace 对齐时再收口。
7. PythonWorkerToolDriver 逐次解析 Manifest（不做进程内适配器缓存）：CLI 单 run 场景解析成本可忽略（每节点一次 read_dir + json parse）；Agent 长循环场景的缓存优化属 agent-engine 既有面，不在本批。
