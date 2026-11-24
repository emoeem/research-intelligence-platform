# M0 复核报告（2026-11-24）

> 本文件是对 `M0-BASELINE.md` 的现场复核补充。由于仓库状态和测试结果已发生变化，本报告中的复核结果优先于旧快照。
> 阶段：M0（代码基线与架构审计）
> 状态：审计已完成；M0 验收仍有阻断项
> 项目根目录：`/media/emo/code/pensting/research-platform`

## 1. Git 与工作区基线

| 项目 | 现场结果 |
|---|---|
| Git 根目录 | `/media/emo/code/pensting/research-platform` |
| 分支 | `main` |
| HEAD | `0f95c926` |
| 当前工作树 | 99 个已修改文件、10 个未跟踪项目 |
| 修改文件构成 | 42 个视觉基线 PNG、57 个源码/配置/文档文件 |
| 目标仓库外层 | `/media/emo/code/pensting`，不属于本次项目修改范围 |

旧版 `M0-BASELINE.md` 记录的“初始工作树干净、126 个单测通过、视觉门禁 71/71 通过”已不再代表当前现场。当前复核必须以本报告为准。

本次没有执行 Git reset、clean、stash、checkout、rebase、commit 或 push。没有修改业务源码、API、数据库或迁移。

复核期间观察到另一个进程执行了 `playwright test ... --update-snapshots`，并重写了 42 张视觉基线 PNG。该快照更新不是本次审计发起的操作；没有回滚或覆盖这些文件。快照是否接受仍需人工视觉审核，不能把“更新后测试通过”当作视觉验收。

## 2. 服务与入口实测

| 地址 | 实测 | 页面身份 |
|---|---|---|
| `127.0.0.1:5173/` | HTTP 200 | React RIP（Vite） |
| `127.0.0.1:7800/` | HTTP 200 | 旧版 M7 静态只读工作台 Beta |
| `5173/api/stats` 无 Bearer | HTTP 401 | 鉴权生效 |
| `7800/api/stats` 无 Bearer | HTTP 401 | 鉴权生效 |
| 两个端口携带有效 Bearer 的 `/api/stats` | HTTP 200 | API 可访问 |

旧版静态页面仍位于 `apps/server/static/{index.html,app.js,style.css,login.html}`，服务端静态目录由 `apps/server/rp_server.py` 中的 `STATIC_DIR` 指向。当前确实存在两个可访问的 Web 页面入口，统一入口尚未完成。

鉴权探测只输出 HTTP 状态，没有输出或记录 Bearer Token。以上仅验证 GET 鉴权，不代表已完成所有写接口的权限审计。

## 3. 工作区与路由盘点

- `web/src/workspaces.ts` 注册 12 个稳定工作区：Dashboard、Projects、Research、Knowledge、Tools、Plugins、Agents、Workflows、Runs、Runtime、Settings、Web Security。
- `web/src/App.tsx` 当前有 31 条显式 Route path 声明，深链接仍由 React Router 承载。
- Web Security 当前注册 7 个子导航：HTTP History、Repeater、Passive Audit、WebSocket Frames、Fuzzer Jobs、Decoder、Comparer。
- Proxy 和证书页面仍使用 `/settings/wsw-proxy`、`/settings/wsw-cert`，并未归入 Web Security 子导航。
- 当前没有独立的 `/websec/intruder` 或 `/websec/plugins` 路由；需要在后续设计冻结阶段明确 Intruder Jobs、Plugin Center 是独立页面、既有功能映射，还是不适用项，不能直接判定后端功能不存在。

现有 `M0-CURRENT-ROUTES.md` 仍只列出 History 与 Repeater 两个 Web Security 子页面，已落后于当前代码，应在 M0 文档收口时同步修订。

## 4. 前端依赖与实际使用

当前锁定并安装的主要版本：

