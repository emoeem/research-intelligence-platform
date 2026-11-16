# P5-0 批次报告：只读审计与设计冻结（Phase 5 首批）

> 批次类型：只读审计（不写业务代码）。本报告是 Phase 5 全部实现批次的底账，
> 页面复用矩阵与后端能力复用矩阵的每一条结论都带代码路径证据。
> 规格源：docs/rip3/RIP3-PHASE5-WEB-WORKBENCH-AND-KNOWLEDGE-DRIVEN-VERIFICATION-PLAN.md
> （§7 P5-0 十项工作项）+ docs/HANDOVER-RIP3-CONTINUATION.md 第七节。

## A. 本批完成内容

审计基线：git 7639a12e（main，工作树干净），2026-11-17。审计范围四块：
Web 壳层与页面族、后端领域模型与 API、RTIS 接口面、Provider/容器面。
核心结论一句话：**执行内核（tool-engine/workflow-engine/runtime-gateway/
context-broker/intel-db 统一检索）与 Web 五区壳层全部可复用；缺口集中在
「研究编排域（VerificationPlan/Item/Event/五态/覆盖矩阵/假设）不存在、
Research 页面是占位、RTIS POC 无平台侧 Adapter、无任何扫描 Provider
Manifest/镜像、无 Workflow 模板机制」五处。**

## B. 架构与复用说明（页面复用矩阵 + 后端能力复用矩阵）

### B1 页面复用矩阵（Web）

| 页面/组件 | 现状 | 证据路径 | Phase 5 处置 |
| --- | --- | --- | --- |
| 五区壳层 Layout | 完整可复用：顶栏（工作区切换/全局搜索/主题切换/Inspector-Activity 显隐）+ 56px 图标轨 + 中央 TabArea + 右 Inspector + 底 Activity，含沉浸态正则钩子（预留，当前不命中） | web/src/components/Layout.tsx（554 行） | 原样复用，Phase 5 不动壳层 |
| 工作区信息架构 | 11 工作区单数据源（编号稳定、buildFeatureMap 推导功能地图）；research 工作区单 tab 且 hint 标注「占位」 | web/src/workspaces.ts | 扩 research 工作区 tabs（数据源一处改） |
| ResearchPage | **通用占位**（WorkspacePlaceholder，18 行） | web/src/routes/ResearchPage.tsx | P5-1 重写为工作区（本批核心缺口） |
| TabArea | 数据无关通用组件（固定 tab + 会话 tab + 编号） | web/src/components/TabArea.tsx（122 行） | 原样复用 |
| InspectorPanel | 数据无关（sections 注入式） | web/src/components/InspectorPanel.tsx（40 行） | 原样复用，Research 页注入对象 sections |
| ActivityPanel | 数据无关（tabs 注入式） | web/src/components/ActivityPanel.tsx（52 行） | 原样复用，注入 Task Queue/Tool Runs/Logs/Evidence |
| UI 基元 | Modal（焦点圈/Escape/焦点还原）、PageHeader、SkeletonPanel、EmptyPanel、ErrorPanel（401/403/404 分态）、StatusBadge（label/tone 注入） | web/src/components/ui.tsx（209 行） | 原样复用；表格/FilterBar 需补（见差距清单） |
| 主题 | Dark/Light 双主题 token 体系（theme.ts + styles/token.css 132 行 + shell.css 811 行） | web/src/theme.ts | 原样沿用，禁散落 Hex |
| API 封装 | ApiError（status/code/message）、apiGet/apiSend、token provider、AUTH_EXPIRED_EVENT 全局广播 | web/src/lib/api.ts（989 行） | 原样复用，追加 research 面 fetch 函数 |
| 全局搜索 | GlobalSearchPalette（Ctrl/Ctrl+K）+ global-search.ts 聚合 | web/src/components/GlobalSearchPalette.tsx（261 行）+ lib/global-search.ts | 原样复用 |
| OverviewPage | KPI + 功能地图，计数来自 /api/overview 真实字段 | web/src/routes/OverviewPage.tsx（216 行） | Dashboard 结构调整的参照底 |
| ProjectsPage | 项目/任务树表 | web/src/routes/ProjectsPage.tsx（125 行） | Research Tasks 表格参照（表格列/筛选交互） |
| KnowledgePage | 知识条目列表 + 详情（520 行，最大可复用列表页） | web/src/routes/KnowledgePage.tsx | Knowledge Context 子页参照 |
| ToolCenter/ToolDetail | 工具目录 + Manifest 详情 + 试运行 | web/src/routes/ToolCenterPage.tsx / ToolDetailPage.tsx | Scan Center 的 Provider 面参照 |
| WorkflowsPage | XYFlow 画布（workflow-graph.ts 图模型）+ 版本 + 运行入口（575 行） | web/src/routes/WorkflowsPage.tsx + lib/workflow-graph.ts | P5-9 模板/画布集成基座，零重建 |
| RunsPage/RunDetailPage/TracePage | Run 列表/详情（Artifact 预览）/Trace 六 ID 面 | web/src/routes/RunsPage.tsx / RunDetailPage.tsx / TracePage.tsx | Evidence/Trace 子页直接复用其行渲染逻辑 |
| DocumentsPage | RIP3 文档中心（rp-doc 通道，303 行） | web/src/routes/DocumentsPage.tsx | 子页加载态/错误态参照 |
| 浏览器实测脚本 | chromium 实拍模板（token 走 localStorage rip_auth_token） | web/scripts/rip3_p1_browser_e2e.mjs + p5_current_ui_shots.mjs + visual-server.mjs | 每个实现批次截图入库用 |

