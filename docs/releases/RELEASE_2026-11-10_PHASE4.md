# 更新说明 · 2026-11-10（RIP 2.0 Phase 4：Context Broker）

> Agent 与知识库、工具库之间的唯一上下文装配层落地：L0 到 L3 四级加载、意图识别与能力匹配、分项 Token 预算与降级驱逐、ContextManifest 三表留痕与引用追踪、内容寻址缓存与七类失效触发器、rp-context 验收 CLI。验收三条（计划 §14:794）全部有测试落证：不默认塞入、可解释选择、Artifact 引用进入上下文。

| 项         | 值                                                  |
| ---------- | --------------------------------------------------- |
| 阶段交付   | 本发布提交（批次 P4-0 到 P4-6 全部交付物一次入库；前序基线 c9e7b23） |
| 数据库迁移 | 008_context_broker.sql（context_manifest / context_manifest_item / context_ref_log 三表，编号按 ROADMAP §1.4 先到先得取 008，设计稿草案号 005 让位；幂等 DDL，只在开发库副本与测试内存库执行，生产 platform*.db 零写入） |
| 自动化测试 | cargo test `--offline --workspace` 收口复测 passed=273 failed=0 ignored=0（验收记录落笔快照 271：P4-5 基线 268 + 验收三测试 3；复测多 2 为快照落笔后的追加用例，以收口时点实测 273 入档）；worker unittest 实测 Ran 125 tests OK（skipped=11）；含验收三测试（acceptance_no_stuffing / acceptance_explainable / acceptance_artifact_ref）随 crates/ 测试自动进既有 CI rust job |
| 静态检查   | cargo clippy `-D warnings` 零告警；cargo fmt --check 通过 |
| 宿主安装   | pacman -Q 基线 2415 与 Phase 3 收口一致（零安装红线全程）；新增依赖仅 toml（cargo vendor 后 --offline 构建通过） |

## 一、面向使用者的变化

1. **新 crate crates/context-broker**：Broker::assemble 签发 AssembledContext（四级层结构 + citations 权威引用集 + budget_report + selection_hash），Broker::expand 按需展开并从 l3_reserved 扣账记账，Broker::manifest 清单视图；ContextSource 端口下六源就绪：rulepack（L0 规则包）、registry（L1 目录直连 tool-registry，Mock 降为测试载具）、knowledge（platform 知识面经 search-engine）、rtis（只读子进程 Adapter，5s 超时单次重试）、artifact（005 产物只读引用源）、plugin（Phase 6 前空实现）。
2. **确定性意图面**：IntentConfig 版本化配置（rulepacks/rip-rules/intent-families.toml，计划 §4.2 十三族触发词映射非硬编码），classify 纯函数（归一化复用 resource-engine tokenizer、槽位检测、低置信 ambiguous 回退附 top3 候选族）；意图事件带 correlation_id 走 audit-engine 旁路发射。
3. **预算与可解释**：BudgetPolicy 七分项（默认分配逐项等于设计稿 §9.1 示例值）、五权重打分（0.40/0.25/0.20/0.10/0.05）、一票否决（治理轴 QUARANTINED/REJECTED/ARCHIVED）、同 content_hash 去重、降级链（summary → ref-only 不删 item）、驱逐低分先逐且 L0/L1 永不驱逐；每个条目 reason 与 matched_signals 非空。
4. **缓存**：BrokerCache 内容寻址（sha256 键，tmp/rename 原子写，worker 先例同构），validation 三元组读时比对 + mtime TTL 兜底，失败不落缓存无负缓存；七类失效触发器逐条测试（工具 version/digest、治理跃迁、INDEX_SCHEMA_VERSION、knowledge.db mtime+size、插件状态、policy_version、broker_version）；brief 缓存命中零生成，版本号变更后 cache-stats 显示重建且旧条目不失效。
5. **CLI**：rp-context 四子命令（assemble / expand / manifest-show / cache-stats），对开发库副本跑通（ROADMAP §5.4 第 2 组命令形式）；est_tokens 校准脚本 scripts/calibrate_est_tokens.py（python3 -I 只读 bge tokenizer，est/bge 中位数 0.9333 容差内常数保持，留痕 est-tokens-calibration.json）。
6. **L0 规则包**：rulepacks/rip-rules/（RULEPACK.md + rules.toml 七条规则，policy_version 与预算策略同源；intent-families.toml 版本化意图配置），规则 id 只增不改的变更纪律随 RULEPACK.md 落盘。

