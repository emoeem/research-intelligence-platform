# 批次任务书：P10-3（Phase 10 第 3/4 批，运行与质量门：双 Runner 调度与故障转移、T0/T1/T2 越界拒绝矩阵与 secrets 不泄露核查进 CI、autoresearch 性能基准计量闭环）

> 状态：已实施；日期：2026-11-14；适用版本：Phase 10 批次工作树（未 commit，由协调方收口）。
> 相关：docs/phase0/PHASE-ROADMAP.md §11.3（P10-3/P10-4 判据原文）、docs/phase1/T1-T2-BOUNDARY-RULING.md（T1/T2 分界裁决）、docs/phase0/TOOL-CONTRACT-DRAFT.md（D-TC-05/D-TC-06）、docs/phase10/PERF-BASELINE.md、docs/phase10/P10-3-BATCH-REPORT.md。

## 一、批次目标与范围

本批覆盖 ROADMAP §11.3 的 P10-3 与 P10-4 两行判据（P10-4 与 P10-1 到 P10-3 并行编写，本批一并落自动化面）：

1. P10-3（多 Runner/多 VM 扩展）：RuntimeNode 多节点调度与故障转移，判据「双 Runner（podman_vm + podman_host 或双 VM）下调度与心跳正确；单节点离线不丢任务」。现状缺口：`Gateway::dispatch` 只投 primary 节点（crates/runtime-gateway/src/lib.rs:196 批前状态），主节点离线即落 WAITING_FOR_RUNNER 挂死在主节点，无转投备用面。
2. P10-4（性能、稳定性、恢复与安全测试）：本批取「T0/T1/T2 越界拒绝矩阵、secrets 不泄露核查」两类的自动化测试进 CI（模型在环与 VM 长演练除外）；另落 autoresearch 性能基准计量闭环（init_experiment/run_experiment/log_experiment 产出 .auto/log.jsonl 与 dashboard），基线对照 Phase 4 装配耗时与 Phase 2 派发延迟既有记录以不回归为门（见 docs/phase10/PERF-BASELINE.md）。
3. 范围外（不动）：remote_runner 开放与否维持 P2 评审（ROADMAP P10-3 原文口径，TOOL-CONTRACT D-TC-01 词表不动）；P10-4 其余三类（并发上限压测、磁盘自愈、VM 故障矩阵）属 VM 长演练口径，本地执行不在本批（recovery_drill.rs 已有磁盘自愈与故障演练载具，本批不重复）；P10-5 备份恢复、P10-6 发布文档不属本批。

## 二、设计：多候选派发与故障转移（runtime-gateway::Gateway）

1. 候选优先序单一事实来源：Gateway 新增 `registration_order: Mutex<Vec<String>>`（lib.rs），登记序即候选序，`register_node` 追加、`set_primary` 重排（把目标节点移到队首并保留其余登记序），`candidate_nodes()` 返回副本供派发与恢复共用。主节点字段（primary_node）语义不变：仅是优先序的队首 + 无候选时的报错来源。
2. 派发语义（`Gateway::dispatch`，P10-3 本体）：引擎受理（D-TC-05）后按候选序逐节点探测（每节点遵守探测间隔，不 tight-poll），首个非 broken 节点承接（`DispatchOutcome::Queued`，node_id 即承接节点），主节点离线时任务转投备用节点（QUEUED，不丢）；全部候选离线才走第 3 行跃迁落 WAITING_FOR_RUNNER（挂主节点，计划 §5.6:336 原文语义：探测失败不产生 FAILED）。单节点登记时行为与 P2-4b 批次完全一致（不推翻已验收能力）。
3. 探测纪律沿用 Phase 2 既有机制：派发路径用 `probe_node`（间隔守卫内沿用落库健康值，不重跑探针命令，heartbeat.rs `due()` 判定）；恢复路径用 `probe_node_forced`（恢复判定必须见真实心跳，P2-4b 原口径）。落库面（runtime_node.health/last_heartbeat_at/version）零改动。

## 三、设计：恢复续跑（任一候选恢复即回 PREPARING）

