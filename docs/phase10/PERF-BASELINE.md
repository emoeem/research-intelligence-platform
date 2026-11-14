# 性能基线：Phase 10 首版（P10-3/P10-4，autoresearch 计量闭环）

> 状态：生效中；日期：2026-11-14；适用版本：Phase 10 批次工作树（P10-3，未 commit，由协调方收口）。
> 相关：docs/phase0/PHASE-ROADMAP.md（§11.3 P10-4 判据、§11.5 风险 4）、docs/phase10/P10-3-TASK.md、docs/phase10/P10-3-BATCH-REPORT.md、docs/phase10/perf-baseline/log.jsonl（ledger 副本）。

## 一、为什么落这个文档

ROADMAP §11.5 风险 4 原文：性能测试的基线缺失，当前无性能数据积累，P10-4 以「不回归」为门（对照 Phase 4 装配耗时、Phase 2 派发延迟的既有记录），不设绝对指标。本批实测核对：docs/phase4 与 docs/phase2 全部验收文档中均无耗时/延迟的绝对数字记录（grep「耗时|elapsed|latency|延迟」零命中既有数值），所谓「既有记录」只有口径没有数字。因此本批以 autoresearch 计量闭环落首版基线：本文档的数值即后续批次的不回归对照基准。

## 二、计量口径（数字必须可复现）

| 项 | 值 |
| --- | --- |
| 指标 1（primary） | `assemble_ms`：Phase 4 上下文装配耗时，context-broker L0 到 L3 冷装配全路径（Mock 源 8 条知识项 + rip-rules rulepack，内存库全链迁移，每次迭代独立任务文本不命中同 content_hash 缓存） |
| 指标 2（secondary） | `dispatch_ms`：Phase 2 工具派发延迟，`Gateway::dispatch` 全链（engine.submit 落行 + 心跳探测 + QUEUED 判定，双 Runner 在位健康态） |
| 载具 | `crates/context-broker/tests/assemble_bench.rs`（`#[ignore]` bench_assemble_latency，n=20 取中位数）；`crates/runtime-gateway/tests/multi_runner_failover.rs`（`#[ignore]` bench_dispatch_latency，n=30 取中位数） |
| 冻结基准 | `.auto/measure.sh`（autoresearch 载具，2 条 cargo test --offline 命令原文，METRIC 行解析）；会话工作目录 `.auto/`（/home/emo/code/pensting） |
| 计量方式 | autoresearch `run_experiment(repeat=3)` 取中位数（median_metric）；ledger 落 `.auto/log.jsonl` |
| 执行面 | 本地执行不进 CI（P10-4 口径：模型在环与 VM 长演练除外；性能基准同为本地，`#[ignore]` 不入常规测试流，ci.yml 注释已写明） |
| 机器上下文 | AMD Ryzen 7 7735H（16 线程），Linux 7.3.4-1-cachyos-bore-lto，cargo 离线 vendored 构建，debug profile |

复跑命令（与 measure.sh 同口径，任一终端可执行）：

~~~bash
cd /home/emo/code/pensting/research-platform
cargo test --offline -q -p context-broker --test assemble_bench -- --ignored --nocapture bench_assemble_latency
cargo test --offline -q -p runtime-gateway --test multi_runner_failover -- --ignored --nocapture bench_dispatch_latency
~~~

## 三、首版基线数值（2026-11-14 实测）

| 指标 | run 1 | run 2 | run 3 | 中位数（基线值） | 口径 |
| --- | --- | --- | --- | --- | --- |
| assemble_ms | 2.399 | 2.463 | 2.148 | **2.399** | n=20 冷装配中位数，单次调用 |
| dispatch_ms | 0.474 | 0.454 | 0.475 | **0.474** | n=30 派发中位数，单次调用 |

ledger 原文（run 行全文，`.auto/log.jsonl`，副本 docs/phase10/perf-baseline/log.jsonl）：

~~~json
{"type":"run","run":1,"segment":1,"status":"noop","metric":2.399,"metrics":{"dispatch_ms":0.474},"asi":{"hypothesis":"基线记录而非优化：autoresearch 计量闭环（init/run×3/log noop）落 .auto/log.jsonl，被测面为 P10-3 批次工作树（#[ignore] bench_assemble_latency 与 bench_dispatch_latency）","next_action_hint":"后续批次跑同一 measure.sh 对照本基线，任一指标中位数劣化超 20% 即触发排查（不回归门）","rollback":"无代码改动，noop 不 commit 不 revert"},"description":"P10-3/P10-4 首版性能基线（repeat=3 中位数）：Phase 4 装配 assemble_ms=2.399（runs 2.399/2.463/2.148），Phase 2 派发 dispatch_ms=0.474（runs 0.474/0.454/0.475）；基线口径与不回归门见 docs/phase10/PERF-BASELINE.md","commit":null,"timestamp":"2026-11-14T09:10:11.347Z"}
~~~

dashboard 静态副本：docs/phase10/perf-baseline/autoresearch-dashboard.html，live 面由 `export_dashboard` 随时重建。

## 四、不回归门（后续批次执行）

1. 同口径复跑：跑本文档第二节两条命令（或 autoresearch measure.sh，repeat=3 取中位数）。
2. 判定：任一指标中位数劣化超过基线 20%（assemble_ms > 2.879 或 dispatch_ms > 0.569）即视为回归，触发排查（先核对机器负载与噪声，repeat=5 复测确认后查改动面）；20% 以内视为进程内测量噪声放行。
3. 可比性边界：基线数值只在同机器（Ryzen 7 7735H）、同 profile（debug）、同载具口径下可比；跨机器或载具变更（`benchmark_drift`）时必须先重落基线（init_experiment 新 segment + 更新本文档第三节），不做跨口径对比。
4. 门只对照「不回归」，不设绝对指标（ROADMAP §11.5 风险 4 原文口径）。

## 五、闭环产物清单

| 产物 | 路径 | 说明 |
| --- | --- | --- |
| ledger | `.auto/log.jsonl`（会话工作目录） | config 行 + run 行，autoresearch 单一事实源 |
| ledger 副本 | docs/phase10/perf-baseline/log.jsonl | 入库证据（本批复制） |
| dashboard | `autoresearch-dashboard.html`（会话工作目录） | export_dashboard 静态副本 |
| dashboard 副本 | docs/phase10/perf-baseline/autoresearch-dashboard.html | 入库证据（本批复制） |
| 基准载具 | crates/context-broker/tests/assemble_bench.rs、crates/runtime-gateway/tests/multi_runner_failover.rs（bench_dispatch_latency） | `#[ignore]`，本地执行 |
| 冻结基准 | `.auto/measure.sh` | 2 条 cargo test 原文，批次内不修改 |

## 六、已知限制（坏消息必写）

1. 进程内 debug profile 测量：数值含 debug 未优化开销（无 --release），绝对值偏大；不回归门只做同口径相对比较，受此影响有限，但不可对外当绝对性能数字引用。
2. 噪声来源：CI/后台任务、CPU 频率波动；repeat=3 中位数可吸收单次抖动，第二节命令可随时复跑核验。
3. 装配基准的数据面是 Mock 源 + rip-rules（与 P4-4 契约测试同构），不是真实平台库数据面；口径变更须重落基线（第四节第 3 条）。
4. dispatch_ms 的双 Runner 是内存探针（SwitchProbe），不含真实 podman/VM 探测开销；真实 Runner 面的延迟属 VM 长演练口径，按 P10-4 保持本地执行、不落本基线。
5. 模型在环（Agent 节点）与 VM 长演练不在本基线面内（P10-4 口径排除项，本地执行）。
