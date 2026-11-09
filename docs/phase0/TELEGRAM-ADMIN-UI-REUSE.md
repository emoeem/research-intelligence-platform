# Telegram Bot Admin 前端复用盘点

- 日期：2026-11-09
- 盘点对象：`/home/emo/code/telegram-bot/admin/`（只读检查，未改动该仓库任何文件）
- 对齐基准：`research-platform/docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md` §2.2（复用分三级）与 §10.7（前端复用落地原则）
- 结论原则：所有结论来自本次实际读到的文件与实际执行的命令；未验证项标注「待验证」
- 红线确认：RIP 最终自建前端应用，不依赖 telegram-bot 仓库整体；本文给的是「搬什么、怎么切、多大工作量」，不是引入仓库级依赖

工作量级定义（下文通用）：S = 0.5 人日以内；M = 1 至 3 人日；L = 1 周量级或更多。按一名熟悉 React 的工程师独立完成估算。

---

## 1. 工程基线（实测）

依赖版本取自 `admin/package.json`：

| 依赖 | 版本 | 备注 |
|---|---|---|
| react / react-dom | ^19.2.8 | |
| react-router | ^8.3.0 | `createBrowserRouter`，basename `/admin` |
| vite | ^8.2.2 | Vite 8 / Rolldown |
| tailwindcss + @tailwindcss/vite | ^4.3.3 | Tailwind 4，CSS-first token |
| daisyui | ^5.7.20 | 组件样式层，无 AntD 类组件库 |
| echarts + echarts-for-react | ^6.1.0 / ^3.0.6 | 动态加载，不进首屏 |
| typescript | ^7.0.2 | strict + noUncheckedIndexedAccess |

构建与脚本（`admin/package.json:6-10`）：`dev`、`build`（`tsc --noEmit && vite build`）、`typecheck`。双入口：`index.html`（管理端）与 `survey.html`（公开问卷端），见 `admin/vite.config.ts:22-31` 的 `rolldownOptions.input`。

规模实测（`wc -l`）：

- 关键基础设施文件合计约 2200 行：Layout.tsx 648、Dialogs.tsx 367、ui.tsx 188、ErrorBoundary.tsx 78、focus-trap.ts 59、StatCard.tsx 81、DeleteWithUndo.tsx 67、workspaces.ts 221、theme.ts 56、global-search.ts 220、hooks.ts 49、App.tsx 128、pwa.ts 39。
- `src/index.css` 7638 行，是主题 token 与组件样式的主要资产。
- `src/api.ts` 1539 行，其中约 1300 行是问卷/报告/授权业务接口封装，通用 fetch 层约 100 行。
- `src/routes/` 下 30 个页面文件。

测试体系（telegram-bot 仓库根，均在本次实测确认存在）：

- 单测：根 `vitest.config.ts` + `tests/unit/admin/` 下 20 个用例文件（含 `workspaces.test.ts`、`global-search.test.ts`、`lazy-routes.test.ts`），纯逻辑模块可以在无 DOM lib 的根 tsconfig 下编译。
- 视觉回归：根 `playwright.config.ts`（testDir `./qa`，webServer 用 `scripts/visual-qa-server.mjs`，单 worker，`screenshot: only-on-failure`）；`qa/visual/admin.spec.ts` 401 行；门禁脚本 `pnpm test:visual:gate`（根 `package.json:19`）。
- a11y：`qa/visual/a11y.ts` 用 `@axe-core/playwright` 扫 WCAG 2.0/2.1 AA，serious/critical 级违规阻断。

---

## 2. 一级：直接借鉴

对齐 §2.2 第 1 条与 §10.7 第 1 条。这一级的共同特征：借鉴的是「方法与结构」，RIP 侧按自己的信息架构重写，不做代码级搬运。

### 2.1 Layout 壳与工作区导航模型

