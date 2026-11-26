# 自有 POC 标准 FTS 搜索索引接入

日期：2026-11-26

## 目标与边界

将 RIP curation Overlay 明确审核的自有 RTIS POC 纳入标准 `rp-search-index` FTS5 派生索引，使其可与现有资源在同一搜索入口检索。索引只纳入 Overlay 明确列出的记录；未进入 Overlay 的 POC 不会因本次索引而自动获得审核状态或可信度。

数据源保持只读：

- Platform staging DB：原有 `resource`、`resource_source`、插件知识来源。
- Catalog DB：原有 `github_metadata`。
- RTIS DB：只按 Overlay 中的 `knowledge_id` 读取 POC 元数据。
- RIP curation Overlay：提供 K0-K4、独立 Lifecycle、来源路径、Commit 和关系投影。

本次 RTIS 查询只选择 `poc.id`、`code`、`title`、`kind`、`category`、`cve`、`component`、`severity`、`tags`。没有读取 POC `description`、`usage`、`content`、`verified_note`；没有执行敏感信息审查、凭据扫描或脱敏处理，也没有修改 RTIS 原始数据库。

## 索引结构

- `search_index_doc` 新增自有 POC 专属字段：`evidence_level`、`knowledge_lifecycle`、`poc_category`、`poc_severity`、`poc_cve_id`、`source_path`、`source_commit`、`related_poc_ids_json`。
- `resource_type=rtis_poc`，稳定 ID 形如 `rtis:poc:<id>`。
- `evidence_level` 与 `knowledge_lifecycle` 分开保存和过滤；不复用平台 `resource.lifecycle`，也不把 Nuclei severity 字段当作自有 POC severity。
- FTS 可检索 POC 标题、类型、分类、CVE、组件、严重程度、tags、Overlay 技术/方法标签、来源路径和 Commit。
- `SearchHit` 返回 K 级、Lifecycle、分类、严重程度、CVE、来源路径、Commit、关联 POC ID 和来源标注。
- `rp-search-unified` 的 SQLite 通道透出同一组 POC facet/source/relation 字段；`--sqlite-filters` 支持 `evidence-level`、`knowledge-lifecycle`、`poc-severity`、`poc-category`、`poc-cve-id`。
- 索引 schema 已升级到 6（v4/v5 均需通过 `--rebuild` 重建）；索引仍是可删除、可重灌的派生数据。

## 构建方式

```bash
cargo run --offline -p search-engine --bin rp-search-index -- build \
  --platform-db rip-data/staging/platform.db \
  --catalog-db rip-data/staging/platform.db \
  --rtis-db integrations/rtis/knowledge.db \
  --curation-overlay integrations/rtis/curation/k_meta_overlay.json \
  --index-db /tmp/rip-search-index.db --rebuild
```

`--rtis-db` 与 `--curation-overlay` 必须成对提供。三个源数据库均以只读模式打开；只有 `--index-db` 是写入目标。

## 检索方式

```bash
cargo run --offline -p search-engine --bin rp-search-index -- query \
  --index-db /tmp/rip-search-index.db \
  --keywords zeai --resource-type rtis_poc \
  --evidence-level K2 --knowledge-lifecycle Researched \
  --poc-severity high --limit 20
```

新增过滤参数：

- `--evidence-level K0|K1|K2|K3|K4`
- `--knowledge-lifecycle Candidate|Researched|Lab-validated|Field-validated|Verified|...`
- `--poc-severity <severity>`
- `--poc-category <category>`
- `--poc-cve-id <CVE>`

`rp-search-unified` 的 SQLite 通道也支持相同 facet，通过 `--sqlite-filters` 传入，并在 `detail` 返回 K 级、Lifecycle、来源路径、Commit 和关联 POC IDs：

```bash
rp-search-unified --query zeai --index-db /tmp/rip-search-index.db \
  --sqlite-filters resource-type=rtis_poc,evidence-level=K2,knowledge-lifecycle=Researched
```

该统一命令同时需要可用的 PostgreSQL DSN；上述过滤条件只作用于 SQLite 通道。

原有 `--nuclei-severity`、`--nuclei-protocol`、`--cve-id`、`--cwe-id` 仍专用于 Nuclei 字段，不与自有 POC facet 混淆。

## 实测结果

使用 staging Platform DB、RTIS 原始只读 DB 和当前 Overlay，在临时索引库执行全量重建：

| 项目 | 结果 |
|---|---:|
| 索引文档总数 | 130 |
| 原有 github_repo | 79 |
| 原有 github_page | 2 |
| Overlay POC | 49 |
| FTS 行数 | 130 |
| K0 / Candidate | 29 |
| K1 / Researched | 9 |
| K2 / Researched | 10 |
| K3 / Lab-validated | 1 |
| POC 来源路径 | 41 |
| POC 来源 Commit | 38 |
| 至少一条关联关系的 POC | 25 |
| 空字符串 CVE 字段 | 0 |

