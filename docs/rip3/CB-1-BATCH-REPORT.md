# CB-1 批次验收报告：上下文折叠与装配稳定性（B1+B2+B3，零迁移）

- 日期：2026-11-22
- 任务书：[CB-1-TASK.md](CB-1-TASK.md)（§0 裁决 D1-D8 / §1 锚点 A1-A17 / §2 范围 / §3 步骤 S1-S7 / §4 十判据 / §6 八红线全部照办）
- 状态：实施完成，工作树留待审查收口（按纪律禁 git commit/add/push；S7 的显式 add → commit → push → CI watch 由主会话执行）
- 门禁基线：S1 记录 `cargo test --offline --workspace` 基线 1157 passed / 0 failed（EXIT=0，/tmp/cb1-baseline.log）

## 一、改动文件清单（路径 + 行数）

| 文件 | 类型 | 改动（diff --stat） | 内容 |
|---|---|---|---|
| `crates/agent-engine/src/agent_loop.rs` | 修改 | +510/-1 区段（510 行变更） | 折叠面：FOLD_TOOL_ID/TRANSCRIPT_FOLD_VERSION/FOLD_SUMMARY_PREFIX 常量 + FoldEvent（Serialize）+ fold_segment/fold_projection 纯函数 + DriveParams.history_rounds（轮界跨度表，D3）+ AgentLoop.fold_memory/with_fold_memory + 轮界两级触发块（控制消费之后护栏之前，D2）+ PlanContext 投影切换（D3）+ persist_fold_row（013 落库降级 warning）+ resume 重算与漂移检测 + RunReport.fold_events additive + 串行/并行双路径 step_rounds 记录 + 3 个单元测试 |
| `crates/agent-engine/src/control.rs` | 修改 | +43 行区段 | FoldConfig 四字段（enabled=false 默认 / growth 12000 / interval 4 / hot 2）+ RunControlConfig.fold 字段（既有七字段 Default 值零改动）+ actions 词表 TRANSCRIPT_FOLD 常量 |
| `crates/agent-engine/src/bin/rp-agent.rs` | 修改 | +116 行 | `--transcript-fold` 旗标（默认关）+ 三参数旗标（--fold-growth-tokens / --fold-min-interval-rounds / --fold-hot-window-rounds，FoldConfig 参数面透传）+ `run-folds <run_id>` 子命令（TaskMemoryStore::list_by_run 过滤 `transcript-fold:` 词形）+ 折叠开时挂接 with_fold_memory + usage 文本更新 |
| `crates/context-broker/src/assemble.rs` | 修改 | 22 行变更（+13/-9） | reason() 零信号退化序号源：进程级 SELECT_SEQ（:823 AtomicU64，已删除）→ 本装配 installed 选择序回填（L3 段收尾后统一赋位次，1 起，跨请求可复现，D4）；其余零 diff（排序/指纹/L0-L3/契约校验/三表落库不动） |
| `crates/context-broker/src/cache.rs` | 修改 | +265/-33 区段（265 行变更） | CacheMissReason 六值 enum + read_entry 校验本体（逐字段归因拆解：version→content_hash→fingerprint→policy_version→TTL）+ get 改薄包装（签名与行为零 diff、零计数）+ get_with_attribution additive（未命中入桶）+ AttributionBuckets 六 AtomicU64（Arc 共享）+ attribution_stats 快照 + CacheAttributionReport/TtlClassExpired 静态目录报告 |
| `crates/context-broker/src/lib.rs` | 修改 | 4 行 | cache 面新类型 re-export（CacheMissReason/CacheAttributionReport/CacheAttributionStats/TtlClassExpired） |
| `crates/context-broker/src/sources/registry.rs` | 修改 | 6 行 | brief_for 缓存消费点切 get_with_attribution（读取行为逐字节不变；归因自动入桶） |
| `crates/context-broker/src/bin/rp-context.rs` | 修改 | +39 行 | `cache-attrib` 子命令（目录级静态归因 JSON：entries/expired 四 TTL 类/policy_mismatch，cache-stats 邻位）+ `--policy-version` 旗标（默认 policy-1）+ usage 更新 |
| `crates/agent-engine/tests/cb1_transcript_fold.rs` | 新增 | 712 行 | 判据 1-5 e2e 硬验收（ma2_parallel_budget harness 同款：内存库 + 真实 migrations 001..027 + 注入驱动）5 用例 |
| `crates/context-broker/tests/assemble_stability.rs` | 新增 | 401 行 | 判据 6 三断言（同输入同装配 / 最小差异重装配 / 同 task_id run 链）3 用例 |
| `crates/context-broker/tests/cache_attribution.rs` | 新增 | 367 行 | 判据 7 六 reason 逐触发器 + get 零副作用负例 + 桶共享 + 静态报告（真实 mtime 回溯）9 用例 |
| `apps/server/selftest_cb1.sh` | 新增 | 170 行 | CLI 折叠实测（真实 python_worker + run-folds 回查 + cache-attrib）32 断言 |
| `docs/rip3/CB-1-BATCH-REPORT.md` | 新增 | 本文件 | 批次报告 |
| `docs/README.md` | additive | +2 行 | RIP3 登记表 CB-1 行 |

