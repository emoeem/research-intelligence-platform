# MA-2 批次验收报告：并发派发引擎（per-caller 预算化并行 + 三帽 Rust 强制 + FamilyLimiter per-caller + 027 caller 归属）

- 日期：2026-11-22
- 任务书：[MA-2-TASK.md](MA-2-TASK.md)（§0 裁决 D1-D10 / §1 锚点 A1-A16 / §3 步骤 S1-S7 / §4 九判据 / §6 八红线，全部照办，不发挥不加戏）
- 状态：实施完成，工作树留待审查收口（**未 commit / 未 push / 未 git add**——按批次指令，显式 add → commit → push → CI watch 属收口方动作；任务书 §3 S7 的该子句由审查收口执行）
- 环境：`CARGO_TARGET_DIR=/home/emo/rt3-target`，Rust 全部 `--offline`，测试 `--test-threads=1`

## 一、S1 基线核验记录

| 项 | 结果 |
|---|---|
| Cargo.lock 包数 | `grep -c '^\[\[package\]\]' Cargo.lock` = 399 ✓（S7 复核同值，`git diff Cargo.lock` 为空，红线 6 双点闭环） |
| git status | 干净（仅未跟踪 `docs/rip3/MA-2-TASK.md`）✓ |
| 三 crate 测试基线 | `cargo test --offline -p agent-engine -p tool-engine -p database -- --test-threads=1` exit 0 全绿（基线跑法有缺陷：管道 `tail` 吞掉 cargo 退出码与计数，后经 S7 全量复跑补证，见坏消息 6） |
| selftest_auth.sh 基线 | PASS=28 FAIL=0 exit 0（含 MA-1 #8 多 key 块，判据 5 before 基线） |
| 锚点复核 | A1-A16 函数名逐一重定位：control.rs `RunControlConfig`/`allow_parallel_subtasks`、agent_loop.rs :610-617 警告位、store.rs :271 create_run、tool_link.rs RunDriver/ToolCallSpec、podman_host.rs :131-196/:759-761、lib.rs submit :161/submit_correlated :292、rp-admin.rs :9-13/:42、rp-workflow.rs :879——无漂移（行号按本档实测，落点以内聚函数名锚定） |
| migrate.rs:62 守卫语义 | 复读确认：ALTER ADD COLUMN 逐行识别 + 单次执行语义，四 ALTER 单行书写 ✓ |

## 二、改动文件清单（git diff --numstat 实测：41 修改 + 4 新增）

### 2.1 任务书 §2.1 落点（批次主体）

