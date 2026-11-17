# P5-9a 批次报告：Workflow 模板与画布（十三步两模板 + Schema 表单对齐 + 非执行面实例化）

> 批次：P5-9 子批一（Workflow 模板与画布）。规格源 = Phase 5 计划 §6.1 / §6.2 / §6.3 / §7 P5-9 行 + HANDOVER-P5-WIP.md 第四节；实施清单 = rip3-p5-finish P5_9A_BRIEF。接手状态 = 前会话半成品（两份模板 JSON 未跟踪、无报告、无 commit），本批以 git 与装载门实测为准接手收口。

## A. 本批完成内容

1. 通用渗透测试十三步模板 `wf-pentest-research-v1`（§6.1 逐条映射，18 节点 / 19 边）落 `workflows/` 目录 JSON 定义惯例（parse-pipeline-v1 / review-card-v1 同款 shape），`rp-workflow validate` 装载门 + 静态校验 PASSED。
2. SRC 漏洞挖掘十三步模板 `wf-src-bughunt-v1`（§6.2 逐条映射，17 节点 / 17 边）同上，PASSED。接手时半成品含解析错误（见 B 节坏消息与本批教训），修复后通过。
3. 节点 Schema 表单：NodeInspector（P8-4 既有）八类词表覆盖核对通过；本批唯一 web 增量 = condition 节点 `expression` 字段由 string 编辑器换 JSON 编辑器（模板的 expression 是 JSON Schema 条件载荷 object，string 编辑器渲染显示 `[object Object]` 且编辑会把 object 压成 string），与模板参数形状对齐，沿用 JsonField 既有"解析失败内联报错不静默丢改"语义。
4. 画布保存与版本：复用既有面零新增（FlowCanvas / VersionList / workflow-engine 017 store 版本面），模板 put 入库即经 WorkflowsPage `/api/workflows` 列表选用。
5. 模板实例化（非执行面）：新增集成测试 3 条（装载门 + 十三步钉值 + put/get 往返与版本面）+ CLI 真库实拍（put ×2 + list + get）。
6. ADR 两条：零新增节点类型（见 B 节）；循环上限不在图上做回边（见 B 节）。

## B. 架构变化和复用说明

### 坏消息单列：接手时半成品未过装载门

前会话半成品 `workflows/src-bughunt-v1.json` 在 line 174 存在 trailing comma（`impact_assessment` 节点 params 对象尾逗号），`rp-workflow validate` 报 `parse error: trailing comma at line 174 column 9`，即接手时该文件从未通过装载门。本批修复该字符后 PASSED。pentest 模板接手时即 PASSED，未改动。

### ADR 1：零新增节点类型

§6.3 候选节点逐条核对既有八类词表（start/end/tool/knowledge/agent/human/condition/parallel，dsl.rs:72 NodeTypeV1，016 CHECK 域同源）：全部十三步可用八类映射，无新增节点，无需扩词表。映射对账：

| §6.1 步 | 通用渗透节点 | 类型 | | §6.2 步 | SRC 节点 | 类型 |
| --- | --- | --- | --- | --- | --- | --- |
| 1 目标范围确认 | scope_confirm | human | | 1 收集公开资料 | public_recon | tool |
| 2 资产清点 | asset_inventory | tool（parallel 分支） | | 2 资产业务功能 | asset_function_map | tool |
| 3 技术栈识别 | tech_fingerprint | tool（parallel 分支） | | 3 技术栈 | stack_fingerprint | tool |
| 4 知识检索 | kb_context | knowledge | | 4 相似案例检索 | case_search | tool |
| 5 威胁建模 | threat_model | agent | | 5 覆盖矩阵 | coverage_matrix | agent |
| 6 特征检测 | feature_detection | tool | | 6 低影响检测 | low_impact_scan | tool |
| 7 人工或 Agent 分析 | deep_analysis + escalation_gate + analyst_review | agent/condition/human | | 7 业务逻辑权限边界 | authz_boundary_analysis | agent |
| 8 受控验证 | controlled_verification | tool | | 8 安全假设 | hypothesis_draft | agent |
| 9 Evidence 收集 | evidence_collect | tool | | 9 受控验证 | controlled_verify | tool |
| 10 Finding 关联 | finding_link | human | | 10 记录证据 | evidence_record | tool |
| 11 风险评价 | risk_rating | agent | | 11 评估影响 | impact_assessment | agent |
| 12 修复建议 | remediation | agent | | 12 查重 | dedup_search + dedup_gate + dedup_review | tool/condition/human |
| 13 报告生成 | report_generate | tool | | 13 SRC 报告 | src_report_lint | tool |