- 实际路径：`admin/src/components/Layout.tsx`（648 行）、`admin/src/workspaces.ts`（221 行）
- 现状：
  - 两层导航：桌面常驻 56px 工作区图标轨 + 进入工作区后主区域顶部的横向 tab 条（`workspaces.ts:63-78` 的设计注释；渲染在 `Layout.tsx:256-344` 与 `Layout.tsx:426-447`）。
  - 角色驱动的导航过滤：creator 只见问卷工作区，未接授权中心的部署隐藏授权/控制中心两个 tab（`Layout.tsx:149-163`）。
  - 移动端抽屉：`<640px` 抽屉 + 遮罩按钮 + body 滚动锁 + Esc 关闭 + 路由变化自动收起（`Layout.tsx:170-185`、`258-344`）。
  - 沉浸式编辑态：编辑器/场景编辑器路径整宽渲染，图标轨与 tab 条隐藏，正则 `IMMERSIVE_PATH`（`Layout.tsx:75`、`168`）。
  - 路由级错误隔离：`ErrorBoundary key={location.pathname}`，单页渲染错误不炸整个壳（`Layout.tsx:451-453`）。
  - 稳定编号坐标：工作区 order（1 至 4）与 tab order 组成 `1.4` 式编号，角色过滤后编号不漂移（`workspaces.ts:149-156`）。
- 耦合点：
  - 数据依赖：`fetchAdminSession` / `fetchDeploymentInfo` / `fetchEnvironment` 来自业务 `api.ts`（`Layout.tsx:29-36`、`137-145`）。
  - Telegram 认证：`getTelegramInitData` 决定测试横幅与浏览器模式横幅（`Layout.tsx:40`、`223-227`）。
  - 业务 IA 数据：`workspaces.ts:79-144` 的问卷/内容/社区/系统四个工作区是问卷业务的组织方式。
  - 标题映射与返回逻辑硬编码业务路由（`Layout.tsx:193-253`）。
- 借鉴方式：壳结构（图标轨 + tab 条 + 抽屉 + 沉浸态 + 按路由 re-key 的 ErrorBoundary）整套照抄结构；`workspaces.ts` 的数据模型（零 import 纯数据、图标存字符串键由壳映射组件、稳定编号函数、`buildFeatureMap` 从同一数据推导功能地图）几乎可以原样采纳，只换成 RIP 的工作区数据。`workspaces.ts:7-11` 注释了它为什么保持零 import（根目录 vitest/tsc 连编译），这个仓内约定值得继承。
- 工作量级：M。数据模型搬运接近零成本，壳组件需按 RIP 路由与 API 重写（会话探测、环境标识、标题映射全部重做）。

### 2.2 全局搜索与命令面板交互

- 实际路径：`admin/src/global-search.ts`（220 行，纯逻辑）、`Layout.tsx:461-648`（GlobalSearchPalette 面板）、`Layout.tsx:127-135`（⌘K/Ctrl+K 注册）
- 现状：
  - 纯逻辑与 UI 分层：规范化/匹配/分组/排序在零依赖纯模块，面板只负责取数与交互（`global-search.ts:1-18` 头注释）。
  - 匹配模型：全角转半角 + 空白折叠的规范化（`global-search.ts:109-116`）、AND 多词匹配、首词前缀命中排前的组内排序（`global-search.ts:118-135`）。
  - 面板交互：⌘K 开关、Esc 关闭、上下键选择、Enter 打开、焦点恢复到触发元素、body 滚动锁、空态热门入口 chips、组头 + 扁平 hits 双结构（`Layout.tsx:502-525`、`536-644`）。
- 耦合点：
  - 索引数据源是三个业务列表接口（`Layout.tsx:479-481`：`/api/admin/surveys`、`/api/admin/report-templates`、`/api/admin/media-library`）。
  - 命中跳转目标与分组标签是问卷业务路由（`global-search.ts:75-79`、`186-215`）。
  - 热门词是问卷功能入口文案（`global-search.ts:97-102`）。
- 借鉴方式：匹配/分组/排序纯函数骨架整体平移（换掉类型里的三个业务组定义即可）；面板交互行为逐条对照复刻；RIP 的索引源换成 knowledge/plugin/tool/task 等 Application API。这正是 §2.2 说的「命令面板交互」直接借鉴与 §10.7 第 6 条「不因复用牺牲 RIP 信息架构」的落点。
- 工作量级：S（纯逻辑平移）+ S（面板对接 RIP 数据源），合计接近 M 下限。

### 2.3 主题系统

