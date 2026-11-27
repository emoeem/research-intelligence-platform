# RIP Unified Workbench UI Redesign

> 状态：M0-M7 在用户批准范围内 CLOSED；M8 SPA 按用户决定 staged 于 7801、默认根未切换。用户已明确 Projects/Research Assets independent、M7 reference、M8 staged、Figma existing。
> 日期：2026-11-24
> 适用仓库：`/media/emo/code/pensting/research-platform`
> 前置规范：`AGENTS.md`、`docs/DOC-STANDARD.md`、`docs/ui-ux/AI-IMPLEMENTATION-GUIDE.md`、`docs/SESSION-HANDOFF.md`

## 一、目标与原则

本计划把 RIP 从多个页面和两个前端入口组成的管理面，演进为一个统一的专业研究工作台。保留现有 React/Vite/RIP Token、业务路由、API、安全边界和真实数据契约。改造信息架构、组件组织、页面布局、可操作性和视觉一致性，不重写 Rust Core、Worker、Tool Engine、Workflow Engine 或数据库。

强制原则：

1. 设计先行。页面规格和浏览器原型完成后，用户提供的 Dashboard 参考图作为本轮 M2-M7 的视觉验收基准；Figma 像素级核对仍需独立补充，但不阻塞已获用户授权的实施。
2. 单一正式 UI。rip-web 是唯一正式用户界面。M7 的只读 API、Token 验证和服务端拒写仍然保留。
3. 成熟库优先。通用表格、虚拟滚动、分栏和代码编辑优先成熟库；不为同一能力维护自研与第三方两套实现。
4. 现有设计系统优先。复用 RIP Dark/Light、Tailwind 4、daisyUI 5、Lucide、现有基础组件。禁止引入第二套完整 UI 框架。
5. 真实数据优先。生产页面只消费真实 API；Mock 只允许出现在明确标记的设计原型和测试夹具中。
6. 设计规格是实施依据。实现不能擅自改变页面结构；如发现规格和业务契约冲突，先提出 ADR 修订并记录。
7. 视觉验收是硬门禁。构建成功不等于 UI 完成，必须有真实浏览器截图、交互测试和无障碍结果。
8. 安全边界不变。只读由后端强制，不以隐藏按钮代替授权，不削弱 Token、审计、脱敏、Scope 和 Body 加载边界。

## 二、统一入口与服务边界

目标结构：

```text
Browser
  |
  v
rip-web (正式用户 UI)
  |
  +-- Dashboard / Projects / Research / Knowledge
  +-- Tools / Plugins / Agent Studio / Workflow Studio
  +-- Runs / Runtime / Web Security / Settings
  |
  v
rp_server (API、Token、权限、限流、只读/写入策略)
  |
  +-- Rust Core / Tool Engine / Workflow Engine
  +-- Python Workers
  +-- PostgreSQL / SQLite / CAS
  +-- MCP / CLI
```

5173 是本机 Vite 开发入口，不是生产端口约束。7800 保留 API 和安全边界。当前 M7 静态页面先保留作为迁移期间的回退入口；只有 rip-web 覆盖只读功能、API 对账、权限测试、回滚演练全部通过后，才在独立收口批次退出正式入口。不得删除 rp_server 的 API 或只读逻辑。

## 三、信息架构

保留现有 12 个工作区 ID 和路由语义，不无故重编号：

1. Dashboard
2. Projects
3. Research
4. Knowledge
5. Tools
6. Plugins
7. Agents
8. Workflows
9. Runs
10. Runtime
11. Settings
12. Web Security

导航分工：

- Sidebar：全局工作区入口，分组显示。
- WorkspaceNav：当前工作区的子页面导航，只显示当前工作区 tabs。
- SessionTabs：真实打开的文档、请求草稿或编辑会话，不等于路由访问历史。
- Breadcrumb：当前页面和对象上下文。
- Inspector：当前选中领域对象的属性和关联。
- Activity Dock：后台任务、Tool Runs、日志和输出，默认收起。

## 四、统一页面模板

根据业务选择模板，不将所有页面强行做成同一种卡片页：

