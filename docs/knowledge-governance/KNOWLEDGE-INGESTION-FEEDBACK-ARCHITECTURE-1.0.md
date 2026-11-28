# Knowledge Ingestion & Feedback Architecture 1.0

> 状态：P1 已实现；Document Revision 适配、区块证据定位、CVE/CWE 提及抽取、
> 精确重复提示、block-hash 版本差异、审核台账接线、候选证据读取，以及 Nuclei
> 候选向量和 review-only 相似建议已实现。人工语义评测、Agent/Workflow 接线、
> 权威知识发布/索引刷新及完整来源验收仍未完成。

## 一、目标

RIP 后续的知识回流与知识新增统一通过一条 Knowledge Ingestion
Pipeline。输入可以是 Agent Finding、结构化报告、GitHub 仓库、PDF、
Markdown、HTML、DOCX、漏洞公告、POC、Nuclei 模板、工具或 Skill，
但出口必须遵循相同的 Raw、Candidate、Published 状态和 Evidence
治理规则。

知识资产必须可检索、可关联、可追溯、可更新和可验证。不能把文档数量、
解析成功或 AI 摘要等同于正式知识。

## 二、三层数据模型

### Raw

保留原始来源快照、来源类型、URL、Commit、ETag、采集时间、文件哈希、
存储 URI 和许可证。Raw 数据不直接进入正式检索或 Agent 上下文。

现有复用：

- `content_objects`
- `source_records`
- `documents`
- `document_revisions`
- `document_assets`
- `document_blocks`
- `knowledge_document_fetch`

### Candidate

保存自动解析、结构化提取、去重、实体匹配、版本差异和质量检查结果。
Candidate 必须保留来源位置、证据引用、AI 生成标记、解析器版本和置信度，
但不能自动批准。

现有复用：

- `knowledge_feedback_candidates`
- `knowledge_evidence_provenance`
- `review_batch`
- `review_batch_item`
- `review_batch_decision_log`
- `knowledge_relation_evidence`

### Published

只有人工批准后形成正式 KnowledgeCard、实体、关系和证据记录。Published
资产可进入检索和 Agent Context，但仍保留版本和撤销历史。

现有复用：

- `knowledge_cards`
- `knowledge_entity_refs`
- `knowledge_entity_sources`
- `knowledge_entity_source_assessments`
- `knowledge_relations`
- `knowledge_relation_review_log`

## 三、统一摄入协议

所有入口提交相同的任务载荷：

```text
KnowledgeIngestionRequest
  workspace_id
  source
  source_refs
  document_or_asset
  entry_type
  parser_policy
  quality_policy
  extraction_policy
  trace_id
  run_id
  requester
```

处理结果：

```text
KnowledgeIngestionResult
  ingestion_id
  raw_revision_id
  parse_state
  candidate_ids
  relation_candidates
  duplicate_candidates
  update_candidates
  review_batch_id
  evidence_refs
  warnings
  blockers
```

状态机不得允许 Candidate 自动跳到 Approved。自动处理只生成候选、
差异和审核批次；正式写回由现有批次审批和权威数据写回路径执行。

## 四、流水线

1. 来源登记。
2. 内容寻址和原始快照。
3. 格式识别和安全解析。
4. IR 结构化及 Chunk 定位。
5. 实体和 Knowledge 提取。
6. 精确标识符、源码哈希、规范化文本和相似度去重。
7. 与已有记录比较，生成新增、修改、删除、失效和冲突候选。
8. Evidence、来源可信度、许可证、风险和质量检查。
9. 创建或追加审核批次。
10. 人工批准后发布并刷新派生索引。

每个阶段必须持久化 run、trace、Artifact、Evidence 和状态，支持重试、
暂停、恢复和幂等。

## 五、复用现有能力

| 能力 | 现有实现 | 摄入面使用方式 |
|---|---|---|
| 文档域 | V0002 / V0003 | Raw、Revision、Block、Asset 和解析状态 |
| 文档路由 | `crates/document-router` | PDF、Office、媒体、归档及安全通道 |
| 文档 IR | `crates/intel-db/src/ir.rs` | 统一结构化解析结果 |
| Evidence 提取 | `crates/research-engine` | 提取物和内容哈希 |
| Finding 回流 | `crates/intel-db/src/maintenance.rs` | Report IR 到候选知识和 Evidence |
| 知识卡片 | `crates/intel-db/src/card_repo.rs` | 发布候选和审核状态 |
| 实体关系 | V0011 | 技术实体、资产和关系 |
| 来源评估 | V0012 | 来源、许可和 trust 只读结论 |
| 批量审核 | V0014 与 `rp-batch-review` | 统一差异和风险审批 |
| 版本来源 | `knowledge_source_entry` | 文件、Commit 和上游变更 |
| 搜索派生 | Search Engine 与 PG Hybrid | 发布后重建或增量刷新 |
| Workflow | `workflow-engine` | 摄入工作流持久化和恢复 |

## 六、差异审计结论

已具备且应直接复用：

