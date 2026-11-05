# M4 交付报告：Unified Search & Data Governance

日期：2026-11-06　执行：RIP M4 实施会话　范围：research-platform/ 下新增与修改
上游依据：SESSION-HANDOFF.md（第三节序列）、MODULE_BOUNDARIES.md §8.1/§8.2/§8.3/§8.4、M2_REPORT.md（差异表）、ERI 参照实现（security-research-upgrade/eri/eri_p1.py、eri_p2.py）

## 一、交付清单

| 交付物 | 路径 | 说明 |
|---|---|---|
| search-engine crate | `crates/search-engine/`（8 源文件 + rp-search-index CLI，17 单测） | FTS5 派生索引层 + `SearchQuery(keywords, filters, limit, offset) -> Hits` + 混合排序。加入 workspace members |
| FTS5 索引层 | `crates/search-engine/src/index.rs` | 独立索引库（落地裁定见 §二）。三张派生表全部 `search_index_` 前缀、DDL 头注标注"可重建"：`search_index_doc`（元数据快照）、`search_index_fts`（FTS5 contentless 虚表，bm25 可用）、`search_index_meta`（构建元信息）。doc 与 fts 行同 rowid 对齐 |
| 索引数据源 | index.rs `load_docs` | 双源只读折叠：platform 库（`resource` 正文/质量/双轴 + `resource_source` 的 alias/name 即 source_section/source_repository 并入 aliases 列）+ production catalog 库（`github_metadata` 的 topics/language/license_status/maintenance_status，补齐 M2_REPORT 差异表 #2 的 topics 缺口）。license/maintenance 缺证归一 UNKNOWN（与 eri 输出口径一致） |
| 查询模型 | `crates/search-engine/src/model.rs` | `SearchQuery`（keywords + filters{grade, license_status, maintenance_status, resource_type} + limit + offset）与 `Hits`（每命中含 score、fts_score、keyword_coverage、可解释 reasons、来源标注 source_annotations） |
| 混合排序 | `crates/search-engine/src/weights.rs` + `query.rs` | FTS bm25 50% + quality_score 归一 30% + 六轴中可迁移的 keyword 覆盖 20%；权重常量集中于 weights.rs，注释"初始可调，黄金集校准待引入"。缺证轴剔除后归一（eri 口径）；bm25 做结果集内相对归一（|bm25_i|/max|bm25|，确定性、单调、有界） |
| 分词 | `crates/search-engine/src/tokenizer.rs` | ASCII 沿用 eri 口径（复用 resource-engine::tokenizer 的 ALIASES 与词元函数，单一同步源，不跨模块读库）；中文 2-gram 预分词入库；CJK_BRIDGE 查表同步自 eri_p2.py（eri 实况核对结论见 §六） |
| quality-engine crate | `crates/quality-engine/`（5 源文件 + rp-review CLI，12 单测） | 数据治理独立 crate。归属裁定与理由见 §三 |
| 生命周期状态机 | `crates/quality-engine/src/state.rs` | 表驱动：DISCOVERED→FETCHED→PARSED→CANDIDATE→VERIFIED→INDEXED / STALE（+STALE→FETCHED 重扫循环），跳段/回退/同态自跃迁一律拒绝；治理轴 ACTIVE→{QUARANTINED,REJECTED,ARCHIVED}、QUARANTINED→{ACTIVE,REJECTED}、REJECTED/ARCHIVED→ACTIVE（复议恢复），与 lifecycle 正交（K-08 口径） |
| VERIFIED 守卫 | `crates/quality-engine/src/governor.rs` | `update_resource`（属性/内容更新路径）对 lifecycle=VERIFIED 行返回 `NeedReview` 错误，拒绝先于任何写入；生命周期推进 VERIFIED→INDEXED 为状态机合法跃迁（索引流水线，不改动已验证内容，口径说明见 §三） |
| Review 候选队列 | `crates/quality-engine/src/review.rs` | `Propose/Decide`（§8.4 稳定接口）：从 migrations/quarantine/*.jsonl 读候选（d5_candidates.jsonl 68 条 + assets_isolation.jsonl 2 条），`propose` 幂等入队（(source_system, source_pk) 冲突即跳过，decision=PENDING），`decide` 写决议到 review 表（与 platform review 表同形，便于迁移期对账）。decide 只记录决议不自动改资源实体（M1 口径：D5 不自动合并或删除） |
| CLI | `crates/search-engine/src/bin/rp-search-index.rs`、`crates/quality-engine/src/bin/rp-review.rs` | 索引 build/query 与 review propose/decide/pending/counts 的命令行投影 |
| M4 parity 测试 | `tests/tests/m4_search_parity.rs`（4 测试）+ tests/src/lib.rs 扩展 | 3 组任务指定查询 + 1 组 CJK 桥接组（渗透），与 eri_p2.resolve 双轨对照 |
| M4 governance 集成测试 | `tests/tests/m4_governance.rs`（2 测试） | 真实 001 迁移 schema 上的状态机/守卫全链路 + 真实隔离清单入队/决议 |
| 派生索引库 | `migrations/platform-search.db`（新增文件） | 真实 81 资源索引（见 §四） |
| 治理库 | `migrations/platform-governance.db`（新增文件） | 真实隔离清单 review 队列（见 §五） |

合计新增 Rust 源 14 文件 3271 行。migrations/ 既有文件零修改（只读连接核验见 §七）。

## 二、索引库落地裁定（任务口径二选一）

选**独立索引库** `migrations/platform-search.db`，不在 platform-dryrun.db 内建 search_index 前缀派生表：

1. platform-dryrun.db 是 M1 ETL 的对账产物（verify.py 100% 红线），在其内建表会改 schema 并复杂化复核；本次实测其 md5 全程不变。
2. MODULE_BOUNDARIES §8.2.3 可重建纪律的最强形态：整个索引库可删可重灌，写面隔离（rp-search-index 对 platform/catalog 只读打开，对索引库可写）。
3. contentless FTS5 不支持增量删除，与"整库重建"策略一致（更新 = 重建，实测 81 文档重建 132ms，规模可接受；更大规模再评估 external content 表）。

## 三、governance 归属裁定（任务口径二选一）

选**独立 crates/quality-engine**，不并入 search-engine 的 governance 模块：

1. MODULE_BOUNDARIES §8.1 把"评分/隔离/License/D5 审核"与 scores + reviews 数据面划给 quality-engine；§8.4 把 `Propose(Review)/Decide(review_id, decision)` 列为其稳定接口。
2. search-engine 的数据面是"全部可重建"的派生索引（只读检索面），治理是写路径（状态机跃迁、review 决议落库）。并入会把索引重建流程与治理写路径耦合，违背 §8.2.3 的模块语义。
3. §8.3 映射表：eri.py/eri_p1/eri_p2 → resource-engine + quality-engine + 部分 search，治理归属明确。

**VERIFIED 守卫口径说明（如实标注）**：任务原文"任何更新路径对 VERIFIED 行返回 NeedReview 错误"。实现为：`Governor::update_resource`（属性/内容更新，AI 富化与自动管道的更新入口）对 VERIFIED 行一律 NeedReview；生命周期推进 `VERIFIED→INDEXED` 为状态机表合法跃迁（不视为"覆盖"：不改已验证内容，否则索引稳态不可达，与真实库 81 行 INDEXED 已过 VERIFIED 的事实矛盾）；治理轴隔离/拒绝与生命周期正交（K-08），不受守卫约束（隔离 VERIFIED 资源是合法治理动作）。守卫拒绝先于任何写入（测试断言内容未被部分覆盖）。

## 四、真实索引运行（verified）

命令：`rp-search-index build --platform-db migrations/platform-dryrun.db --catalog-db ../security-research-upgrade/eri/db/catalog.db --index-db migrations/platform-search.db --rebuild`

```json
{
  "doc_count": 81,
  "fts_rows": 81,
  "rebuilt": true,
  "schema_version": 1,
  "elapsed_ms": 132.16
}
```

索引行数：**81 文档 / 81 FTS 行**（1:1，rowid 对齐）。license/maintenance：56 IDENTIFIED / 16 NO_LICENSE_FILE / 7 FILE_UNIDENTIFIED / 2 UNKNOWN（无元数据行）；topics 67 条折叠入索引。

示例查询输出（`rp-search-index query --index-db migrations/platform-search.db --keywords "payload,bypass" --limit 2`，节选首条）：

```json
{
  "query_id": "srq-20261105194310773208-cd3746",
  "total_matched": 3,
  "search_status": "ok",
  "weights": {"fts_bm25": 0.5, "quality": 0.3, "keyword_coverage": 0.2},
  "hits[0]": {
    "resource_id": "gh:swisskyrepo/payloadsallthethings",
    "score": 0.979, "fts_score": 1.0, "keyword_coverage": 1.0,
    "keyword_matches": ["bypass", "payload"],
    "quality_final_score": 93.0, "quality_grade": "A",
    "license_status": "IDENTIFIED", "maintenance_status": "ACTIVE",
    "topics": ["bounty", "bugbounty", "bypass", "...共17个"],
    "source_sections": ["Other Useful Repositories"],
    "reasons": ["fts bm25 (normalized): 1.0000", "quality 93.0 (A)", "keyword hits: [bypass, payload]"],
    "source_annotations": ["platform:resource", "platform:resource_source(sections:1)", "catalog:github_metadata"]
  }
}
```

CJK 查询（`--keywords 渗透`）：total_matched=9，经 CJK_BRIDGE 桥接（渗透→pentest）跨语言召回，top-1 为 gh:swisskyrepo/payloadsallthethings（eri 同）。

## 五、治理真实数据 smoke（verified）

```
rp-review propose --db migrations/platform-governance.db --quarantine-dir migrations/quarantine
  -> candidates: 70, inserted: 70, skipped: 0（68 d5_candidate_pair + 2 asset_isolation）
rp-review counts  -> {"PENDING": 70}
rp-review decide  --review-id "quality-engine:quarantine/assets_isolation.jsonl:1"
                    --decision KEEP_BOTH --reviewer m4-smoke-human --confidence 0.9
  -> 决议落 review 表，reviewed_at 写入
rp-review decide  --review-id "quality-engine:quarantine/d5_candidates.jsonl:1"
                    --decision NEEDS_MORE_EVIDENCE --confidence 0.3
rp-review propose（幂等重放） -> inserted: 0, skipped: 70
rp-review counts  -> {"KEEP_BOTH": 1, "NEEDS_MORE_EVIDENCE": 1, "PENDING": 68}
```

## 六、ERI 行为对照（M4 parity，verified）

方法：3 组任务指定关键词 + 1 组 CJK 桥接（渗透），eri_p2.py resolve（--limit 5 --json，production catalog 的 sqlite backup 副本上跑，ERI_DB 注入，生产库零写入）vs search-engine（in-process，索引由 platform-dryrun.db + catalog 副本现场构建于临时目录）。断言：双方命中非空、top-5 集合交集非空（硬断言）；fuzzing 组另加 M2 差异修复回归断言（topics-only 召回资源必须在 rust 命中集中）；top-1 一致性记录不作为硬断言（权重不同允许顺序差异，M2 口径延续）。

| 组 | eri top-1 | rust top-1 | top-1 一致 | 命中集合关系 |
|---|---|---|---|---|
| fuzzing | gh:enablesecurity/awesome-rtc-hacking | gh:secfigo/awesome-fuzzing | 否 | **4/4 完全一致**（仅顺序异） |
| payload,bypass | gh:swisskyrepo/payloadsallthethings | 同左 | 是 | **3/3 完全一致** |
| android | gh:ashishb/android-security-awesome | 同左 | 是 | **2/2 完全一致** |
| 渗透（CJK 桥接） | gh:swisskyrepo/payloadsallthethings | 同左 | 是 | 4/5 重叠（FTS 与 LIKE 召回面差异，见 §八 #3） |

**M2 差异修复结论（verified）**：M2_REPORT 差异表 #2 的 topics 缺口已由 catalog github_metadata.topics 折叠入索引补齐。M2 时经 topics/category 命中的两条资源（awesome-rtc-hacking、rfsec-toolkit）在 M4 全部进入 rust 命中集，fuzzing 组命中集合与 eri 完全一致（M2 时为 2/5 重合）。回归断言已固化进 m4_search_parity.rs。

**eri 2-gram 实况核对（如实标注）**：任务书要求"用 eri 的 2-gram 策略，查 eri.py 的实现方式"。核对结论：eri 全链（eri_p1.tokens / eri_p2.resolve 的 blob_toks）**没有 2-gram 实现**，其正则 `[a-z0-9+#.\-]{3,}` 把 CJK 字符整体丢弃；eri 对中文的唯一处理是 CJK_BRIDGE 查表。因此 Rust 侧分词为：ASCII 与 eri 同策略；中文按 2-gram 预分词入库（FTS5 unicode61 不切 CJK，2-gram 是其标准做法）；CJK_BRIDGE 同表移植用于查询扩展（数据表非代码）。两侧 CJK 查询行为已在渗透组对照验证一致（top-1 同）。

## 七、质量门（全部 verified）

| 门 | 命令 | 结果 |
|---|---|---|
| clippy | `cargo clippy --workspace --all-targets -- -D warnings` | 0 error 0 warning |
| fmt | `cargo fmt --all` + `-- --check` | 通过 |
| test | `cargo test --offline --workspace` | **69 通过 0 失败**：单测 57（domain 10 / database 6 / audit-engine 4 / resource-engine 8 / search-engine 17 / quality-engine 12）+ 集成 12（eri_parity 3 / m2_integration 3 / m4_search_parity 4 / m4_governance 2） |
| 依赖面 | workspace 依赖 | 无新增外部 crate（rusqlite bundled + serde + serde_json 均已在 vendor）；FTS5 经 vendored libsqlite3-sys（build.rs 带 -DSQLITE_ENABLE_FTS5）启用，离线可构建 |
| 只读纪律 | `md5sum -c` | platform-dryrun.db 与 production catalog.db 前后 md5 不变；dryrun 库无 search_index 表（独立索引库口径） |

## 八、权重与差异说明

| # | 项 | eri_p2.resolve | M4 search-engine | 影响与处置 |
|---|---|---|---|---|
| 1 | 权重 | 六轴（keyword .25/category .20/tech .15/profile .15/relation .15/quality .10） | 三轴（bm25 .50 / quality .30 / keyword 覆盖 .20），黄金集校准待引入 | top-1 顺序可异（fuzzing 组实测异）；命中集合实测一致；归一规则（缺证轴剔除）与 eri 一致 |
| 2 | bm25 归一 | 无（词元覆盖率打分） | 结果集内相对归一（|bm25_i|/max|bm25|） | 确定性、单调、有界；分值跨查询不可比（eri 分值同样跨查询不可比） |
| 3 | 召回面 | SQL LIKE 预筛（name/description/m.description/topics/canonical_url） | FTS5 MATCH（name/description/aliases/topics/language 五列） | FTS 分词后 canonical_url 不参与匹配（eri 的 LIKE 对 url 可命中）：渗透组 eri 召回 redteam-physical-tools 而 rust 未入 top-5 的根因之一；aliases 列使 rust 可经章节名召回（eri 无此面） |
| 4 | license/maintenance | github_metadata 实值 | 实值（catalog 折叠），缺证归一 UNKNOWN | 与 eri 输出语义一致；M2 差异表 #4 在 search 面已收编 |
| 5 | topics | github_metadata.topics | 同左（索引折叠） | M2 差异表 #2 已修复（见 §六） |
| 6 | 过滤面 | resolve 无 filters 输入 | filters{grade,license_status,maintenance_status,resource_type} + 浏览模式（无关键词） | search-engine 的 §8.4 Query 接口面扩展；eri 侧无可比输入维度 |
| 7 | query_id | erq-sha1[:6] | srq-FNV1a[:6]（形状同） | 前缀区分检索面与 resolve 面 |
| 8 | 排序 tie-break | (-score, -quality, id)，4 位小数 | 同 | 一致 |
| 9 | 治理可见性 | EXCLUDED_STATUSES（QUARANTINED/REJECTED/UNAVAILABLE） | governance NOT IN (QUARANTINED,REJECTED,ARCHIVED) | K-08 双轴拆分后以 governance 轴表达；UNAVAILABLE 语义在 platform 库归 lifecycle/source_status，本批未单独过滤（81 行均为 INDEXED/ACTIVE，无实测影响） |

## 九、未完成项与限制（如实标注）

verified：本报告 §四/§五/§六/§七所列全部结果均实测执行，非推断。

unverified / 未实现（后续里程碑）：
1. 黄金集校准：三轴权重（50/30/20）为初始裁定值，未经相关性黄金集回归校准（weights.rs 已注明）。校准后权重可能移动，fuzzing 组 top-1 顺序或随之变化。
2. 六轴剩余三轴：category/tech/profile/relation 轴（resource_categories/skill_mappings/site_profile 数据源）未接入，keyword 覆盖为六轴中唯一可迁移项。随 M5+ workflow-engine/knowledge-engine 接口面落地后收编。
3. 向量检索：§8.1 search-engine 职责含"FTS/向量/混合"，本批只落 FTS + 混合；向量面无数据源（embeddings 表不存在），未实现。
4. 增量索引：contentless FTS5 无增量删除，更新 = 整库重建（81 文档 132ms）。catalog 规模增长后需迁移 external content 表或分段重建策略。
5. 索引漂移告警：索引库 built_at 与源库 updated_at 的一致性检查未实现（源库更新后索引不会自动失效，需人工 rebuild）。
6. decide 与资源实体的联动：decide 只记录决议（M1 口径），对资源的实际变更（合并/隔离/释放）需另行经 Governor 执行，该联动编排属 M5+。
7. 容器验证：本批未重跑 podman 构建（M2 的 Containerfile 未含新 crate 的 COPY 变更；tests crate 的容器排除清单需加 m4_governance）。构建为宿主机 offline 全绿。
8. 中英混合查询 gram 混排：纯 ASCII 词与 CJK 词混查时各自走 MATCH OR 分支，跨语言相关性融合只靠桥接词，未做语言感知加权。

边界遵守：
- 未 git commit/push。
- migrations/ 既有文件零修改：platform-dryrun.db 只读打开（md5 核验不变），quarantine jsonl 只读消费；platform-search.db 与 platform-governance.db 为本批新增文件。
- 未改 eri 生产代码（eri_p2.py 以子进程 + 生产库副本运行）。
- 未碰 workers/、apps/。
- tests/tests/m2_integration.rs 一处断言更新（非本批交付物破坏）：并行 M6 会话向 migrations/ 新增 003_tools_workflow.sql 后，M2 硬编码的迁移版本断言 [1,2] 失真；改为"前两版固定 1/2 + 版本数等于 sql 文件数 + user_version 跟随"，语义等价且随迁移目录演进。tests/ 不在禁改清单内，改动已最小化并加注。
