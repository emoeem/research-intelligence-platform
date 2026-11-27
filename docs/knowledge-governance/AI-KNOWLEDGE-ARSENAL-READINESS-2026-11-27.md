# AI Knowledge & Arsenal Readiness Audit

Date: 2026-11-27  
Status: **P0 baseline complete; P1 LLM Planner/tool contract implemented (real Provider E2E pending); P2 Skill/Tool/Knowledge candidate graph implemented; POC/Nuclei/CVE governance queues generated; candidate-aware retrieval implemented; human review and live-provider acceptance pending**  
Scope: read-only inventory of the current Platform, active plugin KSE, RTIS POC database and live derived search projection. This audit does not execute a PoC/template, contact a target, promote review states, or mutate source databases.

## 1. Executive decision

RIP has substantial raw source material and useful platform infrastructure, but it does **not yet meet the original goal of a complete, dependable knowledge base and weapon arsenal that AI agents can use end-to-end**.

The main blocker is not simply the count of 46 Platform Knowledge rows. The data is split across independent authorities and the runtime integration is incomplete:

- Platform `knowledge` has 46 governed records; this is not the total number of ingested source records.
- Active KSE versions contain 17,471 source entries; 13,797 remain `DISCOVERED` and only 3,659 are `APPROVED`.
- RTIS has 14,721 POC/template records, but only 49 curated POCs are represented in the current derived search projection.
- 125 Skills exist, but **0/125 declare `required_tools` and 0/125 declare `knowledge_refs`**.
- Platform has 20 registered tools, of which 3 are degraded and 1 is missing.
- The Platform database has 0 `agent_definition`, 0 `agent_run`, and 0 `workflow_definition` rows. The 13 legacy `workflow` rows are not evidence of a populated, runnable Agent catalog.

Therefore the correct status is **data-rich, agent-readiness incomplete**. Do not solve this by bulk-promoting entries, copying all raw records into `knowledge`, or inflating the visible total.

## 2. Source-of-truth inventory

All SQLite stores passed `PRAGMA integrity_check = ok` during this audit.

| Store / table | Count | Meaning and caveat |
|---|---:|---|
| `migrations/platform.db:knowledge` | 46 | Governed Platform knowledge records; 37 `VERIFIED`, 7 `CANDIDATE`, 2 `DISCOVERED` |
| `migrations/platform.db:resource` | 81 | 79 GitHub repositories + 2 pages; quality A 4 / B 28 / C 44 / D 5 |
| `migrations/dev-p6.db:knowledge_source_entry` | 18,102 | Includes historical plugin versions; not 18,102 unique current resources |
| Active KSE version rows | 17,471 | APPROVED 3,659 / DISCOVERED 13,797 / REJECTED 15 |
| `integrations/rtis/knowledge.db:poc` | 14,721 | RTIS authoritative POC/template records; not all are independently validated exploit POCs |
| `rip-data/derived/curated-poc-search.db:search_index_doc` | 3,789 | Rebuildable search projection, not a content authority |
| Search FTS rows | 3,789 | One-to-one with the current derived projection |

### Active KSE source distribution

| Active source | Entries | APPROVED | DISCOVERED | REJECTED |
|---|---:|---:|---:|---:|
| Awesome-Hacking 0.1.3 | 84 | 6 | 75 | 3 |
| CISA KEV 0.1.2 | 1,805 | 1,805 | 0 | 0 |
| Nuclei Templates 0.1.1 | 14,111 | 552 | 13,559 | 0 |
| PoC Index KEV 0.1.0 | 1,296 | 1,296 | 0 | 0 |
| SecSkills 0.1.3 | 101 | 0 | 89 | 12 |
| Tongui Course Notes 0.1.2 | 74 | 0 | 74 | 0 |

The 18,102 total includes historical Awesome-Hacking, SecSkills and Tongui plugin versions. Current totals must be calculated from the active `(plugin_id, plugin_version)` pair, not by summing every historical version.

`APPROVED` means source/classification review passed; it does not mean a Nuclei template detected a vulnerability successfully or a PoC is currently reproducible.

### RTIS POC / weapon inventory

| Source | Records |
|---|---:|
| Nuclei | 14,111 |
| Web | 451 |
| Self | 139 |
| Manual | 12 |
| KB | 7 |
| Null / unknown | 1 |

Kinds are dominated by templates: 14,125 templates, 444 payloads, 74 scripts, 41 POCs, 17 rules, 10 EXPs, 4 cases, 2 methods and 4 rows with no kind.

Field coverage in the 14,721 RTIS rows:

