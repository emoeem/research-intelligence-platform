# SK-1 ADR：通用 Knowledge Catalog/Document 摄取接线（P6-9）

> 状态：已裁决（SK-1 批次内实施）  
> 日期：2026-11-12  
> 基准：docs/RIP-SECURITY-KNOWLEDGE-SOURCES-PLAN.md §4/§8（P6-9 七步）、docs/phase6/PHASE6-PLAN.md、docs/DOC-STANDARD.md  
> 相关：migrations/010_knowledge_ingest.sql、crates/plugin-runtime/src/ingest.rs、crates/context-broker/src/sources/plugin.rs、crates/search-engine/src/index.rs

## 一、裁决摘要

| 议题 | 裁决 |
| ---- | ---- |
| Domain 实体 | `domain::knowledge` 新增四实体：Source Snapshot、Catalog Entry、Document、ContentRef；配套三个词表枚举（EntryKind 六值、ReviewStatus、RightsPolicy） |
| 摄取表形态 | **新建通用表** `knowledge_source_entry`（011 迁移），不放宽 010 的 CHECK，不改写 knowledge_ingest 既有行 |
| RTIS 兼容 | `knowledge_ingest` 表与 ingest.rs RTIS 三类（poc/skill_md/method_case）行为、计数、幂等键零改动，仍是 RTIS 三类的唯一写者 |
| 去重语义 | 新表 UNIQUE(plugin_id, plugin_version, entry_kind, item_key)；catalog_entry 的 item_key = 规范化 canonical URL；document_markdown/skill_candidate 的 item_key = 包内相对路径；正文 content_sha256 另列，用于变更侦测与 Evidence 复核 |
| 审核闸 | 新条目初始 review_status = DISCOVERED；Search 第三源与 Context Broker PluginSource 只收录 review_status = APPROVED 的行；审核状态翻转为受控写面（plugin-runtime knowledge_review + audit 事件 + rp-plugin review 子命令） |
| Search 接线 | search-engine `load_docs` 增第三源（同 platform 库 knowledge_source_entry 表，APPROVED 且治理 ACTIVE），索引仍为可整库重建的派生物，INDEX_SCHEMA_VERSION 升 2 |
| Context Broker | PluginSource 空实现替换为读库实装：L1 元数据 / L2 摘要 / L3 按需正文，逐查询重读 plugin 表状态（禁用即撤出匹配面，无缓存失效窗口） |
| Evidence | 011 新表 `context_citation_evidence` 固化「实际给模型的引用片段 + excerpt/content sha256 + Source ID/Plugin ID/Version/Commit/URL/章节锚点」；写入走 resource-engine 新增受控写面，压缩与重试后凭哈希复核 |

## 二、背景与缺口（会话实查）

1. `crates/plugin-runtime/src/ingest.rs:38` `CONTENT_KINDS` 仅 `["poc","skill_md","method_case"]`，采集逻辑硬编码包内 `knowledge.db`、`skills/*.md`、`pocs/` 三个 RTIS 特定目录（ingest.rs:127 到 266）。
2. `migrations/010_knowledge_ingest.sql:32-33` `content_kind` CHECK 同款三值；SQLite 不支持 ALTER CHECK，改词表必须重建表并重写 15,155 行生产对账数据。
3. `crates/context-broker/src/sources/plugin.rs:21-38` `candidates()` 返回空集，`load()` 报错（Phase 6 前占位，注释自述）。
4. `crates/search-engine/src/index.rs:254` `load_docs` 只读 platform resource/resource_source 与 catalog github_metadata，插件知识正文不入索引。
5. assemble.rs:264 到 269 把 kind=Plugin 的候选标注「deferred to Phase 6」直接搁置。

## 三、Domain 实体裁决（计划 §4.1 四实体落位）

### 3.1 Source Snapshot（来源快照）

一个来源（Git 仓库固定 Commit / 站点 / 本地目录）在某插件版本下的一次冻结快照。字段：source_id、source_type、canonical_url、source_path、source_commit、license_status、rights_policy。落库形态：不单独建表，折叠进 `knowledge_source_entry` 的 source_* 列族（同一快照的条目共享相同值），理由：SK-1 的摄取单元是条目而非快照，快照级聚合可由列族 GROUP BY 得出，P6-10 sync 工具需要快照级状态时再提升为独立表（届时为纯增量迁移）。

### 3.2 Catalog Entry（目录条目）

