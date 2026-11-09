# RIP 2.0：独立研究平台、Web 工作台与工具装备体系升级计划

> 状态：架构规划稿（基于 2026-11-09 本机仓库只读检查）
> 项目：/home/emo/code/pensting/research-platform
> Web 参考：/home/emo/code/telegram-bot
> 本文定义目标架构、边界、路线和验收标准，不代表已经开始实施代码改造。

---

## 1. 核心结论

RIP 不应只是 MCP Server，也不应只是知识库前端。它应成为一套**能够独立运行、自己具备工具能力、管理知识与证据、运行 Agent 和 Workflow，并向外部 AI 提供能力的研究工作平台**。

平台必须同时拥有两类核心资产：

- **大脑（Intelligence）**：知识库、资源目录、研究记录、证据、经验、方法、技能、上下文管理。
- **装备（Arsenal）**：真实执行工作的工具、脚本、CLI、分析器、适配器和运行环境。

只有知识而没有工具，AI 只能阅读、总结和提出建议；只有工具而没有知识，AI 又会重复摸索、缺乏方法。因此知识体系与工具装备体系必须是两个并列的一等能力，并由任务、Agent 和 Workflow 连接。

### 产品形态

1. 独立研究工作台：不依赖外部 AI，也能管理项目、知识、工具、任务、证据和报告。
2. AI 研究运行时：内部 Agent 能发现知识、选择工具、执行任务、保存证据。
3. MCP Server：外部 AI 按需使用 RIP 的知识、工具、Agent 和 Workflow。
4. REST API：为 Web、CLI、外部应用和自动化系统提供统一业务接口。
5. 插件生态：动态接入第三方项目、工具、知识包、Agent、Workflow 和集成。
6. 本地优先：研究数据保存在用户控制的存储中；工具执行与宿主机环境隔离。

### 明确不做

- 不把 RIP 限定为 MCP 项目。
- 不把所有工具写死在 Agent 提示词中。
- 不让 AI 临时在 CachyOS 宿主机安装依赖。
- 不把所有第三方项目源码合并进主仓库。
- 不建立一个包含所有工具、依赖和数据的巨型容器。
- 不推翻已完成的 Rust Core、Python Workers、统一数据库和 M8 工作台。
- 不在架构审计前大规模重写。

---

## 2. 当前仓库基线与可复用资产

本次检查了 research-platform 与 telegram-bot 的目录和关键入口文件，没有执行代码修改。

### 2.1 RIP 当前状态

当前 research-platform/README.md 声明 M0-M8 已交付，M8 已切换生效，RTIS 生产 Resolve 路径默认走统一库，Web 工作台通过 systemd 用户服务运行。

已看到的结构：

| 路径 | 当前职责 | RIP 2.0 处理 |
|---|---|---|
| crates/domain | Domain Model | 保留，作为统一领域模型基础 |
| crates/database | SQLite 数据访问 | 保留并演进 |
| crates/audit-engine | 审计能力 | 保留并扩展运行记录/证据事件 |
| crates/resource-engine | Resource / Resolver | 保留 |
| crates/search-engine | 检索 | 保留，扩展跨插件检索 |
| crates/quality-engine | 质量评估 | 保留 |
| workers/python | parsing / embedding / evaluation Workers | 保留，不做全量 Rust 重写 |
| apps/server | 当前 Web 服务 | 逐步演进为 API / Web 服务边界 |
| integrations/rtis | RTIS 接入 | 保留兼容与回滚路径 |
| migrations | 数据迁移与统一库 | 保留迁移纪律 |
| scripts | 当前维护脚本 | 纳入工具登记，不视为完整工具平台 |
| Containerfile | Rust Core 容器构建 | 保留，不等于最终工具运行架构 |

目前 Web 静态资源位于 apps/server/static，入口是原生 HTML/CSS/JavaScript。app.js 主要通过只读 API 渲染 Overview、Resources、Reviews、Tasks。它是 M7/M8 阶段的轻量工作台，不应被误认为已经具备完整研究工作台能力。

### 2.2 Telegram Bot 项目可借鉴的部分

检查了 /home/emo/code/telegram-bot：

- 管理端位于 admin/，使用 React 19、TypeScript、Vite、React Router、Tailwind CSS 4、DaisyUI 5、Lucide、ECharts。
- admin/src/App.tsx 使用 route-level React.lazy 和 Suspense，页面按路由动态加载。
- admin/src/components/Layout.tsx 已有工作区导航、全局搜索入口、角色/能力驱动的导航过滤、移动端抽屉、沉浸式编辑界面等成熟交互。
- admin/src/workspaces.ts 将工作区与 Tab 组织为数据模型。
- 管理端已有大量独立业务页面、统一组件和视觉回归经验。
- admin/src/game-plugins/ 已出现插件定义、类型和加载器的局部实现，可研究其前端扩展模式；它不是 RIP 后端插件运行时的替代品。
- 项目有生产应用经验，包括路由懒加载、主题、响应式、视觉测试、错误边界和后台页面组织。

**建议：把 Telegram Bot 管理端作为 RIP Web Workbench 的首选前端参考工程，并评估抽取通用 UI 基础设施的可行性。不要直接搬运问卷业务代码。**

复用分三级：

1. 直接借鉴：布局、工作区导航、全局搜索、命令面板交互、主题系统、响应式行为、空/错/加载状态、路由懒加载、视觉回归方法。
2. 解耦后复用：通用 UI primitives、表格、统计卡、Dialog、Toast、错误边界、页面骨架、快捷键管理、主题 token。先检查依赖与授权边界。
3. 不复用：问卷编辑器、问卷/答卷 API、Participant Report、Telegram 认证、业务角色、金币和许可证逻辑。

