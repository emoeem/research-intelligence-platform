# RIP3 Phase 5 补充执行计划：Web 工作台与知识驱动漏洞验证

> 文档类型：Phase 5 补充任务书 / AI Agent 执行规格
>
> 适用仓库：`/media/emo/code/pensting/research-platform`
>
> 上位规范：`docs/rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md`（R2 修订版）
>
> 当前阶段：Phase 4，P4-1、P4-2、P4-3 已收口，P4-4 Benchmark 尚待完成
>
> 本文件状态：**设计与实施要求，尚不代表功能已经实现或验收通过**
>
> 重要声明：本文件只细化 Phase 5 的 Web 工作台和安全研究执行面，不取代 RIP3 R2 总计划，不改变 Phase 0 到 Phase 8 的先后顺序，不创建第二套平台。

---

## 0. 给接手 AI 的第一句话

你接手的不是一个空白项目，也不是要求你重新搭建一个 Yakit。

你需要在现有 RIP3 项目基础上，补齐一个真正可用的 **Research Workspace + Knowledge-driven Vulnerability Verification Engine**。平台的核心差异是：它能从项目自身积累的知识中筛选适用的检测方法，组织检测任务，调用成熟工具或已有 POC，判断证据，抑制重复测试，并把研究结论通过既有 RTIS 接口回流。

你必须先读取本文件列出的上位规范，再检查当前真实代码和数据库迁移状态。不能仅凭本文件或 README 猜测某个功能是否已经存在。

**先完成 P4-4，再进入 Phase 5。不得为了 UI 重构或扫描引擎提前打断 Phase 4。**

---

## 1. 当前真实基线与文档关系

### 1.1 用户确认的 RIP3 进度

| 阶段                     | 用户确认状态 | 备注                                         |
| ------------------------ | ------------ | -------------------------------------------- |
| Phase 0 架构审计与四冻结 | ✅ 完成      | afe6f5b，用户评审通过                        |
| Phase 1 文档域与存储     | ✅ 完成      | P1-1、P1-2                                   |
| Phase 2 文档智能         | ✅ 完成      | P2-1、P2-2                                   |
| Phase 3 知识治理         | ✅ 完成      | P3-1 至 P3-4，D1-D5 去重、审核双闸、十型关系 |
| Phase 4 混合检索         | 🔄 进行中    | P4-1、P4-2、P4-3 已完成，剩余 P4-4 Benchmark |
| Phase 5 工作流与安全研究 | ⬜ 未开始    | 本文件的主要实施范围                         |
| Phase 6 报告智能         | ⬜ 未开始    | 不提前实现                                   |
| Phase 7 知识回流         | ⬜ 未开始    | 不提前实现                                   |
| Phase 8 规模测试         | ⬜ 未开始    | 不提前实现                                   |

本地资料交叉核验结果：

- `docs/rip3/batches/P4-3-BATCH-REPORT.md` 记录了 P4-3 的 rerank、tokenizers、缓存和 selftest 结果。
- `docs/HANDOVER-RIP3-CONTINUATION.md` 将 P4-4 列为 Phase 4 收口批次。
- `web/src/routes/ResearchPage.tsx` 当前仍使用 `WorkspacePlaceholder`，这是 Research 工作区需要优先补齐的已知缺口。
- Web 前端为独立 npm 工程，当前 package 基线是 React 19、Vite 8、TypeScript 7、Tailwind CSS 4、daisyUI 5、TanStack Query 5、React Router 8、ECharts 6、XYFlow 12。
- 既有 Web 壳层、主题、路由、API 封装、Inspector、Activity Panel、全局搜索和 Workflow Studio 组件应先审计复用，不得直接推倒重写。

### 1.2 本文件与上位文档的优先级

出现冲突时按以下顺序处理：

1. 用户在当前会话明确提供的 Phase 0 至 Phase 8 进度与当前目标。
2. `docs/rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md` 的正式架构边界、RTIS 映射、安全红线和实施顺序。
3. `docs/HANDOVER-RIP3-CONTINUATION.md` 的当前批次交接记录。
4. 本文件的 Phase 5 页面细节、检测引擎细化和验收扩展。
5. 其他历史报告、旧 UI 设计稿和 README。

本文件是补充规格，不允许用它重定义已冻结的领域模型、Document IR、存储策略、安全策略或 RTIS 映射。

### 1.3 开工前必须阅读

按顺序读取：

1. `AGENTS.md`
2. `docs/SESSION-HANDOFF.md`
3. `docs/HANDOVER-RIP3-CONTINUATION.md`
4. `docs/rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md`
5. `docs/rip3/00-ARCHITECTURE-AUDIT.md`
6. `docs/rip3/03-DOMAIN-MODEL.md`
7. `docs/rip3/13-SECURITY-AND-PRIVACY.md`
8. `docs/rip3/05-DOCUMENT-INTELLIGENCE.md`
9. `docs/rip3/batches/P4-3-BATCH-REPORT.md`
10. `docs/rip3/batches/P4-4-TASK.md`（文件创建后）
11. `web/README.md`、`web/src/App.tsx`、`web/src/workspaces.ts`、`web/src/components/Layout.tsx`
12. Research、Projects、Overview、Workflow、Findings、Evidence 对应的实际页面和后端 API。

不要只读文档。必须从代码追踪真实调用链、Schema、迁移、API 和测试。

### 1.4 仓库边界

实际 RIP3 Git 仓库是：

```text
/media/emo/code/pensting/research-platform/
```

外层目录 `/media/emo/code/pensting/` 还包含用户其他研究、证据、运行记录和未完成工作。不要在外层目录运行批量清理或格式化，不要把外层文件当成 RIP3 前端源码。

开始工作前检查两个 Git 工作区状态。只在 RIP3 子仓库中按任务书修改指定文件。外层现存改动和未跟踪资料全部视为用户数据，不得清理或覆盖。

---

## 2. 产品定位与目标

### 2.1 产品定位

RIP 应成为一个可以利用自身知识资产持续开展安全研究的工作平台，而不是把一批 CLI 工具包在漂亮页面里。

产品需要把以下环节连接起来：

```text
Project
  -> Research Task
  -> Asset / Endpoint / Technology Profile
  -> Knowledge Retrieval
  -> Verification Plan
  -> Tool / POC Execution
  -> Evidence
  -> Finding Reference (RTIS)
  -> Coverage Review
  -> Research Report / Knowledge Feedback
```

其中 Phase 5 的直接交付重点是：

- Workflow Studio 的模板和安全研究工作流能力。
- Research Workspace 从占位页面变为可用工作区。
- 知识驱动漏洞验证引擎的最小可验证闭环。
- 测试覆盖矩阵和自适应研究。
- Finding、Evidence、Trace 的完整追溯。
- Web UI 对这些能力提供清晰、稳定、可操作的入口。

### 2.2 与普通扫描器的区别

普通扫描器通常从规则库或模板库出发，对目标执行检测。

RIP 的执行路径应该从项目上下文出发：

1. 读取本项目已知资产、服务、技术栈和 API 信息。
2. 使用 Phase 4 的统一检索与 Context Broker 查找相关知识。
3. 从 RTIS 获取适用的 POC、检测方法和历史使用信息。
4. 判断知识与目标是否匹配，排除版本、协议、认证和前置条件不适用的候选项。
5. 生成有依据、可解释、可预算的验证计划。
6. 复用 Tool Engine、Runtime Gateway、Workflow Engine 调用工具或 Provider。
7. 采集请求响应、日志、截图、原始输出等证据。
8. 对结果进行结构化归类，避免把执行失败误判为漏洞不存在。
9. 将正式漏洞事实通过 RTIS 既有接口处理。
10. 将研究任务、覆盖情况、运行轨迹和可复用方法保留在 RIP3 的相应领域内。

核心价值不是“运行更多 POC”，而是**更少的无效测试、更高的适用性、更可解释的结果、更好的证据质量和更低的重复成本**。

### 2.3 不做什么

Phase 5 不做以下事情：

- 不重写 RIP 现有 Web 前端，不替换 React、Vite、Rust Core 或 Python Workers。
- 不另建通用任务队列、第二套 Workflow Runtime 或第二套工具注册表。
- 不另建漏洞、POC、Finding、攻击链、证据等级或漏洞报告事实数据库。
- 不把 RTIS 的 POC 正文和利用细节复制进 RIP3 文档知识库。
- 不将所有知识库条目不加筛选地对所有目标执行。
- 不将“POC 没跑通”视为“目标没有漏洞”。
- 不以扫描命中数量代替测试覆盖率或研究质量。
- 不在 Phase 5 提前做 Phase 6 的完整报告渲染器、Phase 7 的自动知识回流治理或 Phase 8 的规模压测。
- 不为了复刻 Yakit 而开发完整 Yaklang 运行时、MITM 引擎或第二套脚本语言。
- 不用静态截图、硬编码 JSON 或假任务状态冒充真实功能。

---

## 3. 硬性架构边界

### 3.1 RTIS 是漏洞研究唯一生产事实源

以下约束来自 R2 §3.4 和冻结的领域映射，必须逐条遵守：

| 对象               | RIP3 负责什么                             | 权威事实在哪里                                               |
| ------------------ | ----------------------------------------- | ------------------------------------------------------------ |
| ResearchTask       | 研究编排、步骤、测试覆盖、假设、执行状态  | RIP3                                                         |
| Finding            | 展示、关联、任务上下文、状态引用          | RTIS                                                         |
| Evidence           | 采集 Artifact、哈希、Trace 关联和检索入口 | 原始证据按既定 Artifact 规则保管，结论性证据状态以 RTIS 为准 |
| POC / 漏洞知识细节 | 检索、选择和调用既有能力                  | RTIS                                                         |
| Attack Chain       | 可视化和报告引用                          | RTIS                                                         |
| Report             | 结构化、渲染、导出与追溯                  | 漏洞事实引用 RTIS，报告引擎负责呈现                          |