- 实际路径：`admin/src/theme.ts`（56 行）、`admin/index.html:11-33`（首帧内联脚本）、`admin/src/index.css:14-45`（foxgold 主题块）、`index.css:47-93`（`@theme` token）
- 现状：
  - 三主题（暗金/暗色/明亮），`data-theme` 属性切换，localStorage 持久化，退役主题 id 归一化映射（`theme.ts:23-37`）。
  - 首帧防闪烁：`index.html` 内联 IIFE 在首绘前解析存储并设 `data-theme`，与 `theme.ts` 的运行时解析逻辑保持同一映射。
  - token 双层：daisyUI 变量契约层（`--color-base-100` 等，含对比度复核记录，`index.css:5-13`）+ Tailwind 4 `@theme` 应用层（页面/墨色/边缘/侧栏色、语义圆角四档、统一控件高度 `--control-h`，`index.css:72-93`）；另有主题无关的 fx 动效 token（缓动/时长/辉光，`index.css:104-120`）。
- 耦合点：daisyUI 变量契约（选择器特异性写法 `:root:has(...)` 见 `index.css:11-14` 注释，RIP 若不用 daisyUI 需重写这一层）；foxgold 配色是问卷站品牌，值不复用。
- 借鉴方式：机制（localStorage + data-theme + 首帧内联 + 退役 id 映射 + `@theme` 语义 token 分层）整建制照搬；色值按 RIP 品牌重定；圆角四档与统一控件高度这类「语义化设计决策」直接继承。
- 工作量级：S。

### 2.4 路由懒加载与分包

- 实际路径：`admin/src/App.tsx:12-65`（30 个路由全部 `lazy` + `RouteFallback` 骨架 + `lazyElement` 包装）、`App.tsx:79-124`（`createBrowserRouter`，login 独立于壳外）、`admin/vite.config.ts:22-63`（Rolldown `codeSplitting.groups`）
- 现状：首屏只带壳 + 当前路由；vendor 分组把 react/react-dom、lucide-react、dnd-kit、qrcode、fingerprint、telegram-sdk 各自成稳定 chunk；`vite.config.ts:17-21` 注释记录了一个实测教训：react-router 不进双入口共享 vendor 组，否则公开页访客要多下载约 170KB。
- 图表懒加载：`admin/src/components/EChart.tsx:21-70` 双层 lazy（首挂载才拉库）+ `echarts/core` 按需注册（bar/pie/grid/tooltip/axisPointer/legend/双 renderer），并记录了两个坑：barrel import 会拖回全家桶（22-26 行注释）、`lib/core` 是 CJS 在 rolldown interop 下会炸（39-43 行注释）。
- 耦合点：纯工程模式，无业务耦合。
- 借鉴方式：作为 RIP 脚手架的既定决策照做；EChart 封装整体平移（见 3.9）。
- 工作量级：S（属脚手架决策，不算独立抽取任务）。

### 2.5 视觉回归方法与 a11y 门禁

- 实际路径：`playwright.config.ts`（根）、`scripts/visual-qa-server.mjs`（88 行）、`qa/visual/admin.spec.ts`（401 行）、`qa/visual/a11y.ts`、根 `package.json:18-20`
- 现状（方法要点，全部读自上述文件）：
  - 离线静态服务器：`visual-qa-server.mjs` 服务 `admin/dist`，公开页路由映射到 `survey.html`，带 SPA fallback（`scripts/visual-qa-server.mjs:38-60` 一带），API 全部由测试内 route mock 接管，不依赖真实后端。
  - 矩阵：管理端 20 条路径 × 2 主题（light/dark）× 2 视口（390x844 / 1440x900）= 80 组合，一组参数化用例跑完（`admin.spec.ts:61-73`、`309-314`）。
  - 确定性控制：`test.use({ reducedMotion: "reduce" })` + EChart 组件侧响应 `prefers-reduced-motion` 关动画，杜绝「动画第几帧」型偶发失败（`admin.spec.ts:10-17` 注释 + `EChart.tsx:96-115`）；活时钟文案用 `mask` 屏蔽（`admin.spec.ts:394-397`）；断言等待的是「数据落地后的稳定标题」而非固定 sleep（`admin.spec.ts:26-40` 注释）。
  - 质量断言：水平溢出 ≤1px、console.error 与 pageerror 零容忍（`admin.spec.ts:384-390`）；截图阈值 `maxDiffPixelRatio: 0.002`。
  - a11y 同门禁：每次截图前跑 axe AA 扫描，serious/critical 阻断、advisory 警告（`a11y.ts:13-31`）。
  - 门禁接线：`test:visual:gate` = build + fixtures + `playwright test --grep-invert @production`，联网用例打 `@production` 标签不进 push 门禁。
  - 字体确定性：`index.css:95-102` 显式 CJK 字体栈并注明是为了 CI 与本地快照一致；快照基线「只能由 CI 字体环境生成」（telegram-bot 仓库最近一条提交 a1796e8 的主题即为此事）。