链接型目录项（Awesome-Hacking/SecSkills/课程索引）。身份 = 规范化 canonical URL（item_key）；正文 = 条目行的原始文本与描述；不把外链项目正文视为来源仓库许可的覆盖面。

### 3.3 Document（Markdown 文档）

按许可策略允许持久化的 Markdown 正文。身份 = 包内相对路径（item_key）；正文 = 去 frontmatter 后全文；content_sha256 逐字节。

### 3.4 ContentRef（内容引用）

下游（Search 命中、Context 候选、Evidence）指向上述实体的统一引用：source_id（=plugin_id）+ entry_kind + item_key + plugin_version + content_sha256 + canonical_url。落库形态：Evidence 表的溯源列族（见第六节），Context ContentRef 结构体复用 domain::context::ContentRef 承载（source="plugin"，ref_id=`<entry_kind>:<item_key>`，ref_version=plugin_version，content_hash=content_sha256，uri=canonical_url）。

### 3.5 词表（domain::knowledge）

- EntryKind：`poc | skill_md | method_case | catalog_entry | document_markdown | skill_candidate`。前三类为 RTIS 既有三类（010 词表，只进 knowledge_ingest）；后三类为本 ADR 新增（只进 knowledge_source_entry）。两表词表互补、不相交，任何一类内容只有一个权威落点。
- ReviewStatus：`DISCOVERED | UNREVIEWED | APPROVED | REJECTED | DEPRECATED`（计划 §7 第 4 条初始 DISCOVERED，人工审查后 APPROVED；上游删除转 deprecated，不物理删除）。
- RightsPolicy：`catalog_only | approved_only | metadata_only`（Manifest 每内容类型声明；ingest 时落到 rights_policy 列）。

## 四、SQLite 兼容性裁决（计划 §4.1 优先方案的会话实测评估）

### 4.1 备选一（扩展 knowledge_ingest，被否决）

010 的 CHECK 约束在表定义内，SQLite 无 `ALTER TABLE ... MODIFY CHECK`；放宽词表必须 `CREATE TABLE 复制 + RENAME + 重放 15,155 行`，动生产对账基线，且 010 的幂等键 `UNIQUE(plugin_id, plugin_version, content_kind, content_sha256)` 对目录条目语义错误：两条不同链接的目录项若标题与描述同为空/同文本，content_sha256 相同即被 INSERT OR IGNORE 静默合并，丢条目。读写调用点实查：knowledge_ingest 的写面只有 ingest.rs（INSERT OR IGNORE），读面只有契约测试与对账脚本，无生产查询方依赖其词表闭集，但重建风险与去重缺陷两者已足够否决。

### 4.2 备选二（新建通用表，采纳）

011 迁移新增 `knowledge_source_entry`：

- 词表 CHECK 收全六值（含 RTIS 三类，为 Phase 7 收敛预留迁移路径），但 SK-1 的 RTIS 三类写面仍固定写 knowledge_ingest，新表不接 RTIS 三类（见 4.3）；
- 去重键 `UNIQUE(plugin_id, plugin_version, entry_kind, item_key)`，正文哈希 `content_sha256` 独立列：重跑幂等（键不变即 skipped），键同哈希变则 skipped 并在报告 `changed` 计数中显式暴露（版本化快照语义留给 P6-10/11 的 sync 工具，本批不覆盖历史行，保住 Evidence 可复核性）；
- 溯源列族：plugin_id、plugin_version、corpus_id、source_type、source_path、source_commit、license_status、rights_policy、review_status、risk_flags_json；
- 溯源位置列族：source_location_json（文件、行号、章节锚点、标题路径 category_path_json）。

### 4.3 两表关系（不保留两套长期互不兼容实体）

knowledge_ingest 定位冻结为 RTIS 兼容面（010 头注语义不变，只加不加改）；knowledge_source_entry 定位为 SK-1 起的通用知识来源面。Phase 7 Knowledge 正文模型收敛时，以一条一次性迁移把 knowledge_ingest 六值可表达行搬入新表并留下只读视图，届时三值 CHECK 的历史包袱随表退役。本批不做搬迁。

## 五、Manifest/Schema 裁决

