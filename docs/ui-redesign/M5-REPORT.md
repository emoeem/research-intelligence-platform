# M5 Report — Web Security Workbench

日期：2026-11-24
状态：**前端工作台实现完成；后端安全契约保持不变**

## 页面交付

| 工作区 | 路由 | 真实数据/动作 |
|---|---|---|
| HTTP History | `/websec/history` | `/api/wsw/history` 游标分页、过滤/搜索、详情与 Body 按需加载、标签/备注、发送到 Repeater |
| Repeater | `/websec/repeater` | Tab 会话、请求草稿/版本、发送/取消/状态轮询、Response 六视图 |
| Proxy / MITM | `/settings/wsw-proxy` | Listener 状态、拦截队列、现有后端 Start/Stop/Intercept 受保护操作 |
| CA | `/settings/wsw-cert` | 证书状态/指纹与既有显式导出/重生成策略 |
| Intruder / Fuzzer | `/websec/fuzzer` | 统一复用 Rust FuzzSpec；Sniper/Cluster Bomb/Pitchfork、预算/Scope 确认、Job/Case 状态、取消 |
| WebSocket | `/websec/websocket` | 帧列表游标、详情；Payload 按需读取，不在列表预加载 |
| Decoder | `/websec/decoder` | `/api/wsw/decode` 后端转换链 |
| Comparer | `/websec/comparer` | `/api/wsw/compare` 服务端比较；本地文本预览明确标记为预览 |
| Passive Audit | `/websec/passive` | 规则注册表与命中游标只读视图 |

## 安全与数据纪律

- 不创建新的 Listener、代理、扫描或绕过 Scope 的执行路径。
- Repeater 仅经既有受保护 API 发送；保留取消和服务端状态回查。
- HTTP Body 仍按需读取，遵守后端 8 MiB 限制、哈希校验和脱敏投影。
- Fuzzer 创建前显示 Scope 与预算确认；Rust 引擎再次验证 FuzzSpec。
- WebSocket Payload 仅在选中详情时加载。
- Plugin Center 复用 Plugins Registry 与 TrustConfirmDialog，不降低确认等级。
- 未实现的 API 能力禁用或说明，不渲染假成功。

## 验证

当前 Web 全量 Vitest、TypeScript、Prettier、生产构建和 Playwright visual/axe gate 包含 WSW 页面。`wsw-workspaces.test.tsx` 覆盖 History 空态、Passive 规则、WebSocket 延迟 Payload、Fuzzer Scope 确认、Decoder 与本地 Comparer 预览；已有 WSW browser E2E 脚本继续保留。最终全量门禁：Vitest 21 files / 149 tests；Playwright 78/78；typecheck、Prettier、production build、git diff --check 全部通过。M5 CLOSED。