## 二、修复与根因

1. 复合外键列级声明被 SQLite 拒（008 ref_log 对 item 的两列父键）：改表级 FOREIGN KEY 声明，双外键 + CASCADE 语义不变（P4-1）。
2. brief 压缩步进初版先截摘要后被键名列表吃满预算且无截断标记：改为键名列表先收敛、截断必带省略号，压缩路径有专项用例（P4-2）。
3. rt.py --q 对整串多词与 CJK 混排实测 0 命中：rtis Adapter 定稿为词元逐查合并去重（至多 3 词元），「kingdee HTTP 检查」类任务全链实测命中（P4-3）。
4. 装配期溢出候选写 EVICTED 事件被 ref_log 复合外键拒：溢出记录落 budget_report.evicted 与 totals_json 快照，EVICTED 事件留给运行期 L3 收回（P4-4）。
5. 契约校验口径落定为整体序列化 vs 总预算（fail fast），连带小总量策略需容纳 JSON 结构开销，已记入实施差异文档 D-7（P4-4）。

## 三、已知限制与待验证（全量见 docs/phase4/PHASE4-IMPLEMENTATION-DIFF.md）

1. CITED_IN_TOOL_CALL / CITED_IN_EVIDENCE 事件属 Phase 7 工具调用与 Evidence provenance 接线（引用追踪闭环下游两跳）。
2. past_success 权重恒 0（tool_execution 0 行），首批 Run 数据积累后校准。
3. est_tokens 校准样本级越界如实记录：brief 形态样本 0.7536 低于容差下限 0.8（est 低估约 25%，L2 brief 预算按 est 计可能被真实 tokenization 超出）、纯 ascii 标识符/路径类低估约 2 倍（bge 切分远细于 4:1）；中位数口径 0.9333 常数保持，brief 形态与重 id 面列入 Phase 7 校准范围（差异文档 D-12）。
4. 小总量策略（约 600 tokens 以下）被整体序列化契约闸拒绝（结构开销），context_policy 下限设定随 Phase 7。
5. 向量检索轴按边界不实现；memory_history 分项留位待 Phase 7 任务记忆。
6. TTL 过期路径未直接实测时钟推进（mtime 兜底与坏条目路径同构）；分层 TTL 常量为设计稿默认值。
7. tool-engine egress_proxy 既有测试在全 workspace 并行负载下偶发竞态（P3-2 交付面，Phase 4 零改动），隔离复跑稳定，修复归属 tool-engine 独立加固项。

## 四、红线遵守

- 生产 RTIS 行为零改动（只读子进程 Adapter，git diff integrations/rtis 零行）；M0 到 M8 交付物未推翻
- migrations/platform.db 与 platform-dryrun.db 全程零写入（git status 无任何 *.db 行），库操作走开发库副本 dev-p4.db 与测试内存库
- 宿主机零安装（pacman 基线 2415 全程不变）；Tool Registry 登记链路零改动

## 五、文档索引

- 阶段说明与批次表：docs/phase4/PHASE4-DESIGN.md（P4-0）
- 批次验收报告：docs/phase4/P4-1 到 P4-6-BATCH-REPORT.md
- 阶段验收：docs/phase4/PHASE4-ACCEPTANCE.md（验收三条达成 + ROADMAP §5.4 四组命令实测）
- 实施差异文档：docs/phase4/PHASE4-IMPLEMENTATION-DIFF.md（D-1 到 D-12 + 待验证处置）
- 校准留痕：docs/phase4/est-tokens-calibration.json