- CVE reference: 4,413 rows.
- Non-empty affected `versions`: 39 rows (14,682 missing).
- Non-empty description: 8,438 rows (6,283 missing).
- Non-empty source URL: 14,572 rows (149 missing).
- Non-empty `content`: 605 rows.
- Legacy `verified=1`: 134 rows. This is a historical flag only; it is not current lab verification and must not be promoted as such.

Important storage distinction: the Nuclei YAML source bodies are present in KSE `knowledge_source_entry.body` (all 14,111 active Nuclei entries have non-empty `body_bytes`). The RTIS `poc` projection's `usage` field is primarily a CLI invocation, not the YAML template body; its `content` field is empty for the Nuclei source. Do not mistake the RTIS projection for the authoritative template corpus.

The current search projection contains 3,107 catalog entries, 552 Nuclei templates, 79 GitHub repositories, 49 curated RTIS POCs and 2 pages. It intentionally does not index all 14,721 RTIS rows as searchable trusted POCs.

## 3. Governance and quality gaps

- RTIS enrichment tables contain 28 `k_meta`, 17 `k_negative`, and 34 `k_relation` rows. These do not yet provide comprehensive evidence/lifecycle/negative-knowledge coverage for the full POC corpus.
- The curated overlay has 49 entries and 25 relations. Its evidence distribution is K0 29 / K1 9 / K2 10 / K3 1 / K4 0; 33 records still have 94 evidence gaps. Curated does not mean verified.
- Tool registry: 20 entries; 16 `installed/ok`, 3 `installed/degraded`, 1 `missing/unknown`. Tool registry presence is not proof that a tool can safely execute in a particular task context.
- Skill registry: 125 entries, 119 enabled and 125 with a body reference; **none has a non-empty required-tool mapping or knowledge reference mapping**.
- Workflow registry: 13 legacy rows; all have phases, related Skills, knowledge queries and stop conditions, but only 4 have non-empty prerequisites. `workflow_definition` is empty.
- Agent registry: `agent_definition` and `agent_run` are both empty in the audited Platform DB. Agent UI/API route availability is not proof that runnable Agent definitions exist.
- Search evaluation: M4 sampled acceptance was recorded, but the 140-pair batch still has 0 saved human grades and numerical relevance metrics remain null. AI advisory grades are not human labels.

## 4. What this means for AI usage

The current platform can search a useful subset of metadata and expose Skills, Tools, Workflows and evidence records. It cannot yet demonstrate a reliable autonomous research loop because:

1. Skills are not mapped to the tools they require or the knowledge they depend on.
2. There are no persisted Agent definitions in the audited Platform registry.
3. Many POC/template records lack affected-version metadata and current validation evidence.
4. The full RTIS POC corpus is not the same thing as the 49-item curated search overlay.
5. Search quality still lacks a saved human-judged cross-source benchmark.

## 5. Implementation sequence

### P0 — Inventory and truthful counters (complete)

- Add the repeatable read-only audit command `scripts/security-intel/audit_ai_knowledge_arsenal.py`.
- Keep source-row, active-version, curated, indexed, approved, and runtime-verified counts separate.

### P1 — Unified search consumption (UI integration complete; Agent registry rollout pending)

- Connect the global search palette to authenticated `/api/search` with debounce.
- Preserve server-side hybrid ranking and distinguish Knowledge Cards, source entries, Nuclei templates and curated POCs.
- Provide deep links into the relevant Knowledge views; keep tool/task/page navigation available.
- Regression coverage now passes for query debounce, result type and source-filtered Knowledge deep-link behavior (web: 22 files / 159 tests).
- The authenticated API/UI path is connected. The unified-search tool manifest exists and passed isolated cross-store acceptance, but it is not yet registered in the audited production `platform.db`; runtime rollout must target an explicitly selected development/test DB first.
- Search indexing now retains active plugin rows in two explicit states: `APPROVED`→`ACTIVE` and `DISCOVERED`→`CANDIDATE`. Default retrieval is fail-closed to `ACTIVE`; `include_candidates=true` explicitly opens the candidate lane and returns `governance_status` + `lifecycle_status` with every hit.
- The RTIS search projection now includes non-curated, non-Nuclei weapon metadata as `rtis_weapon_candidate` (title/tags/CVE facets only; no body, usage, descriptions, verification notes or local source files). These records are excluded from default retrieval and are only visible through the candidate lane. The 49 curated POCs remain a separate reviewed overlay projection.
- Candidate indexing is a derived FTS projection only. It does not change RTIS/KSE source review states, `verified` flags, or source content.