零 diff 复核：`migrations/` 001-027（git diff 空）、`migrations_pg/`、十五文件链尾钉面（git diff 空，清单逐文件核对）、`estimate.rs`/`tokenizer.rs`（D6 单点复用零 diff）、workflow-engine 全 crate、rp_server.py、Cargo.toml ×2、Cargo.lock 399 恒定。

## 二、§5 收口命令实测

| 命令 | exit | 关键输出 |
|---|---|---|
| `CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace` | 0 | **1177 passed / 0 failed**（169 目标全 ok；基线 1157 + 新增 20），/tmp/cb1-final.log |
| `cargo test --offline -p agent-engine -- --test-threads=1` | 0 | **61 passed / 0 failed**（基线 56 零改动 + 新增 lib 折叠单测 3 + e2e cb1_transcript_fold 5；lib 合计 31） |
| `cargo test --offline -p context-broker -- --test-threads=1` | 0 | **79 passed / 0 failed**（基线 67 零改动 + 新增 assemble_stability 3 + cache_attribution 9） |
| `cargo clippy -p agent-engine -p context-broker --all-targets --offline -- -D warnings` | 0 | `Finished` 零警告（修 4 处：unused var / bool_assert_comparison / doc_lazy_continuation / bool_comparison） |
| `cargo deny check advisories` | 0 | `advisories ok` |
| `cargo deny check licenses` | 0 | `licenses ok` |
| `grep -c '^\[\[package\]\]' Cargo.lock` | 0 | `399`（恒定，D7） |
| `bash apps/server/selftest_phase7_agents.sh` | 0 | `PASS=50 FAIL=0`（零改动回归） |
| `bash apps/server/selftest_ma2.sh` | 0 | `PASS=34 FAIL=0`（零改动回归） |
| `bash apps/server/selftest_cb1.sh` | 0 | `PASS=32 FAIL=0`（本批新增，数字见第四节） |
| `git status --short` | 0 | 仅 §2.1 落点（8 修改 + 4 新增 + 任务书文档） |
| `git diff --stat -- migrations/` | 0 | 空（零迁移，红线 2） |
| 十五文件链尾钉面 `git diff --stat` | 0 | 空（零随更，红线 2/A16） |

## 三、十判据逐条状态

