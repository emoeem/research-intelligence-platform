# RELEASE：Phase 7 P7-4 运行控制与 AgentRun 持久化轨迹（2026-11-13）

> 本批把 Agent 循环的护栏从「只有步数上限」补成九类控制全项：暂停、恢复、
> 取消、重试、停滞检测、重复调用检测、目标偏移检测、证据不足提示、预算与
> 资源限制，全部与循环同批上线（ROADMAP §8.5 风险 1 不允许先上循环后补
> 护栏）。用户可感知的变化：run 可以中途暂停再恢复且已完成步骤不重跑；
> 失控循环（停滞、重复调用、目标偏移、预算耗尽）会被循环即时截停并在报告
> 与审计流留痕；取消与 ToolRun D-TC-04 同为协作式语义。

| 项 | 值 |
| --- | --- |
| 状态 | 生效中（编排批次 P7-4 第 4/7 批，内容对应 ROADMAP §8.3 P7-5「运行控制」行；批次序号错位口径同 P7-3，见下文「批次对位」） |
| 日期 | 2026-11-13 |
| 阶段交付 | 未提交（工作区改动与 P7-1/P7-2/P7-3 未提交改动共存，提交收口由编排层决定；本批改动面见「一、改动清单」） |
| 数据库迁移 | 新增 014_agent_run_control.sql（agent_run_control + agent_run_step 两表，追加式）；在测试内存库（crates/agent-engine 三个测试 + crates/database 四个测试）与 migrations/dev-p7.db 的 /tmp 拷贝（migration_dev_copy_p7_3.rs 实测 12 → 14）执行两次无差异；migrations/platform*.db 零写入（红线，migration_dev_copy_p7_3.rs 断言原件 user_version 仍为 12） |
| 自动化测试 | `cargo test --offline -p agent-engine`：26 lib + 16 集成（closed_loop 2 / memory_contract 2 / migration_dev_copy 1 / migration_idempotent 1 / run_control_p7_4 10）= 42 passed 0 failed；`cargo test --offline --workspace`：88 个目标 481 passed 0 failed（含 database 链尾断言顺延 014 与 plugin-runtime 链尾距离 5 断言） |
| 静态检查 | `cargo clippy --offline --workspace --all-targets -- -D warnings` 通过（0 error；fmt 只保留 agent-engine 与 domain 改动面，顺带触碰的 knowledge.rs 按 P7-3 同款纪律 git checkout 还原） |
| 相关 | docs/phase0/PHASE-ROADMAP.md §8.3 P7-5 行、§8.4、§8.5；docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md §9:474；docs/DOC-STANDARD.md |

## 一、改动清单（按 ask 关键路径逐项）