- 耦合点：Telegram bridge stub（`admin.spec.ts:62-76`）与约 30 个业务 API mock 表（`admin.spec.ts:90-306`）是问卷业务；机制本身零耦合。
- 借鉴方式：整套方法（配置、静态服务器模式、矩阵组织、确定性控制、a11y 并轨）照搬；RIP 侧重写 stub 与 mock 表，页面矩阵从 10 页量级起步。
- 工作量级：M（为 RIP 建服务器 + 首批页面矩阵 + CI 接线）。

### 2.6 空/错/加载三态与数据获取模式（模式层）

- 实际路径：`admin/src/hooks.ts`（49 行）、`admin/src/components/ui.tsx:24-92`
- 现状：`useApi` 做 stale-while-revalidate（旧数据留在屏上，避免每次输入都闪骨架）、abort 竞态取消、retry、身份变更事件重载（`hooks.ts:16-44`）；页面统一走 SkeletonPanel / ErrorPanel / EmptyPanel 三态（`ui.tsx:24-92`）。这正是计划 §10.6 要求的「统一 Loading/Empty/Error/Retry」的现成参照。
- 耦合点：`useApi` 依赖业务 `api()`（见 3.5）；ErrorPanel 的 401 文案是 Telegram 语境（`ui.tsx:37-44`）。
- 借鉴方式：模式照搬，代码在二级条目 3.1/3.5 处理。
- 工作量级：并入 3.1 与 3.5。

---

## 3. 二级：解耦后复用

对齐 §2.2 第 2 条与 §10.7 第 3 条。这一级是「搬代码，先切依赖」。共同前提：以「复制 + 改造」进入 RIP 自己的仓库，两个项目独立构建、独立发布（§2.2 末段原文原则），不在 RIP 内引用 telegram-bot 路径。

### 3.1 UI primitives：`components/ui.tsx`（188 行）

- 耦合点：
  - `import { ApiError } from "../api"` 与 `STATUS_LABELS`、`SurveyStatus`（`ui.tsx:3-5`、`8-13`）：只影响 `ErrorPanel` 与 `StatusBadge` 两个组件。
  - `Modal`、`PageHeader`、`SkeletonPanel`、`EmptyPanel` 四个组件零业务依赖（仅 lucide 图标与 CSS 类）。
- 解耦做法：四个干净组件直接平移；`ErrorPanel` 把 401 文案改为注入或按状态码映射；`StatusBadge` 泛型化（label map 与 tone map 由调用方传入），不再绑定 `SurveyStatus`。
- 可访问性亮点值得保留：Modal 统一了 Escape 关闭、body 滚动锁、焦点进入面板、关闭后焦点归还、backdrop 是真 button 可键盘触达（`ui.tsx:94-148` 注释记录了此前四种弹窗实现并存的历史）。
- 工作量级：S。

### 3.2 Dialog 与 Toast：`components/Dialogs.tsx`（367 行）

- 现状：`useDialogs()` 提供 promise 化的 `confirm` / `alert` / `toast`；confirm/alert 共用单队列防止叠层（`Dialogs.tsx:53-54`）；toast 支持变体、时长、action 按钮与倒计时（`Dialogs.tsx:33-39`、`318-367`）。成熟度实测：全仓 33 个文件消费 `useDialogs`（grep 计数）。
- 工程细节（解耦时必须一起带走，都是注释里写明的事故教训）：
  - state updater 保持纯函数，promise resolve 放在 updater 外，否则 StrictMode 双调用会让 `await confirm()` 挂起（`Dialogs.tsx:104-107`）。
  - provider 卸载时对未决 dialog 兜底 resolve，否则路由切换/HMR 后调用方永久挂起（`Dialogs.tsx:118-129`）。
  - confirm 初始焦点落在取消键，Enter 不会误触危险默认（`Dialogs.tsx:227-229`）。
  - toast 用主题 token 而非 `dark:` 变体，避免「浅色应用 + 深色系统」反色（`Dialogs.tsx:330-340` 注释）。
