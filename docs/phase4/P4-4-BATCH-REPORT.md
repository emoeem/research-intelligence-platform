# 批次验收报告：P4-4（装配全流程、三表落库与 AssembledContext 契约）

- 批次：P4-4（第 5/7 批 = 设计稿 CONTEXT-BROKER-DESIGN §11 M4.3；2026-11-10）
- 修改范围：crates/context-broker（assemble.rs + manifest.rs + sources/artifact.rs 新增；source.rs 增 brief_for 默认方法；sources/registry.rs 覆写 brief_for；sources/mod.rs、lib.rs、Cargo.toml 登记）、tests/assemble_contract.rs（新增 10 项）、docs/phase4/（本报告）
- 依赖面变更：database 升为主依赖（ManifestStore 三表落库 + ArtifactSource 只读查询，P4-1 时冻结的 dev-dep 升格）；serde 升为主依赖（AssembledContext 序列化契约校验用，serde_derive 既有 vendored 面，quality-engine 先例）；零新增第三方
- 新增接口：Broker（assemble/expand/manifest 三公开方法，设计稿 §10 签名）、AssembleRequest、AssembledContext 及其子类型（IntentSummary/Layers/Citation/BudgetReport/EvictionRecord 等，全 Serialize）、Expanded、ManifestView、ManifestStore（insert_manifest/insert_item/append_ref_event/manifest/items/ref_events/expanded_tokens）、ArtifactSource、ContextSource::brief_for（默认 None，RegistrySource 覆写）
- 数据变更：开发库副本 migrations/dev-p4.db（测试重建，gitignored）与测试内存库；生产 platform*.db 零写入（红线，git status 无任何 *.db 行）

## 判据逐条（P4-4 批次 ask）

1. **同一任务两次装配产生两个 manifest_id 且 selection_hash 一致可对账**：测试在开发库副本（dev-p4.db，全链迁移 001..008 后）装配同任务两次：manifest_id 与 context_id 均不同（重装配出新 id），mode=ro 只读池对账 context_manifest 恰两行、selection_hash 逐字节相等，且与「按清单条目重算指纹」（source|ref_id|ref_version|content_hash|included|est_tokens 排序后 sha256，哈希原语复用 tool_registry::hash::sha256_hex）一致；只读池写操作被拒（mode=ro 面）。
2. **expand 写 context_ref_log 并从 l3_reserved 扣账返回增量 tokens**：40 字符载荷实测授予 10 tokens（est_tokens 口径），context_ref_log 落 EXPANDED 事件且 detail.granted_tokens=10；已用额度由 EXPANDED 事件推导（expanded_tokens，只追加语义无 totals 回写）；连续 expand 至额度耗尽后拒绝，错误信息含剩余额度与替代建议（「fallback: read summary」，设计稿 §5.3 第 4 条）。
3. **三表只追加不更新**：manifest.rs 零改写语句（测试 include_str 断言 + grep 复核 0 处）；同 manifest_id 重复插入被 PK 拒；每条目恰一条 ASSEMBLED 事件。
4. **同 content_hash 只装一次**：双候选同 sha256 装配后仅一个条目，warnings 落 dedup:content_hash 留痕。
5. **内联量超预算即装配失败（fail fast，设计稿 §9.2 第 5 条）**：60 候选灌入 1000-token 预算，驱逐记录使序列化结果约 1600 tokens 超总量，装配返回 InvalidInput（budget_report.used，信息含 fail fast）；落库发生在契约校验之后，失败路径零库写。
6. **驱逐顺序与降级链**：降级链用例（四个 800-token 候选，l2_knowledge=1900）：前两装 summary，后两降级 ref-only（item 保留、摘要清空、零驱逐）；驱逐用例（九个 ~700-token 工具，l2_tools=760）：tool:0 装 summary、tool:1 到 tool:7 降级 ref-only（756/760）、tool:8 连 ref-only 额度也无被逐（同分按 item_id 稳定序尾部先逐），原因定位到桶（budget:l2_tools）；L0/L1 永不驱逐有断言。
7. **citations[] 与 l2 摘要分离且字段齐全**：同一条目两处呈现（l2 摘要 + citations），citation 的 source/version/content_hash/uri 四字段齐全有断言；下游唯一合法引用集 = citations。
8. **Artifact 只读引用源（005 数据面）**：内存库按 005 建链（tool → tool_run → artifact），装配后 artifact 以 l3_refs + citations 引用进上下文（sha256 进引用，计划 §14:794 第三条），序列化面零正文内联（run_id/mime 只在 load 载荷）；expand 走 artifact load 记账扣 l3。
9. **【串行】依赖面**：005 tool_run/artifact 表在位（P1-3/P2 交付），P4-2/P4-3 的源面与 intent/score 面零语义改动（source.rs 只增 brief_for 默认方法）。

