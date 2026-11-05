# Worker Protocol v1

版本：1（2026-11-06）　机器可读契约：同目录 `v1.json`（JSON Schema draft 2020-12）　参考实现：`workers/python/runtime/worker_runtime.py`

## 1. 传输与分帧

- JSON-RPC 2.0 over stdio。**一行一个 JSON 对象**（newline-delimited），UTF-8，无长度前缀，无批量（batch）。
- Worker 的 stdout 只承载协议行；所有日志走 stderr（结构化 JSON 行），二者互不污染。
- stdin EOF 即优雅退出（exit 0）。SIGTERM/SIGINT 触发优雅关闭。
- 每个请求 REQUIRED 携带 `id`（即 request_id，string/integer/null）。本协议不支持无 id 的 notification：缺 id 返回 -32600（id=null）。
- 同一时刻 request_id 必须唯一：重复的在飞 id 返回 -32600（已完成的 id 可以复用）。

## 2. 方法清单（5 个）

| 方法 | params | 响应 result | 说明 |
|---|---|---|---|
| `initialize` | `{protocol_version, client?}` | `{protocol_version, worker_name, worker_version, capabilities}` | 握手第一步。版本不匹配返回 -32000 |
| `get_capabilities` | 无 | `{protocol_version, worker_name, worker_version, methods, capabilities, initialized}` | 可随时调用，幂等只读 |
| `execute` | `{task, budget?, idempotency_key?}` | `{worker_name, worker_version, protocol_version, status, cached, data?, error?, usage}` | 核心执行方法，budget/幂等/取消只作用于它 |
| `cancel` | `{target_request_id}` | `{cancelled, state, target_request_id}` | 协作式取消：置取消令牌，目标任务自行检查后退出 |
| `ping` | 无 | `{pong, worker_version, uptime_ms, active_tasks}` | 健康检查，任何阶段可用 |

## 3. 握手流程

```
host                                        worker
 |  {"jsonrpc":"2.0","id":1,"method":"initialize",        |
 |   "params":{"protocol_version":"1"}}                   |
 |------------------------------------------------------->|
 |  result{protocol_version:"1", worker_name,              |
 |         worker_version, capabilities:[...]}             |
 |<-------------------------------------------------------|
 |  {"id":2,"method":"get_capabilities"}                   |
 |------------------------------------------------------->|
 |  result{methods:[...], capabilities:[...], ...}         |
 |<-------------------------------------------------------|
 |  {"id":3,"method":"execute","params":{task,budget,...}} |
 |<------------------------------------------------------>| (worker 协作)
```

- `initialize` 的 `protocol_version` 不等于 "1"：返回顶层 error -32000，data 含 `expected`/`received`。
- `execute` 在 initialize 之前调用：顶层 error -32001。
- 主版本不兼容（未来 v2）时 worker 应拒绝握手，由 host 决定降级或终止。

## 4. execute 语义

### 4.1 响应三态

`result.status` 取值：

- `result`：成功，`data` 为 worker 输出对象（或截断信封）。
- `error`：任务级失败，`error` = `{code, message, data?}`，进程不退出（崩溃安全）。
- `timeout`：预算超时，`error.code = -32002`，任务线程被置取消令牌后遗弃（daemon 线程），其迟到的产出被丢弃。

### 4.2 budget

```json
"budget": {"timeout_ms": 30000, "max_output_bytes": 1048576}
```

- `timeout_ms`：[1, 600000]，默认 30000。超限返回顶层 error -32003。
- `max_output_bytes`：[256, 67108864]，默认 1048576。约束**最终响应行的 UTF-8 字节数**。超限时 `data` 被替换为截断信封：

```json
{"truncated": true, "original_bytes": 81234, "data_head": "{\"sections\":[...前缀..."}
```

`data_head` 是原始 data JSON 的字节前缀按 UTF-8 字符边界截断后的字符串，信封恒为合法 JSON。`usage.truncated=true`。error/timeout 响应超预算时按同样策略收缩 message，最终兜底为 id=null 的最小错误行。

### 4.3 usage

`{tokens_est, elapsed_ms, output_bytes, truncated}`。`tokens_est = ceil(序列化 data 字符数 / 4)`，是启发式估计，不是真实 tokenizer。`output_bytes` 是实际交付行字节数。

### 4.4 幂等

