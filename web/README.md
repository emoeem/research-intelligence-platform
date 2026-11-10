# RIP Web Workbench（rip-web）

RIP（Research Intelligence Platform）Web 工作台前端，Phase 5 车道 B 的独立 npm 工程。布局模型与解耦 primitives 借鉴参照实现（admin，见 docs/phase0 的 UI 复用盘点文档）的已验证模式（复制改造进本仓库，不依赖其仓库整体，不复用其业务页面与其认证链路）。阶段计划见 `docs/phase5/PHASE5-PLAN.md`。

## 命令

| 命令                       | 作用                                                                       |
| -------------------------- | -------------------------------------------------------------------------- |
| `npm run dev`              | 开发服务器（`/api` 代理到 127.0.0.1:7800）                                 |
| `npm run build`            | `tsc --noEmit` + vite build                                                |
| `npm run typecheck`        | 仅类型检查                                                                 |
| `npm run test -- --run`    | vitest 单次运行（不带 `--run` 为 watch 模式）                              |
| `npm run format`           | prettier 写回                                                              |
| `npm run format:check`     | prettier 检查（格式门）                                                    |
| `npm run test:visual:gate` | build + Playwright 视觉矩阵（6 页 x 2 主题 x 2 视口 + axe AA，无快照基线） |

`/api` 代理目标默认 7800（apps/server rp_server.py），本机端口被占用时用 `RP_API_TARGET=http://127.0.0.1:<port> npm run dev` 覆盖。视觉门禁默认驱动系统 chromium（/usr/sbin/chromium），可用 `PW_CHROMIUM=<路径>` 覆盖；快照基线不本地生成（ROADMAP 风险 3），详见 docs/phase5/PHASE5-ACCEPTANCE.md。

## 技术栈（版本精确锁定，2026-11-10 实测安装）

Vite 8.3.7（Rolldown 内核）、React 19.3.1、react-router 8.5.1、TypeScript 7.0.2、Tailwind CSS 4.3.5 + daisyUI 5.7.50（仅自定义主题）、vitest 5.1.0 + @testing-library/react 16.3.3 + jsdom 31.0.0、prettier 3.9.10。已接线：@tanstack/react-query 5.107.1（Provider + useApi fetcher）、echarts 6.1.0（直连 echarts/core 的按需注册封装，无第三方绑定层）。已锁定待用：zod 4.6.7（P5-4 后的契约与表单批次用）。

## 目录结构

```text
web/
├── index.html                    # 含首帧 data-theme 内联脚本（防主题闪白）
├── src/
│   ├── main.tsx                  # 启动：主题重设 + 挂载
│   ├── App.tsx                   # 路由表：六页全部动态 import + Suspense 骨架
│   ├── workspaces.ts             # 工作区信息架构纯数据（零 import、稳定编号、buildFeatureMap）
│   ├── theme.ts                  # 主题注册表（与 index.html 内联映射保持同步）
│   ├── components/
│   │   ├── Layout.tsx            # 壳层：56px 图标轨 + tab 条 + 移动抽屉 + re-key ErrorBoundary
│   │   ├── GlobalSearchPalette.tsx  # ⌘K 面板（索引源：/api/tools|knowledge|tasks 真实端点）
│   │   ├── EChart.tsx            # 图表封装（直连 echarts/core + ResizeObserver）
│   │   ├── echarts-install.ts    # echarts 按需注册（静态具名 import 保证 tree-shake）
│   │   ├── Dialogs.tsx           # promise 化 confirm/alert/toast
│   │   ├── ui.tsx                # Modal/PageHeader/SkeletonPanel/EmptyPanel/ErrorPanel/StatusBadge
│   │   ├── ErrorBoundary.tsx     # 路由级错误边界（按 key 复位）
│   │   └── focus-trap.ts         # WAI-ARIA 焦点陷阱
│   ├── lib/
│   │   ├── api.ts                # ApiError + 非 JSON 防御 + 401 事件化 + Bearer 注入
│   │   ├── hooks.ts              # useApi（fetcher 注入、SWR、abort、retry）
│   │   ├── global-search.ts      # 搜索纯匹配模块（规范化/AND 匹配/分组排序）
│   │   └── format.ts             # formatDateTime
│   ├── styles/token.css          # 双层 token：daisyUI 变量契约 + @theme 应用层
│   ├── styles/shell.css          # 壳层样式（图标轨/tab 条/抽屉/搜索面板）
│   └── routes/                   # 六个页面（总览已接 /api/overview，其余占位待批次）
└── tests/                        # api / focus-trap / global-search / workspaces / echart / shell 冒烟
```

## 壳层

两层信息架构（数据唯一源 `src/workspaces.ts`）：桌面常驻 56px 图标轨（工作区 1 研究、2 执行），进入工作区后主区顶部出现该工作区横向 tab 条（带 `2.1` 式稳定编号，过滤不漂移）；<640px 图标轨变抽屉（滚动锁 + Esc 关闭 + 路由变化自动收起）。⌘K / Ctrl+K 开全局搜索面板，索引源是真实只读端点与 workspaces 路由，不造假索引。总览页接 `/api/overview` 真实数据（KPI 卡 + 知识生命周期柱图 + 功能地图）。

## 主题

`rip-dark`（深空，默认）与 `rip-light`（晨白）两套自定义 daisyUI 5 主题，色板按 RIP 重定义。切换链路：`localStorage["rip_theme"]` + `<html data-theme>`，index.html 内联脚本在首绘前解析，theme.ts 在启动时按同一映射重设，两处映射改主题时必须同步；图标轨底部按钮可往返切换。对比度为设计目标值，逐对实测在 P5-9 axe 门禁收口。

## 边界与纪律

- 依赖只进 `web/node_modules` 与 `package-lock.json`，不全局安装（npm 全局基线不因本工程变化）。
- 开发期数据一律走 `/api` 代理（apps/server 的 rp_server.py 九个只读端点），不直连数据库；新页面数据不得新增直连 SQL 的端点。
- 页面数据接入批次：六个页面全部接真实只读端点。总览（/api/overview + /api/stats）、知识库（/api/knowledge?q= 检索 + 详情侧栏）、工具中心（/api/tools 列表 + 过滤 + /tools/:toolId 详情，试运行为禁用占位）、运行记录（/api/runs 列表 + status 过滤 + /runs/:runId 详情含 usage/environment JSON 块与 artifact 预览入口）、运行时（/api/runtime 节点与心跳，实时心跳待 Gateway 常驻编排属待验证）、项目（/api/tasks 列表 + by_state/by_type 计数，P5-9 最小版，任务详情与时间线裁剪），Skeleton/Error/Empty 三态统一。