不要为了复用而让 RIP 依赖整个 Telegram Bot 仓库。最终形成 RIP 自己的前端包或组件库，两个项目独立构建、独立发布。

---

## 3. 产品架构总览

~~~text
用户入口：Web Workbench / CLI / MCP Clients / REST Clients / SDK
                              |
                       Application API
       Auth / Workspace / Project / Search / Plugin / Tool
             Agent / Task / Workflow / Evidence / Report
                  /                         \
       Intelligence Core                 Execution Plane
 Domain / Knowledge / RAG              Tool Registry / Scheduler
 Resource / Quality / Search           Tool Runner / Runtime Adapters
 Evidence / Provenance                 VM Gateway / Podman
                  \                         /
                     Agent & Workflow
          Context Broker / Planner / Tool Selection
          Memory / Evidence Tracking / Orchestration
                              |
                          Data Plane
     SQLite / FTS / Embeddings / File Store / Plugin Store / Artifacts
~~~

### 架构原则

- Application API 是唯一业务入口；Web、MCP、CLI 不各自实现业务逻辑。
- Knowledge Engine 与 Tool Engine 是平级核心能力。
- Agent 负责决策与编排，不直接承担底层工具实现。
- Tool Runner 负责执行；Tool Registry 负责发现、描述和版本管理。
- Context Broker 按需加载知识和工具说明，不把整个知识库或所有工具描述塞入每次上下文。
- 所有工具执行产生标准化 Run、Artifact、Evidence 和日志。
- 外部插件不能直接访问主数据库文件，通过 API、SDK 或受控 Adapter 访问业务能力。

---

## 4. 工具装备体系：让 AI 不再空手工作

### 4.1 为什么必须自己建设工具体系

AI 模型本身并不天然拥有本机命令、浏览器、网络分析、APK 分析、代码审计或文件处理能力。即使模型会生成脚本，也不代表它拥有可用执行环境。

没有统一工具平台会导致：

- Agent 只会给命令建议，却无法执行。
- 不同 Agent 重复实现相似脚本。
- AI 临时安装 Python 包、系统包或 CLI，污染宿主机。
- 工具版本不一致，结果无法复现。
- 一个工具升级影响其他工具。
- 工具输出格式各异，Agent 难以稳定解析。
- 工具来源、许可证、维护状态、参数、资源要求和运行记录无法管理。
- MCP 只暴露零散工具，外部 AI 不知道有哪些能力以及如何组合。

因此 RIP 需要自带可扩展、可发现、可调度、可审计的**工具装备库（Tool Arsenal）**。Agent 工作方式应是：发现能力 → 读取说明 → 获取 Schema → 调用执行 → 解析结构化结果 → 保存证据，而不是现场安装软件。

### 4.2 工具分类

| 工具族 | 典型能力 | 主要输出 |
|---|---|---|
| Web / HTTP | HTTP 请求、响应头、重定向、Cookie、TLS、页面抓取 | 请求记录、响应摘要、页面快照 |
| DNS / Domain | DNS 记录、子域信息、证书透明度数据整理、域名关系 | 域名实体、解析记录、来源 |
| Network | 端口与服务识别、协议探测、连通性诊断 | 服务清单、指纹、扫描记录 |
| Web Analysis | 页面结构、JS 资源、路由/API 线索、静态资源提取 | URL、端点、脚本、线索 |
| Source Code | Git、代码搜索、依赖清单、调用关系、静态检查 | 文件定位、代码证据、依赖图 |
| Mobile / APK | APK 元数据、Manifest、权限、组件、资源、DEX/字符串分析 | APK 报告、组件清单、证据 |
| Binary / Reverse | 文件类型、哈希、字符串、符号、反汇编/反编译适配 | 二进制元数据、分析结果 |
| Document | PDF、Office、Markdown、HTML、OCR、文档转换 | 文本、页码、图片、结构 |
| Data Processing | JSON/CSV/YAML/XML 解析、去重、规范化、格式转换 | 标准化数据集 |
| Knowledge | 索引、检索、实体关联、质量检查、来源核验 | Knowledge Item、引用、质量结果 |
| Evidence | 截图、请求/响应归档、文件哈希、证据包导出 | Evidence Artifact、Manifest |
| Utility | 文件操作、压缩解压、差异比较、编码、哈希 | 结构化工具结果 |
| Browser Automation | 页面导航、交互、截图、下载、DOM 提取 | 页面状态、截图、下载物 |

这是能力规划，不代表每一种工具都从零实现。优先复用成熟开源工具，通过 Adapter 纳入统一 Tool Engine。

### 4.3 工具来源与接入标准

工具可以来自 RIP 原生小工具、经过验证的开源 CLI、第三方项目、自定义脚本和远程工具服务。

接入时记录：来源仓库/发行页、版本或 Commit、许可证、维护状态、平台支持、依赖、镜像摘要、输入输出 Schema、资源要求、已知限制和测试结果。

不要只凭 Star 数决定是否纳入。至少做源码/接口检查、最小功能验证和输出格式验证。

### 4.4 Tool Manifest 示例

~~~yaml
schema_version: "1"
id: "rip.http.inspect"
name: "HTTP Inspector"
version: "1.0.0"
kind: "tool"
description: "发起 HTTP 请求并提取结构化响应信息"
runtime:
  type: "container"
  image: "registry.local/rip/http-inspector@sha256:<digest>"
  entrypoint: ["/opt/rip/bin/http-inspector"]
