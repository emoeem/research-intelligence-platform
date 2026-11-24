# M3 Report — 通用专业组件

日期：2026-11-24\
状态：**CLOSED — 全量自动化门禁通过**\
阶段边界：M3 only；未进入 M4。

## 1. 交付组件与真实调用点

| 组件 | 实现 | 真实接入 |
|---|---|---|
| DataTable | TanStack Table v9 排序、列宽拖动/键盘调整、列显隐、列宽/显隐版本化持久化、重置、行选择、语义化表头与行键盘操作 | Research Tasks |
| FilterBuilder | 类型化字段/操作符、添加/编辑/删除/清空条件、空条件不误过滤 | Research Tasks |
| VirtualTable | TanStack Virtual、稳定业务 ID、窗口化、键盘 Home/End/方向键、Enter 激活、active descendant、数据更新时按 key 保留活动行、无 ResizeObserver 初始窗口回退 | Trace |
| Pagination | 游标模式、已加载/总量、忙碌状态、无下一游标时隐藏按钮；不生成页码/OFFSET | WebSocket Frames |
| ResizablePanel | react-resizable-panels v4 单一适配器；统一 Group/Panel/Separator；最小/最大尺寸、可选折叠、键盘/鼠标调整、恢复默认、布局持久化 | HTTP History、Repeater、M2 shell split wrappers |
| MessageViewer | Raw/Pretty/Headers/Params/Body/Hex 六视图；ARIA tab/tabpanel；方向键/Home/End 切换；Hex 优先读取按需 Body API 的原始字节 | HTTP History |
| MessageEditor | 复用已有 CodeMirror 6/JSON 语言包，支持只读、JSON、保存快捷键、主题和扩展 | WSW Repeater |
| JsonViewer | 折叠树、深度/条目/体积保护、复制原 JSON、非序列化值安全失败 | Activity Run error payload |
| DiffViewer | 行级 LCS 增删/上下文、显示/隐藏未变化、输入体积和 DP 复杂度保护 | Comparer |
| LogViewer | TanStack Virtual、级别/文本过滤、可关闭的尾部跟随、复制单行/过滤结果、缓存离线提示 | Activity Run lifecycle/error events |
| StatePanels | Loading、Success、Empty、Error、Unauthorized、Read-only、Partial、Offline | Research Tasks、Comparer、Activity Run details、LogViewer |

## 2. 数据语义与边界

- Research Tasks 当前 API 没有服务端排序/高级过滤查询参数。DataTable 排序和 FilterBuilder 明确只作用于当前已加载结果，不伪称服务端查询；页面提示已标注。
- WebSocket Frames 使用现有 next cursor。Pagination 只提供下一游标，不虚构 OFFSET 或总页数。
- Trace 的 VirtualTable 对 10,000 行 API 响应只渲染可见窗口和 overscan；不将 Body 数据装入列表。
- HTTP History Hex 视图在 Body API 返回 base64 时使用解码后的原始字节；没有原始字节时只展示文本投影的 UTF-8 字节，不冒充 wire capture。
- Run Logs 展示 API 已提供的生命周期事件和 error_json；后端没有 stdout/stderr streaming endpoint，不伪装成实时终端。
- DiffViewer 是文本预览，不替代服务端比较结果，也不代表原始网络字节。
- 本阶段未修改后端 API、数据库、安全策略或权限模型。

## 3. 依赖审计

无新增生产依赖、无 package.json / lockfile 依赖升级。复用：
- TanStack Table 9.3.1
- TanStack Virtual 3.14.24
- react-resizable-panels 4.24.1
- @uiw/react-codemirror 4.25.12 / CodeMirror 6
- @codemirror/lang-json 6.0.2

新增的分栏适配器集中封装底层库，业务页面不直接导入 react-resizable-panels。

## 4. 验收覆盖

专项单元测试：`web/tests/m3-components.test.tsx`，14 项覆盖 DataTable、FilterBuilder、VirtualTable、DiffViewer、LogViewer、状态、MessageViews、MessageEditor、Pagination、JsonViewer。

专项 Playwright：`web/qa/visual/m3-components.spec.ts`，5 项覆盖：
1. 10,000 行 Trace 响应的虚拟窗口、稳定行数与 End 键滚动
2. DataTable 列宽键盘与鼠标拖动、布局持久化
3. 360 / 768 / 1024 / 1280 / 1600 五视口下 DataTable 与 VirtualTable 无页面横向溢出
4. zh-CN / en-US / ja-JP × dark / light 六种组合与 axe serious/critical 扫描
5. ResizableSplitView 鼠标拖动、键盘调整、折叠持久化与窄屏 Tabs

全量门禁已完成，详见下方最终收口记录。M3 不以专项测试通过代替全项目回归。

## 5. 最终门禁记录

- Vitest：21 files / 152 tests PASS。
- TypeScript：PASS。
- Prettier：PASS。
- Production build：PASS（2439 modules transformed）。
- Playwright visual/axe：78/78 PASS。
- axe serious/critical：本次覆盖矩阵未发现。
- 10,000 行 Trace VirtualTable：浏览器中窗口化通过，DOM row 少于 40。
- M3 专项 5 项 Playwright：全部 PASS。
- `git diff --check`：PASS。
- 本轮 M3 未新增/升级依赖；工作树中 package manifests 的既有差异未在本阶段修改。

## 6. 人工复核边界

本阶段自动化验证覆盖 10k 虚拟化与主要交互；人工截图/像素级签收仍按总计划 M7 进行。Figma 未声称同步或签收。


## 7. 阶段决定

M3 的组件实现、真实页面接入、专项验证与全项目回归均已通过。**M3 CLOSED**，允许依照 MASTER-PLAN 进入 M4。没有执行 commit、push、reset、clean、stash、checkout 或 rebase。