硬性规则：

1. 对 RTIS 的读取和写入只能经过既有接口，例如 `rt.py` CLI 或 RTIS MCP 工具面。
2. 禁止直接连接 RTIS 生产数据库写数据。
3. 禁止修改 RTIS Schema、接口签名和生产行为。
4. 禁止在 RIP3 新建一套与 RTIS 重叠的漏洞事实库。
5. POC 正文、利用细节和漏洞敏感事实不得复制进 RIP3 的通用文档知识库或公开仓库。
6. RTIS 历史对账必须保持零回归。

### 3.2 既有执行内核必须复用

实施前从代码确认并复用：

- `crates/domain`：领域对象和 ID。
- `crates/database`、当前 PostgreSQL 情报核和既有迁移链：数据读写。
- `crates/intel-db`：Phase 4 统一检索与索引。
- `crates/context-broker`：任务上下文、Token 预算、缓存。
- `crates/tool-registry`：工具 Manifest、Schema、版本与健康信息。
- `crates/tool-engine`：工具派发和执行契约。
- `crates/runtime-gateway`：Podman 隔离执行面。
- `crates/workflow-engine`：Workflow DSL、状态、Checkpoint、暂停恢复和执行。
- `crates/research-engine`：研究能力契约和编排。
- `crates/agent-engine`：动态研究决策与 Trace。
- `apps/server`：统一 Application API。
- `workers/python`：已有 Python Worker 适配和模型推理。
- `integrations/rtis`：RTIS 现有接口适配。

先做调用链盘点。上面列出的模块是候选复用位置，不表示所有目标功能已经完成。

### 3.3 Web 数据访问边界

- React 页面只能经由既有 Application API 获取数据。
- 页面不得直连 PostgreSQL、SQLite、RTIS 数据库或容器。
- 不得为了一个新页面随意新增直连 SQL 的后端旁路。
- 新 API 复用现有认证、错误响应、分页、Correlation ID 和 Trace 约定。
- REST、MCP、CLI 的业务操作最终复用同一 Application Services，不在不同入口重复实现逻辑。
- 运行状态、日志和任务进度优先复用现有事件机制；先检查是否已有 SSE、轮询或其他可靠通道，再决定扩展方式，不要盲目新造 WebSocket 服务。

### 3.4 不擅自加入平台授权审批流程

遵循用户已裁决的 R2 安全策略：

- 平台不新增独立的授权管理系统、法律授权确认表单或强制人工审批节点。
- 研究任务的目标、目标列表和检测参数属于任务输入。
- 发行物按上位计划携带使用免责声明。
- 这不代表工具可以无边界执行：工具仍必须遵循已有执行权限、超时、网络边界、资源限制、审计和低并发约束。
- 默认研究策略采用低影响、可观测、可中止的检测配置。
- 破坏性、持久化、数据修改、拒绝服务类测试不纳入默认自动验证路径。
- 任务创建、启动、停止和操作记录必须清晰可见，不能在用户不知情时后台持续发起扫描。

不要把通用 Workflow 中可能存在的人工节点误解为本项目必须新增授权审批流程。若已有通用 Human Approval 节点，先保持其通用兼容性，本阶段不将其作为安全研究强制步骤。

---

## 4. Web 工作台总体设计

### 4.1 设计目标

现在的问题不是简单的“配色不好看”。目标是让用户能在一个工作区内完成研究，而不是在功能地图、知识库、工具列表、运行记录之间反复跳转。

设计要求：

1. 保留当前五区工作台壳层和稳定路由，基于现有组件做增量升级。
2. 让当前所在项目、研究任务、目标、运行状态和证据上下文始终清晰。
3. 复杂功能通过页面层级、Tabs、Inspector 和 Activity 组织，不要把所有字段全部堆在一个页面。
4. 页面优先呈现“下一步能做什么”和“当前发生了什么”，而不是大面积说明文字或装饰卡片。
5. 所有统计必须来自真实 API，数据缺失时显示 Empty、Unavailable 或 Error 状态，不得编造数字。
6. 桌面端优先，窄窗口和移动端提供合理的查看、搜索、过滤和任务状态能力。
7. Dark / Light 两套现有主题都必须保持一致，默认沿用当前用户主题设置。

### 4.2 五区桌面布局

以 1600 × 1000 为主要设计画布，同时在 1440 × 900、1366 × 768 验收。

```text
┌───────────────────────────────────────────────────────────────────────────────┐
│  顶部全局栏：工作区 / 项目 / 搜索 / 快捷操作 / 运行任务 / Runner / 通知 / 用户  │
├──────┬──────────────────────────────────────────────────────────────┬─────────┤
│      │  工作区 Tabs：Overview / Tasks / Assets / Findings / ...     │         │
│ 左侧 ├──────────────────────────────────────────────────────────────┤ 右侧    │
│ 主导 │                                                              │ Inspector│
│ 航轨 │                    中央主工作区                              │         │
│      │                                                              │ 当前对象│
│      │                                                              │ 属性     │
│      │                                                              │ 关联知识 │
│      │                                                              │ 运行配置 │
├──────┴──────────────────────────────────────────────────────────────┴─────────┤
│ 底部 Activity：Task Queue / Tool Runs / Logs / Output / Evidence / Status     │
└───────────────────────────────────────────────────────────────────────────────┘
```

#### A. 顶部全局栏

包含：

- 工作区切换入口。
- 当前 Project 选择器，显示项目名称和环境标签。
- 全局搜索入口，显示快捷键提示。
- Quick Actions：新建 Project、Research Task、Workflow、导入资料。
- 正在运行的任务数量和最近运行状态。
- Runner / Runtime 连接状态。
- 通知入口。
- 用户设置入口。
- Inspector、Activity 面板显隐按钮。

交互要求：

- 全局搜索支持 Ctrl+K 和 Meta+K，沿用现有搜索实现。
- 项目切换必须改变当前上下文，并让页面数据随 Project 重新获取。
- 运行中任务入口可以直接打开任务详情。
- Runner 离线不能只用一个无解释的红色圆点表示，应显示最近心跳、错误原因和诊断入口。
- 顶栏不塞入大量低频按钮，次要功能放在菜单中。

#### B. 左侧主导航

- 桌面默认 56px 图标轨，与当前实现保持兼容。
- 图标必须有 Tooltip、明确的选中状态和可访问名称。
- 工作区编号和名称作为稳定的数据定义，不要在多个组件里重复硬编码。
- 展开模式可以显示图标和名称，但不能挤压中央区域。
- 导航按产品域组织，不把 Project、Agent、Workflow 混为同一种对象。

建议导航：

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

以上工作区延续当前稳定编号和路由，新增子页面在相应工作区内部组织。

#### C. 工作区 Tabs

- 显示当前工作区内打开的页面。
- 支持固定、关闭、最近访问和路由深链接。
- 同一研究任务可在任务概览、Findings、Evidence 和 Trace 之间切换。
- 刷新页面后 URL 能恢复当前主要对象。
- 不允许仅在 React 内存中保存关键任务上下文而导致刷新丢失。

#### D. 中央主工作区

- 页面头部固定包含 Breadcrumb、标题、简短描述和主要操作。
- 内容区采用 12 列栅格或清晰的两栏/三栏组合。
- 表格、图表、日志、编辑器必须使用合理的内容宽度。
- 主要任务操作必须有明确的主次层级。
- 列表上方提供搜索、过滤、排序、视图切换和批量操作。
- 大列表必须分页或虚拟化，不能一次性渲染全部历史记录。

#### E. 右侧 Inspector

桌面常驻可折叠，建议展开宽度约 300 至 360px。

根据当前选择对象切换内容：

- Project：环境、目标数量、最近任务、关联资料。
- Target / Asset：主机、端口、技术栈、发现来源、最近检测。
- Research Task：状态、阶段、预算、运行时间、关联工作流。
- Finding：RTIS 引用、风险等级、验证状态、位置、证据数量。
- Evidence：Artifact 引用、SHA256、采集时间、Trace 关联。
- Workflow Node：Schema 参数、输入输出、节点状态。
- Tool：工具版本、Runner、Manifest、运行健康。

禁止显示与当前对象无关的固定知识/工具标签。没有选择对象时，Inspector 显示简洁的当前项目上下文、快捷提示或折叠状态。

#### F. 底部 Activity

默认在空闲时收起，执行任务时可自动展开或显示任务数量提示。

Tabs：

- Task Queue
- Tool Runs
- Logs
- Output
- Evidence
- Status

要求：

- 支持高度拖拽和折叠。
- 日志支持级别、时间、任务和工具过滤。
- 任务可取消；恢复和重试调用后端真实状态机。
- 不得仅通过前端隐藏一个任务来伪造取消成功。
- 不得在 Activity 关闭时停止后台任务。

### 4.3 视觉规范

目标是专业的深色安全研究工作台，不是游戏 HUD，也不是运营大屏。

- 默认使用现有 RIP Dark Theme，不创建一套与 `theme.ts`、`token.css` 分离的颜色系统。
- 深色背景采用深蓝灰而非纯黑。
- 表面层级通过轻微明度差、细边框和留白区分。
- 青绿色用于主要交互和选中状态，紫色用于辅助强调。
- 红、橙、黄只用于错误、警告和风险等级，不能作为装饰色。
- 不使用大面积渐变、霓虹描边、玻璃拟态或强烈发光。
- 圆角克制，延续现有 token，不要所有元素都使用胶囊形。
- 页面正文建议 13 至 14px 起；表格和辅助信息不应缩小到难以阅读；核心标题 20 至 24px。
- 表格行高、按钮高度和输入框必须适合长时间桌面操作。
- 中文优先保证字形清晰，不能因为密度而把全站文字压得很小。
- 状态不能只靠颜色区分，必须配合文字、图标或标签。