关键词检索验证：`zeai` 命中 3 条、`OpenAPI` 命中 3 条、`ThinkPHP` 命中 3 条、`gateway` 命中 2 条。K0 + Candidate 组合过滤命中 29 条；K2 + Researched 等组合过滤已由端到端单元测试验证。

## 自动化验证

- `cargo test -p search-engine --offline`：22 个单元测试通过，另有 2 个 `rebuild_after_restore` 集成测试通过。
- 新增 `rtis_curated_pocs_are_indexed_with_overlay_facets_and_relations`：验证只读 POC 元数据、Overlay facet、来源 Commit、关系投影及组合过滤。
- `b5_rtis_kit_directory_untouched`：保护范围调整为 RTIS 生产资产；只排除 RIP 自有 `integrations/rtis/curation/` Overlay 路径，测试通过。
- `mcp_subtask_roundtrip_and_caller_attribution`：测试环境显式不挂载与派发回归无关的 RTIS / curated-KB 上下文源，保留真实 MCP → rp-agent 子进程和 caller attribution 验证，单项测试通过。
- `cargo test --offline -p unified-search`：3 个测试通过，覆盖跨库归并、统一结果字段投影与 CLI 的 POC facet 解析。


## Workspace 最终回归

- `cargo fmt --all -- --check`：通过。
- `cargo clippy --offline --workspace --all-targets -- -D warnings`：通过。
- `cargo test --offline --workspace`：197 个 test targets，1,421 passed、0 failed、6 ignored，退出码 0。
- `python3 integrations/rtis/curation/validate_overlay.py`：49 条 Overlay、25 条关系验证通过。
- 两项 ignored Nuclei 全量隔离摄取审计：14,111 条模板，14,109 parsed、1 invalid YAML、1 body unavailable；legacy 15,084 条；Overlay 49 条、25 条关系，全部通过。
- RTIS 原始 `integrations/rtis/knowledge.db` SHA-256 仍为 `7b6083f8e1e84c9aff60a3d4f429f29cfc436ef1aafa7221d4d3cd945df641a7`。

所有构建与搜索验证均使用临时索引库；未修改 RTIS 原始数据库、staging Platform DB 或既有产品搜索索引。未执行 Git commit / push。

## Knowledge Web 工作台接入（2026-11-26）

标准 FTS 现已通过 `GET /api/curated-pocs/search` 接入 Knowledge 页「自有 POC」Tab。Web 可按关键词、K0–K4、Lifecycle、Severity、Category、CVE 筛选，并查看来源路径、Commit 和关联 POC ID。服务端在 `rip-data/derived/curated-poc-search.db` 维护独立派生索引，源数据库只读；不改写既有 `migrations/platform-search.db`。接口只返回 Overlay POC 元数据，不读取正文或 `verified_note`。三语言 UI、HTTP 鉴权和 API 契约均已补齐。

验证：`apps/server/selftest_poc_search.sh` 通过；`apps/server/selftest_phase9_rest.sh` 33 PASS / 0 FAIL；Web 22 个测试文件 / 156 个测试通过；`npm run build` / `npm run typecheck` 通过。完整实现与限制见 `docs/knowledge/POC-WEB-SEARCH-INTEGRATION-2026-11-26.md`。

## 2026-11-27：Search Index schema v6

schema v6 在 v5 的证据治理字段上增加 `poc_source_urls_json`，同时保留 `related_entities_json`。`SearchHit` 现在返回：K 级、Lifecycle、Severity、Category、CVE、来源路径、Commit、上游 URL、内部关联 POC、外部案例组、来源完整性、核验结论、去重/独立知识状态、缺失证据、证据引用和审核门槛。

新增 CLI filters：

- `--poc-source-integrity complete|partial|missing`
- `--poc-verification-result candidate_not_verified|historical_evidence_supported|not_applicable`
- `--poc-dedup-status independent|related-family|duplicate_candidate|duplicate`
- `--poc-missing-evidence true|false`

FTS 同时索引上游来源 URL 和外部案例组引用。外部案例节点必须在 Overlay `external_entities` 中显式声明，否则 Overlay validator 或索引构建失败。旧 schema v4/v5 索引需 `--rebuild`。

真实 Overlay 当前有 49 条记录、25 条关系、2 个外部案例组；治理字段完整率 49/49，33 条记录有 95 个待补证项。来源路径 41/49、Commit 38/49、上游 URL 9/49。剩余缺失路径/Commit均如实保留，不推断或生成。

`rp-search-unified` 的 `--sqlite-filters` 解析器也已同步支持上述四个新 facet：`poc-source-integrity`、`poc-verification-result`（含 `not_applicable`）、`poc-dedup-status`、`poc-missing-evidence=true|false`；SQLite 结果 detail 会透传 `poc_source_urls`、来源/证据治理字段及外部案例关系，不再只暴露旧版 K/Lifecycle/路径字段。
