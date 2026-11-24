# Web Security Workbench 页面设计规格

> M1 设计冻结文档。生产实现必须遵循统一 Design System、AppShell、Navigation 与 Responsive 规范。
> 业务数据必须来自现有 API；本规格不授权新增 API 或使用 Mock 生产数据。

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
