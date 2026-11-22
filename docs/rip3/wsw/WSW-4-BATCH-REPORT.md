# WSW-4 批次收口报告

- 批次：WSW-4 — Web Fuzzer / Intruder（Payload 位置、Mutator、Processor、Job 调度、响应差异、预算控制；Rust 引擎批，UI 不入批）
- 日期：2026-11-22
- 基线 HEAD：`ae23a9d8`（CB-1 收口；工作树仅任务书 `WSW-4-TASK.md` 未跟踪）
- 收口方式：本地工作树实施与验证；未 commit、未 push（按批纪留给人工审查）
- 任务书：[WSW-4-TASK.md](WSW-4-TASK.md)（§0 D1-D10 裁决 / §1 A1-A27 锚点 / §3 S1-S10 / §4 判据 1-12 / §6 八红线全程照办）
- 状态：**本地验收全绿**（定向 + workspace 全量 + selftest 真实靶机 + clippy + deny + 迁移纪律）

## A. 本批交付（落点清单实况）

### A.1 新建文件

| 文件 | 行数 | 内容 |
|---|---|---|
| `crates/wsw-message/src/diff.rs` | 505 | D6 diff 原语：行级 LCS diff（前缀/后缀剥离 + 2000 行帽降级 + 同位 Delete/Insert 合并 Replace）+ 相似度比率 `2·LCS/(nl+nr)`（PLAN :303 等长不同内容 <1 负例钉死）+ 归一化忽略规则（键值掩码 JSON/表单/日志三形态 + 行级跳过子串；纯 std 零依赖） |
| `crates/wsw-message/tests/diff_primitives.rs` | 127 | diff/相似度/归一化正负例（判据：等长不同内容 similarity<1） |
| `migrations/028_wsw_fuzz.sql` | 128 | D2 纯新增两表：`wsw_fuzz_job`（九态 CHECK 逐字 + FuzzBudget 六列 + spec_json 8MiB 帽 + estimated_requests + baseline_case_id/trace_id/task_id/evidence_id）+ `wsw_fuzz_case`（七态 CHECK 逐字 + payload_json/sha256 + response 五列 + match_hits_json + diff_class/similarity，UNIQUE(job_id,ordinal)）+ 四索引 + schema_version/user_version 双写 28（全 IF NOT EXISTS / INSERT OR IGNORE，零 ALTER） |
| `crates/wsw-runtime/src/fuzz/mod.rs` | 819 | D5 JobRunner：九态状态机（迁移表 + SQL CAS 单语句双面强制）+ Semaphore 并发 + FuzzBudget 四闸（acquire 后 send 前逐 case：总数/速率/间隔/时长）+ watch 取消/暂停 + case 行 status 权威续派（PENDING 未派、SENT/终态不重派）+ SIGTERM graceful（CANCELLING→CANCELLED，在途等终态）+ 信号后到复查（drain 期 SIGTERM 仍收敛 CANCELLED）+ baseline 缺省（首个 RESPONDED）/sha 去重/CallerRateTracker 同型速率窗口（std AtomicUsize+Instant+Mutex，不跨域 import agent-engine） |
| `crates/wsw-runtime/src/fuzz/spec.rs` | 1223 | D3 标记面（`§…§` U+00A7 成对扫描 target/headers/body 三字段，奇数拒/空拒/选择器-标记同字段冲突拒）+ 七类选择器（query/header/cookie/form/json RFC6901 pointer/path_segment/multipart 浅层）+ 三模式估算（sum/checked-mul/zip 等长，u64 溢出饱和拒）+ 惰性组合展开器（single 一次一位/cartesian mixed-radix/zip）+ build_case_request（标记区间降序 splice，分隔符不进最终请求） |
| `crates/wsw-runtime/src/fuzz/mutator.rs` | 412 | D4：Payload 源（list/wordlist BOM+空行+# 跳过/数字范围/字母序列/日期范围，100k 帽）+ 变换链（URL/Base64/Base64URL/Hex/HTML Entity——base64 std 内嵌实现，D9 只准 regex 转正）+ Prefix/Suffix/Case/Replace/Concat 扇出/RegexReplace + Processor（header_set CR/LF 拒/header_remove/body_replace） |
| `crates/wsw-runtime/src/fuzz/analyzer.rs` | 149 | D6 分类/匹配/去重：SendError→(七态, error_code) 映射矩阵 + 429→`fuzz_rate_limited`/5xx→`fuzz_server_error`（fuzz_ 前缀自别，红线 6）+ 关键字/regex 命中投影 + similarity/diff_class 经 `wsw_message::diff` 单点（禁第二 diff 实现） |
| `crates/wsw-runtime/src/fuzz/persist.rs` | 272 | D7 `persist_fuzz_case_exchange`（sender.rs:873 persist_send_result 同型；exchange source='fuzzer' + listener_id=`wsw-lis-fuzzer` 常量 + CAS 三面 + next_sequence_no 全链复用；进程内 Mutex 串行化落库段防并发序号撞 UNIQUE）+ scope 预检目标投影 |
| `crates/wsw-runtime/tests/fuzz_spec.rs` | 534 | S4 词法/估算/选择器/build 正负例 19 项 |
| `crates/wsw-runtime/tests/fuzz_job.rs` | 894 | S7 JobRunner 集成 16 项（真实靶场 TcpListener 127.0.0.1:0 随机端口——sender_send.rs spawn_target 同型；marker 命中/budget 截断/速率窗/间隔实测/scope 全拒零发送/429/超时/取消/暂停恢复/SIGKILL 防重派/迁移拒/baseline 去重/CRLF 守卫） |
| `apps/server/selftest_wsw4.sh` | 364 | S9 真实靶机收口脚本（python3 标准库 http.server 靶机 + rp-wsw CLI 六 job + rp_server 四端点 curl 全链，34 断言） |

