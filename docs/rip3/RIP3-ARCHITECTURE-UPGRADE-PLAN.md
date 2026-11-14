# RIP 平台架构升级计划书（修订版 R2）

> 本文件是「原 RIP 3.0：大规模知识智能平台架构升级与实施计划书」经 2026-11-14 架构评审后的**唯一权威修订版**。后续执行代理只遵循本文件，不需要也不应该参考旧版计划书或评审意见来猜测要求；凡本文件与旧版冲突之处，一律以本文件为准。
>
> | 项 | 值 |
> |---|---|
> | 状态 | 已评审定稿；2.0 已收口（2744ada），Phase 0 已产出（见修订记录） |
> | 修订日期 | 2026-11-14 |
> | 取代对象 | 原 RIP 3.0 计划书全文、首轮架构评审意见全文 |
> | 生效条件 | RIP 2.0 完成收口（见第零部分）**（已满足，2026-11-14）** |
> | 实施预算 | 暂按 3 到 4 个周末窗口规划，以 Phase 0 实际审计结果重排 |
> | 工程根 | `research-platform` 仓库（独立 git 仓库，origin `github.com/emoeem/research-intelligence-platform`） |

## 修订记录（R2 定稿后的用户裁决，正文中冲突处以本记录为准）

1. **数据库策略（2026-11-14 用户裁决，修订 §6.3 与 §10.1 的「第一阶段继续 SQLite」口径）**：情报核（Document/Chunk/Vector/KnowledgeCard/Relation/Finding/Report/治理面）采纳 PostgreSQL 17 + pgvector + pg_jieba + pg_search，Phase 1 起新增式落地（refinery 迁移链），新增域永不落 SQLite；运营核暂留 SQLite（绞杀者路线）。全文见 decisions/ADR-0001；实施细节 12-STORAGE-AND-MIGRATION.md。原文「第一阶段继续 SQLite + FTS5」与「未来迁移 PostgreSQL 的路径必须明确」由本条取代（迁移路径已决策并立即执行）。
2. **许可证口径（2026-11-14 用户裁决）**：许可证不做淘汰门槛，能力优先（许可仅记录）。§4.3/§4.4 中 PyMuPDF（AGPL）与 BM25 面 ParadeDB pg_search（AGPL）按能力转正选型；台账见 decisions/ADR-0002。
3. **Phase 0 已产出（2026-11-14）**：00-ARCHITECTURE-AUDIT.md（A-G 登记表 + 三关键问题实证：向量检索未接入 / RTIS 五面映射完整度 / source_sync 泛化差距；迁移编号修正为下一可用 020）+ 四冻结物（03/04/12/13）+ ADR-0001/0002。窗口重排建议 5 窗（见 00-审计第六节），待用户确认。


---

# 第零部分：执行边界（执行 AI 先读本部分）

## 0.1 与 RIP 2.0 的硬性顺序约束

RIP 2.0 尚未收口。收口清单（以 `docs/SESSION-HANDOFF.md` 第三节与 `docs/HANDOVER-RIP2-CONTINUATION.md` 为准）：

1. Phase 8 Workflow Studio：批次 1 到 3 已实施（WIP 备份 `7707d13`），批次 4（web 画布）进行中，之后是五道门禁、独立验收、RELEASE 入库。
2. Phase 9：MCP/REST/CLI 对外能力。
3. Phase 10：生态与发布（backup/restore、FTS 重建校验、视觉基线、性能基准）＋发布物统一携带使用免责声明（一切由使用者自行承担，2026-11-14 用户裁决）。
4. 收尾闭环：SESSION-HANDOFF §十二与 §十四（ChatGPT 全面审阅、GitHub 知识库与武器库补齐、实战校验三面，循环到 2.0 计划 §17 全绿）。

**在 2.0 收口之前：**

- 只允许：阅读仓库、准备和讨论本计划、完成与本计划相关的纯文档工作（且不得写入 research-platform 仓库树，见第十六部分）。
- 禁止：暂停、修改、替换、提前迁移正在运行的 2.0 工作流；在本计划名义下修改任何 crates、migrations、web、apps、workers 代码；执行任何数据库迁移；在 research-platform 仓库执行 git commit/push。
- 2.0 的既有纪律（批次入库、每阶段 CI 硬门槛、RELEASE 登记）继续按原样执行，本计划不做任何变更。

**Phase 0 在 2.0 收口后立即开始**，以收口后的真实仓库状态为审计基线。Phase 0 只做只读审计与架构文档，不做功能开发。

## 0.2 版本命名决策（延后）

RIP 2.0 收口前，本升级不建立独立的 3.0 Release 或 Session-Handoff 体系，不引入「3.0」标签到任何 RELEASE 文档。2.0 收口后，根据兼容性评估（新增实体与既有 18 个迁移、RTIS 数据面、Web 工作台的兼容成本）决定：作为 2.x 的 Phase 11+ 扩展演进，或正式升级版本号。该决策记录为 D8（见第十九部分），Phase 0 审计给出评估输入。

## 0.3 预算

暂按 3 到 4 个周末窗口规划（参考实测：2.0 的 Phase 6.5 到 8 消耗约 160M token）。窗口分配在 Phase 0 审计后重排，本文件第十四部分给出示意映射，不作为承诺。

---

# 第一部分：项目目标与基本原则

## 1.1 最终目标

将当前 RIP 从以知识收集、检索和 Agent 工具调用为主的平台，升级为完整的研究智能平台，具备以下核心能力：

1. 接收任意常见格式的研究资料（书籍、PDF、Markdown、Word、PPT、Excel、网页、代码、图片、报告）。
2. 自动识别、解析、结构化和理解资料。
3. 建立统一的文档、知识、实体和关系数据模型。
4. 自动检测重复、近似重复、错误、过时和互相矛盾的知识。
5. 支持大规模知识库的高效检索。
6. 根据研究任务动态选择知识，控制 Token 使用量。
7. 建立通用的安全研究与 SRC 漏洞挖掘工作流。
8. 当已有漏洞特征不足时，支持基于业务逻辑和安全假设的补充研究。
9. 形成研究任务、Finding、Evidence、Knowledge 和 Report 的完整闭环。
10. 自动生成专业报告，支持多种格式。
11. 允许用户方便地导入、管理、审核和维护自己的资料。
12. 支持长期扩展，不因知识量、用户量和功能数量增长而频繁重构核心架构。

最终形态：**Document Intelligence + Knowledge Intelligence + Research Intelligence + Workflow Engine + Report Intelligence**，全部生长在现有 Rust Core + Python Workers + React Web + Podman 架构之上。

## 1.2 架构原则（A 到 G，全部硬性）

**原则 A：现在建立扩展能力，而不是以后再补。** 所有核心实体从第一版设计时就考虑：数据版本、来源追踪、内容哈希、生命周期、审核状态、关联关系、软删除与恢复、索引版本、解析器版本、数据迁移。不允许先做只支持 Markdown 的简化模型再计划重构。

**原则 B：原始数据与派生数据分离。** 明确区分原始文件、解析结果、文档结构、知识分块、知识摘要、知识卡片、向量索引、实体关系、研究证据、报告。所有派生内容必须能追溯到原始文件与对应版本。

**原则 C：AI 不能成为事实数据库。** AI 可以总结、分类、关联、发现相似、提出假设、提取知识、生成报告，但 AI 生成内容必须标记为 AI 派生，保留来源、模型、提示版本、生成时间和验证状态。AI 不能把未经验证的推测变成正式知识。

**原则 D：知识数量不等于知识质量。** 所有知识必须经过来源审查、内容质量评价、重复检查、污染检查、许可证检查、时效性评价和必要的人工审核。未经批准的资料默认不能进入正式研究上下文。

