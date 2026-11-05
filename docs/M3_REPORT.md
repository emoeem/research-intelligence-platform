# M3 报告：Python AI Runtime（Worker Protocol v1 + 运行时 + 三 Worker）

日期：2026-11-06　状态：完成　约束遵守：stdlib-only（零第三方依赖）、未调用外部模型 API、未做 git 操作、未触碰 crates/ migrations/ apps/ legacy/

## 1. 交付清单

| 文件 | 说明 |
|---|---|
| `schemas/worker-protocol/v1.json` | 协议 v1 机器可读契约（JSON Schema draft 2020-12） |
| `schemas/worker-protocol/README.md` | 协议人读规范：分帧、握手、方法表、错误码表、budget/幂等/取消/截断语义、版本策略 |
| `workers/python/runtime/worker_runtime.py` | WorkerRuntime 基类：stdio 主循环、请求分发、线程化 execute、超时中断、预算截断、幂等缓存、stderr 结构化日志、崩溃安全 |
| `workers/python/parsing_worker.py` | 解析 Worker：md/txt/html，章节切分 + 标题树 + 词元统计（html 用标准库 html.parser 剥标签） |
| `workers/python/embedding_worker.py` | Embedding Worker：sha256 signed hashing trick，n-gram -> 256 维 + L2 归一化；留 `VectorModel` 接口与 `MODEL_REGISTRY` 作为真实模型替换点 |
| `workers/python/evaluation_worker.py` | Evaluation Worker：知识条目 -> candidate/reject/review 建议与理由；规则外置 JSON 可配 |
| `workers/python/rules/default_ruleset.json` | 默认规则集（无来源拒、CVE 入候选、证据引用数打分），host 可用 task.rules / task.rules_path 覆盖 |
| `workers/python/tests/`（6 个测试模块 + `_harness.py` + `run_all.py`） | 契约测试：对每个 worker 以子进程方式跑协议全流程 |
| `docs/M3_REPORT.md` | 本报告 |

所有路径相对 `research-platform/`。Python 实测版本：3.14.9。

## 2. 协议摘要（v1）

传输：JSON-RPC 2.0 over stdio，一行一个 JSON 对象（UTF-8），无批量。stdout 只承载协议行，日志全部走 stderr（JSON 行）。请求必须带 id（request_id），同一时刻 id 唯一。

方法（5 个）：

| 方法 | 要点 |
|---|---|
| `initialize` | 握手。protocol_version 不为 "1" 回 -32000（data 含 expected/received） |
| `get_capabilities` | methods + capabilities + initialized，随时可调 |
| `execute` | `{task, budget?, idempotency_key?}`；响应 `status=result|error|timeout` + `usage{tokens_est, elapsed_ms, output_bytes, truncated}` |
| `cancel` | `{target_request_id}`；协作式取消令牌；目标不存在回 -32007 |
| `ping` | 健康检查 + active_tasks |

错误码（顶层信封与 execute 任务级共用编码空间）：

| code | 含义 | 层 |
|---|---|---|
| -32700 | 请求行非合法 JSON | 顶层 |
| -32600 | 结构违规（jsonrpc/id 缺失、重复在飞 id） | 顶层 |
| -32601 | 未知方法 | 顶层 |
| -32602 | 参数错误（含未知 task op） | 顶层/任务级 |
| -32603 | 运行时内部错误 | 顶层/任务级 |
| -32000 | 协议版本不匹配 | 顶层 |
| -32001 | 未 initialize 就 execute | 顶层 |
| -32002 | budget 超时（status=timeout） | 任务级 |
| -32003 | budget 数值非法/超硬上限 | 顶层 |
| -32004 | 被 cancel 取消 | 任务级 |
| -32005 | 幂等键与不同 task/版本复用 | 顶层 |
| -32006 | handler 未捕获异常（worker 不退出） | 任务级 |
| -32007 | cancel 目标不存在 | 顶层 |
| -32008 | 输入不可读/超大/规则集非法 | 任务级 |
| -32009 | 解析格式不支持 | 任务级 |

分层规则：传输/握手/前置条件失败用顶层 error 信封；任务结果一律包在顶层 result 内用 status 区分。完整语义见 `schemas/worker-protocol/README.md`。

## 3. 预算、幂等、取消的实现语义（实测行为）