| 文件 | 改动 |
| --- | --- |
| migrations/014_agent_run_control.sql（新） | 两张追加式新表：agent_run_control（1:1 控制请求与续接上下文：协作式 pause/cancel 请求位 + goal_text + retry_of_run_id）与 agent_run_step（步骤轨迹：UNIQUE(run_id, seq)，input_json/content_hash 快照，resume 续接权威面）。006 的 state CHECK 六值域不动：控制请求不新增状态词值 |
| crates/domain/src/agent.rs | AgentRunState 扩展两个协作式语义谓词：`pause_requestable()`（仅 RUNNING）与 `cancel_requestable()`（QUEUED/RUNNING/PAUSED，与 D-TC-04 语义一致）；跃迁表零改动（重试仍按新 run 重放设计，不复活终态行） |
| crates/agent-engine/src/state.rs（新） | RunStateStore：请求位写入（发起态谓词校验 + CAS，受理语义与 tool-engine request_cancel 同为返回 bool）与消费（consume-on-observe 一次性信号）、goal_text/retry 链读写、步骤轨迹 record_step/steps（UNIQUE(run_id, seq) 冲突即拒绝不静默覆盖） |
| crates/agent-engine/src/control.rs（新） | RunControlConfig（九类护栏参数，默认即低并发纪律最严面）+ RunControl 编排面（pause / cancel 协作式 + QUEUED/PAUSED 直接跃迁 / retry 新 run 重放链）+ 检测器纯函数（canonical_call_key 规范键、drift_step、step_has_new_evidence、insufficient_evidence、elapsed_over_budget）+ 审计动作词表 actions |
| crates/agent-engine/src/agent_loop.rs | run() 拆出共用执行尾 drive()；新增 resume() 入口（PAUSED→RUNNING CAS + 轨迹续接 + goal 从 agent_run_control.goal_text 续接）；循环步间消费控制请求（取消先于暂停，消费点立即 CAS 跃迁）；九类护栏内嵌（轮次/墙钟预算、停滞窗口、判重跳过、偏移容限、证据不足提示、并行子任务默认关闭串行化）；步骤轨迹逐步骤落库；submit_key 加步序（`agent:{run}:{seq}:{tool}`，多步 run 下同工具不同入参不能共享 python_worker 直通 idempotency_key）；StepRecord 增 content_hash、RunReport 增 resumed_steps（均 additive） |
| crates/agent-engine/src/lib.rs | 注册 control/state 模块与公开导出；头注更新 P7-4 口径 |
| crates/agent-engine/tests/run_control_p7_4.rs（新） | 九类控制契约用例 + 端到端用例（见「三」） |
| crates/database/tests 四文件 + crates/plugin-runtime/tests/lifecycle_full_path.rs + crates/agent-engine 两个迁移测试 | 链尾断言按编号冻结规则顺延至 014（追加式断言更新，无既有能力回退；与 P7-3 顺延到 013 同款） |

## 二、九类控制语义（全部确定性，测试即规格）

1. **暂停**（协作式）：RunControl::pause 仅 RUNNING 可发起，写 agent_run_control.pause_requested_at 信号位；执行循环在轮间与步间消费（一次性，消费即清空），消费点立即 CAS RUNNING→PAUSED 并落 agent.run.paused 审计事件。QUEUED 不涉暂停，终态拒绝（domain 谓词）。
2. **恢复**：AgentLoop::resume 仅接受 PAUSED run；目标文本从 agent_run_control.goal_text 续接（run() 创建时落行），已完成步骤从 agent_run_step 轨迹续接（fresh run 的 goal_text 由 loop 落行，resume 不重传）；重复调用键与停滞已见哈希由轨迹重建（跨段连续）；轮次从轨迹最大轮次 + 1 续接；上下文重装配（每轮上下文唯一来源 Broker），偏移基准在新段首个计划面重置。终态 run 恢复拒绝。
3. **取消**（协作式，D-TC-04）：RUNNING 写 cancel_requested_at，循环步间消费后 CAS RUNNING→CANCELLED，finish_reason 带「cancelled (cooperative, D-TC-04)」；QUEUED/PAUSED 不在执行循环内，RunControl::cancel 直接跃迁 CANCELLED（域跃迁表合法对）；终态拒绝。
4. **重试**：RunControl::retry 仅接受 FAILED 源 run，按「新 run 重放同定义版本」设计（domain 跃迁表不复活终态行，P7-2 批次注明的 P7-5 口径在本批落地）：新 run QUEUED、同 agent_id/agent_version/task_id、agent_run_control.retry_of_run_id 链回源 run；源 run 保持 FAILED。新 run 的执行走 loop.run 正常流程。
5. **停滞检测**：进度定义 = 步骤 SUCCEEDED 且登记 evidence 且 artifact content_hash 出现新值；连续无新证据步数达 stall_window（默认 2）即停，finish_reason「guardrail: stall detected」+ agent.run.guardrail 审计。失败步因 fail-fast（P7-2 既有）不进累计。
6. **重复调用检测**：canonical_call_key = sha256(tool_id + 规范化 input)（serde_json Value 默认 BTreeMap 键序，键序无关）；已执行键命中即跳过（warning + agent.step.duplicate 审计，不入轨迹）；整轮全是重复即停（guardrail: duplicate call detected），防规划器死循环。键集合在 resume 时从轨迹重建。
7. **目标偏移检测**：确定性代理定义：初始计划面 = 本执行段首个产出步骤的 plan 的工具集合；面外工具跳转计偏移事件（容限内放行 + warning），连续偏移步数超过 drift_tolerance（默认 1）即停（guardrail: goal drift detected，点名偏移工具）。能力绑定闸先行（面外工具直接拒绝，不进偏移计数）。
8. **证据不足提示**：收尾时可用证据（SUCCEEDED 且登记 evidence）少于 min_evidence_hint（默认 1）即在 warnings 留「insufficient evidence: N usable evidence rows ... conclusions 待验证」；提示不改变终态（与 ROADMAP §2 报告纪律一致：未验证的写成待验证）。
9. **预算与资源限制**：轮次预算 max_rounds（默认 8，0 = 不限）、墙钟预算 max_elapsed_ms（默认 0 = 不限，步间检查）、步数预算（P7-2 既有 max_tool_calls）；资源限制 = 并行子任务默认关闭（allow_parallel_subtasks 默认 false，低并发纪律 ROADMAP §8.5 风险 1），单轮多步骤一律串行执行并在 warnings 留「parallel subtasks disabled ... serialized」。护栏停机均落 agent.run.guardrail 审计事件。

