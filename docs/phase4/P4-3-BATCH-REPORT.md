# 批次验收报告：P4-3（意图纯函数、五权重打分与 knowledge/rtis 源）

- 批次：P4-3（第 4/7 批 = 设计稿 CONTEXT-BROKER-DESIGN §11 M4.2；2026-11-10）
- 修改范围：crates/context-broker（intent.rs + score.rs + sources/knowledge.rs + sources/rtis.rs + source.rs 候选扩两字段 + error.rs 无改动 + lib.rs/Cargo.toml 登记）、rulepacks/rip-rules/intent-families.toml（新增，版本化映射表）、tests/intent_pure.rs + tests/rtis_source.rs（新增）、docs/phase4/（本报告）
- 依赖面变更：context-broker 主依赖加 resource-engine / search-engine / audit-engine 三内部件（PHASE4-DESIGN §5.1 总则 2 复用优先的既定面；P4-1 冻结五内部依赖至此集齐，第三方仍为 P4-2 的 toml 一项，本批零新增）
- 新增接口：IntentConfig::from_toml/from_dir、classify()、detect_slots()、emit_intent_event()、ScoreWeights/CandidateMetrics、vetoes()/score_candidate()/family_of()、KnowledgeSource（SearchEngine 直连）、RtisSource（search_with_status 三态 + get_poc）、RtisStatus 三态枚举
- 数据变更：测试载具内存库；生产 platform*.db 零写入（红线，git status 无任何 *.db 行）；RTIS 本体零改动（红线，git diff 无 integrations/rtis 任何行）

## 判据逐条（P4-3 批次 ask）

1. **归一化复用 resource-engine，不重写分词**：intent.rs 与 score.rs 全部走 `resource_engine::tokenizer::{tokenize, expand_keywords, keyword_hits}`（tokenizer.rs:25/43/68 公开函数），Broker 内零分词代码（词元化仅一处调用点/模块）。
2. **knowledge 源走 SearchEngine::search，不重写 FTS5**：sources/knowledge.rs 唯一检索入口 `SearchEngine::search`（search-engine/src/query.rs:66），索引构建用 `SearchIndex::build`，零 SQL 拼接、零索引库直读；治理轴兜底依赖引擎查询面同款域约束（query.rs:268 `governance NOT IN ('QUARANTINED','REJECTED','ARCHIVED')`），score 层 `vetoes()` 二次把关。
3. **「kingdee HTTP 检查」类任务 rtis 只读命中（M4.2 判据）**：真实 kit 测试（rtis_real_kit_returns_readonly_hits_for_kingdee_task）走 `python3 -I rt.py poc search --q kingdee --limit 2 --json` 链路，实测命中 ≥1 且 `rtis://poc/<id>` 引用齐全、poc get 全文展开可用；本会话命令行同款实测：`rt.py poc search --q kingdee --limit 2` 返回 900008（K3Cloud 差分脚本）与 15630（nuclei EAS LFI）两条，退出码 0。
4. **治理轴一票否决有拒绝用例**：QUARANTINED/REJECTED/ARCHIVED 三态 `vetoes` 全拒 + 「满分候选带 QUARANTINED 仍被否决」用例；knowledge 源集成用例实建索引含 QUARANTINED 高质量文档（95/A），断言其不出现在命中集（引擎面 + 打分面双层把守）。
5. **意图事件带 correlation_id 走 audit-engine emit**：emit_intent_event 走 `emit_best_effort`（audit-engine/src/lib.rs:45 emit 与旁路面，设计稿 §2 第 7 条写失败不影响装配）；测试经内存库迁移后落库断言 correlation_id/action/detail 齐全，且无表场景返回 false 不炸调用方。
6. **rtis 超时与失败语义实测（设计稿 §13.6）**：假 kit 载具三类实测：慢脚本（5s sleep + 300ms 超时）恰好两次尝试后报 Unavailable（attempts 计数文件实测恰 2 行，eri_adapter.py「失败单次重试」口径）、非零退出（exit 3）重试后 Unavailable、坏 JSON 重试后 Unavailable；两次失败即空集返回（设计稿 §9.2 第 4 条空结果合法）。
7. **knowledge.db poc 表 14717 条（两处口径差裁定）**：本会话 `python3 -I` mode=ro 实查 `select count(*) from poc` = 14717（README 口径 14717 为准，rtis.py 头注 14664 为过时口径），作为「不默认塞入」测试规模口径（P4-6 使用）。
8. **与 P4-2 可并行**：本批零改动 P4-2 交付面语义（source.rs 候选结构体只增两可选字段 governance/quality_grade，P4-2 测试同步补字段编译，行为不变）；P4-4 起消费本批 intent/score 输出。

## 执行中修正记录（如实声明）

