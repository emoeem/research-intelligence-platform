# RIP3 Web Security Workbench 扩展规划：Burp Suite / Yakit 风格流量分析与测试工作台

> 文档类型：产品范围补充 / 架构规划 / 后续 AI 实施任务书
>
> 状态：设计提案，待用户评审；不是已实现功能清单
>
> 日期：2026-11-17
>
> 适用仓库：`research-platform`
>
> 上位规范：`docs/rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md`
>
> 关联计划：`docs/rip3/RIP3-PHASE5-WEB-WORKBENCH-AND-KNOWLEDGE-DRIVEN-VERIFICATION-PLAN.md`
>
> UI 参考：`docs/rip3/ui-reference/yakit/`

## 一、先明确用户要的是什么

这里所说的「WebFuzzy」不是一个单独的 Web Fuzzing 扫描器，而是一个完整的 Web 安全测试工作台。产品体验应参考 Burp Suite 和 Yakit：有独立的抓包 / MITM 界面，有 HTTP History，有请求重放器，有 Fuzzer / Intruder，并把解码、对比、扫描、插件、证据管理等工具连接成一条连续工作流。

因此，本文统一使用以下名称：

- **Web Security Workbench**：整个产品工作区的名称。
- **Proxy / MITM**：本地 HTTP/HTTPS 代理、请求响应拦截和流量修改。
- **HTTP History**：代理捕获的请求与响应历史。
- **Repeater**：手动编辑、重复发送和比较 HTTP 请求。
- **Web Fuzzer**：参数位置、Payload、请求变体和响应结果分析。
- **Intruder / Batch Runner**：基于位置和 Payload 的批量测试任务。
- **WebSocket Workbench**：WebSocket 握手、帧捕获、编辑和重放。
- **Decoder / Comparer**：编码解码、哈希、格式转换和差异比较。
- **Scanner / Passive Audit**：被动流量审计与已有扫描 Provider 的统一入口。
- **Plugin Center**：可审计、可版本化的扩展工具和规则入口。

目标不是逐项复制 Burp 或 Yakit 的全部实现，也不是引入 Yaklang Runtime，而是在 RIP 已有 Rust Core、Tool Engine、Runtime Gateway、Workflow、ResearchTask、Evidence、Trace 和 RTIS 边界上，构建一套原生集成的安全工作台。

## 二、与当前 RIP3 阶段的关系

### 2.1 不打断当前 Phase 5

当前交接记录显示，P5-0 至 P5-7 已收口，P5-8 仍有收尾工作，P5-9 尚未开工。接手 AI 必须先以 `docs/HANDOVER-P5-WIP.md` 和 `docs/HANDOVER-RIP3-CONTINUATION.md` 的最新记录核实状态。

本规划不改变当前 P5-8、P5-9 的顺序，不允许以本规划为理由提前打断当前批次，也不把本规划描述的完整代理能力伪装成 Phase 5 已交付功能。

### 2.2 对既有 Phase 5 规格的补充

原 Phase 5 计划 §4.10 已提出 HTTP History、Request/Response Viewer、Repeater 和简单 Fuzzer，但明确将完整 MITM 证书安装、透明代理和系统级流量劫持留到后续评估。

用户现已明确：需要的正是 Burp / Yakit 那种完整的抓包、拦截、历史、重放和其他配套工具。因此，本文将原先「先做轻量 History / Repeater」的边界升级为一个独立扩展工作流，规划完整 Proxy / MITM 能力。

这是一项**后续扩展提案**，不是对现有 P5 批次的追溯性改写。Phase 5 当前已有的 HTTP History / Repeater / Fuzzer 交付与本文规划的完整 MITM 工作台必须明确区分。

### 2.3 不能破坏的既有架构边界

1. Rust 是核心业务与网络能力的首选实现语言。
2. React Web 继续使用现有壳层、路由、主题、组件、API 和状态管理体系。
3. 不另建第二套 Agent、Workflow Runtime、Tool Registry、ResearchTask、Finding 或 Evidence 数据体系。
4. 漏洞事实仍以 RTIS 为唯一生产事实源，读写继续走既有 `rt.py` / RTIS MCP 接口。
5. 新功能必须复用 Trace 六 ID、Artifact、Evidence、Tool Engine、Runtime Gateway 和现有审计体系。
6. 不直接访问 RTIS 生产数据库，不复制 POC 正文或敏感漏洞知识到通用知识库。
7. 不默认安装系统级依赖，不把 Python 或 Node 变成代理、HTTP 协议和 Fuzzer 的核心运行时。
8. 不因为新功能重写现有成熟模块。先审计、复用、Adapter 化，再决定是否新增 crate 或迁移。

## 三、产品工作流总览

```text
浏览器 / 测试客户端
        |
        v
本地 Proxy Listener
        |
        +---- TLS / HTTP / WebSocket 协议处理
        |
        +---- Intercept Queue（按规则暂停、放行、丢弃、修改）
        |
        +---- Passive Rules / Traffic Tags
        |
        v
HTTP History（统一流量记录）
        |
        +---- Send to Repeater
        |
        +---- Send to Web Fuzzer / Intruder
        |
        +---- Send to Scanner / Tool
        |
        +---- Send to Comparer / Decoder
        |
        +---- Attach to Research Task
        |
        v
Artifact + Evidence + Trace
        |
        v
ResearchTask / Workflow / RTIS Finding Reference
```

必须实现完整的对象流转：在 Proxy 捕获一条请求后，可以从 History 打开详情、复制到 Repeater、创建 Fuzzer Job、将结果关联 ResearchTask，并把必要的原始请求响应保存为 Artifact / Evidence。所有入口最终走统一的 Rust Application Service，不允许前端自行拼接一套独立业务逻辑。

## 四、功能范围

### 4.1 Proxy / MITM：抓包代理主界面

这是整个工作台的基础能力，不是普通的日志查看页。

**代理监听器管理**

