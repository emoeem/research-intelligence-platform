# RIP 自有 POC Web 检索接入（2026-11-26）

## 目标与范围

将前一阶段完成的 RTIS + RIP curation Overlay FTS 检索能力接入 Knowledge 工作台。新增「自有 POC」视图，不改变既有「平台知识」与「知识来源」视图。

仅展示 Overlay 明确列出的 `rtis_poc` 元数据；未列入 Overlay 的 POC 不会因搜索接入而自动获得审核状态或可信度。

## 实现

### HTTP API

新增 `GET /api/curated-pocs/search`，受现有 Bearer 鉴权、全局限频与访问审计机制保护。

支持参数：

- `q`：关键词，最多 200 字符。
- `evidence_level`：K0–K4。
- `knowledge_lifecycle`：Candidate / Researched / Lab-validated。
- `poc_severity`：critical / high / medium / low / info。
- `poc_category`：分类标识。
- `poc_cve_id`：CVE 编号。
- `limit` / `offset`：分页，limit 上限 100。

必须提供关键词或至少一个结构化筛选条件；非法 facet 返回 HTTP 400。搜索结果包括 resource ID、标题、K 级、Lifecycle、分类、严重程度、CVE、来源路径、来源 Commit、关联 POC ID 和搜索评分。

### 派生索引生命周期

- 默认索引：`rip-data/derived/curated-poc-search.db`。
- 可通过 `RP_CURATED_SEARCH_INDEX_DB` 指定其他路径。
- 可通过 `RP_SEARCH_INDEX_BIN` 指定 `rp-search-index` 可执行文件。
- 默认 catalog、RTIS DB 与 Overlay 路径分别为 `migrations/sources-snapshot/catalog.db`、`integrations/rtis/knowledge.db` 和 `integrations/rtis/curation/k_meta_overlay.json`；均可通过对应 `RP_SEARCH_*` 环境变量覆盖。
- 索引不存在或源文件 mtime 更新时，服务端在进程内互斥锁下重建派生索引。
- 平台 DB、catalog DB、RTIS DB 均由 Rust 索引器以只读模式打开；只有单独的派生索引可写。
- 不复用或覆盖既有 `migrations/platform-search.db`。

### 元数据边界

该端点不会读取或返回 RTIS POC 的 `description`、`usage`、`content`、`verified_note` 等正文/注记字段。搜索结果的 `description` 为空；UI 只呈现结构化审核字段与溯源元数据。

K 级与 Lifecycle 独立过滤，不使用旧版 `verified=1` 推断 K2/K3。K0 仍以 K0 展示，不会因被搜索命中而升级。

### Web 工作台

Knowledge 页面新增「自有 POC」Tab：

- 关键词搜索（300ms 防抖）。
- K0–K4 证据等级筛选。
- Candidate / Researched / Lab-validated 生命周期筛选。
- Severity、POC Category、CVE 筛选。
- 展示来源路径、Commit 与关联 POC ID。
- 包含空态、加载态、错误态、重试和清除筛选。
- zh-CN / en-US / ja-JP 三种界面语言均已补齐。

## 验证

- `python3 -m py_compile apps/server/rp_server.py`：通过。
- `apps/server/selftest_poc_search.sh`：通过。真实 HTTP + Bearer 测试覆盖关键词命中、K2 + Researched、K0 + Candidate、Category、Severity、非法 K 级、非法 CVE、空查询拒绝、未授权 401、惰性索引构建。
- 搜索端点 E2E：`zeai` 命中 3 条；K2 过滤命中 2 条；K0 + Candidate 组合命中 1 条。
- 测试前后 `migrations/platform.db`、catalog DB、RTIS DB 与 curation Overlay 的 SHA-256 完全一致；临时索引写入测试临时目录。
- `apps/server/selftest_phase9_rest.sh`：33 PASS / 0 FAIL；REST 路由清单、鉴权、限频、审计和契约文档对账通过。
- `cd web && npm test`：22 个测试文件、156 个测试全部通过。
- `cd web && npm run build`：TypeScript 检查与 Vite 生产构建通过。
- `cd web && npm run typecheck`：通过。

## 明确未做

- 未对 POC 原始内容进行敏感信息审查、凭据扫描或脱敏。
- 未扫描公网目标、执行 POC 或触发真实验证码/短信流程。
- 未修改 RTIS 原始 POC 或 RTIS 原始数据库。
- 未改写既有 `migrations/platform-search.db`。
- 未执行 Git commit / push。

## 2026-11-27 治理工作台补强

### API 与索引 schema v6

新增 `GET /api/curated-pocs/summary`，只读取 Overlay JSON，返回 49 条记录的 K 级/Lifecycle/来源完整性/核验结论/去重分布、证据缺口、路径/Commit/URL 覆盖、关系与待补证数量。

`GET /api/curated-pocs/search` 额外支持：

- `poc_source_integrity`: complete / partial / missing。
- `poc_verification_result`: candidate_not_verified / historical_evidence_supported / not_applicable。
- `poc_dedup_status`: independent / related-family / duplicate_candidate / duplicate。
- `poc_missing_evidence`: true / false。
- `poc_verification_result` 同时支持 `not_applicable`，专用于非目标漏洞验证的工具/方法记录。

SearchHit 新增 `poc_source_integrity`、`poc_verification_result`、`poc_dedup_status`、`poc_independent_knowledge_status`、`poc_missing_evidence`、`poc_evidence_refs`、`poc_review_gate`、`poc_source_urls` 和 `related_entities`。外部案例组关系不会再从检索结果中静默丢失。上游 URL 只作为 provenance 输出，不会由服务端自动抓取。

索引 schema 从 v5 升级为 v6，旧索引必须完整重建。源数据库仍只读，派生索引仍是唯一写入目标。

### Web 工作台

默认打开 33 条证据待补队列，提供来源完整性、核验结论、去重状态和缺失证据筛选；显示每条的缺失证据、review gate、source path、commit、upstream URLs、内部 POC 关系和外部案例组。汇总卡展示纳入总数、待补证队列、缺失证据项、关系数量、治理元数据待补数和缺少 Commit 数。简中/英文/日文均已补齐。

### 当前数据覆盖

- Overlay 记录：49；四个治理字段完整率：49/49。
- 证据缺口：33 条 / 95 个缺失项；全部有 review gate。
- 来源路径：41/49；Git Commit：38/49；上游 URL：9/49。
- 关系：25 条，含 2 个显式外部案例组，未解析端点 0。
- 仍缺来源路径的 8 条：7 条仅有上游工具 URL，ID 16 历史源文件缺失。
- 仍缺 Commit 的 11 条：8 条无本地路径，ID 900061–900063 的源文件没有 Git 历史。

### 验证

- `python3 integrations/rtis/curation/validate_overlay.py`：49 条、2 个外部案例组、25 条关系通过。
- `cargo test -p search-engine --offline`：22 单测 + 2 集成测试通过。
- `apps/server/selftest_poc_search.sh`：真实 HTTP/Bearer E2E 全部通过，包含缺失证据队列、来源 URL 检索、外部案例组关联、`not_applicable`、汇总统计与未授权 401。
- 测试前后平台 DB、Catalog DB、RTIS DB 和 Overlay SHA-256 不变；索引仅写临时目录。

敏感信息审查/凭据扫描/脱敏仍不在本阶段范围内；未执行公网验证、真实短信或支付流程。