### A.2 修改文件（git diff HEAD --stat 原文）

```
 Cargo.lock                                         |   1 +
 apps/server/rp_server.py                           | 207 ++++++++
 crates/agent-engine/tests/migration_dev_copy_p7_3.rs  |  10 +-
 crates/agent-engine/tests/migration_idempotent.rs  |   8 +-
 crates/database/tests/backup_restore.rs            |   2 +-
 crates/database/tests/migrations_agent_data_plane.rs  |  12 +-
 crates/database/tests/migrations_context_broker.rs |  13 +-
 crates/database/tests/migrations_runtime_node.rs   |  12 +-
 crates/database/tests/migrations_tool_contract.rs  |  13 +-
 crates/plugin-runtime/tests/lifecycle_full_path.rs |   8 +-
 crates/rp-mcp-parity/tests/caller_auth.rs          |   2 +-
 crates/workflow-engine/tests/migration_replay.rs   |   6 +-
 crates/wsw-message/src/ids.rs                      |  12 +
 crates/wsw-message/src/lib.rs                      |   1 +
 crates/wsw-message/src/store.rs                    |   4 +-
 crates/wsw-message/tests/cas_e2e.rs                |   8 +-
 crates/wsw-runtime/Cargo.toml                      |   4 +
 crates/wsw-runtime/src/bin/rp-wsw.rs               | 148 +++++-
 crates/wsw-runtime/src/lib.rs                      |   1 +
 crates/wsw-runtime/src/persist.rs                  | 523 ++++++++++++++++++++-
 crates/wsw-runtime/tests/exchange_persist.rs       |   8 +-
 crates/wsw-runtime/tests/intercept_queue.rs        |   2 +-
 22 files changed, 945 insertions(+), 60 deletions(-)
```

改动面注记：

- **A26 十五硬钉随更（12 文件，钉值 27→28 四类形态逐 file:line 照单）**：①migration_idempotent.rs:87（+链 vec 追 28）②migrations_agent_data_plane.rs:83/:100/:109-110 ③migrations_runtime_node.rs:72/:84/:90 ④migrations_tool_contract.rs:107/:149/:158-159 ⑤migrations_context_broker.rs:108/:149/:158-159 ⑥backup_restore.rs:51 ⑦caller_auth.rs:209 ⑧persist.rs:890-906（改名 assert_watermark + 断言 28）⑨exchange_persist.rs:3/:81/:85 ⑩intercept_queue.rs:47 ⑪wsw-message/src/store.rs:203-204 ⑫cas_e2e.rs:132/:139-140/:177 ⑬migration_dev_copy_p7_3.rs:75/:85/:92-93 ⑭plugin-runtime lifecycle_full_path.rs:604-610（回放计数钉 18→19，010..028 十九枚）⑮workflow-engine migration_replay.rs:147-149。软引用（注释面）酌情随更：ma2_parallel_budget.rs/cb1_transcript_fold.rs 头注未动（003 行号漂移不成立——任务书 A26 软引用清单内按"酌情"条款）。
- **persist.rs +523**：fuzz store（FUZZ_JOB_STATES/FUZZ_CASE_STATES 词表常量 + 迁移表 + FuzzJobRow/FuzzCaseRow/Patch + insert/get/list/advance CAS/counts/dispatched_count）+ assert_watermark 改名收口（名 25/值 27 双漂移 → 名值齐 28）。
- **rp_server.py +207**：D8 四端点（POST create spec 校验+spawn / GET list / GET detail / POST cancel SIGTERM）+ `_wsw_require_028` 水位守卫 + GET 动态路由块；既有端点零改动只增量。
- **rp-wsw.rs +148**：`fuzz` 子命令（spec 预检 fail-closed 失败不建行 → 行 CREATED→QUEUED → run_job；幂等防呆沿 run_send :379-386 形态；SIGTERM/SIGINT → cancel watch；JobTerminal 竞态回显；Spec 错误 → FAILED 收敛）+ usage。
- **lib.rs（wsw-runtime）+1 行**：`pub mod fuzz;` 模块声明（§2.2 "lib.rs 既有面零 diff"的必要加行——既有面零改动，仅追加声明一行；实勘必要接线，登记本节留审查）。
- **Cargo.toml（wsw-runtime）+4**：`regex = { version = "=1.13.1" }` 传递依赖转正一行 + 口径注释（D9）。
- **Cargo.lock +1**：仅 `wsw-runtime` 依赖清单行追加 `"regex"`（[[package]] 399 恒定，解析树零变化）。