- 支持创建、启动、暂停、停止和删除本地代理监听器。
- 默认只绑定 `127.0.0.1` 或 `::1`，不得默认暴露到局域网或公网。
- 显示监听地址、端口、运行状态、启动时间、当前连接数、捕获数量和错误状态。
- 支持多个具名 Listener Profile，但必须限制资源总量。
- 支持 HTTP Proxy、HTTPS CONNECT 和 TLS MITM。
- 支持显式代理配置；透明代理 / TPROXY 作为独立后续能力，不得作为第一版前置依赖。
- 提供一键复制代理地址、浏览器代理配置说明和测试连通性功能。

**HTTPS 证书管理**

- 支持为当前本地工作区生成独立 CA 证书。
- 展示 CA 指纹、有效期、证书状态和使用说明。
- 用户必须主动执行信任安装操作，不得静默写入系统信任库。
- 支持导出 CA 公共证书、撤销 / 轮换 CA 和卸载说明。
- CA 私钥必须限制文件权限，并使用系统密钥环或加密存储方案；不得写入 Git、日志、截图或普通 Artifact。
- 证书安装与卸载要覆盖 Linux、Windows、macOS 的差异，并明确浏览器独立证书库的处理方式。
- 对证书固定（Certificate Pinning）、客户端证书、mTLS、QUIC / HTTP/3 等无法透明解密的场景，给出清晰状态，而不是显示成空白或假成功。

**拦截控制**

- Intercept 总开关、仅请求拦截、仅响应拦截、请求与响应均拦截。
- 单条请求支持 Forward、Drop、编辑后 Forward、Send to Repeater。
- 队列支持按域名、方法、路径、MIME、状态和规则过滤。
- 支持 Host / URL / Header / Body 匹配规则，以及规则启停、排序和命中统计。
- 支持 Match & Replace 规则，但每次修改必须在消息详情中明确标记。
- 支持拦截超时和队列上限，防止页面长期卡死或无限堆积。
- 默认关闭拦截，避免普通浏览行为因大量静态资源请求而被阻塞。

**协议处理**

- 第一优先级：HTTP/1.1、HTTPS CONNECT、TLS MITM。
- 第二优先级：HTTP/2、WebSocket、SSE。
- HTTP/3 / QUIC：先支持识别、提示和不解密透传策略，是否实现完整解析另立 ADR。
- 对 Gzip、Brotli、Deflate、Chunked、Multipart、Charset、Content-Length 等提供可靠处理。
- 必须区分原始传输内容、解码后的展示内容和发送时重新序列化的内容，不得把展示格式误当成原始报文。

### 4.2 HTTP History：流量历史中心

History 是整个工作台的流量事实入口。Intercept 关闭时仍应持续记录符合捕获策略的流量。

**列表字段**

| 字段 | 说明 |
|---|---|
| Index | 当前工作区内递增序号 |
| Time | 请求开始时间与完成时间 |
| Host / IP | 主机名、实际连接 IP |
| Method | GET、POST、PUT 等 |
| URL | 路径和查询参数 |
| Status | HTTP 状态码 |
| Length | 请求体、响应体大小 |
| MIME | 请求 / 响应内容类型 |
| TLS | 是否经过 TLS |
| Protocol | HTTP/1.1、HTTP/2、WebSocket 等 |
| Duration | 总耗时、TTFB |
| Source | Proxy、Repeater、Fuzzer、Scanner、Import |
| Tags | 用户标签、规则标签、被动检测标签 |
| Edited | 是否修改过 |
| Scope | In Scope / Out of Scope / Unknown |

**筛选和搜索**

- 域名、IP、方法、路径、状态码、MIME、时间区间、协议、来源和标签过滤。
- 支持包含 / 排除规则、正则匹配和多条件组合。
- 搜索 URL、Header、Body 文本和响应内容。
- 支持按 Host 分组、按时间排序、固定列、隐藏列和自定义列。
- 支持清空当前视图、删除选中记录、批量标记和导出。
- 大型响应体采用按需加载和分页读取，列表不能一次把全部 Body 载入内存。

**详情查看器**

- Request / Response 双栏或上下分栏。
- Raw、Pretty、Headers、Params、Cookies、Body、Hex、Rendered 等视图按内容类型展示。
- JSON、HTML、XML、Form、Multipart、图片、文本、二进制内容应有合适的查看方式。
- 头部展示状态码、长度、耗时、协议、远端地址和捕获来源。
- 显示原始消息与修改后的消息差异。
- 允许编辑 Notes、Tags，并关联 Project、ResearchTask、Evidence。
- 支持右键或工具栏动作：Send to Repeater、Send to Fuzzer、Compare、Decode、Export、Attach to Task。

### 4.3 Repeater：手动请求重放器

Repeater 必须是独立的一级工具页面，而不是弹窗或只读详情面板。

**布局**

- 顶部为多标签栏，每个 Tab 对应一份独立请求草稿。
- 中央左侧为 Request Editor，右侧为 Response Viewer。
- 左右面板支持拖动调整宽度，支持上下布局切换。
- 顶部操作区包含 Send、Cancel、History、Follow Redirects、Protocol、Timeout。
- 右侧 Inspector 可显示 URL、Query、Headers、Cookies、Body 参数和认证信息。
- Tab 支持重命名、分组、复制、固定、关闭和恢复最近关闭项。

**请求编辑**

- 支持 Raw HTTP 编辑和结构化编辑两种模式。
- 修改 Method、URL、HTTP Version、Headers、Cookies、Query、Form、JSON、XML 和 Body。
- 保留重复 Header、重复参数和参数顺序的表达能力。
- 对 Content-Length、Transfer-Encoding、Host、Connection 等字段提供显式的自动修复 / 原样发送选项。
- 自动修复必须可见、可切换、可审计，不能悄悄改变用户构造的请求。
- 支持变量引用和从响应提取值，但变量必须有明确作用域和生命周期。
- 支持从 History、Fuzzer、Scanner、Evidence 和文件导入请求。

**发送和结果**

