# RIP3 统一领域模型与 RTIS 映射（03-DOMAIN-MODEL，冻结物 1）

> 状态：Phase 0 冻结（2026-11-14）；评审通过后为 Phase 1 实施唯一依据。
> 关联：00-ARCHITECTURE-AUDIT.md（实证基线）、R2 第三部分（判据原文）、
> ADR-0001（PG 绞杀者决策：本模型全部实体落 PostgreSQL）。
> 复用纪律：ID 惯例沿 crates/domain/src/ids.rs，不在本模型引入第二套 ID 体系。

## 一、核心实体（字段级定稿）

### 1.1 Document（用户导入的原始资料，PG 表 documents）

| 字段 | 类型 | 约束/说明 |
|---|---|---|
| document_id | text PK | 前缀 doc-（ids.rs 惯例新类型 DocumentId） |
| title | text | 非空 |
| document_type | text | 词表：book/pdf/docx/pptx/xlsx/html/markdown/code/image/dataset/other |
| original_filename | text | 非空 |
| mime_type | text | 真实格式检测值（魔法数，不信扩展名，13-SECURITY） |
| file_size | bigint | 字节 |
| storage_uri | text | Object Storage Adapter URI（cas://&lt;sha256&gt; 惯例，禁止业务代码拼文件系统路径，R2 10.1B） |
| sha256 | text | 原始文件内容哈希（CAS 寻址键，D1 去重键） |
| owner_id | text | 预留（单用户期固定 owner-local） |
| workspace_id | text | 工作区隔离边界（权限前置过滤键，13-SECURITY 第五节） |
| visibility | text | 词表 private/shared/public（缺省 private） |
| language | text | 检测语言（可空） |
| created_at / updated_at | timestamptz | UTC |
| lifecycle_status | text | 词表 ACTIVE/DELETED（软删除优先，R2 10.4） |

### 1.2 DocumentRevision（版本链，表 document_revisions）

| 字段 | 说明 |
|---|---|
| revision_id PK | 前缀 drev- |
| document_id FK | documents（同库 FK） |
| revision_number | 整数，(document_id, revision_number) UNIQUE，从 1 递增 |
| content_hash | 该版本规范化内容哈希（D2 去重键） |
| parser_version | 解析器标识+版本（可空：未解析版本） |
| source_version | 上游版本（Git commit/URL etag，source_sync 泛化携带） |
| revision_status | 词表 CURRENT/SUPERSEDED（旧版本不因上传被删除，R2 3.1） |
| created_at | |

### 1.3 DocumentBlock（解析结构节点，表 document_blocks）

| 字段 | 说明 |
|---|---|
| block_id PK | 前缀 blk- |
| document_id / revision_id FK | 版本锚定 |
| parent_block_id | 可空 FK（父子结构，不允许压平，R2 3.1） |
| block_type | 词表 14 值：document_title/heading/paragraph/list/table/code/image/figure/formula/quote/reference/page/chapter/section |
| order_index | 兄弟序 |
| content | text |
| page_number | 可空整数 |
| location_metadata | jsonb（页内 bbox/字符偏移，source_mapping 的存储侧） |
| extraction_method | 词表 docling/pymupdf/tesseract/native_markdown/manual |
| extraction_confidence | real 可空 |

索引：(revision_id, parent_block_id)、(revision_id, block_type)、GIN(location_metadata)。

### 1.4 KnowledgeChunk（检索分块，表 knowledge_chunks）

| 字段 | 说明 |
|---|---|
| chunk_id PK | 前缀 chk- |
| document_id / revision_id FK | |
| source_block_ids | text[]（原始位置映射，R2 3.1 必须保留） |
| parent_chunk_id | 可空（层级分块） |
| content_hash | 规范化哈希 |
| content | text |
| token_count | 整数（tokenizers 校准后实值） |
| language | |
| chunk_strategy | 词表 fixed_size/semantic/section（可扩展） |
| embedding_status | 词表 PENDING/DONE/FAILED（向量列另建，见 1.7） |
| quality_status | 词表 PENDING/PASSED/FLAGGED |

### 1.5 KnowledgeCard（可复用知识，表 knowledge_cards）