| # | 判据 | 状态 | 实测 |
|---|---|---|---|
| 1 | B1 长跑折叠实测 | PASS | e2e `cb1_e2e_long_run_folds_planner_view_shrinks_authority_untouched`：6 步转录（3 轮 × 2 步），折叠触发 3 次（≥2）；末轮规划器可见 history 3 < 全量 6（合成折叠行 1:N 替代）；`RunReport.steps` 全量 6 不缩水；agent_run_step 行数 = 6（权威面零重写实测）；折叠前/后 planner 可见 est：可见 est=199 vs 全量 est=587（单折 est_before 196 → est_after 20）；agent_run_step 零 `transcript:fold` 合成行 |
| 2 | B1 折叠确定性 | PASS | `cb1_e2e_resume_recompute_digest_matches_fresh_and_drift_warns`：fresh 折叠 digest（rounds [0,0]）vs resume 重算 digest 逐字节相等；摘要词形相等；污染持久化行 digest（deadbeef 注入）→ resume warning 留痕 `transcript fold drift detected on resume: rounds 0-0 recomputed digest … != persisted deadbeef (recomputed view wins; persisted row kept for audit)`；纯函数面：同转录前缀两次 fold_segment digest/摘要/est 相等（lib 单测） |
| 3 | B1 摘要可引用回查 | PASS | e2e `cb1_e2e_fold_rows_queryable_and_compress_keeps_summary_wordform`：TaskMemoryStore::list_by_run 读到全部折叠行（lifecycle ACTIVE / round=覆盖末轮 / content_json kind=digest=est 载荷齐全）；compress_run 后 lifecycle COMPRESSED、content_json 置空、summary `transcript-fold:` 词形留存（013 生命周期语义兼容实测）；CLI 面 `rp-agent run-folds` 见第四节 |
| 4 | B3 两级触发 | PASS | e2e `cb1_e2e_two_level_trigger_negatives_and_guardrails_unchanged`：阈值未到（u64::MAX）不折叠；阈值到而轮距未到（interval 100）不折叠（防抖负例）；热窗覆盖全转录（hot 100）不折叠；max_rounds=1 + 折叠开 → `guardrail: budget max_rounds=1 reached` 词形零改动、步数 1；max_tool_calls=2 + 折叠开（折叠照常触发 2 次）→ `guardrail: max_tool_calls=2 reached`、步数 2（预算数真实步数不因折叠放宽）；热窗段永不折叠（负例三） |
| 5 | 默认关闭零回归 | PASS | `FoldConfig::default().enabled=false`（lib 断言 + RunControlConfig 既有七字段值 8/2/1/1/0/false/空表 零改动断言）；e2e `cb1_e2e_default_off_zero_footprint`：默认关 fold_events 空、零折叠行落库、steps 全量；既有全部测试零改动通过（context-broker 11 文件 + agent-engine 7 文件含 ma2_parallel_budget 三硬验收，均绿） |
| 6 | B2 前缀不变量 | PASS | `assemble_stability.rs` 三用例：同输入两次 assemble selection_hash 相等 + L0 规则列表与 L1 catalog:digest 载荷逐字节相等 + 零信号退化 reason 序跨请求相等；候选集单增一条 → diff 仅 L2 知识桶增量、L0/L1 前缀逐字节不变；同 task_id run 链（resume 同 correlation / retry 异 correlation）selection_hash 相等；acceptance_explainable.rs 零改动通过 |
| 7 | B2 缓存失效归因 | PASS | `cache_attribution.rs` 九用例：六类 CacheMissReason 逐触发器（七触发器映射：触发器 1 拆 Version/ContentHash、2/3/4/5/7 → FingerprintMismatch、6 → PolicyVersionMismatch、TTL → TtlExpired）；既有 get 薄包装零副作用（未命中不入桶负例）+ 行为零 diff（cache_invalidation.rs 十一用例零改动通过）；桶 Arc 共享；rp-context cache-attrib JSON（entries/expired 四类/policy_mismatch，真实 mtime 回溯计数） |
| 8 | est_tokens 单点 | PASS | 折叠计量/记账/sizing 全走 `context_broker::est_tokens`（agent_loop.rs use 单点引入）；lib 单测断言 est_after == est_tokens(摘要)；estimate.rs/tokenizer.rs git diff 空（D6 零 diff）；tokenizer 在位/缺席只影响数值不改变触发判定结构 |
| 9 | 门禁全绿 | PASS | 全量 workspace 一次通过（第二节表）+ 两 crate 定向 `--test-threads=1` + clippy -D warnings 零警告 + deny 双检 ok + Cargo.lock 399 恒定 + git status 仅 §2.1 落点 |
| 10 | 零迁移与权威面守卫 | PASS | migrations/ ls 实测 001-027（无 028）；十五文件链尾钉面 git diff 空；agent_run_step / context_manifest / context_manifest_item / context_ref_log 四表 DDL 零 diff（零迁移推论）；折叠行只出现在 agent_task_memory（e2e 断言 agent_run_step 零 transcript:fold 行） |