- 单次发送、取消正在发送的请求。
- 可配置连接超时、读取超时、总超时、重定向策略、Cookie Jar 和 TLS 验证策略。
- 显示请求实际发送版本、最终 URL、重定向链、远端 IP、状态码、TTFB 和总耗时。
- Response 提供 Raw / Pretty / Hex / Rendered 视图。
- 保存每次发送的请求快照和响应快照，支持版本历史和 A/B 对比。
- 同一 Tab 的请求修改历史不得被覆盖；可恢复上一次版本。

### 4.4 Web Fuzzer / Intruder：参数与请求变体测试

Fuzzer 不只是把一个参数替换成字典。需要覆盖 Yakit Web Fuzzer 的请求构造、Payload 处理、数据提取和结果分析工作流，同时保留 Burp Intruder 的可理解操作方式。

**Payload 位置**

- URL Path。
- Query 参数。
- Header。
- Cookie。
- Form 字段。
- JSON 字段。
- XML 节点。
- Multipart 字段。
- Raw Body 指定位置。
- 多位置组合和请求序列中的变量位置。

**Payload 来源与处理**

- 手工列表。
- 本地字典文件。
- 数字、字母、日期和范围生成器。
- 编码 / 解码链。
- 前缀、后缀、大小写、替换、拼接和正则变换。
- 可扩展 Mutator / Processor。
- 结构化 Payload 模板。
- 复用已有 RTIS 知识和工具插件，但不得把 RTIS POC 正文复制到普通工作区数据库。

**运行模式**

- 单位置逐个替换。
- 多位置笛卡尔积。
- 多位置同步迭代。
- 请求序列 / 多包联动。
- 低并发批处理。
- 顺序执行与并行执行必须区分。
- 每种模式在启动前显示预计请求数、并发数、速率和预算。

**结果分析**

- 状态码、响应长度、耗时、标题、MIME、关键字和正则匹配。
- 响应正文相似度和差异。
- Timeout、连接失败、TLS 错误、限流和服务端错误分类。
- 支持提取器、匹配器、结果标记、去重和排序。
- 支持把有价值的请求发送到 Repeater，把结果关联 Evidence。
- 结果必须保存输入 Payload、请求快照、响应摘要和规则版本，能够复现。

**必须有的控制项**

- 最大请求总数、最大并发、请求速率、请求间隔、最大响应体、单任务总时长。
- 暂停、恢复、取消和失败重试。
- 取消后不再产生新请求，已发送请求的最终状态仍要记录。
- 预算由 Rust 后端强制执行，不能只依赖前端输入限制。
- 默认低速、低并发，不提供默认高强度爆破配置。
- Out of Scope 目标不得进入主动 Fuzz 队列。

### 4.5 WebSocket Workbench

- 捕获 WebSocket 握手请求与响应。
- 展示连接状态、方向、Opcode、时间、Payload 长度和帧序号。
- 文本帧可读展示，二进制帧支持 Hex。
- 支持双向帧列表、搜索、过滤和导出。
- 支持编辑后发送单帧、重放历史帧和按序列重放。
- 支持 Ping / Pong / Close 状态观察。
- 每条帧可关联对应连接、HTTP History、ResearchTask 和 Evidence。
- 不把 WebSocket 帧与普通 HTTP Request 混成同一种实体。

### 4.6 Decoder / Encoder

- URL、Base64、Base64URL、Hex、HTML Entity、Unicode、JWT 结构解析。
- Gzip、Deflate、Brotli 解压。
- JSON 美化、压缩、格式校验。
- 常见 Hash 计算。
- 多级编码链，支持从上一步输出进入下一步。
- 对 JWT 只做结构与签名信息展示，不默认执行密钥猜测或伪造。
- 输入输出可互换，支持复制和发送到 Repeater。
- 插件扩展必须声明输入输出类型与资源限制。

### 4.7 Comparer：请求与响应差异比较

- 支持两个 Request、两个 Response、两个 Body 或两个 Fuzzer Result 对比。
- 文本 Diff、JSON 结构 Diff、Header Diff、二进制 Hex Diff。
- 高亮增删改区域。
- 支持忽略动态字段、时间戳、随机 Token 和指定 Header。
- 结果可以保存为 Artifact，并关联到对应的两个来源对象。
- 不允许只用响应长度相同就认定响应内容相同。

### 4.8 Passive Audit / Scanner

- 代理流量经过被动规则分析时，不阻塞正常请求转发。
- 被动规则可识别安全 Header 缺失、Cookie 属性、信息泄露迹象、异常跳转和常见配置问题。
- 被动发现只能生成候选项，不得直接成为已确认 Finding。
- 主动扫描继续复用现有 Nuclei、ZAP、RTIS POC Adapter 和 Tool Engine。
- 不新建第二套扫描器注册表。
- Scanner 结果遵守现有五态结果模型，Finding 事实仍由 RTIS 管理。
- 主动扫描与 Proxy 捕获必须共享同一 Scope、Budget、Trace 和审计上下文。

### 4.9 Plugin Center / 扩展能力

- 复用 Tool Registry、Plugin Registry、Plugin Runtime 和现有 Tool Contract。
- 插件必须声明名称、版本、来源、许可证、输入 Schema、输出 Schema、权限需求和运行环境。
- 插件不得直接写业务数据库，不得绕过 Runtime Gateway 执行受控工具。
- 规则、Payload Processor、Passive Check、数据提取器、格式转换器可以作为不同类型的扩展点。
- 优先复用成熟工具或现有 RTIS 能力，不引入 Yaklang Runtime 作为平台核心依赖。
- 插件安装、更新、启用、禁用和执行记录沿用既有插件信任与审计机制。

### 4.10 Web Screenshot / Browser Evidence