1. **rtis 查询策略实测改定**：`--q "kingdee http 检查"` 与 `--q "kingdee http"` 实测均为 0 命中（rt.py FTS5 对整串多词与 CJK 混排不命中），单词 kingdee 实测 2 命中。Adapter 定稿为「resource-engine tokenize 词元逐查合并」（至多 3 词元、按 item_id 去重、部分可用记 Ok、全部不可用才 Unavailable），M4.2 判据全链（classify → candidates）实测命中。
2. **intent 配置解析**：族 id 强制过 ToolFamily::parse（十三族拼写漂移即拒绝）；触发词 ASCII 走词元匹配、非 ASCII 走子串匹配（CJK 不入 tokenizer 词元集）。
3. **clippy 四处**：too_many_arguments（无，本批为 Copy 于含 String 结构体移除、bool as f64 改 if、两处 Candidate 字面量补字段、unused_mut/unused_import 测试清理），语义不变。
4. **既有测试竞态发现（非本批引入，如实记录）**：crates/tool-engine/src/adapter/egress_proxy.rs:680 的 proxy_tunnels_allowlisted_and_rejects_others 在全 workspace 并行负载下偶发 `index out of bounds: the len is 2 but the index is 2`（第三次 CONNECT 的拒绝事件落 events 列表与 403 回包读取存在竞态）。本批对 tool-engine 零改动（git diff 0 行），隔离复跑 5/5 全过、全 workspace 复跑全绿；属 P3-2 交付测试的既有低概率竞态，修复归属 tool-engine 侧（批次外，不越界顺带修）。

## 测试命令与结果（实测）

- `cargo test --offline -p context-broker`：36 passed 0 failed（lib 1 + intent_pure 11 + policy_estimate 13 + catalog_sources 5 + rtis_source 6）
- `cargo test --offline -p domain -p context-broker -p database`：94 passed 0 failed
- `cargo test --offline --workspace`：248 passed 0 failed（P4-2 基线 231 + 本批 17）
- `cargo clippy --offline --workspace --all-targets -- -D warnings`：零告警，退出码 0
- `cargo fmt --all -- --check`：通过
- `grep -P '[\x{2014}\x{2013}]'`（本批全部新文件）：退出码 1（两处文档注释长横线已清除复验）
- `cd integrations/rtis && python3 -I rt.py poc search --q kingdee --limit 2`：退出码 0，命中 2 条（900008/15630）
- `python3 -I` sqlite mode=ro `select count(*) from poc`：14717

## 未执行的测试

- 向量检索轴：Phase 4 明确不实现（PHASE4-DESIGN §3 第 2 条）
- knowledge 源 L3 正文展开：P4-4 expand 记账面（load 显式拒绝并指明批次）
- rtis 源对 rt.py 多路重试的并发行为：单线程顺序调用（低并发纪律），无并发用例
- platform 真实库（migrations/platform.db 衍生索引）上的 knowledge 检索：测试用内存夹具（search-engine index.rs:385 同款模式），真实库对账随 P4-4 装配落库

## 对 ERI / RTIS / KB / Skills / Tools 的影响

- RTIS 本体零改动（红线；只读子进程调用，`python3 -I` 隔离运行）；ERI/KB 无接触；search-engine/resource-engine/audit-engine 零改动（纯公开接口消费）

## 风险

- 意图置信度常数（0.4/0.2/0.2/0.2 封顶 2 次）为规则打分落面默认值，未对真实任务分布校准（设计稿 §13.1 同源待验证，随 P4-6 校准批复核）
- rtis 逐词元查询在多词元任务下最多 3 次子进程调用：顺序执行 + 单次 5s 超时 + 失败重试上限，最坏路径约 3 x (2 x 5s + 1s) = 33s，P4-4 装配面需把 rtis 源调用置于后台或降低词元上限（落 P4-4 设定，本批在 candidates 文档注明）
- egress_proxy 既有测试竞态（§执行中修正 4）：全量门在并行负载下有小概率红，修复归属 tool-engine 侧独立加固项（PHASE3-DESIGN §3.2 工单化口径）

## 回滚方式

- 代码：`git revert <批次 commit>`；intent.rs/score.rs/sources/knowledge.rs/sources/rtis.rs 为整文件新增，revert 即移除；source.rs 两字段与 P4-2 测试补丁随批回滚（回滚后 P4-2 测试恢复原字面量仍可编译）
- 依赖：三内部依赖随批回滚（workspace.dependencies 行），零 vendor 变更
- 规则包：intent-families.toml 随批回滚，不影响 rules.toml（P4-2 交付面独立）
- 生产库零写入，无生产回滚面

## 后续工作

- P4-4（M4.3）：assemble 全流程（选择/去重/驱逐）+ 三表落库 + expand 记账 + AssembledContext 契约校验；消费本批 intent/score/源候选，rtis 源调用方式（后台化或词元上限收紧）在该批设定
- P4-5（M4.4）：cache + 七类失效触发器 + rp-context CLI