### B2 后端能力复用矩阵（Rust/Python/RTIS）

| 能力 | 现状 | 证据路径 | Phase 5 处置 |
| --- | --- | --- | --- |
| Application API | rp_server.py 单文件 3274 行：19 条 GET + 3 条 POST（documents/import、workflows、workflow-runs）；Bearer 认证（constant-time）、X-Trace-Id/X-Correlation-Id 响应头、JSONL 访问审计、限频 | apps/server/rp_server.py（API_ROUTES 表 2953 行起） | 新端点按此表追加；进程边界即数据边界 |
| Python→Rust 通道 | subprocess + JSON stdout（rp-doc/rp-plugin 二进制，错误帧走 stdout 非零退出映射 502） | apps/server/rp_server.py `_rp_doc_json`（2851 行） | 新 research/verification CLI 沿用同款 |
| ResearchTask 域 | **仅 ETL 镜像表**（rtis-tasks 源直通：task_id/title/objective/target/task_type/state/status_history/source_file/schema_version），只读 | migrations/002_tasks.sql 32-44 行；apps/server/rp_server.py handle_tasks（574 行） | 编排容器（阶段/预算/计划/覆盖/假设）需新建，不复用此表（差距 G2） |
| Finding/Evidence 域 | Finding/Evidence 表是 RTIS ETL 镜像；evidence.payload_json 追加列 | migrations/002_tasks.sql + etl_evidence.py | 不建重复实体；RIP3 只持引用 + Artifact（差距 G3） |
| Trace 六 ID | 015 迁移 trace_correlation + tool_run.trace_id 列 + /api/traces | migrations/015_trace_correlation.sql；web/src/routes/TracePage.tsx | 原样复用（验证链六 ID 锚定） |
| Tool Engine | 状态机 QUEUED→WAITING_FOR_RUNNER→PREPARING→RUNNING→{SUCCEEDED/FAILED/TIMED_OUT/CANCELLED}；request_cancel 仅 Running/WaitingForRunner 受理（state.rs cancel_requestable）；submit/submit_correlated/drive/drive_resumed | crates/tool-engine/src/lib.rs（141-707 行）+ state.rs | Provider 执行一律经此（P5-5/5-6） |
| Runtime Gateway | Podman 隔离执行面 + 心跳/健康（health_store.rs + heartbeat.rs） | crates/runtime-gateway/src/ | 原样复用，容器 stdout 并发读坑位已修（P4-4） |
| Workflow Engine | DSL v1（8 节点型 start/end/tool/knowledge/agent/human/condition/parallel）+ checkpoint + 暂停/恢复/取消控制面（request_pause/consume_pause_request/record_cancel/propagate_pause_to_agents/propagate_cancel_to_entities）+ 016-018 迁移 | crates/workflow-engine/src/{dsl.rs,control.rs,state.rs,store.rs}；migrations/016-018 | P5-6 派发 + P5-9 模板基座；节点型扩展需 DSL 评估（差距 G6） |
| Workflow 定义实例 | workflows/ 仅 2 份 JSON（parse-pipeline-v1、review-card-v1），**无模板机制**（无模板注册/实例化/参数注入） | workflows/ 目录 | P5-9 新建模板面（差距 G5） |
| 统一检索（Phase 4） | SearchFilters/bm25_search/vector_search/rrf_fuse/mmr_diversify/hybrid_search/rerank_search/rerank_hits + RerankWorkerProvider（容器排序）+ 双缓存（query-vector + retrieval，治理态指纹失效，SEARCH_CACHE_VERSION v3） | crates/intel-db/src/search.rs + search_cache.rs | Knowledge Matcher 唯一检索入口（禁绕过） |
| 跨库归并 | unified-search 薄 crate：SQLite 知识面 + PG 情报面 RRF k=60 归并，sqlite_error 降级通道 | crates/unified-search/src/ | Matcher 候选来源之一 |
| Context Broker | assemble/estimate/intent/policy/score/source/tokenizer（tokenizers 接入）+ cache.rs 内容寻址 | crates/context-broker/src/ | Matcher/Planner 上下文与预算承载 |
| 研究能力契约 | research-engine 七能力（planner/discovery/extraction/verification/synthesis/citation/evaluator）14 份 JSON Schema + execute_request 信封 + 六 ID Correlation | crates/research-engine/src/contract.rs + schemas/research-capability/（14 份） | P5-2/P5-4 契约基准：Verification Plan/Item 字段沿 planner/verification 契约语义扩展，不另起协议 |
| 领域 ID/时间 | domain::ids（六 ID 类型校验）、time.rs | crates/domain/src/ids.rs | 新实体 ID 沿同表 |
| RTIS POC 接口 | rt.py poc search（FTS5+过滤 --json 元数据面）/get/add/use/update + skill/kb + mcp 子命令（NDJSON stdio）；rtis_mcp.mjs tools/list | integrations/rtis/rt.py（1073-1150 行）；integrations/rtis/rtis_mcp.mjs | RTIS POC Adapter 唯一通道（实测 `python3 integrations/rtis/rt.py poc search --q kingdee --json` 返回 3 条元数据，含 verified/hit_count/组件/版本字段）；integrations/rtis 零修改 |
| 情报核迁移链 | migrations_pg refinery V0001-V0006（V0006=search_hybrid 链头） | migrations_pg/ | 研究域新 PG 表从 V0007 起（若判定为情报核） |
| 运营核迁移链 | SQLite 001-019（018=workflow control correlation 链头） | migrations/ | 研究编排执行态若判运营核则 020+ 小迁移（见 B3 判定） |
| 容器族 | rip-pg17/rip-parser/rip-gov/rip-embed/rip-rerank + tool-base/coreutils/source/webresearch 六工具镜像在位；**无 nuclei/zap 镜像与 Manifest** | containerfiles/；podman images 实测 | Nuclei Adapter 需新镜像+Manifest（差距 G4） |
| 工具 Manifest | manifests/tools/ 37 份全 rip.* 前缀（doc/evidence/http/kb/knowledge/ops/security/text），Schema 驱动 | manifests/tools/rip.http.tls_inspect.yaml 等 | Nuclei/ZAP 走同款 Manifest 面 |
| 浏览器/CI 面 | 既有 e2e 脚本 + 视觉门 | web/scripts/ | 实现批次沿用 |

