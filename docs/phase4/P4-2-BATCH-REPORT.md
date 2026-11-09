# 批次验收报告：P4-2（ContextSource 端口与目录/规则包/插件源）

- 批次：P4-2（第 3/7 批 = 设计稿 CONTEXT-BROKER-DESIGN §11 M4.1；2026-11-10）
- 修改范围：crates/context-broker（source.rs 端口 + sources/ 三源 + error.rs Source 变体 + lib.rs 登记）、rulepacks/rip-rules/（新增：RULEPACK.md + rules.toml）、Cargo.toml/Cargo.lock（toml 增量 vendor + context-broker 依赖面更新）、vendor/（toml 1.2.1 及其传递件）、docs/phase4/（本报告）
- 新增接口：ContextSource trait（candidates/load，设计稿 §10 签名）、Candidate/Content 载荷、RulepackSource（RulePack::from_toml 解析 rules.toml）、RegistrySource（tool_cards/tool_briefs/tool_briefs/tool_schema 三级说明面）、generate_brief/generate_card 纯函数生成器、PluginSource 空源、ContextBrokerError::Source 变体
- 依赖面变更（如实声明，两项均按既定纪律走）：
  1. 新增第三方依赖 toml 1.2.1+spec-1.1.0（workspace 锁定）：rules.toml 结构化解析用，选型理由 = 维护活跃的 TOML 1.1 规范实现，按复用优先纪律不手写解析器（P4-0 §5.1 总则 4「确需新依赖时先评估维护状态，选型后增量 cargo vendor 保证 --offline 构建通过」的既定路径）；增量 cargo vendor 完成，`cargo test --offline` 全程通过。
  2. 内部依赖加 tool-registry（workspace path）：P4-0 决议 2「目录源直连已交付 tool-registry」的编译落位；P4-1 冻结的五内部依赖面（domain/database/audit-engine/search-engine/resource-engine）按决议 2 扩为六，Mock 降为纯测试载具（tests 内 MockSource）。
  3. database 仅为 dev-dependency（测试载具建内存库 + 迁移），主依赖面待 P4-4 manifest store 再加。
- 数据变更：测试载具内存库（DbPath::Memory + 全链迁移 001..008）；生产 platform*.db 零写入（红线，git status 无任何 *.db 行）

## 判据逐条（P4-2 批次 ask）

1. **Registry discover() 登记面全量逐条出 tier1 card 与 tier2 brief**：测试载具在内存库登记三个目录全部 20 份 JSON Manifest（manifests/tools/ 14 份 = T01 到 T10 已登记面；containerfiles/rip-tool-webresearch/ 1 份 + containerfiles/rip-tool-coreutils/ 5 份 = Phase 3 两包 6 工具，c9e7b23 口径），九条校验全过、全量推 ENABLED 后：discover() 实查 20 条，tool_cards() 与 tool_briefs() 各 20 条一一对应；card 峰值 23 tokens（软上限 45，设计稿约 30 口径的落面），brief 峰值 90 tokens（软上限 120，设计稿 §8「约 120」）；每条 brief 必填键保留在文本内、est_tokens 与文本重算一致；tier3 full 经 load(ContentRef) 与 tool_schema() 双路取到 input/output Schema 全文。
2. **T11/T12 缓登不进目录**：三目录无 reqmcp/yakitmcp/dump_flows/TPscan 登记（T11 与 T12 按批次 ask 沿用 Phase 1 缓登口径）；目录计数以 discover() 实查 20 为准（ask 明示落库实查数为准）。
3. **规则包目录版本化**：rulepacks/rip-rules/rules.toml 携带 pack_version 1.0.0 与 policy_version policy-1；测试断言 policy_version 与 BudgetPolicy::default_for 的策略版本同源相等（设计稿 §7.2「策略变化」失效触发器的数据基础）；候选引用带 ref_version = pack_version；pack_version 变化即条目级失效、policy_version 变化即 L0 与预算相关缓存整体失效（信号读取口 RulepackSource::pack()）。
4. **与 P4-3 可并行**：本批零改动 P4-3 的写面（intent.rs/knowledge 源/rtis 源尚未创建），source.rs 端口为 P4-3 的消费接口（打分与 matched_signals 属 P4-3，源保持中性候选）。

