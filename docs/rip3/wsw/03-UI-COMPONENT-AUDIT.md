# WSW 前置审计 03 · UI 组件与可复用面盘点（web/src）

> **文档类型：** 只读审计记录（WSW 批次 Phase 0，AI-IMPLEMENTATION-GUIDE §11 Phase 0）
> **范围：** `web/src` 现有可复用组件面、主题 Token、API/状态管理、路由与工作区注册、e2e 骨架、截图目录；以及 WSW 十一子页面（Proxy/History/Repeater/Fuzzer/WebSocket/Intruder/Decoder/Comparer/Passive/Plugin/Settings）对照 `docs/rip3/WEB-SECURITY-WORKBENCH-PLAN.md` 的缺件清单。
> **审计方式：** 全量读 `web/src`（58 文件）+ 依赖清单 + e2e/视觉门配置 + 截图目录实勘；本批零代码改动。
> **日期：** 2026-11-18

---

## 0. 《docs/ui-ux/AI-IMPLEMENTATION-GUIDE.md》第 0 节最高优先级指令要点

全文已读（743 行）。第 0 节「给接手 AI 的最高优先级指令」（AI-IMPLEMENTATION-GUIDE.md:14-30）要点如下，WSW 批次全程受其约束：

1. **任务不是重新设计或重写整个 RIP**，也不是一次性把所有业务页面画完（:19）。
2. **先理解现状，再动手**：先读仓库 `AGENTS.md`、`README.md`、`docs/SESSION-HANDOFF.md`、`docs/README.md`，再核对 `web/` 实际代码、路由、组件、样式和测试（:22）。
3. **Figma 是视觉基准，不是业务事实来源**：业务能力、API、状态、数据结构以仓库代码、Schema 和契约文档为准（:23）。
4. **保留现有架构和功能**：不得因设计稿或个人偏好重写 Rust Core、Workers、服务端、API、数据库或整个 Web 前端（:24）。
5. **先复用，再扩展**：优先复用现有组件、依赖、设计 Token、API 和状态管理；确认现有能力无法满足才新增实现（:25）。
6. **一次只做一个明确批次**：先审计、列最小计划、实施、测试、视觉验收、更新文档；不得未经要求铺开所有页面（:26）。
7. **真实数据优先**：不得把静态数字、假日志、假运行状态伪装成生产数据；Mock 必须明确隔离并标注（:27）。
8. **设计持续演进**：在现有 Figma 文件和组件体系内补充，**不创建平行设计系统**，不随意推翻已有视觉方向（:28）。
9. **冲突裁决**：本文件与仓库实际实现冲突时，以实际代码、当前有效契约及仓库级 `AGENTS.md` 为准；先记录差异再提最小修订（:30）。

与 WSW 直接相关的落地条款：Token 映射纪律——指南 §3.1 的建议 Token（`--rip-*` 十三项，:109-123）明确要求「实施时先检查 `web/` 是否已有等价 CSS Variables / daisyUI Theme；**如已有，应映射到现有命名，不要创建重复 Token**」（:107）；批次固定工作流与验收清单见指南 §12/§14（:610-689）。

---

## 1. 现有可复用组件清单（存在 / 部分 / 缺）

判定口径：「存在」= 有独立可复用组件且已被多页消费；「部分」= 有雏形或仅页内联实现，复用需抽取/增强；「缺」= 全 `web/src` 无任何实现（已用 grep 实勘：`Splitter|virtual|CodeMirror|monaco|LogViewer|JsonViewer|CodeEditor|Drawer|EventSource|WebSocket` 等 0 命中或仅无关命中）。

### 1.1 存在（可直接复用）