| 模板 | 使用场景 | 布局 |
|---|---|---|
| Dashboard | 平台概览 | 行动优先、多列统计、近期事件 |
| Data Workspace | Projects、Knowledge、Runs | 工具栏 + 数据表格 + 按需详情 |
| Research Workspace | Research | 任务上下文 + 证据/知识/覆盖关联 |
| Security Workbench | HTTP History、Proxy、Fuzzer | 全宽、可调分栏、高密度数据 |
| Editor Workspace | Repeater、Decoder | 多会话 + 编辑器/结果双栏 |
| Canvas Workspace | Workflow Studio | 画布 + Palette + Inspector + Run Panel |
| Settings Workspace | Settings | 分类导航 + 表单 + 状态说明 |

## 五、阶段计划与门禁

### M0：基线审计（已完成）

读取权威规范、代码、路由、组件、Token、依赖、服务状态和测试基线。交付见本目录 M0 文档。

### M1：设计系统与页面原型（规格与原型完成，视觉方向已确认）

交付：
- Design System、Token 映射、字体与排版规范。
- App Shell、导航、Inspector、Activity 规格。
- 成熟组件库评估和 ADR。
- 12 个工作区的页面规格。
- Dashboard、HTTP History、Repeater、Agent Studio 的浏览器原型。
- Dark/Light 和桌面/平板/手机状态的设计验收清单。

M1 门禁：页面目标、组件、布局、交互、空/载入/错误/只读状态和响应式行为已写入规格，原型可在浏览器运行。用户于 2026-11-24 提供 Dashboard 参考图并要求继续完成计划，视为本轮实现方向确认。Figma 的像素级核对仍未完成，不代表 Figma 已更新。

### M2：Design System 与 AppShell

统一 Sidebar、Topbar、WorkspaceNav、SessionTabs、Inspector、Activity Dock 和页面布局模式。保持所有现有路由可用。

### M3：通用专业组件

封装 DataTable、FilterBuilder、VirtualTable、Pagination、ResizablePanel、MessageViewer/Editor、JsonViewer、DiffViewer、LogViewer 和状态组件。只有经 Spike 验证的成熟依赖才进入 package-lock。当前实施与审计证据见 `M3-EXECUTION-PLAN.md` / `M3-REPORT.md`。M3 已于 2026-11-24 通过全量门禁并 CLOSED，可按顺序进入 M4。

### M4：基础业务页面

依次完成 Dashboard、Projects、Research、Knowledge、Tools、Plugins。逐页对照 M1 页面规格，真实 API 接入，逐页截图验收。

实施记录：`M4-REPORT.md`。Dashboard、Research、Knowledge、Tools、Plugins 已接入现有真实 API。后端当前没有 `/api/projects` 和 `/api/research/assets`，Projects 页面已停止将 task 镜像冒充 Project，Research Assets 保持明确不可用状态；这两个领域数据能力需独立后端契约，不得在 UI 层伪造。

### M5：Web Security 工作台

依次完善 History、Repeater、Proxy/MITM、Fuzzer、Intruder、WebSocket、Decoder、Comparer、Passive Audit、Plugin Center、Proxy Settings。保留 WSW Rust 引擎、API、Body 按需加载、敏感数据脱敏和 Scope 限制。

实施记录：`M5-REPORT.md`。Intruder/Fuzzer 统一复用既有 Rust FuzzSpec，不另建第二套执行引擎。

### M6：Agent、Workflow、Runs、Runtime

重构工作区外围 UI，不替换 Agent/Workflow/Tool Engine 和已有执行状态机。实现真实运行状态、Trace、日志、Artifact 与 Activity Dock 关联。

实施记录：`M6-REPORT.md`。

### M7：全站视觉与响应式收口

统一双主题、字体、间距、状态、键盘操作、断点和截图回归。完成无障碍与性能验收。

实施记录：`M7-REPORT.md`。当前已有局部三语/双主题/响应式自动化证据，但全路由矩阵、42 张变更基线人工审阅、Figma 对照和性能人工记录未完成，M7 保持 OPEN。

### M8：单一入口迁移与收口

在功能对账、只读权限对账、深链接检查、回滚演练通过后，停用 M7 独立静态用户界面入口。保留 API 服务、Token、Readonly 约束和后端数据面。更新运维和部署说明。

实施记录：`M8-ENTRY-MIGRATION-READINESS.md` 与 `M4-M8-STATUS.md`。当前无 SPA 部署/回滚配置、无真实授权 parity 演练，`rp_server` 对未知静态路径返回 404；M8 保持 BLOCKED，未经用户验收不得改 7800 根入口或下线旧 UI。

## 六、成熟依赖引入门禁

