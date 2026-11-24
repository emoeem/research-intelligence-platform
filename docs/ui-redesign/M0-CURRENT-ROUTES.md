# RIP 当前工作区和路由清单

> 状态：M0 快照，依据 `web/src/workspaces.ts` 与 `web/src/App.tsx`。
> 日期：2026-11-24

## 一、工作区与子导航

| 顺序 | Workspace ID | 标签 | 已注册子导航 |
|---:|---|---|---|
| 1 | dashboard | Dashboard | 总览 |
| 2 | projects | Projects | 项目 |
| 3 | research | Research | 研究总览、任务、资产、知识上下文、验证计划、证据、覆盖矩阵 |
| 4 | knowledge | Knowledge | 知识库、文档中心 |
| 5 | tools | Tools | 工具中心 |
| 6 | plugins | Plugins | 插件中心 |
| 7 | agents | Agents | Agent Studio |
| 8 | workflows | Workflows | Workflow Studio |
| 9 | runs | Runs | 运行记录 |
| 10 | runtime | Runtime | 运行时 |
| 11 | settings | Settings | 设置、代理状态、证书状态 |
| 12 | websec | Web Security | HTTP History、Repeater |

## 二、现有深链接路由

- `/`、`/projects`、`/knowledge`、`/documents`
- `/tools`、`/tools/:toolId`
- `/runs`、`/runs/:runId`、`/traces`
- `/runtime`
- `/plugins`、`/plugins/:pluginId`
- `/research`、`/research/tasks`、`/research/tasks/:taskId`
- `/research/assets`、`/research/knowledge`、`/research/plans`
- `/research/evidence`、`/research/coverage`
- `/agents`、`/workflows`、`/settings`
- `/settings/wsw-proxy`、`/settings/wsw-cert`
- `/websec/history`、`/websec/repeater`

## 三、迁移约束

所有现有路径必须继续解析到同一业务页面。导航重构不得要求用户改变收藏夹、深链接或外部 MCP/Trace 回跳。对象详情可采用面包屑或 Session Tab，但不将每个路由访问历史永久置顶。


---

## 复核补充（2026-11-24）

本文件中的 Web Security 子导航表是早期快照，仅列出 HTTP History 与 Repeater。当前实际注册的 7 个子导航及 Proxy/证书页面归属，以 `M0-RECHECK.md` 的路由复核为准。