- 耦合点：无业务依赖，只有中文默认文案（可保留，RIP 也是中文产品）与 `focus-trap` 引用。
- 解耦做法：整文件平移，几乎零改动。
- 工作量级：S。

### 3.3 焦点陷阱：`components/focus-trap.ts`（59 行）

- 零依赖纯函数，WAI-ARIA dialog pattern 的 Tab 首尾循环；用 `getClientRects` 判可见性并解释了 `offsetParent` 在 fixed 容器下的误判（`focus-trap.ts:26-32`）；刻意不用 focusin 抢回焦点并给出理由（13-14 行）。
- 耦合点：无。
- 工作量级：S（直接复制）。

### 3.4 错误边界：`components/ErrorBoundary.tsx`（78 行）

- class 组件 + `scope` 命名 + reset/reload 双出口 + PROD 下隐藏 message 与 stack（`ErrorBoundary.tsx:54-63`）；配合 Layout 按 `location.pathname` re-key 使用。
- 耦合点：无业务依赖（仅 CSS token 与 `.btn` 类）。
- 工作量级：S（直接复制）。

### 3.5 数据获取 hook：`hooks.ts` 的 `useApi`（49 行）

- 耦合点：`import { api, ApiError } from "./api"`（`hooks.ts:2`）；而 `api.ts` 顶部 import 了 Telegram 认证与问卷存储（`api.ts:1-4`：`getTelegramInitData`、`safeGet`），认证头在 `authHeaders()`（`api.ts:17-32`：`x-telegram-init-data`、`x-telegram-user-id`、`x-dev-auth-secret`）。这是「解耦后复用」清单里最典型的一个切点。
- 解耦做法：保留 `useApi` 的 SWR/取消/retry 骨架，把 fetcher 改为参数注入（RIP 传入自己的 API client）；身份变更事件按 RIP 会话模型重定义（RIP 没有 Telegram identity，可换成 token 刷新事件）。`ApiError`（status + data，`api.ts:6-15`）与非 JSON 响应防御（`api.ts:34-51` 注释记录了 HTML 错误页被当成成功响应的事故）值得照搬。
- 工作量级：S。

### 3.6 统计卡：`components/StatCard.tsx`（81 行）

- 现状：一卡三形态（div / Link / button），`aria-pressed` 与可访问名注入（`StatCard.tsx:62-80`）；视觉定义收在 CSS `.admin-stat*` 而非 Tailwind 串里，注释记录了两个页面卡片漂移的事故（4-16 行）。
- 耦合点：react-router `Link`（可改为 props 注入或保留，RIP 同用 react-router）；`.admin-stat` 系列类在 `index.css`（如 `index.css:3597` 起 `.admin-stat-grid`）。
- 解耦做法：组件平移 + 从 index.css 摘出 `.admin-stat*` 段落。
- 工作量级：S。

### 3.7 删除可撤销：`components/DeleteWithUndo.tsx`（67 行）

- 现状：确认弹窗 + 软删成功后 10 秒可撤销 toast（倒计时 + action），撤销失败降级提示去回收站（`DeleteWithUndo.tsx:28-60`）。
- 耦合点：仅 `useDialogs`；但它假设服务端有 tombstone/恢复语义（`DeleteWithUndo.tsx:14-16` 注释）。RIP 的 API 是否提供软删与恢复接口待验证（RIP 后端尚未实现对应端点，本文不臆断）。
- 工作量级：S（组件本身）；落地依赖 RIP API 具备软删恢复能力。

### 3.8 通用格式化函数：`format.ts`（43 行）

- 可复用：`formatDateTime`（`format.ts:20-27`）。
- 不复用：`STATUS_LABELS`、`QUESTION_TYPE_LABELS`、`matrixColumns` 是问卷业务（`format.ts:4-18`、`29-43`）。
- 工作量级：S（并入其他条目，不单列任务）。

### 3.9 图表封装：`components/EChart.tsx`（131 行）

- 现状见 2.4；组件本身零业务依赖，附带 `usePrefersReducedMotion` 与等高骨架 fallback。
- 耦合点：echarts 6 + echarts-for-react 3 的版本细节（esm/core 互操作），升级依赖时需重验；install 列表按 RIP 图表需求增删。
- 工作量级：S。

### 3.10 表格：无共享组件，只有模式（对 §2.2「表格」一项的现状修正）

