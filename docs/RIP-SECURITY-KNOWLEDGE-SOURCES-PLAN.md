# RIP 安全知识源接入与工具扩展计划

> 状态：实施规格草案，不代表已经安装或完成接入  
> 基准日期：2026-11-11  
> 目标仓库：<https://github.com/emoeem/research-intelligence-platform>

## 1. 目标与边界

将以下资料纳入 RIP 的长期安全研究资料体系：

1. [Awesome-Hacking](https://github.com/Hack-with-Github/Awesome-Hacking)：安全工具、专题资源和 Awesome Lists 的聚合目录。
2. [SecSkills](https://github.com/DaoYiSec/SecSkills)：Agent、Skills、MCP、代码审计、渗透测试、CTF、逆向等项目目录。
3. [tongui 安全课程精选站点](https://tongui66.github.io/course-notes/) 与其[完整课程原稿仓库](https://github.com/tongui66/Cyber-Security-Course-Notes)：网站作为精选导航，Git 仓库作为完整 74 篇课程的权威结构化来源。

目标不是把三个网站/仓库的所有内容和所有外链项目一股脑复制进 RIP，也不是自动安装目录中的工具。目标是建立“来源跟踪 → 目录解析 → 资源登记 → 许可/安全审查 → 按需抓取 → 文档解析 → 索引检索 → Evidence 引用”的可追溯流程。

必须复用 RIP 当前的 Rust Core、Python Workers、Plugin Registry/Runtime、Tool Registry/Engine、Search Engine、Context Broker、Audit Engine 和 Runtime Gateway。不得另建平行知识库、平行插件系统或绕过现有执行授权。

## 2. 当前仓库核实结果与关键缺口

已核对主分支的相关实现和阶段文档：

- `docs/phase6/PHASE6-PLAN.md`：P6-1 至 P6-7 已完成；P6-8 Plugin Center 页面尚未开工。P6-4 已有 local_dir/git/archive/mcp_server/http_api 来源适配；P6-5 目前落地的是 `knowledge.rtis-redteam` 专用知识插件。
- `crates/plugin-runtime/src/ingest.rs`：当前 `CONTENT_KINDS` 仅允许 `poc`、`skill_md`、`method_case`，采集逻辑固定读取包内 `knowledge.db`、`skills/*.md` 和 `pocs/`。
- `migrations/010_knowledge_ingest.sql`：`knowledge_ingest.content_kind` 有同样的三项 CHECK 约束。
- `crates/context-broker/src/sources/plugin.rs`：仍是空实现，`candidates()` 返回空集合，`load()` 无法展开插件内容。
- `crates/context-broker/src/sources/knowledge.rs`：当前平台知识源经 Search Engine 检索元数据，但 L3 全文展开受现有正文数据面限制。
- `crates/search-engine/src/index.rs`：当前 FTS 派生索引来自平台 Resource 与 catalog GitHub metadata，未纳入通用插件知识正文。
- 已存在工具可复用：`rip.doc.parse`（Markdown/TXT/HTML 结构解析）、`rip.text.embed`（hash / BGE embedding）、`rip.kb.evaluate`、`rip.kb.keyword_search`。

因此，**仅添加三个知识插件 Manifest 不会让资料进入可检索 Knowledge，也不会让 Context Broker 能加载全文**。必须先补通用摄取和检索接线，再安装/启用来源插件。

## 3. 来源登记方案

配置文件：`rip-security-knowledge-sources.yaml`。当前主分支尚无这一配置契约；此文件是本计划提出的 Source Registry 输入规格，不应伪装成现有 Plugin Manifest v1，也不能直接交给当前 `rp-plugin install` 当作可执行 Manifest。

### 3.1 Awesome-Hacking

| 字段 | 当前核实值 |
|---|---|
| 仓库 | `https://github.com/Hack-with-Github/Awesome-Hacking` |
| 默认分支 | `master` |
| 基准 Commit | `440486fd08b0a0d9ad9a041a0bb234dcdf65dee2` |
| 主要内容 | README 中的分类目录、项目名称、描述、链接 |
| 仓库自身许可证 | CC0-1.0 |
| 建议摄取方式 | 只解析 README 中的分类与链接条目，登记为 `catalog_entry`；不把外链项目正文视为 CC0 |
| 建议刷新方式 | Git 固定 Commit 快照；检查新 Commit 后先 Diff，再按审核策略更新 |

它是链接型目录，不是大量完整教程的正文知识库。应提取分类、名称、简述、原始 URL、资源类型、来源行/锚点和仓库快照 Commit。外链仓库及文档需各自核验许可、维护状态和内容。

### 3.2 SecSkills

| 字段 | 当前核实值 |
|---|---|
| 仓库 | `https://github.com/DaoYiSec/SecSkills` |
| 默认分支 | `main` |
| 基准 Commit | `1867eb6b71889af8632107884e9d0235510f9f78` |
| 主要内容 | README 中约 102 个目录条目，涉及 Agent、Skills、MCP、代码审计、渗透测试、CTF、逆向等 |
| 仓库自身许可证 | Apache-2.0 |
| 仓库结构 | 当前主要是 `README.md`、`LICENSE`、图片，不是打包好的 102 个 Skill 正文目录 |
| 建议摄取方式 | 只解析目录项，登记为 `catalog_entry`，附带分类和上游链接；精选内容再进入独立审查流程 |

必须过滤无关“娱乐”条目和推广内容，不将 README 中的模型中转站链接作为安全工具。SecSkills 的 Apache-2.0 不自动覆盖它所链接的第三方项目、Skills 或 MCP Server。

### 3.3 tongui 安全课程

| 字段 | 当前核实值 |
|---|---|
| 精选站点 | `https://tongui66.github.io/course-notes/` |
| 完整原稿仓库 | `https://github.com/tongui66/Cyber-Security-Course-Notes` |
| 默认分支 | `master` |
| 基准 Commit | `45a12a759e82adec13c3d171a34d1cbeea1a15b3` |
| 内容规模 | README 声明完整课程 74 篇、13 章；精选站点约 31 篇 |
| 根目录许可证 | 当前树未发现 `LICENSE` 文件，按 UNLICENSED/未明确授权处理 |
| 建议摄取方式 | 先登记课程目录、文章标题、章节、URL、路径和 Commit；全文按需在线读取，不默认批量复制到持久知识库 |

精选站点与完整仓库是同一课程体系的不同入口。Git 仓库为完整课程目录，站点用于标注精选内容和导航。通过规范化 URL、路径、标题和内容哈希去重。

由于未发现明确许可证，不能把整套课程原文镜像到可分发的知识包或公开仓库。需要全文持久化时，先获得作者许可；在此之前可存储来源元数据、引用链接和必要的短摘录，并按需获取用户问题所需的页面内容。

## 4. 推荐的数据模型

### 4.1 统一 Source Record

每个来源记录至少保存：

- `source_id`、`plugin_id`、`plugin_version`、`corpus_id`
- `source_type`、`canonical_url`、`source_path`、`source_commit`
- `title`、`description`、`category_path`、`resource_kind`
- `license_status`、`license_evidence_url`、`rights_policy`
- `first_seen_at`、`last_seen_at`、`last_checked_at`
- `content_sha256`、`metadata_sha256`、`upstream_updated_at`
- `review_status`、`risk_flags`、`source_trust`

至少区分三种记录：

1. `catalog_entry`：Awesome-Hacking、SecSkills 中的链接目录项，以及课程文章索引项。
2. `document_markdown`：取得许可、或明确允许在当前使用场景持久化的 Markdown 正文。
3. `skill_candidate`：Skill、Agent、MCP、Tool 的候选元数据与原始来源引用，不直接代表已经安装或启用。

保留现有 `knowledge_ingest` 与 RTIS POC 兼容性。实施前检查 SQLite CHECK 约束、索引、迁移版本和全部读写调用点；不要直接放宽代码常量却忘记迁移数据库约束，也不要通过绕过摄取管道直接写主库。

优先方案：新增通用知识来源数据面/迁移，由 Rust Domain 定义稳定实体与状态，Plugin Runtime 通过单一受控摄取入口写入。是否扩展 `knowledge_ingest` 或新建通用表，必须在 P6-9 ADR 中基于兼容性、去重语义和 Phase 7 Knowledge 正文模型评审决定。不得保留两套长期互不兼容的 Knowledge 实体。

### 4.2 去重和版本策略

- Git 来源固定 Commit，不允许浮动分支安装；更新检查时读取新 HEAD，先生成变更清单。
- 文档正文使用 SHA-256；目录条目使用规范化 canonical URL + source ID + stable item key。
- 同一课程文章从站点和 Git 仓库发现时合并为同一个逻辑资源，保留两份 `source_ref`，以 Git 路径为正文来源。
- 上游删除条目不能立即物理删除已有 Evidence/Run 引用；改为 `upstream_missing` 或 `deprecated`，保留历史溯源。
- 内容变化形成新版本或新快照，不覆盖历史 Evidence 使用的旧内容。

## 5. Knowledge Plugin 拆分

建议建立三个独立的 Knowledge Plugin，以便单独刷新、禁用、审计和处理许可：

| Plugin ID | 来源 | Capability | 内容模式 |
|---|---|---|---|
| `knowledge.awesome-hacking` | Awesome-Hacking Git | `knowledge_source` | `catalog_entry` |
| `knowledge.secskills` | SecSkills Git | `knowledge_source` | `catalog_entry` + `skill_candidate` |
| `knowledge.tongui-course-notes` | 课程 Git + 站点 | `knowledge_source` | `catalog_entry`；正文按许可状态按需读取 |

每个真实 Plugin Manifest 应采用当前 `schemas/plugin-manifest/v1.json` 完整校验，至少声明：固定 Git Commit、来源主页、准确许可证状态、`knowledge_source` capability、受支持内容类型、解析工具引用、索引策略和按需加载入口。

注意：`content_kinds` 当前摄取白名单只有 `poc`、`skill_md`、`method_case`，所以以上类型属于 P6-9 扩展契约，不是当前已支持值。完成 Schema、Domain、DB migration、Ingest API 和 Search/Context 接线后，才能生成可安装的 Manifest。

## 6. 需要增加的工具

### 6.1 复用现有工具

| Tool | 复用方式 | 当前边界 |
|---|---|---|
| `rip.doc.parse` | 解析 Markdown/HTML/TXT 章节与标题树；扩展可选 Markdown link extraction 输出 | T0 纯解析，不联网、不执行文档内容 |
| `rip.text.embed` | 对通过许可策略的文档片段做 embedding | T0；保留模型后端/维度协商 |
| `rip.kb.evaluate` | 对解析质量、内容完整性、重复度做质量评估 | T0 |
| Context Broker + Search Engine | 作为统一检索和上下文入口 | 不新增平行 Search API；补齐 Plugin Knowledge source 的 candidates/load |
| Plugin Registry/Runtime | 固定版本来源下载、安装、启停、更新检查和受控 ingest | 不另造一个 Git 下载/插件生命周期系统 |

### 6.2 新增工具建议

**A. `rip.knowledge.catalog.extract`（T0）**

用途：从 Awesome-Hacking、SecSkills 和课程索引的 Markdown/HTML 中抽取链接目录项。

输入：`source_id`、`document_text` 或 Artifact ref、可选 `category_map`、`max_entries`。  
输出：`entries[]`，每项含 `name`、`description`、`canonical_url`、`category_path`、`resource_kind`、`source_location`、`raw_link_text`、`dedupe_key` 和抽取置信度。

约束：只解析内容，不访问链接；拒绝 `javascript:`、`data:`、`file:` 等非预期 scheme；保留原始 URL，不从文字猜造 URL；HTML 解析时剔除 script/style；对超量条目做分页/截断报告。

**B. `rip.security.extension.audit`（T0）**

用途：静态检查待纳入的 Skill、Agent、MCP Server、Tool Manifest 和仓库文件。

检查：许可证和版权声明；版本/Commit pin；依赖锁文件；安装脚本；命令执行入口；网络和文件系统能力；Secrets 读取；容器/宿主机边界；权限升级；MCP tools 输入输出 Schema；Prompt Injection/越权指令迹象；维护状态和已知安全公告。

输出：机器可读 `findings[]`、`risk_level`、`review_status`、`required_permissions`、`license_status`、`supply_chain_notes`。只给出静态结论，不执行安装脚本或被审查 Skill，不声称能够证明代码安全。

**C. `rip.knowledge.document.fetch`（T1）**

用途：根据已登记的 `source_id + entry_id` 按需抓取一个已审核来源的文档正文，给 Context Broker/Agent 做定点阅读。

约束：不接受 Agent 任意拼接 URL；只能从 Registry 已批准的 canonical URL 取值；域名 allowlist、DNS/IP 私网检查、重定向逐跳复核、大小/MIME/超时限制、内容哈希和抓取时间记录；禁止 localhost、RFC1918、link-local、metadata endpoint；默认只读 GET，不带用户 Cookie/Authorization header，不执行页面脚本。

**D. `rip.knowledge.source.sync`（T1）**

用途：触发已安装知识插件的版本检查、固定版本抓取、解析、去重、摄取和索引更新。

约束：调用 Plugin Runtime/受控 Ingest API，不直写 SQLite；支持 dry-run、`idempotency_key`、最大文件数/字节数/耗时、失败重试和部分失败报告；Git 更新必须先固定解析到的完整 Commit；不得执行仓库中的脚本。

### 6.3 工具权限分级

- 上述目录解析、静态审查、文档解析、embedding：T0。
- 固定来源的 Git/HTTP 只读同步、单篇 HTTP GET、平台内部知识入库：按现有 T1/T0 规则与实际副作用裁定；所有写入必须走受控平台 API 并 Audit。
- 未来引入的主动扫描、远程利用、修改目标状态、向外部服务提交任务等操作：T2，必须携带 `authorization_ref`，由既有 Tool Policy Resolver 强制校验。不得因工具来自公开目录而自动获得执行权限。

## 7. 安全、许可与内容治理

1. **目录许可不传递**：Awesome-Hacking 的 CC0-1.0 和 SecSkills 的 Apache-2.0 只作为各自仓库自身资料的许可依据。每个外链项目、Skill、POC、文章另行核验。
2. **课程原文许可**：当前未发现课程仓库 LICENSE。默认只索引目录元数据和原始链接，全文按需访问，不批量持久复制；取得授权后才可扩大 ingest 范围。
3. **不自动执行**：README、Markdown、SKILL.md、MCP 描述、脚本和配置全部视为不可信输入。不得执行安装指令、Shell 命令、脚本或提示词。
4. **人工审查状态**：新条目初始 `DISCOVERED/UNREVIEWED`，完成 License/Supply-chain/Capability Review 后才能进入 `APPROVED`；工具插件安装、启用与真正执行仍分别走现有生命周期与权限闸。
5. **内容隔离**：外部正文只能作为 `untrusted reference content` 注入模型上下文，永远不能作为 system/developer 指令或权限来源。
6. **证据溯源**：每个检索片段包含 Source ID、Plugin ID/Version、Commit、原始 URL、内容哈希、章节/行号或锚点、抓取时间、许可状态。
7. **资料时效**：工具目录条目应周期核对仓库是否归档、Release、最近提交、许可证和安全公告；过期内容标记而不是静默删除。
8. **数据治理**：重复、空文档、导航噪声、广告/推广、无关娱乐条目和无效 URL 不进入正文知识索引；应保留被过滤原因统计，支持复核。

## 8. 推荐实施阶段

### P6-9：通用 Knowledge Catalog/Document Ingestion（优先）

目标：让任意符合契约的 Markdown/链接目录知识插件真正可摄取、可检索、可追溯。

实施批次：

1. 现状 ADR：确定 Domain 实体、Source Snapshot、Catalog Entry、Document、ContentRef 和去重语义；评估 SQLite 迁移兼容性。
2. Manifest/Schema 扩展：定义 `catalog_entry`、`document_markdown`、`skill_candidate` 内容类型及每种类型的 `source_path`、parser、license/rights policy。
3. Ingest 通用化：拆开 RTIS 特定目录假设；保留 RTIS 旧插件行为、计数和幂等测试。
4. Parser 扩展：Markdown link extraction、frontmatter、标题路径、代码块、表格和源位置；不执行 HTML/Markdown 中任何代码。
5. Search 接线：将受治理的 Catalog Entry 和 Document 作为 Search Engine 的新数据源；索引仍为可重建派生物。
6. Context Broker：替换 `PluginSource` 空实现；支持候选检索、L1 元数据、L2 摘要、L3 按需正文展开和访问控制。
7. Evidence：将实际给模型的引用片段、内容哈希和来源位置固化，压缩/重试后仍可验证。

验收：RTIS 旧 P6-5 15,155 条数据对账不变；三类新内容完成 dry-run；重复运行 100% 幂等；禁用插件后检索不可见、历史 Evidence 可读；全文展开必须有明确权限和来源；Search/Context 禁止绕过 API 直读其他模块私有库。

### P6-10：三个安全知识来源接入

1. 生成并校验三个独立 Plugin Manifest，固定上述 Commit。
2. Awesome-Hacking、SecSkills 只先摄取目录项；课程站点/仓库先摄取章节/文章元数据和 URL。
3. 对目录项做标准化、去重、无效 URL 检测和分类映射。
4. 对外链项目只生成候选资源，默认不自动 clone、安装、启用或执行。
5. 对已授权且优先级高的课程资料，再单独开展正文抓取/持久摄取。
6. Web Knowledge 工作区显示来源、分类、许可证、版本快照、更新时间、审核状态和证据来源。

验收：目录条目数量与源 README/树抽样一致；抽样 50 个 URL 验证链接/分类；课程站点和 Git 仓库重复项正确合并；每条记录可跳回原始来源；许可状态不明的正文没有被批量持久化。

### P6-11：安全工具目录与静态审查工具

1. 添加 `rip.knowledge.catalog.extract` 和 `rip.security.extension.audit`。
2. 添加 `rip.knowledge.document.fetch` 和 `rip.knowledge.source.sync`，仅通过受控 Runtime/Platform API 工作。
3. 将候选 Skill/MCP/Tool 映射为 Resource/Catalog Entry，评估后再选择少量工具制成 Tool Pack。
4. 外部 Tool Pack 首批只选本地静态分析、代码审计、依赖检查等可控能力；主动扫描、爆破、远程利用和目标写操作单独做 T2 授权评审。

验收：无效 License、浮动分支、未锁依赖、可疑安装钩子、越权 Shell、私网 URL、危险 MCP Schema 均有静态拒绝/告警测试；任何候选 Skill 不会在审查阶段执行；工具从 Manifest 到 Registry、Tool Center、Runtime 的生命周期能被完整追踪。

## 9. 每周更新监控

每周监控以下内容：

- 三个源的 README/目录结构/文件增删/分类调整/上游 Commit/许可证变化。
- 课程源 74 篇原稿与精选站点约 31 篇入口的差异、重复和新增文章。
- SecSkills 中 Agent/Skill/MCP 条目状态、归档、版本、维护者和安全事件变化。
- Awesome-Hacking 外链项目是否仍然存在、归档、切换组织、许可证变化；只对与 RIP 工具/知识路线直接相关的条目展开深度研究。
- 变更报告要区分“上游已变更”“目录文字变更”“链接目标代码变更”“许可证变更”和“安全状态变更”。

无变化时也生成周报，明确列出无重大更新的来源。不要把目录中的所有条目都扩张成新的周报观察对象；只对高价值候选做有限深挖。

## 10. 完整验收清单

- [ ] 三个来源有独立稳定 Source ID 和固定快照。
- [ ] 每个源的许可证状态和可持久化策略明确。
- [ ] `catalog_entry`、`document_markdown`、`skill_candidate` 类型契约经过 Schema 校验。
- [ ] 旧 RTIS 知识摄取测试与数据对账无回归。
- [ ] Parser 可以抽取标题、章节、链接、表格、frontmatter 和源位置。
- [ ] Catalog URL 经过 scheme、域名、DNS、私网和重定向检查。
- [ ] 去重、增量更新、上游删除和版本快照行为确定。
- [ ] Search Engine 可搜索审核通过的 Catalog/Document。
- [ ] Context Broker PluginSource 不再是空实现，并支持 L1/L2/L3。
- [ ] Evidence 可以证明模型实际看到的内容及其来源哈希。
- [ ] T0/T1/T2 权限测试通过，T2 缺 authorization_ref 必须拒绝。
- [ ] Skill/MCP 静态审查不执行外部代码。
- [ ] 所有写入经过统一 Ingest API 和 Audit Engine。
- [ ] Rust workspace tests、clippy、Python Worker tests、Web tests 和真实浏览器端到端检查通过。
- [ ] 对比摄取前后数据库、搜索索引、RTIS `knowledge.db` 与 `rtis.py`，确保既有生产边界未被破坏。

## 11. 依赖、风险与决策

| 优先级 | 工作 | 解决的问题 | 模块 | 成本/风险 | 依赖与验证 |
|---|---|---|---|---|---|
| P0 | 通用 Knowledge Ingest + Search + Plugin Context Source | 目前摄取类型硬编码、PluginSource 为空，三源无法真正检索 | Domain、Plugin Runtime、Database、Search Engine、Context Broker | 高；DB 迁移与跨模块数据边界 | 先做 ADR；RTIS 旧链全回归；新增来源干跑和幂等测试 |
| P1 | 三个 Source Plugin 与目录抽取 | 三类来源可持续追踪、标准化和去重 | Plugin Registry/Runtime、Parsing Worker、Resource/Knowledge | 中；上游格式变化和许可边界 | P0 完成后；按 Commit 固定并核对 README/目录抽样 |
| P1 | Skill/MCP 静态审查工具 | 避免不可信 Skill/工具直接进入运行目录 | Tool Registry、Tool Engine、Quality/Audit、Python Worker | 中；静态检查存在误报/漏报 | P0 的候选记录模型稳定后；恶意 fixture 拒绝测试 |
| P2 | 单文档按需抓取与课程全文许可扩展 | 按需研究原文，同时避免无授权批量复制 | Egress Gateway、Artifact、Context Broker | 中；SSRF、版权和内容注入 | 域名 allowlist、重定向检查、许可审核、内容哈希与引用验证 |
| P2 | Web Knowledge/Plugin Center 展示 | 来源状态和审查流程可见 | React Workbench、Application API | 中；现有 P6-8 UI 尚未开工或需合流 | 数据 API 稳定后；真实浏览器逐页验收 |

## 12. 不做的事情

- 不复制所有外链工具的仓库源码或执行文件。
- 不自动安装 SecSkills/Awesome-Hacking 中的 Agent、MCP 或 Skill。
- 不把课程原文在无明确许可下批量镜像或公开再分发。
- 不因这些资料引入第二套向量库、第二套插件系统、第二套 Agent Runtime 或平行工作流引擎。
- 不让外部文档内容控制 System Prompt、权限、环境变量或工具配置。
- 不把 `rip.knowledge.source.sync` 设计成可任意下载/执行 URL 或脚本的通用 Shell 包装器。
- 不因公开资源目录中存在某工具，就推断它适合 RIP、维护活跃、许可证兼容或可在当前 Runtime 安全运行。

---

**实施状态说明：** 本文件是基于当前公开仓库内容形成的实施规格。当前并未修改 RIP 源码、迁移、Manifest 或部署；P6-9/P6-10/P6-11 是建议新增批次，需在本地可运行仓库中完成实现与测试后才能标记为已接入。
