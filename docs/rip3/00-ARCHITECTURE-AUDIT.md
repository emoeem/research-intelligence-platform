# RIP3 Phase 0 架构审计（00-ARCHITECTURE-AUDIT）

> 状态：Phase 0 交付（2026-11-14）；审计基线 = RIP 2.0 收口提交 2744ada 的真实仓库状态。
> 取证纪律（R2 §2.1.3）：结论全部来自实际代码与调用链（path:line 级），文档只作线索；
> 唯一写入面为 docs/rip3/（本文件与四项冻结物、ADR），零代码/迁移/依赖变更。
> 取证手段：三个并行取证代理（domain/context/research/resource/database、
> workflow/agent/容器执行面、RTIS 五面与安全边界）+ 本会话直查（迁移链、API 路由、
> CI/selftest、podman 镜像面）。

## 一、三关键问题实证结论（R2 §2.7.3）

### 1.1 向量检索接线状态：未接入（头号怀疑点证实）

证据链（不接受文档说法，全部为代码实证）：

1. 统一 Search Engine（crates/search-engine/src/，7 文件）全文 grep
   `vector|embedding|cosine|similarity` 零命中：统一检索面为纯 FTS5/BM25 词元打分。
2. workers/python/embedding_worker.py 有真实双后端实现（hash 占位 + bge-small-zh-v1.5
   ONNX 512 维，M8 尾批交付），但**生产调用链为零**：全仓引用仅出现在测试
   （workers/python/tests/_harness.py:21、test_embedding_model.py:109）与插件注册
   fixture（plugin-registry/tests/static_chain.rs:65 等），无任何 Rust 侧或服务侧调用方。
3. knowledge 表（migrations/001_init.sql）无向量列；全库 19 个迁移无任何向量存储结构。

结论：向量面 = 仅有独立 worker 与接口定义，无接入。RIP3 Phase 4 的 Hybrid Search
是全新建设面；bge-small-zh-v1.5 512 维 worker 可直接复用为嵌入供应侧。

### 1.2 RTIS 五面映射完整度：run-task 面最完整，vuln/chain/report 无仓内载体