| 字段 | 说明 |
|---|---|
| knowledge_id PK | 前缀 kc- |
| knowledge_type | 词表：vuln_principle/vuln_signature/exploit_condition/detection_method/fix_method/security_technique/tool_usage/research_case（R2 3.1 覆盖面，可扩展） |
| title / summary | |
| structured_content | jsonb（类型化载荷，按 knowledge_type 有子 Schema） |
| confidence / quality_score | real（质量分模型见 5.3 落地面） |
| verification_status | 词表 UNVERIFIED/VERIFIED（AI 派生内容必须标 AI 衍生与验证状态，原则 C） |
| review_status | 词表 DISCOVERED/PARSED/CANDIDATE/APPROVED/QUARANTINED/REJECTED/DEPRECATED（R2 5.4 统一审核态） |
| evidence_level | 词表 K0-K4（对齐 RTIS k_meta 语义，外部文档入库行携带，清 00-审计缺口 4） |
| created_at / updated_at | |

文档关联：document_cards 关联表（document_id, knowledge_id, document_revision_id）N:M。

与 RTIS 分工：knowledge_type 覆盖方法论知识；具体漏洞条目/POC/链属于 RTIS（第六节映射）。

### 1.6 KnowledgeRelation（知识关联网络，表 knowledge_relations）

- relation_id PK（前缀 krel-）；src_id / dst_id（KnowledgeCard 或 Chunk 的 ID）；
  relation_type 词表 10 值：SAME_CONTENT/NEAR_DUPLICATE/RELATED_TO/SUPPORTS/
  CONTRADICTS/SUPERSEDES/FIXES/DETECTS/DERIVED_FROM/REFERENCES；
- relation_evidence jsonb、relation_source（deducer 名 + 版本）、confidence real、
  created_at；UNIQUE(src,dst,type)。
- 每条关系必须有独立证据与来源（R2 3.1）；D4 语义相似只产 CANDIDATE 候选边，
  不自动合并（R2 5.2）。

### 1.7 向量面（表 knowledge_chunk_vectors，pgvector）

| 字段 | 说明 |
|---|---|
| chunk_id PK/FK | knowledge_chunks |
| embedding | vector(512)（bge-small-zh-v1.5；HNSW 索引 vector_cosine_ops） |
| embedding_model_id / embedding_version / dimension | 元数据三件（R2 6.2 禁止写死模型） |
| index_version | 重建/增量追踪 |

VectorBackend trait 封装（后端 = pgvector；LanceDB/Qdrant 为可换候选，ADR-0001）。

### 1.8 ContentObject 与 SourceRecord（内容复用与来源分离，R2 5.5）

- content_objects：content_hash PK（sha256）、size_bytes、mime_type、storage_uri、
  ref_count、created_at。不同来源共享同一对象（CAS 寻址）。
- source_records：source_id PK（前缀 src-）、content_hash FK、source_type
  （git/github/url/upload）、source_url、source_commit/etag、license_status、
  imported_at、workspace_id。同一文件三仓库 = 1 对象 + 3 来源 + 3 许可检查。
- 删除保护：ref_count > 0 的对象禁删；孤儿清理（ref_count=0 且无版本引用）走
  保留归档面（12-STORAGE）。

### 1.9 Finding / ResearchTask / Report（编排面，事实引用 RTIS）

- research_tasks（PG）：task_id（前缀 task-）、objective、workflow_run_id（017 关联，
  ID 引用非跨库 FK）、coverage_matrix jsonb、assumptions jsonb、status。
- findings（PG）：finding_id（前缀 find-）、task_id、rtis_ref（RTIS 漏洞条目引用键，
  细节不落 3.0 库，R2 3.4）、title、severity（RTIS 权威值镜像只读）、
  five_state 词表 CONFIRMED/NOT_REPRODUCED/NOT_TESTED/INCONCLUSIVE/FALSE_POSITIVE
  （R2 7.5D 硬性）、evidence_ids text[]。
- reports（PG）：report_id（前缀 rep-）、task_id、report_ir jsonb（04/11 号文档 Schema）、
  format_exports jsonb、created_at。Finding 与 Evidence 的事实内容引用 RTIS，
  3.0 负责结构化/渲染/导出。

## 二、实体关系（R2 3.2 全覆盖）

Document 1:N DocumentRevision；DocumentRevision 1:N DocumentBlock；
DocumentRevision 1:N KnowledgeChunk；Document N:M KnowledgeCard（document_cards）；
KnowledgeCard N:M KnowledgeCard（knowledge_relations 自引用）；ResearchTask 1:N Finding；
Finding 1:N Evidence（证据以 artifact/evidence 六 ID 引用）；ResearchTask 1:N Report；
Report N:M Finding；Report N:M Evidence。全部以稳定 ID 关联（同库 FK + 跨库 ID 引用），
禁止标题/文件名关联。

