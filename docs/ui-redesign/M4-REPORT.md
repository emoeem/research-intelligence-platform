# M4 Report — 基础业务页面

日期：2026-11-24
状态：**CLOSED — Projects 与 Research Assets 独立领域及 UI/API 已完成**

## 范围

Dashboard、Projects、Research、Knowledge、Tools、Plugins 六个基础业务工作区。遵循 M1 页面规格与真实 API 优先原则；Projects 与 Research Assets 按用户决策独立建模，所有展示数据来自持久化领域 API，不伪造业务数据。

## 页面审计与处理

| 页面 | 当前数据来源/实现 | 结论 |
|---|---|---|
| Dashboard | `/api/overview`、`/api/stats`、`/api/runtime`、`/api/runs`；KPI、待处理队列、Runtime 健康和近期活动 | 已实现真实 API 页面 |
| Projects | 独立 `/api/projects` REST API + Rust Store/CLI；workspace-scoped Project CRUD 与 Task/Asset 关系 | 已实现项目目录、创建/更新、详情、RTIS Task 关联和 Asset 关联；不再将任务镜像冒充 Project |
| Research | `/api/tasks`、`/api/runs`、`/api/overview`；计划、Evidence、Coverage 使用既有 research API | 已实现；任务/计划/证据/覆盖子页保留各自真实契约 |
| Research Assets | 独立 `/api/research/assets` REST API + Rust Store/CLI；Asset/Endpoint/Observation/Technology Profile | 已实现资产目录、手动导入、端点、append-only 观察记录和 evidence-referenced 技术画像 |
| Knowledge | `/api/knowledge`、`/api/knowledge_sources`；搜索、详情和来源视图 | 已实现真实 API 页面 |
| Tools | `/api/tools`、`/api/mcp-catalog`、工具详情 | 已实现真实 API 页面；无执行能力时不伪造试运行成功 |
| Plugins | `/api/plugins`、`/api/plugins/trust-policy`、详情与既有确认流程 | 已实现；沿用 TrustConfirmDialog 和后端策略 |

## 重要修正

Projects 路由此前将 `/api/tasks` 的 RTIS 任务镜像呈现为项目，违反 M1 对 Project 与 Workspace/Task 概念分离的要求。现已建立独立 Project 实体与 Project-Task 关联；Research Assets 也有独立 Asset/Endpoint/Observation/Technology Profile 实体。新增 Projects/Assets 回归测试，确认 Projects 使用 `/api/projects`，不再从 `/api/tasks` 伪造 Project。

## 当前实现边界

Projects 与 Research Assets 已作为独立领域实现。Research Asset 的自动发现 Provider/外部 Feed 尚未接入；当前提供手动导入、Endpoint 注册、append-only Observation 与 evidence-referenced Technology Profile API，后续 Provider 应复用该契约。

## 验收

M4 页面纳入全站 Vitest、TypeScript、Prettier、生产构建与 Playwright visual/axe 门禁。全量门禁：22 files / 155 tests、Playwright 85/85；Rust verification-engine 56 tests、Projects/Assets HTTP 集成测试通过。M4 CLOSED。


## Follow-up implementation — independent Projects and Research Assets (2026-11-24)

用户明确选择 Projects 与 Research Assets 均为 independent。原先“领域 API 缺口”不再作为最终交付边界，转为本轮待完成的后端实现工作。

已完成：

- 新增 `migrations/031_research_projects_assets.sql`：Project、Project-Task、Research Asset、Endpoint、append-only Observation、evidence-backed Technology Profile、Project-Asset、Task-Asset 八张表。
- Observation 增加 UPDATE/DELETE 拒绝触发器；Technology Profile 必须引用 Observation；Asset 身份按 workspace/type/canonical value 唯一。
- 新增 `decisions/M4-PROJECTS-ASSETS-DOMAIN.md`，冻结领域边界、SQLite 运营面存储、Python→Rust 子进程写入边界、API 方向与非目标。
- 使用 SQLite 内存库执行迁移 smoke test：8 表、user_version=31、JSON/外键约束与 Observation append-only 触发器通过。
- `git diff --check` 对新增迁移和决策文档通过。