| 能力 | 组件/文件 | 证据与说明 |
|---|---|---|
| 全局壳层（五区） | `web/src/components/Layout.tsx:111-553` | 顶栏（工作区切换/⌘K 搜索/运行任务/Runner/通知）+ 左图标轨 + 中央多标签 + 右 Inspector + 底部 Activity；沉浸态预留 `IMMERSIVE_PATH`（Layout.tsx:66，当前不命中任何路由） |
| 侧边导航（图标轨 rail） | `Layout.tsx:385-428` + `styles/shell.css:31-165` | 由 `WORKSPACES` 数据驱动，含主题切换按钮；窄屏转抽屉（`.rail.is-open` + `.rail-backdrop`，body 滚动锁 + Esc，Layout.tsx:181-194） |
| 中央多标签 Tabs | `web/src/components/TabArea.tsx`（通用组件） | 与数据无关：pinned 固定 tab + 会话 tab（可关闭），由 Layout 组装（Layout.tsx:222-258, 520-528）；结构约束注释见 TabArea.tsx:1-13 |
| Modal | `web/src/components/ui.tsx:110-200` | 唯一弹层：Esc 关闭、锁 body 滚动、焦点圈（focus-trap）、焦点还原、遮罩是真 button |
| Dialog/Toast | `web/src/components/Dialogs.tsx` | `useDialogs()` promise 化 confirm/alert/toast，零业务依赖 |
| 分级确认对话框 | `web/src/components/TrustConfirmDialog.tsx` | 信任级驱动确认强度（single/confirm/provenance/double_confirm_typed），策略不可得 fail-closed |
| 状态徽标 | `ui.tsx:12-24 StatusBadge` | 泛型：label map 与 tone map 由调用方注入，未知状态直显原文 |
| 空态 | `ui.tsx:75-99 EmptyPanel` | 图标/文案/可选动作按钮 |
| 错误态 | `ui.tsx:41-73 ErrorPanel` | 按 ApiError.status 分文案（401/403/404/通用）+ 重试 |
| 骨架屏 | `ui.tsx:26-39 SkeletonPanel` | 行数可调 |
| PageHeader | `ui.tsx:202-209` | 标题 + actions 区 |
| DataTable | `web/src/components/data.tsx:37-86` | 列配置驱动、行 key、空态行、行激活语义、sr-only caption；样式沿用 token 面（rounded-surface/border-edge） |
| FilterSelect / FilterSearch / CountChips | `data.tsx:88-161` | aria-label 下拉过滤 / 文本过滤（清空钮）/ 计数芯片组（tone 注入） |
| Inspector 属性面板 | `web/src/components/InspectorPanel.tsx` | 通用：sections 由调用方传入，open prop 控制折叠；现内容由 Layout 填充（Layout.tsx:260-316） |
| 底部 Activity 面板 | `web/src/components/ActivityPanel.tsx` | 通用：tabs 由调用方传入；现 5 页签中「日志/输出/状态」为占位文案（Layout.tsx:336-353） |
| 全局搜索 | `components/GlobalSearchPalette.tsx` + `lib/global-search.ts` | ⌘K（Layout.tsx:151-160），索引源为真实端点 /api/tools、/api/knowledge、/api/tasks + 页面组 |
| 路由级错误边界 | `components/ErrorBoundary.tsx` | scope 命名 + reset/reload 双出口；按路由 re-key（Layout.tsx:532），单页错误不炸壳，PROD 隐藏 stack |
| 图表 | `components/EChart.tsx` + `echarts-install.ts` | 直连 echarts/core（绕开 echarts-for-react 测量链事故，1085→561 kB），ResizeObserver 跟随，强制关闭入场动画（prefers-reduced-motion 与截图双兼顾） |
| 工作流画布族 | `components/workflow/{FlowCanvas,NodePalette,NodeInspector,RunPanel,VersionList}.tsx` | @xyflow/react 12.12.1（package.json dependencies），画布/节点/连线/运行面板可参考 |
| 占位页 | `routes/WorkspacePlaceholder.tsx` | 编号/标题/后续填充项/关联入口结构化占位（SettingsPage.tsx 等消费） |
| 焦点圈 | `components/focus-trap.ts` | 独立纯模块，有单测（web/tests/focus-trap.test.ts） |
| 日期格式化 | `lib/format.ts:3-10` | 仅 `formatDateTime` 一个可复用函数 |