| 依赖 | 锁定版本 | 现场使用情况 |
|---|---:|---|
| React / React DOM | 19.3.1 | 使用中 |
| Vite | 8.3.7 | 使用中 |
| TypeScript | 7.0.2 | 使用中 |
| Tailwind CSS / daisyUI | 4.3.5 / 5.7.50 | 使用中 |
| React Router | 8.5.1 | 使用中 |
| TanStack Query | 5.107.1 | 使用中 |
| TanStack Table | 9.3.1 | 已安装，当前源码未发现实际使用 |
| TanStack Virtual | 3.14.24 | `VirtualList` 使用 `useVirtualizer` |
| react-resizable-panels | 4.24.1 | `ResizableSplitView` 使用 v4 的 `Group/Panel/Separator` |
| @uiw/react-codemirror | 4.25.12 | Repeater 使用 |
| @codemirror/lang-json | 6.0.2 | 当前源码未发现实际使用 |
| i18next / react-i18next | 27.2.6 / 18.0.3 | 使用中，支持 zh-CN、en-US、ja-JP |
| ECharts | 6.1.0 | 使用中 |
| XYFlow | 12.12.1 | Workflow Studio 使用 |
| Lucide React | 1.70.0 | 使用中 |

当前 `web/src/components/data.tsx` 是项目自有表格组件；源码中未发现 `useReactTable` 或 `createColumnHelper` 的使用。HTTP History 也未使用 TanStack Table 或 TanStack Virtual。后续不能仅凭 package.json 中存在依赖就认定复杂表格与虚拟化已经完成。

Repeater 页面已经接入 CodeMirror，但 JSON 语言包当前未见调用。新增依赖的许可证、维护状态和 React 19 兼容性仍需在依赖审计中保留证据。

## 5. 当前质量门禁复测

在当前快照更新完成后重新运行：

| 门禁 | 结果 | 说明 |
|---|---|---|
| `npm run typecheck` | PASS | exit 0 |
| `npm test` | PASS | 20 个测试文件，135/135 通过 |
| `npm run format:check` | FAIL | 5 个文件格式不符合 Prettier |
| `npm run build` | PASS | Vite 构建成功，2423 modules transformed |
| `npm run test:visual:gate` | FAIL | 46/71 通过，25 项失败 |

Prettier 未通过的文件：

- `scripts/rip3_p5_9b_browser_e2e.mjs`
- `scripts/rip3_p5_9c_browser_e2e.mjs`
- `scripts/rip3_wsw2_browser_e2e.mjs`
- `scripts/rip3_wsw3_browser_e2e.mjs`
- `src/components/Layout.tsx`

视觉门禁失败原因：

1. 24 项 axe 检查均报告同一个 serious 级问题：`.rail-profile` 的普通 `div` 使用了 `aria-label`，但没有有效 role（`aria-prohibited-attr`，WCAG 4.1.2）。这不是 24 个独立根因，而是同一壳层缺陷在不同页面和主题下重复出现。
2. 1 项 Agents 桌面深色截图测试仍存在约 4% 像素差异。

其余页面截图矩阵与 Workflow 的三项布局截图在本轮快照基线下通过。该结果不等于全量视觉验收：当前 Playwright 页面矩阵没有覆盖所有 Web Security 子页面，也没有覆盖计划规定的全部五种视口和全部八种数据状态。

Vitest 有 ECharts 的 jsdom Canvas/ResizeObserver 警告，以及部分 React act(...) 警告，但 135 个测试均通过。

## 6. 浏览器截图现场观察

现有设计截图目录中已有 Dashboard、HTTP History 等图片。现场检查：

- Dashboard 设计截图具有统一 Sidebar、顶部操作栏和密集信息区，但它展示的顶部 Runs、Runner、Notifications 控件与当前 Layout 源码/浏览器 DOM 不完全一致，不能作为当前实现已通过的证据。
- HTTP History 截图及当前源码存在面向用户的内部实现术语，包括“025 单行纪律”“列表零 body 字节”“8MiB”“游标分页”“过滤指纹守卫”等。这违反计划中禁止把内部实现说明展示给最终用户的要求。
- HTTP History 的空状态可见，但当前截图没有证明真实流量列表、虚拟滚动、请求/响应双栏、Body 按需加载和 Repeater 发送流程已全部通过真实浏览器验收。