### B3 研究域存储判定（P5-2 前置设计冻结）

研究编排实体（ResearchTask 编排容器、VerificationPlan/Item/Event、覆盖矩阵、
假设）按 R2 §3.4 是 RIP3 编排面，与 workflow_run/tool_run/agent_run 同族
（执行状态机数据），非情报核知识数据：**落 SQLite 020+ 运营面迁移**（符合
铁律「SQLite 020+ 仅运营面小迁移」；情报核五面不新增表，Finding 事实留在
RTIS）。PG 侧本阶段零新表（无需动 migrations_pg 链，selftest_phase1_pg 钉值
不重置）。

## C. 修改文件清单

本批零业务代码，仅文档：docs/rip3/batches/P5-0-BATCH-REPORT.md（本文件）。

## D. 数据库迁移与 Schema 变化

无（只读审计批）。

## E. 测试命令及真实结果

只读核验命令（全部实际执行）：

```text
node integrations/rtis/rt.py poc search --q kingdee --json   # 误用 node 报 SyntaxError（rt.py 是 python 脚本，坑位登记）
python3 integrations/rtis/rt.py poc search --q kingdee --json  # count=3，元数据面字段齐（id/code/title/kind/severity/verified/hit_count/component/versions/tags）
podman images | grep rip    # rip-pg17/rip-parser/rip-gov/rip-embed/rip-rerank 在位，无 nuclei/zap
ls manifests/tools/ | wc -l # 37（全 rip.* 前缀，无扫描器 Manifest）
ls workflows/               # parse-pipeline-v1.json + review-card-v1.json（无模板机制）
```