`ui.tsx`/`data.tsx` 消费面实勘：`components/data.tsx`（DataTable/Filter 族）已被 5 个页面族消费（routes/research/{ResearchOverviewPage,shared,ResearchTasksPage,ResearchPlansPage,ResearchKnowledgePage}.tsx）；`ui.tsx` 四组件 + PageHeader 被 20 个路由文件消费（grep 实勘清单见 §1.3 前的命令记录）。

### 1.2 部分（有雏形，WSW 复用需抽取或增强）

| 能力 | 现状 | 证据 | 缺口 |
|---|---|---|---|
| JSON 查看（Raw/Pretty） | Evidence 页内联实现：`JSON.stringify(row.payload, null, 2)` + Raw/Pretty 双按钮 + 大 payload 截断 | `routes/research/ResearchEvidencePage.tsx:8,32,43,213-232`；RunDetailPage artifact 为新标签打开 uri（RunDetailPage.tsx:7） | 不是独立组件：无折叠、无语法高亮、无搜索、无 Hex/Rendered 视图；WSW 消息查看器（Raw/Pretty/Headers/Params/Body/Hex/Rendered，WSW-PLAN:401）需抽成通用 `MessageViewer` |
| Drawer（可关抽屉） | 仅移动端 rail 抽屉 + Modal；Inspector/Activity 折叠为布尔开关（非拖宽） | Layout.tsx:181-194, 536-548；shell.css:52-78 | 无通用右侧/底部可拖拽 Drawer 组件 |
| 日志查看 | ActivityPanel「日志」页签为占位文案 | Layout.tsx:336-343 | 无 LogViewer（滚动跟随、级别染色、过滤、下载） |
| 分页 | 无分页组件；DataTable 注释明言「分页由调用方决定…本组件不隐藏分母」 | data.tsx:9-10；服务端 limit 参数散布（lib/api.ts 各 fetch 函数） | 无 Pagination 组件、无游标分页封装 |
| 过滤 | FilterSelect/FilterSearch 两个单字段控件 | data.tsx:88-136 | 无 Filter Builder（多条件组合，WSW History 需要，WSW-PLAN:397） |
| 请求执行模式 | 写操作经 apiSend POST + 轮询状态（workflowRun 受理/状态经 GET 轮询，api.ts:826-845 注释「受理即返回（异步承载），状态经 GET 轮询」） | api.ts:826-862 | 无 WebSocket/SSE/长轮询封装；Proxy/WS 实时流需新增事件通道 |

### 1.3 缺（全 src 无实现，grep 实勘）

| 缺件 | 实勘证据 | WSW 需求出处 |
|---|---|---|
| **Splitter（可拖分割条）** | grep `Splitter|splitter|resize|drag` 仅命中 EChart.tsx（ResizeObserver）与 xyflow 画布内部，无自研拖拽 | Repeater「中间分割线可拖动」（WSW-PLAN:411） |
| **虚拟滚动** | package.json dependencies 仅 7 项（@tanstack/react-query、@xyflow/react、echarts、lucide-react、react、react-dom、react-router），无 react-window/react-virtual；src 无虚拟化实现 | History「大列表使用虚拟滚动、服务端分页或游标，不允许一次性加载所有 Body」（WSW-PLAN:404）；性能要求同指南 §7.3（AI-IMPLEMENTATION-GUIDE.md:365） |
| **CodeEditor（HTTP 消息/代码编辑器）** | grep `CodeMirror|monaco|contentEditable` 0 命中；现编辑形态仅 `<pre>` 只读与表单 input/textarea | 「请求编辑器需要等宽字体、行号、语法高亮、搜索、换行控制和大文本性能优化」（WSW-PLAN:433） |
| **LogViewer** | 无任何日志流组件 | Proxy 页底部 Activity：Listener 日志/TLS 错误（WSW-PLAN:390-393） |
| **WebSocket/SSE 客户端基础设施** | grep `EventSource|WebSocket|websocket` 在 web/src 0 命中 | WebSocket Workbench 帧捕获/重放（WSW-PLAN:274-283）与 Proxy 实时拦截队列 |
| **Tooltip/Popover/Dropdown 组件** | 仅原生 `title`/`aria-label`；下拉只有 `<select>`（FilterSelect、Layout 工作区切换器） | 指南 §6.1 组件目录（AI-IMPLEMENTATION-GUIDE.md:257） |
| **Breadcrumb / Pagination / Avatar / Switch / Checkbox 封装** | 无独立组件（Switch/Checkbox 可用 daisyUI 原生类，但无统一封装与测试） | 指南 §6.1（AI-IMPLEMENTATION-GUIDE.md:253-265） |
| **diff 视图（Comparer）** | 无 diff 库依赖、无 diff 渲染实现 | Comparer「两个 Request/Response/Body 对比」（WSW-PLAN:297-298） |
| **可调列宽/可排序 DataTable** | 现 DataTable 无排序、无列宽拖拽（data.tsx:37-86） | History「可排序、可调整列宽的请求列表」（WSW-PLAN:398） |