该 follow-up 的 schema smoke test 是实现第一批；其后 Rust Store/CLI、rp_server API、Projects/Assets CRUD、关联流程和端到端测试已全部完成，最终结果见下方 Final implementation closeout。迁移只应用于临时/隔离 staging DB；主库 user_version 仍为 30。

## Figma 视觉参考

2026-11-24 收到四张桌面深色主题参考图：Tools & Agents、Report Studio、Research Investigation、Workflow Builder。其共同视觉规范已登记于 `decisions/M4-PROJECTS-ASSETS-DOMAIN.md`。参考图未覆盖 Projects/Assets、Light 主题或移动端；这些仍需对照既有页面规格与主题 Token 验收。

## Final implementation closeout — independent Projects and Research Assets (2026-11-24)

**M4 status: CLOSED for the agreed first-class domain and UI scope.**

### Persistence and Rust domain service

- `migrations/031_research_projects_assets.sql` is additive and workspace-scoped.
- Rust `verification-engine::project_assets` provides validated Project/Asset operations.
- Rust CLI `rp-verify` exposes project-list/create/get/update, task/asset relations, asset-list/create/get, endpoint creation, append-only observation creation, and evidence-referenced technology profile creation.
- Project and Asset reads enforce `workspace_id`. Wrong-workspace detail requests return 404; updates are scoped by workspace.
- Observation UPDATE and DELETE are rejected by SQLite triggers. Technology profile creation requires an Observation belonging to the same Asset, a non-empty evidence reference, and confidence in [0,1].
- Endpoint observations validate that the Endpoint belongs to the same Asset.
- Manual Asset import creates an immutable `user_import` Observation with a SHA-256 payload hash. It does not infer technologies from a domain or URL string.

### HTTP and UI

- Read API: `GET /api/projects`, `GET /api/projects/{id}`, `GET /api/research/assets`, `GET /api/research/assets/{id}`.
- Write API: Project create/update; Project-Task and Project-Asset links; Asset manual import; Endpoint, Observation and Technology profile creation.
- All writes go through the Rust CLI subprocess. `rp_server.py` adds no SQL write path for these domains.
- Project page: searchable/filterable project registry, create form, project detail, task relation and asset relation.
- Research Assets page: searchable/type-filterable asset registry, manual import, asset detail, Endpoint list/add, Observation append and evidence-referenced Technology profile.
- Added zh-CN/en-US/ja-JP translations for both pages.
- Visual page matrix now includes Projects and Research Assets in Dark/Light × desktop/mobile coverage.

### Verification

- `bash apps/server/selftest_projects_assets.sh` — PASS. Exercises authenticated HTTP CRUD, workspace isolation, Project↔Task/Asset relations, Asset/Endpoint/Observation/Technology writes, Origin and Bearer guards. It writes only to a temporary DB copy.
- `cargo test --offline -p verification-engine` — 56 passed (45 unit + 2 project/assets integration + 9 additional integration tests).
- Web unit tests — 22 files / 155 tests passed.
- Web typecheck, Prettier check and production build — PASS.
- Full Playwright suite — 85/85 passed, including the 360-combination all-route locale/theme/five-viewport responsive smoke.
- `selftest_phase9_rest.sh` — 33 PASS / 0 FAIL, including API contract parity and production DB zero-write fingerprint.
- `selftest_spa_entry.py` — 5/5 passed.
- Primary `migrations/platform.db` remains at user_version 30. The new 031 migration is applied only to temporary/staging DB copies during verification.

### Explicit boundary

This closes the independent Projects and Research Assets implementation agreed for M4. It does not claim that discovery providers or external feeds are already populating Assets automatically; manual import, explicit endpoint registration, append-only observation and evidence-referenced technology profile are the implemented first operational path. Provider adapters can add observations through the same contract in a later integration.