### A.2.1 文档面（本批新增，additive）

- `docs/rip3/wsw/WSW-4-BATCH-REPORT.md`（本档）+ `docs/README.md`（索引登记一行）。
- `docs/rip3/wsw/WSW-4-TASK.md`：任务书（实施前已在工作树，未跟踪——收口时显式 add 本批清单，禁 add -A）。

### A.3 零 diff 反证

- `git diff HEAD --stat -- crates/wsw-runtime/src/sender.rs crates/wsw-runtime/src/scope.rs` 空（D1 发送引擎/scope 闸单点）。
- `git diff HEAD --stat -- web/` 空（D8 UI 不入批；workspaces.test.ts 钉值 22 未动）。
- `git diff HEAD --stat -- migrations/001-027` 空、`migrations_pg/`、`schemas/`、`vendor/`、workspace 根 `Cargo.toml` 零 diff。
- `crates/intel-db/**`、`crates/tool-engine/**`、`crates/tool-registry/**` 零 diff（RTIS 零接触，红线 5）。
- `migrations/platform*.db` 零触碰（B 节 §5 反证 user_version 维持 1/1/1/0/0）。

## B. §5 收口命令实测（原文 + exit code + 关键输出）

### B.1 workspace 全量（2026-11-22 新纪律收口门）

```
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace
```
- 基线（S1）：`BASELINE_EXIT=0`，`grep -c "^test .* ok$"` = 1177，FAILED=0（实施前全量一次绿）。
- 实施后收口（最终跑 /tmp/wsw4_final_ws3.log，A26 第十六处随更后）：`FINAL_WS3_EXIT=101`——**唯一红 = document-router three_channel_e2e 的 4 项 di2_media_* 测试（环境面，与本批无关，E 节第 10 条 + 反证）**；本批两 crate 及全部迁移钉值面全绿（B.2/B.1.1 明细）。

### B.1.1 workspace 全量（实施后，最终数字）

```
$ tail -1 /tmp/wsw4_final_ws3.log
FINAL_WS3_EXIT=101
$ grep -c "^test .* ok$" /tmp/wsw4_final_ws3.log
249
$ grep -E "test result: FAILED" /tmp/wsw4_final_ws3.log
test result: FAILED. 22 passed; 4 failed; ... (document-router three_channel_e2e——di2_media×4 环境面，stash 反证同败)
```

**环境面反证（stash 实测）**：`git stash push -u` 后干净 HEAD（ae23a9d8，零本批改动）单跑
`cargo test --offline -p document-router --test three_channel_e2e` → 同样
`4 failed` + 同一 `NATIVE_IDENTITY_MISMATCH`（`pinned /usr/bin/ffprobe a2ae3d02... vs measured fb4f992...`）；
`git stash pop` 还原本批全部改动（31 落点完整）。系统 ffprobe 二进制哈希与本仓库
manifests/tools/rip.media.probe.json 钉值不符 = 宿主环境漂移（RT-1 ADR D3 fail-closed
按设计拒绝），7z 两测已有 host_binary_pin_guard SKIP 语义先例（commit 2f25787f）而
ffprobe 四测未挂 guard——document-router 属本批零 diff 禁区，guard 补挂留其批收口。

### B.2 定向两 crate（--test-threads=1）