主题色、字号、间距、阴影、圆角统一使用设计 Token。不要在页面里散落大量任意 Hex 值。

### 4.4 Dashboard 页面目标

首页不应该只是功能地图。功能地图可以作为辅助导航，但不能占据首页主体。

推荐桌面布局：

```text
┌──────────────────────────────────────────────────────────────────────┐
│ Dashboard                                      时间范围 / 项目筛选   │
│ 当前研究概况，最近任务、发现和知识状态                               │
├────────────┬────────────┬────────────┬────────────┬──────────────────┤
│ Projects   │ Assets     │ Findings   │ Running    │ Knowledge        │
│ 项目数     │ 资产数     │ 已确认数   │ 运行任务   │ 待审核/可复用     │
├──────────────────────────────────────┬───────────────────────────────┤
│ 最近研究活动 / 任务趋势               │ 扫描与验证结果分布            │
├──────────────────────────────────────┼───────────────────────────────┤
│ 最近研究项目                         │ 最近 Findings                │
├──────────────────────────────────────┼───────────────────────────────┤
│ 运行任务队列                         │ 知识质量与 Runner 状态        │
└──────────────────────────────────────┴───────────────────────────────┘
```

具体模块：

1. KPI 区：Projects、Assets、Confirmed Findings、Running Tasks、Knowledge Review Queue。数字来自 API。
2. 最近研究活动：按时间展示任务创建、发现、验证、报告和知识审核事件。
3. 任务状态分布：Queued、Running、Paused、Completed、Failed 等真实状态。
4. 最近项目：名称、目标数量、最近活动、当前状态。
5. 最近 Findings：标题、严重程度、验证状态、目标、发现时间。
6. 知识库健康：审核队列、重复候选、过期条目和质量趋势。
7. Runtime：Runner 在线状态、最近心跳、任务执行负载。

首页必须处理以下场景：

- 首次运行且无项目。
- 有项目但无研究任务。
- 没有 Findings。
- API Token 过期。
- API 暂时不可用。
- Runner 不在线。
- 用户没有某个项目的数据权限。

Token 过期不能导致整个首页只剩一个错误框。应保留页面骨架，显示局部错误面板、重新连接操作和受影响的数据范围。不得虚构 KPI 作为占位。

### 4.5 Research Workspace 页面族

Research 是用户最常使用的核心区域。不能继续让 `ResearchPage.tsx` 显示通用 Placeholder。

建议在 Research 工作区内设置以下子页：

| 子页              | 解决的问题                 | 主内容                                                    |
| ----------------- | -------------------------- | --------------------------------------------------------- |
| Overview          | 当前研究整体进度           | KPI、阶段、最近活动、待处理项                             |
| Tasks             | 研究任务管理               | 任务表、状态、优先级、创建时间                            |
| Assets            | 研究目标与资产             | Host、URL、IP、Port、技术栈、来源                         |
| Knowledge Context | 本次研究实际使用了哪些知识 | 检索命中、相关度、来源、排除原因、Token 预算              |
| Scan Center       | 漏洞验证任务               | 匹配知识、验证计划、Provider、运行队列                    |
| Coverage          | 测试覆盖情况               | 七维覆盖矩阵、未测试区域、失败区域                        |
| Findings          | 漏洞发现列表               | RTIS 引用、风险、状态、目标                               |
| Evidence          | 原始证据与 Artifact        | 请求/响应、日志、截图、哈希                               |
| Trace             | 完整执行链                 | ResearchTask、AgentRun、ToolRun、Artifact、Evidence 六 ID |

这些是同一 ResearchTask 的不同观察面，不应创建彼此孤立的数据实体。

#### A. Research Overview

页面顶部：

- 当前 Project。
- 活跃 ResearchTask。
- 目标资产数量。
- 已完成验证项。
- 待验证假设。
- Findings 数量。
- Coverage 完成情况。

中央区域：

- 研究时间线。
- 七维覆盖概览。
- 当前任务阶段。
- 相关知识和工具。
- 最近验证结果。

右侧 Inspector 显示所选 Task 或 Asset。

#### B. Tasks 列表

表格列：

- Task 名称。
- Project。
- 目标数量。
- 研究模板。
- 当前阶段。
- 运行状态。
- 创建时间。
- 最近更新时间。
- 操作菜单。

筛选：

- 状态。
- Project。
- 模板类型。
- 创建时间。
- 是否存在未处理的 Finding。
- 是否存在失败或未完成项。

支持搜索、排序、分页、批量归档（如既有模型允许），以及创建任务。

#### C. Task Detail

推荐 Tabs：

- Summary
- Plan
- Knowledge
- Execution
- Coverage
- Findings
- Evidence
- Trace

任务详情必须明确区分：

- 已完成步骤。
- 正在执行步骤。
- 尚未执行步骤。
- 因条件不满足而跳过的步骤。
- 执行失败步骤。
- 结果不确定步骤。

每一步都可展开查看输入摘要、知识引用、工具、运行参数、耗时、输出和证据引用。

#### D. Assets

资产列表以可读表格为主，不用大量无意义的大卡片。

显示：

- Asset 类型。
- Host / URL / IP。
- Port / Protocol。
- Technology Profile。
- Discovery Source。
- 最近观察时间。
- 是否关联研究任务。
- 最近验证状态。

详情页显示技术栈证据、已发现端点、相关知识、最近测试、相关 Findings 和历史变化。

资产信息必须来自真实发现或导入记录，不能仅根据域名字符串推测技术栈。

#### E. Knowledge Context

这是 RIP 相比普通扫描器最重要的界面之一。

至少显示：

- 本次检索查询和目标上下文。
- 匹配的 KnowledgeCard 或 RTIS POC 引用。
- 来源、版本、适用产品和适用条件。
- 匹配分数和排序依据。
- 被排除的候选及排除原因。
- 是否已在相同目标和条件下执行。
- 是否命中缓存。
- 上下文 Token 预算与实际使用。

不得只显示“匹配 32 条”这种无法解释的数字。用户应该知道某个检测为什么被选中，为什么另一个检测没有执行。

### 4.6 Scan Center 页面

Scan Center 是 Research 工作区中的专门功能页，不另建独立扫描器应用。

#### A. Scan Center 首页

顶部摘要：

- Queued。
- Running。
- Completed。
- Confirmed。
- Inconclusive。
- Failed。

主体：

1. 当前扫描/验证任务队列。
2. 最近验证结果。
3. 最近使用的检测 Provider。
4. 知识匹配概况。
5. 任务预算和请求消耗。
6. 缓存命中及重复抑制统计。

主要按钮：

- New Verification Task。
- View Findings。
- View Coverage。
- Open Task Queue。

#### B. 新建验证任务

不要做一个几十个输入项堆在同一页的表单。采用分区布局或逐步配置：

1. **Target**：选择当前 Project 中的资产，或输入本任务使用的目标。
2. **Profile**：展示技术栈、协议、版本和已知条件。
3. **Knowledge**：显示匹配知识，允许用户查看候选和筛选检测类别。
4. **Strategy**：选择被动检测、低影响主动检测、指定 Provider 或限定检测类别。
5. **Budget**：请求上限、速率、超时、总时长、并发上限。
6. **Plan Preview**：展示准备执行的检测项、跳过项、缓存命中项及原因。
7. **Start**：用户明确启动任务，任务开始后显示真实运行状态。

此流程是任务配置，不增加单独的法律授权审批系统或强制人工审批节点。安全边界来自现有工具执行策略、网络边界、预算、低并发、超时、审计和用户可控的停止操作。

#### C. 扫描任务详情

```text
┌────────────────────────────────────────────────────────────────────┐
│ Task Name       Running / Paused / Completed        Stop / Retry   │
├─────────────┬──────────────┬────────────┬───────────────────────────┤
│ Targets     │ Planned Tests│ Requests   │ Elapsed / Budget          │
├──────────────────────────────────────┬─────────────────────────────┤
│ Verification Items                   │ Current Item Inspector      │
│ - queued                             │ - Knowledge Source          │
│ - running                            │ - Match Reason              │
│ - confirmed                          │ - Provider / Version         │
│ - inconclusive                       │ - Execution Parameters      │
│ - skipped                            │ - Evidence Links            │
├──────────────────────────────────────┴─────────────────────────────┤
│ Activity / Timeline / Logs / Evidence                              │
└────────────────────────────────────────────────────────────────────┘
```

每一个 Verification Item 都能看到：

- 对应知识或 POC 的引用 ID。
- 检测规则和版本。
- 匹配目标的理由。
- 前置条件。
- Provider。
- 执行状态。
- 耗时和请求数量。
- 结果状态。
- Evidence 引用。
- 失败或跳过原因。

#### D. Findings 列表

列：

- Finding 标题。
- 风险等级。
- 置信度或证据等级（以 RTIS 语义为准）。
- Verification Status。
- Asset / Endpoint。
- POC / Source Reference。
- 首次发现时间。
- 最近复核时间。

交互：

- 支持按风险、状态、资产、来源、时间过滤。
- 同一漏洞的重复观察应聚合展示，而不是简单生成多条相似 Finding。
- 可以打开 Finding 详情、证据和 Trace。
- Finding 的漏洞细节从 RTIS 引用读取，不能复制一套 RIP3 本地事实副本。

#### E. Finding Detail

建议布局：

- 左侧或中央：标题、风险、状态、目标位置、影响摘要。
- 主体 Tabs：Overview、Evidence、Reproduction Reference、Remediation、History。
- 右侧 Inspector：RTIS ID、任务来源、知识来源、Artifact 数量、Trace 六 ID。