1. `probe_and_resume(run_id)` 从「只重探主节点」改为按候选序强制重探：任一候选恢复即走第 4 行跃迁（WAITING_FOR_RUNNER 到 PREPARING）；全部候选仍离线报 `GatewayError::RunnerUnavailable`（报主节点健康，单节点场景与既有测试语义一致）。
2. 批量恢复 `recover_waiting_runs(limit)`（P10-3「恢复续跑」收口面）：每候选节点强制探测一次（结果供本批全部待恢复 run 复用，不按 run 数重探，不 tight-poll），任一候选可用即把 WAITING_FOR_RUNNER 的 run（created_at 序，至多 limit 条）逐条恢复回 PREPARING；全部候选离线报 RunnerUnavailable 且不触碰任何 run。返回 `RecoveryReport { healthy_nodes, resumed_run_ids, skipped_run_ids }`：skipped 记录扫描与恢复之间状态已漂移的 run（并发推进不误伤）。
3. 枚举面新增 `Engine::runs_by_status(status, limit)`（tool-engine lib.rs）：按状态列 run_id 的只读投影（created_at 升序），网关批量恢复的唯一新增引擎面；状态机跃迁面（mark_waiting_for_runner/resume_to_preparing/drive_resumed）零改动。

## 四、设计：T0/T1/T2 越界拒绝矩阵与 secrets 不泄露核查（tool-engine tests/security_matrix.rs）

1. 口径来源（不新造规则，全部引用既有裁决）：T0/T1/T2 分界以外部目标写副作用为准（docs/phase1/T1-T2-BOUNDARY-RULING.md §1 表：T0 无外部副作用；T1 对外只读或平台内部读写；T2 改动外部目标，派发必须携带 authorization_ref）；POLICY_DENIED 与零落行出自 TOOL-CONTRACT D-TC-05（tool-engine submit 路径既有强制面，lib.rs T2 闸）；D-TC-06 出自 TOOL-CONTRACT-DRAFT.md:43（secrets 不进调用协议、不进 Manifest 值、不进重放记录）。
2. 矩阵五行（每行一个用例，见第五节文件清单）：登记面域外值拒绝（validate.rs 第 8 条）；派发面 T2 缺授权引用 POLICY_DENIED 且零落行；派发面 T2 带授权引用受理且 permission_ctx 记录；派发面 T0/T1 无闸照常受理（裁决第 1 节「写平台内部库不升 T2」）；调用协议面 secrets 键被结构检查拒绝（request.rs 禁用字段，D-TC-06 原文引用）。
3. secrets 核查三面：Manifest 面（值形键拒绝、键名形态 `^[A-Z][A-Z0-9_]+$` 违规拒绝、required secret 缺平台 secrets 存储侧信拒绝，validate.rs 第 7 条既有强制面）；泄露面（planted 假想 secret 值在 tool/tool_version/tool_run 全行全列与终态信封逐字节扫描零命中；侧信接口只传键名布尔，值从结构上进不了平台）；存储面澄清（tool-registry store.rs:40 provenance_json 只取 manifest.provenance 字段，不逐字持久化 Manifest，登记存储无 secrets 声明面）。
4. 进 CI 方式：workspace 全量测试门（ci.yml rust job `cargo test --offline --workspace`）天然覆盖两个 crates 的新测试文件；ci.yml 头注释显式点名两载具与「性能基准 #[ignore] 保持本地执行」口径，门禁行为零改动（不推翻既有 CI）。

## 五、设计：autoresearch 性能基准计量闭环（P10-3/P10-4 覆盖 ROADMAP P10-4）