input_schema:
  type: object
  required: ["url"]
  properties:
    url: { type: string, format: uri }
    method: { type: string, default: "GET" }
    timeout_seconds: { type: integer, minimum: 1, maximum: 60 }
output_schema:
  type: object
  properties:
    status_code: { type: integer }
    final_url: { type: string }
    headers: { type: object }
    body_artifact_id: { type: string }
resources:
  cpu: "1"
  memory: "512Mi"
  timeout_seconds: 90
context:
  summary: "HTTP 请求与响应检查工具"
  docs: "docs/usage.md"
~~~

正式实现需要 JSON Schema 校验、版本兼容策略和字段规范。镜像摘要、运行参数、网络策略必须由可信 Registry 元数据提供，不能让 Agent 任意拼装宿主机命令。

### 4.5 统一工具调用协议

调用对象建议包含：

- tool_id、tool_version
- input：经过 Schema 校验的参数
- workspace_id、task_id、run_id
- context_refs：本次调用使用的知识/证据引用
- timeout、resource_limits、execution_profile
- result_schema_version

统一状态：

QUEUED → PREPARING → RUNNING → SUCCEEDED / FAILED / TIMED_OUT / CANCELLED

统一输出：

- 机器可读 JSON 和人类可读摘要。
- 标准错误类别。
- 运行时间与资源消耗。
- 版本与环境指纹。
- 输出文件 Artifact。
- Evidence 引用。
- 可重放的调用参数（敏感值脱敏）。

原始 stdout/stderr 可以保存为 Artifact，但必须额外生成结构化结果。

### 4.6 Tool Center 用户体验

- 已安装、可用、禁用、异常、更新待处理等状态。
- 工具分类、搜索、标签、来源、许可证、版本。
- 输入 Schema 查看器和参数表单自动生成。
- 单工具试运行页面。
- 运行历史、耗时、资源使用和结果预览。
- 工具依赖和运行镜像信息。
- 启用/禁用、固定版本、更新、卸载。
- 工具包安装和按需加载。
- Agent 可见性控制。
- 工具健康检查和兼容性检查。

---

## 5. 工具执行环境：宿主机、容器还是虚拟机？

### 5.1 明确推荐

**推荐：专用 Linux 工具虚拟机 + 虚拟机内部 Podman + 按工具/工具族拆分容器镜像。**

- CachyOS 宿主机：运行 RIP 主服务、Web 工作台、知识库、数据库和现有模型服务。
- 工具虚拟机：专门运行外部工具和用户脚本。
- VM 内 Podman：管理工具镜像、依赖、生命周期和容器隔离。
- RIP Tool Gateway：把任务提交给 VM，收集结构化结果与 Artifact。
- Agent：只调用 RIP Tool API，不直接登录宿主机、不直接安装依赖、不直接操作 Podman。

这样比工具散装安装在宿主机干净，也比所有东西塞进一个容器更易维护。

### 5.2 方案比较

| 方案 | 隔离性 | 维护成本 | 依赖冲突 | 宿主机污染 | 结论 |
|---|---|---|---|---|---|
| 直接安装在 CachyOS 宿主机 | 低 | 长期高 | 高 | 高 | 不采用 |
| 单个巨型容器运行全部工具 | 中 | 中高 | 容器内部仍会冲突 | 低 | 不采用 |
| 专用 VM + VM 内 Podman 多容器 | 高 | 中 | 低 | 很低 | **推荐** |
| 每个工具单独 VM | 很高 | 很高 | 很低 | 很低 | 仅特殊系统工具考虑 |

### 5.3 为什么不是只用一个容器

不同工具可能需要不同版本的 Python、Node、Java、Go、Rust，不同系统库、网络能力、文件访问范围、资源配置，甚至不同基础发行版。把所有工具放进一个容器会重新制造依赖冲突，一个工具升级也可能影响全部工具。

建议结构：

~~~text
Tool VM
└── Podman
    ├── rip-tool-web
    ├── rip-tool-network
    ├── rip-tool-source
    ├── rip-tool-mobile
    ├── rip-tool-document
    ├── rip-tool-python
    └── rip-tool-browser
    ├── Tool Cache
    ├── Task Workspaces
    └── Artifact Staging
~~~

不是每个小脚本都必须拥有独立镜像。相同运行时、依赖和风险级别的工具可以组成 Tool Pack 镜像；不同依赖栈或隔离需求的工具应拆开。

### 5.4 VM 内容器运行约束

默认使用 rootless Podman：

- 容器默认非 root 用户。
- 根文件系统只读；写入目录单独挂载。
- 每次任务使用独立临时工作目录。
- 只挂载当前任务所需输入和输出目录。
- 不挂载宿主机家目录、SSH Agent、浏览器配置和系统目录。
- 不挂载 Podman/Docker Socket 给工具容器。
- 默认不使用 privileged。
- 关闭不必要 Linux capabilities。
- 设置 CPU、内存、PID、磁盘和执行时间上限。
- 网络访问由执行配置决定，工具不能任意改变 VM 网络规则。
- 镜像固定版本和 digest；更新先验证再切换。
- 任务结束后清理临时容器与目录；需要保留的结果先归档。
- VM 与宿主机之间只开放 Tool Gateway 所需通信。

需要特殊内核能力、USB/设备访问或系统级网络功能的工具，单独标记特殊执行配置，不为了少数工具把整个环境改成 privileged。