## 三、验收（实测）

### 3.1 九类控制契约用例（tests/run_control_p7_4.rs，10 passed 0 failed）

| 控制项 | 用例 | 关键断言 |
| --- | --- | --- |
| 暂停 | pause_contract_request_is_signal_and_loop_consumes_it | 请求只写信号位；终态拒绝（Refused(FAILED)）；消费后请求位清空；run 落 PAUSED |
| 恢复 + 端到端 | pause_resume_e2e_state_continuity_no_rerun_and_audit_chain | 见 3.2 |
| 取消 | cancel_cooperative_and_queued_direct | RUNNING 协作式（CANCELLED + finished_at + agent.run.cancelled）；QUEUED 直接跃迁（CancelledNow）；SUCCEEDED 拒绝；不存在报 RunNotFound |
| 重试 | retry_replays_failed_run_as_new_run_with_link | 新 run QUEUED 同定义版本同 task；retry_of_run_id 链回源；源 run 保持 FAILED；非 FAILED 源拒绝 |
| 停滞检测 | stall_detection_stops_after_window_without_new_evidence | 3 步触发窗口 2；finish_reason「guardrail: stall detected (2 steps ...)」+ guardrail 审计 |
| 重复调用检测 | duplicate_call_detection_skips_and_stops_when_exhausted | 同键步跳过（warning + agent.step.duplicate）；整轮全重复即停；轨迹仅 1 步 |
| 目标偏移检测 | goal_drift_detection_stops_after_tolerance | 轮 0 面内成功；轮 1 面外放行 + warning；轮 2 连续偏移超容限停（第三工具不执行） |
| 证据不足提示 | insufficient_evidence_hint_appears_on_zero_usable_evidence | 失败 run warnings 带「insufficient evidence: 0 usable ...」；成功 run 对照组不触发 |
| 预算与资源限制 | budget_rounds_and_parallel_serialization | max_rounds=1 截停（finish_reason「guardrail: budget max_rounds=1 reached」）；单轮 2 步串行执行 + 「parallel subtasks disabled ... serialized」；轨迹 seq 0/1 同轮 |

### 3.2 端到端用例（ask 指定：一次 run 暂停后续跑，已完成步骤不重跑且 audit_event 事件链连续）

pause_resume_e2e_state_continuity_no_rerun_and_audit_chain 实测链路：

1. ControlArmedPlanner 在轮 1 对控制面发起协作式暂停请求（PlanContext.run_id 视图）；轮 0 的步骤真实执行（计数驱动 1 次）后循环消费请求，run 落 PAUSED，轨迹 1 步，agent_run_control 带 goal 原文。
2. resume()：PAUSED→RUNNING CAS（agent.run.resumed 审计）→ 轨迹续接 1 步（resumed_steps=1）→ 只执行轨迹之外的第二步（计数驱动增量恰 1 次，总计 2 次）→ SUCCEEDED。
3. 不重跑判据：第一步 tool_run_id 暂停前后逐字节一致；agent_run_step 恰 2 行（seq 0/1）。
4. audit_event 事件链连续（correlation_id 全程 = run_id，AuditEngine::trace 升序断言）：accepted < step.completed(步 1) < paused < resumed < step.completed(步 2) < finished；暂停收尾无 finished 事件。
5. 终态 run 再次 resume 拒绝。