### P1 — Model-backed Agent Planner (implementation complete; live-provider acceptance pending)

- Added the model-backed `LlmPlanner` to `agent-engine`; `rp-agent --planner llm` and Workflow Agent nodes share the same planner path.
- Reuses `async-openai 0.43.0` for OpenAI-compatible DeepSeek, Moonshot/Kimi and local endpoints; keeps the existing Agent Loop, Tool Engine, capability checks, approvals, evidence and trace path.
- JSON-only, one-step planning; strict Serde parsing; the model can select only a capability-bound tool present in Broker L2. Tool Engine still performs the final schema/workspace/permission checks.
- API keys are read from a named environment variable only. HTTPS is required except exact loopback endpoints. Request timeout is bounded.
- Token preflight labels model-matched HF tokenizer counts separately from conservative estimates; Provider-reported usage calibrates future estimates upward only. The BGE embedding tokenizer is not reused as a generation tokenizer.
- Local mock OpenAI-compatible server verifies request shape, bearer auth, JSON mode, usage parsing and timeout. Provider/Planner errors now produce `Decision::Abort`, `FAILED` Agent Run state and a Guardrail audit event. AgentLoop integration verifies LLM Planner → Broker → Tool Engine → Artifact/Evidence. Real-provider test remains pending until a local Secret/environment injects an API key.

### P2 — Skill / Tool / Knowledge mapping (candidate pipeline implemented; human review pending)

- Added typed `skill` identities to the existing M1 graph through `migrations_pg/V0013__skill_entity_type.sql`; no legacy rows are copied and no review state is changed by this migration.
- Added an atomic graph service for Skill → `security_tool` `USES_TOOL` candidate creation with a content-addressed Skill-body Evidence link. The operation creates only `CANDIDATE` relations and `UNREVIEWED` source assessments; it cannot approve a relation.
- Added `scripts/security-intel/suggest_skill_tool_mappings.py`, a deterministic, read-only candidate generator. It constrains Skill body reads to the repository tree, records body SHA-256 + line snippets, distinguishes command/section/metadata mentions, excludes negative mentions and reports tool health. It never updates `platform.db`.
- Current generated batch: 146 exact-mention candidate pairs across 67 of 125 Skills; 6 negative mentions were excluded. Candidates reference 17 of the 20 registered tools. 120 pairs are eligible for consideration only after review based on current Skill/tool enabled/governance/install/health flags; this is not an approval or authorization count.
- Added `rp-skill-map`: dry-run is default; `--apply` writes atomically to the configured PostgreSQL graph, attaches Evidence and leaves all relations `CANDIDATE` and source assessments `UNREVIEWED`. Full-batch disposable-PG acceptance imported all 146 candidates and verified 146 `CANDIDATE` relations, 146 Evidence links, 67 Skill identities, 17 Tool identities, 67 shared Evidence identities and 151 `UNREVIEWED` source assessments. A second full import created 0 duplicate relations and 0 duplicate Evidence links. No relation was approved.
- Exact CVE matching between Skill bodies and the current 46 Platform Knowledge rows produced no direct Knowledge Card matches; no `knowledge_refs` were inferred from those cards. A separate read-only join against active KSE Nuclei classifications found 42 Skill↔CVE pairs (14 Skills / 37 CVEs), 131 Skill↔Nuclei-template pairs (82 unique templates), and 36/42 CVE pairs also present in the active CISA KEV source. Of the matched template rows, 125 are KSE `APPROVED` and 6 `DISCOVERED`.
- Added `scripts/security-intel/suggest_skill_knowledge_mappings.py` and `rp-skill-ref-map`. The generator uses exact CVE identifiers in Skill body lines and active KSE Nuclei `classification.cve-id`; it does not fuzzy-match titles. It creates candidate `REFERENCES` relations to `vulnerability` and `detection_template` identities, with immutable Skill-body Evidence. A full 173-row import and replay against disposable PostgreSQL created 173 candidate relations and 173 evidence links on the first pass, then 0 duplicates on replay. No relation was approved and all new graph source assessments remained `UNREVIEWED`.
- The 36 CISA KEV matches use the active CISA source snapshot. The remaining six vulnerability identities are explicitly derived from linked Nuclei classifications; this distinction is retained in relation evidence. The graph does not treat KSE `APPROVED` as automatic graph source approval.
- Exact cross-source CVE identity audit joins active CISA KEV (1,805), PoC Index (1,296), Nuclei classifications (4,357 unique CVEs) and RTIS weapons (4,358 unique CVEs) into 5,658 union CVE identities. Exact intersections: CISA∩PoCIndex 1,296; CISA∩Nuclei 505; CISA∩RTIS 505; Nuclei∩RTIS 4,357. Original source snapshots remain independent; no destructive source merge occurs.
- Added `scripts/security-intel/audit_poc_governance.py` and `poc-governance-candidates-2026-11-27.json` for the 610 non-Nuclei RTIS weapon records. The report distinguishes 41 `poc` rows, 444 payloads, 74 scripts, 17 rules, 10 EXPs, 14 templates, 4 cases, 2 methods and 4 unknown types. Only 28/610 have structured governance metadata; 39/610 have affected-version metadata; 134 carry a legacy `verified=1` flag; 460 have a source URL; 125 have a local source file; no duplicate code/content hashes were found. Static scanning flagged 237 records with executable/network behavior and 35 records with possible secret/credential indicators, including 2 historical verification notes. Raw content, paths and notes are not exported.
- Added sanitized human-review batch `poc-review-batch-001-2026-11-27.json` for all 41 true PoC-kind records: P0 18, P1 8, P2 13, P3 2. Added `poc-sensitive-review-batch-001-2026-11-27.json` for all 35 records with possible secret indicators. Neither batch includes payloads, raw notes or secret values; no test was run against a target.
- Added `scripts/security-intel/audit_vulnerability_nuclei_governance.py`, producing the exact-CVE cross-source identity index and Nuclei review candidates. Nuclei has 14,111 active templates: 14,109 metadata-parsed, one invalid YAML and one unavailable body; 552 APPROVED and 13,559 DISCOVERED. The first sanitized review batch contains the two parser exceptions and 198 high/critical templates linked to CISA KEV. All remain candidates; engine validation and target validation are explicitly `NOT_RUN`.
- Added `build_nuclei_review_batch.py` to page through the full 14,111-template review queue in bounded batches. It exports metadata/provenance only, never template bodies.
- `skill.required_tools` and `skill.knowledge_refs` remain unchanged until batch review. Candidate files are `docs/knowledge-governance/skill-tool-mapping-candidates-2026-11-27.json` and `docs/knowledge-governance/skill-knowledge-candidates-2026-11-27.json`.
- `skill.required_tools` and `skill.knowledge_refs` remain unchanged until batch review.