| 文件 | 行数 | 说明 |
|---|---|---|
| `migrations/027_caller_attribution.sql` | 新增 92 行 | D1/D2：四表 `ADD COLUMN caller_id TEXT`（**四 ALTER 单行书写**，migrate.rs:62 守卫语义）+ 四索引（ix_research_task_caller / ix_workflow_run_caller / ix_agent_run_caller / ix_tool_run_caller）+ `caller_budget` 配置表（caller_id 真 FK→caller，026:52 先例；CHECK 域与时间 GLOB 沿 024/025/026 口径；六帽列缺省 0=并行关/帽不限/沿用 LoopConfig，rate_window_ms 缺省 60000 且 >0）+ schema_version=27 登记行 + `PRAGMA user_version = 27`；头注含 018:50-53 ALTER 单次执行语义 + 026:5-6 027 预留声明对账 + 计划 :46 引用 |
| `crates/agent-engine/src/agent_loop.rs` | +802 / -6 | RunRequest.caller_id（D4①）+ DriveParams.caller_id + 受理面跨 run 并发帽 BudgetDenied 硬拒（D5.4，create_run 前）+ drive() caller 预算面解析（D7 分层）+ 每轮前 caller 总预算闸（D5.3）与速率闸（D5.2：tool_run.caller_id 行 COUNT + AtomicUsize/Instant 进程内计数复核取 max）+ :610-617 双支路（放行三与条件 D3，串行路径逻辑零改动，既有 warning 条件扩为「并行关或预算面不齐」）+ `run_round_parallel` fork-join 执行器（DispatchQueue 共享 deque+Condvar 帽排队不拒绝、派发间隙消费 cancel/pause/墙钟/速率复核、R5 预估 PARALLEL_PLAN 审计 + warnings 实测数字、GUARDRAIL kind=budget_caller_parallel/rate/total、主线程收齐终态后按 plan 序统一落 agent_run_step（seq=steps.len()+ordinal，UNIQUE(run_id,seq) 确定性）/memory/审计）+ `execute_parallel_step` 子步执行（受理+驱动+Evidence 登记；轨迹/审计主线程统一落）+ `CallerRateTracker`（std AtomicUsize+Instant，零新依赖） |
| `crates/agent-engine/src/control.rs` | +93 / -5 | CallerBudget 结构体 + RunControlConfig.caller_budgets 字段（Default 空表 = 既有六字段语义零改动，红线 1）+ `load_caller_budgets` 装配层读表（D7：JOIN caller revoked_at IS NULL——停用即失效，引擎不直查配置表）+ actions 词表区加 `PARALLEL_PLAN` 常量（control.rs:59 词形纪律）+ retry 继承源 run caller（D4③，:203 同点传参） |
| `crates/agent-engine/src/store.rs` | +41 / -7 | create_run 扩参 caller_id + INSERT agent_run 扩列（None → NULL，零回归）+ StoredRun.caller_id 读列（trace_id Option 先例同款）+ run() SELECT 扩列 + `count_active_runs_by_caller`（D1 记账公式 COUNT WHERE caller_id AND state IN (QUEUED,RUNNING)） |
| `crates/agent-engine/src/error.rs` | +4 / -0 | AgentError 加 `BudgetDenied(String)`（additive，D5.4）+ Display 臂 |
| `crates/agent-engine/src/tool_link.rs` | +26 / -4 | RunDriver 加 `: Send + Sync` 超界（D3 fan-out 跨线程共享前提；三个实现均为纯数据持有）+ ToolCallSpec.caller_id（D4④）+ ToolLink.call 路由：caller Some → `submit_for_caller`，None → 既有 `submit_correlated` 逐字节不变 |
| `crates/agent-engine/src/bin/rp-agent.rs` | +29 / -1 | `--caller-id` / `--allow-parallel` flags + caller_budget 启动一次性装配读取（失败降级串行 + warn，rp_server.py:236-239 先例同源）+ with_control 装配 + usage 文本更新 |
| `crates/tool-engine/src/adapter/podman_host.rs` | +157 / -4 | LimiterInner 加 caller_active 记账 + caller_cv + FamilyGuard.caller（Drop 双释放，锁序恒 slots→caller_active）+ `acquire_for` / `try_acquire_for` 双层判定（族内 default_limit 既有逻辑 :159/:180 **零改动**，caller 顶层帽独立叠加）+ `caller_active` 观测面 + ContainerCall 加 `caller`/`caller_cap`（D7）+ 挂点 :759-761 改造（caller None → 既有 acquire 直通路径逐字节不变） |
| `crates/tool-engine/src/lib.rs` | +57 / -2 | submit 内核改 `submit_with_caller`（INSERT tool_run 扩 caller_id 列，None → NULL 行形状零变化）+ `submit_for_caller`（submit_correlated 邻位，D4④/D5.2）+ `caller_id_of` 读面；submit_correlated 既有实现零 diff |
| `crates/database/src/bin/rp-admin.rs` | +440 / -1 | D6 四子命令：budget-set（UPSERT ON CONFLICT + caller 在用校验）/ budget-show（无行 exit 2）/ budget-clear（幂等 cleared=true/false）/ caller-revoke（幂等 already_revoked，key-revoke 同款）+ usage + Mode 变体 + 手写解析（0/1/2 口径沿用 rp-admin 既有形态：0 ok/help、2 参数与执行错误）+ `#[cfg(test)]` 预算面回环单测（临时库真实迁移链） |
| `crates/agent-engine/tests/ma2_parallel_budget.rs` | 新增 508 行 | 三硬验收 e2e（判据 2/3/4）+ 判据 5 legacy 守卫 + 判据 6 retry 继承 + D5.① 帽排队审计（budget_caller_parallel tasks/cap/queued 实测断言）；共享 Pool 跨线程观测，TimedDriver 并发峰值观测面 |
| `crates/tool-engine/tests/podman_host_budget.rs` | +156 / -0 | 判据 7：acquire_for 双层判定单测（族内满 None / caller 顶层满 None / 另一 caller 同族可进 / cap=None 不限 / 阻塞 acquire_for 帽位释放后进入）+ ContainerCall.caller=None 挂点直通 e2e（podman 载具，V-9 SKIP 守卫） |
| `apps/server/selftest_ma2.sh` | 新增 209 行 | CLI 实测（判据 2/3/4/8，数字见四） |
| `docs/rip3/MA-2-BATCH-REPORT.md` + `docs/README.md` | 新增 / +1 | 本报告 + 索引登记 |

