# RIP Navigation 与 Session 规格

## 一、三级概念

| 层级 | 对象 | 内容 | 行为 |
|---|---|---|---|
| 1 | Global Sidebar | 12 个工作区分组 | 切换产品模块 |
| 2 | WorkspaceNav | 当前 Workspace.tabs | 切换该工作区页面 |
| 3 | SessionTabs | 用户打开的请求、文档、编辑会话 | 切换/关闭工作会话 |

面包屑负责对象位置，不承担重复导航。全局标签栏不得再平铺所有 Workspace.tabs。

## 二、Sidebar 分组

- Overview：Dashboard。
- Research：Projects、Research。
- Knowledge：Knowledge。
- Capabilities：Tools、Plugins。
- Automation：Agents、Workflows。
- Operations：Runs、Runtime、Web Security。
- System：Settings。

工作区 ID、route 和稳定编号继续以 `workspaces.ts` 为唯一来源，分组只增加呈现元数据，不复制路由表。

## 三、WorkspaceNav

进入某个工作区后，仅显示当前 workspace.tabs。例如 Web Security 显示 HTTP History 和 Repeater；Research 显示研究总览、任务、资产、知识上下文、验证计划、证据、覆盖矩阵。对没有子页面的工作区，可省略导航带，避免只有一个 tab 时浪费高度。

## 四、SessionTabs

- 只为真实会话对象创建，例如 Repeater 请求、打开的文档、长期编辑的 Agent/Workflow 草稿。
- 不能把最近访问的 12 个工作区固定显示为 SessionTabs。
- 允许固定、关闭、恢复最近关闭项时，状态必须可持久化且有 schema version。
- Session Tab 标题来自对象数据，不使用虚假的占位名称。
- Repeater 内部请求 Tab 属于 Repeater 自己的模型，不与全局路由 Tabs 混用。

## 五、键盘与可访问性

- 全局搜索继续支持 Ctrl+K / Meta+K。
- Sidebar 支持 Tab、Enter、方向键合理导航。
- 当前路由使用 aria-current。
- 所有 IconButton 提供 aria-label 和 tooltip。
- 不使用纯颜色表示当前 Workspace。