- 实测：`grep -rn "<table" admin/src/routes/` 命中 10 处（VersionsPage:69、AuditPage:78、UsersPage:177、ReportsPage:304、SurveysPage:449、ResponsesPage:315、ResponseActivityPage:190、DashboardPage:446 与 495、LicensesPage:224），全部是手写 `<table className="tbl">`，样式集中在 `index.css:454-480` 的 `.tbl` 系列类。仓库内没有 DataTable 抽象，没有 TanStack Table 类表格库（`admin/package.json` 依赖表可证）。
- 结论：计划 §2.2 把「表格」列入解耦后复用，但实际可搬的只有 `.tbl` 样式与「列表页四态 + URL 状态 + 批量操作」的页面组织模式（见 `SurveysPage.tsx:59-101`：搜索防抖 300ms、状态/分类进 URL、翻页重置选择）。RIP 若要功能化表格（排序、列选、虚拟滚动）需自建，工作量级 M；若接受轻量模式，搬 `.tbl` + 模式即可，工作量级 S。

### 3.11 主题 token 与壳层 CSS（`src/index.css`，7638 行）

- 可复用段落（本次逐段定位过锚点）：foxgold 变量块（14-45）、`@theme` 应用 token（47-93）、fx 动效 token（104-120）、`.tbl`（454-480）、`.admin-shell` 壳层（3531 起）、`.admin-env-pill`（3758）、`.admin-search-*` 搜索面板（3807-4000 一带）、`.ui-modal`（4943 起）、`.admin-stat*`（3597 起）、`.admin-sidebar`（3201 起还有媒体查询段）。
- 耦合点：daisyUI 变量契约层；文件内混杂大量问卷业务组件样式（编辑器、场景、卡片墙等），不能整文件搬。
- 解耦做法：按上述锚点逐段摘取进 RIP 自己的 token.css 与 shell.css；daisyUI 层视 RIP 是否沿用 daisyUI 5 决定保留或重写。
- 工作量级：M（甄别成本是主体）。

---

## 4. 三级：不复用

对齐 §2.2 第 3 条。以下判定均基于本次读到的文件头部与 import 结构：

| 模块 | 实际路径 | 判定依据 |
|---|---|---|
| 问卷编辑器 | `admin/src/editor/`（含 `useSurveyEditor.ts` 771 行 op 队列模型）与 `admin/src/components/editor/`、`components/editor/next/` | 问卷领域模型 + 业务 API 直连（`useSurveyEditor.ts:1-4` import ApiError/apiSend/EditorData） |
| 业务路由页面 | `admin/src/routes/` 30 个文件 | 问卷/答卷/金币/授权/树洞/小游戏等问卷业务 |
| 业务 API 层 | `admin/src/api.ts`（1539 行） | 约 1300 行问卷/报告/授权接口封装（仅 `ApiError` 与 fetch 防御模式按 3.5 借鉴） |
| Telegram 认证与 bridge | `admin/src/telegram.ts`（83 行）、`api.ts:17-32` 认证头 | `x-telegram-init-data` 认证链路与 Mini App bridge 是 Telegram 专属 |
| 浏览器登录页 | `admin/src/routes/LoginPage.tsx`、`Layout.tsx` browserMode 横幅 | RIP 有自己的认证模型 |
| 参与者端子应用 | `admin/src/survey/` 整目录 | 公开问卷填写端，含 EmailAuth、金币、展示区等 |
| 小游戏插件 | `admin/src/game-plugins/` | 问卷小游戏业务（§2.2 注明其模式可研究但不替代 RIP 插件运行时） |
| dev 身份横幅 | `admin/src/components/TestBanner.tsx` | 模拟 Telegram user id 的本地开发设施 |
| PWA 初始化 | `admin/src/pwa.ts`（39 行） | 缓存键与问卷站部署路径约定耦合（`STALE_CACHE_PREFIX` 注释） |
| 工作区业务数据 | `workspaces.ts:79-144` 的四工作区定义 | 问卷业务 IA；模型层已列入 2.1 借鉴 |
| 部署/授权业务 | `LicensesPage`、`CoinEconomyPage`、`ControlCenterPage` 及对应 API | §2.2 点名的金币与许可证逻辑 |

---

## 5. 抽取路线建议（RIP 自建前端）

