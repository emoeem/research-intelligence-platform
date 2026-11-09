# Phase 4 阶段验收（Context Broker，P4-6 收口）

- 日期：2026-11-10
- 执行基准：计划 §14 Phase 4（786 到 794 行）验收三条；ROADMAP §5.4 验收命令；设计稿 §11 M4.5 判据与质量门（既有 CI rust job，不新增独立标准）；PHASE4-DESIGN §6 批次表
- 批次覆盖：P4-0（设计冻结）到 P4-6（本验收），七批全部交付；实施差异见 [PHASE4-IMPLEMENTATION-DIFF.md](PHASE4-IMPLEMENTATION-DIFF.md)，逐批证据见 P4-1 到 P4-6 批次验收报告

## 一、验收三条达成（计划 §14:794 原文 → 落证）

1. **不默认把整个工具目录或知识库正文塞入上下文**：tests/acceptance_no_stuffing.rs，登记全量 20 工具面（T01 到 T10 的 14 + Phase 3 两包 6，manifests 双份目录九条校验全过）+ RTIS 全库量级 14717 候选（poc 表实查口径），装配后 used 分项逐项 ≤ allocated 且总和 ≤ 总量、L1 单项卡片、L2 条目数有界（≪ 14717）、序列化面零 input_schema 与零正文载荷。ROADMAP §5.4 第 3 条对账命令同向印证（见 §三）。
2. **可解释选择了哪些知识和工具、为什么选择**：tests/acceptance_explainable.rs，每个 Manifest item 的 reason 与 matched_signals 非空且同源（reason = 信号原样拼接），L2 条目信号可解释到词元级命中（kw:kingdee），三表行数与装配报告一致（items = citations = ASSEMBLED 事件数），selection_hash 可重算对账。
3. **工具输出通过 Artifact 引用进入上下文**：tests/acceptance_artifact_ref.rs，005 数据面 Mock Run 产出 Artifact 后上下文仅引用与哈希（sha256 进 citations，run_id 与正文零内联），item 形态 ref-only，全文经 expand 记账取得（l3_reserved 扣账），EXPANDED 事件落 context_ref_log。

## 二、质量门（设计稿 §11：进既有 CI，不新增独立标准）

| 门 | 命令 | 结果（实测 2026-11-10） |
|---|---|---|
| 测试 | `cargo test --offline --workspace` | 271 全绿 0 失败（P4-5 基线 268 + 本批 3 验收测试） |
| 静态检查 | `cargo clippy --offline --workspace --all-targets -- -D warnings` | 零告警，退出码 0 |
| 格式 | `cargo fmt --all -- --check` | 通过 |
| Worker 测试 | `python3 -m unittest discover -s tests -v`（workers/python） | 未跑（本阶段零 Python 改动；Phase 3 收口基线 125 全绿仍有效） |

三验收测试随 crates/ 下测试自动纳入既有 CI rust job（cargo test --workspace），无新增工作流或独立标准。

## 三、ROADMAP §5.4 验收命令实测（2026-11-10，仓库根）

第 1 组（质量门）：见 §二。

第 2 组（装配端到端，P4-5 实测并于本批复核）：

~~~text
cargo run --offline -p context-broker --bin rp-context -- assemble --task-id t-acceptance
→ 退出码 0；签发 ctxm-7f854617bc809a9ce6e54f3a（intent ok / L0 七规则 / L1 计数 / citations 齐全；P4-5 首测签发 ctxm-14c4b1316d5ebe159bb33da5，两轮均通）
cargo run --offline -p context-broker --bin rp-context -- manifest-show --manifest-id ctxm-7f854617bc809a9ce6e54f3a
→ 退出码 0；items 含 catalog:digest（catalog-card）与 rp-rules:*（summary，reason 非空）
cargo run --offline -p context-broker --bin rp-context -- cache-stats
→ 退出码 0；{entries:1, bytes:437}（expand 载荷缓存落盘）
cargo run --offline -p context-broker --bin rp-context -- expand --manifest-id ctxm-... --item-id rp-rules:on-demand-loading
→ 退出码 0；granted_tokens=55、remaining_l3=1997（l3_reserved 扣账，P4-5 实测口径）
~~~

第 3 组（验收第 1 条「不默认塞入」对账，开发库副本 mode=ro）：

~~~text
python3 -I -c "import sqlite3; c=sqlite3.connect('file:migrations/dev-p4.db?mode=ro',uri=True); print(c.execute('select layer,kind,sum(est_tokens) from context_manifest_item group by layer,kind').fetchall())"
→ [('L0','rule',328), ('L1','doc',22)]
→ 无 L3 正文内联行；L1 为目录卡片；分项和（350）远小于分项预算（开发库副本在测试间被重置重建，CLI 装配为当前全量内容：P4-5 首测含 registry brief 时为 656/38/24/6，两轮口径一致）
~~~

第 4 组（验收第 3 条「Artifact 引用」对账）：

~~~text
python3 -I -c "import sqlite3; c=sqlite3.connect('file:migrations/dev-p4.db?mode=ro',uri=True); print(c.execute('select event,count(*) from context_ref_log group by event').fetchall())"
→ [('ASSEMBLED', 8)]
→ CITED_IN_TOOL_CALL 事件属 Phase 7 工具调用接线（调用对象 context_refs，计划:219）；Phase 4 落证为验收三测试的 EXPANDED/ASSEMBLED 事件与 artifact ref-only 条目（acceptance_artifact_ref.rs，其内存库实测 EXPANDED 事件），引用指向 artifact（sha256 进 citations）
~~~