- 支持 Viewport、Full Page、Element 和指定区域截图。
- 截图与当前 URL、最终 URL、页面标题、时间、Viewport、DPR、浏览器版本、加载状态和 Trace 关联。
- 截图、DOM、页面文本、网络请求摘要进入统一 Artifact 管理。
- 可从 HTTP History、ResearchTask、Finding Reference 和 Evidence 页面查看。
- 截图仅证明某时刻的视觉状态，不单独构成漏洞确认。
- 需要评估 Chromium 生命周期、并发、超时恢复和浏览器隔离，不将 Node / Python 浏览器驱动变成 Rust Core 的替代品。

## 五、界面信息架构与详细布局

### 5.1 一级导航

在现有 RIP Web 工作台中增加一个明确的 **Web Security** 工作区，不另建一套独立 SPA。

建议子导航：

1. Proxy / MITM
2. HTTP History
3. Repeater
4. Web Fuzzer
5. WebSocket
6. Intruder Jobs
7. Decoder
8. Comparer
9. Passive Audit
10. Plugin Center
11. Proxy Settings

现有 Research、Projects、Knowledge、Tools、Workflows、Runs、Evidence 等页面继续保留。Web Security 是新工作区入口，不替换全局导航或五区壳层。

### 5.2 Proxy / MITM 页面

**顶部状态栏**

- Listener 状态指示、地址和端口。
- Start / Stop 控制。
- Intercept On / Off 开关。
- 当前连接数、排队请求数、累计捕获数。
- 证书状态和设置入口。

**左侧拦截队列**

- 请求方法、Host、Path、时间和方向。
- 请求 / 响应拦截类型标识。
- 规则命中标记。
- 选中请求后中央区域显示消息编辑器。

**中央消息区**

- Request / Response 两个可切换编辑面板。
- Raw / Pretty / Params / Headers / Body 标签。
- Forward、Drop、Send to Repeater 等明确操作按钮。
- 修改前后差异提示。
- 队列中切换上一条 / 下一条请求。

**右侧 Inspector**

- URL 解析结果。
- Header / Cookie / Query / Body 参数。
- 目标 Scope 和解析 IP。
- 被动规则命中情况。
- Trace、Project、ResearchTask 关联信息。

**底部 Activity**

- Listener 日志。
- TLS 握手与证书错误。
- 连接失败和解析错误。
- 拦截队列积压和资源状态。

### 5.3 HTTP History 页面

- 顶部为搜索栏和 Filter Builder。
- 主体为可排序、可调整列宽的请求列表。
- 默认按时间倒序。
- 下方或右侧为 Request / Response 详情。
- 详情区支持 Raw、Pretty、Headers、Params、Body、Hex、Rendered。
- 行操作提供 Repeater、Fuzzer、Comparer、Decoder、Evidence 和 Export。
- 支持按 Host 分组与仅显示 Scope 内请求。
- 大列表使用虚拟滚动、服务端分页或游标，不允许一次性加载所有 Body。

### 5.4 Repeater 页面

- 顶部 Tab Bar 支持多请求并行编辑。
- 左侧 Request Editor，右侧 Response Viewer。
- 中间分割线可拖动。
- Send 按钮有发送中状态和 Cancel。
- Response Header 展示状态、耗时、长度和协议。
- 请求编辑历史可以回退和恢复。
- 支持复制 Tab、分组和批量关闭。
- 允许将请求保存为可复用模板，但模板不保存明文凭据。

### 5.5 Web Fuzzer 页面

建议采用四区结构：

1. **请求区**：原始请求、结构化参数、Payload Position 标记。
2. **Payload 区**：Payload 来源、Mutator、Processor、编码链和组合策略。
3. **运行区**：并发、速率、超时、请求总量、预算和启动 / 暂停 / 取消。
4. **结果区**：结果表格、状态码分布、长度分布、耗时分布、差异预览和详情。

启动前必须展示请求数量估算和当前 Scope 判定。运行过程中显示已完成、排队、成功、失败、超时和被跳过数量。用户可从结果行打开原始请求响应或发送到 Repeater。

### 5.6 统一交互规范

- 深色和浅色主题沿用 RIP 主题 Token，不复制 Yakit 配色。
- 表格、过滤器、详情面板、状态徽标、空态、错误态和 Loading 沿用现有组件。
- 桌面端优先，支持常见 1366 / 1440 宽度；窄屏提供查看和任务控制能力，不强求在手机上编辑大型原始报文。
- 请求编辑器需要等宽字体、行号、语法高亮、搜索、换行控制和大文本性能优化。
- 键盘操作：发送、切换 Tab、搜索、保存、关闭等提供快捷键，并显示快捷键提示。
- 所有破坏性或高影响操作有清晰确认和可见执行状态。
- 不使用静态假数据伪造流量、任务、结果或运行状态。

## 六、Rust-First 技术架构

### 6.1 语言职责

| 层 | 首选技术 | 职责 |
|---|---|---|
| 协议与网络核心 | Rust | Proxy、TLS、HTTP、WebSocket、消息解析、序列化、Repeater、Fuzzer |
| 领域模型与存储 | Rust | Session、Exchange、Message、Job、Result、Artifact 引用、状态机 |
| 执行与调度 | Rust | 并发、速率、预算、取消、重试、Scope、Trace、审计 |
| Application API | 现有 Rust Core / 已有服务边界 | Web API、事件推送、MCP / CLI 同源操作 |
| UI | 现有 React / TypeScript | Proxy、History、Repeater、Fuzzer 等交互页面 |
| AI / 专用算法 Worker | Python 可选 | 模型推理、OCR、视觉分析、特定研究算法 |
| 第三方工具 | 按工具自身语言 | 通过 Tool Adapter / Runtime Gateway 集成 |

**Rust 是主实现语言，不等于把所有工作强行塞进一个 crate。** 是否拆分 crate 必须由 P0 审计和 ADR 决定，不能仅为了目录整齐而拆分。

Python Worker 不得维护第二套 Proxy Session、History、Repeater、Fuzzer Job、权限或 Evidence 事实状态。Worker 只能通过 Tool Contract 接收输入并返回结构化结果。

### 6.2 建议的模块边界

以下是候选边界，不是预先冻结的 crate 名称：