1. `schemas/plugin-manifest/v1.json`：knowledge_source capability 的 provides 增 `content_kinds` 词表约束（六值枚举，向后兼容旧三值子集）与 `kinds` 每类型声明对象（source_path、parser、license_policy、max_entries）。
2. `plugin-registry/src/validate.rs` rule 3 增补精确归因：kind=knowledge 且 capability declares 三类新值任一时，`provides.kinds.<kind>` 必填且 parser/license_policy 在词表内、source_path 为安全包内相对路径（拒绝绝对路径与 `..`，复用 rule 7 的 entry_relpath_ok 语义）；错误码 `knowledge_provides_invalid`。RTIS 三类不触发新要求（零回归）。
3. parser 词表（平台内置，per-kind 注册表键）：`markdown_links`（catalog_entry）、`markdown_doc`（document_markdown）、`skill_frontmatter`（skill_candidate）。
4. license_policy 词表：`catalog_only`（只登记条目元数据，不持久化外链正文）、`approved_only`（仅 APPROVED 条目可进 Search/Context）、`metadata_only`（只存元数据与哈希，正文列允许空）。

## 六、Ingest 与 Evidence 裁决

1. ingest.rs 重构为 per-kind parser 注册表（kind 到 parser 函数映射），RTIS 三类解析函数逐行保留原逻辑；三类新 parser 为纯文本解析（不执行 Markdown/HTML 中任何代码、不访问链接，计划 §8 P6-9 第 4 步）。
2. dry-run：`IngestOptions { dry_run }`，写步以 SAVEPOINT 包裹后 ROLLBACK TO + RELEASE，报告计数与实跑完全同构，零落盘。
3. Evidence（计划 §8 P6-9 第 7 步）：011 新表 `context_citation_evidence`（evidence_id、manifest_id、item_id、ref_source、ref_id、ref_version、excerpt、excerpt_sha256、content_sha256、source_id、plugin_id、plugin_version、source_commit、canonical_url、section_anchor、source_location_json、created_at）。装配期 L2 装入即写（excerpt = 实际进入 AssembledContext 的摘要文本），L3 expand 写（excerpt = 实际展开正文，分段截断保存头部 4KiB 全量哈希仍为整段 content_sha256）；行只追加，插件禁用/卸载不影响读取（历史 Evidence 可读验收项）。写面收在 resource-engine/sqlite.rs 新增受控方法（与既有 ResourceRepository 同库同写者纪律），context-broker 只经该方法写。

## 七、Search 与 Context 裁决

1. 第三源准入条件（两条同时满足）：`review_status='APPROVED'` 且 plugin 行 `is_active=1 AND state IN ('ENABLED','LOADED','ACTIVE') AND source_uninstalled=0`。理由：禁用即撤出匹配面（PLAN 验收原文），卸载来源同理；索引为可重建派生物，准入条件变化走整库 rebuild。
2. 索引文档映射：resource_id=`kse:<plugin_id>@<plugin_version>:<entry_kind>:<item_key>`，resource_type=entry_kind，governance 恒 ACTIVE，license_status 取行值缺省 UNKNOWN。INDEX_SCHEMA_VERSION 1 升 2（查询端拒旧版即强制重建，既有语义）。
3. query.rs 治理值过滤兜底：既有 `governance NOT IN (...)` 参数化过滤保留；plugin 条目入库即 ACTIVE，但查询端对 review 闸再加一层 `resource_type NOT IN ('poc','skill_md','method_case')` 之外的过滤不需要，准入在 load_docs 侧一次收口（第六节 1），查询端零改动即兜底成立，本 ADR 记录该定位。
4. PluginSource（context-broker）：candidates/load 每次调用实时联查 plugin 表与 knowledge_source_entry（APPROVED + 可见插件），无进程内缓存即无失效信号缺口；禁用后候选为空集（合法结果，设计稿 §9.2 第 4 条），历史 context_citation_evidence 行不删（验收「历史 Evidence 可读」）。assemble.rs 的 Plugin 桶从「deferred」改为并入知识桶（L2 摘要形态）。
5. 内容隔离：插件正文只作为 L2/L3 参考内容注入，Source 词表沿用 `plugin:<id>` 前缀形态，不得进入 L0 规则层。

## 八、后果与不做

- 后果：15,155 RTIS 对账基线零回归（新表不接 RTIS 写面）；Search/Context 只见 APPROVED 条目，DISCOVERED 条目对模型不可见（治理闸优先于可用性）。
- 不做：审核工单系统（review 翻转只提供受控写面与 CLI）、目录条目正文抓取（rip.knowledge.document.fetch 属 P6-11 T1 工具）、向量索引（content_kinds provides 的 vector 字段本批仅登记不消费）、knowledge_ingest 数据搬迁（Phase 7）。
