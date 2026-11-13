# RELEASE：Phase 8 P8-2 执行引擎与非智能节点 + 运行状态持久化（2026-11-13）

> 本批把 Phase 8 的执行半边落成可验收的代码面：017 迁移三张新表
> （workflow_run 六态 + workflow_run_node 五态 attempt + workflow_checkpoint
> 每步快照只追加）+ Tool / Knowledge / Condition / Parallel 四类非智能节点
> 执行器（Tool 走 tool-engine 派发链真实 python_worker 派发，Knowledge 走
> context-broker 装配检索，Condition 是 jsonschema 纯函数谓词，Parallel 是
> fork-join 并发收敛）+ rp-workflow CLI 新增 run/status/nodes/resume/retry/
> cancel 六个子命令。用户可感知的变化：一条命令即可把登记好的 workflow 定义
> 跑到终态并逐节点对账；kill 掉执行中的进程后 resume 能从 checkpoint 续跑
> （已完成节点不重跑）；失败节点 retry 后在新 run 里 attempt 递增重跑；
> 节点超时有显式 failed 终态（code=node_timeout）。Agent 节点属 P8-3，
> 受理面即拒绝；画布属 P8-4。

| 项 | 值 |
| --- | --- |
| 状态 | 生效中（Phase 8 第 2/4 批，ROADMAP §9.3 P8-2） |
| 日期 | 2026-11-13 |
| 阶段交付 | 待入库（commit 短哈希按仓库回填惯例补记，先例 docs/README.md Phase 7 行；基线 HEAD 102cab5，P8-1 与 P8-2 改动同在未提交工作树） |
| 数据库迁移 | 017_workflow_run.sql（三张新表 + 三索引 + schema_version 登记 + user_version = 17；在临时库与 migrations/dev-p8.db 开发库实测，migrations/platform*.db 生产库零触及：CLI --db platform 前缀守卫、对账走 mode=ro URI、selftest 前后指纹一致复核） |
| 自动化测试 | `cargo test --offline --workspace`：568 passed 0 failed（P8-1 基线 536 + 本批新增 32：state.rs 单测 4、exec 模块单测 19（exec/mod 3：resolve_refs/edge_view/new_run_id + node_tool 3 + node_knowledge 5 + node_condition 5 + node_parallel 3）、集成测试 run_execution.rs 9、既有链尾断言顺延后全数保留） |
| 静态检查 | `cargo clippy --offline --workspace --all-targets -- -D warnings` 通过（0 error；manual_contains ×2 / redundant closure / needless borrow / dead code 逐条修复后归零） |
| 端到端自验 | apps/server/selftest_phase8_workflow.sh：46 PASS 0 FAIL（生产库指纹前后一致） |
| 相关 | docs/phase8/RUN-STATE-MACHINE.md（设计稿）、docs/phase0/PHASE-ROADMAP.md §9.3/§9.4、docs/phase8/samples/p8-*.json（五个样例）、docs/DOC-STANDARD.md |

## 一、改动清单