- PostgreSQL 文档、来源、对象、Revision 和 Block 模型。
- Document Router 的格式嗅探、安全通道和 Document IR。
- Finding 到候选 Knowledge 的事务回流。
- Evidence provenance 和 append-only 审核历史。
- 实体、关系、来源评估和 batch review 数据面。
- V0014 的人工 `decide / undo / apply` 分离。
- SQLite KSE 版本和 review 状态。
- 搜索索引派生和重建能力。
- Agent/Workflow 的 run、trace、artifact 和 evidence 关联。

尚缺的核心能力：

1. Agent Tool 适配和 Document/URL/Repository 等其它来源适配器。
2. 文档 Chunk 到 Evidence 位置的统一映射。
3. 自动提取知识和资产的结构化 extractor orchestration。
4. 来源、规范化文本、POC/template 和知识实体去重流程。
5. 基于旧 Revision 的新增、修改、删除和失效差异。
6. 差异优先、风险优先的审核工作台展示。
7. 发布后索引刷新和正式关系写回。
8. 大批摄入的任务预算与可观测工作流集成。

## 七、成熟技术选择

成熟库优先评估，不因 vendor 缺失或容器偏好直接否决：

| 能力 | 优先方向 | 约束 |
|---|---|---|
| PDF/DOCX/Office | 现有 Document Router 和宿主工具为主 | Docling 只作为复杂文档候选评估 |
| Markdown | native_markdown 与现有解析链 | 保留代码、表格和文件位置 |
| HTML | 成熟正文提取库与现有抓取审计 | 不绕过 SSRF 和跳转策略 |
| Word/PDF 复杂版面 | Docling | 必须评估许可证、依赖、性能和离线 vendor |
| ETL 补充格式 | Unstructured | 只补充确实存在的格式缺口 |
| Office 转换补充 | MarkItDown | 先做真实样本对比，不同时批量引入 |
| Schema 校验 | Pydantic / JSON Schema | 与 Rust Serde/Schema gate 共享词表 |
| 精确去重 | PostgreSQL 约束和 SHA-256 | 不用相似度替代精确哈希 |
| 语义去重 | 现有 pgvector / BGE | 只生成 candidate，不自动合并 |
| 工作流 | 现有 workflow-engine | 不另建调度器 |

引入 Docling、Unstructured 或 MarkItDown 前必须记录维护状态、许可证、
依赖、MSRV/Python环境、离线构建、真实文档效果、性能和迁移路径。
未通过本项审查不得同时引入三个库。

## 八、实施阶段

### P1：统一摄入协议

定义 request/result、ingestion、原始来源和 Revision 关联，复用现有状态、
run、trace、artifact 和 evidence 模型。

`KnowledgeIngestionRequest` 携带 contract version、workspace、idempotency key、
source、requester 和可选 run/trace；PostgreSQL V0015/V0016 持久化任务状态、
来源版本、证据锚点、候选及输出引用。Report adapter 调用既有
`maintenance::ingest_report` 并保留其 confirmed finding + Evidence 门槛。
Document Revision adapter 只消费已由既有 `rp-doc parse` 处理的 revision，
不复制原始内容或新建解析器。

- CLI：`rp-ingest report/document/status/retry`。
- REST：`POST /api/knowledge-ingestions`、`GET /api/knowledge-ingestions/{id}`、
  `GET /api/knowledge-ingestions/{id}/candidates/{candidate_id}`、
  `POST /api/knowledge-ingestions/{id}/retry`，复用 Bearer 鉴权及 PostgreSQL
  子进程约定。
- MCP：`rip.submit_knowledge_ingestion`、`rip.get_knowledge_ingestion_status`、
  `rip.get_knowledge_ingestion_candidate`；通过 caller 解析后的受控
  `rp-ingest` 子进程调用，workspace 固定为 `default`，requester 来自 MCP
  caller，调用参数不能携带 workspace 或数据库凭据。候选详情沿用 100 条/
  240 字符证据上限。
- Idempotency：workspace + key 唯一；同 key 不同请求拒绝；PROCESSING 任务
  过期后可被 retry 回收。报告来源和原始 IR 仍由既有 `reports` 表持有。
- 安全边界：API 当前仅接受 `default` workspace，直到 caller-workspace ACL
  可用；CLI 可指定 workspace，pipeline 执行既有 workspace 隔离检查。
- 当前结果：报告引用、Finding 与 Candidate ID/计数、review-required 未发布
  状态；没有新建第二套 Evidence 或审核权威表。
- Candidate 详情只对请求指定的 ingestion + workspace 返回，文档证据最多返回
  100 条、每条最多 240 字符的上下文，并显式标记为不可信来源文本。
- 这些 MCP 工具是外部 MCP caller 的服务入口；内部 Agent/Workflow 尚无经批准
  的服务凭据代理，不能直接调用或绕过 Tool Registry / Tool Engine。

REST 仅接受 `default` workspace，直到 caller-workspace ACL 建立。文档摄入要求
revision 的 parse_state 为 `APPROVED` 或 `REVIEW_PENDING`；Evidence 锚点引用
Document IR block、block SHA-256、页码/位置元数据及匹配字符偏移，原始内容
仍在 Document Domain/CAS。Report 候选沿既有 feedback 审核；文档候选进入
`knowledge_ingestion` review batch。批次批准只将候选标记为 `REVIEWED`，
含义是人工确认标识符提及，不是漏洞/弱点事实验证或知识发布；支持 undo 回滚。