截图路径（仓库相对路径）：

- `docs/ui-redesign/screenshots/m7-dashboard-desktop-final.png`
- `docs/ui-redesign/screenshots/m7-history-desktop-final.png`

## 7. M0 问题编号

| ID | 严重度 | 问题 | M0 结论 |
|---|---|---|---|
| M0-001 | Blocker | 旧 M0 基线文档声称工作树干净、测试全部通过，与当前现场不符 | 需要以本报告修正基线 |
| M0-002 | Blocker | 5173 与 7800 均提供可访问 Web 页面，旧 M7 静态入口仍在 | 统一入口尚未完成 |
| M0-003 | High | `.rail-profile` 的 ARIA 属性导致 24 项 axe serious 失败 | 需修复后重跑 axe |
| M0-004 | High | Agents 桌面深色视觉截图仍有差异 | 需人工比较 actual/diff，不得盲目更新快照 |
| M0-005 | High | HTTP History 向用户展示内部实现术语 | 需改为产品语言 |
| M0-006 | High | TanStack Table 已安装但未使用；History 未接入 Table/Virtual | 不能宣称复杂表格和虚拟化已完成 |
| M0-007 | Medium | Web Security 仅 7 个导航项，Proxy/证书仍在 Settings，缺少独立 Intruder/Plugin 路由映射 | 需在设计阶段逐项确认功能归属 |
| M0-008 | High | 当前 AppShell 顶栏缺少计划要求的 Runs、Runner、Notifications 全局操作 | 与冻结目标结构不一致 |
| M0-009 | Medium | `@codemirror/lang-json` 已安装但未发现使用 | 依赖应按实际功能核实 |
| M0-010 | Medium | 当前视觉矩阵未覆盖所有 Web Security 页面、五种视口和八种数据状态 | 视觉验收范围不足 |
| M0-011 | Medium | 42 张快照已被更新但尚未完成逐张人工审核 | 不得直接视为已验收 |
| M0-012 | Medium | Prettier 检查仍有 5 个文件失败 | 后续按修改批次格式化，不做全仓库无差别写入 |

## 8. M0 结论与阶段边界

M0 的代码、路由、依赖、服务入口、Git 状态和测试现状已完成现场盘点。现有 UI 已经包含大量预先实施的改造，不应把这些工作树修改当成干净基线，也不应在没有核实设计冻结证据的情况下跳到 M2 或继续扩大页面编码。

M0 结论：**审计盘点完成，但项目整体质量门禁未通过，M0 验收状态为有阻断项。**

下一步仍应停留在 M0 收口：确认 42 张快照变更的来源和接受范围，更新过时的 M0 路由/问题文档，并保留所有既有未提交修改。只有 M0 基线得到确认后，才能进入 M1 设计规范和原型冻结。

本报告不授权自动修复上述问题，不授权清理快照、不授权修改后端/数据库，也不授权 Git 提交或推送。



## 9. 复测与收口补充（2026-11-24）

后续复测已完成，旧表中的部分状态应以本节为准：

| 门禁 | 最新结果 |
|---|---|
| TypeScript | PASS，exit 0 |
| Unit | PASS，20 files / 135 tests |
| Prettier | PASS，已格式化本轮检查指出的 8 个文件 |
| Production build | PASS，2423 modules transformed |
| Playwright visual gate | PASS，71/71 |
| axe serious/critical | PASS，之前 `.rail-profile` 的重复 ARIA 错误已不再出现 |
| git diff --check | PASS |

Vitest 仍有 jsdom Canvas/ResizeObserver 与 React act 警告，但不影响当前测试通过。42 张基线图片是在此前已运行的 snapshot-update 进程中产生修改，自动化复测通过并不替代人工视觉审批。

M0 已作为代码基线与架构审计阶段收口。更新后的正式结论见 `M0-CLOSEOUT.md`。下一阶段 M1 的设计规格补齐情况见 `M1-PROTOTYPE-ACCEPTANCE.md` 和各独立页面规格。