### 5.5 VM 与宿主机文件交换

不要让 Agent 任意访问宿主机绝对路径。

1. 用户或 RIP 将文件导入指定 Project/Task Workspace。
2. RIP 为输入文件生成 Artifact ID、哈希、大小和来源元数据。
3. Tool Gateway 将任务所需文件映射/复制到 VM 任务目录。
4. 工具只在任务目录读取输入、写入输出。
5. Tool Gateway 收集输出，计算哈希并登记 Artifact。
6. Web 工作台提供结果查看、下载或导出。

宿主机文件系统只通过受控 Import/Export 服务暴露给 RIP，不向 Agent 直接开放 shell 路径。

### 5.6 VM 失联与恢复

- Tool Gateway 心跳检测 VM。
- VM 离线时任务进入 WAITING_FOR_RUNNER，不伪装成工具失败。
- 任务使用唯一 Run ID 和幂等提交键。
- 重连后查询 VM 已完成/正在运行的任务。
- 超时任务可取消；取消后回收容器。
- VM 镜像升级失败可回滚。
- Runtime 页面展示 VM、磁盘、镜像、缓存和任务状态。

---

## 6. Tool Gateway 与 Tool Engine

### 模块边界

- Tool Registry：工具定义、版本、分类、能力索引。
- Tool Pack Manager：工具包安装、启停、升级和依赖。
- Tool Scheduler：排队、并发、资源分配、优先级。
- Tool Runner：创建任务运行实例并收集结果。
- Runtime Adapter：Podman VM、现有 Python Worker、Rust、MCP、HTTP、CLI 等执行后端。
- Artifact Manager：文件归档、哈希、MIME、来源和生命周期。
- Tool Health Monitor：工具、镜像和 Runner 健康检查。
- Tool Policy Resolver：根据工具声明、工作区配置和任务参数生成执行配置。

### Runtime 类型

| Runtime | 用途 | 优先级 |
|---|---|---|
| podman_vm | VM 内 Podman 容器 | P0 |
| python_worker | 现有 Python Workers | P0，兼容现状 |
| native_rust | RIP 自身 Rust 工具 | P1 |
| mcp_client | 外部 MCP 工具服务 | P1 |
| http_api | 外部 HTTP 服务 | P1 |
| cli_adapter | 已安装在 VM 的 CLI 工具 | P1 |
| remote_runner | 后续远程执行节点 | P2 |

**禁止把宿主机任意 Shell 当作默认 Runtime。** 如确实需要本机工具，应由明确登记的 Native Adapter 实现，并限制可执行文件、参数和工作目录。

---

## 7. 插件系统：像 VS Code 一样扩展，按需加载

插件不只是可执行程序。它可以是工具、第三方项目、知识包、Markdown 文档、Agent、Workflow 或集成。

### 六类插件

1. Knowledge Plugin：Markdown、PDF、代码仓库文档、知识库、方法论。
2. Tool Plugin：脚本、CLI、容器化工具、分析器。
3. Project Plugin：完整第三方项目，通过 MCP/API/CLI/Adapter 接入。
4. Agent Plugin：特定研究领域的 Agent 定义、提示词、规划策略。
5. Workflow Plugin：可导入、可编辑、可复用的流程。
6. Integration Plugin：GitHub/GitLab/Gitee、搜索、模型、向量库、远程 MCP 等。

### 插件目录建议

~~~text
rip-data/
├── plugins/
│   ├── registry/
│   ├── installed/
│   ├── disabled/
│   └── cache/
├── tools/
│   ├── packs/
│   ├── images/
│   └── manifests/
├── workspaces/
├── tasks/
├── artifacts/
├── knowledge/
└── runtime/
    ├── vm/
    └── runner-cache/
~~~

目录位置通过配置指定，不硬编码到源码。

插件状态：

DISCOVERED → VALIDATED → INSTALLED → REGISTERED → ENABLED → LOADED → ACTIVE

禁用、卸载、升级是不同操作。禁用不删除数据；卸载插件不默认删除已进入主知识库的知识和证据。

支持本地目录、Git 仓库、GitHub/Gitee/GitLab、压缩包、MCP Server、HTTP API 等来源。支持固定 Commit/Release，记录许可证和来源。

### 按需加载

插件中心只加载 Manifest 元数据；任务匹配后加载插件摘要；真正调用时才加载完整说明和 Schema。

---

## 8. 知识库与 Context Broker

### 知识库职责

- 保存知识来源、原始文档、解析结果、分块、索引和实体关系。
- 支持跨插件检索、来源追踪、版本追踪和引用。
- 管理重复、低质量、过期、无来源、损坏、冲突和许可证未知内容。
- 保留原始文档，不让清洗过程覆盖唯一原件。
- 通过质量评估和使用反馈改善知识排序。

### 四级上下文加载

| 层级 | 内容 | 加载时机 |
|---|---|---|
| L0 | RIP 全局能力说明与核心规则 | 会话启动 |
| L1 | 插件、工具、知识包目录元数据 | 发现能力时 |
| L2 | 匹配插件摘要、工具说明、知识目录 | 任务规划时 |
| L3 | 完整文档、代码、证据、工具 Schema | 实际需要时 |

Context Broker 负责识别任务意图、检索候选知识/插件/工具、排序、计算 Token 预算、去重压缩、保留来源引用，并防止不相关插件进入上下文。