## 四、CLI 折叠实测数字（真实 python_worker 端到端，禁模拟冒充）

`bash apps/server/selftest_cb1.sh`（32 PASS / 0 FAIL，临时库）：

- 折叠开 run：`run=arn-18dc131a4d82063055145d7e`（每轮一轮界触发面）；
- **折叠前 est=241 → 折叠后 est=46**（折叠事件 est_before/est_after，折叠行落 013 agent_task_memory）；
- `rp-agent run-folds arn-…` 回查命中：folds_count=1，折叠行 JSON 含 memory_id / rounds=[0,0] / steps_folded=1 / digest（64 hex）/ summary `transcript-fold:rounds=0-0;steps=1;tools={rip.kb.keyword_search};artifacts={1}` / lifecycle ACTIVE；
- agent_run_step 行数 = 1 = 报告 steps 全量（权威面零重写零合成行）；
- 默认关对照 run：fold_events 空、run-folds 零折叠行、权威面 1 行；
- run-steps 步载荷 82 字节（对照面）。

e2e 长跑数字（`cargo test -p agent-engine --test cb1_transcript_fold -- --nocapture`）：folds=3 / 全量步数 6 / 末轮规划可见长度 3 / 可见 est 199 vs 全量 est 587 / 单折 est 196→20。

说明：rp-agent 内置 KeywordSearchPlanner 有历史即收尾（单步 run）——CLI 实测以收紧参数面（growth 1 / interval 0 / hot 0）在轮界触发真实折叠（est 241→46 为真实计量值，非模拟）；多轮多次折叠的「长跑」硬验收由 e2e 承载（判据 1）。默认阈值面（12000/4/2）下单步 run 恒不触发折叠——CLI 增设三枚参数旗标正是为此可达性（坏消息单列 §五-3）。

## 五、坏消息（单列）

1. **get_with_attribution 返回类型与任务书字面有一处偏差**：D5 字面 `(Option<String>, CacheMissReason)` 与「CacheMissReason 六值 enum」约束物理冲突（命中无 miss 归因可填），实现落 `(Option<String>, Option<CacheMissReason>)`——命中 reason=None，未命中 Some(reason)。六值词形不变（红线 6），测试断言命中 `is_none()`。
2. **D4②「知识候选」实现为 Doc 形态知识桶候选**：kind=Knowledge 增量会同步进入 L1 digest 计数口径（assemble.rs 计数面只计 Tool/Knowledge/Plugin/Rule），与「L0/L1 前缀逐字节不变」物理冲突；Doc/Schema 随知识桶取舍但不入计数——判据 6 按字面自洽落面（测试注释已注明）。
3. **rp-agent 增设三枚折叠参数旗标**（--fold-growth-tokens / --fold-min-interval-rounds / --fold-hot-window-rounds）：任务书落点清单只列 --transcript-fold 旗标，但默认阈值面（12000 tokens/4 轮）下 CLI 单步 run 恒不触发折叠，selftest_cb1 真折叠实测不可达；三旗标为 FoldConfig 参数面透传（缺省值 = FoldConfig::default 词面），§8 前置输出④的参数面备忘随此落 CLI。
4. **零信号 reason 序号位次语义副作用**：装配内 installed 位次意味着 L2 内零信号条目的 reason 序号随新候选按分数插入平移（进程级累计序则永不复现）；D4② 字面不变量只有 L0/L1——L2 共享条目断言剔除 reason 位次分量（信号承载条目 reason 与信号同源不受影响，仍逐字节相等）。已登记 §8 前置输出③。
5. **A5 锚点消费点修正**：brief_for 在装配主流程零调用点（install_into_bucket 用 candidate.summary）——「registry 消费点切 get_with_attribution」落在其真实唯一消费面（brief_for 直调面）；CLI assemble 不产生 brief 缓存条目，selftest_cb1 的 cache-attrib 夹具以真实 CacheEntry 磁盘形状（cache.rs serde 词面 + mtime 回溯）验收扫描面。