## E2. CI 各 Job 状态和运行链接

本批为文档批，随批入库后以 HEAD run 为准（见提交记录）。

## F. 性能数据

不适用（只读批）。P5-3 起建立 Matcher/Planner 基线指标。

## G. 差距清单（实现批次唯一判据源）

| 编号 | 差距 | 实证 | 归属批次 |
| --- | --- | --- | --- |
| G1 | Research 页面是 WorkspacePlaceholder，research 工作区仅 1 个占位 tab | web/src/routes/ResearchPage.tsx（18 行） | P5-1 |
| G2 | 研究编排域不存在：research_task 仅 RTIS ETL 只读镜像，无编排容器（阶段/预算/计划/覆盖/假设/五态） | migrations/002_tasks.sql 32-44 行 | P5-2（SQLite 020 运营面） |
| G3 | 无 Verification Plan/Item/Event Schema 与五态结果域；research-engine 七能力契约是纯函数契约面（无编排态） | crates/research-engine/src/contract.rs（信封无持久化） | P5-2/P5-4 |
| G4 | 无 RTIS POC 平台侧 Adapter（无 subprocess 封装/无指纹/无 use 回填）；无 Nuclei/ZAP Manifest 与容器镜像 | manifests/tools/ 无扫描器；podman images 实测 | P5-5 |
| G5 | 无 Workflow 模板机制（无模板注册/实例化/参数注入；workflows/ 仅 2 份具体定义） | workflows/ 目录 | P5-9 |
| G6 | Workflow DSL 8 节点型无 evidence-collection/finding-reference/coverage-update/research 节点型 | crates/domain/src/workflow.rs 117-165 行 | P5-9（DSL 评估后决定扩展或以 tool/knowledge 节点参数化承载） |
| G7 | 无覆盖矩阵七维与自适应研究（假设/预算轮次）域 | 全仓 grep 无 coverage_matrix/hypothesis 编排面 | P5-8 |
| G8 | rp_server 无 research/verification/coverage/plan 端点 | apps/server/rp_server.py API_ROUTES 表 | P5-2 起逐批追加 |
| G9 | Web 缺可复用 Table/FilterBar/DetailPanel 基元（现各页手写表格，风格不一） | 各 route 页对比 | P5-1 |
| G10 | 前端对运行任务无跨页「正在运行」聚合（Activity 面板内容空壳待注入） | web/src/components/Layout.tsx 315 行 activityTabs 现状 | P5-6 |

## H. 下一批依赖与非目标

- P5-1 依赖：B1 矩阵全部组件 + workspaces.ts 单数据源 + /api/tasks 与
  /api/runs 既有端点（真实数据驱动，无假数据）。
- 非目标（本阶段冻结）：不建 RTIS 对等库、不实现 MITM/代理/Yaklang、
  不做 Phase 6 报告渲染器/Phase 7 回流/Phase 8 压测、不加授权审批子系统。
- RTIS 红线复核：integrations/rtis/ 零修改（本批未触碰）；POC 正文不进
  3.0 库与公开仓库；对账零回归基准继续沿用。

## I. UI 截图与浏览器验收证据

不适用（本批无 UI 改动）。参考图现状：docs/rip3/ui-reference/current/ 六张
（A 组现状）+ yakit/ 三张（B 组交互参考），p5_current_ui_shots.mjs 可重拍。

## J. RTIS 对账和数据安全回归结果

本批零 RTIS 写操作（仅一次 poc search 只读元数据查询，走 rt.py CLI 正道）。
生产 migrations/platform*.db 零触碰。15155 对账口径不受影响。