- Proxy Runtime：监听器、连接生命周期、TLS MITM、协议转发。
- HTTP Message Model：原始报文、解析消息、Header、Body、协议元数据。
- Traffic Store：History 元数据、Artifact 引用、检索和分页。
- Intercept Controller：拦截规则、队列、Forward / Drop / Modify。
- Request Sender：Repeater 请求发送、重定向、Cookie 和 TLS 策略。
- Fuzz Engine：Payload、Mutator、Processor、Job Scheduler、Result Analyzer。
- WebSocket Engine：握手、Frame、连接状态和重放。
- Passive Audit Engine：规则匹配与候选发现。
- Evidence Adapter：Artifact / Evidence / Trace 统一接入。
- Web Security Application Service：对 UI、MCP、CLI 提供同源业务接口。

必须先审计现有 `tool-engine`、`verification-engine`、`runtime-gateway`、`domain`、`database` 和 `intel-db`。新模块只补足当前缺口，不复制已有执行器和存储能力。

### 6.3 HTTP 消息模型的关键原则

必须区分：

1. **Wire Representation**：捕获时的原始传输表示，能保留的字节必须保留。
2. **Parsed Representation**：用于 UI、过滤、搜索和规则匹配的结构化表示。
3. **Display Representation**：经过解压、字符集转换和格式化后的展示内容。
4. **Outbound Representation**：实际发送到目标端的序列化字节。

四者不能混为一谈。对于无法完整保留的传输细节，应记录明确的 Capture Fidelity 状态和原因，不能声称是逐字节原样重放。

建议每个 Exchange 至少具备：

- exchange_id、session_id、listener_id、sequence_no。
- direction、source、protocol、method、scheme、authority、path。
- request_headers、request_body_artifact、response_status、response_headers、response_body_artifact。
- started_at、completed_at、duration_ms、ttfb_ms。
- remote_ip、tls_version、alpn、http_version。
- scope_decision、edited、tags、notes。
- trace_id、task_id、tool_run_id、artifact_id、evidence_id（适用时）。
- capture_fidelity、parse_status、error_code。

消息 Body 大小必须受限，较大内容进入 Artifact / CAS，数据库只保留元数据和引用。历史记录必须有容量上限、保留策略和用户可控清理方式。

### 6.4 事件与状态

- Proxy Listener：STOPPED、STARTING、RUNNING、DEGRADED、STOPPING、FAILED。
- Intercept Item：QUEUED、FORWARDED、DROPPED、MODIFIED_FORWARDED、EXPIRED。
- Repeater Send：PENDING、SENDING、SUCCEEDED、FAILED、CANCELLED、TIMED_OUT。
- Fuzzer Job：CREATED、QUEUED、RUNNING、PAUSED、CANCELLING、CANCELLED、COMPLETED、FAILED、BUDGET_EXHAUSTED。
- Fuzzer Case：PENDING、SENT、RESPONDED、TIMEOUT、NETWORK_ERROR、SKIPPED_SCOPE、SKIPPED_BUDGET。
- 被动发现与正式 Finding 必须分开建模，不能把规则命中直接提升为已确认漏洞。

状态迁移由 Rust 后端定义，前端只展示和发起动作。事件必须具备事件 ID、时间、Trace 关联和幂等处理规则。

## 七、网络安全、隐私与运行边界

### 7.1 本地代理安全

- 默认只监听 loopback，禁止默认开放为公网或局域网代理。
- 禁止成为匿名开放代理。
- Listener 可设置本地访问令牌或会话绑定，防止其他本机进程滥用。
- 监听器停止后关闭连接并释放端口。
- 不得通过自动修改系统代理设置来隐式改变用户网络环境。
- 不得静默安装或信任根 CA。

### 7.2 Scope 与目标边界

- Scope 复用 ResearchTask 的目标和现有工具执行边界。
- Scope 至少支持 Scheme、Host、Port、Path、Include / Exclude、解析 IP 和重定向策略。
- 每次主动发送或 Fuzz 前由后端再次校验 Scope。
- 重定向跳转必须重新进行 Scope 校验。
- DNS 解析结果变化时重新检查目标地址，避免 DNS Rebinding 绕过。
- 私网、回环、链路本地、云元数据地址等按任务策略明确处理，不允许默认意外访问。
- 被动捕获可以按用户配置监控，但主动 Fuzz / Scanner 不得自动扩大目标范围。
- Out of Scope 记录必须可见，不能悄悄被发送。

### 7.3 请求与资源限制

- 全局与单任务并发上限。
- 每秒请求速率和最小请求间隔。
- 单请求连接、读取和总超时。
- 最大请求体、响应体、Header 数量和单 Header 长度。
- 最大拦截队列长度和 History 存储容量。
- 最大 Fuzzer Case 数、总预算和最长运行时间。
- 取消操作必须尽快停止后续请求。
- 所有上限在 Rust 后端强制执行。

### 7.4 敏感数据

- Cookie、Authorization、API Key、Token、密码和个人信息应支持展示脱敏。
- 原始证据与脱敏展示分离，访问原始数据必须经过既有权限边界。
- 不将凭据写入日志、错误信息、截图或导出文件的默认元数据。
- 不把明文 CA 私钥、Cookie、Token 和目标敏感数据写入 Git。
- Artifact 设置访问权限、保留期限和删除策略。
- 导出 HAR / JSON 时支持脱敏规则预览。

### 7.5 异常与恢复

- 代理崩溃不得造成目标请求无限挂起。
- Listener 退出时，拦截队列中未处理请求必须有明确终态。
- 大响应体、慢速响应、无响应连接和异常 Chunked 必须有资源保护。
- TLS 证书生成失败、证书缓存异常和客户端证书固定必须有可诊断错误。
- Fuzzer 服务重启后恢复任务状态，不重复派发已发送 Case。
- 取消、暂停、恢复和重试必须有 Trace 与审计记录。

## 八、数据归属与既有 RIP 集成