### 2.2 链尾断言随更（S2，只改预期值与描述文本，红线 5）

`migrations_agent_data_plane.rs`（6/6）、`migrations_context_broker.rs`（7/6）、`migrations_tool_contract.rs`（7/6）、`migrations_runtime_node.rs`（6/6）、`backup_restore.rs`（4/3）、`migration_idempotent.rs`（5/4：vec 1..26→1..27 + user_version 26→27）、`migration_dev_copy_p7_3.rs`（5/5）、`migration_replay.rs`（3/3）、`caller_auth.rs`（3/3：Some(26)→Some(27)）、`wsw-message/src/store.rs`（2/2）、`wsw-message/tests/cas_e2e.rs`（8/8）、`wsw-runtime/src/persist.rs`（3/3：assert_watermark_25 断言值 26→27，方法名沿 025 批次先例不改）、`wsw-runtime/tests/exchange_persist.rs`（2/2）、`wsw-runtime/tests/intercept_queue.rs`（1/1）。A16 清单列名五文件，实际随 027 入链需同步的断言文件共十四个（MA-1 批次同款先例：链长/链尾钉面随链演进，只改预期值）。

### 2.3 编译机械适配（struct 字段扩列的连带，行为零变化）

`ContainerCall` 加 caller/caller_cap 两字段（D7 mandate）导致 22 处既有字面量构造点补 `caller: None, caller_cap: None`：`intel-db` src ×4（gov_worker/embed_worker/parser/search，各 +2/-0）、`mcp-server/src/dispatch.rs`（+2）、`runtime-gateway/tests/recovery_drill.rs`（+2）、`verification-engine/src/provider.rs`（+2）、`workflow-engine/src/bin/rp-workflow.rs`（+3，含 RunRequest 字面量 `caller_id: None`）、tool-engine tests ×5（podman_host_e2e 8/0、podman_host_egress 8/0、core_utils_e2e 2/0、web_research_e2e 2/0、src_analysis_e2e 2/0）。`RunRequest`/`ToolCallSpec` 字段扩列的测试机械点：run_control_p7_4.rs（run_request 助手 + create_run 扩参 ×4）、closed_loop_p7_2.rs（+2）、memory_contract_p7_3.rs（+1）、trace_correlation_p7_5.rs（+1）、tool_link.rs 内嵌测试（+2，`caller_id: None`）。**全部为 None 直通值，零行为变化**；与「现有测试零改动」字面要求的冲突与处理见坏消息 2。

## 三、§5 测试面逐条实测