**工具说明也要分层。** Agent 不需要每轮读取所有工具手册；先发现工具能力摘要，确定候选后再加载参数 Schema 和用法。

---

## 9. Agent 与 Workflow

### Agent Engine

Agent 是任务执行者，不是工具本身。

~~~text
理解目标
  ↓
Context Broker 检索知识与能力
  ↓
制定计划
  ↓
选择工具 / Workflow
  ↓
调用 Tool Engine
  ↓
解析结果并登记 Artifact / Evidence
  ↓
判断继续、换工具、补充检索或结束
  ↓
生成报告
~~~

支持任务创建、暂停、恢复、取消、子任务、并行执行、上下文压缩、失败恢复、有限重试、停滞检测、重复调用检测、目标偏移检测、证据不足提示、预算与资源限制。原始证据不能因上下文压缩而丢失。

Workflow 与 Agent 分离。Workflow 负责确定性编排，Agent 负责动态决策。支持顺序、条件、并行、循环、子工作流、重试、暂停恢复、可视化编辑、版本管理和导入导出。

知识与工具联动示例：

1. 根据任务检索知识和历史案例。
2. 根据目标类型发现所需工具包。
3. 调用基础信息收集工具。
4. 根据结果动态加载对应分析方法。
5. 调用专用分析工具。
6. 把原始结果保存为 Artifact。
7. 提取事实并创建 Evidence。
8. 对照知识库方法与历史案例。
9. 生成报告和待补充问题。

这是编排示例，不应把所有工具硬编码到单一流程。

---

## 10. Web Workbench：完整界面设计

### 10.1 设计目标

RIP Web 要成为日常研究工作台，而不是当前四个只读页签的扩展版。

参考 Telegram Bot Admin 的工作区导航、全局搜索、路由懒加载、响应式布局、统一组件、沉浸式编辑和视觉回归方式；结合 VS Code 的插件与工作区、Obsidian 的知识浏览、GitHub 的仓库/变更信息、Linear 的任务组织和专业 IDE 的 Inspector/Console。

不要照搬任何一个产品的皮肤。整体应信息密度高、层级清晰、工具感强，避免大面积空白卡片堆砌。

### 10.2 桌面端主布局

~~~text
┌─────────────────────────────────────────────────────────────────────┐
│ RIP │ Workspace │ 全局搜索 / ⌘K │ 当前任务 │ Runner │ 通知 │ 用户    │
├──────┬──────────────────────────────────────────────┬───────────────┤
│      │                                              │               │
│ 主   │               中央工作区                     │   Inspector   │
│ 导   │                                              │               │
│ 航   │  多标签页 / 页面标题 / 面包屑                │  当前对象信息 │
│      │                                              │  关联知识     │
│ 总览 │  Dashboard / Knowledge / Tools / Agent      │  关联工具     │
│ 研究 │  Workflow / Evidence / Project / Reports     │  属性 / 来源  │
│ 知识 │                                              │  运行配置     │
│ 工具 │                                              │               │
│ 插件 │                                              │               │
│ Agent│                                              │               │
│ 流程 │                                              │               │
│ 证据 │                                              │               │
│ 项目 │                                              │               │
│ 设置 │                                              │               │
├──────┴──────────────────────────────────────────────┴───────────────┤
│ Activity │ Task Queue │ Tool Runs │ Logs │ Output │ Status         │
└─────────────────────────────────────────────────────────────────────┘
~~~

布局区域：

- 顶部栏：工作区切换、全局搜索、命令面板、当前运行任务、Runner 状态、通知和用户菜单。
- 左侧导航：工作区入口和可配置导航。
- 中央区域：多标签页面，支持固定、关闭、最近访问和深链接。
- 右侧 Inspector：当前选中知识、工具、插件、任务、证据的属性与关联关系。
- 底部活动面板：任务队列、工具运行、日志、输出和后台任务。

右侧 Inspector 与底部面板可折叠；研究、编辑、流程设计页面可以进入沉浸模式。

### 10.3 一级导航与页面结构

| 一级入口 | 页面 | 核心内容 |
|---|---|---|
| Overview | Dashboard | 任务概况、最近项目、知识增长、工具运行、Runner 状态 |
| Research | Research Workspace | 研究项目、目标、任务计划、研究时间线 |
| Knowledge | Explorer / Search / Sources / Quality | 知识检索、文档浏览、来源、质量治理 |
| Arsenal | Tool Center / Tool Packs / Runs | 工具目录、工具包、运行历史、测试工具 |
| Plugins | Plugin Center / Registry | 插件发现、安装、启停、升级、详情 |
| Agents | Agent Studio / Runs / Memory | Agent 配置、能力绑定、运行记录、记忆 |
| Workflows | Workflow Studio / Templates | 流程画布、节点配置、运行与版本 |
| Evidence | Evidence Explorer / Artifact Viewer | 证据列表、文件预览、来源、关联任务 |
| Projects | Projects / Workspaces | 项目、文件、知识、任务和成员 |
| Integrations | Connections / MCP / APIs | 外部服务、MCP 连接、API Key、状态 |
| System | Runtime / Settings / Logs | VM、Podman、镜像、资源、系统设置 |

### 10.4 重点页面布局

**A. Dashboard**

顶部 KPI：活跃研究任务、知识条目、可用工具、已启用插件、待处理运行、证据数量。

中部左侧为任务时间线与最近项目；右侧为 Runner/VM 状态和工具健康状态；下部展示最近工具运行、知识更新、失败任务和待处理事项。每个指标都应能点击进入对应列表。

**B. Research Workspace**