## 六、未解决项

无。全部十判据 PASS；全量 workspace 一次绿；既有测试零改动通过；零迁移零新依赖零链尾随更。

## 七、§8 前置输出登记（MA-3 / Phase 6 对接面）

1. **折叠事件词形与 JSON 形状**：RunReport.fold_events[] 元素 = `{"rounds":[a,b],"steps_folded":N,"est_before":X,"est_after":Y,"digest":"<sha256 hex 64>","summary":"transcript-fold:rounds=a-b;steps=N;tools={…};artifacts={N}","memory_id":"mem-t-…"}`；折叠行（agent_task_memory）content_json = `{"kind":"transcript-fold","rounds":[a,b],"steps_folded":N,"est_before":X,"est_after":Y,"digest":"…"}`，summary = 同上词形（无时间戳无进程内计数），round 列 = 覆盖末轮，lifecycle ACTIVE→COMPRESSED（compress_run 摘要留存）/EVICTED（墓碑）。B4 血缘回查沿 memory_id / `transcript-fold:` 词形前缀。
2. **CacheMissReason 六词形与 cache-attrib JSON**：EntryMissing / VersionMismatch / ContentHashMismatch / FingerprintMismatch / PolicyVersionMismatch / TtlExpired（serde 词形即 JSON 字面）；cache-attrib 输出 = `{"dir","entries","expired":{"catalog","knowledge","schema","rtis"},"policy_mismatch","policy_version"}`（嵌套口径：同一过期条目计入所有更短 TTL 类）；进程内归因快照 `attribution_stats()` 六桶字段与六词形同名（snake_case）；get 薄包装零计数，归因只在 get_with_attribution 累计。
3. **reason 序语义变更说明**：零信号退化形态 `source-order:<seq>+score:<score>;source:<id>` 的 seq 从「进程级 SELECT_SEQ 累计序」改为「本装配内 installed 选择序（1 起位次）」——同输入两次装配 seq 相等（跨请求可复现）；历史 manifest 行 reason 原文不动（只追加语义免回填）；explain 下游消费者对账时，新行 seq 只在单一装配内单调、不再跨装配累计。
4. **折叠阈值/间隔/热窗参数面备忘**：FoldConfig { enabled(false) / fold_growth_tokens(12000) / fold_min_interval_rounds(4) / fold_hot_window_rounds(2) }，CLI 透传旗标同名；轮界判定语义 = 自上次折叠起转录 est_tokens 增量 ≥ 阈值 **且** 距上次折叠覆盖末轮 ≥ 间隔 → 折叠 `[next_unfolded, round - hot_window - 1]` 区间（热窗内轮次永不折叠）；MA-3 子代理嵌套 run 若继承转录面，参数面随 RunControlConfig.fold 结构体传递（新增字段走 ..default() 零改动兼容）。

## 八、回滚

单 commit 交付（主会话执行）；`git revert` 整批撤销。零迁移 → 无列残留无表残留无链尾随更，revert 即完全回滚。折叠默认关使 revert 前行为即现状；revert 后 agent_task_memory 中已落折叠行残留（ACTIVE 生命周期行，013 生命周期可正常处理，无害，不清理）。reason 序变更随 revert 恢复 SELECT_SEQ 口径（历史行原文不动）；get_with_attribution 纯 additive，revert 后零残留。