**原则 E：不破坏现有架构。** 以扩展现有架构为原则，禁止为新功能推翻已有核心模块。已交付且经五道门禁与独立验收的能力（trace 六 ID 贯穿、agent-engine 执行闭环、workflow-engine、Context Broker L0-L3、podman 执行面、五区壳层）是升级的地基，不是遗留物。

**原则 F：RTIS 是漏洞研究领域的唯一生产事实源。** 不建立第二套漏洞、POC、证据等级、攻击链和研究报告事实库（详见 3.4 与第十一部分）。RTIS 生产行为零改动是红线。

**原则 G：继承优先，泛化而非重建。** 以下已交付能力必须直接复用或扩展，禁止重复开发：

| 已有能力 | 交付位置 | 在本计划中的角色 |
|---|---|---|
| Trace 六 ID（trace_id/task_id/agent_run_id/tool_run_id/artifact_id/evidence_id）、`v_trace_chain` 视图、`/api/traces` | P7-5（三栈贯穿） | Evidence 与 Report 的追踪基础 |
| Tool Contract Schema 信封 + fail-closed 校验（`$defs` 内联、双份 request/result 惯例） | Phase 1 + P7-6 | Document IR Schema 的规范模板 |
| workflow-engine（DSL、六类节点、016 到 018 迁移、checkpoint、控制面） | P8 批次 1 到 3 | 文档解析任务生命周期的执行内核，不新建队列系统 |
| Context Broker L0-L3（意图打分、预算缓存） | Phase 4 | 保留现有分层，按 6.6 扩展 |
| Podman 隔离执行面（runtime-gateway、tool pack） | Phase 2/3 | 解析器与 OCR 的隔离执行基础 |
| audit-engine correlation_id 体系 | Phase 2 + P7-5 | Token 成本与资源消耗统计的关联标识 |
| plugin-runtime source_sync（Git/GitHub/URL 同步、checksum、catalog 对账） | SK-2（三源，15,155 条对账零回归） | 统一 Document 导入管道的基础，泛化而非重建 |
| RTIS（rt.py 知识库、K0-K4 证据分级、POC/链/报告工具面） | 独立生产系统 | 漏洞研究领域唯一事实源 |

---

# 第二部分：Phase 0 架构审计（生效后第一步）

## 2.1 审计前提

1. 2.0 已收口（第零部分 0.1 清单全部完成）。
2. 只读：不改代码、不跑迁移、不装依赖、不动工作树。除写入 `docs/rip3/` 审计产物外零写入。
3. 不以文档为准：README、SESSION-HANDOFF、RELEASE 只能作为线索，结论必须来自实际代码、调用链、Schema、数据库与测试。

## 2.2 审计范围（目录清单以仓库实际结构为准）

核心 crates（逐个盘）：

- `crates/domain/`（领域模型与 IDs）
- `crates/database/`（SQLite 层与迁移执行）
- `crates/search-engine/`
- `crates/context-broker/`
- `crates/plugin-registry/` 与 `crates/plugin-runtime/`（含 source_sync）
- `crates/agent-engine/`（loop/control/memory/trace）
- `crates/workflow-engine/`（DSL/state/store/control/exec 六类节点）
- `crates/research-engine/`（七能力契约）
- `crates/tool-engine/` 与 `crates/tool-registry/`
- `crates/resource-engine/`
- `crates/runtime-gateway/`（podman 执行面）
- `crates/audit-engine/`（correlation_id 体系）
- `migrations/`（000 到 018 全量，确认下一可用编号）

应用与外围：

- `apps/server/`（rp_server.py API 面、selftest 脚本）
- `workers/python/`（各 worker 与 runtime）
- `web/src/`（五区壳层、既有页面、画布）
- `integrations/rtis/` 与 RTIS 生产侧（rt.py、知识层、K0-K4、报告工具面；按第七部分 3.4 的映射要求盘）
- `containerfiles/`（镜像台账与 digest 钉死惯例）
- `docs/phase0/PHASE-ROADMAP.md`、`docs/releases/`、`docs/SESSION-HANDOFF.md`（仅作线索）
- `.github/workflows/ci.yml`（CI 三 job 现状与硬门槛机制）

## 2.3 审计内容

**A. 数据模型**：Document 是否独立建模；Knowledge 与原始资料是否分离；是否存在 Revision；能否保存书籍章节结构；能否定位 PDF 页码与坐标；解析器版本、来源、许可证是否可记录；知识被哪些任务引用是否可追；多来源指向同一内容对象是否支持。

**B. 存储**：SQLite 表结构与索引；外部文件存储（rip-data 布局）；Artifact 生命周期；大文件处理；迁移纪律（新增式、测试副本验证）；并发写入现状（连接池上限、PRAGMA 卫生：WAL/busy_timeout 是否实际生效）；备份恢复现状（Phase 10 交付面）。

**C. 检索**：FTS5/BM25 现状；**向量检索是否真正接入统一 Search Engine，还是只存在于独立 Worker 或接口定义（用调用链与测试实证，这是审计的头号怀疑点）**；Chunking 现状；Reranking 是否存在；Context Broker L0-L3 实际行为；引用与证据回链；缓存；Token 预算。

**D. Workflow**：DSL、版本、Node Schema、Execution State、Checkpoint、Human Approval、Retry、Trace、画布（P8 交付面）、模板能力现状。

**E. 报告**：Finding/Evidence/Artifact/Trace 数据面现状；既有报告能力（RTIS redteam_report 工具面、DOC-STANDARD 模板）；导出能力现状。

**F. 对外能力**：rp_server.py 全部 API 路由清单；2.0 Phase 9 规格的 MCP/REST/CLI 计划现状；Tool Center 同源约束。

**G. 工程面**：五道门禁与 CI 三 job 的实际覆盖；RTIS 对账机制；selftest 脚本族；vendor 离线约束对 3.0 依赖引入的影响面（Rust 走增量 vendor，Python 重依赖进容器镜像）。

## 2.4 审计产物

`docs/rip3/00-ARCHITECTURE-AUDIT.md`，登记表每行必须包含：

| 项目 | 要求 |
|---|---|
| 当前实现 | 真实代码位置（path:line 级） |
| 已有功能 | 有测试或可运行验证的能力 |
| 部分实现 | 有接口但不完整，缺什么 |
| 仅有规划 | 文档存在但没有实现 |
| 设计缺陷 | 未来规模化风险 |
| 迁移风险 | 修改可能影响的模块与回归面 |
| 改进方案 | 具体实现方式与复用点 |
| 验收方式 | 如何证明完成（命令与预期输出） |

禁止把文档中的计划当成已实现功能。

## 2.5 Phase 0 冻结项（只冻结四项）

以下四项在 Phase 0 内完成设计并冻结，其余一切设计按阶段以 ADR 滚动形成（第十六部分），不提前冻结：

1. **统一领域模型及其与 RTIS 的映射**（第三部分 + 3.4 映射表定稿）。
2. **Document IR Schema**（第四部分 4.4，按 Schema 信封模式定稿 v1）。
3. **存储、CAS 与数据库迁移策略**（第十部分 + 3.3，含迁移编号规划）。
4. **安全边界与数据隔离策略**（第十一部分，含容器隔离、提示词注入防护、权限过滤前置）。

## 2.6 Phase 0 执行任务清单