---

## 2. 主题与 Token 体系现状

- **双层结构**（token.css 头注：daisyUI 变量契约层 + @theme 应用层，`styles/token.css:1-6`）：
  - **daisyUI 5 自定义主题两枚，不启用内置主题**：`rip-dark`（默认暗色，oklch 全量色板，token.css:11-44）与 `rip-light`（明亮，token.css:50-82）。
  - **应用层语义 token（第二层）**：`--edge / --edge-soft / --sidebar / --surface-2 / --muted` 随 data-theme 切换（token.css:92-106），经 `@theme inline` 映射为 Tailwind 工具类 `bg-surface-2`、`border-edge`、`text-muted` 等（token.css:108-114）。
  - **语义圆角与控件高度**：`--radius-surface: 1rem`（rounded-surface）、`--radius-control: 0.625rem`（rounded-control）、`--control-h`（token.css:116-122）。
  - **确定性 CJK 字体栈**：`--font-sans`/`--font-mono` 钉死字族保证视觉快照不漂移（token.css:124-131）。
- **主题切换机制**：`theme.ts` — `THEME_OPTIONS` 仅 rip-dark/rip-light（theme.ts:6-13）、localStorage `rip_theme` 持久化 + `applyTheme` 设 `data-theme`（theme.ts:38-46）、退役 id 归一化映射表预留（theme.ts:19-21）。
- **首帧防闪烁**：`index.html:2` 默认 `data-theme="rip-dark"` + 内联脚本首绘前解析 localStorage（index.html:16-26）；theme.ts 头注强调 index.html 与 theme.ts 两处映射必须同步改。
- **对比度纪律**：rip-light primary 从 L=0.55 压到 0.52 过 axe AA 4.5:1（token.css:47-49）；`--muted` 为实色 token，取代透明度稀释（token.css:86-91）。
- **与指南 §3.1 的关系**：指南建议的 `--rip-bg/--rip-accent` 等 13 项 Token（AI-IMPLEMENTATION-GUIDE.md:109-123）与现有 daisyUI/oklch 命名并存但语义可一一映射；按指南 :107 纪律，**WSW 不新建 Token，只映射到现有命名**（primary≈accent、base-100≈surface、muted 已有等）。

---

## 3. API Client 与状态管理现状

- **API 基础层**（`web/src/lib/api.ts`，零业务依赖的底座）：
  - `ApiError`（status + data，api.ts:6-16）；**非 JSON 响应防御**——200 HTML 错误页不再伪装成 `{}`（api.ts:41-56）。
  - **Bearer 认证**：`setAuthTokenProvider` 注入令牌来源（api.ts:22-39）；401 派发 `AUTH_EXPIRED_EVENT`（api.ts:18-20, 58-61），不跳登录页（RIP 无登录面）。
  - `apiGet` / `apiSend`（api.ts:80-96），端点函数族已覆盖：plugins（含 trust-policy/manifest-inspect/install/upgrade/cleanup-rollback）、knowledge_sources、agents/agent-runs、workflows/workflow-runs（含 run/pause/resume/retry/put）、mcp-catalog、documents（api.ts:98-990）。
  - **WSW 启示**：新端点函数按现有模式追加进 api.ts 分节注释块（每个 Phase 一节），路径参数一律 `encodeURIComponent`。