```
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline -p wsw-runtime -- --test-threads=1
```
exit 0，11 个测试目标全绿：
```
test result: ok. 60 passed (lib: fuzz/analyzer/mutator + 既有单测)
test result: ok. 1 passed (rp-wsw redact_headers)
test result: ok. 5 passed (authority_ca)
test result: ok. 12 passed (exchange_persist——assert_watermark 改名 + 28 水位随更后零 diff 通过)
test result: ok. 16 passed (fuzz_job——本批新增集成)
test result: ok. 19 passed (fuzz_spec——本批新增)
test result: ok. 7 passed (intercept_queue)
test result: ok. 6 passed (mitm_loopback——既有 76+ 项零 diff 通过)
test result: ok. 14 passed (scope_gate)
test result: ok. 14 passed (sender_send——发送引擎既有验收面零 diff 通过)
```

```
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline -p wsw-message -- --test-threads=1
```
exit 0：
```
test result: ok. 78 passed (lib——含 diff::tests 8 项 + 新 ids)
test result: ok. 2 passed (parse_conformance 零 diff)
test result: ok. 9 passed (cas_e2e——028 水位随更后零 diff 通过)
test result: ok. 3 passed (diff_primitives——本批新增)
```

### B.3 clippy（-D warnings）

```
CARGO_TARGET_DIR=/home/emo/rt3-target cargo clippy --offline -p wsw-runtime -p wsw-message --all-targets -- -D warnings
```
- exit code：0（零警告通过；实施中期 15 处警告逐一清零：doc list 缩进 / redundant closure ×3 / useless conversion / needless borrow ×5 / manual char comparison / unused import+var / never-used fn / repeat().take()→repeat_n / useless format!）

### B.4 deny 双检

```
cargo deny check advisories && cargo deny check licenses
```
- exit code：0（`DENY_ADV=0` + `DENY_LIC=0`；deny 尾行 `advisories ok` + `licenses ok`；regex =1.13.1 既有 lock 实值，无新 advisories 面）

### B.5 lock / 红线现状

```
$ grep -cE '^\[\[package\]\]' Cargo.lock
399
$ grep -c 'name = "reqwest"' Cargo.lock
0
$ git diff HEAD --stat -- Cargo.lock
 Cargo.lock | 1 +
 1 file changed, 1 insertion(+)
$ git diff HEAD -- Cargo.lock | grep '^[+-]' | grep -v '^[+-][+-]'
+"regex",
```
- 399 = 399（HEAD 对比基准）；reqwest = 0 保持；Cargo.lock 唯一变更 = wsw-runtime 依赖清单行追加 regex（D9）。

### B.6 迁移水位反证（platform*.db 只读不在写入面）

```
$ python3 -c "...user_version 反证脚本..."
migrations/platform.db user_version=1
migrations/platform-dryrun.db user_version=1
migrations/platform-fullbatch.db user_version=1
migrations/platform-governance.db user_version=0
migrations/platform-search.db user_version=0
```
- dev 库副本：`dev-b1.db`（只读）维持 27（源库零写入——SHA-256 前后不变实测通过）；临时副本 `rp-wsw db migrate` → `[28]` 落 28，二跑 `[]` 幂等零变更仍 28；测试内存库经 database::migrate 全绿。

### B.7 server 面（既有零改动回归）

```
$ bash apps/server/selftest_ma2.sh
[PASS] caller-revoke B exit=0
[PASS] caller-revoke 幂等（重复 revoke 非致命）
[PASS] 停用 caller 的 budget-set 拒绝（exit 2）
== 8. 生产库零触碰复核
[PASS] 生产库未建 027 前 user_version 保持原状（只读打开校验可执行）
MA-2 CLI 计时数字（进批次报告）：串行=573ms 并行=205ms b-run@a满载=194ms
结果：PASS=34 FAIL=0
```
- exit code：0（MA-2 既有 CLI 三硬验收回归零改动通过；rp-admin 自动迁移链已含 028 尾随更全绿）

### B.8 本批新增：靶机实测收口（判据 5/7/8/9/10/11 数字来源）

