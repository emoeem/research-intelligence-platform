# M7 Report — 全站视觉、多语言、响应式与无障碍

日期：2026-11-24
状态：**自动化门禁部分完成；人工视觉/Figma 与全路由三语矩阵待签收**

## 已有实现和证据

- RIP Dark/Light 主题、统一 token、导航、页面模板、可调整分栏和窄屏顺序化面板。
- zh-CN / en-US / ja-JP 语言选择与本地持久化；`ui-resources.ts` 静态资源。
- `I18N-IMPLEMENTATION.md` 记录静态键扫描：619 个唯一字面量 key、资源表 770 个源 key、静态扫描缺项 0（动态 `translateUi(value)` 仍需浏览器检查）。
- 12 张 `m7-*-final.png` 专项截图覆盖 Dashboard、History、Fuzzer、Passive、WebSocket、Comparer、Decoder。
- 最新全量 Playwright suite 85/85 PASS：包括 12 个主要页面双主题桌面/移动视觉矩阵、axe 覆盖，以及新增的 12 路由 × 3 语言 × 2 主题 × 5 视口（360/768/1024/1280/1600px）响应式 smoke，360/360 组合通过。
- `pages.spec.ts` 对 12 个主要页面做双主题、桌面/移动截图和横向溢出检查；主要页面桌面双主题 axe。
- M3 专项测试另覆盖组件级 3 语言 × 2 主题 axe；M5 Fuzzer 截图有英文和日文专项。

## 尚未满足的 M7 验收项

- 360 组全路由三语言/双主题/五视口已完成结构与横向溢出 smoke；但尚未为这 360 组逐一生成并审阅视觉截图基线，不能将 smoke 等同于全矩阵像素验收。
- 全部空、加载、错误、未授权、只读、部分数据、断连状态的逐页截图仍不完整。
- 42 张此前已变更的 Playwright 基线仍需人工逐张审阅；自动化 pass 不能替代人工视觉签收。
- Figma 像素级对照未完成，未声称已修改或通过 Figma。
- 代表性大数据量下的交互性能人工复核尚未形成完整报告。

## 结论

本阶段已具备稳定的双主题、三语资源和响应式基础，主要页面自动化视觉/axe 有覆盖，但不将局部矩阵或已有截图外推为全站人工验收完成。M7 在全路由视觉截图矩阵、基线人工签收、Figma 对照和真实设备交互性能签收完成前保持 OPEN；360 组自动化结构/布局 smoke 与 48 组视觉/axe 组合不替代这些人工验收。


## 2026-11-24 follow-up — Projects/Assets + Figma reference decision

User explicitly chose M7=reference and Figma=existing, and supplied four desktop dark screenshots for Tools & Agents, Report Studio, Research Investigation, and Workflow Builder. These are the canonical visual references for this pass; shared navigation, navy surfaces, restrained violet actions, semantic green/amber/neutral statuses, and card/border hierarchy were preserved.

### Additional coverage

- Projects and Research Assets added to the production visual page matrix.
- Projects/Assets screenshot baselines generated for Dark/Light × desktop/mobile (four screenshots per page).
- Added Projects/Assets zh-CN/en-US/ja-JP render tests.
- Full Playwright suite now **85/85 passed**, including visual/axe checks and the all-route five-viewport responsive smoke (360 route/locale/theme/viewport combinations).
- Vitest now **22 files / 155 tests passed**; TypeScript, Prettier and production build pass.
- Full route set is currently 12 primary pages; automated screenshots cover two viewport classes. This is not the full five-viewport, three-locale screenshot matrix.

### Human review remains open

- 42 visual baseline files that were already modified in the existing worktree still require human review; automated snapshot equality only verifies against the checked-in baseline.
- Figma file frame metadata/pixel-level comparison was not completed in this pass. Canonical existing file: https://www.figma.com/design/aqB4Y2xjlTva50C6k2eSQn/RIP-%E2%80%94-Research-Intelligence-Platform?m=auto&t=Rqfew6Udowb4Zx9m-1. The four supplied screenshots are used as reference, but do not cover Projects, Research Assets, Light theme or mobile.
- Full five-viewport manual layout/interaction review and performance acceptance remain outstanding. A successful route response is not equivalent to visual sign-off.

**M7 status remains OPEN for human acceptance**, despite the automated gate passing. Do not claim pixel-perfect Figma parity.


### Local performance smoke

A production-build browser smoke on the staged entry covered 12 routes at desktop 1440×900 and mobile 390×844 (24 navigations): 24/24 HTTP 200, zero API errors, zero browser console errors after pinning the theme bootstrap script hash in CSP. Warm-cache navigation loadEventEnd median 24 ms / p95 28 ms. This is local loopback/headless smoke only, not Lighthouse/Core Web Vitals or cold-network profiling. Details: M7-PERFORMANCE-REPORT.md.

Figma access note: a new Figma session was created for the existing file, but the subsequent get_metadata request was explicitly rejected because the Figma MCP Starter tool-call limit had been reached. No node/page IDs were available, and no Figma content was modified. The four screenshots supplied directly in this conversation remain the only inspectable Figma reference material for this pass.