| # | 命令 | exit | 输出摘要 |
|---|---|---|---|
| 1 | `cargo test --offline -p agent-engine -- --test-threads=1` | **0** | lib 28 ok（含 control 三 caller 面新断言零新增——既有 9 用例零改动全过）+ run_control_p7_4 10 ok + closed_loop 2 ok + memory_contract 1 ok + trace_correlation 3 ok + migration_idempotent 1 ok（001..027 全链幂等 uv=27）+ migration_dev_copy 1 ok（013..027）+ **ma2_parallel_budget 8 ok** + rp-agent bin 单测 1 ok；零 FAILED |
| 2 | `cargo test --offline -p tool-engine -- --test-threads=1` | **0** | lib 54 ok + contract_min_set 11 ok + podman_host_budget **8 ok**（含新增 family_limiter_acquire_for_two_layer_verdict；container_call_none_caller 在本机以 V-9 SKIP 路径过——本地镜像库无验收镜像，skip=return 照常 exit 0）+ core_utils_e2e 20 ok（**:528-534 族基线断言零改动通过**）+ first_batch_b2 2 ok + 其余全绿；零 FAILED |
| 3 | `cargo test --offline -p database -- --test-threads=1` | **0** | lib 11 ok + rp-admin bin **6 ok**（含新增 ma2_budget_subcommands_roundtrip_and_revoke_idempotent）+ migrations_* 四文件 9 ok（001..027 链尾 uv=27 + 幂等）+ backup_restore 4 ok；零 FAILED |
| 4 | `cargo clippy -p agent-engine -p tool-engine -p database --all-targets --offline -- -D warnings` | **0** | 首跑红 3 处（doc_lazy_continuation ×2 + type_complexity ×1，均在 agent_loop.rs 新增面）→ DispatchQueue type alias + 头注改写后全绿（过程如实记，坏消息 7）；另对机械适配五 crate（workflow-engine/mcp-server/intel-db/verification-engine/runtime-gateway）加跑 clippy -D warnings 同绿 |
| 5 | `cargo deny check advisories` | **0** | `advisories ok` |
| 6 | `cargo deny check licenses` | **0** | `licenses ok` |
| 7 | `grep -c '^\[\[package\]\]' Cargo.lock` | **0** | `399`（红线 6 双点复核；git diff Cargo.lock 为空） |
| 8 | `bash apps/server/selftest_auth.sh` | **0** | PASS=28 FAIL=0（既有 11 例 + MA-1 #8 块零改动全过） |
| 9 | `bash apps/server/selftest_phase6.sh` | 1 | PASS=48 FAIL=1；唯一 FAIL = 「rp_server.py 全文件无 SQL 写语句」——**S1/MA-1 基线同款既有红**（MA-1 报告坏消息 1），与本批无关，rp_server.py 零 diff 实证 |
| 10 | `bash apps/server/selftest_phase7_agents.sh` | **0** | PASS=50 FAIL=0（rp-agent 真实 python_worker 闭环 + 生产库指纹一致；rp-workflow.rs 机械适配后零回归） |
| 11 | `bash apps/server/selftest_ma2.sh` | **0** | PASS=34 FAIL=0（本批新增，数字见四） |

`git status --short` 收口复核：41 修改 + 4 未跟踪（selftest_ma2.sh / ma2_parallel_budget.rs / 027_caller_attribution.sql / MA-2-TASK.md 任务书本身）——全部落在 §2.1 落点 + 链尾随更 + 编译机械适配三类（见二、坏消息 2/3）；`migrations/001-026` 零 diff、`Cargo.toml`/`Cargo.lock` 零 diff、生产 `platform*.db`/`platform-dryrun.db`/`integrations/rtis/` 零 diff（phase6/phase7 指纹断言实证）。

## 四、三硬验收实测数字（真实计时，禁模拟冒充）

**硬验收①（判据 2）单 caller N=3 并行提速**（`cargo test -p agent-engine --test ma2_parallel_budget hard_acceptance_parallel_speedup -- --nocapture` 实测输出）：

```
MA-2 判据2 计时：串行=909ms 并行=308ms 比值=0.339（并行帽=3，步延时 300ms）
```
- 并行总耗时 308ms < 串行 909ms × 0.7 = 636ms ✓（比值 0.339）
- 驱动侧并发观测峰值 peak=3 ≥ 2 ✓（TimedDriver AtomicUsize fetch_max 实测）
- 断言：`parallel_ms * 10 < serial_ms * 7` 与 `driver_parallel.peak() >= 2` 双硬界

**CLI 实测（selftest_ma2.sh，真实 python_worker 端到端）**：