1. 载具：两个 `#[ignore]` 基准测试（不进常规测试流与 CI，P10-4「模型在环与 VM 长演练除外」同口径本地执行）：`crates/context-broker/tests/assemble_bench.rs`（Phase 4 装配耗时，n=20 冷装配中位数，Mock 源 8 条知识项 + rip-rules rulepack，与 P4-4 契约测试同构）与 `crates/runtime-gateway/tests/multi_runner_failover.rs::bench_dispatch_latency`（Phase 2 派发延迟，n=30 中位数，双 Runner 健康态全链 dispatch）。
2. 冻结基准：`.auto/measure.sh`（autoresearch 载具，两条 cargo test --offline 命令原文，METRIC 行解析），会话工作目录 /home/emo/code/pensting/.auto/；RIP 仓库根执行 cargo 门禁（vendor 按 CWD 发现）。
3. 闭环：init_experiment（assemble_ms，lower）→ run_experiment(repeat=3) → log_experiment(status=noop，记账不 commit 不 revert，批次纪律：本批不 commit）→ export_dashboard。基线数值、口径、不回归门（劣化超 20% 触发排查）落 docs/phase10/PERF-BASELINE.md；ledger 副本与 dashboard 副本入 docs/phase10/perf-baseline/。
4. 对照既有记录：实测核对 Phase 4/Phase 2 验收文档无绝对耗时数字记录（grep「耗时|elapsed|latency|延迟」零命中数值），ROADMAP §11.5 风险 4「无性能数据积累」在批内成立，故本批落首版基线并定义不回归门，不设绝对指标（ROADMAP 原文口径）。

## 六、实施步骤与文件清单

| 文件 | 动作 | 内容 |
| --- | --- | --- |
| crates/runtime-gateway/src/lib.rs | 改 | 候选序（registration_order/candidate_nodes）、dispatch 故障转移、probe_and_resume 多候选、recover_waiting_runs + RecoveryReport、模块头 P10-3 说明 |
| crates/tool-engine/src/lib.rs | 改 | `Engine::runs_by_status`（按状态列 run_id，只读投影） |
| crates/runtime-gateway/tests/multi_runner_failover.rs | 新 | 4 个判据用例 + bench_dispatch_latency（#[ignore]） |
| crates/tool-engine/tests/security_matrix.rs | 新 | 7 个矩阵/泄露核查用例 |
| crates/context-broker/tests/assemble_bench.rs | 新 | bench_assemble_latency（#[ignore]） |
| .github/workflows/ci.yml | 注释 | 显式点名 P10-3/P10-4 门随 workspace 测试生效与性能基准本地口径 |
| docs/phase10/PERF-BASELINE.md | 新 | 首版基线数值、口径、不回归门、已知限制 |
| docs/phase10/perf-baseline/log.jsonl、autoresearch-dashboard.html | 新 | autoresearch ledger 与 dashboard 入库证据副本 |
| docs/phase10/P10-3-TASK.md、P10-3-BATCH-REPORT.md | 新 | 本任务书与批次报告 |

成熟库优先核对：本批零新增依赖（Cargo.toml 零改动、vendor 零增量），故障转移是业务编排逻辑（无成熟库可替，手写并在 ADR 复审范围内小面实现）；探测/落库/状态机/信封/校验/hash 全部复用既有栈（heartbeat、HealthStore、Engine 状态机、validate.rs、envelope::to_value）。

## 七、验收命令与隔离纪律

~~~bash
cd /home/emo/code/pensting/research-platform
cargo test --offline -p runtime-gateway -p tool-engine    # 106 passed 0 failed 1 ignored（见批次报告）
cargo clippy --offline --workspace --all-targets -- -D warnings    # 零告警 exit 0
# 性能基准（本地，不进 CI）：
cargo test --offline -q -p context-broker --test assemble_bench -- --ignored --nocapture bench_assemble_latency
cargo test --offline -q -p runtime-gateway --test multi_runner_failover -- --ignored --nocapture bench_dispatch_latency
~~~

隔离纪律：全部测试与基准走内存库（DbPath::Memory）与全链迁移，生产 migrations/platform*.db 零写（批次报告含指纹留档）；本批不 commit；文档无 em/en dash（实测 grep）。

## 八、风险、已知限制与回滚

1. 派发多候选探测的次序性：派发路径在间隔守卫内可能拿到主节点的缓存健康（600s 内二次派发不重探），首版接受该行为（生产探测周期 600s，主节点健康以最近一次心跳为准）；若需派发前强制重探，属策略调参（换 probe_node_forced），回滚即换回。
2. recover_waiting_runs 的 limit 截断：单批至多 limit 条恢复，剩余 run 留待下轮（不丢：仍 WAITING_FOR_RUNNER，状态机原态）；常驻循环调用面属 P2-4c 编排批次，本批只供显式调用。
3. 性能基准的噪声与口径：进程内 debug 测量、Mock 数据面、单机口径（详见 PERF-BASELINE.md 第六节），跨机器不可比；不回归门 20% 阈值为首版经验值，变更须重落基线。
4. 回滚方式：整批 revert 工作树新增/修改文件即可（无迁移、无 schema、无生产库触碰）；dispatch 单节点行为与 P2-4b 语义一致（既有 wait_runner.rs 4 用例全绿为证），revert 不影响已验收能力。