- **TanStack Query 用法**（全仓唯一消费面 `web/src/lib/hooks.ts`）：
  - `useApi(path|null, {fetcher?, refetchIntervalMs?})`：queryKey=`["api", path]`、staleTime 0、retry false、窗口聚焦/重连不重取、`placeholderData: keepPreviousData`（旧数据留屏）、非 ApiError 统一包成 `ApiError(0,...)`（hooks.ts:40-90）。
  - 身份变更 `IDENTITY_EVENT`（rip:identity）令全部在屏查询失效（hooks.ts:20-24, 46-52）；`refetchIntervalMs` 是现成的轮询口（hooks.ts:36-38）——WSW 轮询面（拦截队列、Fuzzer Job 状态）应走它而非自建定时器。
  - QueryClient 默认值与 useApi 对齐（App.tsx:105-116），client 随 App 挂载创建、测试互不串扰（App.tsx:118-121）。
  - 头注红线：hooks.ts 是 **src 内唯一 react-query 消费面**（hooks.ts:1-3）——页面不得直用 useQuery 绕过。
- **Mock 纪律**：生产代码无 mock 层；Playwright 离线模式在测试内 `route-mock` 全部 `/api/**`（scripts/visual-server.mjs:1-8 头注 + playwright.mcp.config.ts:1-15），属测试夹具口径，不构成生产回退路径。

---

## 4. 路由与工作区注册方式

- **workspaces.ts 是信息架构唯一数据源**（零 import 纯数据，workspaces.ts:1-17）：11 工作区 `WORKSPACES`（dashboard/projects/research/knowledge/tools/plugins/agents/workflows/runs/runtime/settings，workspaces.ts:87-306）、`WORKSPACES_BY_ORDER` 防御排序（:309）、`tabNumberOf` 稳定编号 `N.M`（:318-320）、`buildFeatureMap` 功能地图推导（:357-385）。
- **路由注册**：App.tsx 全部一级路由 `lazy()` 动态 import（初始包只带壳+当前路由，App.tsx:1-94），嵌套在 `<Route element={<Layout />}>` 下（App.tsx:126-153），通配回 `Navigate to="/"`。
- **新工作区接入三步**（现有模式）：① `workspaces.ts` 加 Workspace 条目（图标为字符串键）；② `Layout.tsx` 的 `ICONS` Record 加映射——全键覆盖，漏键 tsc 报错（Layout.tsx:44-58）；③ App.tsx 加 lazy Route。不变量由 `web/tests/workspaces.test.ts` 锁定（编号漂移/路径冲突，workspaces.test.ts:1-30）。
- **WSW 对应**：`Web Security` 作为第 12 个工作区加入（WSW-PLAN:337「不另建一套独立 SPA」），子页以 workspace.tabs 扩展；编号坐标沿用 tabNumberOf 机制。

---

## 5. e2e / 测试骨架现状

### 5.1 独立浏览器 e2e 脚本（`web/scripts/`，5 个）

模式统一：`@playwright/test` 的 chromium + `probeChromium()` 三级探测（PW_CHROMIUM 环境变量 → ~/.cache/ms-playwright 用户缓存 → 系统 chromium，rip3_p5_9c_browser_e2e.mjs:13-27），**真后端**（rp_server 7801 + 实拍库 + vite proxy），`localStorage.setItem("rip_auth_token", ...)` 注入凭据，`pageerror` 收集断言零未处理异常，PASS/FAIL 计数 + exit code，截图目录 argv[2] 可覆盖：

