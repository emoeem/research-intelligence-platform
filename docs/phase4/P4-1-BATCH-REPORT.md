# 批次验收报告：P4-1（迁移 008 与 context-broker 骨架）

- 批次：P4-1（第 2/7 批 = 设计稿 CONTEXT-BROKER-DESIGN §11 M4.0；2026-11-10）
- 修改范围：migrations/008_context_broker.sql（新增）、crates/domain（context 模块 + error 一变体 + lib 登记）、crates/context-broker（新增 crate：Cargo.toml + lib/error/estimate/policy + tests/policy_estimate.rs）、crates/database/tests/migrations_context_broker.rs（新增载具）、既有三个迁移测试链长断言 001..007 更新为 001..008、根 Cargo.toml 与 Cargo.lock（workspace 成员与依赖登记）、docs/README.md（P4-0 已登记）与 docs/phase4/（P4-0 设计文档 + 本报告）
- 新增迁移：1 个（008_context_broker.sql，编号落库前再实查 `ls migrations/*.sql` 为 001 到 007，008 空闲；P4-0 决议 1 与 ROADMAP §1.4 规则一致）
- 新增第三方依赖：0 个（serde_json 为既有 vendored 面，tool-registry/tool-engine 已用；context-broker 依赖面本批为 domain + serde_json 两个，audit-engine/search-engine/resource-engine 在 P4-2/P4-3 消费批次加入，database 随 P4-4 manifest store 加入，全阶段封顶 PHASE4-DESIGN §5.1 冻结的五内部依赖集）
- 数据变更：开发库副本 migrations/dev-p4.db（测试载具新建，git check-ignore 实证 IGNORED）与内存库测试；生产 platform*.db 零写入（红线，git status 无任何 *.db 变更行）

## 判据逐条（PHASE4-DESIGN §6 P4-1 行）

1. **迁移在开发库副本幂等执行两次无差异**：crates/database/tests/migrations_context_broker.rs 载具（migrations_tool_contract.rs 的 dev-p13 先例同款形态）实测通过：discover 见 001..008 有序；首跑八条全应用（链尾 008_context_broker，user_version=8，schema_version 带 008_context_broker.sql 登记行）；二跑零变更（sqlite_master 结构转储、user_version、schema_version 行三者逐一相等）；DDL 字段覆盖设计稿 §6.1 三表草案列 + selection_hash（§7.3）；五类 CHECK 词表拒绝用例（intent_status/layer/kind/included/est_tokens/event 越界值全拒）；外键孤儿拒绝（item 无父 manifest、ref_log 无父 item）；删除语义实测（删 manifest 连带 item 与 ref_log 归零，ON DELETE CASCADE，pool.rs:247 foreign_keys=ON）。
2. **estimator 纯函数单测基线通过**：tests/policy_estimate.rs 十三项（空文本 0、ASCII 4 比 1、CJK 1 比 1、三类混合、向上取整、count_chars 分解）；预算策略另九项（默认分配在 total=8000 逐项等于设计稿 §9.1 示例 400/608/1900/760/2052/1140/1140 且七项和恰 8000、大预算 l0 封顶生效、40 tokens 微预算取整吸收、JSON 全量加载、五类畸形 JSON 拒绝、new 参数越界拒绝、reserve 硬下限分配期强制、借用规则仅 l3 到 l2、分项名与设计稿 §5.1 行名一致）。
3. **cargo test --offline -p domain -p context-broker -p database 全绿**：72 passed 0 failed（逐测试二进制：context-broker lib 1 + policy_estimate 13 + database lib 6 + 四个迁移载具 7 + domain lib 29 + agent_types 7 + tool_run_types 9）。
4. **全量门**：cargo test --offline --workspace 226 passed 0 failed（Phase 3 收口基线 207 + 本批 19）；cargo clippy --offline --workspace --all-targets -- -D warnings 零告警退出码 0；cargo fmt --all -- --check 通过。
5. **头部约束**：context-broker 与 domain 新模块沿用 #![forbid(unsafe_code)] 与 #![deny(missing_docs)]（crate 级属性，域内 pub 项全带文档）。

## 执行中修正记录（如实声明）

1. **复合外键表级化**：008 初稿把 ref_log 对 item 的复合外键写成列级 `REFERENCES context_manifest_item(manifest_id, item_id)`，SQLite 拒绝（"foreign key on item_id should reference only one column"，migrations_agent_data_plane 载具首跑实测报错）；修正为表级 `FOREIGN KEY (manifest_id, item_id) REFERENCES ... ON DELETE CASCADE`，修正后全绿。该修正属 DDL 语法落位，不改设计稿 §6.1 语义（双外键 + CASCADE 原样保留）。
2. **clippy 两处**：doc_lazy_continuation（domain lib.rs 文档 bullet 第二行以 + 开头被当作嵌套列表）、manual_range_contains（policy.rs 校验两处区间判断改 (0.0..=1.0).contains），均已修，语义不变。
3. **fmt 归一**：cargo fmt --all 对三个测试文件与 policy.rs 做了空白归一（无语义变化）。

## 未执行的测试

- worker unittest（python）：本批零 Python 改动，未跑（Phase 3 收口基线 125 全绿仍有效）
- intent/source/assemble/manifest/cache 面：属 P4-2 到 P4-5 批次（本批未实现，判据不涉及）
- 向量检索轴：Phase 4 明确不实现（PHASE4-DESIGN §3 第 2 条）

## 对 ERI / RTIS / KB / Skills / Tools 的影响

- RTIS 本体零改动（红线）；ERI/KB 无接触；Tool Registry 登记面零改动（P4-0 决议 3 前提）；既有九 crate 中仅 domain 增量（纯追加：context 模块 + error 一变体 + lib 登记），其余八 crate 零改动

## 风险

- token 估算常数未对真实 tokenizer 校准（设计稿 §13.1 待验证，P4-6 前校准，只修常数不改结构）
- 三表仅结构在位，数据面语义（装配写入、expand 记账）在 P4-4 落地时才受真实写入检验；008 已按幂等 + 只追加纪律落位，若 P4-4 发现列缺口，走「只新增编号文件」的增量迁移（不回改 008）
- 既有三个迁移测试的链长断言从 1..7 放宽到 1..8：后续每批新增迁移需同步维护（既有先例，007 合入时同款操作）

## 回滚方式

- 代码：`git revert <批次 commit>` 整批撤销；crates/context-broker 与 008 为整文件新增，revert 即移除；domain 的 context 模块与 error 变体随批回滚，不影响既有调用面（无既有调用方）
- 数据：dev-p4.db 为测试载具（gitignored，删除即清净）；生产 platform*.db 零写入，无生产回滚面
- 文档：本报告与 docs/README.md 登记行随批回滚

## 后续工作

- P4-2（M4.1）：L0 规则包源 + L1 目录源直连 tool-registry（P4-0 决议 2）+ card/brief 生成器（决议 3 落位），brief 生成开销经 cache-stats 落数
- P4-3（M4.2）：intent 纯函数 + knowledge/rtis 只读源 + 打分与 matched_signals