### 8.1 不建立第二套漏洞事实库

- Proxy Session、HTTP Exchange、Repeater Tab、Fuzzer Job 属于工作台运行数据。
- HTTP 请求响应原文属于 Artifact，不是 KnowledgeCard。
- Passive Audit 结果属于候选发现。
- 正式 Finding、POC、漏洞等级、攻击链和验证事实继续以 RTIS 为准。
- ResearchTask、Workflow、ToolRun、Trace 和 Evidence 复用现有实体与接口。

### 8.2 存储决策

不要在本文直接指定新增 SQLite 或 PostgreSQL 迁移。先根据 P5-0 已冻结的运营核 / 情报核边界和当前真实迁移链，判断哪些是运行控制数据、哪些是大体积 Artifact、哪些是 RTIS 引用。

必须做到：

- 元数据与大 Body 分离。
- Body 内容寻址、哈希校验和去重。
- 数据库保存稳定 Artifact 引用，不复制大块二进制。
- History 可按 Session、Host、时间、方法和状态高效分页检索。
- 任务、请求、响应、Evidence 和 Trace 能互相定位。
- 迁移采用新增式策略，并在开发数据库副本验证。
- 不触碰生产 `platform*.db`，不直接写 RTIS 生产库。

### 8.3 导出与互操作

第一批应评估：

- HAR 1.2 导入 / 导出。
- 原始 HTTP 请求导入 / 导出。
- JSON 结构化导出。
- WebSocket 帧导出。
- Fuzzer 结果导出。
- 与现有 Tool Contract / Evidence / Artifact 的引用互操作。

导入内容必须标注来源，不能把外部数据伪装成当前代理的真实捕获记录。

## 九、分阶段实施路线

以下 WSW 是独立工作流编号，不占用 RIP3 Phase 6、7、8 的既有编号，也不改变它们的依赖关系。状态记录（2026-11-18）：WSW-0 已完成；WSW-1 本地验收完成，交付见 `docs/rip3/wsw/WSW-1-BATCH-REPORT.md`。WSW-2 当前仅完成安全基础模型 crate（`crates/wsw-runtime`），验收记录见 `docs/rip3/wsw/WSW-2-FOUNDATION-REPORT.md`；不等同于完整 Proxy/MITM Runtime，网络捕获/转发/TLS 面仍未实现。后续批次逐步核验并保持授权边界。

| 批次 | 名称 | 主要交付 | 不包含 |
|---|---|---|---|
| WSW-0 | 只读审计与 POC | 现有架构映射、依赖评估、协议兼容性 POC、数据边界 ADR | 生产代码实现 |
| WSW-1 | HTTP Message Foundation | Rust 消息模型、解析 / 序列化、Artifact Adapter、协议测试语料 | MITM UI |
| WSW-2 | Proxy / MITM Runtime | Listener、HTTP/HTTPS CONNECT、TLS CA、拦截队列、Forward / Drop | 透明代理、HTTP/3 |
| WSW-3 | HTTP History + Repeater | History 列表详情、过滤搜索、请求编辑、重放、Tab 历史 | 大规模 Fuzz |
| WSW-4 | Web Fuzzer / Intruder | Payload 位置、Mutator、Processor、Job 调度、响应差异、预算控制 | Yaklang 全语法兼容 |
| WSW-5 | WebSocket Workbench | 握手、Frame History、帧编辑和重放 | QUIC / HTTP3 |
| WSW-6 | Decoder / Comparer / Passive Audit | 编解码、Diff、被动规则、候选发现 | 第二套扫描引擎 |
| WSW-7 | Plugin / Scanner Integration | Tool / Plugin Registry 接入、Nuclei / ZAP / RTIS Adapter | 新建工具注册系统 |
| WSW-8 | Browser Evidence / Integration | 页面截图、DOM Artifact、ResearchTask / Workflow / Evidence 关联 | 用截图代替漏洞确认 |
| WSW-9 | Cross-platform / Reliability Acceptance | Linux / Windows / macOS、性能、安全、故障恢复、用户文档 | 未经验证的发布声明 |

### WSW-0 必须先完成的审计

1. 检查现有 Rust 网络栈、Tokio 运行时、HTTP Client / Server、TLS 和 WebSocket 依赖。
2. 检查现有 Application API、MCP、CLI 是否可以复用同一 Service。
3. 检查 Tool Engine、Runtime Gateway、Workflow Engine 的执行、取消和事件能力。
4. 检查 Artifact、Evidence、Trace 的真实 API 和存储位置。
5. 检查现有 Web 壳层、Code Editor、Splitter、Tabs、Virtual Table 等组件是否已有可复用实现。
6. 对 MITM、HTTP/2、WebSocket、证书管理和消息保真度建立最小 POC。
7. 对候选 crate 做维护状态、许可证、MSRV、feature、依赖树、平台支持、漏洞公告和测试覆盖评估。
8. 输出依赖比较矩阵、架构图、数据流图、威胁模型、性能基线和 ADR 草案。
   > 勘误（2026-11-18 WSW-0 评审修订批）：WSW-0 实际交付为依赖比较矩阵、威胁模型、ADR（00-05 五份审计与裁决）+ 静态资源/构建面登记（vendor 增量与构建实测，ADR-0004 §五 表）；吞吐/延迟/内存性能基线**移交 WSW-2 验收**（POC 未压测，禁止预填，ADR-0004 D5）；架构图/数据流图由 00 §三差距清单与 05 数据归属表替代承载，WSW-2 补正式图。
9. 明确哪些能力复用，哪些通过 Adapter 集成，哪些才允许自研。
10. 经设计评审后才进入 WSW-1。

## 十、成熟项目与技术选型候选

**这里只列候选，不代表已经选定。** 必须以 WSW-0 当时的官方文档、仓库活动、版本、依赖树和本地 POC 为准。