| 脚本 | 覆盖 | 默认截图目录 |
|---|---|---|
| `p5_current_ui_shots.mjs` | Phase 5 前现状截图（正常态+过期 token 态） | `docs/rip3/ui-reference/current/` |
| `rip3_p1_browser_e2e.mjs` | /documents 页全链（真 PG 容器） | `/tmp/rip3-e2e`（argv 可覆盖） |
| `rip3_p5_browser_e2e.mjs` | Research Workspace 骨架 | `docs/rip3/batches/browser-e2e/p5-1/` |
| `rip3_p5_9b_browser_e2e.mjs` | Evidence 页 + Coverage 矩阵（实拍库） | `docs/rip3/batches/browser-e2e/p5-9b/` |
| `rip3_p5_9c_browser_e2e.mjs` | §14 六场景（任务/模板/计划 CONFIRMED/证据 marker/覆盖盲区/Trace） | `docs/rip3/batches/browser-e2e/p5-9c/` |

### 5.2 Playwright 门禁配置（4 份）

- `playwright.config.ts`：**视觉门**——testDir `qa/visual`，快照基线 `qa/visual/baseline/{testFileName}/{arg}`，`maxDiffPixelRatio 0.02`，animations disabled + 页内注入 Noto Sans CJK SC 钉字体；webServer = `scripts/visual-server.mjs 4177`（离线静态服务器，服务 dist + SPA fallback，零 API）。
- `playwright.plugins.config.ts`（P6-8 插件面）、`playwright.mcp.config.ts`（P9-4 MCP 状态卡）、`playwright.plugins-trust.config.ts`（P10-2 信任级确认）：离线 route-mock 模式 + `RP_P6_E2E=1`/`RP_P9_E2E=1` 切真后端。
- 运行入口：`npm run test:visual:gate`（= `tsc --noEmit && vite build && playwright test`，package.json scripts）。

### 5.3 单测（vitest）

`vite.config.ts:71-77`：vitest 只跑 `tests/**/*.test.{ts,tsx}`；`*.spec.ts` 归 Playwright 门禁。`web/tests/` 22 个文件：18 个页面/模块 smoke（agents/api/documents/echart/focus-trap/global-search/knowledge/mcp/overview/plugins/projects/runs/shell-components/shell/tools/trace/trust-confirm/workflows）+ 3 个 .spec.ts（Playwright）+ `workspaces.test.ts`（十一工作区不变量）。

### 5.4 既有截图目录实勘

| 目录 | 内容 |
|---|---|
| `web/qa/visual/baseline/pages.spec.ts/` | 45 张基线：11 工作区着陆页 × rip-dark/rip-light × desktop/mobile |
| `web/qa/visual/baseline/workflows.spec.ts/` | 4 张 workflows 基线 |
| `web/qa/plugins/` | 12 张（插件目录/详情双主题双视口 + e2e 过程图）；`web/qa/mcp/` 为空目录 |
| `docs/phase9/browser-e2e/` | 01-04 真后端截图 + mock/ 目录 + README |
| `docs/rip3/batches/browser-e2e/` | p5-1/、p5-9b/、p5-9c/ 三个批次目录 + 01-documents-list 等 3 张 + summary.json |
| `docs/rip3/ui-reference/` | `current/`（6 张现状）+ `yakit/`（yakit-home / yakit-http-history / yakit-web-fuzzer，3 张参照截图） |

**WSW 骨架建议落点**：沿用 rip3_p5_9c 模式新增 `web/scripts/rip3_wsw_browser_e2e.mjs`（真 rp_server + WSW 工作区子页遍历 + 截图落 `docs/rip3/batches/browser-e2e/wsw/`），视觉基线沿用 qa/visual/baseline 目录约定。

---

## 6. WSW 十一个子页面缺件清单

对照 `docs/rip3/WEB-SECURITY-WORKBENCH-PLAN.md` §5.1-5.6（子导航 :339-351，各页布局 :355-426，统一交互规范 :428-436）。**总原则（红线）**：Web Security 是新工作区入口，不替换全局导航或五区壳层（WSW-PLAN:337,353）；主题沿用 RIP Token（:430）；表格/过滤器/详情面板/状态徽标/空态/错误态/Loading 沿用现有组件（:431）；不用静态假数据伪造流量/任务/结果（:436，同指南 §0:27）。

