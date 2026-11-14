# rmcp 客户端端到端验收结果（2026-11-14）

> 真实 MCP 客户端（独立 Node 实现，仅 node: 内建模块零 npm 依赖）对 rmcp
> 版 rp-mcp 的 stdio 全链验收。审计 §5 任务 9 的留证目录：脚本
> [rmcp_client_e2e.mjs](rmcp_client_e2e.mjs)（可运行产物）+ 会话日志
> [e2e-session-2026-11-14.jsonl](e2e-session-2026-11-14.jsonl)（114 行逐条
> client-send / assert / step-ok 记录）。结论：**10 步全过，进程退出码 0**。

| 项       | 值                                                              |
| -------- | --------------------------------------------------------------- |
| 状态     | 通过（10/10 步，summary ok=true）                                |
| 日期     | 2026-11-14                                                       |
| 对象     | target/debug/rp-mcp（rmcp 3.10.0 server + stdio transport）       |
| 客户端   | Node 22.19.0 独立脚本（与服务端 SDK 无共享代码，协议面独立实现）  |
| 前置     | `cargo build --offline -p mcp-server --bin rp-mcp` 后仓库根执行   |
| 复跑命令 | `node docs/phase9/rmcp-client-e2e/rmcp_client_e2e.mjs`            |

## 十步验收面（与会话日志 step-ok 一一对应）

1. **initialize 版本协商**：客户端请求 2024-11-05，服务端协商回显
   `2024-11-05`（P1 整改：不再无脑回显任意客户端版本）；capabilities.tools
   listChanged=false、serverInfo.name=rp-mcp、一行导航 instructions
   （<200 字符）、响应零工具 Schema 字段。
2. **notifications/initialized 静默**：通知后 300ms 观测窗 stdout 零新行。
3. **版本协商两路**：2025-11-25（rmcp 支持列表内最新握手版本）协商回显；
   1999-01-01（列表外）回落服务端声明版本 2025-11-25，不假装支持。
4. **tools/list 多页**：2 页 28 条（能力面 6 + 注册面 22，生产
   migrations/platform.db 主库 + dev-b5 aux 同源读面）；每条 Tool 对象含
   标准必需 name 与 inputSchema（P0 整改断言，逐条逐字段）。
5. **tools/call 成功**：rip.get_tool_schema rip.kb.rtis，isError=false，
   input_schema.properties.q 与 dev-b5 tool 表 input_schema 列同源。
6. **tools/call 结构化失败**：rip.run_tool 未登记工具，isError=true +
   ok=false + unknown tool（D-TC-05 拒绝不落行语义，不伪造成功）。
7. **未知工具**：rip.nothere 返回 JSON-RPC error code -32602。
8. **通知与超时处理**：未知通知（notifications/unknown-notify、
   notifications/progress）静默零响应；客户端 3s 超时口径下 ping 照常应答
   （服务不被通知流卡死）。
9. **stdout/stderr 分离**：stdout 9 行全部为合法 JSON-RPC 消息零坏行；
   stderr 含 `[rp-mcp] ready` 日志且零协议 JSON 泄漏。
10. **进程退出**：stdin EOF 后服务收尾，退出码 0。

## 三态令牌边界（脚本外的同批留证）

令牌三态由 `apps/server/selftest_phase9_mcp.sh` §4 与
`crates/mcp-server/tests/mcp_protocol.rs::token_boundary_three_states`
承载（同一二进制）：缺失 / 空白 -> exit 2 fail-closed 拒启，正确注入 ->
正常服务。本脚本以正确态运行（宿主注入授权边界，无协议认证字段）。

## 覆盖缺口（如实登记）

- rip.run_tool 真实 worker 执行链未在本脚本重复（cargo tests/run_tool_e2e.rs
  与 parity op3/4/5 已全链覆盖，本脚本只打未登记拒绝面）。
- 截图不适用（stdio 进程面无 UI）；留证为逐行 JSONL 会话日志。