| 候选 | 适合评估的部分 | 评估重点 |
|---|---|---|
| Hudsucker | Rust HTTP/S MITM Proxy、请求响应修改、WebSocket、HTTP/2 feature | 连接生命周期、HTTP/2 兼容、Body 流式处理、稳定性 |
| http-mitm-proxy | 面向 Burp 类应用的 Rust MITM 代理 | API 设计、TLS、SSE、WebSocket 原始流能力 |
| slinger / slinger-mitm | 安全研究 HTTP Client 与 MITM 子项目 | HTTP 请求能力、原始 Socket、证书管理、项目成熟度 |
| Hyper / hyper-util | HTTP 协议基础设施 | 与代理候选的版本兼容和组合边界 |
| rustls / tokio-rustls | TLS | CA、证书缓存、客户端 TLS、平台兼容 |
| tokio-tungstenite | WebSocket | 帧处理、连接管理、协议兼容 |
| Playwright | 浏览器自动化与截图参考 | 本项目核心不直接依赖 Node 驱动，需评估 Rust 控制方案 |
| chromiumoxide / playwright-cdp | Rust Chromium 控制候选 | 浏览器版本、截图、DOM、稳定性和跨平台 |
| Nuclei / OWASP ZAP | 主动扫描 Provider | 继续走 Tool Engine / Runtime Gateway |

参考来源：

- Burp HTTP History：https://portswigger.net/burp/documentation/desktop/tools/proxy/http-history
- Burp Repeater：https://portswigger.net/burp/documentation/desktop/tools/repeater
- Burp Proxy Settings：https://portswigger.net/burp/documentation/desktop/settings/tools/proxy
- Burp Message Editor：https://portswigger.net/burp/documentation/desktop/tools/message-editor
- Yakit 官方手册：https://yaklang.com/en/products/intro/
- Yakit MITM / History：https://yaklang.com/products/mitm/History/
- Yakit Web Fuzzer：https://yaklang.com/products/expert-mode/Web%20Fuzzer/Basic-usage/
- Yakit GitHub：https://github.com/yaklang/yakit
- Hudsucker：https://github.com/omjadas/hudsucker
- Hudsucker Docs：https://docs.rs/hudsucker/latest/hudsucker/
- http-mitm-proxy：https://docs.rs/http-mitm-proxy/latest/http_mitm_proxy/
- slinger：https://github.com/emo-crab/slinger

选型硬要求：

1. 不以 Star 数作为主要依据。
2. 必须核实最近维护时间、Release、Issue、PR 和安全公告。
3. 必须确认 Linux、Windows、macOS 构建支持。
4. 必须确认 HTTP/1.1、HTTP/2、TLS、WebSocket 的真实支持程度。
5. 必须确认 Body 流式处理和大流量下的内存边界。
6. 必须核对许可证、MSRV、依赖树和 feature 最小集。
7. 先做独立 POC 和 conformance tests，再确定依赖。
8. 不为使用某个 crate 而扭曲现有领域模型或运行时边界。

## 十一、测试与验收

### 11.1 协议正确性

- HTTP/1.1 Keep-Alive、Pipeline、Chunked、Content-Length。
- Gzip、Brotli、Deflate。
- Multipart、重复 Header、重复参数、非 UTF-8 Body。
- HTTPS CONNECT、SNI、ALPN、TLS 版本和证书错误。
- HTTP/2 多路复用和流取消。
- WebSocket 握手、文本帧、二进制帧、Ping / Pong / Close。
- SSE 流式响应。
- 重定向、Cookie、压缩和字符集边界。
- 原始消息、解析消息、展示消息和发送消息之间的保真度测试。

### 11.2 MITM 行为

- 浏览器通过本地 Listener 正常访问本地测试站点。
- Intercept Off 时请求自动转发且 History 正常记录。
- Intercept On 时请求暂停，Forward 后正常完成。
- 修改请求后发送，目标收到的内容与编辑器所示一致。
- Drop 后目标不收到该请求。
- TLS 证书安装、信任、轮换、撤销和卸载流程可验证。
- Listener 停止、异常退出、客户端断开后没有挂起连接。
- 默认不接受外部网络连接，不形成开放代理。

### 11.3 Repeater / Fuzzer

- History 请求发送到 Repeater 后内容可编辑。
- 同一 Tab 多次发送保留每次请求版本和响应。
- 重定向和 Cookie 行为符合配置。
- Fuzzer 多位置组合数估算正确。
- 速率、并发、总请求数和总时长限制在后端生效。
- 暂停、恢复、取消和进程重启恢复无重复派发。
- Scope 外请求被拒绝并有可见记录。
- 结果分析可以区分响应差异、网络错误、超时和服务端限流。

### 11.4 安全测试

- DNS Rebinding。
- 重定向越界。
- IPv4 / IPv6 地址绕过。
- 私网、回环、链路本地和云元数据地址边界。
- 开放代理和未授权 Listener 访问。
- CA 私钥文件权限、导出和日志泄露。
- Cookie / Token / Authorization 脱敏。
- 超大 Header、超大 Body、慢速响应、压缩炸弹。
- 拦截队列耗尽和内存压力。
- 并发取消和服务重启。
- 插件越权、Tool Registry 绕过和 Runtime Gateway 边界。

### 11.5 UI 验收

必须使用真实后端和真实 Chromium 浏览器，不允许仅用 Mock 截图通过验收。

至少覆盖：

1. Proxy Listener 启停。
2. Intercept On / Off。
3. 请求 Forward / Drop / 修改后 Forward。
4. History 搜索、过滤和详情。
5. History 到 Repeater 的请求流转。
6. Repeater 多 Tab、编辑、发送和响应展示。
7. Fuzzer 位置标记、Payload 配置、运行和结果查看。
8. Comparer 差异展示。
9. WebSocket 帧展示。
10. Evidence 关联和截图查看。
11. Dark / Light 主题。
12. 桌面端常见视口与窄屏降级。

### 11.6 性能验收

> 勘误（2026-11-18 WSW-0 评审修订批）：WSW-0 未建立性能基线——POC 六项实测为功能覆盖面证据，未压测（ADR-0004 §五/D5："POC 已证明性能"表述禁用）；本节基线由 WSW-2 建立并作验收数字。