| 序 | 审计对象 | 检查内容 | 预期产物 | 验收标准 |
|---|---|---|---|---|
| 1 | `crates/domain/` + `crates/database/` + `migrations/` | 实体清单、表结构、索引、迁移链完整性、下一可用编号、迁移测试覆盖 | 审计登记表 A 组；迁移编号规划草案 | 每张表有 path:line；迁移 000 到 018 逐个标注用途与测试 |
| 2 | RTIS 生产侧（rt.py、知识层、报告工具面） | 数据模型（POC/链/凭据/资产/报告）、K0-K4 语义、写入接口清单、对账机制 | **RTIS 映射表 v1**（3.4 要求的五面字段级映射） | 映射表覆盖 vuln/chain/poc/credential/report 五面；零直写生产库路径确认 |
| 3 | `crates/plugin-runtime/` source_sync + SK RELEASE | 同步管道能力边界（协议、commit 钉死、checksum、catalog）、泛化为 Document 导入的改造点 | 导入管道复用评估（差距清单） | 三源同步调用链有实测证据；泛化改造点逐条列出行号级位置 |
| 4 | `crates/search-engine/` + embedding worker + Context Broker | 向量检索接线实况（头号怀疑点）、FTS 配置、chunking、缓存、预算 | 检索现状报告；sqlite-vec 评估输入 | 向量检索「已接入/部分/未接入」有调用链实证，不接受文档说法 |
| 5 | `crates/workflow-engine/` + `crates/agent-engine/` | DSL 能力、节点类型、checkpoint/恢复、控制面、与解析管线的适配成本 | 解析管线复用设计草案（4.6 的依据） | 六类节点逐一有测试证据；解析状态机到节点类型的映射表 |
| 6 | `crates/runtime-gateway/` + `containerfiles/` + tool pack | 隔离执行能力、镜像台账惯例、资源限制面 | 解析器容器化方案草案（4.7 的依据） | 现有镜像 digest 台账惯例确认；资源限制清单齐 |
| 7 | `apps/server/` + `web/src/` + CI | API 路由全清单、壳层与画布现状、CI 覆盖、selftest 族 | API Contract 底账（第十三部分输入）；Web 改造差距清单 | 路由清单与代码一致；CI 三 job 覆盖面如实 |
| 8 | 四项冻结物起草 | 领域模型+RTIS 映射 / Document IR v1 / 存储与迁移策略 / 安全边界 | 四份文档进 `docs/rip3/` | 通过设计评审（用户或 ChatGPT 审阅流程）后才进 Phase 1 |

## 2.7 Phase 0 验收标准

1. 登记表覆盖 2.2 全部对象，无一行缺少代码证据。
2. 四项冻结物完成并通过评审。
3. 向量检索接线状态、RTIS 映射、导入管道复用点三个关键问题有实证结论。
4. 预算与窗口分配按审计结果重排并经用户确认。
5. 全程零写入（`docs/rip3/` 除外）、零迁移、零依赖安装。

---

# 第三部分：统一领域模型（含 RTIS 映射）

这是整个升级最重要的基础，必须在文档解析、知识治理、检索和报告之前完成。

## 3.1 核心实体

### Document（用户导入的原始资料）

document_id、title、document_type、original_filename、mime_type、file_size、storage_uri、sha256、owner_id、workspace_id、visibility、language、created_at、updated_at、lifecycle_status。

Document 只描述原始资料，不承担解析与知识信息。

### DocumentRevision（同一资料的不同版本）

revision_id、document_id、revision_number、content_hash、parser_version、created_at、source_version、revision_status。

必须支持：文件更新、版本比较、版本恢复、历史引用、解析结果复用。旧版本不因上传新版本被删除。

### DocumentBlock（解析后的结构化节点）

block_id、document_id、revision_id、parent_block_id、block_type、order_index、content、page_number、location_metadata、extraction_method、extraction_confidence。

block_type 至少支持：document_title、heading、paragraph、list、table、code、image、figure、formula、quote、reference、page、chapter、section。必须支持父子结构，不允许把文档压平。

### KnowledgeChunk（检索与装配用的知识分块）

chunk_id、document_id、revision_id、source_block_ids、parent_chunk_id、content_hash、content、token_count、language、chunk_strategy、embedding_status、quality_status。

必须保留原始位置映射。

### KnowledgeCard（从资料中提取的可复用知识）

knowledge_id、knowledge_type、title、summary、structured_content、confidence、quality_score、verification_status、created_at、updated_at。

knowledge_type 覆盖：漏洞原理、漏洞特征、利用条件、检测方法、修复方案、安全技术、工具使用、研究案例等。一条知识必须支持关联多个来源。

注意与 RTIS 的分工：漏洞原理/特征/修复方法等**方法论知识**属于 KnowledgeCard；具体漏洞条目、POC、证据链属于 RTIS（见 3.4）。

### KnowledgeRelation（知识关联网络）

关系至少包括：SAME_CONTENT、NEAR_DUPLICATE、RELATED_TO、SUPPORTS、CONTRADICTS、SUPERSEDES、FIXES、DETECTS、DERIVED_FROM、REFERENCES。

每条关系必须有独立的关系证据、关系来源和置信度。

### Finding / Evidence / ResearchTask / Report

领域对象保留，但其与 RTIS 的关系按 3.4 执行：Finding 与 Evidence 的**漏洞事实内容**以 RTIS 为准，3.0 侧重编排、追踪与渲染。

## 3.2 实体关系

Document 1:N DocumentRevision；DocumentRevision 1:N DocumentBlock；DocumentRevision 1:N KnowledgeChunk；Document N:M KnowledgeCard；KnowledgeCard N:M KnowledgeCard；ResearchTask 1:N Finding；Finding 1:N Evidence；ResearchTask 1:N Report；Report N:M Finding；Report N:M Evidence。

所有关系必须有稳定 ID，禁止用标题或文件名作关联依据。

## 3.3 统一 ID 与迁移策略

必须定义：全局 ID 规范（沿用 `domain::ids.rs` 既有域类型惯例）、ID 不可变规则、数据版本规则、关系完整性规则、数据删除规则（软删除优先）、迁移规则。

数据库迁移全部采用新增式：只新增编号文件，在测试副本验证后合入；不删除 RTIS、Knowledge Catalog 和 Plugin Runtime 的既有数据表；对旧实体建立 Adapter 或兼容视图逐步迁移；保留历史 ID 与新 ID 的映射。迁移编号从审计确认的下一可用编号起（当前 018 已占用，预期从 019 起，以审计为准）。

## 3.4 RTIS 映射（强制产出）

RTIS 是漏洞研究领域的唯一生产事实源（原则 F）。Phase 0 必须产出**字段级映射表**，覆盖五面：

| 3.0 领域对象 | RTIS 对应面 | 映射规则（Phase 0 定稿） |
|---|---|---|
| Finding | RTIS 漏洞/finding 面 | 3.0 ResearchTask 产出的漏洞结论必须落 RTIS；3.0 库内只存引用与状态，不复制漏洞细节 |
| Evidence | RTIS evidence 面 + 3.0 Artifact | 原始证据（请求响应、截图、log）以 3.0 Artifact 承载并按六 ID 关联；结论性证据状态以 RTIS 为准 |
| ResearchTask | RTIS run/任务面 | 3.0 ResearchTask 是编排容器（工作流 run、覆盖矩阵、假设清单），安全结论经既有接口落 RTIS |
| Attack Chain | RTIS 攻击链面 | 链路事实只存 RTIS；3.0 可视化与渲染引用之 |
| Report | RTIS 报告工具面 | 报告事实数据（漏洞、证据、链）引用 RTIS；3.0 Report Engine 负责结构化、渲染与导出 |

配套硬性规则：

1. **零直写**：3.0 一切对 RTIS 的读写必须走 RTIS 既有接口（rt.py CLI / rtis MCP 工具面），禁止直连 RTIS 生产库。
2. **零行为变更**：RTIS 的 schema、接口签名、生产行为零改动；3.0 对 RTIS 的依赖只增不改。
3. **细节不落库**：漏洞细节（POC 正文、利用细节）只存在于 RTIS 与授权提交平台，不进 3.0 的文档知识库，也不进公开仓库。
4. **对账延续**：既有 RTIS 对账机制（如 SK 15,155 条零回归口径）继续作为回归红线。

---

# 第四部分：Document Intelligence（文档智能系统）

## 4.1 系统定位

