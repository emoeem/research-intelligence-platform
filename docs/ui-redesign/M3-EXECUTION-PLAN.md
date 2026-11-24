# M3 执行计划与审计基线

日期：2026-11-24\
状态：CLOSED\
范围：通用专业组件，不进入 M4 页面重构。

## 1. M3 范围

按冻结的 COMPONENT-SPEC.md 收口以下能力：

1. DataTable：TanStack Table v9；排序、列宽、列可见性、列布局持久化与恢复默认、键盘可访问的行选择/行操作。
2. FilterBuilder：组合条件、字段/操作符类型约束、增加/删除/清空、可序列化条件；服务端过滤只输出契约，不擅自发请求。
3. VirtualTable：TanStack Virtual 长列表窗口化，稳定行 ID、键盘焦点/选择、滚动锚点与加载更多钩子；不得把 HTTP Body 放入列表。
4. Pagination：遵循 API 游标协议，显示已加载/总数、加载状态与下一游标；不得伪造 OFFSET/页码。
5. ResizablePanel：统一使用 react-resizable-panels v4 适配器；现有 ResizableSplitView 保持兼容并补齐持久化验证/响应式与可访问性。
6. MessageViewer/Editor：保留六视图 Viewer；统一复用现有 @uiw/react-codemirror 4.25.12（MIT）与 CodeMirror 6 依赖，区分只读投影、编辑草稿和原始捕获数据。
7. JsonViewer / DiffViewer：折叠式 JSON 树、逐行差异、深度/体积/复杂度保护；不把展示 diff 当作 Wire bytes。
8. LogViewer：虚拟化、级别/文本筛选、自动跟随尾部可控、复制单行/当前筛选结果、空/错误/离线状态。
9. State components：Loading、Success、Empty、Error、Unauthorized、Read-only、Partial Data、Offline 的统一呈现。

## 2. 实施前代码审计结论（基线）

- `components/data.tsx` 已使用 TanStack Table v9.3.1，但只有排序，没有列宽、列显隐、持久化和重置。
- `VirtualList.tsx` 已用 TanStack Virtual 3.14.24，但 `getItemKey(index)` 不稳定，缺少键盘焦点/锚点 API；History 已调用它，不能直接替换破坏游标加载。
- `ResizableSplitView.tsx` 已用 react-resizable-panels 4.24.1，History/Repeater 已接入；M2 有自己的 shell 分栏封装。M3 统一适配器需向后兼容，不做大范围移动。
- `MessageViews.tsx` 已有 Raw/Pretty/Headers/Params/Body/Hex；Hex 对文本体按 UTF-8 编码，不能宣称原始 Wire bytes。Repeater 已使用 @uiw/react-codemirror 4.25.12，当前不增加第二套编辑器依赖。
- 现有 `ui.tsx` 有 StatusBadge、SkeletonPanel、ErrorPanel、EmptyPanel，但 Unauthorized、Read-only、Partial、Offline 状态未统一封装。
- 当前 DataTable 主要用于 Research Tasks；FilterSearch/FilterSelect 在 Research 页面使用。History 是 VirtualList 的真实高频场景。
- M3 没有单独 LogViewer、DiffViewer、FilterBuilder、VirtualTable 通用组件。

## 3. 依赖门禁 / 选择

| 依赖 | 已安装版本 | License | 决策 |
|---|---:|---|---|
| @tanstack/react-table | 9.3.1 | MIT | 复用，不升级 |
| @tanstack/react-virtual | 3.14.24 | MIT | 复用；与现 VirtualList 对拍 |
| react-resizable-panels | 4.24.1 | MIT | 复用 v4 API |
| @uiw/react-codemirror | 4.25.12 | MIT | 复用现有 CodeMirror 6 集成 |
| @codemirror/lang-json | 6.0.2 | MIT | 复用现有 JSON 语言包 |
| Diff engine | — | — | 不增加依赖；实现受限的逐行差异算法并做复杂度保护 |

本阶段不添加生产依赖。原因：M3 所需的基础能力可由现有依赖实现；避免重复 CodeMirror wrapper 或引入另一套 diff 体系。

## 4. 实施批次