### P2 — Remaining mapping review and projection work

- Reuse the M1 PostgreSQL `knowledge_entity_refs` / `knowledge_relations` authority; do not create a competing graph or a duplicate facts store.
- Add the missing typed `skill` identity and explicit, reviewable relations between Skills, security tools, knowledge cards, vulnerabilities, templates and POCs.
- Generate mapping candidates from Skill text, workflow references and tool manifests; record why each candidate was suggested. Candidates must remain unapproved until reviewed with source evidence.
- Backfill the existing `required_tools` / `knowledge_refs` projections only from approved mappings; never bulk-infer and silently activate them.

### P3 — Agent catalog and execution contract

- Populate Agent definitions from explicit role/capability manifests, with versioned tool allowlists, context budgets, authorization/scope gates, stop conditions and output schemas.
- Make Agent retrieval consume the same unified search and reviewed identity graph; return citations/source versions/evidence status in context.
- Test the complete Agent → Context Broker → Skill → Tool Engine → Evidence/Trace path in an isolated workspace.

### P4 — Arsenal completeness and lifecycle

- Structure Nuclei metadata and preserve YAML bodies/source pins separately from RTIS command-line usage projections. (Metadata classification and exact-CVE cross-source inventory are now present; full Nuclei engine `-validate` remains uncompleted because the local CLI validation invocation did not return within the bounded probe. Do not mark templates engine-validated.)
- Improve affected-version, advisory, source URL, license, remediation, test and negative-case coverage.
- Add source-change/removal monitoring and explicit parser-valid / static-reviewed / lab-tested / target-observed states.
- Expand tools by security domain only after verifying provenance, license, maintenance, install state and isolation policy.

### P5 — Knowledge domain coverage and retrieval quality

- Build a coverage matrix for web/API, cloud/container, mobile, Windows/Linux, network, identity/AD, IoT/firmware, code audit, reverse engineering, AI/LLM security, detection and incident response.
- Ingest from pinned authoritative sources; deduplicate conservatively; keep unreviewed material out of trusted Agent context.
- Build a human-judged benchmark with exact identifiers, conceptual questions, cross-source conflicts, Chinese/English/Japanese queries and negative/abstention cases. The current exact-CVE self-consistency evaluation is not a substitute for human qrels.

