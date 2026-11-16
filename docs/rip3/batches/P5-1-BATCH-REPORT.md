# P5-1 批次报告：Web 设计系统与 Research Workspace 骨架

> 批次类型：实现批（UI + 一处服务端只读端点增量）。规格源：Phase 5 补充计划
> §7 P5-1 + §4.4/§4.5；审计依据：P5-0-BATCH-REPORT.md 差距清单 G1/G9。

## A. 本批完成内容

1. **Research Workspace 骨架**（G1 收口，ResearchPage 不再是 WorkspacePlaceholder）：
   - `/research` = 研究总览（KPI：研究任务数/活跃运行/知识条目/验证结果待交付位，
     全部来自 `/api/tasks` + `/api/runs` + `/api/overview` 真实字段；状态计数
     chips；最近任务列表可点入详情）；
   - `/research/tasks` = 任务列表（真实 42 行 dev 库、状态/类型/文本三过滤、
     DataTable 行点击进详情）；
   - `/research/tasks/:taskId` = 任务详情（单条全字段 + status_history 保真
     时间线 + 关联运行按 task_id 匹配，无关联时诚实空态说明随 P5-6 出现）；
   - `/research/assets` = 资产页骨架（诚实空态：资产域未落库，随 P5-2/P5-5
     交付；不凭域名推测技术栈）；
   - `/research/knowledge` = 知识上下文首版（服务端 `/api/knowledge?q=` 五字段
     LIKE 检索；能力边界说明框：Matcher 命中/排除解释面随 P5-3 升级）。
2. **共用数据基元**（G9 首步）：`web/src/components/data.tsx` = DataTable（列
   配置/空态/caption）/FilterSelect/FilterSearch/CountChips，零业务依赖、
   token 合规，后续页面族统一消费。
3. **workspaces.ts research 工作区扩为四 tab**（3.1 研究总览/3.2 任务/3.3
   资产/3.4 知识上下文），单数据源改动，功能地图/tab 条/图标轨同步生效；
   Scan Center/Coverage/Findings/Evidence/Trace 子页随对应批次落页后再扩
   （避免死 tab）。
4. **服务端一处增量**（真实数据驱动所需）：`handle_tasks` 支持 `?task_id=`
   单条全字段（含 status_history；404 口径对齐 knowledge 单条惯例）+ 列表
   SELECT 补 status_history 列。镜像表只读语义不变，零写面。
5. **视觉矩阵与基线**：/research 视觉门 marker 更新为「研究总览」；四张
   research 基线（dark/light × desktop/mobile）重生成至新页面形态。
6. **浏览器实测脚本**：`web/scripts/rip3_p5_browser_e2e.mjs`（11 断言，可重跑）。

## B. 架构变化和复用说明

- 复用：Layout/TabArea/InspectorPanel/ActivityPanel/ui.tsx（Modal/ErrorPanel/
  EmptyPanel/StatusBadge/SkeletonPanel）/theme/api.ts/useApi/懒路由模式全部
  原样复用，零壳层改动；Inspector/Activity 面板自动承接新路由（深链接登记
  会话 tab 走既有机制）。
- 新增组件 data.tsx 是纯展示基元，不发请求不 import workspaces.ts，与
  InspectorPanel/ActivityPanel 同款「布局与数据分离」纪律。
- 五态结果卡在总览页标注「待交付」（P5-7），不渲染编造数字；资产页空态
  写明交付批次——「不用静态占位冒充真实功能」的执行方式是真实空态 +
  边界说明，而非假数据。

## C. 修改文件清单

```text
apps/server/rp_server.py                                    # handle_tasks 单条 + status_history
web/src/App.tsx                                             # research 子页路由族
web/src/workspaces.ts                                       # research 四 tab
web/src/components/data.tsx                                 # 新增（DataTable/Filter/CountChips）
web/src/routes/ResearchPage.tsx                             # 占位 → 总览着陆
web/src/routes/research/shared.tsx                          # 新增（类型/tone 词表/列配置）
web/src/routes/research/ResearchOverviewPage.tsx            # 新增
web/src/routes/research/ResearchTasksPage.tsx               # 新增
web/src/routes/research/ResearchTaskDetailPage.tsx          # 新增
web/src/routes/research/ResearchAssetsPage.tsx              # 新增
web/src/routes/research/ResearchKnowledgePage.tsx           # 新增
web/tests/workspaces.test.ts                                # 路由数 12→15
web/tests/shell-smoke.test.tsx                              # tab 断言 11→14
web/qa/visual/pages.spec.ts                                 # marker 研究工作台→研究总览
web/qa/visual/baseline/pages.spec.ts/research-rip-*.png     # 四基线重生成
web/scripts/rip3_p5_browser_e2e.mjs                         # 新增（P5 系列 e2e 模板）
docs/rip3/batches/browser-e2e/p5-1/*.png                    # 七张实测截图
```