start/done 为图骨架不计步。检测面与受控验证面分离（两处 nuclei 节点，模板锚由 run 输入分别承载）；授权审批不内置（human 停等 = 018 awaiting_approval 既有面，放行参数人工注入）；Finding 与报告事实只经 RTIS 既有接口（finding_link 为 human 停等节点，CONFIRMED 由人工经 rp-verify finding-link 或 rt.py poc update 回填，模板零 RTIS 直写节点，测试断言背书）。

### ADR 2：循环上限不在图上做回边

definition-v1 静态校验禁环（validate 规则 cycle），迭代上限统一以节点参数承载：nuclei 节点 rate_limit（5）/max_requests（30 或 20）/timeout_seconds（60）+ verification-engine PlanBudget.max_rounds（P5-8 面）。覆盖矩阵与假设草稿持久化走 rp-verify item-add / hypothesis-add，零自建矩阵运行时。

### 复用矩阵（本批零重建）

| 能力 | 复用物 | 增量 |
| --- | --- | --- |
| 装载门 + 静态校验 | workflow-engine dsl::parse_definition + validate_definition（P8-1/P8-2） | 零 |
| 模板存储与版本 | 017 workflow_definition 三表 + WorkflowStore.put/get/list 每版追加 | 零 |
| 节点 Schema 表单 | NodeInspector 八类词表表单（P8-4） | condition expression 换 JsonField |
| 画布 | FlowCanvas（XYFlow）+ NodePalette + workflow-graph 纯函数（P8-4） | 零 |
| 画布保存与版本 | VersionList + buildDefinition put 面（P8-4） | 零 |
| 模板实例化入口 | rp-workflow put/get/list CLI + WorkflowsPage /api/workflows 列表 | 零 |

## C. 修改文件清单

| 文件 | 变化 |
| --- | --- |
| workflows/pentest-research-v1.json | 新文件入库（半成品接手，validate PASSED 未改动） |
| workflows/src-bughunt-v1.json | 新文件入库（半成品接手，修复 line 174 trailing comma） |
| crates/workflow-engine/tests/p5_9a_templates.rs | 新文件：模板钉值集成测试 3 条 |
| web/src/components/workflow/NodeInspector.tsx | condition expression 编辑器 string → JSON（JsonField） |
| docs/rip3/batches/P5-9a-BATCH-REPORT.md | 新文件（本报告） |

工作树用户 WSW 素材 4 项（M docs/README.md、M RIP3-PHASE5-...PLAN.md、?? docs/rip3/WEB-SECURITY-WORKBENCH-PLAN.md、?? docs/ui-ux/）不提交不还原，本批 commit 显式 add 本批文件清单。

## D. 数据库迁移、Schema 和兼容性变化

零迁移。模板登记走 017 workflow_definition 既有表（WorkflowStore.put 装载门 + 静态校验门 + 每版追加），SQLite 链头保持 022，PG 链头保持 V0006，零链推进。模板 JSON 为 workflows/ 目录静态定义文件，不入库 schema 面。compat 恒 graph-v1（016 CHECK 域既有词值）。

## E. 测试命令及真实结果

### E1 装载门与静态校验（2026-11-18 实测）

```
$ cargo run --offline -p workflow-engine --bin rp-workflow -- validate workflows/pentest-research-v1.json
{"compat":"graph-v1","edges":19,"nodes":18,"result":"PASSED","version":1,"workflow_id":"wf-pentest-research-v1"}

$ cargo run --offline -p workflow-engine --bin rp-workflow -- validate workflows/src-bughunt-v1.json
workflow definition parse error: trailing comma at line 174 column 9   ← 接手时实测，修复后：
{"compat":"graph-v1","edges":17,"nodes":17,"result":"PASSED","version":1,"workflow_id":"wf-src-bughunt-v1"}
```

### E2 CLI 真库实例化实拍（非执行面）