**四个子页共用的公共缺件（先建才能铺页面）**：
1. **HTTP 消息查看/编辑器**（Raw/Pretty/Headers/Params/Body/Hex/Rendered + 等宽行号编辑）：现最优雏形是 ResearchEvidencePage 内联 Raw/Pretty `<pre>`（ResearchEvidencePage.tsx:32,213-232），需抽成通用 `MessageViewer`/`MessageEditor`，编辑器本体（行号/高亮/大文本）为全新缺件（§1.3）。
2. **可拖 Splitter**：全新缺件（§1.3），Repeater 明确要求（WSW-PLAN:411）。
3. **虚拟滚动大列表**：全新缺件（§1.3），History/结果区明确要求（WSW-PLAN:404）。
4. **实时流通道**（WebSocket/SSE 或轮询封装）：src 零命中（§1.3）；Proxy 拦截队列与 WS 帧捕获依赖。
5. **Filter Builder**：现仅单字段 FilterSelect/FilterSearch（data.tsx:88-136），History 需要多条件组合（WSW-PLAN:397）。
6. **可排序/可调列宽 DataTable**：现 DataTable 无此能力（§1.3），History 需要（WSW-PLAN:398）。

| # | 子页面 | 可直接复用 | 缺件 |
|---|---|---|---|
| 1 | **Proxy / MITM** | 壳层五区（Layout 已按 Proxy 页布局对齐设计：左队列/中消息/右 Inspector/底 Activity，WSW-PLAN:355-393 全部能映射到现有区）；StatusBadge（拦截类型/规则命中）、Modal（确认）、CountChips（连接/排队计数）、InspectorPanel（URL 解析/Scope/关联） | Listener 状态条组件（Start/Stop/Intercept 开关）、拦截队列虚拟列表、Request/Response 双编辑面板、Forward/Drop/Send-to-* 操作条、实时流通道（§6 公共件 1/2/4）、证书状态面 |
| 2 | **HTTP History** | DataTable（需增强）、FilterSearch、StatusBadge、EmptyPanel/ErrorPanel、PageHeader、lib/api 分节模式 | 可排序/可调列宽 DataTable、虚拟滚动、Filter Builder、MessageViewer 详情（Raw/Pretty/Headers/Params/Hex/Rendered）、行操作组（Send to Repeater/Fuzzer/Comparer/Decoder/Evidence/Export）、Host 分组与 Scope 过滤、服务端游标分页封装 |
| 3 | **Repeater** | TabArea（多 tab 语义可复用，但现语义是路由 tab，请求级 tab 需页内状态）、Modal、Dialogs | HTTP 请求编辑器（§6 公共件 1）、Response Viewer（状态/耗时/长度/协议头行）、可拖 Splitter、编辑历史回退/恢复、Send 中状态 + Cancel、请求模板保存（WSW-PLAN:406-415） |
| 4 | **Web Fuzzer** | EChart（状态码/长度/耗时分布图）、DataTable（结果表）、StatusBadge、CountChips、ErrorPanel | 四区结构容器（请求/Payload/运行/结果，WSW-PLAN:419-424）、Payload Position 标记编辑器、Payload 配置面（来源/Mutator/Processor/编码链）、运行控制面（并发/速率/预算/暂停/取消）、启动前请求数估算 + Scope 判定展示 |
| 5 | **WebSocket** | StatusBadge、InspectorPanel、DataTable | WS 客户端基础设施（src 零命中）、连接/握手捕获列表、帧列表（区分 WS 帧与 HTTP 实体，WSW-PLAN:283）、帧编辑/重放、帧↔连接↔History↔Evidence 关联面 |
| 6 | **Intruder Jobs** | DataTable、StatusBadge、CountChips、activityTab 轮询模式（refetchIntervalMs） | 批量任务表（进度列/暂停/取消）、任务详情（payload 位置统计）、Job 受理→轮询模式（可仿 workflowRun api.ts:826-845 的受理+GET 轮询契约，但 WSW 端点为新增） |
| 7 | **Decoder** | Modal、daisyUI 控件类、font-mono token（token.css:129-131） | 编解码/哈希/格式转换交互面（纯前端可实现的全新页面）、输入输出互换、复制/发送到 Repeater 动作（WSW-PLAN:285-293） |
| 8 | **Comparer** | DataTable（对比项列表）、Modal | 双栏 diff 视图（无 diff 库依赖，§1.3）、差异高亮/差异项导航、两个 Request/Response/Body/Fuzzer Result 的选取面（WSW-PLAN:297-298） |
| 9 | **Passive Audit** | DataTable、StatusBadge、CountChips、FilterSelect | 规则命中列表与详情（规则↔请求关联）、被动审计事件流（依赖实时流通道）、与主动扫描共享 Scope/Budget/Trace 上下文的展示面（WSW-PLAN:305-313） |
| 10 | **Plugin Center** | **复用度最高**：现有 /plugins 页面族（PluginsPage/PluginDetailPage）、TrustConfirmDialog、pluginAction/pluginInstall/fetchTrustPolicy 全套 api（api.ts:231-365）、WSW-PLAN:315-320 明确复用 Tool/Plugin Registry 与 Runtime | WSW 语境的扩展点分类视图（规则/Payload Processor/Passive Check/数据提取器/格式转换器，WSW-PLAN:320）、规则类插件与 WSW 功能面的接线 |
| 11 | **Settings（Proxy Settings）** | 现有 SettingsPage 为结构化占位（SettingsPage.tsx:1-12，planItems 机制）；WorkspacePlaceholder | 代理设置表单面（Listener 地址端口/证书/上游代理/Scope/捕获策略）、设置读写端点（api.ts 尚无 settings 面）、表单校验与提交反馈模式 |