- 超时：`budget.timeout_ms` 由 threading.Timer 强制执行。到点先交付 `status=timeout, error.code=-32002` 的响应，再置取消令牌；任务线程是 daemon，其迟到产出被丢弃，主循环继续服务后续请求（测试验证超时后 ping 正常）。
- 截断：`budget.max_output_bytes` 约束最终响应行 UTF-8 字节数（分帧换行不计）。超限时 `data` 被替换为 `{truncated:true, original_bytes:N, data_head:"..."}` 信封，data_head 按 UTF-8 字符边界截断，信封恒为合法 JSON；`usage.truncated=true` 且 `usage.output_bytes` 精确等于交付行字节数（行内自引用字段用不动点法收敛，非估算）。
- 幂等：`idempotency_key` 结果缓存到 `workers/.cache/<sha256(key)>.json`（原子写 tmp+rename；文件名 64 位 hex，无路径注入面）。命中条件：key + task 规范化指纹（含 worker_version）一致，命中回 `cached=true` 且预算仍生效。同 key 不同 task 回 -32005。只有 `status=result` 落缓存。同 key 的流水线并发请求会等待前一任务完成（至多自身 timeout_ms）再查缓存，串行化不重复执行。
- 取消：`cancel` 置令牌立即回 `{cancelled, state}`；handler 在安全点 `ctx.check_cancelled()` 抛出取消，任务级回 -32004；handler 无视令牌时允许自然完成。超时复用同一令牌。
- 崩溃安全：handler 任何未捕获异常回 `status=error, error.code=-32006`，进程不退出，后续请求正常服务（测试验证 boom 后 ping 正常）。
- 日志隔离：stderr 结构化 JSON 行（startup/request_received/response_sent/task_timeout/task_cancelled/cache_hit/cache_write/handler_exception 等），契约测试逐行断言 stderr 为合法 JSON 且 stdout 行全部可解析。

## 4. 三个 Worker

### parsing-worker（0.1.0）
- 输入：`task.text`（内联，缺省 format=txt）或 `task.path`（按扩展名推断，可显式 format 覆盖）；输入硬上限 10 MiB。
- 输出：`format`、`source{kind,path,bytes,sha256}`、`sections[{index,title,level,start_line,end_line,content,word_count,char_count}]`、`heading_tree`（level 嵌套）、`stats{chars,lines,words,unique_words,cjk_chars,ascii_words,tokens_est,sections,headings}`。
- 词元口径：ASCII 词（小写）+ 逐个 CJK/假名字符；md 支持 ATX 与 setext 标题，文件直接以标题开头时空前导章节被丢弃；html 剥 script/style 后按 h1-h6 切章。

### embedding-worker（0.1.0）
- 输入：`text` 或 `texts`（批量 <=64），可选 `dim`（v1 仅 256）、`ngram_range`（1..3，默认 [1,2]）、`model`。
- 实现：词元 n-gram，`bucket = sha256 前八字节 % 256`，第 9 字节奇偶定符号（signed hashing），计数后 L2 归一化（零向量保持零），输出 round 到 6 位小数。确定性：同输入同输出（测试断言向量逐位相等）。
- 真实模型替换点：`VectorModel` 接口（embed(texts)->list[list[float]]，256 维已归一化、确定性）+ `MODEL_REGISTRY` 注册表；host 经 `task.model` 切换，未知名回 -32602。接真实模型只需实现接口并注册，协议层 budget/幂等/截断零改动；模型升级递增 worker_version 即失效旧幂等缓存。

### evaluation-worker（0.1.0）
- 输入：`entries`（<=500 条，字段宽松：source/sources、evidence_refs/evidence_ref_count、title/content），规则三选一：`task.rules` 内联 / `task.rules_path` 文件 / 缺省 `rules/default_ruleset.json`。
- 判定顺序：reject_if 命中 -> reject；否则 candidate_if 命中 -> candidate；否则分数阈值兜底（>= candidate 阈值 -> candidate，<= reject 阈值 -> reject），否则 review。
- 信号：source_count、cve_count（CVE-\d{4}-\d{4,7} 正则抽取）、evidence_ref_count、content_chars；score 为加权归一和（默认权重 source .3 / cve .2 / evidence .3 / content .2，evidence 按 refs/3 归一，content 按 chars/2000 归一）。
- 输出：每条 `{index, entry_id, suggestion, score, signals, matched_rules, reasons[]}` + `summary{candidate,reject,review}` + `ruleset_version`。建议仅供参考，不直接改写知识状态（符合 M3 "AI 输出默认是 Candidate/Suggestion" 约束）。

## 5. 契约测试与实测输出

运行方式（两种入口等价）：

```
cd research-platform/workers/python/tests && python3 run_all.py
cd research-platform/workers/python && python3 -m unittest discover -s tests
```

用例分布（109 例，全部对 worker 以子进程方式经 stdio 执行，非进程内直调）：

| 模块 | 用例数 | 结果 |
|---|---|---|
| test_parsing_contract（协议全流程） | 25 | ok |
| test_embedding_contract（协议全流程） | 25 | ok |
| test_evaluation_contract（协议全流程） | 25 | ok |
| test_parsing_functional | 11 | ok |
| test_embedding_functional | 10 | ok |
| test_evaluation_functional | 13 | ok |
| 合计 | 109 | 全部通过 |