### P6 — End-to-end acceptance

- Evaluate retrieval Recall@K / MRR / NDCG and duplicate rate on human-judged queries.
- Test real research tasks in authorized labs only; do not treat API 200, static parser success, or source approval as execution validation.
- Require every Agent conclusion to link to source snapshots and execution evidence; test that missing evidence causes abstention rather than fabricated certainty.

## 6. Safety and data boundaries

- This audit was read-only. No source database was modified.
- No PoC or Nuclei template was executed; no target was contacted.
- Do not change existing plugin review states or legacy `verified` flags to inflate counts.
- Do not merge records based only on titles, shared corpus URLs or fuzzy similarity.
- Keep the original RTIS DB and `migrations/platform.db` outside write scope. Use disposable clones for migrations and mapping experiments, then verify backup/restore before any persistent rollout.
- External source content and Skill bodies are untrusted data. They cannot grant tool permissions or override Agent policy.

## 6.1 六域资产审计补充（2026-11-27）

已进一步拆分知识、漏洞、POC、Nuclei 模板、工具和技能六个域，分别统计库存、审核状态、字段质量和来源覆盖。完整结果见：

- `docs/knowledge-governance/ASSET-DOMAIN-INVENTORY-2026-11-27.md`
- `docs/knowledge-governance/asset-domain-inventory-2026-11-27.json`
- 可复现审计器：`scripts/security-intel/audit_asset_domains.py`

本次补充仍为只读盘点；没有执行模板/POC，也没有更改任何审核状态。特别注意：Nuclei 元数据 `parsed` 不等同于引擎验证，RTIS 的历史 `verified=1` 不等同于当前实验室复现。

## 7. Reproduction

Run from repository root:

```bash
.venv-check/bin/python scripts/security-intel/audit_ai_knowledge_arsenal.py
```

The command opens all four databases in SQLite read-only/query-only mode and emits JSON to stdout. Alternate database paths can be supplied through `--platform-db`, `--plugin-db`, `--rtis-db` and `--search-db`.

Generate Skill→Tool mapping candidates without modifying any source database:

```bash
.venv-check/bin/python scripts/security-intel/suggest_skill_tool_mappings.py \
  --output docs/knowledge-governance/skill-tool-mapping-candidates-2026-11-27.json
cargo run --offline -p intel-db --bin rp-skill-map -- \
  --input docs/knowledge-governance/skill-tool-mapping-candidates-2026-11-27.json
.venv-check/bin/python scripts/security-intel/suggest_skill_knowledge_mappings.py \
  --output docs/knowledge-governance/skill-knowledge-candidates-2026-11-27.json
cargo run --offline -p intel-db --bin rp-skill-ref-map -- \
  --input docs/knowledge-governance/skill-knowledge-candidates-2026-11-27.json
```

Both import commands are dry-run unless `--apply` is explicitly supplied. `--apply` must target a disposable or intentionally selected `RP_PG_DSN`; they only write unreviewed evidence-backed candidates and never approve them.

## 本轮 AI 检索端到端验收（2026-11-27）