## 四、est_tokens 校准（ROADMAP §5.5 风险 1 收口）

- 命令：`python3 -I scripts/calibrate_est_tokens.py`（只读 models/bge-small-zh-v1.5/tokenizer.json，四文件 ls 实测在位；stdlib only，宿主零安装）
- 结果：八类样本（规则正文/工具 brief/意图查询/ascii 标识与路径/标题）est/bge 中位数 0.9333，容差 [0.8, 1.25] → 中位数口径 verdict keep，常数不修
- 留痕：docs/phase4/est-tokens-calibration.json（同批提交）
- **样本级越界如实记录（2026-11-10 修复轮校正）**：brief-mixed 样本 0.7536 低于容差下限 0.8，est 对 brief 形态低估约 25%，L2 brief 预算按 est 计可能被真实 tokenization 超出；纯 ascii 标识符/路径类样本低估约 2 倍（bge 对 id/路径切分远细于 4:1）。CJK 规则正文与查询类样本 0.93 到 1.0 在容差内。**brief 形态与重 id 面列入 Phase 7 校准范围**（按形态分桶修正常数或换精确计数服务，差异文档 D-12）

## 五、宿主零安装对照（红线复核）

- `pacman -Q | wc -l` = 2415（与 Phase 3 收口基线一致，全程 diff 为空）
- 本阶段新增工具链依赖为零（Python 脚本 stdlib only；cargo 依赖增量仅 toml/serde derive/database 升格，全部走既有 vendored 面）

## 六、交付物清单（P4-0 到 P4-6）

- **代码**：crates/context-broker（policy/estimate/error/source/intent/score/cache/assemble/manifest + sources 六源 + rp-context CLI）；crates/domain context 模块；migrations/008_context_broker.sql（三表）；rulepacks/rip-rules/（RULEPACK.md + rules.toml + intent-families.toml）；scripts/calibrate_est_tokens.py
- **文档**：PHASE4-DESIGN.md（P4-0）、P4-1 到 P4-6 批次验收报告、PHASE4-IMPLEMENTATION-DIFF.md、本验收、docs/releases/RELEASE_2026-11-10_PHASE4.md
- **测试**：56 个 context-broker 用例（含三验收测试）+ domain/database 增量，全数进既有 CI rust job
- **数据**：生产 platform*.db 零写入（红线全程）；开发库副本 dev-p4.db 与测试内存库承载全部库操作；缓存目录为运行时数据不入库

## 七、遗留与移交（含 2026-11-10 验收意见修复轮处置）

1. **CITED_IN_TOOL_CALL / CITED_IN_EVIDENCE（Phase 7 验收清单项）**：验收方裁定接受 Phase 7 边界（ROADMAP §5.4 第 4 条期望已同步修订，2026-11-10 注记）。Phase 7 验收须含：工具调用以 context_refs 填 (manifest_id, item_id) 并发射 CITED_IN_TOOL_CALL（存储面已就绪：RefEvent::CitedInToolCall 在 008 CHECK 域内，ManifestStore::append_ref_event 直接可用，缺的只是调用方）；Evidence provenance 回链发射 CITED_IN_EVIDENCE（设计稿 §6.2 两跳）。
2. past_success 权重校准：首批 Run 数据积累后回看（差异文档 D-5）
3. 向量检索轴：按边界不实现，预留接口位（设计稿 §13.5）
4. memory_history 分项：Phase 7 任务记忆接入（分项预算已留位）
5. knowledge/rtis 源的 CLI 接线与 rtis 检索缓存（10 分钟 TTL 常量已备）：随 Phase 7 Agent Engine 消费场景定参
6. **platform-kb 源 L3 load 移交（验收意见修复轮新增）**：sources/knowledge.rs load 现显式拒绝（原注「P4-4 expand 记账面」口径不准确，已改）。根因是数据面现实：platform 库 resource 表无正文载荷列（name/description/canonical_url 即全部，索引库同构），L3「全文」在 Phase 4 数据面不存在。Phase 7 定义知识正文存储（knowledge 表全文列或与 RTIS poc 全文合流）后接 resource 读路径；rtis 源 poc get 不受此限（已可用）。差异文档 D-13。
7. **BrokerCache 容量卫生（Phase 7 生产化前）**：现实现无淘汰与容量上限（过期条目永留磁盘、stats 只增）；生产化前加容量上限/清扫（TTL 过期清扫或 LRU），或评估换用维护中的内容寻址缓存库（读时校验语义层保留平台自定义，设计稿 §7.1）。差异文档 D-14。
8. **转办（非本阶段问题）**：crates/tool-engine/src/adapter/egress_proxy.rs:680 proxy_tunnels_allowlisted_and_rejects_others 在全 workspace 并行负载下偶发越界（拒绝事件落列与 403 回包读取竞态；Phase 4 对 tool-engine 零改动，隔离复跑稳定），列入 tool-engine 独立加固项（PHASE3-DESIGN §3.2 工单化口径）。