新增独立子系统，负责：文件导入、格式识别、文档解析、OCR、结构恢复、元数据提取、章节识别、图片提取、表格提取、解析质量检测、统一文档表示生成。

分工沿用现有架构：Rust Core 负责调度、生命周期和领域对象；Python Worker 负责重量级解析；**workflow-engine 作为解析任务生命周期的执行内核**（状态机、checkpoint、恢复、重试全部继承，不新建队列系统，见 4.6）；Podman 负责隔离执行。

## 4.2 格式支持

第一批：

| 格式 | 处理方式 |
|---|---|
| PDF | 文本、页码、布局、表格、图片 |
| EPUB | 章节、目录、正文 |
| Markdown | 标题、代码、链接、表格 |
| DOCX | 标题、段落、表格、图片 |
| PPTX | 幻灯片、文本、图片、备注 |
| XLSX | 工作表、单元格、表格结构 |
| HTML | 正文、标题、链接、结构 |
| TXT | 编码识别、正文 |
| PNG/JPG | OCR、图片说明 |
| ZIP | 安全解包、批量导入 |

后续预留：RTF、XML、CSV、LaTeX、邮件文件、视频字幕、音频转录、Jupyter Notebook。

不允许无限增加互不兼容的解析器：所有解析器统一输出 Document IR（4.4）。

## 4.3 解析器选型与约束

候选：Docling（通用文档解析）、Tesseract（OCR 兜底）、PaddleOCR（中文 OCR，条件引入）、PyMuPDF（PDF 页面/文本/坐标辅助）、EPUB 专用解析库、Pandoc（转换与报告输出）。

正式引入前逐项评估：维护状态、许可证、依赖体积、CPU 与内存消耗、GPU 是否必需、离线运行能力、中文支持、容器兼容性、原始位置保留能力、与 Document IR 的适配成本。不为使用知名项目而强行引入全部依赖。

**OCR 硬性约束**：优先 Docling；Tesseract 兜底；PaddleOCR 只有在中文真实语料实测不达标时才引入。所有 OCR 与解析器一律运行在隔离容器中（4.7）。

**依赖引入约束**：Rust 新依赖走增量 `cargo vendor` 保 `--offline`；Python 重依赖（Docling、PyMuPDF 等）只进 worker 容器镜像（镜像属运行时数据，沿用既有 ADR §3.2 口径与 digest 台账惯例），宿主机零安装。

## 4.4 Document IR 统一格式

**规范模板：沿用 Tool Contract 与 P7-6 的 Schema 信封模式**（`$defs` 内联自包含、fail-closed 校验、schema_version 显式、禁用宽松默认），不另起规范风格。

DocumentIR 结构：

- schema_version
- document_metadata
- parser_metadata（解析器标识与版本）
- structure（章节树）
- blocks（结构化节点，含父子与顺序）
- assets（图片/表格等提取物清单与哈希）
- references（引用清单）
- extraction_quality（质量自检结果）
- source_mapping（到原始文件位置的双向映射）

每个 Block 必须能关联：所属文档、所属版本、父级章节、页码或原始位置、原始内容、规范化内容、解析方式。

必须提供 Schema 版本迁移机制：修改 JSON 结构不得导致旧解析结果不可读（旧版本 IR 以 schema_version 路由到对应读取器）。

## 4.5 书籍专门处理

书籍必须保留完整层级：Metadata、TableOfContents、Part、Chapter、Section、Subsection、Paragraph、Figure、Table、References。

解析完成后可生成：全书摘要、章节摘要、小节摘要、核心知识卡片、术语表、工具索引、引用索引、章节关联。所有摘要必须能反向定位原始章节和页码。

不允许为了让 AI 回答一个小问题而加载整本书（分层检索见第六部分）。

## 4.6 解析任务生命周期（workflow-engine 实现）

状态机：DISCOVERED、VALIDATING、QUEUED、PARSING、NORMALIZING、EXTRACTING、QUALITY_CHECK、REVIEW_PENDING、APPROVED；失败态：FAILED、CANCELLED、QUARANTINED。

实现约束：

1. 状态机与阶段跃迁定义为 workflow 定义（DSL + 节点），复用既有 checkpoint 与断点恢复，服务重启不丢任务。
2. 重试、幂等（以 content_hash 为幂等键）、解析器升级后重解析、失败原因记录、资源限制、优先级全部映射到 workflow-engine 既有控制面（017/018 迁移已交付的能力），缺什么在 ADR 里补，不重写内核。
3. Phase 0 审计任务 5 产出「解析状态机到节点类型的映射表」作为本节实施依据。

## 4.7 安全要求（不可信输入）

所有外部资料视为不可信输入：容器隔离；默认禁止网络；限制 CPU/内存/磁盘；限制解压文件数量与递归深度；压缩炸弹检测；路径穿越防护；真实格式检查（magic bytes，不信扩展名）；OCR 与解析超时控制。

**解析出的文本不是 Agent 系统指令**：外部资料一律视为数据（数据通道），任何来自文档正文的内容不得进入系统提示或工具指令位；与第五部分污染防护衔接。

---

# 第五部分：Knowledge Governance（知识治理系统）

## 5.1 系统定位

新增统一 Knowledge Governance Engine，负责：精确去重、近似去重、语义重复检测、冲突检测、质量评分、许可证检查、时效性评价、来源可信度、污染隔离、审核工作流。

**作用域（硬性）**：治理引擎作用于 3.0 导入的文档语料与 KnowledgeCard（第三部分实体面）。RTIS 知识面（POC 库、K0-K4、POC 去重）**不重建、不改行为**，只做对账与引用（3.4 规则 4）。两面的接续点：3.0 的方法论类 KnowledgeCard 可经审核后按 RTIS 既有接口登记为 RTIS 知识条目；反向，RTIS 条目可作为高质量来源进入 3.0 检索。

## 5.2 五级去重

**D1 原始文件去重**：SHA256，相同原始文件只创建一个内容对象；不同来源保存独立 Source Record。

**D2 标准化正文去重**：规范化规则（统一换行与编码、去无意义空白、处理页眉页脚、保留代码与关键标点、保留原始版本），规范化哈希识别正文重复。

**D3 近似重复**：评估 MinHash、SimHash、Shingles、LSH，用于复制漏洞报告、重复 POC、跨仓库相同文档、微改重复的检测。

**D4 语义重复**：Embedding 做候选发现。**语义相似不得直接触发自动合并**：漏洞原理、POC、修复方案语义相关但不是重复知识。

**D5 知识冲突**：不同版本结论、不同测试条件、不同影响范围、过期修复、矛盾建议，建立 CONTRADICTS 关系，进审核而不是直接删一方。

## 5.3 知识质量模型

独立 Knowledge Quality Score，初始权重（可配置）：来源可靠性 20%、内容完整度 15%、可验证性 20%、时效性 15%、独立性 15%、技术准确性 15%。

硬性风险标记独立于总分：UNTRUSTED_SOURCE、LICENSE_UNKNOWN、POSSIBLE_PROMPT_INJECTION、OUTDATED、CONTRADICTED、LOW_EXTRACTION_CONFIDENCE、UNVERIFIED_CLAIM。不能只凭总分判定可信。

## 5.4 审核状态

统一状态：DISCOVERED、PARSED、CANDIDATE、APPROVED、QUARANTINED、REJECTED、DEPRECATED。

必须支持：自动初筛、人工审核、审核原因、审核历史、状态回退、批量审核、来源独立审核、版本重新审核。未经批准的知识默认不能进入普通研究上下文。

## 5.5 内容复用与来源分离

Content Object 与 Source Record 必须分离：同一文件来自三个仓库时，内容对象复用一份，三个来源、三份许可证检查、三个上游版本全部独立保留。绝不因文件相同丢失来源信息。

## 5.6 知识污染防护