- `rip.kb.unified_search` Manifest 升级到 `0.2.0`：输入字段统一为 `query`，可选 `include_candidates`；`tool-engine` 参数整形器兼容旧 `keyword` 输入，并把候选开关安全地传给 worker。输出分组新增“待审核武器候选”，详情保留 `governance_status` / `lifecycle_status`。
- Manifest 已通过 `rp-tool validate`，并注册/启用在开发运行库 `rip-data/dev/agent-runtime-search-2026-11-27.db`（T0、ACTIVE、ENABLED）。开发 Agent `agent.research.minimal` 的 capability refs 已切换为统一搜索入口；旧 `rip.kb.keyword_search` 工具仍保留注册，但不再作为该 Agent 的首个 Python worker。
- 备份：`rip-data/dev/backups/dev-p7-before-unified-search-2026-11-27.db`。注册后的运行库保存在 `rip-data/dev/agent-runtime-search-2026-11-27.db`；`migrations/dev-p7.db` 已恢复到原始 `user_version=12` 测试基线。没有修改 `migrations/platform.db` 或用户的持久 PostgreSQL 数据库。
- 新增 `rp-db-migrate --apply` 显式 PostgreSQL 迁移 CLI；搜索 CLI 本身不执行数据库迁移。一次性 PostgreSQL 容器中应用迁移后完成 Agent 闭环，容器已清理。
- Agent 默认通道：`CVE-2021-44228` → 5 条 ACTIVE 结果（CISA KEV、PoCIndex、APPROVED Nuclei 等）。Agent 候选通道：`sourcemap` + `include_candidates=true` → 5 条结果（3 Nuclei、2 RTIS weapon），全部为 CANDIDATE/DISCOVERED，并正确分组为“Nuclei 模板”和“待审核武器候选”。两次 Agent Run 与 Tool Run 均 `SUCCEEDED`。
- 这次 Agent 闭环使用确定性 `KeywordSearchPlanner`，不是实际 LLM Provider 调用；LLM Planner 的 `query` 参数契约和候选开关通过适配器测试，但真实模型在环验收仍待 Provider 配置。测试 PostgreSQL 是空的迁移后数据库，因此 `pg_intel` 通道 0 条；本轮 E2E 主要验证 SQLite 知识投影和工具调用链。
- 统一检索 LLM 闭环集成测试也修正为 Manifest 0.2.0 的 `query` 字段；此前测试 fixture 仍发送旧 `keyword`，被严格 JSON Schema 正确拒绝。修正后 LLM Planner → Broker → Tool Engine → Evidence 的 mock-provider 集成测试通过。
- 初始 Agent E2E 曾使用 50,000 token 预算隔离工具链；后续已修复 L2 相关性和 L3 task-scoped 选择，并在 12,000 token 预算下完成 CVE、sourcemap 候选、Apache Struts 三类 Agent E2E。模型专用 tokenizer 和真实 Provider usage 校准仍未完成。
- 初期 L3 自动扩展曾选中 EZVIZ OAuth、Airbnb iframe 等不相关案例；现已由 task-scoped Artifact 和 L2 相关性门修复，并通过 CVE / Apache Struts 12K 回归。
- 已修复上述 L3 污染：`ContextSource` 新增 task-scoped 候选入口；`ArtifactSource` 只返回当前 `task_id` 对应的 `tool_run` 产物，旧的无任务范围查询 fail-closed 返回空集。避免把其他任务的历史 Artifact 注入新 Agent 上下文。对应 `acceptance_artifact_ref`、`assemble_contract` 测试通过。
- 当前运行中的旧 API 进程未重启，默认派生索引仍是 schema 7（3,789 ACTIVE 文档）。新代码使用 schema 8；为避免旧进程故障，搜索引擎暂时保留 schema 7 的只读兼容（只有 ACTIVE 通道）。服务重启后，API 会按 schema 8 自动重建完整 18,147 文档索引，候选通道才会在默认派生索引中可用。测试索引 `rip-data/dev/asset-governance-search.db` 已是 schema 8。
- Podman PostgreSQL 中发现 `rip_m4_acceptance` 和 `rip_user_test_20261127` 两个候选数据库，二者当前图数据计数相同（31 identities / 3 relations / 1 evidence link / 32 source assessments）。本轮只读检查，没有猜测目标数据库，也没有把 173 条 Skill 关联候选写入任何一个持久库。候选 JSON 和可重放导入器已就绪，待明确目标库后再批量写入 CANDIDATE/UNREVIEWED。

## 验收边界

- 漏洞身份、POC/Nuclei 来源治理、Skill 关联候选、候选分层检索的生成和自动化管线已实现；**不代表 5,657 个漏洞、14,111 个 Nuclei 模板、610 条 RTIS 武器已完成人工审核或已批准**。
- Nuclei CLI `-validate` 在隔离 HOME 下 20 秒超时；引擎级模板验证没有结果。不得将 YAML metadata parse 视为 Nuclei Engine 验证。
- 没有执行任何 POC、EXP、Nuclei 模板，也没有对目标发送请求。没有自动批准、提交或推送。

## 13. 检索相关性与 12K 上下文回归（2026-11-27，更新前述待修项）

本节覆盖并更新 §5/§6 与上一节中“12,000 token 上下文超限、L3 相关性偏差仍待修复”的旧状态：这些问题已完成第一轮代码修复和真实 Agent 回归，但仍未完成真实模型 Token 校准与人工 qrel 评测。

- `context-broker` 增加模型上下文相关性门：先提取 query 中的具体实体/技术/CVE 词，过滤通用任务词；CVE 查询把规范化 CVE ID 作为身份约束；候选标题、摘要、item/ref ID 均参与匹配。L2 知识/插件候选按相关性排序并过滤零相关候选；工具目录和 task-scoped Artifact 不受该过滤影响。
- PluginSource 对当前活动版本的全部 APPROVED 记录进行相关性计分后再应用 limit，避免仅凭 `CVE` 等通用词命中时截断在前 64 条；仍严格过滤 DISCOVERED/REJECTED 和禁用插件。
- 无关候选 warning 从逐条输出改为总数 + 最多 5 个样例，避免 80+ 条噪声污染 Agent Run 报告。
- CVE 相关性规则纳入模板路径和权威引用 ID；例如 Nuclei 的 `.../CVE-2021-44228.yaml` 不再因 CVE 只存在于 ref_id 而被漏掉。CVE 查询不接受仅在描述中提到其他 CVE 的记录作为主身份匹配。