左侧项目树、任务树、研究阶段；中央研究目标、计划、当前步骤和笔记；右侧关联知识、候选工具、插件和证据；底部当前任务的 Tool Runs、日志和产物。支持多项目并行。

**C. Knowledge Explorer**

三栏布局：左侧分类/来源/插件/标签/质量筛选；中间搜索结果/文档目录；右侧 Markdown/PDF/代码预览、元数据、引用和实体关系。支持全文、语义、混合检索、来源跳转、版本对比和质量反馈。

**D. Tool Center（重点）**

左侧分类树：Web、Network、Source、Mobile、Binary、Document、Data、Evidence、Utility。

中央工具列表：名称、描述、版本、状态、来源、运行时、健康状态。

右侧详情：能力与适用场景、输入 Schema、运行环境与依赖、许可证和来源、资源限制、运行历史、试运行入口、相关知识和 Agent。

试运行生成独立 Run，不直接执行未经登记的任意命令。

**E. Plugin Center**

类似 IDE 扩展市场：已安装、可更新、已禁用、可发现；官方/社区/本地/远程来源；搜索、分类、兼容性、许可证；详情、Manifest、能力、依赖图；安装、启用、禁用、更新和卸载。

**F. Agent Studio**

左侧 Agent 列表和模板；中央角色、目标、上下文策略、可用工具、知识和 Workflow；右侧能力测试、Token 预算和运行配置；底部试运行对话与工具调用轨迹。能力由平台配置提供，不由 Agent 临时安装软件获得。

**G. Workflow Studio**

左侧节点分类（知识检索、工具调用、Agent、条件、并行、数据转换、证据保存）；中央可视化画布；右侧节点参数、Schema 和运行配置；底部执行日志、节点状态和产物。

**H. Runtime 页面**

显示 Tool VM 在线状态、心跳、系统版本；Podman 版本和容器数量；镜像与 digest；CPU、内存、磁盘、缓存；执行/排队/失败任务；VM 连接、更新和回滚状态。

### 10.5 移动端布局

- 左侧导航变为抽屉或底部主导航。
- Inspector 变为独立详情页或底部 Sheet。
- 运行面板变为全屏任务详情。
- 复杂 Workflow 画布允许只读查看，编辑提示切换宽屏。
- 工具参数使用 Schema Form，支持折叠高级参数。
- 表格转换为卡片/列表，保留筛选和排序。

### 10.6 前端技术路线

建议优先沿用 Telegram Bot Admin 已验证的技术栈：

- React 19 + TypeScript
- Vite
- React Router
- Tailwind CSS 4 + DaisyUI 5（先评估现有设计 token）
- Lucide React
- ECharts（按需动态加载）
- XYFlow（Workflow Studio）
- JSON Schema 驱动的参数表单与校验
- Playwright 视觉与交互回归

要求 route-level lazy loading、页面按工作区拆包、大型图表/编辑器/画布按需加载、API 客户端与 UI 分离、统一 Loading/Empty/Error/Retry、集中主题 token、快捷键和命令面板，并将可访问性与移动端适配纳入验收。

### 10.7 前端复用落地原则

1. 评估 Telegram Bot Admin 的 Layout.tsx、workspaces.ts、theme.ts、components/ui.tsx、全局搜索和视觉测试方案。
2. 识别与 Telegram、Survey、业务 API 强耦合的代码。
3. 抽取与业务无关的 UI primitives 和布局模型。
4. 在 RIP 单独建立 Web 应用。
5. 用 RIP 自己的 API/Domain Model 实现页面。
6. 逐步补齐视觉回归，不因复用组件牺牲 RIP 的信息架构。

---

## 11. MCP 与外部 AI

MCP 是 RIP 的一种接入协议，不是系统核心。

### MCP Tools

- rip.search_knowledge
- rip.get_knowledge_item
- rip.list_plugins
- rip.search_plugins
- rip.get_plugin_manifest
- rip.list_tools
- rip.get_tool_schema
- rip.run_tool
- rip.get_run_status
- rip.cancel_run
- rip.list_agents
- rip.start_agent_task
- rip.get_task_status
- rip.list_workflows
- rip.run_workflow
- rip.search_evidence
- rip.get_artifact

工具目录支持分页、分类和按需发现。不要在 MCP 初始化时把所有工具的完整说明暴露给模型。

### MCP Resources

- rip://workspace/current
- rip://knowledge/catalog
- rip://plugins/catalog
- rip://tools/catalog
- rip://tasks/{task_id}
- rip://runs/{run_id}
- rip://evidence/{evidence_id}

### 外部 AI 没有工具时的工作方式

1. AI 通过 MCP 发现 RIP 能力。
2. 查询知识库，理解任务背景。
3. 查询 Tool Registry，发现可用工具。
4. 获取工具 Schema。
5. 调用 RIP 的 run_tool。
6. RIP 将任务派发到 Tool VM。
7. 返回结构化结果、Artifact 和 Evidence 引用。
8. AI 根据结果继续规划。

工具能力由 RIP 提供，而不是寄希望于外部 AI 自己安装环境。

---

## 12. 数据模型建议

正式 Schema 以当前 Domain Model 审计为准。新增实体候选：