不要把整个 POC 正文直接展示为普通知识卡片。敏感详情仍按 RTIS 和 Artifact 的安全边界访问。

### 4.7 Evidence 页面

证据页面必须是研究过程中的一等视图。

支持：

- HTTP Request / Response 对照查看。
- Raw / Pretty 两种视图。
- Headers、Body、Cookies 分区。
- JSON 格式化。
- 文本搜索。
- 截图和文件 Artifact 预览。
- SHA256、来源任务、采集时间、工具版本。
- Trace 六 ID 跳转。
- 证据与 Finding、ResearchTask 的关联。

要求：

- Evidence 不因知识更新而覆盖。
- 原始数据和派生展示分离。
- 大 Body 必须分页或截断展示，并提供原始 Artifact 引用。
- 敏感数据应按既有数据权限处理，不能因为 UI 便利而公开展示。
- 页面必须提示证据是否完整、是否经过脱敏、是否为原始内容。

### 4.8 Coverage Matrix 页面

以七维覆盖为基础：

1. Assets。
2. API / Endpoints。
3. Identity & Permissions。
4. Input Points。
5. Business Flows。
6. File Processing。
7. Data Flows。

矩阵建议：

- 行：目标资产、端点、模块或业务流程。
- 列：七维覆盖项。
- 单元格：Covered、Partial、Not Tested、Failed、Not Applicable。
- 点开单元格显示相关 Task、Verification Item、Evidence 和未覆盖原因。

禁止用单纯百分比掩盖覆盖缺口。总覆盖率旁边必须显示分母、统计规则和未测试项数量。

### 4.9 Workflow Studio

继承现有 XYFlow 画布，不重新造流程编辑器。

桌面布局：

- 左栏：节点库和模板。
- 中央：画布。
- 右栏：当前节点 Schema 配置。
- 底部：执行状态、节点日志、输出和 Evidence。

本阶段需要完善：

- 渗透测试模板。
- SRC 研究模板。
- Knowledge Search / Analysis 节点。
- Tool Execution 节点。
- Agent Execution 节点。
- Condition / Parallel 节点。
- Evidence Collection 节点。
- Finding Reference / Creation 节点。
- Coverage Update 节点。
- Report Generation 节点的引用接口（完整报告渲染留 Phase 6）。

沿用现有 Workflow DSL、Schema、Checkpoint、版本、暂停、恢复和失败重试语义。循环节点必须有最大迭代、超时和预算限制。

### 4.10 HTTP History / Repeater / Fuzzer 与完整 Web Security Workbench 的边界

Phase 5 当前批次只按本计划既定范围交付已有请求记录、请求响应查看、Repeater 式单次验证和受控 Fuzzer 闭环。它们不等于完整的 Burp / Yakit 风格抓包工作台。

用户后续明确提出完整产品目标：需要独立 Proxy / MITM 抓包与拦截界面、HTTP History、独立 Repeater、多位置 Web Fuzzer / Intruder，以及 WebSocket、Decoder、Comparer、Passive Audit、插件和证据能力。完整范围、界面规格、Rust-First 架构、协议边界、安全要求和分批验收统一见：

- [RIP3 Web Security Workbench 扩展规划](WEB-SECURITY-WORKBENCH-PLAN.md)

该扩展规划是后续独立工作流，不改变当前 P5-8 / P5-9 顺序，不得提前打断 Phase 5。不得将当前 Phase 5 的轻量 History / Repeater / Fuzzer 交付描述为已实现完整 MITM。

---

## 5. 知识驱动漏洞验证引擎

### 5.1 名称与职责

暂定名称：**RIP-VRVE（Vulnerability Research & Verification Engine）**。

职责：

- 从已有知识和目标画像中筛选候选检测方法。
- 组织可解释的 Verification Plan。
- 复用成熟检测 Provider 和现有工具执行面。
- 去重、缓存和限制重复执行。
- 统一收集执行事件与 Evidence。
- 对结果进行结构化分类。
- 将正式漏洞事实通过 RTIS 既有接口处理。

它不是一个独立数据库、独立容器调度器或独立工作流系统。

### 5.2 输入

至少支持以下输入：

| 输入                | 来源                             | 用途                 |
| ------------------- | -------------------------------- | -------------------- |
| Target / Asset      | Projects、资产发现、ResearchTask | 研究对象             |
| Technology Profile  | 真实指纹与 Evidence              | 适用性判断           |
| Endpoint / API      | 资产发现、用户导入、爬虫结果     | 检测入口             |
| HTTP Context        | 既有请求记录和任务输入           | 请求构造和身份上下文 |
| Knowledge           | Phase 4 Search + Context Broker  | 方法、原理、检测条件 |
| POC                 | RTIS 接口                        | 既有验证逻辑         |
| Historical Result   | 任务执行记录和 RTIS 使用记录     | 去重与有效性评估     |
| Provider Capability | Tool Registry / Manifest         | 可调用能力与版本     |

### 5.3 主执行流程

```text
1. Target Intake
       |
2. Asset / Technology Context
       |
3. Knowledge Retrieval
       |
4. Applicability Filter
       |
5. Duplicate & Cache Check
       |
6. Verification Plan
       |
7. Provider Dispatch
       |
8. Evidence Collection
       |
9. Result Evaluation
       |
10. RTIS Reference / Write-back
       |
11. Coverage Update
```

每一步必须产生结构化状态或可追踪事件。失败时不能丢失已经收集的部分证据。

### 5.4 Knowledge Matcher

必须通过 Phase 4 统一 Search API 和 Context Broker 检索，不能绕开统一检索直接遍历整个知识库。

匹配因素至少包括：

- 漏洞类别或安全问题类型。
- 产品、框架、中间件或组件名称。
- 版本范围和版本证据。
- 服务协议和端口。
- 请求方法和端点类型。
- 必要的认证或会话条件。
- 参数和输入点类型。
- 环境前置条件。
- 相关技术栈与历史 Finding。
- 知识来源、审核状态、质量和时效。
- 历史执行结果和失效记录。

匹配输出必须能解释：

- 为什么命中。
- 命中了哪些条件。
- 哪些条件未验证。
- 为什么排除其他候选。
- 候选知识来自哪个来源。
- 候选版本是否仍有效。
- 是否有相同条件下的历史结果。

禁止只用 Embedding 相似度决定是否执行。向量相似只是一项候选排序信息，最终适用性要结合结构化条件和证据。

### 5.5 Verification Planner

把候选知识转换为可执行计划。计划项建议包含以下语义字段，最终字段名必须以现有 Domain Model 和 Schema 审计为准：

- verification_item_id。
- research_task_id。
- target_reference。
- endpoint_reference。
- knowledge_reference。
- rtis_poc_reference。
- provider_id 与 provider_version。
- applicability_status。
- applicability_reasons。
- prerequisites。
- risk_class。
- request_budget。
- timeout_budget。
- deduplication_key。
- cache_decision。
- execution_order。
- expected_observation。
- negative_control。
- result_status。

Planner 要完成：

1. 合并相同目标、相同 POC 版本、相同检测条件的重复项。
2. 排除明显不匹配或前置条件不满足的候选。
3. 对信息不足的候选标记为需要补充上下文，而不是强行执行。
4. 按预期价值、适用性、风险、成本和历史有效性排序。
5. 默认优先低影响、只读、非破坏性验证。
6. 给每个计划项设置可解释的预算。
7. 输出执行前预览，展示候选、跳过、缓存命中和排除原因。

### 5.6 Provider / Adapter 体系

统一接口建议：

```text
Verification Provider
  ├── RTIS POC Adapter
  ├── Nuclei Adapter
  ├── OWASP ZAP Adapter
  ├── Existing RIP Tool Adapter
  └── Future Provider Adapter
```

各 Adapter 只负责适配外部工具或 RTIS，不得实现自己的第二套任务管理、证据库或 Finding 库。

#### A. RTIS POC Adapter

- 通过 RTIS 既有 CLI / MCP / Adapter 调用。
- 先查找 POC，再获取需要执行的受控引用和元数据。
- 调用后记录 POC 使用关系，遵循 RTIS 现有接口。
- 保留 RTIS POC ID、版本或内容指纹。
- 不直连 RTIS 生产数据库。
- 不复制 POC 正文到 RIP3 通用 KnowledgeCard。

#### B. Nuclei Adapter

- 把 Nuclei 作为成熟 Provider，不把它当成 RIP 的全部扫描逻辑。
- 使用官方模板格式、验证能力、模板过滤、速率限制和结构化 JSONL 输出。
- 保留 Template ID、模板版本或内容指纹、Severity、Matcher、请求响应引用和工具版本。
- 由 Planner 先筛选模板或模板标签，不能不加条件运行全量模板。
- 结果进入 RIP 的统一结果归一化，不直接写 RIP 之外的第二套数据。
- 模板升级后必须使相关缓存失效或重新评估。

#### C. OWASP ZAP Adapter

- 优先利用 ZAP 已有被动扫描、主动扫描、Context、Sites Tree 和 Automation Framework。
- 清楚区分 Passive Scan 与 Active Scan。
- 适配任务配置、运行状态、Alert、请求响应和报告引用。
- 不把 ZAP Alert 数量直接等同于已确认 Finding。
- ZAP 的认证、Session 和 Scope 能力只有在现有架构和本阶段目标需要时才接入，不因此重建另一套身份管理服务。

#### D. Existing RIP Tool Adapter

- 已存在的 rip.* Tool Manifest、Tool Registry 和 Tool Engine 优先复用。
- 工具执行必须经过 Runtime Gateway。
- 保留 ToolRun、Artifact、Evidence 和 Trace 关联。
- 不从前端直接运行 Shell、Podman 或外部二进制。