每个 worker 的契约面（25 例）覆盖同一矩阵：initialize 成功/版本不匹配 -32000/缺版本 -32602、未初始化 execute -32001、get_capabilities、ping、未知方法 -32601、非 JSON 行 -32700、缺 jsonrpc/缺 id -32600、execute 主路径（响应结构与 usage 字段校验）、output_bytes 与交付行字节数精确一致、budget 非法两例 -32003、预算超时（status=timeout + -32002 + 超时后存活）、cancel 在飞任务（-32004 + 存活）、cancel 未知目标 -32007、幂等命中（cached=true + 数据一致 + 缓存文件落盘）、幂等冲突 -32005、幂等键类型校验、截断（行字节数 <= 预算 + truncated 信封）、boom 崩溃安全（-32006 + 存活）、未知 op -32602、debug op 生产环境门禁（无 WORKER_ALLOW_DEBUG 时拒绝）、stderr 结构化日志/stdout 纯净。

实测输出（2026-11-06，Python 3.14.9，连续两轮全绿，第二轮复跑确认无时序 flake）：

```
$ python3 tests/run_all.py
Ran 109 tests in 6.891s

OK

SUMMARY tests=109 failed=0 errors=0 failures=0 skipped=0 ok=True
（第二轮复跑：Ran 109 tests in 7.385s，OK）
```

## 6. budget 验证结论（逐项）

| 项 | 验证方式 | 结论 |
|---|---|---|
| 超时 | 三 worker 各 1 例：sleep 8000ms + timeout_ms=300 | verified：约 300ms 交付 status=timeout/-32002，daemon 任务被弃置，worker 随后 ping 正常 |
| 截断 | 三 worker 各 1 例：大输出 + max_output_bytes=1400 | verified：交付行 <= 1400 字节，truncated 信封含非空 data_head，output_bytes 精确等于行字节数，worker 存活 |
| 幂等命中 | 三 worker 各 1 例：同 key 同 task 连发两次 | verified：第二次 cached=true，data 逐字段一致，`<sha256>.json` 缓存文件存在 |
| 幂等冲突 | 三 worker 各 1 例：同 key 改 task | verified：-32005 |
| 取消 | 三 worker 各 1 例：sleep 60s 后 cancel | verified：cancel 回 state=running，execute 回 -32004，worker 存活 |
| 崩溃隔离 | 三 worker 各 1 例：boom 抛异常 | verified：-32006 不退出，后续请求正常 |

## 7. M3 退出门槛对照

| 门槛 | 状态 | 依据 |
|---|---|---|
| Worker 与数据库解耦 | 达成 | worker 进程零 DB 访问，输入输出全走 v1 协议 |
| Worker 输出可追踪、可重放或可复核 | 达成（协议层） | 响应含 worker_version/usage/cached；幂等键可重放同结果；stderr 事件链可审计。运行记录入库留待 M4 检索接入时由 host 侧落库 |
| Worker 异常不会破坏核心事务 | 达成 | 崩溃安全测试 + worker 无核心库写权限 |
| 未验证的 AI 输出不能覆盖已验证知识 | 达成（本层） | evaluation 输出仅为 suggestion；协议无任何写回通道 |
| Worker 版本升级具备兼容性测试和回滚方案 | 部分达成 | 版本不匹配 -32000、幂等指纹含 worker_version、回滚 = 按版本拉起旧 worker 进程；跨版本兼容矩阵尚未建（见限制） |

## 8. 已知限制

1. embedding 为确定性占位实现：hashing vectorizer 无语义能力，仅可用于索引/管线联调与回归基线；真实模型经 `VectorModel`/`MODEL_REGISTRY` 接入，接真实模型会引入第三方依赖，超出本阶段 stdlib-only 约束。
2. tokens_est 是 chars/4 启发式估计，非真实 tokenizer；成本核算需在 host 侧换算。
3. 取消与超时是协作式：handler 无视令牌时任务可能自然完成；超时任务的 daemon 线程可能继续占用内存直至自然结束（产出被丢弃）。硬杀需 host 终止整个 worker 进程。
4. 幂等缓存只在本进程内存 + 磁盘文件层生效，无 TTL/容量上限/清理策略（依赖 host 控制缓存目录）；缓存目录可经 WORKER_CACHE_DIR 重定向。
5. 并发模型：execute 多任务并发（线程），但 CPython GIL 下 CPU 密集任务相互挤占；需要隔离时 host 应拉多进程 worker 并做负载均衡。
6. 协议无批量请求、无压缩、无分块续传；单请求行上限受主计划内容大小约束约束于输入校验（各 worker 有输入上限），但请求行本身无长度预检。
7. debug op（sleep/boom）由 WORKER_ALLOW_DEBUG=1 门禁，仅用于测试；生产环境不设该变量时按未知 op 拒绝（已测试）。
8. Worker 运行记录（M3 计划书"每次执行记录..."）目前以响应字段 + stderr 日志形式存在，结构化落库由 host/Rust 侧在 M4 接入时完成。
9. reranking worker 未在本阶段实现（主计划 M3 交付物含 Parsing/Embedding/Reranking；rerank 依赖真实模型才有意义，随真实 embedding 一并接入）。