WSW-2 建立以下基线（验收数字），后续批次在其上记录：

- Proxy Throughput。
- 单请求代理延迟增量。
- 并发连接数。
- History 写入吞吐。
- 大 Body 内存峰值。
- 列表分页和全文检索延迟。
- Repeater 请求延迟。
- Fuzzer 调度吞吐和取消延迟。
- 长时间运行内存稳定性。

所有性能数字必须来自真实命令和可复现测试，不能在设计阶段预填目标值冒充实测结果。

## 十二、禁止事项

1. 禁止把 WebFuzzy 简化成单一目录爆破或参数 Fuzz 命令。
2. 禁止把抓包、History、Repeater、Fuzzer 做成互不关联的孤立页面。
3. 禁止用 Python / Node 实现核心代理、HTTP 消息模型、重放和 Fuzzer 调度。
4. 禁止新建第二套数据库事实源、Workflow Runtime、Agent Runtime 或 Tool Registry。
5. 禁止绕过 Tool Engine / Runtime Gateway 执行受控扫描工具。
6. 禁止绕过 RTIS 直接写漏洞事实。
7. 禁止默认安装根 CA、修改系统代理或暴露公网监听端口。
8. 禁止静默扩大 Scope、自动跟随越界重定向或在 Scope 外发起主动测试。
9. 禁止把截图当成漏洞确认依据。
10. 禁止未经 WSW-0 审计就锁定 crate、迁移、数据库或 UI 组件。
11. 禁止在当前 Phase 5 未收口前抢跑实施 WSW 代码。
12. 未经用户明确要求，不执行 commit、push、reset、clean、checkout、rebase 或其他历史改写操作。

## 十三、AI 接手执行流程

后续 AI 必须严格按以下顺序推进：

1. 读取 `AGENTS.md`、`docs/SESSION-HANDOFF.md`、`docs/HANDOVER-RIP3-CONTINUATION.md`、`docs/HANDOVER-P5-WIP.md` 和本规划。
2. 核实当前 Git 工作树、HEAD、CI、Phase 5 收尾状态，不覆盖用户已有改动。
3. 确认当前 RIP3 阶段允许开展的工作。未完成 P5-8 / P5-9 时，只能进行文档审阅和不影响当前批次的只读准备。
4. WSW-0 只读审计：追踪实际代码、Schema、API、迁移、测试和组件。
5. 输出现状矩阵：已存在、部分存在、未实现、建议复用、需要 POC。
6. 检索成熟 Rust 方案并完成最小 POC，记录版本、许可证、feature 和平台结果。
7. 输出架构图、消息数据流、UI 页面规格、威胁模型、性能基线和 ADR。
8. 设计评审通过后，才开始 WSW-1。
9. 每个批次采用：分析 → 核对来源 → 最小设计 → 实现 → 格式化 → 定向测试 → 全量门禁 → 真实浏览器验收 → 影响面检查 → 批次报告。
10. 每个批次都记录失败尝试、已知限制、回滚方式和可复现命令。
11. 只有用户明确要求时才提交或推送。

## 十四、交付完成定义

本规划文档本身完成，不代表功能完成。

整个 Web Security Workbench 只有在以下条件全部满足后才可称为交付：

- Proxy / MITM 能稳定捕获 HTTP / HTTPS 并按规则拦截。
- History 能保存、检索和查看完整请求响应。
- Repeater 能编辑、重放、保留历史并对比响应。
- Fuzzer 能构造受控变体、执行、取消和分析结果。
- WebSocket、Decoder、Comparer 和 Passive Audit 具有可用的独立入口。
- 插件和扫描器复用现有工具运行架构。
- Artifact、Evidence、Trace、ResearchTask 和 RTIS 引用闭环真实可用。
- Linux、Windows、macOS 支持矩阵有实测结果。
- 安全测试、性能测试、自动化测试和真实浏览器验收通过。
- 用户文档、安装配置、CA 管理、故障排查和数据保留策略齐备。

## 十五、现有 UI 参考图

仓库已存在以下 Yakit 参考图，后续设计和 AI 编码时应直接使用，不要让 AI 根据文字自行猜测：

| 文件 | 用途 |
|---|---|
| `docs/rip3/ui-reference/yakit/yakit-home.png` | 工具总览与模块入口 |
| `docs/rip3/ui-reference/yakit/yakit-http-history.png` | MITM / History 列表和详情 |
| `docs/rip3/ui-reference/yakit/yakit-web-fuzzer.png` | Web Fuzzer 请求编辑和结果工作区 |

参考原则：只参考信息架构、操作流程和面板组织，不复制 Yakit 品牌、配色、Electron 技术栈或 Yaklang Runtime。RIP 页面必须使用自身现有主题和组件体系。

## 十六、当前未决事项

以下问题留给 WSW-0 通过代码审计和 POC 得出结论，不在本文提前拍板：

1. MITM 主体采用 Hudsucker、http-mitm-proxy、slinger-mitm 或其他实现。
2. HTTP/2 MITM 的完整支持程度和边界。
3. CA 私钥采用系统密钥环、加密文件还是其他本地安全存储。
4. History 元数据最终落在哪个既有数据库面。
5. Body Artifact 的保留期限、最大容量和自动清理策略。
6. UI 原始报文编辑器采用现有组件还是引入成熟编辑器。
7. WebSocket 帧解析和重放使用何种成熟库。
8. HTTP/3 / QUIC 是否只做识别与透传，或后续另立项目。
9. Browser Evidence 的 Rust 控制方案和浏览器版本管理。
10. 是否需要桌面系统代理配置助手，以及各平台实现方式。

**最终原则：RIP 的目标不是给现有扫描器加一个抓包页面，而是形成一套 Rust 为核心、与研究任务和证据体系深度整合、具备 Burp / Yakit 核心工作流的 Web 安全测试工作台。**