| 实体 | 主要字段/关系 |
|---|---|
| Plugin | id、version、source、manifest、state |
| PluginCapability | plugin_id、capability_type、capability_ref |
| ToolDefinition | tool_id、version、schema、runtime、source |
| ToolPack | pack_id、tools、dependencies |
| ToolInstallation | tool_id、version、runtime_id、state |
| RuntimeNode | id、kind、endpoint、health、version |
| ToolRun | run_id、task_id、tool_id、input_ref、status、timestamps |
| Artifact | artifact_id、run_id、uri、hash、mime、size、provenance |
| AgentDefinition | agent_id、version、context_policy、capability_refs |
| AgentRun | run_id、task_id、agent_id、state、memory_refs |
| WorkflowDefinition | workflow_id、version、graph、input_schema |
| WorkflowRun | run_id、workflow_id、state、node_states |
| ContextManifest | task_id、selected_knowledge、selected_tools、budget |
| Evidence | evidence_id、source_artifact、claim_refs、provenance |

所有新实体需要明确唯一键、外键、索引、迁移策略、删除语义和版本兼容性。未审计现有表结构前不要直接新增重复实体。

---

## 13. Rust Workspace 演进建议

不一次性创建所有模块。审计确认后分批增加：

- plugin-registry
- plugin-runtime
- tool-registry
- tool-engine
- runtime-gateway
- context-broker
- evidence-engine
- agent-engine
- workflow-engine
- api-server
- mcp-server

Python Workers 继续承担适合 Python 生态的解析、Embedding、评估和专用分析任务。Rust 负责稳定领域模型、数据访问、调度、协议和高可靠核心逻辑。

每个新 Crate 必须有清晰职责、公开接口、错误类型、测试和依赖方向。不要建立巨型 platform Crate。

---

## 14. 实施路线与阶段边界

### Phase 0：只读现状审计与架构冻结

目标：确认当前已实现能力与目标架构的差距。

任务：
- 盘点 Domain Model、数据库、迁移、API、Web、MCP、RTIS、Workers、脚本。
- 盘点 pensting/tools、toolkit、scripts、Python Workers 的能力、依赖、来源和使用方式。
- 盘点 Telegram Bot Admin 可复用 UI 模块与强耦合部分。
- 检查 Containerfile、Podman 使用方式和 VM 条件。
- 形成 Tool Inventory、UI Reuse Matrix、Architecture Gap Matrix。
- 确定 VM 与 Tool Gateway 部署拓扑。
- 明确首批工具包和最小 Web 页面范围。

验收：
- 现有模块有明确的保留/扩展/替换结论。
- 没有重复造轮子。
- 不修改现有生产数据库和 RTIS 行为。
- 输出架构决策记录和分阶段计划。

### Phase 1：Tool Contract 与 Registry

- 定义 Tool Manifest 和 JSON Schema。
- 定义 ToolDefinition、ToolPack、ToolRun、Artifact 基础模型。
- 建立工具发现、查询、Schema 获取和健康状态接口。
- 将现有 Python Workers 和脚本登记为首批候选工具。
- 实现 Mock Runner 验证调用协议。

验收：工具可发现、查看 Schema、验证参数；不执行任意宿主机 Shell；Registry 有版本和来源信息。

### Phase 2：Tool VM 与 Podman Runtime

- 准备专用 Linux VM。
- 配置 VM 内 rootless Podman。
- 建立 Tool Gateway 与心跳机制。
- 建立基础镜像和镜像缓存。
- 实现容器创建、输入挂载、输出收集、超时和取消。
- 完成资源限制和故障恢复测试。

验收：真实工具可从 RIP API 派发到 VM 运行；宿主机没有新增工具依赖；容器不能访问宿主机任意目录；输出可登记 Artifact；VM 离线不会导致任务状态错乱。

### Phase 3：首批 Tool Packs

建议首批：

- Core Utilities Pack：文件元数据、哈希、压缩、格式转换、JSON/YAML/CSV。
- Web Research Pack：HTTP、TLS、页面抓取、静态资源提取。
- Source Analysis Pack：Git、代码搜索、依赖清单、静态检查。
- Document Pack：PDF、Office、Markdown、OCR/文本抽取。
- Mobile Analysis Pack：APK 元数据、Manifest、权限和基础静态分析。

不追求工具数量，优先保证安装、运行、结果 Schema、错误处理和证据归档可靠。

### Phase 4：Context Broker

- 知识、工具、插件的分层发现。
- 任务意图与能力匹配。
- Token 预算与优先级。
- 文档摘要、完整内容按需加载。
- Context Manifest 和引用追踪。

验收：不默认把整个工具目录或知识库正文塞入上下文；可解释选择了哪些知识和工具、为什么选择；工具输出通过 Artifact 引用进入上下文。

### Phase 5：Web Workbench 基础版

- 建立 React/TypeScript 前端。
- 复用评估 Telegram Bot Admin 的通用布局和组件思路。
- 实现 Overview、Projects、Knowledge、Tool Center、Runs、Runtime。
- 实现统一 API Client、路由懒加载、主题和全局搜索。
- 加入桌面与移动端适配。

验收：Web 能管理工具、查看运行、浏览知识和任务；工具结果可预览；关键路由有视觉和交互测试。

### Phase 6：Plugin Runtime 与 Plugin Center

- 插件 Manifest、Registry、生命周期。
- Knowledge / Tool / Project / Agent / Workflow / Integration 插件。
- 本地目录、Git、MCP、HTTP 接入。
- 依赖检查、版本固定、启停和升级。

验收：至少接入一个外部项目和一个知识插件；插件启停不影响核心数据库；元数据与完整内容按需加载。

### Phase 7：Agent Engine

