# RIP UI 依赖审计与成熟库选型基线

> 状态：M0 候选评估完成，未安装任何新依赖。
> 日期：2026-11-24
> 当前基线：`web/package.json` 与 `web/package-lock.json`

## 一、现有依赖，优先保留

| 能力 | 已有依赖 | 决策 |
|---|---|---|
| React UI | React 19.3.1 / React DOM | 保留 |
| 构建 | Vite 8.3.7 | 保留 |
| 类型 | TypeScript 7.0.2 | 保留 |
| 样式 | Tailwind CSS 4.3.5 | 保留 |
| 基础组件样式 | daisyUI 5.7.50 | 保留，统一包装 |
| 路由 | React Router 8.5.1 | 保留 |
| 服务端状态 | TanStack Query 5.107.1 | 保留 |
| 图表 | ECharts 6.1.0 | 保留 |
| 图标 | Lucide React 1.70.0 | 保留 |
| Workflow 画布 | @xyflow/react 12.12.1 | 保留 |
| 测试 | Vitest / Testing Library / Playwright | 保留 |
| 无障碍测试 | @axe-core/playwright 4.14.0 | 保留 |

不新增主题库，不迁移至另一套完整 UI 框架。RIP Dark/Light 继续以现有 CSS Token 为单一来源。

## 二、候选成熟库，本机 npm registry 实测

以下版本由本机 `npm view <package> version license peerDependencies --json` 在 2026-11-24 读取。安装前仍需复核，不能把当前 registry 版本永久写死为未来最新版。

| 能力 | 候选 | 当前版本 | License | 兼容信息 | 决策 |
|---|---|---:|---|---|---|
| Headless 表格 | @tanstack/react-table | 9.3.1 | MIT | React >=18 | 推荐，先 Spike |
| 虚拟滚动 | @tanstack/react-virtual | 3.14.28 | MIT | React/React DOM 支持到 19 | 推荐，先与现 VirtualList 对拍 |
| 可调整布局 | react-resizable-panels | 4.24.1 | MIT | React/React DOM 18、19；0 dependencies | 推荐，使用 v4 API |
| 编辑器 DOM View | @codemirror/view | 6.43.15 | MIT | 模块化依赖 | 推荐用于 Raw/Structured 编辑 |
| 编辑器 State | @codemirror/state | 6.7.6 | MIT | 模块化依赖 | 随 CodeMirror 组合 |
| 编辑器组合包 | codemirror | 6.0.2 | MIT | 需核对语言包组合 | 推荐作为入口评估 |
| Tooltip 基元 | @radix-ui/react-tooltip | 1.4.1 | MIT | React 16.8 到 19 | 可选，仅按缺口引入 |

## 三、采用边界

### TanStack Table v9

用于大数据表的列模型、排序、过滤、行选择、服务端状态和列宽调整。它是 headless 逻辑层，不提供 RIP 的最终视觉组件，渲染继续使用 RIP Token 和 daisyUI 类。必须使用 v9 官方文档和 v9 API，禁止照搬 v8 示例。官方文档说明 React adapter 支持 React 18 或更新版本，并可完全自定义标记和样式。

### TanStack Virtual

用于 History、Knowledge、Runs、日志和 Fuzzer Results 等长列表。现有 `VirtualList` 不能立即删除，先以相同记录数、动态尺寸、滚动定位、键盘选择和游标分页做对拍。只有在接口封装和性能回归均通过后才替换。

### react-resizable-panels v4

用于 HTTP History 的请求/响应和列表/详情分割、Repeater 编辑器、Agent Studio、Inspector 与 Activity Dock。版本 4.24.1 为当前候选。必须按 v4 的 Grid/Cell/Separator 等 API 写适配层，禁止照搬 v2 的 PanelGroup/Panel/PanelResizeHandle 示例。持久化布局必须加 schema 版本与范围校验。

### CodeMirror 6

用于可编辑 HTTP Request、Raw/JSON/HTML/XML 与较大响应正文。采用模块化扩展，语言包按实际页面加载。Read-only Message Viewer 仍可保留轻量文本/JSON 组件，不要求所有文字预览都加载编辑器。保留原始字节、脱敏文本投影和重新序列化请求之间的差异标记。

### Radix Primitives

仅在 Tooltip、Popover、Menu 等交互中，现有 daisyUI/原生组件无法满足键盘、焦点和 ARIA 要求时按单个 primitive 引入。不得安装 Radix 全套后再建立第二套视觉体系。

## 四、明确不引入

- Ant Design、MUI、Mantine、Chakra UI、Bootstrap。
- 全量 shadcn/ui 迁移。
- Monaco Editor。
- 另一套 CSS/Theme 系统。
- 没有数据契约和用户场景的图表/动画依赖。

## 五、实施前必做 Spike

1. 建一个不进入生产路由的临时样例，验证 React 19、TS 7、Vite 8 和 Tailwind 4 编译。
2. 验证 Table v9 服务端排序/过滤与当前 API 游标逻辑兼容。
3. 验证 Virtual 与现 VirtualList 的键盘、滚动、游标触底一致。
4. 验证 Panels v4 在 1440、1280、1024、390 宽度下的最小宽度与键盘分割线。
5. 验证 CodeMirror 6 与 RIP Dark/Light、CJK、超大请求体、只读模式、脱敏展示和编辑历史。
6. 检查 license、依赖树、bundle 大小、安全公告和 lockfile。
7. 全部 Spike 通过，才允许安装到正式 dependencies。

## 六、来源

- TanStack Table 官方 React 文档：https://tanstack.com/table/latest/docs/framework/react
- TanStack Table 安装与 React 版本要求：https://tanstack.com/table/latest/docs/installation
- TanStack Virtual 安装文档：https://tanstack.com/virtual/latest/docs/installation
- react-resizable-panels：https://www.npmjs.com/package/react-resizable-panels
- CodeMirror View：https://www.npmjs.com/package/@codemirror/view
- Radix Tooltip：https://www.npmjs.com/package/@radix-ui/react-tooltip