```
MA-2 CLI 计时汇总：串行=465ms 并行=179ms（N=3 跨 run；两次实跑分别 451/179、465/179）
MA-2 CLI 计时：a 满载窗口内 b-run 全程 = 180 ms（第二次实跑 181 ms）
```
- 口径说明（如实）：CLI 侧「并行」= 3 次 rp-agent run 并发受理（跨 run 并发帽），串行 = 3 次顺序 run；单 CLI 进程内步级 fan-out 因内置 KeywordSearchPlanner 每轮单步而不触发（步级并行提速的 <0.7 硬界由 Rust e2e 模拟驱动承载——判据 2 原文「三步各 ~300ms 模拟驱动」即此载具）；python_worker 适配器自身有 exec_lock 串行纪律（D7 两通道零实现边界）。CLI 数字如实记录，不冒充步级并行数字。

**硬验收②（判据 3）超预算硬拒三点**（各有实测留痕）：

| 拒绝点 | 实测 |
|---|---|
| 速率帽窗口超限 | e2e：rate_max_steps=2 首轮 2 步执行、次轮新规划步整轮硬停，`finish_reason: guardrail: caller rate budget` + GUARDRAIL `kind=budget_caller_rate`（审计 detail 含 window_steps/proc_window_steps 双计数）+ 超帽步骤不执行（steps=2，tool_run.caller_id 行 COUNT=2）；CLI：第 3 次 run exit 1 + finish_reason 命中 + audit_event budget_caller_rate 留痕 2 行 |
| 总预算步数到位 | e2e：budget_max_steps_per_run=2，`finish_reason: guardrail: caller total budget (budget_max_steps_per_run=2) reached` + GUARDRAIL `kind=budget_caller_total`，2 步后新规划步不执行 |
| 受理面跨 run 并发帽 | e2e：max_concurrent_runs=1，a-run RUNNING 中第二 run `Err: budget denied: caller caller-adm concurrent runs 1 at cap 1 (max_concurrent_runs); new run rejected before create_run`；agent_run 表该 caller 仅 1 行（**run 不落行**）；CLI 同款 exit 1 + stderr `budget denied` + 落行数=1 |

**硬验收③（判据 4）双 caller 互不抢占**：

- e2e：caller-a 双 run 满载（2 RUNNING，wait_active_runs 轮询确认）时 caller-b run 全程 **7ms** 完成受理与执行（a 剩余执行时长 >600ms 窗口内）；a 第三 run `budget denied: ... concurrent runs 2 at cap 2` 硬拒，b 同刻第二 run SUCCEEDED ✓
- CLI：a 满载（在飞记账 1/1）窗口内 b-run 全程 **180ms** 受理并执行成功 exit 0 ✓
- tool-engine 层：acquire_for 单测断言 caller-1 顶层帽占满（2/2）时 caller-2 同族可进（`b1` expect 通过，判据 4 末句）✓

## 五、九判据逐条对照