### 3.3 回归与门禁（实测命令与输出）

```
cargo test --offline -p agent-engine
  26 + 2 + 2 + 1 + 1 + 10 = 42 passed 0 failed
cargo test --offline --workspace
  88 个测试目标，481 passed 0 failed
cargo clippy --offline --workspace --all-targets -- -D warnings
  Finished（0 error）
```

P7-2/P7-3 既有用例（closed_loop_p7_2 2 例、memory_contract_p7_3 2 例）在控制面默认参数下零改动通过：KeywordSearchPlanner 单工具闭环路径行为不变，Memory 落账路径不变。

## 四、批次对位与边界（坏消息如实单列）

1. **批次序号错位（同 P7-3 口径）**：编排 P7-4（第 4/7 批）内容对应 ROADMAP §8.3 表内 P7-5 行（运行控制）；ROADMAP 表内 P7-4 行（三类 Memory）已由上一批（编排 P7-3）覆盖。RELEASE 与迁移头注均按实际内容行标注。
2. **CLI 未挂控制面**：rp-agent 仍是 run/def-add/def-show/run-show 四个子命令，控制编排面（pause/resume/cancel/retry）只以 Rust API + 测试为验收面；CLI 的 --with-memory 开关与控制子命令属后续批次。CLI 运行的 run 自动带默认护栏（九类控制默认参数生效），步骤轨迹与控制行随之落库（014 两表）。
3. **resume 不带显式入参**：fresh run 的 explicit input 未持久化，resume 的规划器按历史与新计划自决（PlanContext.explicit_input 为 None）；如需跨段钉死入参，属后续批次持久化项。
4. **偏移检测是确定性代理定义**：以「初始计划面外工具跳转」为偏移语义，不是目标文本语义相似度（模型在环面属后续批次，ROADMAP §8.5 风险 3）；文档与代码注释均按代理定义如实标注。
5. **协作式语义的孤儿 run**：RUNNING 期进程崩溃后，请求位无人消费、run 行停留在 RUNNING；本批如实声明不覆盖（lib.rs 头注边界），孤儿 run 回收属后续批次。
6. **Research Memory 仅终态落账**：PAUSED 收尾不再触发 Research Memory 记录（P7-3 语义：PAUSED 是中间态，恢复收尾时再落）；终态路径行为不变，memory_contract_p7_3 用例回归通过。
7. **submit_key 形状变更**：`agent:{run}:{tool}` → `agent:{run}:{seq}:{tool}`（多步 run 下同工具两步共享 idempotency_key 直通面会串，属 P7-4 多步能力的前置修正）；tool_run 行 submit_key 值随之变化，无既有断言依赖旧形状（tool-engine 契约测试不校验 agent 前缀形状）。
8. **轨迹落库失败降级为 warning**：与 memory_refs append 同口径（旁路纪律）；但轨迹是 resume 续接权威面，缺失时 resume 无法续接该步，报告 warnings 如实暴露（不伪装连续性）。

## 五、回滚方式

- 代码：本批未提交，`git checkout -- crates/agent-engine crates/domain crates/database crates/plugin-runtime migrations/014_agent_run_control.sql` 可整体回退（与 P7-1..P7-3 未提交改动同区，回退前先与编排层确认切分）。
- 迁移：014 为追加式两表，回退 = 删除 014 文件 + 对已落 014 的开发库副本执行 `DROP TABLE agent_run_step; DROP TABLE agent_run_control; PRAGMA user_version=13;`（schema_version 登记行 14 留存不影响）；生产 platform*.db 本批零写入，无需回滚。