### 5.7 统一执行结果五态

结果状态与任务状态分开。每一个 Verification Item 必须使用以下五种研究结论状态：

| 状态           | 含义                           | 后续处理                   |
| -------------- | ------------------------------ | -------------------------- |
| CONFIRMED      | 有足够证据确认                 | 关联或创建 RTIS Finding    |
| NOT_REPRODUCED | 在本次条件下未复现             | 保存条件、失败原因和限制   |
| NOT_TESTED     | 未执行                         | 记录未测试原因和覆盖缺口   |
| INCONCLUSIVE   | 结果不确定、证据不足或存在冲突 | 补充证据或进入研究分析     |
| FALSE_POSITIVE | 经证据判断为误报               | 保存误报依据和规则质量反馈 |

必须保证：

- 工具退出码为 0 不等于 CONFIRMED。
- 工具执行失败不等于 NOT_REPRODUCED。
- 超时或网络错误不等于 NOT_REPRODUCED。
- 没有命中不等于目标没有漏洞。
- 未测试不能被统计为测试通过。
- FALSE_POSITIVE 必须有明确的判断依据，不能只是用户点击忽略。

### 5.8 去重与缓存

这是本引擎必须优先实现的效率能力。

#### 四层去重

1. **Asset Dedup**：规范化 Host、Scheme、Port、URL 和资产身份。
2. **Endpoint Dedup**：规范化 Path、Method、参数形态和端点身份。
3. **Verification Dedup**：对目标、POC/Template、Provider 版本、检测参数、身份上下文和前置条件生成指纹。
4. **Finding / Evidence Dedup**：以 RTIS Finding 引用、目标位置、检测类别和 Evidence 指纹识别重复观察。

去重不能只按标题或 URL 字符串判断。不同参数、不同认证状态、不同版本和不同验证条件可能代表不同测试。

#### 缓存键组成

建议至少考虑：

- Target Fingerprint。
- Endpoint Fingerprint。
- Knowledge / POC / Template ID。
- POC / Template Content Version。
- Provider ID 与版本。
- 检测参数规范化哈希。
- Authentication Context Fingerprint（只保存不可逆指纹，不存明文凭据）。
- 目标技术画像版本。
- 相关知识审核状态或索引版本。

#### 缓存失效

以下变化必须使相关缓存失效或重新判定：

- 目标指纹变化。
- 端点变化。
- POC 或 Template 版本变化。
- Provider 版本变化。
- 关键检测参数变化。
- 认证上下文变化。
- 技术画像发生影响适用性的变化。
- 相关知识被撤回、审核状态改变或质量判定改变。

缓存只能用于避免重复执行，不得把历史成功结果无条件视为当前目标的已确认漏洞。缓存命中必须展示时间、条件和证据来源。

#### 去重统计

Scan Center 要展示：

- 候选检测总数。
- 实际执行数。
- 缓存命中数。
- 去重抑制数。
- 条件不匹配数。
- 因预算而跳过的数量。
- 每个 Provider 的运行耗时和请求消耗。

这些统计用于证明优化效果，不能只宣称“减少了重复内容”。

### 5.9 预算和执行控制

所有 Provider 复用现有执行边界。

- 默认低并发，按照现有工作区规则每个工具族默认并发 1。
- 设置请求速率、请求总量、单项超时、任务总时长和最大重试次数。
- 不允许遇到错误后无限重试。
- 对网络失败、认证失败、超时、工具不可用分别分类。
- 支持暂停、恢复、取消和失败重试。
- 取消必须向后端真实执行链传播。
- 默认不执行破坏性和高影响测试。
- 任务和 Provider 的资源消耗必须可观察。

### 5.10 自适应研究引擎

当知识库没有直接命中、规则不适用或扫描器无命中时，系统进入研究补充阶段，而不是直接结束并报告安全。

必须覆盖七个维度：

- Assets。
- API / Endpoints。
- Identity & Permissions。
- Input Points。
- Business Flows。
- File Processing。
- Data Flows。

假设生成必须包含：

- 假设内容。
- 来源依据。
- 与目标的关联原因。
- 前置条件。
- 预期观察。
- 负向对照。
- 建议验证方式。
- 预估成本和风险。
- 停止条件。

假设验证后使用五态结果。没有证据的推测只作为 Research Hypothesis 保存，不得当成 Finding。

循环必须受预算和最大轮次限制，避免 Agent 在低价值假设上无限探索。

---

## 6. Workflow 模板要求

### 6.1 通用渗透测试研究模板

按上位计划的十三步组织：

1. 目标范围确认。
2. 资产清点。
3. 技术栈识别。
4. 知识检索。
5. 威胁建模。
6. 特征检测。
7. 人工或 Agent 分析。
8. 受控漏洞验证。
9. Evidence 收集。
10. Finding 创建或关联。
11. 风险评价。
12. 修复建议。
13. 报告生成。

### 6.2 SRC 漏洞挖掘模板

按上位计划的十三步组织：

1. 收集目标公开资料。
2. 识别资产与业务功能。
3. 分析技术栈。
4. 检索相似漏洞案例。
5. 建立测试覆盖矩阵。
6. 执行低影响检测。
7. 分析业务逻辑与权限边界。
8. 建立安全假设。
9. 受控验证。
10. 记录证据。
11. 评估影响。
12. 查重，避免重复提交。
13. 生成 SRC 报告。

Finding 和报告事实必须通过 RTIS 既有接口处理。

### 6.3 Workflow 节点设计

先检查当前 Node Registry 和 DSL，再决定是否需要新增节点类型。

候选节点：

- Target Input。
- Asset Discovery。
- Technology Fingerprint。
- Knowledge Search。
- Knowledge Analysis。
- Threat Modeling。
- Verification Planner。
- Tool Execution。
- Agent Analysis。
- Condition。
- Parallel。
- Evidence Collection。
- Finding Reference / Creation。
- Coverage Update。
- Report Reference / Generation。

节点参数由 JSON Schema 驱动。禁止每个页面自定义一套不一致的参数表单。

循环必须有最大迭代次数、超时和预算上限。

---

## 7. Phase 5 实施批次

P4-4 未完成前，不执行以下代码批次。可以整理设计和审计结果，但不能绕开当前阶段门禁。

### P5-0：只读审计与设计冻结

目标：建立准确的代码和接口底账。

工作项：

1. 审计 ResearchPage 当前路由和实际占位状态。
2. 审计 Layout、TabArea、InspectorPanel、ActivityPanel、Theme、Token、API 封装。
3. 审计 Projects、Runs、Tools、Knowledge、Workflow 页面可复用组件。
4. 盘点 ResearchTask、Finding、Evidence、Asset、Trace 现有 Domain Model。
5. 盘点 RTIS POC Search / Read / Use / Add 的既有接口。
6. 盘点 Nuclei、ZAP 或其他工具当前 Manifest 和容器镜像状态。
7. 盘点现有 Tool Engine、Runtime Gateway、Workflow Engine 的执行和取消能力。
8. 盘点 API 端点、事件流、分页和错误格式。
9. 形成页面复用矩阵和后端能力复用矩阵。
10. 为新 DTO、Schema、迁移和 Provider 接口起草 ADR。

完成标准：

- 每个“已存在、部分存在、不存在”的结论都有代码路径和测试证据。
- 不重复创建已有能力。
- 领域边界、RTIS 映射和安全边界没有冲突。
- 设计评审通过后再进入实现。

### P5-1：Web 设计系统与 Research Workspace 骨架

目标：让 Research 从 Placeholder 变成统一工作区。

工作项：

- 完成 Dashboard 结构调整和真实数据状态。
- 完成 Research Workspace 页面信息架构。
- 完成 Research Tabs、Inspector 对象适配、Activity 任务适配。
- 完成 Task List、Task Detail、Assets、Knowledge Context 的第一版。
- 统一 Table、FilterBar、DetailPanel、StatusBadge、Empty/Error/Skeleton。
- 完成 Dark/Light 主题一致性。

完成标准：

- 真实 API 驱动，无假数据。
- 桌面端布局稳定。
- 1440px、1366px、移动端主要查看场景通过。
- ResearchPage 不再显示通用 Placeholder。
- 页面可通过 URL 深链接定位对象。

### P5-2：研究任务和验证契约

目标：确认研究任务、验证计划和执行事件如何在现有领域模型中表达。

工作项：

- 检查并复用现有 ResearchTask。
- 明确 Verification Plan / Item / Event 的 Schema。
- 明确 Provider Capability 契约。
- 明确结果五态及任务状态的分离。
- 明确 Evidence 和 Trace 六 ID 的关联方式。
- 明确 API Contract 和分页/过滤标准。

完成标准：

- Schema 有版本号和契约测试。
- 不创建重复 Finding/Evidence 实体。
- 不绕过 RTIS。
- 不直接访问生产数据库。

### P5-3：Knowledge Matcher

目标：从知识库里筛选可用于当前目标的候选检测。

工作项：

- 复用 Phase 4 Search API。
- 复用 Context Broker 和 Token 预算。
- 结合结构化条件做适用性判断。
- 展示命中和排除原因。
- 支持知识来源、状态、质量和版本过滤。
- 记录检索 Trace 和缓存命中。

完成标准：

- 有正样本、负样本和边界样本。
- 不适用版本不会被误认为高匹配。
- 无知识命中时正确进入补充研究状态。
- 检索延迟和 Token 使用可测量。

### P5-4：Verification Planner

目标：把候选知识转成有限、可解释、可去重的检测计划。

工作项：

- 适用性过滤。
- 重复指纹。
- 缓存决策。
- 风险和成本排序。
- 请求与时间预算。
- 执行前预览。
- 跳过原因。

完成标准：