顺序对齐 §10.7 第 4、5、6 条（RIP 单独建 Web 应用、用 RIP 自己的 API/Domain Model 实现页面、逐步补视觉回归）：

1. 脚手架：Vite 8 + React 19 + Tailwind 4 +（可选）daisyUI 5；路由全 lazy + vendor 分组 + 首帧主题内联脚本（来自 2.3/2.4）。S。
2. 基础件包：focus-trap、Dialogs（confirm/alert/toast）、Modal 等 primitives、ErrorBoundary、泛化 useApi、EChart 封装。合计 S 至 M，一次性搬齐。
3. 壳层：Layout 模式重写 + RIP 版 workspaces 数据模型 + 全局搜索面板换源。M。
4. 门禁：playwright 静态服务器 + 页面矩阵 + axe 并轨 + CI gate 接线。M。
5. 页面：按 RIP 信息架构逐页实现，每页入矩阵。

边界与合规说明：

- 复制代码进 RIP 仓库属自有资产内部迁移（两个仓库均为本人项目），无第三方许可障碍；第三方 npm 依赖的逐包许可审计可参考 telegram-bot 已有的 `admin/DEPENDENCIES.md`（本次未逐包核对，待验证）。
- telegram-bot 的 admin 仍在活跃开发（最近提交 2026-11-08），复制时点是快照；RIP 侧不再回同步，两边独立演进（§2.2「独立构建、独立发布」）。

## 6. 本次实测记录

- `ls /home/emo/code/telegram-bot/admin/`、`find admin/src -type f`：目录结构确认。
- `sed -n` 读计划 §2.2（60-140 行）与 §10.7（615-700 行）：三级框架原文。
- 逐文件 Read：package.json、App.tsx、Layout.tsx、workspaces.ts、theme.ts、global-search.ts、ui.tsx、hooks.ts、ErrorBoundary.tsx、focus-trap.ts、Dialogs.tsx、DeleteWithUndo.tsx、StatCard.tsx、api.ts（头部）、telegram.ts（头部）、format.ts、main.tsx、index.html、vite.config.ts、index.css（token 段）、EChart.tsx、SurveysPage.tsx（头部）、playwright.config.ts、admin.spec.ts（全文）、a11y.ts、visual-qa-server.mjs、admin/README.md。
- `wc -l`：关键文件行数（见 §1）。
- `grep`：表格分布（10 处 `.tbl`）、useDialogs 消费面（33 文件）、index.css 类锚点、Toast 位置。
- `git -C telegram-bot log -3`：最近提交 2026-11-08。
- 未执行：`pnpm test:visual:gate` 与 `tsc --noEmit`（本任务为只读盘点，且红线禁止在宿主机装依赖；两仓库 node_modules 是否完整未验证，测试可运行性待验证）。

## 附：复用前必读（2026-11-09 补充，用户要求）

实际动手复用（Phase 5 Web Workbench 或 Web 壳层并行车道）之前，必须先查阅 telegram-bot 项目自带的更新文档，以文档为准修正本矩阵中可能过时的结论：

- `admin/README.md`：admin 前端唯一开发入口，技术栈以 package.json 实测为准（react ^19.2.8、react-router ^8.3.0、vite ^8.2.2 Rolldown、tailwind ^4.3.3、daisyui ^5.7.20、echarts ^6.1.0）。
- `admin/DEPENDENCIES.md`：逐包依赖与许可审计（2026-08-22 基线，复用前重核）。
- `docs/HANDOVER.md`：交接文档（最新 2026-11-08：第三批 AI 研发已合入 main，含举报/拉黑/投稿墙上线与遗留风险清单，1808 例单测基线）。
- `docs/UPGRADE_ROADMAP_2026-10-28.md`：升级路线图（编辑器 P2 三栏外壳 S0-S3、游戏插件协议、视觉基线纪律与门禁约定）。
- `docs/FEATURE-OVERVIEW.md`、`docs/DOC-STANDARD.md`、根目录 `AGENTS.md`、根目录 `OPEN_SOURCE_REUSE_REPORT.md`（该项目自己的开源复用研究，含 shadcn/ui 推荐结论，可参考其评估口径）。

注意事项：HANDOVER 与 UPGRADE_ROADMAP 反映的状态比本矩阵审计时点更新，编辑器外壳（editor/）相关结论以 HANDOVER 为准；复用前逐份核对，差异记回本矩阵。