所有外部内容经安全过滤：隐藏提示词、恶意指令、伪造系统提示、未经验证的利用结论、恶意代码、失效 POC、误导性修复方案。

AI 可以提供风险判断，但正式隔离与审核必须有可追溯判定依据（判定依据本身也是审计对象）。

---

# 第六部分：检索与上下文（统一 Search Engine）

## 6.1 架构

建立统一 Search Engine 接口，所有知识查询统一经过该接口；禁止各 Agent 自连数据库或私建检索逻辑。

## 6.2 Hybrid Search

组合：FTS5/BM25、向量检索、元数据过滤、结构化实体查询、知识关系查询、Reranking。

流程：Query → Query Understanding → Metadata Filter → Keyword Search → Vector Search → Candidate Merge → RRF → Reranking → Deduplication → MMR → Context Broker。

向量索引必须支持：Embedding Model ID、Embedding Version、Vector Dimension、Rebuild、Incremental Update、Index Version、Model Migration。禁止把 Embedding 模型写死在业务代码里。

**Embedding 供应约束**：优先容器内本地模型（保离线能力）；外部 API 只能作为可选后端且必须显式配置才启用。

## 6.3 存储选型

第一阶段继续 SQLite + FTS5。向量检索优先评估：sqlite-vec、仓库既有向量实现（以 Phase 0 审计任务 4 的实证结论为准）、独立向量服务（仅作为未来可选方案）。

必须以真实基准测试选择；不允许因为「未来资料量可能很大」现在就引入分布式基础设施。同时定义统一 Vector Backend 接口，后续换后端不影响 Knowledge Domain 与 Agent。

## 6.4 结构化检索过滤

必须支持：文档类型、知识类型、来源、作者、发布时间、许可证、漏洞类型、CWE、CVE、技术栈、编程语言、产品版本、质量状态、审核状态、更新时间。

## 6.5 书籍检索

支持整本书、章节、小节检索，页码定位，上下文扩展，相邻章节查找。结果必须包含：文档名、章节名、页码、内容片段、来源、版本、引用 ID。

## 6.6 Context Broker 升级

保留现有 L0-L3 分层与预算缓存设计。增加：Query Cache、Retrieval Cache、Summary Cache、Tokenizer Adapter、Context Deduplication、Evidence Coverage、Dynamic Budget Allocation。

分层语义：L0 系统规则与任务约束；L1 知识目录与任务知识地图；L2 重要知识摘要与核心结论；L3 真正需要的原文、代码、证据与详情。按任务复杂度动态分配 Token。

## 6.7 Token 成本治理

每次 Agent 调用记录：Model ID、Input Tokens、Output Tokens、Cached Tokens、Retrieval Tokens、Context Tokens、Knowledge Items Used、Retrieval Latency、Cache Hit、Estimated Cost。**以 audit-engine correlation_id（含 trace 六 ID 体系）为关联标识**，提供任务级与项目级统计，支持 Token 预算与超额停止。

不允许只靠缩短文本降成本（可能损失关键证据）。

## 6.8 检索质量评估

建立独立 Retrieval Benchmark。**测试集优先从 RTIS 真实语料播种**（经筛选的真实查询、POC 条目、研究案例），辅以固定合成集。覆盖：精确关键词、语义查询、多语言查询、书籍章节查询、漏洞案例查询、版本差异查询、冲突知识查询。

记录 Recall@K、Precision@K、MRR、NDCG、Latency、Token Cost。一切检索优化必须通过固定测试集比较。

## 6.9 SQLite 运行约束（硬性）

明确并实测生效：WAL 模式；单写者纪律（与既有连接池上限口径一致）；busy_timeout 显式设置；定期 checkpoint；PRAGMA 实际生效检查写入自检脚本（Phase 0 审计任务 1 出现状基线）。

---

# 第七部分：研究工作流体系

## 7.1 执行内核复用（硬性）

不重新开发工作流执行内核。继续使用现有：Workflow DSL、Node Schema、Execution State、Checkpoint、Trace、Human Approval（P8 批次 1 到 3 交付面，阶段收口进行中）。新增能力（模板、新节点类型）以扩展节点注册与 DSL 表达的方式实现。

## 7.2 Workflow Studio

在 2.0 Phase 8 已交付的画布与壳层上收尾与扩展（P8-4 画布批 + 后续增强），三栏布局：左侧节点库、中间流程画布、右侧节点配置，底部运行状态/日志/Evidence/Artifact。

节点类型至少支持：Start、End、Knowledge Search、Knowledge Analysis、Tool Execution、Agent Execution、Condition、Parallel、Human Approval、Evidence Collection、Finding Creation、Report Generation。

节点参数经 JSON Schema 自动生成。支持：拖拽、连线、参数配置、节点校验、保存、版本管理、导入导出、单节点调试、运行轨迹、失败重试、暂停恢复。

循环必须是受控节点：最大迭代次数、超时、预算三限制缺一不可。

## 7.3 通用渗透测试工作流模板

模板十三步：目标范围确认、资产清点、技术栈识别、知识检索、威胁建模、特征检测、人工或 Agent 分析、受控漏洞验证、Evidence 收集、Finding 创建、风险评价、修复建议、报告生成。

（2026-11-14 用户裁决：项目定位是工具，与 nmap、sqlmap 等渗透测试工具同一使用形态：运行与测试不需要书面授权或许可，不内置授权确认与授权范围节点；法律顾虑以**使用免责声明**处理，发布物统一携带「本工具仅供安全研究与学习，使用产生的一切后果由使用者自行承担」，声明文案的落点由用户最终定稿。）

## 7.4 SRC 漏洞挖掘工作流模板

模板十三步：收集目标公开资料、识别资产与业务功能、分析技术栈、检索相似漏洞案例、建立测试覆盖矩阵、执行低影响检测、分析业务逻辑与权限边界、建立安全假设、受控验证、记录证据、评估影响、查重（避免重复提交）、生成 SRC 报告。

产出落点遵循 3.4：Finding 与报告事实经 RTIS 接口落库。

## 7.5 自适应安全研究引擎

解决「特征库不完整、扫描器无命中、知识库无直接案例」时的研究继续问题。

**A. 测试覆盖检查**：资产、API、身份与权限、输入点、业务流程、文件处理、数据流七维覆盖。

**B. 假设生成**：结合系统结构、技术栈、业务功能、既有漏洞知识、历史 Finding、覆盖缺口，生成可验证假设。

**C. 假设验证**：每条假设必须有内容、来源依据、验证条件、预期结果、负向对照、实际结果、Evidence、最终状态。

**D. 结果分类（硬性）**：CONFIRMED、NOT_REPRODUCED、NOT_TESTED、INCONCLUSIVE、FALSE_POSITIVE 五态。**禁止把未测试或未复现描述为没有漏洞**。

**E. 覆盖矩阵**：每任务生成 Coverage Matrix，记录测试范围、已覆盖、未覆盖、失败区域与限制。不依赖扫描器命中数评价测试完整性。

---

# 第八部分：Report Intelligence（报告系统）

## 8.1 Report Domain

报告不是 Agent 直接输出 Markdown。建立结构化 Report Domain，内容由 ResearchTask + Finding + Evidence + Artifact + Workflow Trace + Knowledge References 组成；其中漏洞与证据的**事实内容引用 RTIS**（3.4），3.0 负责结构化、渲染与导出。

## 8.2 报告类型

**A. SRC 漏洞报告**：标题、类型、风险等级、位置、影响范围、前置条件、原理、复现步骤、请求响应、实际证据、影响说明、修复建议。

**B. 专业渗透测试报告**：封面、目录、执行摘要、授权范围、测试方法、资产清单、时间线、覆盖率、风险分布、Finding 清单、详细分析、攻击路径、系统架构、修复方案、整改优先级、回归测试、附录。

**C. 管理层报告**：业务影响、风险概览、关键问题、风险趋势、修复优先级、资源建议。

