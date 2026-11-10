# RIP Web Workbench（rip-web）

RIP（Research Intelligence Platform）Web 工作台前端，Phase 5 车道 B 的独立 npm 工程。布局模型与解耦 primitives 借鉴参照实现（admin，见 docs/phase0 的 UI 复用盘点文档）的已验证模式（复制改造进本仓库，不依赖其仓库整体，不复用其业务页面与其认证链路）。阶段计划见 `docs/phase5/PHASE5-PLAN.md`。

## 命令

| 命令                    | 作用                                          |
| ----------------------- | --------------------------------------------- |
| `npm run dev`           | 开发服务器（`/api` 代理到 127.0.0.1:7800）    |
| `npm run build`         | `tsc --noEmit` + vite build                   |
| `npm run typecheck`     | 仅类型检查                                    |
| `npm run test -- --run` | vitest 单次运行（不带 `--run` 为 watch 模式） |
| `npm run format`        | prettier 写回                                 |
| `npm run format:check`  | prettier 检查（格式门）                       |

## 技术栈（版本精确锁定，2026-11-10 实测安装）

Vite 8.3.7（Rolldown 内核）、React 19.3.1、react-router 8.5.1、TypeScript 7.0.2、Tailwind CSS 4.3.5 + daisyUI 5.7.50（仅自定义主题）、vitest 5.1.0 + @testing-library/react 16.3.3 + jsdom 31.0.0、prettier 3.9.10。已锁定待用：@tanstack/react-query 5.107.1（Provider 已接线）、zod 4.6.7（P5-4/P5-5 用）、echarts 6.1.0（P5-2 封装，按需动态加载，不进首屏）。

## 目录结构

```text
web/
├── index.html              # 含首帧 data-theme 内联脚本（防主题闪白）
├── src/
│   ├── main.tsx            # 启动：主题重设 + 挂载
│   ├── App.tsx             # 路由表：六页全部动态 import + Suspense 骨架
│   ├── AppShell.tsx        # 过渡壳层（P5-3 重写为完整 Layout 模型）
│   ├── theme.ts            # 主题注册表（与 index.html 内联映射保持同步）
│   ├── nav.ts              # 主导航纯数据（零组件 import）
│   ├── styles/token.css    # 双层 token：daisyUI 变量契约 + @theme 应用层
│   └── routes/             # RouteFallback、PagePlaceholder 与六个页面
└── tests/app-smoke.test.tsx
```

## 主题

`rip-dark`（深空，默认）与 `rip-light`（晨白）两套自定义 daisyUI 5 主题，色板按 RIP 重定义。切换链路：`localStorage["rip_theme"]` + `<html data-theme>`，index.html 内联脚本在首绘前解析，theme.ts 在启动时按同一映射重设，两处映射改主题时必须同步。对比度为设计目标值，逐对实测在 P5-9 axe 门禁收口。

## 边界与纪律

- 依赖只进 `web/node_modules` 与 `package-lock.json`，不全局安装（npm 全局基线不因本工程变化）。
- 开发期数据一律走 `/api` 代理（apps/server 的 rp_server.py 四只读端点），不直连数据库；新页面数据不得新增直连 SQL 的端点。
- 六页当前为脚手架占位，数据接入批次见各页标注（P5-5 到 P5-8）。