- M3-A：审计/Spike 与测试矩阵，冻结组件 API。
- M3-B：DataTable、FilterBuilder、VirtualTable、Pagination。已实施并接入 Research Tasks / Trace / WebSocket Frames。
- M3-C：ResizablePanel 统一 v4 适配层、min/max、折叠、恢复默认、版本化布局持久化。已实施；History 分栏有键盘/鼠标/折叠/窄屏浏览器验收。
- M3-D：MessageViewer/Editor、JsonViewer、DiffViewer。已实施并接入 Repeater / Activity Run details / Comparer。
- M3-E：LogViewer 与真实 Run 事件集成。已实施，明确不伪装 stdout/stderr 流。
- M3-F：专项 14 单测、5 项浏览器专项（10k 行、列宽拖动、五视口、三语×双主题 axe、分栏折叠/响应式）通过；全项目单测、typecheck、format、production build、完整 Playwright visual/axe gate 全部通过。
- M3-G：M3-REPORT、MASTER-PLAN、README、SESSION-HANDOFF 已收口。M3 状态：**CLOSED**。人工截图/像素级签收仍按总计划 M7 执行，不阻塞组件阶段关闭。

## 5. 验收红线

- 不改变后端 API、数据库和安全边界。
- 不把本地过滤假称服务端过滤；不把截断文本称原始字节。
- 稳定 key 使用真实业务 ID，不用数组索引作为身份。
- 大列表验证渲染窗口数量与滚动锚点；不得加载完整 Body。
- 表格列布局存储带版本、字段白名单和范围校验；坏数据回退默认。
- 键盘操作、ARIA 名称、焦点可见性和 Reduced Motion 纳入测试。
- 新组件至少有真实业务调用点或明确被现有业务组件封装使用，不交付孤立 Demo。
- 每批固定执行 format、typecheck、unit、build、真实浏览器测试；不以更新快照掩盖回归。
- 保留工作树已有未提交改动；未经明确授权不执行 reset/clean/stash/checkout/rebase/commit/push。

## 6. 当前状态

M3-A 至 M3-G 全部完成。全量自动化门禁通过，M3 于 2026-11-24 **CLOSED**。详细结果见 `M3-REPORT.md`。M7 人工截图签收与 Figma 像素复核按总计划后置，不作为组件阶段关闭条件。

## 7. 2026-11-24 当前实现快照

- 新增：`data/FilterBuilder.tsx`、`data/VirtualTable.tsx`、`data/Pagination.tsx`、`editor/MessageEditor.tsx`、`editor/JsonViewer.tsx`、`editor/DiffViewer.tsx`、`feedback/LogViewer.tsx`、`feedback/StatePanels.tsx`、`ResizablePanelGroup.tsx`。
- 改造：DataTable（TanStack Table v9 sizing/visibility/sort/selection）、VirtualList（稳定 key + 跟随尾部）、MessageViews（ARIA tabs + 原始字节 Hex）、ResizableSplitView/PersistentShellSplit（统一适配器）。
- 业务接入：Research Tasks、Trace、WebSocket Frames、Comparer、Repeater、Activity Run details、HTTP History。
- 新增测试：`tests/m3-components.test.tsx`；`qa/visual/m3-components.spec.ts`。
- 依赖：无新增生产依赖；复用当前锁定的 TanStack、react-resizable-panels v4 与 CodeMirror 6。
- 专项单测：当前专项套件全部通过；完整 Playwright visual/axe 78/78 通过。M3 已 CLOSED，允许按主计划进入 M4。


## 8. 最终门禁结果（2026-11-24）

- Vitest：21 files / 152 tests PASS。
- TypeScript：PASS。
- Prettier：PASS。
- Production build：PASS，2439 modules transformed。
- Playwright visual/axe：78/78 PASS。
- M3 10,000 行 Trace VirtualTable：窗口化通过，DOM 行数小于 40。
- M3 专项 5 Playwright：全部通过；覆盖 5 视口、3 语言 × 2 主题 axe、DataTable 键盘/鼠标列宽、ResizableSplitView 拖动/键盘/折叠/窄屏。
- axe serious/critical：本次覆盖矩阵未发现。
- `git diff --check`：PASS。
- 本轮 M3 没有新增或升级生产依赖；package.json/package-lock 当前相对 HEAD 的其他既有工作树差异未在本阶段修改。
- 单测过程有既有 jsdom Canvas/ResizeObserver 与 React act 警告，但 152 项均通过；它们不属于本轮 M3 的失败项。
- 没有更新视觉基线以掩盖失败；M7 人工截图签收与 Figma 像素复核保留。
