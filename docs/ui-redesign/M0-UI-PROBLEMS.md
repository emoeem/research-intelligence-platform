# RIP UI 问题清单与严重度

> 状态：M0 评估。
> 日期：2026-11-24
> 范围：全局 Shell、Dashboard、HTTP History、M7 前端入口、通用组件。

## 一、阻断级问题

| ID | 位置 | 问题 | 影响 | 处理阶段 |
|---|---|---|---|---|
| UI-001 | `Layout.tsx` | 将所有 workspace.tabs 扁平化后放进同一 TabArea | 工作区导航和页面标签混在一条横向导航中，模块增多后不可读 | M2 |
| UI-002 | `Layout.tsx` | Inspector、Activity 默认展开 | 页面首次打开就损失主内容面积，即便面板内容只是通用说明或占位 | M2 |
| UI-003 | `shell.css` | shell-center 全局 max-width 80rem | History、Repeater、Workflow 等宽屏工作区被限制宽度 | M2 |
| UI-004 | M7 静态页面与 rip-web | 7800 同时提供 API 与旧只读静态 UI，5173 提供主要 React UI | 用户误以为是两个并列产品，入口和功能归属不明确 | M8 |

## 二、高优先级问题

| ID | 位置 | 问题 | 影响 | 处理阶段 |
|---|---|---|---|---|
| UI-005 | InspectorPanel / Layout | Inspector 宽度固定 280px、内容多为当前页面元数据 | 无对象时仍占位；无法适应不同工作区 | M2 |
| UI-006 | ActivityPanel / Layout | Activity 底部固定占位高度，多个 tab 内容是说明文案 | 空间占用高、运行状态没有实时价值 | M2/M6 |
| UI-007 | WswHistoryPage | 列表高度固定 20rem，空记录没有完整引导 | 大屏浪费空间、窄屏不灵活 | M5 |
| UI-008 | WswHistoryPage | 详情区未选择记录时完全不渲染；Request/Response 纵向堆叠 | 页面下半部空白，选中后信息密度仍低 | M5 |
| UI-009 | WswHistoryPage | 搜索实现细节和保真术语直接展示给普通用户 | 页面文本偏开发/实现视角，不符合任务导向 | M5 |
| UI-010 | WswHistoryPage | 过滤状态定义比可见控件更完整 | 用户不能直观访问 MIME、Tags、时间范围等已存在过滤字段 | M5 |
| UI-011 | OverviewPage | KPI、图表和功能地图层级不够明确 | 图表占用过多面积，待处理事项和异常不突出 | M4 |

## 三、组件与一致性问题

- 当前 DataTable 能支持基本列配置，但没有统一的可排序/可调列宽/服务端筛选约定。
- 通用 Inspector 和 Activity 没有拖动分隔线、最小宽度和持久化尺寸约束。
- FilterSelect/Search 是基础筛选器，不是多条件 FilterBuilder。
- 当前有自研 VirtualList，尚未形成全局统一的虚拟表格抽象。
- MessageViews 已有 Raw/Pretty/Headers/Params/Body/Hex 能力，但编辑器、搜索、行号、差异视图和双栏工作区仍不完整。
- Activity 中 Run Logs、Output 等面存在占位内容；未接入的功能应显示准确的“尚未接入”，不能用空白暗示正常运行。
- 视觉门截图矩阵覆盖多数当前页面，但不覆盖全部 WSW 页面和完整真实后端流程。

## 四、设计原则修正

- 不再将所有 workspace tabs 放到一个全局标签带。
- 不再让 Inspector 和 Activity 默认抢占空间。
- 不将所有页面限制在相同最大宽度。
- 不用巨型图表和功能地图填充 Dashboard。
- 不把 HTTP 实现约束直接展示给普通使用者。
- 不删除已有业务能力、深链接、权限或后端数据契约。