### 真实 Agent 回归

| 查询 | 12K 预算结果 | 检索证据 |
|---|---|---|
| `CVE-2021-44228 Log4Shell` | Agent Run / Tool Run `SUCCEEDED`；L2 摘要 369 tokens；L3 按需展开 CISA KEV 295 tokens | CISA KEV、PoCIndex、APPROVED Nuclei 模板均在上下文；包含精确 CVE 模板引用 |
| `sourcemap` + `include_candidates=true` | Agent Run / Tool Run `SUCCEEDED`；L2 知识摘要 0（候选不进入可信上下文） | 工具 Artifact 返回 5 条候选：3 Nuclei、2 RTIS；全部保留 `CANDIDATE / DISCOVERED` |
| `Apache Struts remote code execution` | Agent Run / Tool Run `SUCCEEDED`；L2 摘要 926 tokens；L2 共 16 条已批准插件资料 | 相关 Nuclei 模板与 CISA KEV 条目；L3 展开 Apache Struts KEV 条目 209 tokens |

以上测试均使用隔离的一次性 PostgreSQL 容器和 12,000 token 预算，没有提高预算、写入持久 PostgreSQL、执行 POC/Nuclei 模板或访问目标。候选通道的候选仍只作为 Artifact 供审核，不会被提升为可信 L2 知识。

### 验证与剩余限制

- `cargo test --offline -p context-broker` 全部通过；相关性、CVE 身份、中文确认词、Artifact、预算降级和稳定性测试均通过。
- `cargo clippy --offline -p context-broker --all-targets -- -D warnings` 通过（本节最后一次 stopword 扩展后需随下一轮全量门禁再次执行）。
- 当前测试仍使用 `KeywordSearchPlanner`，不是实际 LLM Provider 调用；真实 Provider 与其 usage/tokenizer 校准未完成。
- 当前未配置模型专用 `RIP_LLM_TOKENIZER_DIR`，上下文 Broker 使用明确标注的 Unicode 估算回退。12K 回归证明上下文装配不再溢出，不等同于 DeepSeek/Kimi 等模型的精确 Token 计数。
- 测试 PostgreSQL 是迁移后的空库，`pg_intel` 知识卡片通道仍为 0 条；本轮覆盖的是 SQLite 知识/插件投影与 Agent 工具闭环。
- 检索相关性规则是确定性 lexical guard，不替代 BM25/向量混合检索或人工 qrels。下一步必须用真实查询集和人工标注评估 Recall@K、MRR、NDCG、误过滤率和跨语言召回，不能只根据这三组案例宣称检索质量已全面达标。

## 14. 漏洞 / 武器 / Nuclei / Skill 批次审核队列（2026-11-27）

新增统一队列索引：`docs/knowledge-governance/governance-review-queue-index-2026-11-27.json`。审核方式固定为 batch；所有生成器只输出脱敏候选任务，状态均为 CANDIDATE/PENDING，不自动批准、不执行 POC/模板、不访问目标。

| 审核域 | 候选数 | 批次大小 | 批次数 | 已生成首批 |
|---|---:|---:|---:|---|
| CVE 漏洞身份 | 5,657 | 200 | 29 | `vulnerability-review-batch-001-2026-11-27.json` |
| RTIS 武器资产 | 610 | 100 | 7 | `weapon-review-batch-001-2026-11-27.json` |
| 真实 POC 子集 | 41 | 41 | 1 | `poc-review-batch-001-2026-11-27.json` |
| 疑似敏感材料 | 35 | 35 | 1 | `poc-sensitive-review-batch-001-2026-11-27.json` |
| Nuclei 模板 | 14,111 | 200 | 71 | `nuclei-review-batch-001-2026-11-27.json` |
| Skill→Tool | 146 | 146 | 1 | `skill-tool-mapping-candidates-2026-11-27.json` |
| Skill→Knowledge/Vulnerability/Nuclei | 173 | 173 | 1 | `skill-knowledge-candidates-2026-11-27.json` |

