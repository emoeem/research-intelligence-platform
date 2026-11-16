# P5-7 批次报告：结果归一化收口、Evidence 与 RTIS 闭环

> 批次类型：实现批（P5-7 收口批）。规格源：Phase 5 补充计划 §7 P5-7 +
> §5.7/§5.8/§3.1。前置：P5-6 批已收口 evaluator 归一化契约面
> （result-normalized.v1 + 契约测试，见 P5-6-BATCH-REPORT.md）；本批交付
> Evidence Artifact/SHA256、Trace 六 ID 贯通、RTIS Finding 闭环、重复
> Finding 识别与五态守卫负例实拍。

## A. 本批完成内容

1. **021 迁移 research_evidence 表**（migrations/021_research_evidence.sql
   新增）：Evidence Artifact 持久层（evidence_id re- 前缀 + item/plan 双
   引用 + tool_run_id/trace_id 六 ID 锚 + evidence_type 词表
   provider_result/normalized_result/log_excerpt + content_hash SHA256 +
   payload_json append-only 不可变快照 + 四索引含 content_hash 查重）；
   schema_version 登记 version=21（020 惯例五列）+ PRAGMA user_version
   双写。
2. **迁移链钉值九文件同步**（ab5907e5 实录清单 + 链尾距离断言，020→021）：
   agent-engine 两测（migration_idempotent 全量清单+user_version、
   migration_dev_copy_p7_3 增量区间与链尾）、database 四组
   （migrations_runtime_node/tool_contract/context_broker/agent_data_plane
   的 discover 清单+链尾+user_version、backup_restore 链尾断言
   Some(21)）、plugin-runtime lifecycle_full_path 重放枚数（010..021
   十二枚）、workflow-engine migration_replay 链尾 user_version。
3. **Store Evidence 采集面**（crates/verification-engine/src/store.rs）：
   - EvidenceRecord 结构 + record_evidence（词表守卫 + ID 形状守卫 +
     payload 紧凑序列化 SHA256 + INSERT + evidence_collected 事件联动，
     事件词表 020 预留面本批启用）；
   - 重复观察识别（§5.8 第四层去重）：同 plan 内同 content_hash 跳过落
     档（append-only 快照不可变语义），dedup 留痕在 evidence_collected
     事件的 dedup/duplicate_of 标记上；
   - find_evidence_by_hash + list_evidence（计划项维度检索入口）；
   - 2 集成测试（采集/哈希/重复跳过/事件留痕 + 词表与 ID 形状守卫）。
4. **Trace 六 ID 贯通（P5-7 完成标准「Evidence 可以从 UI 跳转至来源任务
   和工具运行记录」的链路层）**：
   - provider.rs：execute_nuclei 改 submit_correlated（015 面
     tool_run.trace_id 落列）+ ProviderOutcome.trace_id 落值；可选项打包
     DispatchOptions（task_id/trace_id/network/template_path，参数面收敛
     过 clippy 函数参数阈值）；
   - rp-verify execute-plan：一次 execute 动作生成一个 trace 锚
     （tr-exec-<plan 尾>-<微秒>），贯通 set_result（item.trace_id 落列）、
     dispatched 事件、evidence 双帧；
5. **Evidence 双帧采集**（execute-plan nuclei 分支）：provider_result 帧
   （ProviderOutcome 信封全量：provider/version/tool_version/status_word/
   result/summary/duration_ms/rule_ref）+ normalized_result 帧
   （checked_normalized_doc 契约帧）；派发拒绝帧也落 log_excerpt 证据
   （失败现场留档，不静默）。
6. **RTIS Finding 闭环写面**：
   - provider.rs RtisPocProvider.record_verification（rt.py poc update
     --id N --verified --note，RTIS 既有接口 sanctioned 写路径；note 只带
     平台侧判定依据摘要，POC 正文/利用细节零复制，RTIS 红线）；
   - rp-verify finding-link 子命令：CONFIRMED 项的验证结论回填 RTIS；
     守卫面（非 CONFIRMED 拒绝 = 不把未确认结论当成漏洞事实；无
     rtis_poc_id 拒绝 = Finding 锚在 RTIS，RIP3 零自建事实库，§3.1 规则
     4）；回填成功落 result_assigned 事件（finding_link 标记）；
   - rp-verify evidence-list 子命令（021 检索入口）。
7. **usage_text 同步**（evidence-list/finding-link 两子命令）。

## B. 架构变化和复用说明

- Evidence 存储判定：证据档落 SQLite 021 运营面（020 同族编排执行态，
  P5-0-B3 判定沿用：编排执行态落运营面，PG 情报核零新表）；payload_json
  append-only 不可变满足完成标准「原始证据不会被知识更新覆盖」。
- Trace 六 ID 复用 015 面既有列与 submit_correlated 既有写入点（零新表）；
  六 ID 中 tool_run_id/trace_id 在验证链贯通（evidence 事件流可跳转
  tool_run）；agent_run/artifact 两 ID 本链无实体（验证执行无 agent 步骤、
  Artifact 以 evidence 档承载），如实登记非目标。
- RTIS 读写全部走 rt.py CLI（resolve_metadata/record_use/record_verifica
  tion 三面），零直写零 Schema 变更；重复 Finding 识别以 content_hash
  （Evidence 指纹）+ dedup_fingerprint（P5-4 六段验证层键）双键承载。

## C. 修改文件清单