## 8.3 漏洞通俗解释

每个 Finding 支持两种表达：技术解释（供安全工程师复现分析）、通俗解释（供无安全背景读者）。通俗解释至少回答七个问题：这是什么、正常系统应如何工作、哪里出了问题、攻击者需要什么条件、可能造成什么后果、为什么现有防护没拦住、应该怎样修复。

不允许只把术语换简单词，必须用具体场景解释漏洞发生机制。

## 8.4 Diagram Engine

独立图表模块，支持：Mermaid Flowchart、Sequence Diagram、Architecture Diagram、Attack Path、Data Flow Diagram、Network Topology、Before/After 对比。

所有图表来自结构化的 Finding、Evidence、Workflow 与系统数据（攻击路径事实以 RTIS 攻击链为准），禁止 AI 随意生成未经验证的攻击关系。图表支持：图表 ID、版本、数据来源、Evidence 关联、导出、渲染校验。

## 8.5 导出格式与 Report IR

第一批：PDF、DOCX、HTML、Markdown、JSON。第二批：PPTX、XLSX、自定义模板。

统一 Report IR，不同格式用不同 Renderer；**禁止每种格式重新调用 AI 生成一遍**（内容不一致风险）。

## 8.6 导出质量检查

每次正式导出自动验证：章节完整、图片存在且未损坏、目录正确、页码正确、中文字体正确、代码块完整、表格不溢出、引用可追溯、Finding 与 Evidence 对应、图表与内容一致。导出失败保留错误信息并允许重新导出。

---

# 第九部分：Knowledge Center（用户资料管理）

## 9.1 导入方式（复用 source_sync 泛化，硬性）

**现有 plugin-runtime source_sync（Git/GitHub/URL 同步、commit 钉死、checksum、catalog 对账）是导入管道的基础，本升级是把它泛化为统一 Document 导入管道，禁止重新开发一套导入系统。** Phase 0 审计任务 3 给出差距清单。

导入方式：单文件上传、多文件上传、文件夹导入、ZIP 导入、Git 仓库、GitHub、网页 URL、Markdown 仓库、公开知识源。目录监听与定时同步作为后续增强（经 workflow 定时触发实现，不新建调度器）。

## 9.2 导入界面

上传后必须先显示：文件名、类型、大小、页数或章节数、格式识别结果、解析状态、重复检查结果、来源信息、许可证状态、预计处理资源。

## 9.3 解析预览

入库前可预览：目录、章节、正文、表格、图片、代码、OCR 结果、原始页码、解析质量。

## 9.4 分类与权限

支持 Workspace、Project、Category、Tags、Private、Shared、Public。私有资料不得默认进入公共知识检索；跨项目检索必须经权限检查（权限过滤在检索入口前置执行，见第十一部分）。

## 9.5 管理操作

查看资料、修改分类/标签/描述、更新资料、查看历史版本、重新解析、删除、恢复、查看引用记录、查看质量审核结果。

删除前检查是否仍被 Finding、Report 或历史任务引用；默认可恢复删除，不得直接清理历史证据。

---

# 第十部分：统一存储与扩展架构

## 10.1 数据分层

**A. Metadata Store**：Document/Knowledge/Source/Revision/Relation/Workflow/Finding/Evidence Metadata/Report Metadata。第一阶段继续 SQLite。

**B. Object Storage**：PDF、EPUB、DOCX、原始与提取图片、解析产物、报告文件、大型 Artifact。统一 Object Storage Adapter：第一阶段本地文件系统（沿用 rip-data 布局惯例），预留 S3 兼容接口。业务代码禁止直接拼接文件系统路径。

**C. Search Index**：FTS、Vector、Metadata、Search Cache，全部支持独立重建。

**D. Cache**：Document Parsing、Embedding、Summary、Retrieval、Context，全部可失效可重建。

## 10.2 内容寻址存储（CAS）

原始内容按内容哈希寻址：不同来源共享同一对象，来源关系独立。必须设计：内容对象、来源引用计数、版本引用、删除保护、孤儿对象清理、数据完整性检查。禁止因删除一个来源误删其他来源仍在用的对象。

## 10.3 Provider 接口（防供应商锁定）

以下能力必须有独立接口，领域层不写死任何供应商/模型/存储实现：DocumentParser、OCRProvider、EmbeddingProvider、RerankerProvider、VectorStore、ObjectStorage、ReportRenderer、DiagramRenderer、KnowledgeExtractor。

## 10.4 规模化准备

从现在开始遵循：分页读取、游标分页、批量写入、增量索引、异步任务、幂等键、任务重试、资源限额、软删除、数据版本、索引版本、任务优先级、备份恢复。SQLite 阶段就按这些约束实现（含 6.9 运行约束）。

未来迁移 PostgreSQL 或独立服务的路径必须明确，但**不允许现在为想象中的规模引入分布式系统**。

---

# 第十一部分：安全、权限与数据治理

安全设计是基础能力，不是最后补充。

## 11.1 资料安全

不可信文件隔离、文件格式检查、压缩包安全检查、OCR 资源限制、解析器容器隔离、提示词注入防护（4.7 数据通道原则）、恶意代码隔离、文件访问权限。

## 11.2 研究执行安全

工具权限、执行超时、请求频率限制、网络访问限制、操作审计。（2026-11-14 用户裁决：授权范围管理与人工审批不在平台内置，如需由用户自行添加。）

## 11.3 Evidence 安全

Evidence 必须有：Evidence ID、所属 Finding、来源任务、采集时间、原始 Artifact、SHA256、采集方式、验证状态。追踪基础为 trace 六 ID（原则 G）。历史证据不可被普通知识更新覆盖。

## 11.4 数据权限

所有知识检索在**入口处前置执行权限过滤**，禁止检索完成后才过滤私有资料。防护：跨项目泄露、私有资料进入他人上下文、未授权文件被导出、Agent 访问未授权 Artifact。

## 11.5 RTIS 红线（重申）

RTIS 生产行为零改动；零直写生产库；漏洞细节不进 3.0 库与公开仓库（3.4 规则）。违反即返工。

---

# 第十二部分：Web UI 改造

不重新设计整个 Web 工作台，在现有 React 架构与五区壳层上扩展。统一组件、统一 Loading/Empty/Error 状态、避免重复实现表格与编辑器；支持大资料分页、异步任务进度、取消与重试；桌面端优先，移动端提供合理的查看与管理能力。

新增/升级页面族：

- **Knowledge Center**：Overview、Documents、Knowledge Cards、Sources、Import Queue、Review Queue、Duplicate Review、Quality Dashboard、Knowledge Relations、Storage & Index Status。
- **Workflow Studio**：Canvas、Node Library、Node Configuration、Execution Logs、Run History、Templates、Version History（2.0 Phase 8 画布交付为起点）。
- **Research Workspace**：Research Tasks、Task Planning、Knowledge Context、Test Coverage、Findings、Evidence、Execution Trace。
- **Report Studio**：Report List、Report Editor、Finding Library、Diagram Editor、Template Manager、Export History。

---

# 第十三部分：对外能力：MCP/REST/CLI（继承 2.0 Phase 9）

**硬性约束：MCP、REST、CLI 是 3.0 整体架构的一等公民，继承 2.0 Phase 9 的对外能力设计，不得重新建立重复接口体系。**

1. 同源原则：MCP 与 REST 共用同一套 API Contract、Application Services 与 Schema；MCP 工具面与 Tool Center 同源（沿用 2.0 Phase 9 验收口径）。
2. 3.0 新增的领域能力（文档导入、治理审核、报告导出）以**扩展既有 API Contract** 的方式对外暴露，不另起第二套接口风格。
3. correlation_id / trace 六 ID 在对外接口上透出（Phase 9 既定验收项），3.0 新端点沿用同一惯例。
4. CLI（rp-* 命令族）按同一 Application Services 扩展子命令。
5. Phase 0 审计任务 7 产出 API Contract 底账作为本部分实施输入。