- Agent Definition 与能力绑定。
- Context Broker 集成。
- Tool Engine 调用。
- Task Memory、Evidence Memory、Research Memory。
- 暂停、恢复、取消、重试和运行轨迹。

验收：Agent 能独立查询知识并调用 Tool VM 中的工具；调用可追踪；结果能回到任务上下文并形成 Evidence。

### Phase 8：Workflow Studio

- Workflow DSL 与 Schema。
- 可视化节点画布。
- Tool / Knowledge / Agent / Condition / Parallel 节点。
- 运行、暂停、恢复、失败重试和版本管理。

验收：多步骤 Workflow 可在 Web 中编辑、运行并查看每个节点结果。

### Phase 9：MCP / REST / CLI 对外能力

- MCP Server 适配 Application API。
- 动态工具发现、Resources、Tools、Prompts。
- REST API 和 CLI。
- 外部 AI 使用示例与协议测试。

验收：没有本地工具的外部 AI 仍能通过 MCP 调用 RIP Tool Engine；MCP 与 Web 使用相同业务逻辑。

### Phase 10：生态、可靠性与发布

- 插件注册表和工具包目录。
- 更新策略、兼容性检查和回滚。
- 多 Runner/多 VM 扩展。
- 性能、稳定性、恢复和安全测试。
- 安装、备份、迁移、升级文档。

验收：独立安装部署；Web、MCP、REST、CLI 协同；知识、插件、工具、Agent、Workflow 生命周期完整；宿主机保持干净。

---

## 15. 工程纪律与变更约束

每个开发批次遵循：

1. 分析当前阶段需求。
2. 检查相关源码和既有实现。
3. 检查数据结构、调用链和影响范围。
4. 给出最小实现计划。
5. 编写或更新测试。
6. 实施当前阶段内容。
7. 格式化与静态检查。
8. 单元测试、集成测试和必要的性能测试。
9. 检查对知识库、RTIS、Workers、Web 和工具运行环境的影响。
10. 更新文档与交付报告。

约束：

- 不提前实现后续阶段功能。
- 不为新架构推倒已交付的 M0-M8。
- 不擅自更改生产 RTIS 路径或数据库。
- 不擅自安装工具到 CachyOS 宿主机。
- 不允许 Agent 自由调用系统包管理器安装依赖。
- 不允许工具容器默认访问宿主机文件系统或容器管理 Socket。
- 不擅自 reset、clean、checkout、stash、rebase、commit 或 push。
- 保留用户已有改动；不静默降低测试标准。

---

## 16. Phase 0 首批交付物

1. CURRENT-ARCHITECTURE-INVENTORY.md：模块、API、数据库、Workers、RTIS、Web 和 MCP 清单。
2. TOOL-INVENTORY.md：脚本、工具、依赖、来源、能力、运行方式和复用建议。
3. TOOL-RUNTIME-ADR.md：专用 VM + Podman 的拓扑、网络、文件交换和故障恢复决策。
4. TELEGRAM-ADMIN-UI-REUSE.md：Telegram Bot Admin 可复用模块矩阵。
5. PLUGIN-CONTRACT-DRAFT.md：插件 Manifest 与生命周期草案。
6. TOOL-CONTRACT-DRAFT.md：Tool Manifest、输入输出和 Run 协议草案。
7. CONTEXT-BROKER-DESIGN.md：知识、插件和工具的分层上下文设计。
8. PHASE-ROADMAP.md：按依赖关系冻结的后续实施计划。

---

## 17. 最终验收标准

- 可以作为独立研究平台启动和使用。
- Web Workbench 能管理项目、知识、工具、插件、Agent、Workflow、Evidence 和 Runtime。
- 知识库能够检索、追踪来源、治理质量和关联证据。
- Tool Registry 能发现工具、读取 Schema、管理版本和健康状态。
- Tool Engine 能把任务派发到专用 VM，并由 Podman 隔离执行。
- AI 不需要在 CachyOS 宿主机安装工具或依赖。
- 工具执行有统一 Run、Artifact、Evidence 和日志记录。
- 插件可以动态发现、安装、启用、禁用、升级和卸载。
- 知识与工具说明按任务动态加载，控制 Token 消耗。
- Agent 可以结合知识库与工具库完成多步骤任务。
- Workflow 可以可视化编辑和运行。
- 外部 AI 即使没有自己的工具，也可以通过 MCP 使用 RIP 的工具装备。
- Rust Core、Python Workers 和现有 RTIS 集成保持清晰边界。
- 具备备份、恢复、升级、兼容性与故障诊断文档。

---

## 18. 最终建议与决策

优先顺序不是先做漂亮 Web 页面，也不是先安装几十个工具，而是：

1. **审计现有工具资产**：pensting/tools、toolkit、scripts 和 Python Workers 已有资产先盘点，判断哪些可登记、哪些需包装、哪些应淘汰。
2. **确定工具 VM**：把“AI 不允许随意污染宿主机”落实为架构约束，而不是只写在提示词里。
3. **建立 Tool Contract 和 Registry**：所有工具先有统一身份、Schema、版本和运行记录。
4. **实现 VM + Podman Runner**：让一个真实工具端到端运行。
5. **建设 Web Tool Center 和 Context Broker**：让人和 AI 都能发现、选择和管理工具。
6. **扩展 Agent、Workflow 和插件生态**：让知识和工具成为可组合的研究能力。

**核心决策：RIP 自己拥有工具装备库；工具实际运行在独立 VM 的 Podman 环境中；宿主机只负责平台本身，不承担 AI 临时安装依赖的工作。**