| 文件 | 内容 |
| --- | --- |
| migrations/017_workflow_run.sql（新） | workflow_run（run 六态 CHECK 域 = domain::WorkflowRunState + workflow_version 钉定列 + input/output/error 载荷）、workflow_run_node（五态 CHECK 域 + attempt 计数 + node_kind 词域 + output/error）、workflow_checkpoint（seq 自增只追加 + 终态触发快照）、三索引、schema_version 登记 version=17、PRAGMA user_version = 17；零 ALTER，零既有表改动 |
| crates/workflow-engine/src/state.rs（新） | RunStore 读写面：create_run（queued 头行 + 全节点 pending）、transition_run（乐观守卫 + 冻结跃迁表）、node_start（attempt +1）/ node_finish（WHERE running 守卫，超时残留写入静默丢弃）/ node_skip / node_reset_pending、record_checkpoint（终态触发全量快照只追加）、latest_checkpoint / snapshot_of / restore_from_snapshot（终态回放，非终态复位 pending attempt 保留）、restored_state（input + succeeded 输出按 node_id 升序浅合并）；4 单测 |
| crates/workflow-engine/src/exec/mod.rs（新） | WorkflowExecutor（start/resume/retry/cancel/status + drive_run 波次循环）、accept_definition 受理门（compat graph-v1 / input object + input_schema / branch 边形状 / condition 表达式编译门 / parallel fan 目标词形与入度）、dispatch_node（派发线程 + attempt）、await_dispatch（看门狗 recv_timeout，超时 code=node_timeout）、edge_view（seq 恒放行；condition 按 condition_result == when 布尔字面量；parallel 恒放行）、resolve_refs（`$/` + RFC 6901 JSON Pointer）、new_id（agent-engine/context-broker 同源熵口径）；fail-fast + no_progress 防御 + 协作 cancel 收敛 |
| crates/workflow-engine/src/exec/node_tool.rs（新） | Tool 节点：tool-engine 派发链（submit_correlated D-TC-05 + trace_id = run_id + ToolRunDriver 端口 + 信封交付），submit_key = wf:<run_id>:<node_id>:a<attempt>，输出 object 才可并（fail-closed output_not_object）；3 单测 |
| crates/workflow-engine/src/exec/node_knowledge.rs（新） | Knowledge 节点：Broker::assemble 检索（correlation_id = run_id），AssembledContext 摘要投影输出（knowledge/tool_brief/意图/warnings），broker 未注入 fail-closed；5 单测（StubSource 测试载具） |
| crates/workflow-engine/src/exec/node_condition.rs（新） | Condition 节点：expression JSON Schema 纯函数谓词（`$/` 引用先解析再编译），输出 condition_result；5 单测 |
| crates/workflow-engine/src/exec/node_parallel.rs（新） | Parallel 节点：branch 扇出并发派发（每子节点独立 attempt/row/checkpoint/看门狗）+ join 收敛（全 succeeded 才收敛，失败 parallel_child_failed），子输出按子节点 id 升序浅合并；3 单测 |
| crates/workflow-engine/src/error.rs | 增 5 变体：RunNotFound / IllegalTransition / NodeNotStartable / InvalidInput / Exec（文本含词形 machine 可 grep） |
| crates/workflow-engine/src/lib.rs / Cargo.toml | lib.rs 增 state/exec 模块与导出（RunStore/RunOutcome/WorkflowExecutor/DEFAULT_NODE_TIMEOUT_MS 等），crate 头注从 P8-1 口径更新为 P8-2；依赖面增 tool-engine / context-broker / tool-registry / audit-engine / resource-engine 五个 workspace 内部成员（零新增第三方 package） |
| crates/workflow-engine/src/bin/rp-workflow.rs | CLI 增 run/status/nodes/resume/retry/cancel 六子命令 + --input/--version/--node-timeout-ms/--rulepack-dir/--manifest-dirs/--node 旗标 + PythonWorkerToolDriver（按 tool_run 行回查 tool_id 逐工具解析 Manifest runtime 块）+ build_broker（rp-agent 同款全源装配）+ exit 码表（终态 FAILED = 1，受理拒绝 = 2） |
| crates/context-broker/src/source.rs | ContextSource 端口补 Send + Sync 上界（Parallel 子分支线程内装配需要；既有六实现零行为改动，本批 context-broker 唯一改动点） |
| crates/workflow-engine/tests/run_execution.rs（新） | 9 集成测试：四类节点最小定义各自 run 到 SUCCEEDED + 节点状态对账 + tool_run.trace_id 关联 + manifest 落库对账；condition 双分支 + skipped 级联；parallel join 失败收敛；node_timeout 显式终态；kill 模拟 + resume（SplitDriver 阻塞载具，a 成功 checkpoint + b running 双条件确定性 kill 点）；retry（缺登记失败注入 → 登记 → 新 run attempt 2）；受理门五反例 |
| 链尾断言顺延（6 个既有测试文件 + workflow-engine replay 测试） | crates/database/tests/migrations_tool_contract.rs / migrations_context_broker.rs / migrations_runtime_node.rs / migrations_agent_data_plane.rs（discover 全链 001..017、链尾 017_workflow_run、uv 17）、crates/agent-engine/tests/migration_idempotent.rs（uv 17）、migration_dev_copy_p7_3.rs（dev-p7.db 副本 013..017）、crates/plugin-runtime/tests/lifecycle_full_path.rs（009 自钉回 9 后重放 010..017 八枚）、crates/workflow-engine/tests/migration_replay.rs（017 临时库重放两次幂等 + raw file replay 口径 + 三新表在表清单） |
| docs/phase8/samples/p8-tool-node.json 等 5 个（新） | 四类节点各一最小定义 + retry 病态样例（未登记工具注入）；CLI 与集成测试共用（include_str! 固定装样） |
| apps/server/selftest_phase8_workflow.sh（新） | P8-2 十段自验脚本（构建 / 生产库指纹 / put 样例 / 失败注入 / 四 run / 只读副本对账 / retry attempt 递增 / kill+resume / 节点超时 / status+nodes / 生产库零写复核） |
| docs/phase8/RUN-STATE-MACHINE.md（新） | 设计稿（状态机词表跃迁 / checkpoint 恢复语义 / 四执行器口径 / run 循环 / CLI exit 表 / 已知取舍七条） |
| docs/README.md | Phase 8 行登记 P8-2 批次 |