## 执行中修正记录（如实声明）

1. **装配期溢出候选不写 EVICTED 事件**：初版把驱逐记录同时写 context_ref_log，被 ref_log 复合外键拒绝（事件只能指向已装条目）。定稿：装配期溢出记录在 budget_report.evicted 与 totals_json 快照（清单头），EVICTED 事件留给运行期对已装 L3 展开的收回（P4-5 起使用）。
2. **契约校验口径落定**：设计稿 §9.2 第 5 条「序列化后校验一次」实现为整体序列化 vs 总预算（fail fast）；连带效应是小总量策略必须容纳 JSON 结构开销（结构面约 300 到 500 tokens），测试预算从 200/400/560 上调至 1000/8000 以让语义场景落在真实预算形状内，已在 assemble.rs 模块注声明。
3. **artifact ref_id 前缀统一**：candidates 的 ref_id 补 artifact: 前缀与 item_id/load 解析口径一致（初版裸 id 致 expand 解析拒绝）。
4. **clippy 四处**：clone_on_copy、type_complexity（artifact 行投影拆 type 别名）、redundant_field_names、manual_contains 与测试 helper 的 too_many_arguments（显式 allow，参数成组语义清晰），语义不变。
5. **两处文档注释长横线**（manifest.rs/assemble.rs）清除复验，grep 退出码 1。

## 测试命令与结果（实测）

- `cargo test --offline -p context-broker --test assemble_contract`：10 passed 0 failed
- `cargo test --offline -p context-broker`：46 passed 0 failed（lib 1 + assemble_contract 10 + catalog_sources 5 + intent_pure 11 + policy_estimate 13 + rtis_source 6）
- `cargo test --offline -p domain -p context-broker -p database`：104 passed 0 failed
- `cargo test --offline --workspace`：258 passed 0 failed（P4-3 基线 248 + 本批 10）
- `cargo clippy --offline --workspace --all-targets -- -D warnings`：零告警，退出码 0
- `cargo fmt --all -- --check`：通过
- `grep -P '[\x{2014}\x{2013}]'`（本批全部新文件）：退出码 1

## 未执行的测试

- cache 面（broker-cache 命中/失效）：P4-5（M4.4，本批 Broker 无缓存位，装配确定性由候选与策略版本保证）
- rp-context CLI：P4-5（ROADMAP §5.4 命令组 2）
- RTIS 源参与装配的端到端（真实 kit → assemble）：测试用 Mock 载具（决议 2 口径），真实源端到端随 P4-6 验收三测试
- budget_report.used 的 l3_reserved 与 expand 后实际值的对账视图：expand 记账已实测（expanded_tokens），装配报告快照静态口径属 P4-5 cache-stats 面

## 对 ERI / RTIS / KB / Skills / Tools 的影响

- RTIS 本体零改动（红线）；ERI/KB 无接触；Tool Registry 登记面零改动（brief_for 只读其公开接口）；005/008 两迁移既有结构零改动（本批零 DDL）

## 风险

- 小总量策略（< 约 600 tokens）会被整体序列化闸拒绝（结构开销）；设计稿示例量级 8000 无碍，AgentDefinition context_policy 下限在 Phase 7 接线时按本批次报告口径设定
- 同 content_hash 去重只对携带哈希的候选生效（registry/rtis/mocks 多为 None）：知识源（platform-kb）候选不带哈希（索引库无该列），去重率随 P4-5 缓存与 P4-6 验收测试复核
- 装配期 L0 截断（超 400 tokens 规则包）只记 warning 不驱逐：规则包扩容到 l0 分项以上时尾部规则静默缺失，P4-6 验收测试一覆盖该断言

## 回滚方式

- 代码：`git revert <批次 commit>`；assemble.rs/manifest.rs/sources/artifact.rs 为整文件新增，revert 即移除；source.rs brief_for 与 registry 覆写随批回滚（P4-2/3 测试不受影响）；serde/database 依赖行随批回滚
- 数据：dev-p4.db 为测试载具（gitignored，删除即清净）；内存库即焚；生产 platform*.db 零写入，无生产回滚面
- 文档：本报告随批回滚

## 后续工作

- P4-5（M4.4）：内容寻址缓存 + 七类失效触发器 + rp-context CLI 四子命令；Broker 挂 cache 位，expand 记账与 cache-stats 对账
- P4-6（M4.5）：验收三测试（不默认塞入/可解释/Artifact 引用）+ 实施差异文档汇编 + 收口 RELEASE