## 三、ID 与迁移规范（R2 3.3）

1. ID 惯例沿用 domain::ids.rs：前缀小写连字符（doc-/drev-/blk-/chk-/kc-/krel-/src-/
   task-/find-/rep-）+ 长度 ≤128 校验新类型；生成侧前缀是服务端实现细节（ids.rs:113
   惯例延续）。
2. ID 不可变；数据版本随 revision；关系完整性由 FK（同库）与引用校验（跨库）承担；
   删除软删优先（lifecycle_status），硬删仅 retention 面。
3. 迁移全部新增式：PG-0001 起 refinery 链（每文件事务），SQLite 侧 020 起仅运营面
   新增；不删既有表，旧实体建兼容视图逐步迁移（详见 12-STORAGE-AND-MIGRATION）。

## 四、与既有实体的边界

- knowledge 表（001，RTIS 摄取面）保持现状不动：RTIS 对账 15155 口径不回归（红线）。
  KnowledgeCard 是新增独立实体，两面的接续点（方法论卡片经 RTIS 接口登记）按 R2 5.1。
- context_manifest/citations/agent memory 继承不动（00-审计 D 组）。
- workflow/agent/tool/plugin/runtime/audit 全部留 SQLite 运营核，PG 侧以 ID 引用。

## 五、权限模型落点

workspace_id/visibility 落在 documents/knowledge_cards/source_records；检索入口
前置过滤（WHERE visibility 与 workspace 谓词在 SQL 内，PG RLS 作纵深防御第二层），
禁止检索完成后过滤（R2 11.4）。细节见 13-SECURITY-AND-PRIVACY 第五节。

## 六、RTIS 五面字段级映射表 v1（冻结，Phase 1 起生效）

映射纪律（R2 3.4 四条硬规则全文适用）：零直写（一切读写走 rt.py CLI / rtis MCP 工具面）；
零行为变更（RTIS schema/接口/生产行为零改动）；细节不落库（POC 正文/利用细节只存在
RTIS 与授权平台）；对账延续（15155 口径为回归红线）。

| 3.0 对象 | RTIS 面 | 字段级映射 |
|---|---|---|
| findings.rtis_ref | poc 表（rt.py:85-90 POC_COLS） | 引用键 = poc.code（poc 主业务键）；镜像只读列：severity/cve/component/versions/tags/verified（写侧唯一入口 rt.py poc add/update，CLI 经 subprocess 与 tool-engine 既有适配同款） |
| findings（漏洞结论产出） | rtis.py task/outstanding + k_proposal（migrate_m3.py:88-101：from_run/type/title/claim/sources_json/evidence_level/status） | 3.0 研究结论经 RTIS 接口登记为候选（对应 k_proposal propose 语义），RTIS 评审流为准；3.0 侧只存 rtis_ref 与状态 |
| findings.evidence_ids | evidence 面 + 平台 artifact | 原始证据（请求响应/截图/log）= 3.0 Artifact（六 ID 关联）；结论性证据状态（K 分级）以 RTIS k_meta.evidence_level 为准 |
| research_tasks | rtis.py task/run 面（rtis.py:1586 任务状态机）| 3.0 ResearchTask 是编排容器（workflow run/覆盖矩阵/假设清单），task 编号经接口对齐；安全结论经既有接口落 RTIS |
| knowledge_cards.relation_type=ATTACK_STEP | RTIS 攻击链面（仓外 engagements assets.db，零联邦） | 链路事实只存 RTIS：3.0 图表/渲染引用其接口读面，不复制链数据 |
| reports（漏洞类） | RTIS 报告面（现状 = skill 模板 + 文件约定，无结构化工具） | 报告事实数据（漏洞/证据/链）引用 RTIS；3.0 Report Engine 负责结构化/渲染/导出；RTIS 零行为变更（不要求 RTIS 新增接口，报告组装全在 3.0） |
| knowledge_cards（方法论） | RTIS 知识面接续点 | APPROVED 方法论卡片可经 rt.py poc add（或 rtis MCP redteam_poc_add）登记为 RTIS 条目（显式人工动作，非自动同步）；反向 RTIS 条目作为高质量来源进 3.0 检索（evidence_level 继承 K 分级） |

## 七、冻结范围声明

本文件冻结：实体字段、关系、ID 规范、RTIS 五面映射表 v1。未冻结（按阶段 ADR 滚动）：
具体 DDL 细节、索引参数、RLS 策略语句、KnowledgeCard 子 Schema（随 Phase 1/3 ADR）。
