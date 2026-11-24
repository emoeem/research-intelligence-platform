# RIP UI 重构基线审计

> 快照性质：反映 2026-11-24 本机目标仓库实测状态。
> 状态：M0 完成。
> 目标仓库：`/media/emo/code/pensting/research-platform`

## 一、范围与方法

检查 `AGENTS.md`、`docs/SESSION-HANDOFF.md`、`docs/DOC-STANDARD.md`、`docs/ui-ux/AI-IMPLEMENTATION-GUIDE.md)、WSW 设计计划和组件审计；检查 `web/package.json)、路由、工作区注册、Layout、Theme、Token、Inspector、Activity、History、MessageViews、视觉测试配置；检查本地服务端口、Git 状态和运行测试。

## 二、Git 与服务基线

| 项目 | 实测 |
|---|---|
| 仓库 | `/media/emo/code/pensting/research-platform` |
| 分支 | `main` |
| HEAD | `0f95c926` |
| 远端 | `origin/main` |
| 初始工作树 | 干净，`## main...origin/main` |
| Vite | `127.0.0.1:5173` 正在监听 |
| rp_server | `127.0.0.1:7800` 正在监听 |
| PostgreSQL | `127.0.0.1:5432` 正在监听 |
| Node | `v24.22.3` |
| npm | `12.2.3` |
| Chromium | `158.0.8127.36` |

父目录 `/media/emo/code/pensting` 有其他未提交变更。本次工作严格限定在目标 Git 仓库，不修改父目录的文件或 Git 状态。

## 三、质量门禁实测

| 门禁 | 结果 | 证据 |
|---|---|---|
| `npm --prefix web run typecheck` | PASS | exit 0 |
| `npm --prefix web run test` | PASS | 19 test files，126 tests passed |
| `npm --prefix web run format:check` | FAIL | 14 个既有文件格式不符合 Prettier |
| `npm --prefix web run test:visual:gate` | PASS | 71 tests passed，约 1.1 分钟 |

Vitest 过程中 EChart 测试打印 `ResizeObserver is not defined` 的 stderr 警告，但 126 个测试全部通过。该警告作为 M0 观察项登记，不改变门禁通过结果。

Prettier 失败文件：
- `qa/visual/pages.spec.ts`
- `scripts/rip3_p5_9b_browser_e2e.mjs`
- `scripts/rip3_p5_9c_browser_e2e.mjs`
- `scripts/rip3_wsw2_browser_e2e.mjs`
- `scripts/rip3_wsw3_browser_e2e.mjs`
- `src/components/MessageViews.tsx`
- `src/components/workflow/RunPanel.tsx`
- `src/routes/AgentsPage.tsx`
- `src/routes/research/ResearchCoveragePage.tsx`
- `src/routes/research/ResearchEvidencePage.tsx`
- `src/routes/WswCertPage.tsx`
- `src/routes/WswHistoryPage.tsx`
- `src/routes/WswProxyPage.tsx`
- `src/routes/WswRepeaterPage.tsx`

此次不对上述文件批量执行 Prettier，以免把既有格式差异混入 UI 设计阶段。后续只格式化本批实际修改文件，另立格式清理批次处理存量问题。

## 四、现有前端基线

`web/` 当前为 React 19.3.1、Vite 8.3.7、TypeScript 7.0.2、Tailwind CSS 4.3.5、daisyUI 5.7.50。现有 Theme 为 `rip-dark` 和 `rip-light`，已有 Layout、TabArea、InspectorPanel、ActivityPanel、DataTable、FilterSearch、StatusBadge、EmptyPanel、ErrorPanel、SkeletonPanel、VirtualList、MessageViews、EChart、Workflow Studio 组件族和全局搜索。

当前工作区注册有 12 个 ID：Dashboard、Projects、Research、Knowledge、Tools、Plugins、Agents、Workflows、Runs、Runtime、Settings、Web Security。App 路由涵盖 Research 子页面、Tool/Plugin 详情、Runs 详情、Trace、代理/证书状态、HTTP History 和 Repeater。

## 五、Figma 基线与差异

仓库 `docs/ui-ux/AI-IMPLEMENTATION-GUIDE.md` 记录 Figma 文件：
`https://www.figma.com/design/aqB4Y2xjlTva50C6k2eSQn`

本次 Figma 读取到 3 个页面：
- `01 · Overview`：1 个顶层画板 `RIP · Overview Dashboard`，节点 `2:2`，1440×960，159 个子节点。
- `00 · Design System`：元数据返回 0 个顶层画板。
- `02 · Product Screens`：元数据返回 0 个顶层画板。

仓库指南声称有 7 个核心画板，但元数据当前只确认 Overview 一个顶层画板。Figma MCP 的截图调用受 Starter 计划额度限制，本次未能取得设计画板截图。此处属于明确的 M1 限制，不能宣称已完成像素级 Figma 对照或已修改 Figma。

M1 暂以 `web/src/styles/token.css`、`web/src/theme.ts` 和仓库 UI 指南作为工程依据。恢复 Figma 截图额度后，必须补做视觉核对并记录差异，不创建新的平行 Figma 文件。

## 六、未修改项

M0 仅做审计和测试，没有修改生产源码、API、数据库、迁移、依赖锁文件、运行中服务或 Git 历史。


---

## 复核状态（2026-11-24）

本文件是先前现场快照，不能再作为当前 Git 与测试状态的唯一依据。当前工作树、服务入口、依赖实际使用、31 条路由、最新测试结果及问题编号，以同目录的 `M0-RECHECK.md` 为准。尤其本文件中“工作树干净、126 个测试通过、视觉门禁 71/71 通过”的记录已经过期。