```
$ rp-workflow put workflows/pentest-research-v1.json --db /tmp/p5-9a-e2e/templates.db
{"compat":"graph-v1","edges":19,"nodes":18,"result":"PUT","version":1,"workflow_id":"wf-pentest-research-v1"}
$ rp-workflow put workflows/src-bughunt-v1.json --db /tmp/p5-9a-e2e/templates.db
{"compat":"graph-v1","edges":17,"nodes":17,"result":"PUT","version":1,"workflow_id":"wf-src-bughunt-v1"}
$ rp-workflow list --db /tmp/p5-9a-e2e/templates.db
{"...workflow_id":"wf-pentest-research-v1","latest_version":1,"nodes":18,"edges":19,"version_count":1,...}
{"...workflow_id":"wf-src-bughunt-v1","latest_version":1,"nodes":17,"edges":17,"version_count":1,...}
{"db":"/tmp/p5-9a-e2e/templates.db","total_workflows":2}
$ rp-workflow get wf-src-bughunt-v1 --db /tmp/p5-9a-e2e/templates.db
{"workflow_id":"wf-src-bughunt-v1","version":1,"name":"RIP3 SRC 漏洞挖掘模板（十三步）","compat":"graph-v1"}
```

### E3 定向测试

```
$ cargo test --offline -p workflow-engine
lib 49 passed / migration_replay 3 / p5_9a_templates 3（新）/ p8_3_control 4 / run_execution 9
共 68 passed 0 failed；clippy -p workflow-engine --all-targets -D warnings 零告警

新测试明细（tests/p5_9a_templates.rs）：
- pentest_template_thirteen_steps_valid：装载门 + 静态校验零 issue + 十三步节点 id 钉值 + tool_id 全集钉值 + human 节点面钉值
- src_template_thirteen_steps_valid：同上（SRC 侧，含 dedup 三节点承载步 12）
- templates_instantiate_into_store_non_exec_face：临时库 put ×2（version 1，18/19 与 17/17 钉值）+ get 往返逐字段一致 + 版本追加 v2 + list 两行

$ cd web && npm run build && npm run test
build 775ms 通过；vitest 19 files / 124 passed（NodeInspector 改动后全绿）
```

### E4 CI

本批 commit push 后 `gh run watch` 收口，红则修复批再进下一批（P5-7 dbb582d1 先例）；终态记入 HANDOVER 与 P5-9b 开工核对。

## F. 性能数据

本批零性能面改动（模板为静态 JSON，装载与校验 ms 级；store put/get 与 P8 基线同路径）。web build 775ms（E3 实测）。不新增门禁指标。

## G. 未解决问题与风险

1. condition `expression` DSL 值形状不限（validate 只查键存在非 null），历史定义 string 形状与模板 object 形状并存；JsonField 两种形状均可承载（string 以 JSON 字符串字面量显示），无兼容风险，如实登记。
2. WorkflowsPage 左栏暂为节点分类面板，无"模板清单一键加载"区块；模板经 /api/workflows 列表选择加载。模板选择体验增强留 P5-9b 页面族（若 §4.9 左栏模板区需要落 UI，随 9b 评估，不阻塞完成标准）。
3. egress_allowlist 派发面真实目标实拍（P5-5 G1）与 finding-link 正例 RTIS 写面实拍（P5-7 G1）随 P5-9c 本地靶场，本批如实未做。
4. ZAP Adapter 沿用 Phase 5 后扩展项（completion 由 Nuclei 满足）。
5. agent 节点 agent_id（ag-coverage-planner / ag-authz-analyst / ag-hypothesis-planner / ag-impact-assessor / pentest 侧 threat_model/deep_analysis/risk_rating/remediation 四个）运行前须登记 agent_definition，装载与静态校验不查存在性（DSL 既有口径）；P5-9c 六场景演练时按需登记。

## H. 下一批依赖和明确的非目标

P5-9b（Evidence 与 Coverage 页面）依赖：本批两模板入库（run 轨迹/日志/Evidence 页展示对象）+ P5-7 G3 遗留（Evidence rp_server 端点与 UI 页）+ P5-8 coverage-show 数据面。非目标：不接 ZAP、不动 runtime-gateway 内核、不做 Report Intelligence 与知识回流、不做 Proxy/MITM（WSW 扩展规划另行开工，不得提前打断 Phase 5）。

## I. UI 截图与浏览器验收证据

本批零新增页面与路由（web 唯一改动为 Inspector 单字段编辑器形态，无布局变化）；浏览器端到端验收（模板加载 + 模板运行 + 截图入库 docs/rip3/batches/browser-e2e/）随 P5-9b 页面批与 P5-9c 端到端批执行，本批如实留空。

## J. RTIS 对账和数据安全回归结果

本批零 RTIS 接触（无 rt.py 调用、无 rtis MCP 调用）；commit 前 `git diff integrations/rtis/` 为空已核对。POC 正文零复制（模板仅含模板锚引用形状，rtis-poc:id / knowledge:id 由 run 输入注入）。两模板静态断言"无 rtis 直写工具节点"由 p5_9a_templates.rs 固化。生产 migrations/platform*.db 零触碰。