| 文件 | 改动 |
| --- | --- |
| migrations/021_research_evidence.sql | 新增（research_evidence 表 + schema_version 21 + user_version 双写） |
| crates/verification-engine/src/store.rs | EvidenceRecord + record_evidence/find_evidence_by_hash/list_evidence + sha2 import |
| crates/verification-engine/src/provider.rs | execute_nuclei → submit_correlated + DispatchOptions 打包 + record_verification |
| crates/verification-engine/src/bin/rp-verify.rs | trace 锚生成 + evidence 双帧/拒绝帧落档 + finding-link/evidence-list 子命令 + usage |
| crates/verification-engine/tests/provider.rs | 调用点 DispatchOptions 适配 |
| crates/verification-engine/tests/store.rs | 2 证据档集成测试 |
| crates/agent-engine/tests/migration_idempotent.rs | 钉值 020→021 |
| crates/agent-engine/tests/migration_dev_copy_p7_3.rs | 钉值 013..020→013..021 |
| crates/database/tests/migrations_runtime_node.rs | 钉值 020→021 |
| crates/database/tests/migrations_tool_contract.rs | 钉值 020→021 |
| crates/database/tests/migrations_context_broker.rs | 钉值 020→021 |
| crates/database/tests/migrations_agent_data_plane.rs | 钉值 020→021 |
| crates/database/tests/backup_restore.rs | 钉值 001..020→001..021 + Some(21) |
| crates/plugin-runtime/tests/lifecycle_full_path.rs | 重放枚数 11→12 |
| crates/workflow-engine/tests/migration_replay.rs | 钉值 user_version 20→21 |

## D. 数据库迁移、Schema 和兼容性变化

- SQLite 迁移链 020→021（research_evidence 追加表，additive；零既有列/
  词值域改动）；九文件测试钉值同步（ab5907e5 先例口径）；SQLite 链头
  021，migrations_pg V 链零推进（PG 链头仍 V0006，硬红线「新增域永不落
  SQLite」在本域按 P5-0-B3 判定豁免：编排执行态与证据档属运营面）。
- 契约 Schema 零变更（result-normalized.v1 已随 P5-6 冻结）。

## E. 测试命令及真实结果

```text
cargo test --offline -p verification-engine
  → 38 lib + 2 provider + 6 store（含 evidence 2 新测）全过；0 failed
cargo test --offline -p database（九钉值面含 backup_restore 4 测全过）
cargo test --offline -p agent-engine --test migration_idempotent
  --test migration_dev_copy_p7_3 → 全过
cargo test --offline -p plugin-runtime --test lifecycle_full_path
  （migration_009 重放枚数 12）→ 过
cargo test --offline -p workflow-engine --test migration_replay → 3 过
cargo clippy --offline -p verification-engine --all-targets -- -D warnings
  → 零告警（DispatchOptions 打包收敛函数参数面）
```

## E2. CI 各 Job 状态和运行链接

- P5-7 首提 bf55c9b3：run 43942245579 **Rust workspace 红**
  （migrations_agent_data_plane p7_1 断言 user_version 期望侧漏改）；
  修正批 dbb582d1 补齐三处漏改钉值（链尾名 (21,_research_evidence)、
  全量逐枚清单补回 20 枚、uv/uv1 变量名变体），受影响五 crate 全量复测
  41 套件零 FAILED。教训实录：链尾钉值面比 ab5907e5 实录的八处形状更多
  （逐枚清单/变量名变体），后续链尾演进先按
  `'(1[0-9], "_'`、`'(2[0-9], "_'`、`uv1?` 三形状全量定位再改。

## F. 性能数据

- 证据采集为单行 INSERT + SHA256（payload KB 级，无性能面）；021 迁移
  幂等（IF NOT EXISTS）。无新增基准项。

## G. 未解决问题与风险

1. **finding-link 正例的 RTIS 写面实拍留待 P5-9 本地靶场**：当前实拍目标
   （127.0.0.1:1 无真实漏洞）做 CONFIRMED 正例回填会把真实 POC 条目的
   verified 状态标成虚假事实（污染 RTIS 知识库），按「证据最小必要」纪
   律不落实拍；负例两路（非 CONFIRMED 拒绝 / rt.py 不存在 ID 如实失败）
   已实拍，调用面（run_json）正确性由 P5-5 record_use 实拍先例背书。
2. 六 ID 中 agent_run_id/artifact_id 两锚在验证链无实体（验证执行无
   agent 步骤；Artifact 以 evidence 档承载）；若后续批次引入 agent 参与
   验证决策再补段。
3. Evidence 检索入口当前为 CLI 面（evidence-list）；rp_server 端点与 UI
   Evidence 页（§4.7）随 P5-9 页面族批次交付（依赖 ResearchPlansPage 的
   详情展开面）。

## H. 下一批依赖和明确的非目标

- P5-8 依赖：本批 evidence content_hash（覆盖矩阵的 Evidence 维度）、
  item.trace_id（研究 Trace 面）、两段指纹键补段（画像版本 + 知识审核
  状态，P5-4 G 遗留随本阶段 §5.8 键面收口）。
- 非目标：不做 Evidence 的 rp_server 端点（P5-9）；不做 agent_run 锚补
  段；不动 RTIS Schema 与 15155 对账口径。

## I. UI 截图与浏览器验收证据

- 本批为存储/CLI/链路面，无 UI 改动；Evidence UI 消费随 P5-9 页面族
  （见 G3）。

## J. RTIS 对账和数据安全回归结果

- RTIS 零直写：本批 RTIS 写面全部经 rt.py poc update（sanctioned 既有
  接口）；错误路径实拍（不存在 ID）零生产数据写入；15155 对账零回归
  （RTIS 库 Schema 与数据未动）。
- POC 正文零复制：record_verification 的 note 只携带平台侧判定依据摘要
  （result_reason），POC 正文/利用细节不入 021 证据档与日志。
- 生产 migrations/platform*.db 零触碰（实拍全部走 /tmp 临时库）。
