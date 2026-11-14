# 批次报告：P10-3（运行与质量门，Phase 10 第 3/4 批）

> 状态：已实施（本批未 commit，由协调方收口）；日期：2026-11-14；适用版本：Phase 10 批次工作树。
> 相关：docs/phase10/P10-3-TASK.md、docs/phase10/PERF-BASELINE.md、docs/phase0/PHASE-ROADMAP.md §11.3。

## 一、判据逐条对账（判据原文、命令、实测输出）

### 判据 1（P10-3）：双 Runner（podman_vm + podman_host 或双 VM）下调度与心跳正确；单节点离线不丢任务

命令与实测（仓库根执行）：

~~~text
$ cargo test --offline -p runtime-gateway --test multi_runner_failover
running 5 tests (4 active + 1 ignored bench)
test result: ok. 4 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.11s
~~~

新增用例（`cargo test -- --list` 实列）：

1. `dispatch_prefers_primary_then_fails_over_to_backup`：主节点健康时 QUEUED 落主节点（node-vm-p103）；主节点探针翻转 broken 后派发 QUEUED 落备用节点（node-host-p103，不落 WAITING_FOR_RUNNER），runtime_node.health 行同步（vm=broken、host=ok 实证）；两节点全离线时落 WAITING_FOR_RUNNER 挂主节点（§5.6:336 不 FAILED），此时 probe_and_resume 拒绝（RunnerUnavailable，run 保持原态）。
2. `waiting_run_resumes_when_any_candidate_recovers`：全离线落 WAITING 后仅备用恢复，恢复判定经备用候选通过回 PREPARING，`drive_resumed` 续跑到 SUCCEEDED，轨迹 [WAITING_FOR_RUNNER, PREPARING, RUNNING, SUCCEEDED]（恢复续跑不丢任务）。
3. `recover_waiting_runs_resumes_all_and_reports_skips`：两笔 WAITING run 批量恢复（healthy_nodes=[node-vm-p103]、resumed 按 created_at 序两条），全离线时批量恢复拒绝且不触碰任何 run；已离开 WAITING 的 run 不被二次搬动。
4. `set_primary_reorders_failover_priority`：set_primary 把目标节点移到候选队首（candidate_nodes=[node-host-p103, node-vm-p103] 实证）、未登记节点拒绝（UnknownNode）。
5. `bench_dispatch_latency`（#[ignore]，本地基准）：METRIC dispatch_ms=0.427（n=30 中位数，单次实跑）。

实现面：crates/runtime-gateway/src/lib.rs（候选序 `candidate_nodes`、dispatch 逐候选探测首个可用承接、probe_and_resume 多候选、`recover_waiting_runs` + `RecoveryReport`）；crates/tool-engine/src/lib.rs 新增只读枚举面 `Engine::runs_by_status`。既有 P2-4b 语义零推翻（wait_runner.rs 4 用例全绿，含单节点离线落 WAITING 与恢复回环、600s 不 tight-poll 守卫）。

### 判据 2（P10-4）：T0/T1/T2 越界拒绝矩阵、secrets 不泄露核查（每类一组的自动化测试进 CI）

命令与实测：

~~~text
$ cargo test --offline -p tool-engine --test security_matrix
running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.22s
~~~

矩阵用例（--list 实列，口径引用 T1-T2-BOUNDARY-RULING §1 与 D-TC-05/D-TC-06 原文）：

1. `registration_rejects_permission_level_outside_t0t1t2`：T3/t2/T9/空串登记全部拒绝，回显含 "not in T0/T1/T2"（validate.rs 第 8 条既有强制面固化）。
2. `t2_without_authorization_ref_rejected_policy_denied_with_no_row`：T2 缺授权引用 POLICY_DENIED（rejection.code() == "POLICY_DENIED" 实证）且 tool_run 零落行（§3.2 第 1 行）。
3. `t2_with_authorization_ref_accepted_and_ctx_recorded`：带 authorization_ref=eng-2026-11-14-001 受理，permission_ctx 记录 permission_level=T2 与引用原文。
4. `t0_t1_without_authorization_ref_accepted_no_gate`：T0/T1 缺授权引用照常受理（裁决「闸只在 T2；写平台内部库不升 T2」固化）。
5. `request_protocol_rejects_secrets_field`：调用对象携带 secrets 键被结构检查拒绝（unknown request field，回显引用 D-TC-06），零落行。
6. `manifest_secrets_carry_names_not_values`：值形键（value）拒绝、小写键名拒绝（^[A-Z][A-Z0-9_]+$）、required secret 缺平台 secrets 存储侧信拒绝（ValidationRule::Secrets）。
7. `planted_secret_value_never_enters_registry_run_rows_or_envelope`：planted 假想 secret 值在 tool/tool_version/tool_run 全行全列与终态信封逐字节扫描零命中；secret 名不进 tool_run 行、不进登记存储行（store.rs:40 provenance_json 只取 manifest.provenance 字段）、不进信封重放。

进 CI 方式：ci.yml rust job 既有门 `cargo test --offline --workspace` 覆盖上述两载具（workspace 全量含 runtime-gateway 与 tool-engine 的 tests/ 目录），ci.yml 头注释已显式点名（本批 ci.yml 仅注释改动，门禁行为零改动）。