- 相同条件重复任务可命中缓存。
- 条件变化会使相关缓存失效。
- 不能把缓存结果当成当前实时证据。
- 预算约束在后端生效，不是只有前端输入框。

### P5-5：Provider Adapter

目标：接入成熟检测能力。

优先顺序：

1. RTIS POC Adapter。
2. 现有 RIP Tool Provider。
3. Nuclei Adapter。
4. OWASP ZAP Adapter。
5. 后续第三方 Provider。

每个 Provider 先审计许可、维护状态、官方 API、运行方式、容器隔离、输出 Schema 和资源成本。

完成标准：

- Adapter 通过统一契约测试。
- Provider 不直接写 Finding 或 Evidence 事实库。
- Provider 输出保留工具和规则版本。
- 失败和超时能被正确分类。
- 工具执行经 Tool Engine / Runtime Gateway。

### P5-6：执行调度与 Activity

目标：完成真实的研究任务执行闭环。

工作项：

- 使用 Workflow Engine 派发。
- 复用 Task / Run / Trace 语义。
- 记录开始、进度、状态、输出和错误。
- 支持暂停、恢复、取消、重试。
- 限制并发、请求量、超时和总预算。
- Activity 面板实时呈现任务。

完成标准：

- 前端和后端任务状态一致。
- 刷新页面不丢失任务。
- 取消真实生效。
- 服务重启后不出现重复派发。
- 部分失败不会丢弃已完成 Evidence。

### P5-7：结果归一化、Evidence 与 RTIS 闭环

目标：验证结果可以追溯且不会制造脏 Finding。

工作项：

- 结果五态。
- Evidence Artifact 采集。
- SHA256。
- Trace 六 ID。
- RTIS Finding 读取、引用和既有接口写入。
- 重复 Finding 识别。
- 误报原因和失败原因分类。

完成标准：

- 工具结果不直接等于漏洞确认。
- Finding 事实只以 RTIS 为准。
- Evidence 可以从 UI 跳转至来源任务和工具运行记录。
- 原始证据不会被知识更新覆盖。
- RTIS 对账零回归。

### P5-8：Coverage Matrix 与 Adaptive Research

目标：使没有扫描命中时仍能有序推进研究。

工作项：

- 七维覆盖矩阵。
- 测试盲区分析。
- 假设生成和来源依据。
- 负向对照。
- 五态验证。
- 预算与最大轮次限制。
- 研究 Trace 和停止条件。

完成标准：

- 未测试项与失败项分开显示。
- 无命中不等于无漏洞。
- 假设没有证据时不能生成 Finding。
- 研究停止原因清晰可见。

### P5-9：Workflow 模板、画布集成和阶段验收

目标：让用户可以从模板启动完整研究流程。

工作项：

- 通用渗透测试十三步模板。
- SRC 十三步模板。
- 节点 Schema 和表单。
- 画布保存和版本。
- 运行轨迹、日志和 Evidence。
- 暂停、恢复、失败重试。
- 端到端真实浏览器测试。

完成标准：

- 从 Project 创建 ResearchTask。
- 检索知识并生成计划。
- 运行本地靶场验证。
- 形成 Evidence。
- Finding 走 RTIS 接口。
- Coverage 有实际变化。
- Workflow 可恢复且不重复运行已完成节点。

---

## 8. 数据、API 与兼容性要求

### 8.1 不先假定需要新增哪些表

先审计现有迁移和实体。只有确实缺失且通过 ADR 评审后才新增 Domain Model、Schema 或表。

必须遵守：

- 新实体进入既有 `crates/domain`。
- 新表使用项目当前 PostgreSQL 迁移链。
- 迁移编号先查当前已用编号和工作分支。
- 只在开发数据库副本和测试数据库执行。
- 不写生产库，不修改 RTIS 数据库。
- 所有迁移有向前迁移测试和数据对账。
- 不通过删除旧数据解决模型变化。
- 缓存是可重建派生数据，不作为正式漏洞事实。

### 8.2 API 建议

新增端点只能在审计既有 API 后确定。建议能力分类：

- Research Tasks：列表、详情、创建、状态、暂停、恢复、取消。
- Research Plan：候选计划、预览、计划项详情。
- Verification Runs：运行详情、事件、结果和消耗。
- Assets：目标列表、详情和技术画像。
- Coverage：矩阵、维度详情和未测试项。
- Findings：RTIS 引用列表与详情适配。
- Evidence：Artifact 元数据和安全访问。
- Trace：六 ID 链路。

端点命名最终以当前 REST API Contract 和 Application Services 风格为准，不得先创建一套平行的 `/scanner-v2` 接口。

### 8.3 UI 和 API 状态契约

每个页面必须支持：

- Loading。
- Empty。
- Ready。
- Partial。
- Error。
- Unauthorized / Token Expired。
- Offline / Runtime Unavailable。

错误需要包含可读说明、可执行的恢复动作和 Correlation ID。不要只显示原始堆栈或“Something went wrong”。

---

## 9. UI 参考图片与来源清单

**这是防止 Agent 把 UI 做偏的关键章节。** 给 Agent 的参考资料分为“当前实现问题截图”和“外部交互参考”，两者不能混淆。

### 9.1 A 组：用户提供的当前 UI 截图

这两张截图必须与任务书一起交给实际写代码的 AI。

| 图片 | 对话附件文件名              | 图片内容                                                                      | 正确用途                                                                     |
| ---- | --------------------------- | ----------------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| A1   | `图片(20261116-121840).png` | 当前工作台的功能地图、工作区入口、右侧 Inspector、底部 Activity 区域          | 用于理解当前布局和信息密度，指出功能入口过于像功能清单、内容利用率和层级问题 |
| A2   | `图片(20261116-121841).png` | 当前 Dashboard，中央显示登录状态过期，右侧 Inspector，顶部工具栏和工作区 Tabs | 用于理解当前首页、全局壳层和错误状态表现                                     |

A 组图片是**现状问题参考**，不是最终视觉目标。

特别注意：

- A2 中央的 Token Expired 是异常状态截图，不是 Dashboard 正常设计稿。
- 不要把当前截图的字号、间距、卡片比例和内容布局当成必须复制的设计。
- 应保留其已有的五区工作台概念，但改善中央空间、信息层级、可读性和任务操作流程。
- 当前截图展示的右侧 Inspector 和底部 Activity 是布局参照，不是要求永久占满空间。

如果接手 AI 与本次 ChatGPT 对话不是同一个上下文，请用户重新附上 A1、A2 两张图片。不要假设另一个 AI 能读取本次对话附件。

### 9.2 B 组：Yakit 官方 UI 参考

Yakit 采用 Electron 桌面客户端，但其 MITM、History、Web Fuzzer、插件入口等页面组织方式适合 RIP 的 Web 工作台借鉴。**只借鉴交互和信息架构，不复制它的 Electron 技术栈、配色、品牌、Yaklang Runtime 或业务架构。**

#### B1：Yakit 首页 / 工具总览

