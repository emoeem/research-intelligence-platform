# RIP 工作区页面设计规格

> 状态：M1 设计冻结候选。
> 通用规则：所有页面遵循 `design/DESIGN-SYSTEM.md`、`design/APP-SHELL-SPEC.md`、`design/RESPONSIVE-SPEC.md`。本文件定义业务页面信息层级，不决定 API 新增字段。

## 1. Dashboard

目标：一眼看懂当前需要处理什么、系统是否健康、最近发生什么。顶部简要标题和数据更新时间；第一行 4-6 个真实 KPI，点击进入对应列表；中段左侧为待处理队列，右侧为运行状态和异常；底部为近期活动时间线。只保留能回答具体问题的图表，例如知识生命周期分布，不能用一张巨大柱图填充页面。删除重复功能地图或将其收进“所有工作区”导航页。

## 2. Projects

目标：管理研究项目和项目任务树。页面头部包含项目搜索、筛选、创建入口（仅 API 已支持才出现）；主区为表格/卡片切换；行显示名称、状态、目标、任务计数、最近更新时间；详情采用 Inspector 或独立详情页。不能把 Project 与 Workspace 混为一谈。

## 3. Research

研究工作区包括研究总览、任务、资产、知识上下文、验证计划、证据和覆盖矩阵。总览显示当前任务状态、范围、最近证据和需要动作；任务列表保持高密度；任务详情以时间线、资产、知识、计划、Evidence、Trace 组织；Coverage 以矩阵和盲区解释为主，不用装饰图表代替实际覆盖关系。

## 4. Knowledge

知识库主页：搜索/过滤工具栏、生命周期/质量/来源指标、可排序表格、分页、右侧详情。列至少包括标题/来源/生命周期/质量或验证状态/更新时间。详情要分开来源、内容、验证、引用、关联资源。文档中心与 Knowledge Catalog 用导航区分，不显示重复列表。

## 5. Tools

工具目录以 DataTable 和分类过滤为主。工具详情依次展示身份/版本、描述、输入输出 Schema、权限与治理、关联知识、运行记录。试运行入口只有后端能力、权限策略和安全确认完成后显示。

## 6. Plugins

插件目录展示类别、来源、版本、信任级别、生命周期和治理状态；详情展示 Manifest、能力、权限、依赖、审计 trail。安装、升级、禁用、卸载必须沿用现有 TrustConfirmDialog 和服务端策略，不能因 UI 重构简化确认等级。

## 7. Agent Studio

三栏编辑工作台：左侧 Agent 列表/版本；中央定义、指令、能力绑定与配置；右侧 Inspector 显示权限/引用/Memory；Activity Dock 展示真实 AgentRun、步骤、Trace、Memory 和输出。编辑草稿与已发布版本区分。

## 8. Workflow Studio

保留 @xyflow/react 现有画布。页面由 Node Palette、Canvas、Node Inspector、Run Panel、Version List 组成。画布最大化；Inspector 和 Run Panel 可折叠调整。运行、暂停、恢复、重试、取消必须显示服务端真实状态及 checkpoint。移动端提供只读/简化导航。

## 9. Runs

默认列表：状态、工具/Agent/Workflow 类型、开始时间、耗时、Caller、Trace；状态过滤和分页。详情分为 Summary、Timeline/Steps、Logs、Artifacts、Inputs/Outputs、Trace links。敏感输出在详情中遵循权限和脱敏规则。

## 10. Runtime

展示 Runner/节点健康、心跳、能力、版本、执行槽位和错误。未知状态必须明确显示 unknown/stale，不渲染假绿灯。节点详情用 Inspector 展示配置和关联任务。

## 11. Settings

设置侧栏分类：General、Appearance、Workspace、Security、API/Connection、Runtime、Storage、Advanced。按表单分组；每组有保存状态、验证错误和重置入口。安全设置需要确认，不将系统当前状态伪装成可编辑配置。

## 12. Web Security Workbench

工作区内含 Proxy/MITM、HTTP History、Repeater、Fuzzer、WebSocket、Intruder Jobs、Decoder、Comparer、Passive Audit、Plugin Center、Proxy Settings。保持现有两条 History/Repeater 路由与 Proxy/Cert 页面路由不变；其余页面若后端/API 尚未实现，只显示清晰的“尚未实现/不可用”状态，不加入虚假按钮或假结果。

### HTTP History

全宽 Data Workbench。顶部是搜索、Filter Builder、Scope、Columns、Export；中部是游标分页的虚拟表格，列含 Method、Host、Path、Status、Length、Duration、Source、Time、Scope；底部为可拖动双栏 Request/Response Viewer。未选中记录时显示真实 Empty State 提示启动 Proxy 或发送 Repeater 请求。选中一条时，详情应填满可用高度，Body 按需加载，保留 8MiB 上限和脱敏投影。默认隐藏通用 Inspector。

### Repeater

顶部是 Repeater 请求 Session Tabs；左侧 Request Editor，右侧 Response Viewer；底部/顶部有 Send、Cancel、历史版本、重定向、Timeout 和 fidelity 提示。请求草稿支持版本历史。Response 区显示状态、协议、耗时、TTFB、长度和视图 Tab。小屏切换 Request/Response 而不是压缩双栏。

### Proxy / MITM

顶部 Listener 状态和 Start/Stop/Intercept 控制；中部拦截队列 + 请求/响应编辑；右侧规则/Scope/证书 Inspector；底部 Listener 日志。未启动时明确引导配置和测试连接，不伪装连接数。

### Web Fuzzer / Intruder

四区结构：请求、Payload/Positions、运行配置/预算、结果。运行控制清楚显示范围、请求数量估算、并发/速率预算、暂停/取消和最终统计。必须呈现 Scope 与权限策略，任何未实现的执行能力不得显示为有效按钮。

### WebSocket / Passive Audit

连接/会话列表、帧/事件时间线、详情查看、规则命中与 Request/Trace/Evidence 关联。实时事件流使用后端已提供的 SSE/WS 能力或受控轮询，不在页面里自行发明事实源。

### Decoder / Comparer

Decoder 为输入/输出双栏、格式选择、转换链、复制和发送到 Repeater。Comparer 为双栏差异，支持选择两份真实消息、差异导航和原文定位。

### Plugin Center / Proxy Settings

WSW Plugin Center 复用现有 Plugins Registry、TrustConfirmDialog 和 Runtime；Proxy Settings 复用 Settings 表单体系，显示 Listener、证书、上游代理、Scope 和捕获策略。后端未支持的写入控件必须禁用并解释。