---

# 第十四部分：实施阶段与执行顺序

**前置：以下阶段划分是实施顺序，不代表前期可以不做后续架构设计；但只有第零部分 2.5 列的四项允许 Phase 0 冻结，其余按阶段 ADR 滚动。**

前置前置：2.0 收口（第零部分）是 Phase 0 的启动条件。

| Phase | 目标 | 关键交付 | 验收要点 |
|---|---|---|---|
| **0** | 架构审计与四项冻结 | 00-ARCHITECTURE-AUDIT.md、领域模型+RTIS 映射、Document IR v1、存储/CAS/迁移策略、安全边界；窗口分配重排 | 2.7 全条；设计评审通过 |
| **1** | Document Domain & Storage | Document/Revision/Block/Content Object/Source Record/Object Storage Adapter、版本管理、导入 API、基础上传 UI | 同文件多来源引用；内容不重复存储；更新生成新版本；历史可访问；删来源不误删；内容与元数据可独立备份 |
| **2** | Document Intelligence | Parser Registry、Docling Adapter、第一批格式解析器、OCR Adapter、Document IR 落库、解析质量检查、解析管线（workflow-engine 实现） | 多格式统一入库；结构正确；位置可追踪；失败可重试；解析器版本可记录；与业务 API 隔离 |
| **3** | Knowledge Governance | D1 到 D5 去重、Knowledge Relation、Quality Score、Review Queue、Quarantine、License Review | 跨来源重复可识别；不同知识类型不误合并；来源关系完整；低质量不进正式上下文；审核可追溯；RTIS 对账零回归 |
| **4** | Hybrid Search & Context Intelligence | 统一 Search API、BM25+向量+RRF+Reranker+MMR、Query/Summary Cache、Tokenizer Adapter、Context Broker 升级 | 多格式统一搜索；书籍页码章节准确返回；未审核资料可过滤；过 Benchmark；预算有效；缓存可失效重建 |
| **5** | Workflow Studio & Security Research | 画布增强、Node Schema、Workflow Templates（渗透/SRC）、自适应研究引擎、Coverage Matrix、Human Approval 完整化 | 可视化编辑；版本保存；恢复执行；节点日志；未测试项管理；无命中进入补充研究 |
| **6** | Report Intelligence | Report Domain/IR、模板、Diagram Engine、PDF/DOCX/HTML/Markdown/JSON Renderer | 多格式内容一致；结论可追溯；图表关联 Evidence；中文正确；导出质量自动检测；通俗+技术双解释 |
| **7** | Knowledge Feedback & Maintenance | Finding 到候选知识、Evidence 溯源、上游更新监测、失效链接检查、旧 POC 检查、自动质量巡检、维护报告 | 研究结果可转化为候选知识并经审核入库；重复知识复用；历史不被无故覆盖；上游变化可追踪 |
| **8** | 规模测试与基准（最后一个实施窗口） | 1k/10k/100k 文档与百万级 Chunk 合成语料、导入/索引/检索/内存/存储/并发/迁移/重建/备份恢复基准报告 | 各规模档位有真实数据与基准报告；瓶颈结论以数据支撑迁移决策 |

窗口示意映射（以 Phase 0 审计重排为准）：窗口 1 = Phase 0（+视余量启动 Phase 1）；窗口 2 = Phase 1+2；窗口 3 = Phase 3+4；窗口 4 = Phase 5+6；Phase 7+8 视前四窗实际进度排入后续窗口。每个窗口结束执行一次阶段收口（入库 + CI 硬门槛 + RELEASE 登记，见第十五部分）。

---

# 第十五部分：工程质量与测试要求

不能以「接口返回 200」「页面能打开」判断完成，每阶段真实验证。

## 15.1 门禁（对齐仓库既有纪律）

每阶段执行：Format、Lint、Type Check、Unit Tests、Integration Tests、Migration Tests、Security Tests、Build、Performance Tests、Regression Tests。

与仓库既有门禁并集执行：`cargo test --offline --workspace`、`cargo clippy --offline --workspace --all-targets -- -D warnings`、`python3 -m unittest discover -s workers/python/tests`、`cd web && npm run build`、`cd web && npm run test`，外加各阶段 selftest 脚本。**阶段入库 push 后以 GitHub CI 全绿为硬门槛，红不进下一阶段**（沿用 2.0 既定机制）。

测试失败不得通过删除测试或降低验收标准解决。

## 15.2 端到端集成测试

覆盖全链：Document Upload → Parsing → Document IR → Knowledge Extraction → Deduplication → Approval → Indexing → Retrieval → Context Assembly → Research Task → Finding → Evidence → Report → Export。

## 15.3 真实文档测试集

固定测试资料库：普通 PDF、扫描 PDF、多栏 PDF、有目录 PDF、大型 PDF、EPUB、中文 Markdown、DOCX、PPTX、XLSX、带图片文档、代码文档、损坏文件、加密文件、重复文件、近似重复文件。建立解析正确率与性能基准。

## 15.4 规模测试

1k、10k、100k 文档与百万级 KnowledgeChunk 索引压力测试，**明确排在最后一个实施窗口（第十四部分 Phase 8）**，是测试目标不是提前部署要求。必测：导入速度、索引构建、检索延迟、内存占用、存储增长、批量任务、并发访问、数据迁移、索引重建、备份恢复。SQLite 出现瓶颈时，必须以真实数据与基准报告支撑迁移决策。

## 15.5 回归测试（每次修改必过）

旧版知识记录可读；旧版 Plugin 正常；**RTIS 对账零回归**；Agent 端到端正常（含 trace 六 ID 关联查询）；Context Broker 正常；Workflow 执行状态完整；既有报告模板可用；迁移重放成功；selftest_auth 11 项不回归。

---

# 第十六部分：文档与工程交付

## 16.1 文档树

全部架构设计入仓库 `docs/rip3/`：

```
docs/rip3/
├── 00-ARCHITECTURE-AUDIT.md
├── 01-PRODUCT-REQUIREMENTS.md
├── 02-SYSTEM-ARCHITECTURE.md
├── 03-DOMAIN-MODEL.md（含 RTIS 映射表）
├── 04-DOCUMENT-IR-SCHEMA.md
├── 05-DOCUMENT-INTELLIGENCE.md
├── 06-KNOWLEDGE-GOVERNANCE.md
├── 07-HYBRID-SEARCH.md
├── 08-CONTEXT-INTELLIGENCE.md
├── 09-WORKFLOW-STUDIO.md
├── 10-SECURITY-RESEARCH.md
├── 11-REPORT-INTELLIGENCE.md
├── 12-STORAGE-AND-MIGRATION.md
├── 13-SECURITY-AND-PRIVACY.md
├── 14-API-CONTRACTS.md（含 MCP/REST/CLI 继承说明）
├── 15-TESTING-STRATEGY.md
├── 16-IMPLEMENTATION-ROADMAP.md
├── 17-ACCEPTANCE-CRITERIA.md
└── decisions/
```

## 16.2 本文件的入库时机

2026-11-14 起：本文件已入库为 `docs/rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md`（提前入库以保障跨机续作 clone 可得；原「2.0 收口后再入库」的安排就此提前，内容以仓库内版本为唯一权威）。修订版同步副本保留在仓库外工作区（`/home/emo/code/pensting/docs/rip3/`），以仓库内版本为准。

## 16.3 ADR 纪律

重要架构决定必须写 ADR（decisions/），至少记录：背景、问题、候选方案、最终选择、选择原因、成本、风险、迁移方式、放弃的方案。除 Phase 0 四项冻结物外，其余设计按阶段以 ADR 滚动形成，不提前冻结全部数据模型。

## 16.4 版本命名