**注册动作汇总**（对齐 §4 三步模式）：workspaces.ts 新增 `web-security` 工作区（11 个 tabs）；Layout ICONS 加图标键；App.tsx 加 11 条 lazy Route（或先 entry 单 tab 渐进扩）；`web/tests/workspaces.test.ts` 的 EXPECTED_IDS 需同步扩为 12（该测试锁「顺序即 order」不变量，workspaces.test.ts:29-30）。

---

## 7. 本批红线与结论

1. **UI 沿用现有体系**：全部 WSW 页面用现有五区壳层 + token.css 双主题 + daisyUI/Tailwind 工具类 + ui.tsx/data.tsx 组件；不另建 SPA、不建平行设计系统（WSW-PLAN:337,430-431；AI-IMPLEMENTATION-GUIDE.md:28,107）。
2. **假数据禁用**：所有流量/任务/结果/状态来自真实后端端点；Playwright route-mock 仅存在于测试内（指南 :27、:489-499；WSW-PLAN:436）。
3. **新增依赖纪律**：Splitter/虚拟滚动/CodeEditor/diff 四类缺件涉及新依赖（如 @tanstack/react-virtual、CodeMirror、diff），按仓库 AGENTS.md 成熟库优先 + 依赖审计流程走，先确认现有能力（xyflow/daisyUI/echarts）不可覆盖再引入（指南 :25,174）。
4. **后端先行**：缺件中「Listener 状态/拦截队列/WS 帧/被动审计/代理设置」同时是 API 缺口（api.ts 现无任何 WSW 端点）；UI 批次前需后端契约批次（WSW-PLAN §六 Rust-First 架构），UI 不得发明字段与状态枚举（指南 §8.1,476-477）。
5. 可复用面总量判断：壳层/三态/表格/过滤/徽标/弹层/图表/画布共 24 项可直接复用（§1.1），9 项缺件中 4 项是公共底座（§6 公共件），建议 WSW 第一个批次先落「公共件（MessageViewer/Splitter/虚拟列表/实时流）+ Web Security 工作区注册 + Proxy 页骨架」。