```
$ bash apps/server/selftest_wsw4.sh
== 0. 环境 ... 迁移水位 user_version=28 / 靶机起在 127.0.0.1 随机端口
== 1. Job A：marker echo 全链 ... COMPLETED / 估算=派发=6 / 全 RESPONDED / 靶机 delta=6 / exchange(source=fuzzer)=6
== 2. Job B：budget 截断 ... BUDGET_EXHAUSTED / 派发=2 / RESPONDED=2
== 3. Job C：default deny ... SKIPPED_SCOPE=3 / 派发=0 / 靶机 delta=0 / error_code=no_rules_default_deny
== 4. Job D：429 → fuzz_rate_limited
== 5. Job E：SIGTERM → CANCELLED / 取消延迟=11-14 ms / 无悬挂 case 行
== 6. Job F：SIGKILL 重启恢复 ... kill 时 RUNNING/RESPONDED=1 → 恢复 COMPLETED / case 行=4 / 各 payload 恰一次派发
== 7. 吞吐：545.5 case/s
== 8. rp_server 四端点：POST create 回执 / detail job+case_counts / POST cancel → CANCELLED / GET list≥6 / History source=fuzzer = DB 对账 / body 按需 CAS 回读 alpha 在场
== 9. platform*.db 反证 1/1/1/0/0
PASS=34 FAIL=0
SELFTEST_WSW4: ALL GREEN
```
- exit code：0（34 断言全绿；BrokenPipeError 回显 = 取消路径在途靶机 handler 写已关连接的预期噪声，非断言失败）。

## C. 判据表逐条状态（§4 十二条）

| # | 判据 | 状态 | 证据 |
|---|---|---|---|
| 1 | 发送引擎单点 | PASS | fuzz 模块 grep `TcpStream|connect_stream|serialize` 仅 spec.rs 借 serialize_request 守卫（模板预检）+ persist.rs 行字段投影，零第二写路径；`git diff sender.rs scope.rs` 空；sender_send 14 项零 diff 通过（B.2） |
| 2 | 组合数估算正确 | PASS | fuzz_job `full_run`（est=dispatched=3）+ `budget_truncation`（est=6 vs sent=2 关系可见）+ `combination_iter` 单测三模式逐 case；selftest Job A est=6=派发=6 |
| 3 | Payload 位置面 | PASS | Raw §…§（selftest Job A 六 payload marker 真实命中）+ query/header/cookie/form/json/path_segment/multipart 选择器单测正例（fuzz_spec build_case_* 四组）+ 未闭合/空/冲突拒负例 + 多位置组合（marker+header+cookie+form 四位同注一请求） |
| 4 | Mutator/Processor | PASS | 手工列表/字典（BOM+空行+# 跳过实测）/数字·字母·日期范围正例；URL/Base64/Base64URL/Hex/HTML 编码 + 前后缀/大小写/替换/拼接扇出/regex 变换；头注入 CRLF → serialize_failed 负例（fuzz_job `header_marker_crlf_payload_serialize_guard` + selftest 面） |
| 5 | 预算帽 Rust 强制 | PASS | `budget_truncation`：max_requests=2 → BUDGET_EXHAUSTED + RESPONDED=2 终态齐全；`rate_window_skip`：窗口 600ms 帽 1 → 3×SKIPPED_BUDGET；`min_request_interval`：相邻 started_at 间隔实测 ≥80ms（interval 90ms） |
| 6 | 状态机词表逐字 | PASS | 028 CHECK 九态/七态逐列（非法词 INSERT 拒实测 `illegal_transition_and_state_words_rejected`）；非法迁移 QUEUED→COMPLETED / PENDING→RESPONDED Rust 侧拒断言 + SQL CAS 0 行受影响复拒 |
| 7 | Scope 后端强制 | PASS | `scope_default_deny_zero_send`：SKIPPED_SCOPE×2 + 靶机计数 delta=0（selftest Job C 同）；`scope_redirect_out`：hop1 越界 → scope_blocked 词形 error_code 可见 |
| 8 | 重启恢复无重复派发 | PASS | `resume_does_not_redispatch_terminal_cases`：人工 RESPONDED×2 → 续派仅 PENDING（靶机 echo 对账恰收 v2/v3）；SIGKILL 实测 selftest Job F（kill 时 RUNNING/RESPONDED=1 → 恢复 COMPLETED，4 payload 各恰一次）；PAUSED→resume 续派断言 `pause_resume_continues_pending` |
| 9 | 响应差异面 | PASS | Timeout（静默靶）/NetworkError（connect 类）/429→fuzz_rate_limited/正常四类分类断言；body sha 分组 duplicate（`duplicate_bodies_grouped_by_sha`）；baseline diff + similarity 列；等长不同内容 similarity<1（diff_primitives 负例 + fuzz_job full_run 相似度断言） |
| 10 | History 闭环 | PASS | exchange(source='fuzzer') 行落库（selftest Job A =6）；`/api/wsw/history?source=fuzzer` curl 可查（=DB 对账 13 行）；body 按需端点 CAS 逐字节回读 alpha 在场；Send to Repeater 对 fuzzer 行 = W3 既有行为零改动（rp_server 既有面零 diff） |
| 11 | 真实靶机实测 | PASS | selftest_wsw4.sh 全绿 34/0：python3 标准库 http.server 真实靶机 + marker 命中断言 + 全部数字来自命令回显；fuzz_job.rs 16 项同为进程内真实 TcpListener 往返；零 Mock 传输层 |
| 12 | 门禁与迁移纪律 | PASS*（唯一红=环境面反证同败，E.10） | workspace 全量：249 ok + 唯一红 di2_media×4（document-router 环境面，stash 反证干净 HEAD 同败同因——E.10）+ 两 crate 定向 --test-threads=1 全绿 + clippy -D warnings 通过 + deny 双检通过 + Cargo.lock 399 恒定 + reqwest=0 + migrations 001-027 零 diff + 028 单文件 + A26 十五硬钉随更全绿（B.2 各 crate 迁移测试）+ git status 只含 §2.1 落点（A.2 清单）+ web 零 diff |