按 0.2 执行：2.0 收口前不建独立 3.0 发布体系；收口后以兼容性评估决定 2.x Phase 11+ 或正式 3.0（决策 D8）。

---

# 第十七部分：AI Agent 执行规则

## 17.1 先审计，不直接写代码

第一步检查当前主分支与实际代码，禁止按本计划直接创建大量新模块；先找已有能力避免重复实现（继承清单见原则 G）。

## 17.2 先统一架构，再开发功能

必须先完成（限 2.5 四项）：领域模型与 RTIS 映射、Document IR Schema、存储与迁移策略、安全边界。设计评审通过后才开始实施。

## 17.3 禁止擅自降低目标

以下行为一律禁止：只支持 Markdown；文档压平纯文本；字符串相似度代替知识治理；只建 Embedding 不做混合检索；AI 直读整个知识库；报告实现为一次 LLM 生成；静态图片代替可编辑画布；把未发现漏洞描述为不存在漏洞；未审核资料默认进入 Agent 上下文；删历史数据解决迁移问题；把独立领域实体合并进一张大表；**绕过 RTIS 建立第二套漏洞事实库**；**为导入功能重建 source_sync 已有能力**。

## 17.4 最小可审计修改

每次只做一个有明确目标的任务：检查影响范围、优先新增兼容接口、保留旧接口、写迁移、加测试、验证实际功能、更新文档。不允许无关重构。

## 17.5 Git 纪律（双轨制，硬性）

**手动交互会话**：不擅自 commit/push/reset/checkout/clean/stash/rebase；不覆盖用户未提交修改；发现工作区已有修改必须保护；只有用户明确要求时才执行提交推送或其他破坏性 Git 操作。

**自动化工作流批次**：继续遵守 2.0 既定纪律，每阶段由入库员执行 add/commit/push，push 后以 CI 全绿为硬门槛，SSH 间歇断连重试 2 到 3 次。两轨不得互相覆盖：工作流运行期间，手动会话不插手 git 状态。

## 17.6 阶段交付格式

每完成一个阶段输出：A 完成内容；B 架构变化；C 文件变化；D 数据库变化（Migration/索引/兼容方式）；E 测试结果（命令、结果、失败原因）；F 性能结果（内存/耗时/存储/Token）；**E2 CI 结论（各 job 结论与运行链接）**；G 遗留问题（不隐藏）；H 下一阶段依赖与顺序。

---

# 第十八部分：最终验收场景

## 场景 A：导入一本安全研究书籍

上传 EPUB/PDF → 识别格式 → 解析目录章节 → 提取正文图片表格 → 生成 Document IR → 章节级知识索引 → 章节摘要 → 知识卡片 → 重复与质量检查 → 用户审核 → 进入检索系统。用户提问可准确定位章节与页码。

## 场景 B：导入大量公开安全知识

批量导入 GitHub 仓库、公开 POC、漏洞报告、Markdown 知识库 → 统一解析 → 跨来源重复检查 → 失效过时检查 → 许可证检查 → 建立漏洞/工具/知识关系 → 生成审核队列。同一资料不重复存储，所有来源保留。方法论知识可经审核登记进 RTIS（3.4 接续点）。

## 场景 C：执行 SRC 研究任务

选择 SRC 工作流 → 检索相关知识 → 建立测试计划 → 生成覆盖矩阵 → 特征检测 → 检查未覆盖区域 → 生成安全假设 → 受控验证 → 记录 Evidence → 创建 Finding（经 RTIS 落库）→ 生成报告。**没有检测到漏洞时，系统仍能说明测试覆盖情况与未验证区域**（7.5 五态纪律）。

## 场景 D：生成专业报告

选择任务与模板 → 汇总 Finding → 关联 Evidence → 技术解释 + 通俗解释 → 架构图/流程图/风险统计 → 导出 PDF/DOCX/HTML/Markdown。报告可被完全不懂安全的人理解，同时保留专业人员的复现证据。

## 场景 E：研究结果沉淀

Finding 验证完成 → 提取候选 Knowledge Card → 查重查冲突 → 关联 Evidence → 人工审核 → 正式入库 → 后续研究可检索引用；漏洞类结论同步经 RTIS 接口登记。

---

# 第十九部分：本次修订重大决策记录

| 编号 | 决策 | 原因 |
|---|---|---|
| D1 | 2.0 先收口，Phase 0 在其后；收口前只许准备讨论 | 2.0 工作流正在执行 Phase 8，配额窗口正在消耗 2.0 工作；审计必须以收口后的真实状态为基线；并行改两套必然冲突 |
| D2 | RTIS 为漏洞领域唯一生产事实源，零直写、零行为变更、细节不落 3.0 库 | 防止第二套事实源分裂；RTIS 零改动是仓库红线；映射表强制为 Phase 0 交付物 |
| D3 | MCP/REST/CLI 纳入 3.0 并继承 Phase 9，同一 API Contract/Application Services/Schema | 对外能力是平台一等目标；两套接口体系必然漂移（rp_server/Rust 双实现的前车之鉴） |
| D4 | 导入管道复用 source_sync 泛化，禁止重建 | SK-2 三源同步已交付且 15,155 条对账零回归；重建即重复造轮子 |
| D5 | Phase 0 只冻结四项（领域模型+RTIS 映射 / Document IR / 存储与迁移 / 安全边界），其余 ADR 滚动 | 防瀑布式设计与过早冻结导致的过期文档；仓库成功经验是批次级设计 |
| D6 | OCR：Docling 先行、Tesseract 兜底、PaddleOCR 条件引入且容器化 | 宿主零安装红线；重依赖只进容器镜像；以实测为准 |
| D7 | Embedding 本地容器模型优先，外部 API 显式配置可选 | vendor 离线主义的延续；平台必须可离线运行 |
| D8 | 版本命名延后到 2.0 收口后按兼容性评估决定（2.x Phase 11+ 或 3.0） | 2.0 发布体系跑到一半，提前引入新版本号制造混乱 |
| D9 | 1k 到 100k 文档与百万 Chunk 规模测试排最后一个实施窗口 | 语料与基准本身是实打实工作量；过早执行浪费窗口 |
| D10 | Git 纪律双轨制：手动会话保守、工作流批次按既定自动入库 + CI 硬门槛 | 两套惯例各有存在理由；不划清范围会导致批次收口停摆或误覆盖 |
| D11 | 审计目录清单修正：crates/database、crates/agent-engine，补充 workflow-engine/research-engine/resource-engine/runtime-gateway | 原清单两处目录不存在；漏掉的四个是 P7/P8 新交付面，不盘会误判为空白 |
| D12 | 检索 Benchmark 以 RTIS 真实语料播种 | 真查询真 POC 比合成数据更有判别力，且直接服务 SRC 场景 |

---

# 第二十部分：给执行 AI 的硬性边界声明

1. **本文件是唯一权威版本**。旧版 RIP 3.0 计划书与本文件冲突之处，一律以本文件为准；不要拼接两份文档的要求。
2. **2.0 收口之前**：只允许阅读仓库、准备与讨论本计划、在 research-platform 仓库树之外做纯文档工作。禁止暂停/修改/替换/提前迁移正在运行的 2.0 工作流；禁止以本计划名义修改任何代码、执行迁移、安装依赖、在 research-platform 仓库执行 git commit/push。
3. **Phase 0 是只读审计**：唯一写入是 `docs/rip3/` 审计产物；四项冻结物通过设计评审后才允许进入 Phase 1。
4. **继承清单（原则 G）是底线**：八项已有能力必须复用；遇任何「重新实现其中之一」的冲动，先回头重读本文件。
5. **RTIS 红线**（11.5）与 **CI 硬门槛**（15.1）违反即返工，没有例外。
6. 阶段推进遇阻时，如实报告（17.6 格式），不伪造通过、不降低验收标准。