首批优先级分布：CVE P0=200（全量 P0=1,207、P1=598）；武器 P0=35（疑似秘密）、P1=39（真实 POC）、P2=9（EXP）、P3=69（脚本）；Nuclei P0=2（解析异常）、P1=525（KEV + Critical/High）、P2=27（其他 KEV）、P3=4,541（Critical/High）。完整优先级分布在队列索引。

可继续生成后续批次：

```bash
.venv-check/bin/python scripts/security-intel/build_vulnerability_review_batch.py --offset 200 --limit 200 --output docs/knowledge-governance/vulnerability-review-batch-002-2026-11-27.json
.venv-check/bin/python scripts/security-intel/build_weapon_review_batch.py --offset 100 --limit 100 --output docs/knowledge-governance/weapon-review-batch-002-2026-11-27.json
.venv-check/bin/python scripts/security-intel/build_nuclei_review_batch.py --offset 200 --limit 200 --output docs/knowledge-governance/nuclei-review-batch-002-2026-11-27.json
```

新增 vulnerability / weapon review batch builder 的分页、优先级、无正文导出和不自动批准测试已通过。Skill 关系目前有 319 条证据候选，但仍需人工审核；只有审核通过后才能投影为 Skill 的 `required_tools` / `knowledge_refs`。持久 PostgreSQL 导入尚未执行：当前 Podman 环境有两个合理候选库，必须先明确目标库，不能把 acceptance/test DB 当成 personal DB。

## 15. 精确 CVE 检索路由与评测口径修正（2026-11-27）

### 根因与修正

上一轮 4% Recall@10 的结果已判定为**评测口径错误**，不能作为产品精确 CVE 检索能力的结论：评测器用 `--keywords CVE-...` 走 FTS，却把 ACTIVE 与 DISCOVERED 全部模板都当作默认可见真值；默认检索本来只展示 `governance=ACTIVE`。这既没有测试项目已经存在的结构化 `cve_id` facet，也把默认不可见候选算成了漏召回。

代码核查确认 `search-engine` 已有 `SearchFilters.cve_id`，通过 SQLite JSON1 `json_each(nuclei_cve_ids_json)` 做精确匹配；不需要新建 CVE 映射表或改变 schema。真正缺口是统一检索层没有从用户 query 自动路由到这个结构化过滤器。

已修复 `unified-search::sqlite_query`：当 query 中恰好包含一个合法 CVE ID 时，SQLite 通道自动设置跨来源 `vulnerability_id` 结构化过滤，并清空 FTS 关键词，避免 CVE 只存在于分类元数据时被 FTS 预筛漏掉；不止一个 CVE 时不擅自选第一个，仍走普通 FTS。CISA KEV 插件投影现在从规范化 item_key 建立 `entity_type=vulnerability`、`canonical_key=cve:<CVE>` 与 `identity.vulnerability_ids`，并与 PoCIndex 共用同一身份过滤。PG 通道仍按原 query 做 hybrid 检索。

### 重新评测

`evaluate_exact_cve_search_v8.py` 已改为使用 `--cve-id`，并严格按可见性构建 gold set：

| 可见性 | CVE 查询数 | Recall@10 | Precision@10 | MRR | 全部相关结果进入 Top100 |
|---|---:|---:|---:|---:|---:|
| ACTIVE 默认通道 | 100 | 1.000 | 1.000 | 1.000 | 1.000 |
| ACTIVE + CANDIDATE 显式候选通道 | 100 | 1.000 | 1.000 | 1.000 | 1.000 |

报告：
- `exact-cve-search-self-consistency-v8-2026-11-27.json`
- `exact-cve-search-candidate-lane-v8-2026-11-27.json`

使用修复后的独立派生索引 `rip-data/dev/asset-governance-search-cve-fix.db`（18,147 docs，schema 8）验证 `CVE-2021-44228`：原始结构化索引返回 46 条跨来源结果（CISA KEV 1、PoCIndex 1、Nuclei 44）。真实 Python Worker Adapter + `rp-search-unified` + 一次性 PostgreSQL 容器下，Agent Run / Tool Run 均 `SUCCEEDED`，12K 预算成功；Artifact 返回前 10 条精确 CVE 结果，Context L2 同时保留 CISA KEV、PoCIndex 和 Nuclei 资料，L3 展开 CISA KEV 295 tokens。PG 通道 0 条是因为测试库为空，不是查询失败。

**边界**：这两份 100% 指标只证明结构化 CVE 身份过滤正确、候选可见性边界正确、结果确定；不代表自然语言 FTS 排序、跨语言语义检索或人工相关性已达 100%。后续仍需独立的人工 qrels 评测 BM25/向量召回与排序。