## 二、设计要点（为什么这样做）

1. **词表不新增第三处**：run 六态与节点五态直接沿用 P8-1 冻结的 domain::workflow 词值作 017 CHECK 域，跃迁合法性在执行器按 WorkflowRunState::can_transition 把关；改词表必须新增 domain 变体并升迁移，不存在两处漂移面。
2. **每步落库的快照语义（LangGraph/Temporal 思路）**：checkpoint 在节点到达终态时写全量投影（不是增量事件），恢复 = MAX(seq) 一行整体回放，恢复路径不重放事件序列、不需要幂等去重；只追加表无 UPDATE 路径，对账形状与快照恒一致。
3. **retry 以新 run 承载**：P8-1 冻结跃迁表 failed 是终态（有单测钉住 `!Failed.can_transition(Running)`），本批不改冻结面；retry 读失败 run 的最新 checkpoint 新开 run（钉同版本同 input），只把目标节点回 pending，attempt 在重跑进入 running 时 +1（1 → 2 实测）。ask 自验第 4 条的「attempt 递增且重跑成功」按新 run_id 对账。
4. **超时是显式终态而非取消**：看门狗 recv_timeout 到点节点落 failed（code=node_timeout），残留执行线程终态写入被 WHERE state='running' 乐观守卫静默丢弃；不引入 kill 进程复杂度，python_worker 自身 worker 级 timeout_ms 兜底长任务。
5. **复用优先**：Tool 节点全走 tool-engine 公开接口（受理/状态机/信封零绕过，运行面 PythonWorkerDriver 口径与 agent-engine tool_link 同款）；Knowledge 节点全走 context-broker Broker::assemble（装配/预算/清单/审计零绕过）；Condition 谓词复用既有 jsonschema 0.56（零新增 package）；id 生成复用 agent-engine/context-broker 同源熵口径。Cargo.lock diff 仅 workflow-engine 自身依赖条目，vendor 无新增。
6. **生产库零触及 mechanically 保证**：--db platform 前缀守卫（CLI 三处入口同款）、selftest 指纹（user_version + 逐表行数）前后一致、对账一律 mode=ro URI。

## 三、ask 五条自验逐条实测

1. **四类节点各一最小定义 run 跑通 exit 0 四例**（ask 原命令形，默认库 migrations/dev-p8.db，kb 工具经 rp-tool register 已登记）：

   ~~~text
   cargo run --offline -p workflow-engine --bin rp-workflow -- run wf-p8-tool --input '{"target":"example.com"}'
   exit=0 workflow=wf-p8-tool run_id=wfr-3af6b33945fc04ee00000000 state=succeeded
   exit=0 workflow=wf-p8-knowledge run_id=wfr-f7405bc1fa6979b300000000 state=succeeded
   exit=0 workflow=wf-p8-condition run_id=wfr-d48161251ef4bddb00000000 state=succeeded
   exit=0 workflow=wf-p8-parallel run_id=wfr-3f5d0e493712ee9800000000 state=succeeded
   ~~~

   Tool 节点为真实 python_worker 派发（rip.kb.keyword_search 对开发库只读 LIKE 检索），Knowledge 节点为真实 Broker 装配（manifest 落库 task_id = run_id）。

2. **节点状态对账（只读开发库副本，ask 原 python 单行）**：

   ~~~text
   python3 -I -c "import sqlite3; c=sqlite3.connect('file:/tmp/rp_p82_devcopy.db?mode=ro',uri=True); print(c.execute(\"select node_id,state,attempt from workflow_run_node where run_id='wfr-3af6b33945fc04ee00000000'\").fetchall())"
   [('start', 'succeeded', 1), ('search', 'succeeded', 1), ('done', 'succeeded', 1)]
   # condition run wfr-d48161251ef4bddb00000000（true 分支执行 + false 分支级联 skipped）：
   [('miss', 'skipped', 0), ('done-miss', 'skipped', 0), ('start', 'succeeded', 1), ('check', 'succeeded', 1), ('hit', 'succeeded', 1), ('done-hit', 'succeeded', 1)]
   ~~~

   每节点独立终态（含 skipped 终态）、attempt 落列；selftest 第 5 节对四类 run 全量对账（46 断言内含）。