## D. 数据库迁移、Schema 和兼容性变化

零迁移（SQLite 链未动、migrations_pg 链未动）。rp_server 变更只增只读参数，
向后兼容（无 task_id 时行为与旧版完全一致）。

## E. 测试命令及真实结果

```text
cd web && npm run typecheck        # 0 error
cd web && npm run build            # built in 622ms（chunk 体积警告为既有基线行为）
cd web && npm run test             # Test Files 19 passed (19) / Tests 124 passed (124)
cd web && npm run format:check     # All matched files use Prettier code style!
cd web && npx playwright test      # 71 passed（pages 66 + workflows 5，含 axe AA 22 项）
RP_DB_PATH=rip-data/dev/p5-platform.db RP_AUX_DB=rip-data/dev/p5-aux.db \
RP_AUTH_TOKEN=p5browser RP_PORT=7801 python3 apps/server/rp_server.py   # 测试栈
cd web && node scripts/rip3_p5_browser_e2e.mjs   # 11/11 passed
curl /api/tasks?task_id=rtis-20261102-220853-7827   # item=true status_history=true
```

测试库为副本：`cp rip-data/dev/webtest-platform.db rip-data/dev/p5-platform.db`
（42 条 research_task）与 `cp migrations/dev-b5.db rip-data/dev/p5-aux.db`
（3 条 tool_run），权限 600，生产 migrations/platform*.db 零触碰。

## E2. CI 各 Job 状态和运行链接

随批 push 后以 HEAD run 为准（批内自查全部绿，见 E 节）。

## F. 性能数据

- /api/tasks?limit=200（42 行）与 /api/runs?limit=200 均为既有端点，无新查询
  面；任务详情单条主键查询。页面级性能基线随 P5-6（Activity 实时面）建立。
- 视觉门实测：e2e 11 断言含 390px 无横向溢出（overflowPx=0）。

## G. 未解决问题与风险

1. **视觉门粒度局限（重要观察）**：pixelmatch 阈值（YIQ 0.2）+ 
   maxDiffPixelRatio 0.02 对「同壳层、大面积空画布、局部卡片内容替换」的
   页面改动灵敏度不足——占位页与新总览页的 YIQ 大差异像素实测约 2.1%，恰好
   在门限附近抖动（同一基线同一渲染多轮跑有 pass/fail 边界翻转实录）。本批
   已把四张 research 基线显式重生成到新页面形态（不依赖边界行为）；后续
   页面族批次凡改内容页建议主动重生成基线而非依赖 diff 拦截。该局限登记为
   Phase 5 收口前评估项（是否收紧 maxDiffPixelRatio）。
2. 资产页/验证结果卡当前是诚实空态，P5-2/P5-5/P5-7 落地后必须替换为真实
   数据面（本批已写明交付批次，防止遗留成永久占位）。
3. Tasks 过滤为客户端过滤（服务端列表无 q/state 参数，limit 200 口径与
   ProjectsPage 一致）；服务端过滤参数随 P5-2 编排域端点一起设计。

## H. 下一批依赖和明确的非目标

- P5-2 依赖：本批任务详情页与 Inspector 对象面（编排域字段就位后直接扩展
  行渲染）；SQLite 020 迁移（研究编排域表，P5-0-B3 判定冻结）。
- 非目标：本批不建编排域、不接 Provider、不做五态、不扩 Scan Center/
  Coverage/Findings/Evidence/Trace tab（无死 tab 纪律）。

## I. UI 截图与浏览器验收证据

docs/rip3/batches/browser-e2e/p5-1/（真实 rp_server 7801 + dev 库副本 +
vite proxy，chromium 实拍，1600×1000 / 1440×900 / 390×844）：

| 文件 | 内容 |
| --- | --- |
| 01-research-overview-1600.png | 总览 KPI + 状态 chips + 最近任务（42 任务真实计数） |
| 02-research-tasks-filtered.png | 任务表（状态=CREATED 过滤 28 行） |
| 03-research-task-detail.png | 详情：字段 + status_history 时间线 + 关联运行诚实空态 |
| 04-research-assets-empty.png | 资产页诚实空态与交付说明 |
| 05-research-knowledge-query.png | 知识上下文「EZVIZ」检索命中 8 条 |
| 06-research-overview-1440.png | 1440×900 验收 |
| 07-research-overview-390.png | 390×844 验收（无横向溢出） |

e2e 断言清单：总览渲染/无 JS 异常/任务真实行数/过滤词表/过滤生效/详情深链接/
时间线在位/资产空态/知识检索真实命中（hits=8）/390 无溢出/四 tab 在位。

## J. RTIS 对账和数据安全回归结果

零 RTIS 写操作；RTIS 面零触碰（本批只读 3.0 侧 SQLite 镜像与 PG 知识面检索）。
integrations/rtis 零修改。测试库为副本（600 权限），生产库零触碰。