| 面 | 仓内载体 | 结论 |
|---|---|---|
| vuln（漏洞/POC） | poc 表 28 列（rt.py:85-90 POC_COLS 25 业务列 + id/created_at/updated_at），FTS5 索引，14720 行实查；knowledge_ingest 摄取写面（plugin-runtime/src/ingest.rs:397-437） | 字段级可映射（映射表见 03-DOMAIN-MODEL 第六节） |
| evidence | k_meta.evidence_level（K0-K4 CHECK，migrate_m3.py:48）+ k_event append-only（7 事件型）| 半映射：摄取只收 poc/skill_md/method_case 三类正文，K 分级/反知识/关系边留在 RTIS 侧未摄取 |
| run-task | rtis.py 编排面（task/profile/gate/knowledge/route/handoff/skills/tools/aar，rtis.py:1584-1643）+ k_event 审计 | 最完整：15155 对账载具（ingest.rs:397-437 + knowledge_ingest_provenance.rs:145，幂等键 UNIQUE(plugin_id,plugin_version,content_kind,content_sha256)） |
| chain（攻击链） | 仓内零载体：redteam_chain 工具名仅存在于 skills 文档文字引用（skills/asset-correlation.md:16 等），真实链数据在仓外 ~/.dsh/redteam/engagements/*/assets.db（28 实例，CURRENT_DATABASE_MODEL.md:29-33 认定与知识库无联邦） | 零联邦，经 RTIS 接口引用 |
| report | 无结构化工具：rt.py 与 rtis_mcp.mjs TOOLS（rtis_mcp.mjs:337-440，10 工具）均无 report 工具；报告面 = skill 模板（redteam-report-template.md / report-writing.md）+ REPORTS_DIR 目录约定 | 3.0 Report Engine 从零建，事实引用 RTIS |

K0-K4 语义定义在仓外上游 kit（security-research-upgrade/rtis/design/
Knowledge_Lifecycle_Design.md:89-97：K0 Captured / K1 Researched / K2 Evidence-backed /
K3 Experimented / K4 Reusable），仓内仅有排序权重 K_SCORE（rtis-bridge.js:44）。
配套红线：RTIS MCP 面仅 10 工具（poc/kb/skill/knowledge/task 族），零直写生产库路径成立
（rtis_mcp.mjs:28-32 写路径唯一经 rt.py）。

### 1.3 source_sync 泛化为统一 Document 导入管道：差距清单

现有能力（复用基底，plugin-runtime/src/source_sync.rs）：

- Git/GitHub/URL/local_dir 四通道 + pin 前置闸（source_sync.rs:114-183：pinned.kind
  只接 commit/tag/release，commit 必须全 40 hex，浮动分支 fail-fast floating_ref_rejected）
- checksum 与 catalog 对账（P10-2 catalog-publish/verify 面）
- 15155 条三源对账零回归的摄取写面（ingest.rs:397-437）

泛化差距（Phase 1 改造点，逐条带行号级位置）：

| # | 差距 | 现状位置 | 改造方向 |
|---|---|---|---|
| 1 | 同步单位是插件包（manifest 驱动）而非任意文档集合 | source_sync.rs 全模块 | 导入面新增 document corpus 单位，manifest 通道保留为特例 |
| 2 | 内容消费终点是 knowledge_source_entry（knowledge.rs:201-238 body:String 折叠）而非 Document/Revision/CAS | ingest.rs + 011 表 | 终点改挂 Document 域（03-DOMAIN-MODEL），正文进 CAS 对象 |
| 3 | URL 通道单页抓取（document.fetch 另有 Rust 面 document_fetch.rs:1-33 逐跳复核），缺批量/递归 | document_fetch.rs | 批量导入编排走 workflow（复用执行内核） |
| 4 | 解析终点是 markdown_links 目录抽取（catalog_worker.py:1-12）非通用 IR | catalog_worker.py | 统一改挂 Document IR（04-DOCUMENT-IR-SCHEMA） |
| 5 | checksum 是目录级 content_sha256 非逐文件 blob CAS | source_sync.rs | 逐文件 sha256 入 CAS（12-STORAGE-AND-MIGRATION 第三节） |
| 6 | 无工作区/权限模型（插件全局安装） | paths.rs + store.rs | Document 带 workspace_id/visibility，检索入口前置过滤（13-SECURITY 第五节） |

## 二、登记表（A 到 G，八字段逐行）

字段口径：当前实现（path:line）/ 已有功能 / 部分实现 / 仅有规划 / 设计缺陷 / 迁移风险 /
改进方案 / 验收方式。缺项如实写「无」。

### A 组：数据模型（crates/domain + crates/database + migrations）

| 对象 | 当前实现 | 已有功能 | 部分实现 | 仅有规划 | 设计缺陷 | 迁移风险 | 改进方案 | 验收方式 |
|---|---|---|---|---|---|---|---|---|
| Document 独立建模 | 不存在：domain 全量 grep DocumentRevision/DocumentBlock/ContentObject/SourceRecord 零命中；knowledge.rs:201-238 正文 body:String 折叠在 KnowledgeSourceEntry | 无 | 无 | R2 第三部分 | 正文无法独立版本化与多格式承载 | 新增表零冲突（新增式迁移） | 03-DOMAIN-MODEL 定稿后 PG-0001+ 落地 | IR roundtrip 测试 + 多格式导入端到端 |
| Knowledge/原始资料分离 | 未分离（同上折叠） | SK-1 摄取链 15155 条可用 | 源快照字段在（knowledge.rs:187-196） | R2 原则 B | 原始文件不可追溯 | 摄取链改造需保 15155 对账 | ContentObject/SourceRecord 分离 | 同文件多来源引用测试 |
| Revision 版本链 | 无 | 无 | content_sha256 已有 | R2 3.1 | 更新即覆盖语义 | 无 | DocumentRevision 表 + 版本恢复 | 更新生成新版本、历史可访问 |
| 书籍章节结构 | 无（block 级结构不存在） | parsing_worker.py:30-36 仅 markdown 标题树 | heading_tree 输出已有（parsing_worker.py:400 前） | R2 4.5 | 压平风险 | 无 | DocumentBlock 父子结构（04-DOCUMENT-IR） | 章节树 roundtrip + 页码定位 |
| PDF 页码/坐标 | 无 | 无 | 无 | R2 4.4 source_mapping | 无 | 无 | IR source_mapping + PyMuPDF/Docling | PDF 解析页码对账 |
| 解析器版本/许可记录 | 部分：license_status/source 有（knowledge.rs:226-231）；解析器版本无 | SK-2 许可三态 | parser_version 无 | R2 4.4 | 解析器升级不可追踪 | 无 | parser_metadata 进 IR | 重解析幂等（content_hash 键） |
| 任务引用知识可追 | 已有：context_citation_evidence（011 表，resource-engine/src/sqlite.rs:153-227）+ 013 三 Memory 表 | 完整 citations 面 | 无 | 无 | 无 | 无 | 继承 + PG 侧重建 | 六 ID 对账 |
| 多来源同一内容 | 雏形：resource_source 1:N 子表（001_init.sql:150-168）+ content_sha256 | Resource 面 81 行 1273 alias | Document 面 0 | R2 5.5 | 来源信息随覆盖丢失 | 无 | ContentObject 多 SourceRecord | 同 sha256 双来源引用测试 |

### B 组：存储（crates/database + rip-data + 容器面）

| 对象 | 当前实现 | 已有功能 | 部分实现 | 仅有规划 | 设计缺陷 | 迁移风险 | 改进方案 | 验收方式 |
|---|---|---|---|---|---|---|---|---|
| SQLite 表结构与索引 | 19 迁移 001-019：约 30 表 + v_trace_chain 视图 + ~80 索引 | 全部 2.0 面 | 无 | 无 | 见 WAL 行 | 新增式纪律已验证 19 次 | 情报核转 PG（ADR-0001） | 迁移幂等二跑零变化 |
| rip-data 布局 | paths.rs:9-47（registry/installed/cache/state/logs + ENV_DATA_DIR 优先级） | 插件/运行库/缓存 | 无 | 无 | 无对象存储层 | 无 | Object Storage Adapter 叠加 | 路径常量测试 |
| Artifact 生命周期 | 005 artifact 表 + rp-admin backup（database/src/backup.rs:185/436/448） | 备份/校验/恢复/保留 | Artifact 引用计数无 | R2 10.2 | 删来源可能孤儿化 | 无 | CAS 引用计数 + 孤儿清理 | 删除保护测试 |
| 大文件处理 | parsing_worker.py:28 输入帽 10MiB；无分块存储 | 文本级 | 二进制无 | R2 10.1 | 大 PDF 不可入 | 无 | CAS 分块 + 流式 | 100MB PDF 导入 |
| 迁移纪律 | migrate.rs:3-5（NNN 升序 user_version 跳过）+ ALTER 守卫（migrate.rs:71-112）；execute_batch 无显式事务（migrate.rs:126） | 19 次幂等验证 | 事务性缺 | R2 3.3 | 半途失败可能留半态 | 迁移文件修改需全链重验 | PG 侧 refinery（每文件事务）；SQLite 侧补 BEGIN 包裹评估 | 断电注入测试 |
| 并发写入现状 | pool.rs:36-53（max 2/4，busy_timeout 5s，pool.rs:245-247 仅 foreign_keys=ON） | 单写者纪律靠池上限 | WAL 未设（grep journal_mode 零命中） | R2 6.9 | 写竞争上限低；WAL 缺席 | pool.rs 改动波及全 crate | SQLite 侧 020 批补 WAL + PRAGMA 自检；情报核多写者走 PG MVCC | PRAGMA 自检脚本 + 并发压测 |
| 备份恢复 | rp-admin 四子命令（P10-1）+ 保留归档 retention.rs:104-130 | 实演 16/16 | PG 面无 | 12-STORAGE | 双库备份需双轨 | 无 | pg_dump + rp-admin 双轨 | 双轨演练 |

### C 组：检索（search-engine + embedding worker + context-broker）

| 对象 | 当前实现 | 已有功能 | 部分实现 | 仅有规划 | 设计缺陷 | 迁移风险 | 改进方案 | 验收方式 |
|---|---|---|---|---|---|---|---|---|
| FTS5/BM25 | search-engine/src/index.rs + query.rs + weights.rs | 词元打分（keyword 60% + quality 40%，resource-engine/sqlite.rs:428-463） | 真 BM25 无（自制权重） | R2 6.2 | CJK 靠 2-gram hack（tokenizer.rs） | 检索层重写波及 knowledge 面 | PG pg_search BM25（情报核）+ 既有面保留 | Benchmark Recall@K |
| 向量检索 | 未接入（第一节 1.1 实证） | embedding worker 双后端可用 | 接入零 | R2 6.2/6.3 | 头号缺口 | 无既有向量面可破坏 | VectorBackend trait + pgvector（ADR-0001） | Hybrid 端到端 |
| Chunking | 无块级 chunk 表；L2 摘要桶组装（assemble.rs:273-332） | 摘要级 | 块级无 | R2 3.1 | 整文检索粒度粗 | 无 | KnowledgeChunk 表 + 策略字段 | chunk 位置回链测试 |
| Reranking | 无 | 无 | 无 | R2 6.2 | 无 | 无 | bge-reranker ONNX worker | rerank 前后 NDCG 对比 |
| L0-L3 分层 | policy.rs:43-58 七分项预算 + assemble.rs:203-350 分层装入 + 降级驱逐（assemble.rs:870-899） | 完整可复用 | 无 | 无 | 无 | 改动需回归 76/76 selftest | 按 R2 6.6 增量（Query/Summary Cache 等） | 既有 selftest + 新缓存失效测试 |
| 引用与证据回链 | citations[]（assemble.rs:1009）+ context_citation_evidence 固化（assemble.rs:572-576,716-721） | 完整 | 无 | 无 | 无 | 无 | 继承 | 六 ID 对账 |
| 缓存 | cache.rs:117-207 内容寻址 + 七类失效触发器（cache.rs:17-28）+ 分层 TTL（36-45） | 完整 | 无 | 无 | 无 | 无 | 增 Query/Summary Cache | 失效触发测试 |
| Token 预算 | estimate.rs:71-75 ceil(ascii/4 + cjk + other/2)，自认未校准 | 近似可用 | 校准零 | R2 6.6 | 常数失真 | 无 | tokenizers crate 校准 | 校准后偏差 < 5% |

### D 组：Workflow（workflow-engine + agent-engine）

| 对象 | 当前实现 | 已有功能 | 部分实现 | 仅有规划 | 设计缺陷 | 迁移风险 | 改进方案 | 验收方式 |
|---|---|---|---|---|---|---|---|---|
| DSL 与版本化 | dsl.rs（definition-v1 Schema 门）+ store.rs:122-206（版本 = MAX+1 追加不可变）+ 016 表 | 完整 | 无 | 无 | 无 | 无 | 继承 | 既有 46 断言 |
| 节点类型 | WorkflowNodeKind 8 值（domain/workflow.rs:117-131）+ 六执行器（exec/node_*.rs:30/26/26/28/90/36） | 完整 | 无 | R2 7.2 新节点（Evidence Collection/Finding Creation/Report Generation） | 无 | 词域扩展需迁移 CHECK 同步 | 节点注册扩展 | 新节点端到端 |
| Execution State/Checkpoint | 017 三表 + state.rs:661-741（仅节点终态写快照）+ resume（exec/mod.rs:263-321） | 完整 | 节点内进度快照无 | R2 4.6 | 长解析节点 kill 后整节点重跑 | 无 | 解析管线分片为 Parallel 扇出 + 分片 checkpoint | kill 后续跑测试 |
| Human Approval | 018 awaiting_approval + node_human.rs:36 + 批准注入（exec/mod.rs resume_with_injection） | 完整 | 无 | R2 7.2 完整化 | 无 | 无 | 继承 | 既有 76/76 |
| Retry | retry 新 run 承载只重跑失败节点（exec/mod.rs:327-387） | 完整 | 无 | 无 | 无 | 无 | 继承 | 既有断言 |
| Trace | 015 六 ID + v_trace_chain + trace.rs:329 trace_query | 完整三栈贯穿 | 无 | 无 | 无 | PG 侧重建同款视图 | 12-STORAGE | 双库对账 |
| 画布 | P8-4 五区实页（WorkflowsPage + @xyflow） | 完整 | 无 | R2 7.2 增强 | 无 | 无 | 节点库扩展 | 既有视觉门 71 |
| 模板能力 | 无（全仓 grep template 零命中；唯一复用面 = 版本化定义） | 无 | 无 | R2 7.2 | 无 | 无 | 模板层新增（P5-1） | 模板实例化测试 |

### E 组：报告（现状数据面）

| 对象 | 当前实现 | 已有功能 | 部分实现 | 仅有规划 | 设计缺陷 | 迁移风险 | 改进方案 | 验收方式 |
|---|---|---|---|---|---|---|---|---|
| Finding/Evidence 数据面 | evidence 表（001）+ agent_evidence_memory（013）+ artifact 表 | 证据固化与引用完整 | Finding 无独立表（R2 3.4：事实以 RTIS 为准） | R2 8.1 | 无 | 无 | Finding 编排面（PG）+ RTIS 事实引用 | 双库对账 |
| 既有报告能力 | RTIS skill 模板（redteam-report-template.md）+ REPORTS_DIR（rtis.py:59） | 文本约定 | 结构化零 | R2 8.1 | 无 | 无 | Report IR + 渲染器 | 多格式一致性 |
| 导出 | 无 | 无 | 无 | R2 8.5 | 无 | 无 | typst/docx-rs 渲染器族 | 导出质检 |

### F 组：对外能力（rp_server + MCP/REST/CLI）

| 对象 | 当前实现 | 已有功能 | 部分实现 | 仅有规划 | 设计缺陷 | 迁移风险 | 改进方案 | 验收方式 |
|---|---|---|---|---|---|---|---|---|
| REST 路由 | API_ROUTES 字典 20 条 GET（rp_server.py）+ do_POST 白名单（plugins 写面/workflows 写面）+ 动态 /api/plugins/&lt;id&gt; | 完整 2.0 面 | 无 | R2 13 扩展 | 无 | 新端点按 Phase 9 惯例 | 扩展 API Contract | parity 载具 |
| MCP | rmcp 3.10.0（rmcp 迁移批 8adbf1c）：initialize 协商 + tools/list（P0 修复：全条目 inputSchema）+ run_tool 单一执行通道 + 7 resource + 3 prompt | 完整 | 无 | R2 13 | 无 | 无 | 新领域能力扩展同源 | parity job |
| CLI | 11 bin：rp-admin/agent/context/knowledge-fetch/mcp/plugin/research/resolve/review/search-index/tool/image-pin | 完整 | 无 | R2 13.4 | 无 | 无 | 同 Application Services 扩展子命令 | --help 统一面 |
| 同源约束 | Tool Center / MCP 目录 / /api/tools 三面同源（catalog.rs 对齐 handle_tools） | 完整 | 无 | R2 13.1 | 无 | 无 | 继承 | 三面对账 selftest |

### G 组：工程面（CI + 门禁 + selftest + vendor）

| 对象 | 当前实现 | 已有功能 | 部分实现 | 仅有规划 | 设计缺陷 | 迁移风险 | 改进方案 | 验收方式 |
|---|---|---|---|---|---|---|---|---|
| CI 覆盖 | ci.yml 四 job（rust:28/parity:61/python-workers:85/web:103） | 五道门禁全在 CI（rust job 含 workspace 测试与 clippy；web job 含 build/vitest） | cargo-deny 不在 CI（本地工具） | R2 15.1 | 无 | 无 | cargo-deny 进 CI 评估（P1 批） | CI 全绿 |
| selftest 族 | apps/server/selftest_*.sh 11 脚本（auth/phase5-10 各面） | 机器可核对验收 | PG 面无 | 本计划 | 无 | 无 | PG 基座 selftest 新增（P1-1） | 脚本 exit 0 |
| vendor 离线 | Cargo.lock 164 第三方全 vendor；Python 重依赖只进容器 | 完整纪律 | PG 扩展（pg_jieba/pg_search）需容器内构建 | ADR-0001 | 无 | 无 | 自建 PG 镜像 Dockerfile digest 钉死 | 离线构建复验 |
| RTIS 对账 | ingest.rs:397-437 + provenance 测试 + 15155 口径 | 完整 | 无 | R2 15.5 | 无 | 每阶段红线 | 继承 | 对账复跑 |
| 镜像族 | containerfiles 四族（base/webresearch/coreutils/source）digest 台账（README.md:33-42）+ 资源基线表（54-67） | 完整惯例 | rip-tool-document planned 未建 | R2 4.3 | 解析镜像缺 | 无 | rip-parser 族新建 | 离线构建 + 台账三处同步 |

### RTIS 侧盘点（任务 2，详见 03-DOMAIN-MODEL 第六节映射表）

rt.py 4 命令（poc search/get/add/use/update + kb stats + skill list/get + mcp，rt.py:1070-1146）；
rtis_mcp.mjs 10 工具（rtis_mcp.mjs:337-440）；rtis.py 编排 13 子命令（rtis.py:1584-1643）；
knowledge.db 13 表（poc 28 列 14720 行 + FTS5 + k_meta/k_negative/k_relation/k_event/
k_proposal/k_version，DDL 权威 migrate_m3.py:43-111）；K0-K4 语义在仓外上游
（Knowledge_Lifecycle_Design.md:89-97）。编排红线（rtis.py:22）：绝不修改
poc/k_meta/hit_count/schema，唯一写是 k_event append-only。

## 三、迁移编号确认与 SQLite 运行约束基线

- 迁移链实况：001 到 019 共 19 文件（019_external_api_audit.sql 为 P9-3 交付）。
  **下一可用编号 = 020**（R2 原文「当前 018 已占用，预期从 019 起」过时，本审计修正）。
- SQLite 运行约束基线（R2 §6.9 要求的现状对照）：WAL 未设置（pool.rs 仅
  foreign_keys=ON，pool.rs:247；grep journal_mode 全仓零命中，backup.rs:461 的 WAL
  提及为恢复时侧车清理）；busy_timeout 已显式（pool.rs:31，5s）；单写者纪律靠
  max_connections（readonly 2 / writable 4）；迁移无每文件事务（migrate.rs:126）。
  处置：运营核侧 P1-1 批补 WAL + PRAGMA 自检脚本；情报核侧由 PG MVCC 承接。

## 四、API Contract 底账（R2 §13 输入）

- GET 面 20 条（API_ROUTES 字典 + /api/plugins/&lt;id&gt; 动态）：overview/knowledge/
  knowledge_sources/resources/tasks/tools/runs/runtime/stats/traces/agents/agent-runs/
  workflows/workflow-runs/plugins/plugins/trust-policy/mcp-catalog/reviews/api 根。
- POST 白名单：plugins 安装/启用/禁用/卸载（P6-8 确认闸）+ workflows put/run/pause/
  resume/retry（P8-3）；PUT/DELETE/PATCH 一律 405 readonly（rp_server.py:3008-3014）。
- 鉴权四件套：Bearer 常量时比较（rp_server.py:2913-2924）+ 滑动窗口限频 429
  （186-199）+ JSONL 审计（202-213，019 列对齐）+ 结构化错误（ApiError 170-175）。
- MCP：rmcp stdio（initialize 协商 2024-11-05 至 2025-11-25）+ tools/list 分页全条目
  inputSchema + rip.run_tool 单一执行通道 + rp-mcp-parity 常驻 CI（parity job）。
- CLI 11 bin 与帮助面统一（P9-3 口径）。

## 五、安全边界现状（13-SECURITY-AND-PRIVACY 的输入）

已有强基线（全部 path:line 实证）：容器隔离全套（podman_host.rs:395-415 read-only/
keep-id/cap-drop ALL/资源上限/挂载白名单/tmpfs nosuid,nodev）+ network 词表
（egress_proxy.rs:87-99 None/EgressAllowlist）+ T0/T1/T2 闸（validate.rs:343-346）与
T2 authorization_ref 强制（tool-engine/src/lib.rs:184-186）+ REQUEST_KEYS 封闭枚举
拒 secrets（request.rs:123-135）+ document_fetch 逐跳复核与全地址私网检查及 IP 复核
（document_fetch.rs:433-559）+ rp_server 鉴权四件套。

缺口（Phase 1 起逐项闭环，详见 13 号文档）：

1. **数据通道/提示词注入防护零实现**（context-broker/agent-engine 对
   injection/untrusted/sanitize 零命中；SK-1 摄取正文经检索面可直达 Agent 上下文，
   通道上无任何标记/包裹/降权；RIP3 计划 358/604 行仍为计划态）。
2. egress 代理 CONNECT 面允许列表是 hostname 级且 DNS 在检查后解析
   （egress_proxy.rs:326 到 498-511，无 document_fetch 式 IP 复核；过渡期边界自认
   egress_proxy.rs:15-18）。
3. 权限过滤前置：检索入口无 visibility/权限闸（检索完成后才可过滤）。
4. K0-K4 证据分级未随摄取携带（外部文档入库行无证据等级）。

## 六、窗口重排建议（R2 §2.7.4，待用户确认）

审计后实况重排（原示意 3 到 4 窗 → 建议 5 窗，理由：PG 双库基座 + 解析容器族 +
治理/检索双面各需整窗，规模测试保持独立末窗）：

| 窗口 | 内容 | 依据 |
|---|---|---|
| 窗口 1（本次） | Phase 0 全部产物 + Phase 1 开批（PG 基座 + Document 域） | 审计已完成，PG 决策已裁决 |
| 窗口 2 | Phase 1 收口 + Phase 2（解析容器族 + 解析管线 + IR 落库） | 解析面工作量最大（Docling 镜像 + 九态管线） |
| 窗口 3 | Phase 3 + Phase 4（治理五级去重 + Hybrid Search + Benchmark） | 治理与检索互为输入 |
| 窗口 4 | Phase 5 + Phase 6（模板/自适应引擎 + Report IR/渲染器族） | 复用面高、新建面集中在渲染 |
| 窗口 5 | Phase 7 + Phase 8（回流 + 规模基准 + 运营核移植裁决） | R2 D9：规模测试独立末窗 |

每窗结束执行阶段收口（入库 + CI 硬门槛 + RELEASE 登记，R2 §15.1）。

## 七、Phase 0 验收对账（R2 §2.7 逐条）

1. 登记表覆盖 2.2 全部对象，无一行缺代码证据：A 到 G 七组 + RTIS 侧盘点，每行
   path:line（上表）。
2. 四项冻结物完成：03/04/12/13 四文档同批入库（冻结物即本批产物），设计评审待用户。
3. 三个关键问题实证结论：本文件第一节（向量未接入 / RTIS 五面 / source_sync 差距）。
4. 预算与窗口重排：本文件第六节（待用户确认）。
5. 全程零写入（docs/rip3/ 除外）、零迁移、零依赖安装：commit 前 git status 核对 +
   全量门禁复跑（docs-only 变更不应触碰任何门禁面）。