- 携带 `idempotency_key` 的 execute：结果写入 `<workers>/.cache/<sha256(key)>.json`（原子写）。
- 命中条件：key 相同 且 task 规范化指纹相同 且 worker_version 相同。命中返回 `cached=true`，elapsed_ms≈0，预算仍生效（可再次截断）。
- 同 key 的并发（流水线）请求：后者会等待前者完成后再查缓存，同 key 任务被串行化，不重复执行。等待上限为后请求自己的 timeout_ms。
- key 相同但 task 或 worker_version 不同：顶层 error -32005（IDEMPOTENCY_CONFLICT）。
- 只有 `status=result` 才写缓存；error/timeout/cancelled 不缓存。

### 4.5 取消

- `cancel` 置目标任务取消令牌并立即返回 `{cancelled, state}`：state=running 表示令牌已置；state=finished 表示任务已结束（cancelled=false）。目标不存在返回顶层 error -32007。
- 取消是**协作式**：worker handler 通过 `ctx.check_cancelled()` 在安全点抛出取消，运行时回 `status=error, error.code=-32004`。handler 无视令牌时任务可能正常完成并交付结果。
- 超时同样复用取消令牌（先交付 timeout 响应）。

## 5. 错误码表

顶层 JSON-RPC error 与 execute 内 `error.code` 共用同一编码空间：

| code | 名称 | 出现层 | 含义 |
|---|---|---|---|
| -32700 | PARSE_ERROR | 顶层 | 请求行不是合法 JSON（id=null） |
| -32600 | INVALID_REQUEST | 顶层 | jsonrpc 字段缺失/不为 2.0、缺 id 等结构违规 |
| -32601 | METHOD_NOT_FOUND | 顶层 | 未知方法 |
| -32602 | INVALID_PARAMS | 顶层/任务级 | 参数缺失或类型错误（含未知 task op） |
| -32603 | INTERNAL_ERROR | 顶层/任务级 | 运行时内部错误 |
| -32000 | PROTOCOL_VERSION_MISMATCH | 顶层 | initialize 版本不匹配，data 含 expected/received |
| -32001 | NOT_INITIALIZED | 顶层 | initialize 之前调用 execute |
| -32002 | BUDGET_TIMEOUT | 任务级(status=timeout) | 超过 budget.timeout_ms |
| -32003 | BUDGET_INVALID | 顶层 | budget 数值非法或超硬上限 |
| -32004 | CANCELLED | 任务级 | 任务被 cancel 取消 |
| -32005 | IDEMPOTENCY_CONFLICT | 顶层 | 同 key 不同 task/worker_version |
| -32006 | EXECUTION_ERROR | 任务级 | handler 未捕获异常（worker 不退出） |
| -32007 | UNKNOWN_REQUEST | 顶层 | cancel 的目标请求不存在 |
| -32008 | INPUT_ERROR | 任务级 | 输入不可读/超大/规则集非法 |
| -32009 | UNSUPPORTED_FORMAT | 任务级 | 解析格式不支持 |

分层规则：**传输/握手/前置条件失败用顶层 error 信封**（`{"jsonrpc","id","error"}`）；**任务执行结果（result/error/timeout）一律包在顶层 result 信封内**，用 `status` 区分。这样 host 对"请求送达了没有"与"任务成了没有"可以分别判定。

## 6. 日志与可观测性

stderr 输出 JSON 行：`{"ts","level","worker","event",...}`。关键事件：`startup`、`request_received`、`response_sent`（含 status/elapsed/output_bytes）、`task_timeout`、`task_cancelled`、`cache_hit`、`cache_write`、`handler_exception`、`shutdown`。stdout 绝不出现日志。

## 7. 运行记录与审计字段

每次 execute 的响应自带 `worker_version`、`usage`、`cached`，加上 stderr 日志，可满足主计划 M3 "每次执行记录模型、版本、参数、耗时、错误" 的最低闭环；输入引用由各 worker 的 `data.source`（sha256、路径、字节数）承载。

## 8. 版本策略

- 协议版本只有主版本号字符串（"1"）。只加字段不加语义 = 主版本不变；改既有字段语义 = 升主版本并拒绝旧客户端（-32000）。
- `worker_version` 为语义化版本，幂等缓存按其参与指纹，升级即天然失效旧缓存。

## 9. 测试钩子

运行时内置两个 debug op（`{"op":"sleep","ms":N}`、`{"op":"boom"}`），仅在环境变量 `WORKER_ALLOW_DEBUG=1` 时启用，供契约测试驱动超时/取消/崩溃路径。生产环境不设该变量，未知 op 一律 -32602。契约测试见 `workers/python/tests/`。