每个新依赖必须先写明：用途、现有实现缺口、官方文档、当前版本、License、Peer Dependencies、依赖树、维护状态、安全公告、Bundle 影响、最小集成测试和回滚方式。M0 不安装任何新依赖。

M1 候选基线与 npm 当前实测版本见 `M0-DEPENDENCY-AUDIT.md`。实施时重新核对版本和 API，不复制过期示例。

## 七、每批固定工作流

审计当前代码 → 核对设计规格 → 最小实施计划 → 实现 → format/typecheck/test/build → 真实浏览器逐页测试 → 截图 → 检查 Dark/Light 与窄屏 → 更新文档 → 批次报告。

任何阶段失败，修复后重新跑受影响门禁。不得通过删测试、隐藏功能、放宽断言或批量更新快照掩盖问题。

## 八、禁止事项

- 不允许未完成 M1 设计冻结就开始大规模页面重写。
- 不允许引入 Ant Design、MUI、Mantine、Chakra、Bootstrap 或另一套完整组件/主题系统。
- 不允许在页面中堆叠无意义 KPI、巨型图表、装饰网格和空白。
- 不允许全站固定 80rem 最大宽度。
- 不允许 Inspector/Activity 默认占据主工作区。
- 不允许用静态示例数据冒充生产数据。
- 不允许改变 API、数据库迁移、安全约束或领域状态而不另立任务。
- 不允许为了视觉回归随意更新所有基线。
- 未经用户明确授权，不执行 reset、clean、stash、checkout、rebase、commit、push 等 Git 操作。

## 九、当前限制与下一步

Figma 现有文件已核对页面元数据，但 Figma MCP 截图额度在本次审计时触顶，未能完成画板像素级复核。仓库 UI 指南声明的 7 个核心画板与当前可读元数据存在差异，详见 M0 基线。M1 原型先按代码内真实 RIP Token 和现有 Figma 规范构建，不声称 Figma 画板已更新。Figma 视觉复核恢复后，必须补充一次设计对照。

当前执行状态（2026-11-24）：M2 AppShell 已按用户 Dashboard 参考图收敛分组导航、全局搜索、API/Runtime 状态、语言、主题、Inspector 与 Activity Dock；M3 DataTable、虚拟列表、可调分栏和 HTTP 消息编辑能力已在页面中使用；M4-M6 保留真实业务 API/路由并进行全站文案、空态/错误态与长文案本地化；M5 的 Intruder 与 Fuzzer 统一复用现有 Rust FuzzSpec（ADR-005）；M7 已完成三语静态资源覆盖和首页桌面/手机初验，仍需最终的全路由三语、Axe、视觉门禁和性能收口；M8 仍需新旧静态 UI 功能、Bearer 权限、深链接和回滚演练对账，未授权不得停用旧入口。参考图与数据边界见 REFERENCE-001-DASHBOARD.md，多语言规范见 I18N-IMPLEMENTATION.md。Figma 像素核对仍未完成，不得写作已通过。



## 阶段状态复核（2026-11-24）

- **M0：CLOSED**。现场审计与质量门禁复测记录见 `M0-CLOSEOUT.md`。typecheck、135 tests、format、build、Playwright 71/71 均通过。42 张快照的人工视觉签收仍保留为 M7 的最终截图审核项。
- **M1：DESIGN FROZEN**。12 个工作区页面规格已独立成文；补齐 Token、Typography 和 acceptance 文档。独立浏览器原型继续与生产 API/数据路径隔离。用户提供的 Dashboard 参考图作为视觉方向；Figma 像素级对照不宣称完成，也不阻塞当前阶段实施。
- M2–M7 的生产 UI 改动已存在于当前工作树，本次不把既有实现冒充为本轮新写。后续应逐阶段审计现有实现与冻结规格的符合度，逐页补齐真实浏览器和 API 验收证据；不能仅凭源码存在或构建通过判定阶段完成。
- M8 尚未完成：需要验证统一部署入口、M7 静态 UI 迁移、深链接、Bearer 权限和回滚演练后才能退出旧静态 UI。


## 阶段状态复核（2026-11-24）

