# ADR-003：导航层级与可调面板

状态：M1 设计裁决候选。
日期：2026-11-24

## 决策

Global Sidebar、WorkspaceNav、SessionTabs 三层职责分离。WorkspaceNav 只显示当前 workspace.tabs，SessionTabs 只承载真实的多文档/编辑会话。Inspector 仅在对象上下文有价值时展开，Activity Dock 默认收起。专业页面可用 Resizable Panels，但所有面板的最小宽度、折叠和小屏转换由 RIP 包装组件统一控制。

## 不能做

- 不得把所有工作区 tab 平铺到全局横向栏。
- 不得把路由访问历史当作 Session Tabs。
- 不得默认打开空 Inspector 或空 Activity。
- 不得把 Repeater 内部请求 Tab 与全局导航 Tab 混为一谈。