### P2：文档自动化处理

复用现有 Document Router 和 PG Document IR。已实现已解析 revision 的摄入、
block/page/location/SHA-256 定位和命中字符偏移。真实 PDF、DOCX、Markdown、
HTML、Nuclei YAML、归档等 parser 样本覆盖与解析质量验证仍待补齐；静态
Nuclei 元数据抽取的合成 IR 测试不替代 parser 样本验收。

验收：真实 PDF、DOCX、Markdown、HTML、Nuclei YAML 和归档样本通过，
解析质量不是解析成功率。

### P3：知识提取与实体关联

从 IR block 提取严格格式的 CVE/CWE 标识符提及，连同解析器版本、来源哈希、
块/页/字符偏移和解析质量进入候选及 review batch；不会声称提及代表漏洞或
弱点事实。另有首个结构化 extractor：对不超过 64 KiB 且符合 YAML 结构的
Nuclei block，只抽取模板 ID、`info.name` 和受限 severity 词表，生成
`nuclei-template-metadata.v1` 候选；身份键保留源模板 ID 原样，不按标题或
URL 合并。提取字段标为不可信，request、payload 等其他模板内容不持久化、
不执行。匹配到但无法解析、severity 不受支持或超出 64 KiB 的块会在摄入
结果中返回有界的 extractor diagnostic（最多 100 条，附带截断计数），不静默
伪装成提取成功。Knowledge、POC、Tool、Skill、Project 等结构化候选与完整
实体关联仍待实现。

验收：AI 结论带模型/规则版本、证据位置、置信度和来源快照。

### P4：去重、更新和质量治理

已实现同一 workspace 内对候选标识符和 Nuclei 源模板 ID 的精确匹配、
现有 vulnerability entity 引用探测，以及不同 ingestion 间精确重复提示。
同一文档新 revision 还会按 block SHA-256 多重集记录 added/removed/unchanged；
相同标识符在变更 block 出现时标记 `VERSION_CHANGE_REVIEW` 并指向前序候选，
不合并/覆盖。结构化 metadata 若在同一 ingestion 中同 ID 出现不同值，会在
review payload 标记冲突并保留有界变体。失效、危险命令、敏感信息和质量评分
仍待实现。

V0018 为 Nuclei 模板候选增加派生向量表和相似建议表。`rp-ingest semantic-scan`
只将无冲突且未拒绝的候选 `info.name` 与 allowlisted severity 送入现有
`EmbedWorkerProvider`，沿 Tool Engine / `podman_host` 路径生成并校验向量；
按 workspace 隔离，最多处理 5,000 个候选、64 条一批，记录输入 SHA-256、
模型及 worker 版本。调用者必须显式提供阈值，近邻建议只写入派生表，并在
候选详情中标记为 `review_only`。扫描不创建关系事实、不自动合并、批准、
发布或删除候选；候选级集成烟测已通过一次性 PostgreSQL 和 pinned `rip-embed`
验证。该烟测只证明管线与持久化行为，不构成去重质量验收。

Duplicate/Related/Distinct 的候选人工标注集、独立校准/测试划分、阈值选择、
precision/recall、误合并率和漏检率仍缺失。

验收：重复正式实体数量为零，更新可表示新增、修改、删除和失效，
高风险候选不会自动批准。

### P5：审核与发布

文档提及与静态模板 metadata 候选复用 `review_batch` 决策、apply、undo 与
审计台账；apply 只记录人工审核，不写入 KnowledgeCard、漏洞权威事实或搜索
索引。事实候选的风险筛选、权威写回及派生索引刷新仍待定义和实现。

验收：reviewer、reason、reviewed_at、Evidence 和 apply 状态完整。

### P6：Agent 回流闭环

将摄入能力暴露为 API、CLI 和 Agent Tool，接入 Research 和专业 Agent。

验收：真实 Agent 可提交 Candidate、等待人工审核、读取 Evidence 和完成
发布后检索闭环。

## 九、验收门禁

| 门禁 | 目标 |
|---|---|
| 来源可追溯率 | 正式知识 100% 可回到 Raw/Revision/Source |
| Evidence 完整率 | 正式知识 100% 有有效 Evidence 或明确例外状态 |
| 自动结构化 | 所有正式支持资产类型由统一 pipeline 结构化 |
| 未审核数据 | 不进入 Published 或默认检索上下文 |
| 重复正式实体 | 零未经识别重复 |
| 版本更新 | 能识别新增、修改、删除、失效和冲突 |
| AI 检索 | 返回知识、来源、证据、状态和版本 |
| AI 回流 | Agent 可提交 Candidate并进入人工审核 |
| 危险内容 | 不因批量 approve 被放行 |
| 真实样本 | 使用 PDF、DOCX、Markdown、代码、公告、POC、Nuclei 和 GitHub |

最终发布不能只看导入条数。解析成功、检索命中、API 200、
来源 APPROVED 和静态扫描都不等于事实验证或正式发布。