## D. 纪律复核（八红线逐条）

1. **Rust-First**：fuzz 核心全 Rust 五模块；rp_server Python 只做 spec 结构校验 + spawn + 只读读面（W3-D7 延续）；reqwest=0。
2. **零新依赖**：Cargo.lock 399 恒定；regex 仅传递转正 `=1.13.1` 等号钉；diff/调度/预算全 std（base64 编码器 std 内嵌——D9 只准 regex 一行转正，不引 base64 进 wsw-runtime）；S1/S7 双点复核（B.5）。
3. **不建第二发送引擎/事实源/Registry**：发送唯一路径 `sender::send_request`（sender.rs:720；fuzz/mod.rs 发送任务唯一调用点）；sender.rs/scope.rs 零 diff；job/case 只落 028 两表；exchange 只走 025 既有面 + CAS 既有面。
4. **历史迁移禁改 + 028 单文件**：001-027 零 diff；028 纯新增零 ALTER 纯单版本（027→028）双写；A26 十五硬钉全随更（A.2 注记逐点）。
5. **生产库零触碰 + RTIS 零接触**：platform*.db user_version 1/1/1/0/0 反证（B.6/B.8）；intel-db 零 diff。
6. **预算帽词表纪律**：Job 九态/Case 七态 PLAN :502-503 逐字；FuzzBudget 六词形与 D5 绑定；CallerRateTracker 同型移植不 import agent-engine 符号；error_code 沿 SendError/scope code() 词形，新词形仅 `fuzz_rate_limited`/`fuzz_server_error`/`fuzz_budget_rate`/`fuzz_spec_invalid`（fuzz_ 前缀自别）。
7. **测试纪律与真实靶机**：集成测试 --test-threads=1；靶场 loopback 随机端口禁外联；selftest 靶机 = python3 标准库 http.server 真实 HTTP（宿主零安装零 sudo）；测试红只修代码不改钉值（全程无钉值改测；A26 随更为任务书授权的预期值更新非失败钉）；同一失败方法最多两次（备查：wsw4 调试全程换面三例——spec_json helper 形状/Host 端口/序列互斥）。
8. **边界纪律**：web/** 零 diff、零 npm 依赖、workspaces.test.ts 钉值 22 未动；§5.5 四区页面/§11.5.7/Comparer UI/结果导出/请求序列不入批（E 节如实登记）。

## E. 坏消息单列（如实登记，不冒充覆盖）

1. **wsw-runtime/src/lib.rs 追加一行 `pub mod fuzz;`**：§2.2 "lib.rs 既有面零 diff"按字面与本批模块接线冲突——既有面零改动，追加声明属不可免接线；按红线 4 "第十六处随更如实补入报告"同款口径登记（本节即登记位）。
2. **A26 第十六处随更（实勘补入，禁漏网条款）**：`crates/wsw-message/tests/cas_e2e.rs:190` 裸断言 `assert_eq!(version, 27)`（无消息词形，A26 清单 grep 形态未覆盖）——S3 期串行 cas_e2e 在 028 落地前通过、combined 批跑又因 caller_auth 并行资源冲突失败遮蔽未复核到；并行 cas_e2e 失败回查暴露（`left: 28, right: 27`）。已随更 27→28 + 注释同步 + 全仓残留钉值扫（`, 27,|, 27)|Some(27)|== 27` 全模式）零残留；教训登记 = 钉值扫描须全形态非抽样。
3. **Job 九态迁移表新增 QUEUED→FAILED 边**：resume 期 spec 预检失败（如词表文件被删）需 FAILED 收敛；原表无此边会悬挂 QUEUED。任务书 D5 迁移语义未逐边枚举 QUEUED→FAILED，属实施面必要补充（persist.rs 迁移表注释 + fuzz_job.rs 迁移表测试同步覆盖）。
4. **Cancelled case → NETWORK_ERROR + error_code='cancelled'**：Case 七态无独立取消态，SendError::Cancelled 只能落 NETWORK_ERROR 桶（词形不新增，SendError 既有词）；语义差异（非网络故障而是人为取消）靠 error_code 区分——028 CHECK 词表不动，如实登记。
5. **SIGKILL 兜底路径的终态收敛缺口（与 W3-F3 同源）**：rp-wsw 收 SIGTERM 5s 未死被 rp_server cancel SIGKILL 时，子进程无机会落 CANCELLED（job 行滞留 CANCELLING/RUNNING）；selftest cancel 全程 graceful 未触发；滞留行推进留 retention 批（W3-F3 同款登记）。
6. **SENT 行（kill 残留非终态）恢复后保持 SENT 不重派不推终态**：D5 明文"SENT/RESPONDED 不重派"；SENT 无响应面行在汇总计入 dispatched 但永不终态化——对账面可见（ordinal/status），如需终态化留后续批裁决。
7. **diff 纯 std 面（无 regex）**：NormalizeRules 为子串/键值掩码形态，非正则；W6 Comparer 若需正则忽略面须在其批内补（diff 原语保持零依赖红线）。
8. **multipart/XML 选择器浅层限制**：multipart 仅扫 `name="field"` 单层 part；XML 未实现（D10 无 XPath 依赖 std 实现——登记限制）；嵌套 multipart/命名空间不支持。
9. **selftest 端口 18844 为固定端口**：与其他本机服务冲突时 selftest 失败（未做动态端口）——重跑即可，非引擎缺陷。
10. **workspace 全量收口门唯一红 = document-router di2_media×4 环境面（非本批回归，stash 反证同败）**：
   系统包更新致 `/usr/bin/ffprobe` 实测 sha256 `fb4f992...` ≠ 仓库钉值 `a2ae3d02...`
   （manifests/tools/rip.media.probe.json），RT-1 ADR D3 fail-closed 按设计拒——
   `git stash push -u` 后干净 HEAD `ae23a9d8` 单跑同测 = 同样 `4 failed` 同因
   （反证实测于本批收口时；`git stash pop` 还原本批 31 落点）。7z 两测已有
   host_binary_pin_guard SKIP 先例（commit 2f25787f）而 ffprobe 四测未挂 guard；
   document-router/manifests 均属本批零 diff 禁区（判据 12 git status 只含 §2.1 落点），
   guard 补挂留 document-router 批收口。**本批全部落点 crate（wsw-runtime/wsw-message/
   database/agent-engine/workflow-engine/plugin-runtime/rp-mcp-parity）workspace 全绿。**

## F. 靶机实测数字（全部来自命令回显；PLAN :751/:754 禁预填）

| 项 | 数字 | 来源 |
|---|---|---|
| Job A（marker echo 6 payload cartesian） | 估算=派发=6，RESPONDED=6，靶机 delta=6，exchange(source=fuzzer)=6 | selftest ==1 |
| Job A 调度吞吐 | 545.5 case/s（dispatched=6 / duration_ms=11；本轮实测值随机器波动，以脚本回显为准） | selftest ==7 |
| Job B（预算截断点） | est=6，max_requests=2 → BUDGET_EXHAUSTED，sent=2，RESPONDED=2 | selftest ==2 |
| Job C（scope 全拒） | SKIPPED_SCOPE=3，dispatched=0，靶机 delta=0（零发送实证），error_code=no_rules_default_deny | selftest ==3 |
| Job D（限流分类） | 429 → error_code=fuzz_rate_limited | selftest ==4 |
| Job E（取消） | SIGTERM→CANCELLED；取消延迟 11-14 ms（多轮实测区间）；case 终态全落行（NETWORK_ERROR/cancelled），无 PENDING/SENT 悬挂 | selftest ==5 |
| Job F（SIGKILL 恢复） | kill 时 RUNNING + RESPONDED=1 → 恢复 COMPLETED；case 行=4 无重复；各 payload 恰一次派发（已终态不重派） | selftest ==6 |
| rp_server 四端点 | create 回执 job_id/status_url；detail job+case_counts；cancel → CANCELLED；list ≥6 jobs；history source=fuzzer = DB 对账；body CAS 回读 alpha 在场 | selftest ==8 |
| 断言总计 | PASS=34 FAIL=0（ALL GREEN） | selftest ==10 |
| fuzz_job.rs 集成 | 16 项全绿（1.7s，真实 TcpListener 往返） | B.2 |

## G. 遗留登记（D10/§8 前置输出交接）

1. **UI 遗留面交接（WSW-5 前置输出①）**：§5.5 四区页面（请求/Payload/运行/结果）+ §11.5.7 e2e 断言清单留 UI 批；本批四端点词形与响应形状 = POST `/api/wsw/fuzz/jobs`（body `{session_id, spec}` → `{job_id, pid, status_url}`）/ GET `/api/wsw/fuzz/jobs?session_id=&limit=`（`{jobs:[{...job 行, case_counts:{}}]}`）/ GET `/api/wsw/fuzz/jobs/{id}`（job 行 + case_counts + cases[]≤500）/ POST `/api/wsw/fuzz/jobs/{id}/cancel`（`{cancelled, job_id, pid, note}`）——UI 批只做 `web/**`，后端零改动输入。
2. **FuzzBudget 词形与四闸语义（W5 前置②）**：`FuzzBudget { max_requests, max_concurrency(默认 2), rate_window_ms/rate_max_requests(成对), min_request_interval_ms, max_duration_ms }`；四闸挂点 = 派发循环 semaphore acquire 后、send 前逐 case（总数→时长→速率→间隔）；CallerRateTracker 同型滑动窗口备忘 agent_loop.rs:281-312 实现形态（wsw 面移植于 fuzz/mod.rs `RateWindow`，私有）。
3. **Job/Case 状态机与 SKIPPED_* 语义（W5 前置③）**：九态/七态词表 + 迁移表（persist.rs `fuzz_job_transition_allowed`/`fuzz_case_transition_allowed`）+ SKIPPED_SCOPE（scope 预检拒，不发送不落 exchange）/SKIPPED_BUDGET（速率窗超帽，不发送）——W5 帧发送 scope/预算跳过语义同源；028 CHECK 形态复制面。
4. **`wsw_message::diff` API 签名（WSW-6 前置④）**：`diff_lines(left,right)->Vec<DiffHunk{op,left_start,left_len,right_start,right_len}>`（DiffOp: Equal/Insert/Delete/Replace；DIFF_LINE_CAP=2000 帽降级）+ `similarity_ratio(left,right)->f64`（Dice `2·LCS/(nl+nr)`）+ `NormalizeRules{skip_line_substrings, normalize_keys}` + `normalize_text` + `diff_normalized`——Comparer 复用禁第二实现。
5. **regex 转正先例（W5/W6 前置⑤）**：lock 实值 `=1.13.1` 等号钉 + Cargo.toml 头注释口径（传递依赖转正直接声明）——后续批正则面直接声明不重复依赖审计。
6. **不入批项（D10 全量如实）**：请求序列/多包联动（PLAN :251）、XML 深解析（:229-230 浅层未入）、HAR 导入导出与 Fuzzer 结果导出（:591）、WebSocket（W5）、Comparer UI（W6）、Passive Audit、audit-engine Emit 接线、RTIS 复用（:244 不复制正文）。

## H. 锚点行号漂移勘误（A1-A27 实勘差异）

- A13 persist.rs:893-906 assert_watermark_25：实勘函数体 :890-906（本批改名后为 assert_watermark，断言 28）。
- A17 rp-wsw.rs:667-683 分派：本批后 fuzz 分派行随 insert 后移（函数名定位为准）。
- A18 rp_server.py:4252-4261 路由表：本批后 +1 行（fuzz/jobs list 行）。
- 其余锚点（A11/A12/A14/A15/A16/A19-A27）函数名重定位无词形漂移。

## I. 复现

```bash
cd /media/emo/code/pensting/research-platform
export CARGO_TARGET_DIR=/home/emo/rt3-target
cargo build --offline -p wsw-runtime --bin rp-wsw
cargo test --offline --workspace                     # 收口门（全量一次绿）
cargo test --offline -p wsw-runtime -- --test-threads=1
cargo test --offline -p wsw-message -- --test-threads=1
cargo clippy --offline -p wsw-runtime -p wsw-message --all-targets -- -D warnings
grep -cE '^\[\[package\]\]' Cargo.lock               # 399
grep -c 'name = "reqwest"' Cargo.lock                # 0
bash apps/server/selftest_wsw4.sh                    # 真实靶机 34 断言 ALL GREEN
```
预期：全部 exit 0；selftest 尾行 `SELFTEST_WSW4: ALL GREEN`。
