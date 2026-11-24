# M6 Report — Agent、Workflow、Runs 与 Runtime

日期：2026-11-24
状态：**工作区前端实现完成；执行语义仍由后端负责**

## 页面交付

- Agent Studio `/agents`：Agent 定义、能力绑定、版本/配置、Agent Runs、步骤轨迹与 Memory 视图，读取 `/api/agents`、`/api/agent-runs` 与关联 Trace。
- Workflow Studio `/workflows`：保留 `@xyflow/react` 画布；Palette、Canvas、Node Inspector、Run Panel、Version List；定义版本读写与 Workflow Run 操作走现有 `/api/workflows`、`/api/workflow-runs`。
- Runs `/runs` 与 `/runs/:runId`：真实状态过滤、列表、详情、Timeline/Steps、Inputs/Outputs、错误和 Artifact 能力边界。
- Trace `/traces`：按既有多 ID 关联查询真实 Trace。
- Runtime `/runtime`：节点健康、版本、endpoint、heartbeat 与 unknown/degraded/broken 状态，不把未知节点渲染为绿色健康。
- Activity Dock：沿用 M2 已收口的 Run/Runtime/Artifact API 数据，不伪造 stdout/stderr 实时流或通知。

## 状态机和安全边界

- 不替换 Agent Engine、Workflow Engine、Tool Engine 或执行状态机。
- 运行/暂停/恢复/取消/重试必须由服务端确认并刷新真实状态。
- Workflow 历史版本以只读方式回看；编辑草稿与已保存版本区分。
- Trace、Artifact 和日志仅展示 API 可提供且当前凭据可访问的数据。
- 后端缺失的 Artifact 表/流式日志能力明确显示不可用，不用模拟数据补齐。

## 验证

全量 Vitest 包含 `agents-smoke`、`workflows-smoke`、`runs-smoke`、`trace-smoke`；Workflow 测试覆盖五区画布、历史只读回看和真实 Run 受理；Runs 测试覆盖列表、详情、Runtime。全站 Playwright visual/axe gate覆盖主要 Agent/Workflow/Runs/Runtime 路由。最终全量门禁：Vitest 21 files / 149 tests；Playwright 78/78；typecheck、Prettier、production build、git diff --check 全部通过。M6 CLOSED。