3. **断点恢复**（selftest 第 7 节实录）：长链 run（8 个顺序 tool 节点）至「已成功节点 checkpoint >= 3」即 SIGKILL，run 行停留 running；`rp-workflow resume <run_id>` 续跑 exit 0 至 succeeded，kill 前已完成节点 t1 保持 `('succeeded', 1)`（不重跑），中断节点 t2 及后续续跑。库内层等价断言见集成测试 resume_from_checkpoint_keeps_completed_nodes_and_attempts（SplitDriver 阻塞载具 + a 成功 checkpoint 与 b running 双条件确定性 kill 点）。

4. **失败重试 attempt 递增**（selftest 第 3/6 节实录）：wf-p8-retry 在工具未登记时 run 得 FAILED（search 节点 tool_submit_rejected attempt=1，done 留 pending）；rp-tool register 后 `retry <run_id> --node search` 得新 run SUCCEEDED，对账 `[('done','succeeded',1), ('search','succeeded',2), ('start','succeeded',1)]`（attempt 1 → 2 递增），原 run 保持 FAILED 终态。库内层等价断言见集成测试 retry_after_failure_reruns_only_failed_node_with_incremented_attempt。

5. **全仓门禁**：`cargo test --offline --workspace` 568 passed 0 failed（P8-1 基线 536，零回归）；`cargo clippy --offline --workspace --all-targets -- -D warnings` 0 error。另测 017 迁移临时库重放两次幂等（migration_017_replay_twice_is_idempotent：两次 user_version 均为 17、表清单一致、raw file replay 口径通过）。

补充实测（超出 ask 判据的验收面）：

- 节点超时：`run wf-p8-tool --node-timeout-ms 50` exit 1，search 节点 error code=node_timeout，run error 载荷同源（selftest 第 8 节）。
- 协作取消面：cancel 子命令对终态 run 拒绝（exit 1），集成测试 acceptance_gates 覆盖。
- 受理门反例五枚（集成测试）：agent 节点 / input 非 object / input_schema 违规 / legacy-method-reference 参考行 / condition 上挂 seq 边，全部 exit 2 且错误文本可 grep。
- selftest_phase8_workflow.sh 46 PASS 0 FAIL（十段：构建、生产库指纹、put、失败注入、四 run、只读副本对账、retry、kill+resume、超时、读取面、零写复核）。

## 四、已知取舍

1. retry 以新 run 承载（不复用 run_id）：冻结跃迁表不动（红线），设计稿 §七第 1 条登记；对账按 CLI 输出的新 run_id。
2. 超时残留线程不 kill：终态写入由乐观守卫收口，短暂无谓计算换零 unsafe 零进程管理。
3. resume 后被中断节点 attempt +1（node_start 已计数的行保留计数）：ask 只约束已完成节点不变；设计稿 §七第 3 条。
4. 恢复态合并按 node_id 升序，与正向完成序在键冲突场景可能不同（当前四类节点输出键语义隔离，冲突面为零）。
5. run 级事件不落第四处审计表：轨迹面 = workflow_run_node + workflow_checkpoint + tool_run（trace_id 关联）+ audit_event（Broker 装配），回放收口留 P8-3。
6. dev-p8.db 本批留演示态（5 个 graph-v1 定义 v1 + kb 工具登记 + 5 条 run 轨迹 + 20 checkpoint），*.db 全局 gitignore 可随时删除重建。
7. ContextSource 补 Send+Sync 上界是 context-broker 唯一改动（接口约束收紧，行为零改动）；升级 context-broker 时无需回归本批执行面（上界只增不减）。

## 五、回滚方式

- 代码面：git revert 本批提交（P8-1 与 P8-2 同在未提交工作树时，按文件粒度回退 crates/workflow-engine/{exec/,state.rs}、migrations/017、bin/rp-workflow.rs、tests/run_execution.rs 与链尾断言改动即可回到 P8-1 五子命令形态）。
- 迁移面：017 为纯新增三表，回滚 = 删除 017 文件后重建开发库副本（临时库 / 开发库可删重建；生产 platform*.db 本批零触及，无回滚需求）；workflow_run 三表不承载生产数据，无数据迁移负担。