## 执行中修正记录（如实声明）

1. **brief 压缩步进改序**：初版先截摘要后收敛键名列表，构造用例（600 CJK 摘要 + 30 必填键 + 40 输出键）下摘要被整体清空（键名列表吃满预算），且裸前缀截断比省略号变体多容纳半个 token 导致无标记截断。改为：步进 1 键名列表收敛到 8 键（摘要信息主位不动），步进 2 截摘要且截断必带省略号，步进 3 列表收敛 4 键，步进 4 去限制行，兜底全文前缀截断。修正后压缩用例实测：compressed=true、summary 带省略号、est 90 ≤ 120。
2. **复合外键无涉**；008 迁移本批零改动。
3. **clippy 三处**：too_many_arguments（finish_brief 八参拆 BriefParts 载荷结构）、manual_str_repeat/manual_repeat_n（测试构造改 "长".repeat(600)），语义不变。

## 测试命令与结果（实测）

- `cargo test --offline -p context-broker`：19 passed 0 failed（lib 1 + catalog_sources 5 + policy_estimate 13）
- `cargo test --offline -p domain -p context-broker -p database`：77 passed 0 failed
- `cargo test --offline --workspace`：231 passed 0 failed（P4-1 基线 226 + 本批 5）
- `cargo clippy --offline --workspace --all-targets -- -D warnings`：零告警，退出码 0
- `cargo fmt --all -- --check`：通过
- `grep -P '[\x{2014}\x{2013}]'`（本批全部新文件）：退出码 1，零长横线

## 未执行的测试

- knowledge/rtis 只读源与打分、matched_signals：P4-3 批次（M4.2 判据「kingdee HTTP 检查」类任务冒烟届时落）
- RTIS 子进程真实调用：P4-3（本批零 RTIS 接触，红线不变）
- catalog_sources 用例未触真实开发库副本（dev-p4.db 等）：载具用内存库，登记面为 manifests 双份文件全量，与库副本登记状态无关；真实库对账在 P4-4 装配落库时做

## 对 ERI / RTIS / KB / Skills / Tools 的影响

- RTIS 本体零改动（红线）；ERI/KB 无接触；Tool Registry 登记链路与数据面零改动（P4-0 决议 3 前提维持，context-broker 只读其公开接口 + 文件面登记数据）；新增 rulepacks/ 为随仓库分发的静态资产（L0 规则包，设计稿 §3 L0 行「随仓库分发的规则包目录」）

## 风险

- toml 1.2.1 为 workspace 新增第三方件：已 vendor、--offline 全程通过；升级走 workspace 版本号统一变更并重新 vendor
- ManifestIndex 与库面状态可能漂移（文件面 description/family 与库行来自两次登记）：对账口径以库面 discover() 为准，索引缺失时 family=unknown、description 空串降级（不失败）；P4-4 装配落库时加对账断言
- brief 软上限 120 tokens 与 L2 工具分项 10% 余量配额的匹配在 P4-4 预算闸复核（本批只保证单条 ≤ 120，不分项总量）

## 回滚方式

- 代码：`git revert <批次 commit>`；sources/ 与 source.rs 为整目录新增，revert 即移除；error.rs 的 Source 变体与 lib.rs 登记随批回滚，无既有调用方
- 依赖：toml 从 workspace.dependencies 与 vendor/ 随批回滚（Cargo.lock 同步还原）
- 规则包：rulepacks/ 整目录随批回滚；rules.toml 变更纪律（id 只增不改）使历史 Manifest 不受回滚影响
- 生产库零写入，无生产回滚面

## 后续工作

- P4-3（M4.2）：intent 纯函数 + knowledge/rtis 只读源 + 打分与 matched_signals（「kingdee HTTP 检查」rtis 冒烟判据）
- P4-4（M4.3）：assemble 全流程 + 三表落库 + expand 记账 + AssembledContext 契约校验（catalog_sources 的 20 工具面即装配输入）