| # | 判据 | 状态 | 实测 |
|---|---|---|---|
| 1 | 027 迁移纯增量幂等 | **PASS** | 临时库实跑：27 个迁移首跑 user_version=27；重跑（执行器语义）零变更（migration_idempotent schema 快照零差异）；手工重复执行 027 报 `duplicate column name: caller_id` = 预期守卫（018:50-53 语义，S2 实录）；四 ALTER + 四索引 + caller_budget 全部落列（python3 临时库逐表 PRAGMA table_info 断言）；回滚 = git revert + 库残留四可空 NULL 列/空表（无害，drop 说明登记见 §八） |
| 2 | 硬验收①并行提速 | **PASS** | 串行=909ms 并行=308ms 比值 0.339 < 0.7；驱动侧峰值 3 ≥ 2；CLI 数字如实入四 |
| 3 | 硬验收②超预算硬拒 | **PASS** | 三拒绝点各有实测（见四），审计原文全落 |
| 4 | 硬验收③双 caller 互不抢占 | **PASS** | e2e b-run 7ms @ a 满载 / CLI b-run 180ms；a 第三 run BudgetDenied、b 同刻成功；acquire_for 顶层帽互不抢占单测通过 |
| 5 | legacy/default 零回归 | **PASS** | caller_id=None 全链路：INSERT 列值 NULL、ToolCallRequest JSON 契约零 diff（request.rs 零改动实证）、串行路径零 diff（agent_loop 串行分支逐行保留，仅分支包裹）；既有 selftest 三套零改动通过（auth 28/0、phase6 48/1 既有红、phase7_agents 50/0）；core_utils_e2e.rs:528-534 族基线断言零改动通过 |
| 6 | caller 落库与继承 | **PASS** | agent_run.caller_id / tool_run.caller_id 落列（e2e + CLI 直查断言）；retry 新 run 继承源 run caller（e2e retry_chain_inherits_caller_id_from_source_run：FAILED 源 run caller-retry → 新 run 同 caller + trace tr-ma2-retry 双继承）；trace_correlation_p7_5.rs 零改动通过 |
| 7 | tool-engine per-caller 帽 | **PASS** | acquire_for 双层判定单测：族内满 → None；caller 顶层满 → None（strict cap 1 第二族即拒）；**另一 caller 同族可进**；ContainerCall.caller=None 行为不变（挂点直通 + try_acquire 既有路径断言）；族基线断言（core_utils_e2e.rs:528-534）零改动通过 |
| 8 | rp-admin 预算面完整 | **PASS** | budget-set/show/clear + caller-revoke JSON 输出 + 退出码 0/2 口径（rp-admin 既有形态：0 ok/help、2 参数与执行错误）；bin 单测回环（set→show→upsert→clear 幂等→revoke 幂等→停用后 set 拒绝）；budget-set 后 CLI 立即可装配（selftest 顺序实跑：set→agent run 即生效） |
| 9 | 门禁全绿 | **PASS** | 链尾断言随更后全绿（三 crate + parity caller_auth）；clippy -D warnings 过（三 crate + 机械适配五 crate）；deny 双检 ok；Cargo.lock 399 恒定；git status 只含 §2.1 落点 + 链尾随更 + 编译机械适配三类（偏差单列坏消息 2/3） |

## 六、坏消息（单列）

1. **「现有测试零改动通过」与 D4 结构体扩列的硬冲突（偏差，已按最小面处理）**：RunRequest/ToolCallSpec/ContainerCall/StoredRun 加字段（D4/D7 明文 mandate）使既有测试的 Rust 字面量构造点无法通过编译（Rust 结构体字面量必须穷举字段，无 Rust 语言层面的缺省字段机制）。处理：既有测试仅补 `caller_id: None` / `caller: None, caller_cap: None` / create_run 扩参 `None` 机械值（§2.3 清单，21 处），**断言与钉值零改动**；行为不变式以「caller_id=None 全链路逐字节现状」验收（INSERT 列值 NULL、JSON 契约零 diff、既有测试断言零改动全绿）。这是语言级不可两全，不是设计取舍。
2. **§2.1 落点清单外的连带改动面**：ContainerCall 字段扩列强制 intel-db ×4 / mcp-server / runtime-gateway / verification-engine / workflow-engine（rp-workflow.rs）各补 None 直通字段（§2.2 清单外的 10 个文件）——D7 mandate 的机械连带，全部 None 直通零行为变化；`rp-workflow.rs` 同时是 RunRequest 字面量持有者。§2.2「workflow-engine 零 diff」按字面未能满足（3 行 None 字段），如实登记。
3. **链尾断言随更实际十四文件**：A16 清单列名五文件 + migration_idempotent，实际 027 入链使所有链长/链尾钉面（runtime_node / migration_replay / caller_auth / wsw 三组）全部需要随更——MA-1 批次同款先例（MA-1 报告 §二列 22 文件同类面），只改预期值与描述文本，不改结构。
4. **fan-out 执行器首版缺陷（批内发现即修，如实记录）**：worker 从共享队列取件时把「空队列」误判为「关闭」提前退出（sync_channel Receiver 不可 Clone 改共享 VecDeque 方案引入），并发度实际退化 1 且表现不稳定（peak=1、比值 1.002 首跑假阴性）。修复：worker 空队列在 Condvar 等待、退出只认毒丸（Some(None)）+ notify_one/notify_all 配对。修复后 peak=3、比值 0.339 稳定复现（三次实跑 0.339/0.338/0.339）。该缺陷属实现 bug 非设计缺陷，未出批。
5. **tool-engine INSERT 列序错位一次（批内发现即修）**：submit INSERT 扩 caller_id 列时首版参数序与列序错位（replay_json 落 caller_id 列），first_batch_b2（真实 python_worker 路径）红「keyword must be a string」暴露；git stash 对照确认 HEAD 绿、本批红后定位修复。修复后三 crate 全绿。教训：扩列类改动以 b2 真实驱动为第一冒烟面。
6. **S1 基线跑法缺陷（自曝）**：基线命令 `cargo test ... | tail -60` 使 exit code 是 tail 的而非 cargo 的，且计数被截断——基线全绿结论在 S7 全量复跑补证后才成立（本报告三命令均带完整 result 计数）。后续批次基线命令应输出到文件再 grep。
7. **clippy 首跑红 3 处**（新增面 doc 缩进 ×2 + DispatchQueue type_complexity ×1），改写后绿；非既有面回归。
8. **podman 验收镜像本地缺位**：container_call_none_caller e2e 在本机走 V-9 SKIP 路径（`ensure_image` 返回 None 即 return，照常 exit 0）——该用例的容器面验证依赖 CI/本地镜像库在位时执行；limiter 双层判定单测（判据 7 主体）不依赖 podman，全绿。