### 判据 3：autoresearch 性能基准计量闭环（覆盖 P10-4「性能测试以不回归为门」）

实测闭环（autoresearch MCP 工具链，.auto/ 在会话工作目录 /home/emo/code/pensting）：

1. init_experiment(name=rip-p10-perf-baseline, metric=assemble_ms, lower)：segment 1 建立，.auto/log.jsonl 落 config 行。
2. run_experiment(.auto/measure.sh, repeat=3)：三次运行 exit 0，assemble_ms runs [2.399, 2.463, 2.148] 中位数 2.399；dispatch_ms runs [0.474, 0.454, 0.475] 中位数 0.474（secondary 中位数聚合，输出 METRIC 行原文在列）。
3. log_experiment(status=noop, metric=2.399, metrics={dispatch_ms: 0.474})：ledger run 行落库（noop 不 commit 不 revert，批次纪律）。
4. export_dashboard：autoresearch-dashboard.html 产出（live URL 127.0.0.1:46089）。

基线对照既有记录：Phase 4 装配耗时与 Phase 2 派发延迟在既有验收文档中均无绝对数字（grep「耗时|elapsed|latency|延迟」对 docs/phase2 与 docs/phase4 实测零命中数值，仅 ROADMAP §11.5 风险 4 提及口径），故本批落首版基线（数值、口径、不回归门 20% 阈值、可比性边界）于 docs/phase10/PERF-BASELINE.md，不设绝对指标。基准载具为 `#[ignore]` 测试（bench_assemble_latency / bench_dispatch_latency），保持本地执行不进 CI（ci.yml 注释写明）。

## 二、门禁汇总（本批实跑）

| 门 | 命令 | 结果 |
| --- | --- | --- |
| 指定测试门 | `cargo test --offline -p runtime-gateway -p tool-engine` | 22 个测试目标 106 passed 0 failed 1 ignored（awk 聚合 test result 行，日志 /tmp/p103_test_gate.log） |
| clippy | `cargo clippy --offline --workspace --all-targets -- -D warnings` | Finished dev profile，exit 0，零告警 |
| 测试分布 | 同上 | runtime-gateway：multi_runner_failover 4 passed + 1 ignored、wait_runner 4 passed、recovery_drill 3 passed；tool-engine：security_matrix 7 passed、contract_min_set 37 passed 等全部绿 |
| dash 检查 | grep 扫描 U+2014 与 U+2013 字面（em dash 与 en dash） | 全部新文档零命中（本批五个新 md 与 log.jsonl 副本实测） |

生产库红线：本批全部测试与基准走内存库（载具代码 DbPath::Memory 实证，无任何对 migrations/platform*.db 的写路径调用）；批末指纹留档：platform-dryrun.db=4b2d3209…、platform-fullbatch.db=eb81d37b…、platform-governance.db=afe41186…、platform-search.db=e56171dd…、platform.db=eb81d37b…（sha256sum 原样前 8 位）。本批未执行迁移、未触碰 RTIS 生产行为。

## 三、文件清单

新增：crates/runtime-gateway/tests/multi_runner_failover.rs、crates/tool-engine/tests/security_matrix.rs、crates/context-broker/tests/assemble_bench.rs、docs/phase10/PERF-BASELINE.md、docs/phase10/P10-3-TASK.md、docs/phase10/P10-3-BATCH-REPORT.md、docs/phase10/perf-baseline/log.jsonl、docs/phase10/perf-baseline/autoresearch-dashboard.html。
修改：crates/runtime-gateway/src/lib.rs（候选序 + 故障转移 + 批量恢复）、crates/tool-engine/src/lib.rs（runs_by_status）、.github/workflows/ci.yml（注释点名门禁）。
依赖：Cargo.toml/Cargo.lock/vendor 零改动（成熟库优先核对：本批零新增 vendor）。

## 四、已知取舍与 caveat

1. rustfmt 版本漂移（沿用 P10-1/P10-2 口径）：本批 5 个触碰/新增 Rust 文件按本机 rustfmt 归一实跑；rustfmt 的模块遍历顺带重排了同 lib 树下 9 个未触碰文件（import 顺序/分号级），已按「未触碰文件不重排」纪律 git checkout 还原为批次前状态，仅保留两个本批触碰的 lib.rs 归一。
2. 派发多候选探测在间隔守卫内可能沿用主节点缓存健康（600s 内二次派发不重探）：首版接受（心跳以最近一次落库为准），见任务书第八节风险 1。
3. 基准为进程内 debug 测量（Mock 数据面、内存探针、单机口径），绝对值不对外引用，不回归门只做同口径相对比较；真实 Runner 面（podman/VM 探测开销）与模型在环属 VM 长演练口径，本地执行不落本基线。
4. log.jsonl 中 dispatch_ms 的 secondary 中位数聚合口径由 autoresearch 计算（server.ts 对重复运行按指标名取中位数）；ledger 原文以入库副本为准。
5. 本批不 commit（工作流批次惯例由协调方收口）；批次 diff 面：3 个跟踪文件修改（两个 lib.rs + ci.yml 注释）+ 上述新增路径，git status 实证无越界文件（crates/database、plugin-*、audit-engine 等既有未提交改动属 P10-1/P10-2 批次工作树，本批未触碰）。