![Yakit 官方首页工具总览](https://yaklang.com/assets/images/front-page-33603bded5050ce9e5ebd484575220bf.png)

官方页面：[Yakit 下载与产品首页](https://yaklang.com/products/download_and_install/)
![[Pasted image 20261116204401.png]]
借鉴：

- 工具和能力按类别分组。
- 常用功能快速入口。
- 工具市场和能力概览。
- 页面有明确主次，不把所有工具都塞入一张同等权重的大卡片网格。

不要照搬浅色主题和首页的具体卡片样式。

#### B2：Yakit MITM / HTTP History

![Yakit 官方 HTTP History 页面](https://yaklang.com/assets/images/mitm-15-ab2bcc9cb4d3b2d738996865057a1a55.png)

官方页面：[Yakit MITM History](https://yaklang.com/products/mitm/History/)

借鉴：

- 上方为可过滤、可排序的请求列表。
- 下方或侧边查看选中请求的完整内容。
- 请求和响应分区展示。
- 右键操作、标签、批量操作。
- 将选中的历史请求发送到 Repeater / Fuzzer。
- 列表与详情联动，不需要在多个独立页面间复制粘贴。

这是 RIP HTTP History / Evidence Viewer 的主要交互参考。

#### B3：Yakit Web Fuzzer

![Yakit 官方 Web Fuzzer 页面](https://yaklang.com/assets/images/Web-Fuzzer-1-342739740e7536eff9871e556e6cabc1.png)

官方页面：[Yakit Web Fuzzer 基础用法](https://yaklang.com/products/expert-mode/Web%20Fuzzer/Basic-usage/)

借鉴：

- 左侧配置区。
- 中央 Request Editor。
- 右侧 Response Viewer。
- 多个测试会话使用 Tabs。
- 请求配置与结果对照分离。
- 批量测试结果需要有可比较的列和过滤器。

不要直接实现完整 Fuzztag 或照搬其所有并发选项。RIP 必须使用现有 Tool Engine 和预算限制。

#### B4：Yakit 官方功能手册

[ Yakit User Manual ](https://yaklang.com/en/products/intro/)

重点章节：

- UI Overview。
- MITM。
- History。
- Web Fuzzer。
- Plugin System。
- Vulnerability Detection。

用它理解功能边界和交互流程，不要只从 GitHub README 猜测功能。

### 9.3 C 组：扫描引擎行为参考，不是 UI 设计稿

#### C1：Nuclei

- [Nuclei 官方项目](https://github.com/projectdiscovery/nuclei)
- [Nuclei 官方文档](https://docs.projectdiscovery.io/tools/nuclei/get-started)
- [Nuclei 模板说明与运行输出](https://projectdiscovery.io/open-source)

主要借鉴：

- YAML Template 的结构化规则形式。
- Template ID、Severity、Tags、Matcher、Extractor。
- 模板校验和版本管理。
- 按条件筛选模板。
- JSONL 结构化输出。
- 请求速率和并发约束。
- Workflow 模板编排。
- 请求响应证据的结构化保存。

Nuclei 是 Provider，不是 RIP 的知识库，也不是 RIP 的研究规划器。不要把 Nuclei 模板全集复制成一套新的 RIP 漏洞事实库。

#### C2：OWASP ZAP

- [ZAP Desktop UI Overview](https://www.zaproxy.org/docs/desktop/ui/)
- [ZAP Sites Tab](https://www.zaproxy.org/docs/desktop/ui/tabs/sites/)
- [ZAP Automation Framework](https://www.zaproxy.org/docs/automate/automation-framework/)

主要借鉴：

- Sites Tree 和 Context 的组织方式。
- 选中站点后查看 Request / Response。
- Alert 列表和风险分级。
- 被动扫描与主动扫描分离。
- 认证、Session 和运行配置有明确边界。
- 自动化执行使用可解释的配置文件和任务步骤。

ZAP 文档是行为参考。不要把旧版 ZAP 截图当成现代 UI 像素级目标。

### 9.4 D 组：RIP 自身已有截图

AI 必须优先查看以下项目内截图，确认现有设计和已完成页面，避免重复造壳：

- `docs/phase5/browser-e2e/01-overview-desktop.png`
- `docs/phase5/browser-e2e/10-projects-highlight.png`
- `docs/phase5/browser-e2e/11-global-search-rtis.png`
- `docs/phase5/browser-e2e/13-mobile-overview.png`
- `docs/phase6/browser-e2e/05-plugins-list-categories.png`
- `docs/phase6/browser-e2e/06-tool-center-tls-inspect.png`
- `docs/phase9/browser-e2e/03-tool-center-fullpage-real.png`
- `docs/phase8/browser-e2e/01/` 下的 Workflow Studio 实测截图
- `docs/phase7/browser-e2e/01/`、`02/`、`03/` 下的 Agent Studio 截图

这些是 RIP 实际实现截图，比模型凭空想象现有系统更可靠。读取目录时按实际文件列表核实，不要猜图片文件名。

### 9.5 推荐交给 AI 的参考图组合

**最小必需组合：5 张图**

1. A1：用户当前功能地图截图。
2. A2：用户当前 Dashboard 截图。
3. B1：Yakit 首页工具总览。
4. B2：Yakit HTTP History。
5. B3：Yakit Web Fuzzer。

**完整推荐组合：8 张图或页面**

在上面 5 张基础上增加：

6. RIP Phase 5 Overview 真实截图。
7. RIP Projects 真实截图。
8. RIP Workflow Studio 真实截图。

除此之外，将 B4、C1、C2 官方文档链接一并交给 AI，不要求全部转成图片。

如果目标 AI 无法直接打开网页，将 B1、B2、B3 另存为本地图片并放到：

```text
docs/rip3/ui-reference/
  current/
    current-feature-map.png
    current-dashboard-expired-token.png
  yakit/
    yakit-home.png
    yakit-http-history.png
    yakit-web-fuzzer.png
  rip/
    rip-overview-current.png
    rip-projects-current.png
    rip-workflow-studio-current.png
```

目录中的图片是参考资料，不属于运行时资源。不得把参考图打包进 Web 构建产物。

### 9.6 参考资料的使用优先级

- RIP 自身真实截图：确认当前功能、组件、主题和页面布局。
- 用户 A1/A2：理解用户对现状不满意的具体位置。
- Yakit B1：能力入口和工具组织方式。
- Yakit B2：HTTP History、请求列表和详情联动。
- Yakit B3：Repeater / Fuzzer 的请求与响应工作台。
- ZAP 文档：站点树、Context、Alert 和自动化策略。
- Nuclei 文档：模板、匹配、结构化输出和速率控制。

不能把不同参考项目拼成一个完全不一致的 UI。最终设计以 RIP 五区壳层和本文的视觉规范为准。

### 9.7 编码前必须先交付的 UI 设计产物

先让 AI 给出以下页面的完整高保真页面方案，再进入大规模编码：

1. Dashboard 正常状态。
2. Research Overview。
3. Research Task Detail。
4. Scan Center。
5. Finding Detail + Evidence Viewer。
6. Workflow Studio。
7. 可选：HTTP History + Repeater/Fuzzer。

每个页面至少提供：

- 1600 × 1000 桌面布局。
- 关键组件清单。
- 主要交互说明。
- Loading、Empty、Error 三种状态。
- Inspector 展开和折叠状态。
- Activity 展开和折叠状态。
- Dark 和 Light 主题说明。

方案可以先用当前项目的实际 React 组件做静态原型，或者生成可运行的本地页面。不要只交付文字描述，也不要在未对齐布局前直接写全部业务代码。

用户评审页面方案后再进入对应实现批次。设计通过不意味着业务功能已经完成。

---

## 10. 成熟工具与依赖选择规则

遵守 `AGENTS.md` 的成熟库优先硬约束。

每引入一个外部依赖或工具，必须记录：

- 上游仓库和官方文档。
- 最近维护状态和维护者。
- License。
- Rust/Python/Node 版本要求。
- 依赖树与最小 Feature。
- 安全公告。
- 是否支持离线或容器运行。
- 输入输出协议。
- 替换和迁移路径。
- 采用理由和拒绝替代方案的理由。

具体原则：

- HTTP 协议、JSON/YAML 解析、Schema 校验、并发运行时、文件类型和容器管理优先成熟库。
- Rust 新依赖按既有 vendor 规则增量 vendor。
- Python 重依赖放入 Worker 容器镜像，不安装到宿主机。
- 前端优先使用现有 React Query、React Router、ECharts、XYFlow 和组件系统。
- 不为表格、HTTP Editor、JSON Viewer 或代码高亮随意开发通用基础设施。
- 不同时维护两套相同协议的 Parser、Serializer 或 Dispatcher。
- 拒绝成熟库时必须写 ADR 和可复现的测试依据。

---

## 11. 测试与验收

不能只以 cargo test 或 npm build 通过作为完成标准。

### 11.1 工程门禁

按当前仓库和 CI 实际脚本执行，至少覆盖：

```bash
cargo test --offline --workspace
cargo clippy --offline --workspace --all-targets -- -D warnings
python3 -m unittest discover -s workers/python/tests
cd web && npm run typecheck
cd web && npm run build
cd web && npm run test
cd web && npm run format:check
cd web && npm run test:visual:gate
```

如果当前脚本名称、包管理器或门禁已经变化，以实际工作区脚本和 CI 为准，并在报告中记录实际命令，不要机械执行过时命令。

容器相关测试优先在用户要求的 Podman 环境内运行。并发测试按当前 handoff 记录使用静场串行策略，避免将工具容器并发 flake 误判为产品逻辑错误。

### 11.2 Web 真实浏览器验收

至少检查：

- 1600 × 1000。
- 1440 × 900。
- 1366 × 768。
- 390 × 844。

逐页实测：

- Dashboard。
- Projects。
- Research Overview。
- Task List。
- Task Detail。
- Assets。
- Knowledge Context。
- Scan Center。
- Findings。
- Evidence。
- Coverage。
- Workflow Studio。
- Runs。
- Runtime。

检查：

- 页面可达。
- 路由和深链接可用。
- Loading / Empty / Error 正确。
- API 数据真实。
- 表格过滤和分页有效。
- Inspector 与 Activity 可折叠。
- 不发生横向溢出。
- 窄屏下布局可用。
- Token 失效能够恢复。
- 控制台没有未处理异常。
- axe WCAG AA 基础门禁通过。

### 11.3 验证引擎本地靶场测试

只在本地 Podman 靶场和明确的测试资产上执行端到端测试。

测试场景：

1. 匹配到适用 POC，生成计划并运行。
2. 技术栈不匹配，正确跳过。
3. 版本信息不足，标记为不确定或需要更多上下文。
4. 同一检测重复提交，缓存和去重生效。
5. POC 版本变化，缓存正确失效。
6. 网络超时，不被误判为无漏洞。
7. 工具退出异常，任务状态和已收集证据保持一致。
8. 产生正向证据，结果归为 CONFIRMED。
9. 只有弱信号或冲突证据，结果归为 INCONCLUSIVE。
10. 未执行项目保持 NOT_TESTED。
11. 任务取消后不继续派发新检测。
12. Runner 重启后恢复状态，不重复运行已完成项。
13. RTIS 接口不可用时保留可重试状态，不向 RIP3 私有库写一份伪 Finding。
14. RTIS 对账保持零回归。

### 11.4 性能指标

先建立基线，再谈优化幅度：

- Knowledge Matcher 检索耗时。
- Planner 处理候选数和耗时。
- 去重前后候选数量。
- 缓存命中比例。
- Provider 启动耗时。
- 单任务请求数量。
- 单任务总耗时。
- Evidence Artifact 体积。
- Activity 事件数量。
- UI 大列表渲染耗时和内存。

不得在没有实测数据时宣称减少了多少请求或多少 Token。

---

## 12. 开发纪律和禁止事项

执行 AI 必须遵守：

1. 先审计，再设计，再实现。
2. 一次只实施一个明确批次。
3. 每批先列修改路径、复用能力、风险和测试。
4. 不擅自降低功能目标。
5. 不用 Mock 数据冒充真实后端。
6. 不把异常状态当成正常页面设计。
7. 不覆盖用户已有未提交修改。
8. 不执行 reset、clean、checkout、stash、rebase、commit、push，除非用户在当前任务明确要求。
9. 不修改 RTIS 生产库和生产行为。
10. 不在 Web 前端直连数据库或执行命令。
11. 不复制 POC 正文和漏洞利用细节到普通文档知识库。
12. 不在 Phase 5 提前完成 Phase 6 到 Phase 8 的全部内容。
13. 不增加重复的任务、工具、缓存、Workflow 或报告内核。
14. 不因设计调整对全仓库运行格式化。
15. 每批必须报告真实命令、实际输出、失败原因和遗留问题。

如果代码现状与文档不符，先写出差异并给出最小修订建议，不得静默覆盖上位设计。

---

## 13. 每批次交付报告格式

每个 P5-x 批次结束后更新一份报告，至少包含：

- A. 本批完成内容。
- B. 架构变化和复用说明。
- C. 修改文件清单。
- D. 数据库迁移、Schema 和兼容性变化。
- E. 测试命令及真实结果。
- E2. CI 各 Job 状态和运行链接。
- F. 性能数据。
- G. 未解决问题与风险。
- H. 下一批依赖和明确的非目标。
- I. UI 截图与浏览器验收证据。
- J. RTIS 对账和数据安全回归结果。

没有证据的项目必须标记为“未验证”或“未完成”，不能写成“已完成”。

---

## 14. 最终验收场景

Phase 5 完成至少必须完整演练以下场景。

### 场景 A：从 Project 建立一次研究

1. 打开 Project。
2. 查看资产和技术画像。
3. 创建 ResearchTask。
4. 选择渗透测试或 SRC 模板。
5. 进入 Research Workspace。
6. 查看任务阶段和目标。

### 场景 B：利用知识库生成验证计划

1. 从目标画像触发知识检索。
2. 查看候选知识和 RTIS POC 引用。
3. 查看匹配理由、适用条件和排除原因。
4. 查看已命中缓存的检测。
5. 查看最终 Verification Plan。
6. 确认计划受到预算和并发约束。

### 场景 C：在本地靶场执行验证

1. 启动任务。
2. Activity 展示真实状态和日志。
3. Tool Engine / Runtime Gateway 执行 Provider。
4. 采集 Artifact 和 Evidence。
5. 将结果归为五态之一。
6. UI 能追踪到对应 Task、ToolRun、Artifact 和 Evidence。

### 场景 D：重复执行与缓存

1. 对同一目标和相同检测条件重复运行。
2. 系统识别相同检测指纹。
3. 命中缓存或抑制重复执行。
4. UI 显示跳过原因和历史证据来源。
5. 修改目标画像或 POC 版本后，相关缓存失效。

### 场景 E：无命中后的补充研究

1. 初始知识检索没有直接命中。
2. 系统检查七维覆盖情况。
3. 生成有来源依据的研究假设。
4. 标记验证条件和负向对照。
5. 运行受预算控制的验证。
6. 没有证据的假设不得生成 Finding。

### 场景 F：RTIS 与证据闭环

1. 产生确认结果。
2. 通过 RTIS 既有接口处理 Finding。
3. RIP3 页面显示 RTIS 引用。
4. Evidence 可追踪至 Artifact。
5. Trace 六 ID 可完整回溯。
6. RTIS 对账没有回归。

---

## 15. 给执行 AI 的启动提示词

将下面内容连同本文件和参考图片一起交给负责实施的 AI：

```text
你现在负责实施 RIP3 Phase 5 的 Web 工作台与知识驱动安全研究能力。

工程仓库：
/media/emo/code/pensting/research-platform

必须首先阅读：
1. AGENTS.md
2. docs/SESSION-HANDOFF.md
3. docs/HANDOVER-RIP3-CONTINUATION.md
4. docs/rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md
5. docs/rip3/03-DOMAIN-MODEL.md
6. docs/rip3/13-SECURITY-AND-PRIVACY.md
7. docs/rip3/RIP3-PHASE5-WEB-WORKBENCH-AND-KNOWLEDGE-DRIVEN-VERIFICATION-PLAN.md

当前状态：
Phase 0 至 Phase 3 已完成。
Phase 4 的 P4-1、P4-2、P4-3 已完成。
P4-4 Benchmark 是进入 Phase 5 前的剩余批次。
不得打断 P4-4，也不得提前实施 Phase 6、Phase 7、Phase 8。

你的目标：
1. 将 Research 从 Placeholder 改造成真实 Research Workspace。
2. 在现有五区 Web 壳层上升级 Dashboard、Research、Scan Center、Findings、Evidence、Coverage 和 Workflow Studio。
3. 设计并实现知识驱动漏洞验证引擎。
4. 使用 Phase 4 Search 和 Context Broker 筛选适用知识。
5. 使用 RTIS 既有接口读取 POC 和处理漏洞事实。
6. 复用 Tool Engine、Runtime Gateway、Workflow Engine 和 Trace。
7. 使用 Provider Adapter 接入成熟工具，不重复造扫描引擎。
8. 实现检测去重、缓存失效、预算、证据和五态结果。
9. 使无扫描命中时可以进入有预算的自适应研究。
10. 全程通过真实浏览器、真实 API 和本地 Podman 靶场验收。

UI 要求：
- 以用户当前 A1/A2 截图理解现状，不要照抄其视觉和信息密度。
- 以 RIP 当前 browser-e2e 截图确认现有真实组件。
- 以 Yakit 首页、HTTP History、Web Fuzzer 三类页面分别参考工具入口、流量历史、请求与响应编辑器。
- 以 ZAP 文档参考 Sites Tree、Context、Alerts 和自动化步骤。
- 以 Nuclei 文档参考模板、匹配、JSONL 输出和限速，不将终端截图当作 Web UI 设计目标。
- 默认沿用 RIP Dark Theme，保留 Light Theme。
- 不使用大面积渐变、霓虹或过小字号。
- 在编码前先提供 Dashboard、Research Overview、Task Detail、Scan Center、Finding/Evidence、Workflow Studio 的高保真布局方案。
- UI 必须有 Loading、Empty、Error、Expired Token、Runtime Offline 等真实状态。
- 不得使用静态假数据冒充 API。

架构红线：
- RTIS 是漏洞事实唯一生产来源。
- 不直写 RTIS 生产数据库。
- 不复制 POC 正文和漏洞利用细节到 RIP3 通用知识库。
- 不新增第二套 Findings、Evidence、Workflow 或 Tool Registry。
- Web 不直连数据库。
- 所有执行经 Tool Engine / Runtime Gateway。
- 不擅自新增授权审批子系统或强制人工审批节点。
- 默认低并发、低影响、有限预算、可中止。
- 不提前做完整 Report Intelligence 和知识自动回流。

实施方法：
第一步只读审计真实代码和接口。
第二步输出 P5-0 差距清单、页面复用矩阵和 Provider 方案。
第三步按 P5-1 到 P5-9 分批实施。
每批先写最小计划，再改代码，再格式化，再跑测试，再做真实浏览器验收。
每批保留真实测试结果和截图，不得把未验证内容写成完成。

Git：
保护所有已有修改。不要 reset、clean、checkout、stash、rebase、commit 或 push，除非用户当前明确要求。

发现任何与上位架构或 RTIS 映射冲突的地方，立即记录差异，优先遵循上位冻结规范，不得静默造新架构。
```

---

## 16. 外部参考来源

以下来源用于持续核对上游交互与执行能力，正式接入前应再次查看当前版本：

| 项目             | 用途                                           | 官方来源                                                           |
| ---------------- | ---------------------------------------------- | ------------------------------------------------------------------ |
| Yakit            | 工具工作台、MITM History、Web Fuzzer、插件组织 | https://github.com/yaklang/yakit                                   |
| Yakit Manual     | UI、MITM、History、Fuzzer 和插件工作流         | https://yaklang.com/en/products/intro/                             |
| Yakit History    | 请求历史列表与详情联动                         | https://yaklang.com/products/mitm/History/                         |
| Yakit Web Fuzzer | 请求编辑、参数测试、响应结果对比               | https://yaklang.com/products/expert-mode/Web%20Fuzzer/Basic-usage/ |
| Nuclei           | 模板、Matcher、Workflow、结构化输出和速率控制  | https://github.com/projectdiscovery/nuclei                         |
| Nuclei Docs      | 模板执行、过滤、速率限制和 JSONL               | https://docs.projectdiscovery.io/tools/nuclei/get-started          |
| OWASP ZAP        | Sites Tree、Context、Alerts、主动/被动扫描     | https://www.zaproxy.org/docs/desktop/ui/                           |
| ZAP Automation   | 可配置的自动化执行计划                         | https://www.zaproxy.org/docs/automate/automation-framework/        |

---

## 17. 完成定义

只有同时满足以下条件，才能声明 Phase 5 完成：

- Research Workspace 已经是实际可用页面，而不是 Placeholder。
- Dashboard 和 Research 页面均由真实 API 驱动。
- 知识匹配能够解释命中和排除原因。
- Verification Planner 可以生成有限、可解释的计划。
- 至少一个 RTIS POC Provider 和一个成熟扫描 Provider 完成端到端接入，具体选择以 P5-0 审计结论为准。
- 任务执行经现有执行内核。
- 检测去重和缓存失效均有自动化测试。
- 五态结果能正确区分。
- Evidence 和 Trace 能够追溯。
- Finding 事实通过 RTIS 既有接口。
- Coverage Matrix 可以反映实际测试范围。
- 无命中场景可以产生受预算约束的研究假设。
- Workflow 模板可从 Web 创建、运行、暂停、恢复和查看结果。
- 桌面、窄屏、Dark / Light、Loading / Empty / Error 均通过浏览器实测。
- Rust、Python、Web、CI 和 RTIS 对账回归通过。
- 所有未完成项和限制如实写入阶段报告。

**最终产品应当让用户看到的不只是“扫到了多少漏洞”，而是：系统为什么选择这些测试、哪些内容真正执行了、哪些内容被去重、结果依据是什么、还有哪些区域没有测试，以及下一步值得研究什么。**