## 七、设计裁决执行口径备忘（MA-3 输入）

1. **D5.3 caller 总预算闸语义**：按「已执行步数 ≥ budget_max_steps_per_run 即整轮硬停」实现（判据 3「总预算步数到位」口径；单轮内计划步数超帽不截断——本轮全执行、下一轮闸口硬停），测试规划器按此构造确定性触发。MA-3 若需「预执行截断」语义，另立裁决。
2. **速率窗口秒级粒度**：tool_run.created_at 与窗口下界均为秒级 ISO8601（domain::time 口径，024 GLOB 毫秒格式不参与该比较面），rate_window_ms 取整秒；亚秒窗口由进程内 AtomicUsize/Instant 计数复核兜底。
3. **并行支路的 cancel/pause 消费点**：轮顶（既有）+ 派发间隙（每项派发前）；消费后 CAS 跃迁在主线程收齐终态后统一落（已派发子步照常收齐落库，未派发项不执行）——协作式语义在 fork-join 下的最小自然延伸。
4. **MA-3 前置输出**（任务书 §8.4）：workflow_run.caller_id / research_task.caller_id 落列零写面（027 已落列，写面随节点角色绑定与 RTIS 派发面接线）；caller_budget 配置形状 = caller_budget 表七列（§2.1），成员建队预算传递输入；三项观察项登记：R7 成员级模型可配、RT-3 constraints caller 维度（D9：本批零触碰，policy.rs 零 diff 实证）、native/python 通道级 caller 帽（D7：两通道零实现，exec_lock 串行纪律在位）。
5. **api_key.last_used_at 维持零写入**（D6 显式登记）：本批 Rust CLI 直传 caller_id 不经 key 明文，server 侧写入违 MA-1 红线 3，留后续批。

## 八、回滚

- 单 commit 交付（收口方执行）；`git revert` 整批撤销。027 对已迁移库残留四可空 NULL 列 + caller_budget 空表（无害；SQLite DROP COLUMN 为 3.35+ 特性且禁改历史迁移，残留不清理——回滚说明沿用 MA-1 报告 §八同款登记，drop 参考语句：`ALTER TABLE research_task DROP COLUMN caller_id;`（四表同）+ `DROP TABLE caller_budget;` + `PRAGMA user_version = 26`，仅限开发库手工执行）。
- 代码面零回归设计使 revert 风险低：caller_id=None 全链路逐字节现状；并行支路缺 caller 面自动落串行；caller_budget 读取失败降级串行并记一次 warn（rp_server.py:236-239 先例同源），不炸 run。
- tool-engine acquire_for 纯新增 + 挂点保留 None 直通路径；rp-admin 四子命令纯新增，revert 后零残留。