- **M0：CLOSED**。现场审计与质量门禁复测记录见 `M0-CLOSEOUT.md`。typecheck、135 tests、format、build、Playwright 71/71 均通过。42 张快照的人工视觉签收仍保留为 M7 的最终截图审核项。
- **M1：DESIGN FROZEN**。12 个工作区页面规格已独立成文；补齐 Token、Typography 和 acceptance 文档。独立浏览器原型继续与生产 API/数据路径隔离。用户提供的 Dashboard 参考图作为视觉方向；Figma 像素级对照不宣称完成，也不阻塞当前阶段实施。
- M2–M7 的生产 UI 改动已存在于当前工作树，本次不把既有实现冒充为本轮新写。后续应逐阶段审计现有实现与冻结规格的符合度，逐页补齐真实浏览器和 API 验收证据；不能仅凭源码存在或构建通过判定阶段完成。
- M8 尚未完成：需要验证统一部署入口、M7 静态 UI 迁移、深链接、Bearer 权限和回滚演练后才能退出旧静态 UI。



### M2 收口记录（2026-11-24）

M2 前端实现已完成：Inspector 接入 Run/Tool/Plugin/Task/Knowledge 真实对象上下文；Activity 的 Tool Runs、Run 事件/错误载荷、注册 Artifact 和 Runtime 状态接入现有 API；Inspector/Activity 使用 react-resizable-panels 提供鼠标拖动、键盘调整、最小尺寸与本地持久化；窄屏 Inspector 保留抽屉模式；新增三语翻译、5 视口和 8 数据状态专项浏览器验收。最终验证见 `M2-REPORT.md`。

真实后端边界继续明确：当前没有 Notifications 读取/确认接口，也没有独立 stdout/stderr 实时流 API，因此通知保持禁用，Logs 展示真实 Run 生命周期/失败事件与 error_json，不伪造终端流；Output 只展示 Run API 返回的已登记 Artifact。以上属于真实能力约束，不通过假数据或改造后端契约绕过。

本轮全量单元测试 20 files / 138 tests、typecheck、format、production build 和 Playwright 73/73 已通过；M2 五视口与八状态专项门禁通过。M2 状态：**IMPLEMENTATION COMPLETE**。后续按 M3 顺序推进，不得将本阶段验收结果外推为 M3-M8 已完成。Figma 像素级复核与人工截图签收仍按 M7 单独执行。


## M4–M8 自主推进复核（2026-11-24，最新）

用户决策：Projects=independent；Research Assets=independent；M7=reference；M8=staged；Figma=existing。四张 Figma 截图（Tools & Agents、Report Studio、Research Investigation、Workflow Builder）作为本轮桌面深色参考。

- **M4 CLOSED**：新增 additive migration 031；Rust Store/CLI；Project/Asset HTTP APIs；Project↔Task/Asset 关系；Asset Endpoint、append-only Observation、evidence-referenced Technology Profile；Projects/Research Assets 前端 CRUD/详情；zh-CN/en-US/ja-JP。真实 HTTP 自测和工作区隔离通过，primary platform.db 保持 user_version 30。详见 M4-REPORT.md。
- **M5 CLOSED**：History/Repeater/Proxy/CA/Fuzzer/WebSocket/Decoder/Comparer/Passive 工作台复用既有 API 与安全边界，详见 M5-REPORT.md。
- **M6 CLOSED**：Agent/Workflow/Runs/Trace/Runtime 真实 API 和执行状态机，详见 M6-REPORT.md。
- **M7 CLOSED — USER ACCEPTED / AUTOMATED PASS**：最新复跑 Vitest 22 files / 158 tests；Playwright 85/85；12 路由 × 3 语言 × 2 主题 × 5 视口 360/360 响应式 smoke；TypeScript/Prettier/build 通过。用户确认当前 UI 无问题；M7=reference、Figma=existing，不要求像素级 Figma parity。真实设备性能剖析为非阻塞后续项。详见 M7-REPORT.md。
- **M8 STAGED — NOT DEFAULT**：apps/server/stage_spa.sh 在 127.0.0.1:7801 提供 SPA；隔离 DB 副本；deep link fallback、API JSON 404、missing static 404、Bearer/Origin、CSP/cache/security headers 验证通过。旧 7800 目前未监听，真实部署 reverse proxy/TLS 与回滚演练仍待确认。不得提前切换默认根。详见 M8-STAGED-ENTRY.md 与 M8-ENTRY-MIGRATION-READINESS.md。
- **跨阶段门禁**：selftest_phase9_rest.sh 33 PASS / 0 FAIL；selftest_projects_assets.sh PASS；selftest_spa_entry.py 5/5 PASS；verification-engine 56 tests PASS。
- 没有 commit/push/reset/clean/stash/checkout/rebase；现有 worktree 改动保留。
