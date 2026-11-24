# RIP 通用组件与成熟库规格

## 一、现有组件必须盘点复用

当前可以复用：Layout、TabArea（需拆分职责）、InspectorPanel、ActivityPanel（需重构呈现）、DataTable、FilterSearch、FilterSelect、CountChips、StatusBadge、EmptyPanel、ErrorPanel、SkeletonPanel、PageHeader、Modal、Dialogs、TrustConfirmDialog、GlobalSearchPalette、VirtualList、MessageViews、EChart、workflow 组件族和 ErrorBoundary。

新增前必须搜索已有实现，不重复创建同名能力。

## 二、组件目标目录

```text
components/
├── shell/       Sidebar, Topbar, WorkspaceNav, SessionTabs
├── panels/      InspectorPanel, ActivityDock, SplitLayout
├── data/        DataTable, VirtualTable, FilterBuilder, Pagination
├── feedback/    EmptyState, LoadingState, ErrorState, StatusBadge
├── editor/      MessageViewer, MessageEditor, JsonViewer, DiffViewer
└── workflow/   现有 XYFlow 组件族
```

按真实复用场景拆分，不为了目录整齐而移动全部现有文件。

## 三、Table / Virtual Table

- 表格逻辑由 TanStack Table v9 管理，UI 由 RIP 组件渲染。
- 服务端分页、排序、过滤由 API 契约决定；客户端只管理交互状态。
- 列宽/可见列状态可保存，但必须支持恢复默认。
- 行选择需要键盘可访问。
- 大列表由 TanStack Virtual 承担窗口化，保留数据总数、游标加载、键盘当前项和滚动锚点。
- 不将整个 HTTP Body 放入列表数据。

## 四、SplitLayout

- 采用 react-resizable-panels v4。
- 提供统一包装组件，业务页不得直接各自调用库的低层 API。
- 面板可拖动、可键盘调整、具备 min/max、折叠和恢复默认。
- 布局持久化带版本号，读取时验证边界。
- 小屏自动转 Tabs/Drawer，不保留难以操作的窄双栏。

## 五、MessageViewer / MessageEditor

Viewer 支持 Raw、Pretty、Headers、Params、Body、Hex，后续按内容类型增加 Rendered。Editor 使用 CodeMirror 6。必须分清捕获原始数据、脱敏展示投影、编辑草稿和重新序列化发送内容，不能声称展示内容等于原始 Wire bytes。

## 六、状态组件

每个数据页面必须有 Loading、Success、Empty、Error、Unauthorized、Read-only、Partial Data、Offline 状态。空态解释下一步，但动作只能绑定已实现 API 或真实导航。

## 七、成熟库决策

依赖选择和版本以 `M0-DEPENDENCY-AUDIT.md` 为当前评估依据。实际添加依赖前执行 Spike、license/安全审查、构建与 Playwright 集成。Radix 只补足无障碍交互缺口，不取代 daisyUI。
