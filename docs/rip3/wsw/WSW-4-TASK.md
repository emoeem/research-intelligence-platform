# WSW-4 任务书 · Web Fuzzer / Intruder（Payload 位置、Mutator、Processor、Job 调度、响应差异、预算控制——Rust 引擎批，UI 不入批）

- 批次：WSW-4（PLAN §九 :606：主要交付 = Payload 位置、Mutator、Processor、Job 调度、响应差异、预算控制；**不包含 = Yaklang 全语法兼容**）
- 日期：2026-11-22
- 状态：任务书已定稿（2026-11-22 侦察后落稿；实施按本任务书执行，走 REDLINES 八步循环，显式 add 本批清单禁 add -A）
- 依据：[WEB-SECURITY-WORKBENCH-PLAN.md](../WEB-SECURITY-WORKBENCH-PLAN.md) §4.4（:218-272）/ §5.5（:417-426）/ §6.1-6.4（:444/:454/:465/:471/:502-503）/ §7.2-7.5（:523-528/:537/:556）/ §8.1（:563-566）/ §11.3（:694-703）/ §11.6（:751/:754）/ §十二（:758-768）+ [WSW-CONTINUATION-REDLINES.md](WSW-CONTINUATION-REDLINES.md)（八步循环 + 九条红线）+ [WSW-3-ADR.md](WSW-3-ADR.md)（W3-D4 发送引擎 / W3-D6 Outbound 语义 / W3-D7 写权分治）+ [WSW-3-TASK.md](WSW-3-TASK.md)（形态参照；其非目标 :21 明文"Web Fuzzer/Intruder（W4，PLAN :606）"移交本批）+ [05-DESIGN-AND-THREAT-MODEL.md](05-DESIGN-AND-THREAT-MODEL.md)（Outbound/表归属面）
- 前置：WSW-3 已收口（025 入链、`sender.rs` 发送引擎、History/Repeater 两页与 API）；MA-2 已收口（027 入链、caller_budget 面）；CB-1 已收口（HEAD `ae23a9d8`）；028 迁移空位在且**本批立项**（D2）
- 设计裁决：本批特有裁决见 §0（**不另立 ADR**——裁决内嵌本档，先例 CB-1-TASK §0；与 PLAN 分歧以 PLAN 为准并回改本档，WSW-3-TASK 头注同款）
- 现状锚点：§1 A1-A27 均为 2026-11-22 本会话实测行号（HEAD `ae23a9d8`、工作树干净 `git status --porcelain`=0 实测），实施时若行号漂移按函数名重定位并以本档词形为准

---

## 0. 设计裁决（本批特有，ADR 内嵌）

| # | 裁决 | 理由 |
|---|---|---|
| D1 | **发送引擎复用单点**：fuzz case 发送唯一路径 = `wsw_runtime::sender::send_request`（sender.rs:720，签名 `(&ScopeRules, &OutboundRequest, &SendPolicy, &[PathBuf], &watch::Receiver<bool]) -> Result<SendOutcome, SendError>`）；并发 = JobRunner 以 tokio 任务 + `Semaphore(max_concurrency)` 调用**同一函数**，单 case 单连接语义沿用（sender.rs 每 loop 一轮一连接）；`sender.rs` / `scope.rs` **逐字节零 diff**（不进本批落点清单）；fuzz 产 exchange 行的 listener_id 用常量 `"wsw-lis-fuzzer"`（rp-wsw send `"wsw-lis-repeater"` 常量先例 rp-wsw.rs:505） | 红线"不建第二发送引擎"（PLAN :760 Fuzzer 调度禁 Python/Node + :465 Fuzz Engine 与 Request Sender 分立模块边界）；W3-D4.2 发送路径唯一事实 = `serialize_request`（sender.rs:4-11 doc）；第二套连接/重定向/超时实现 = 保真与 scope 闸双重复制，W2/W3 验收面全部作废 |
| D2 | **028 立项两表**：`migrations/028_wsw_fuzz.sql` 纯新增 `wsw_fuzz_job`（九态词表 CHECK）+ `wsw_fuzz_case`（七态词表 CHECK）+ 索引；链头 027→028 **单文件单版本**推进，纯新增零 ALTER，`schema_version` INSERT OR IGNORE (28,…) + `PRAGMA user_version=28` 双写（025:170-181 先例同款）；payload/position/mutator/processor 全部配置落 job 行 `spec_json`（8MiB 帽同 025 request_json 口径，**模板不立独立表**——无 UI 登记面，spec JSON 即可复用模板，独立模板表留 UI 批裁决）；链尾十五硬钉文件随更预登记（A26 清单，钉值形态四类 = vec 链清单 / 链尾断言 / `001..0NN` 注释 / user_version 断言，WSW-3 S2 实勘口径） | PLAN :556"服务重启后恢复任务状态，不重复派发已发送 Case"→ Job/Case 必须持久化（内存态不满足判据 8）；`wsw_exchange.source` CHECK **已含 'fuzzer'**（025:54）无需动 025；编号冻结规则顺延（migration_dev_copy_p7_3.rs:76 注释原文）；028 空位实测（migrations/ 头 027、dev-b1 user_version=27） |
| D3 | **Payload 位置 = 标记面 + 选择器面双轨，Yaklang 兼容 = 语义面非语法面**：①标记面 = 请求模板（OutboundRequest JSON）内 `§…§`（U+00A7）成对字节区间标记，解析纯函数落 `fuzz/spec.rs`；未闭合/嵌套/交叉标记 → job 预检拒绝 fail-closed；②选择器面 = 结构化命名位置 `query=<name>` / `header=<name>` / `cookie=<name>` / `form=<name>` / `json=<pointer>` / `path_segment=<n>` / `multipart=<field>`，引擎直接定位 OutboundRequest 对应字段值字节（不经标记改写）；多位置组合 = 同模板多标记多选择器并存。**Yaklang 裁决原文**：PLAN :606 不包含列"Yaklang 全语法兼容" + :321"不引入 Yaklang Runtime 作为平台核心依赖" + :34"不逐项复制、不引入 Yaklang Runtime"——本批对齐的是 PLAN §4.4 的**语义四面**（位置类型谱 :224-233 / 处理链 :237-243 / 运行模式 :248-253 / 结果分析 :256-263），**不实现 Yaklang `{{…}}` fuzz-tag 模板引擎全语法**（表达式求值/tag 函数库/Yak 代码内嵌）。标记符选 `§` 非 `{{}}`：SSTI/模板注入 payload 本身含 `{{…}}`（`{{7*7}}` 是核心 fuzz 用例），`{{}}` 作标记必然与 payload 冲突，Burp Intruder `§…§` 成对分隔符无此冲突 | PLAN :220"保留 Burp Intruder 的可理解操作方式"；WSW-1 serialize 不变式"编辑头序/字节保持"要求标记语法不得侵蚀 payload 字节面；fail-closed 同 serialize 入出守卫纪律（serialize.rs:17-19） |
| D4 | **Mutator/Processor = 声明式链 JSON**（spec 内 `mutators`/`processors` 有序数组，逐项 `{type, params}`）：Mutator 生成器 = 手工列表 / 本地字典文件（逐行，空行/`#` 注释跳过）/ 数字·字母·日期范围生成器（PLAN :237-239）；变换 = 编码链（URL/Base64/Base64URL/Hex/HTML Entity——PLAN §4.6 :287 词表对齐）/ 前缀·后缀·大小写·替换·拼接·正则替换（:240-241）；Processor = payload 注入后的请求侧后处理（头值合法性复走 serialize 入出守卫、CL/TE 重定界走 `serialize_request` 既有面，**不发明第二重定界**）。正则面（变换 :241 + 结果匹配 :258）走 `regex` crate **传递依赖转正直接声明**（lock `:2229-2231` 版本 1.13.1 在位，等号钉版；WSW-1 sha2/base64 转正先例） | PLAN :242"可扩展 Mutator / Processor" = 扩展点类型化（type 词表 + params），非 Yaklang 表达式求值（D3 排除）； Processor 与 serialize 单点纪律（D1）不可分——注入后重定界若旁路 serialize 即第二发送路径变体 |
| D5 | **Job/Case 状态机与预算帽挂点**：Job 九态 `CREATED/QUEUED/RUNNING/PAUSED/CANCELLING/CANCELLED/COMPLETED/FAILED/BUDGET_EXHAUSTED`、Case 七态 `PENDING/SENT/RESPONDED/TIMEOUT/NETWORK_ERROR/SKIPPED_SCOPE/SKIPPED_BUDGET` **逐字**采用 PLAN :502-503 词表（028 CHECK 逐列）；预算 = `FuzzBudget { max_requests, max_concurrency(默认 2), rate_window_ms/rate_max_requests, min_request_interval_ms, max_duration_ms }`——滑动窗口计数器 **CallerRateTracker 同型**（std AtomicUsize + Instant + Mutex，agent_loop.rs:281-312 实现形态移植，**不跨域复用 agent-engine 符号**）；帽挂点 = 派发循环内 semaphore acquire 后、`send_request` 前逐 case 判定（总数/速率/间隔/时长四闸）+ 完成 case 回调复核；预算由 Rust 强制（PLAN :270）；**默认最严缺省** concurrency=2、无显式 budget = spec 拒绝（fail-closed，PLAN :271 默认低速低并发）；启动前估算 = 位置×payload 组合数纯函数派生 → `estimated_requests` 落 job 行（PLAN :254）；SIGTERM → CANCELLING（在途 case 等终态落行）→ CANCELLED；预算耗尽 → BUDGET_EXHAUSTED（已发送 case 状态全部落行，PLAN :269）；暂停/恢复 = PAUSED 状态位 + resume 按 case 行 status 权威续派（PENDING 未派、SENT/RESPONDED 不重派——判据 8） | 词表逐字防词形漂移（PLAN :506"状态迁移由 Rust 后端定义"）；CallerRateTracker 是 agent 域内部私有结构（agent_loop.rs:281 非 pub），移植形态而非依赖——跨域复用即隐式耦合违反 crate 边界；最严缺省同 MA-2 `allow_parallel=false` 纪律（control.rs 注释面） |
| D6 | **响应差异 = 新原语 `wsw_message::diff` + 结果分析 analyzer 双层**：`crates/wsw-message/src/diff.rs` 纯 std 零依赖——行级 LCS diff（增删改区间）+ 相似度比率（LCS 长度/行数）+ 归一化比较（忽略规则清单：动态字段/时间戳/指定头，PLAN :301 语义）；fuzz 结果分析（`fuzz/analyzer.rs`）= 状态码/长度/耗时/sha256 分组**去重**（PLAN :261）+ 关键字/正则命中 + 错误分类（Timeout/连接失败/TLS/限流——`SendError` 词形映射 + HTTP 429/5xx 分类，PLAN :260）+ baseline 对比（job 显式 `baseline_case_id` 或首个 RESPONDED case 缺省）落 case 行 `similarity`/`diff_class` 派生列；响应正文**不允许长度相同即内容相同**（PLAN :303）——去重键 = body sha256 非长度 | PLAN :606 交付列明文"响应差异"；diff 原语落 wsw-message（消息比较语义归属）供 WSW-6 Comparer 复用（§4.7 :296-303 是其完整面），**不建第二 diff 实现**；W6 批 Comparer 复用本原语 + 补 UI（§8 前置输出④）；LCS 纯 std = 零依赖红线（D9） |
| D7 | **fuzz 结果与 History/拦截队列关系**：发送成功的 case → `wsw_exchange` 行 `source='fuzzer'`（025:54 词形已备；`exchange.rs:53` MessageSource::Fuzzer 在位）——落库走 `insert_exchanges` + `cas::store_body`（`persist_send_result` sender.rs:873 同型新函数 `persist_fuzz_case`，复用 CAS/blob/next_sequence_no 全链）→ History API/页可见可流转（Send to Repeater 对 fuzzer 行 = W3 既有行为零改动）；**不进拦截队列**（024 wsw_intercept 是 MITM 面语义，fuzz 是主动发送面——两词表不混）；SKIPPED_SCOPE case 不发送不落 exchange、case 行 `error_code` = scope 拒绝词形（scope.rs:234 `code()` 词表）留痕可见（PLAN :528）；job 行 `trace_id/task_id/evidence_id` 三引用列沿 025 引用列口径直传（Evidence 工作流接线留后续批，列承载即引用闭环面） | PLAN :563 Fuzzer Job 属工作台运行数据 + §8.1 不建第二事实库；SendError::Scope（sender.rs:204）已是 scope 拒绝机器词形单点；Source 枚举 Fuzzer 变体注释"WSW-3"为批次预期位（exchange.rs:52-53），本批消费不改动 |
| D8 | **UI 不入批（纯引擎批）+ rp_server 最小四端点**：`web/**` 全目录零 diff——PLAN :606 交付列**无 UI 面**（对照 W3 行 :605 明文"Tab 历史"等 UI 交付、WSW-3-TASK §一.5/6 UI 目标项的批次形态）；PLAN §5.5（:417-426 四区页面）与 §11.5.7（:731）属**工作台整体**规格与验收面，非批次授权（"W4 不做 UI 除非计划明文"——批次表即明文口径，未列即不做）；`docs/ui-ux/AI-IMPLEMENTATION-GUIDE.md` grep `fuzz|intruder` **零命中**（Figma 基线无 fuzzer 页，UI 批届时按 PLAN §5.5 + yakit-web-fuzzer.png PLAN :812 扩展设计）。rp_server 落**最小控制面四端点**：POST `/api/wsw/fuzz/jobs`（spec 校验 + spawn `rp-wsw fuzz` 子进程，W3-D7 send spawn 先例 rp_server.py:4067 同型）/ GET `/api/wsw/fuzz/jobs`（列表）/ GET `/api/wsw/fuzz/jobs/{id}`（job+case 计数）/ POST `/api/wsw/fuzz/jobs/{id}/cancel`（SIGTERM，:4183 先例同型）——Application API 同源纪律（PLAN :97/:447），UI 批零后端改动接页 | UI 越界即提前消费无数据支撑的设计面（W2-D8 "UI 多做即提前消费"否决理由同源）；端点先行 = e2e/selftest 走真实 HTTP 链路（禁模拟冒充判据 11）+ 后续 UI 批只做 `web/**`；`web/tests/workspaces.test.ts` 路由钉值 22 **不动**（workspaces.test.ts:101/:107-108） |
| D9 | **零新依赖**：Cargo.lock `[[package]]` 399 恒定（HEAD 对比基准）；regex 仅传递依赖转正直接声明（lock 实值 `=1.13.1`，[[package]] 计数不变、vendor 零变更）；其余全 std（Vec/AtomicUsize/Instant/HashMap）+ 两 crate 既有 serde/serde_json/tokio/rusqlite 面；两 Cargo.toml 只允许 regex 一行转正（wsw-runtime 或 wsw-message 按消费面落）；S1/S7 双点复核 399 | WSW-3-TASK §五.1 / REDLINES 红线 1 vendor 口径；regex 在 lock :2229-2231 实测在位 = 转正不触发解析树变化；禁 reqwest 红线现状 `grep -c 'name = "reqwest"' Cargo.lock` = 0 保持 |
| D10 | **边界与运行模式裁剪**：运行模式本批 = 单位置逐个 / 多位置笛卡尔积 / 多位置同步迭代（zip）三模式（PLAN :248-250）+ 低并发批处理（:252 并发帽即实现）；**请求序列/多包联动不入批**（:251——多 Job 编排面，如实登记遗留）；单请求多位置全覆盖。XML 节点位置 = 浅层标签路径选择器（无 XPath 依赖 std 实现，深嵌套/命名空间登记限制）；HAR 导入导出与 Fuzzer 结果导出（:591）不入批（W3-TASK :21 已登记"留后续批"口径延续）；WebSocket（W5）/Comparer UI（W6）/ Passive Audit 零进本批；audit-engine Emit 不接线（W2-D9 口径延续）；RTIS/intel-db 零接触 | :251 序列面依赖跨 Job 变量传递（PLAN :206 变量作用域语义），本批单 Job 内变量面已足；裁剪项全部 §8/批次报告如实登记，不冒充覆盖 |

## 1. 现状锚点（2026-11-22 本会话实测，A1-A27）

| 锚点 | 事实 | 出处 |
|---|---|---|
| A1 批次授权 | WSW-4 行：主要交付"Payload 位置、Mutator、Processor、Job 调度、响应差异、预算控制"；不包含"Yaklang 全语法兼容"；前批 WSW-3 行 :605 与其不包含列"大规模 Fuzz"明文移交 | WEB-SECURITY-WORKBENCH-PLAN.md:606（:600-611 批次表） |
| A2 §4.4 全节 | :220 定位（Yakit 工作流 + Burp 操作方式）；Payload 位置 :222-233（URL Path :224 / Query/Header/Cookie/Form/JSON/XML/Multipart :226-231 / Raw Body 指定位置 :232 / 多位置组合 :233）；Payload 来源 :235-244（手工列表 :237 / 字典文件 :238 / 范围生成器 :239 / 编码链 :240 / 前缀后缀大小写替换拼接正则 :241 / 可扩展 Mutator·Processor :242 / 结构化 Payload 模板 :243 / RTIS 复用不复制正文 :244）；运行模式 :246-254（单位置 :248 / 笛卡尔积 :249 / 同步迭代 :250 / 请求序列 :251 / 低并发 :252 / 顺序并行区分 :253 / 启动前显示预计数 :254）；结果分析 :256-263（:258 状态码长度耗时标题 MIME 关键字正则 / :259 相似度差异 / :260 错误分类 / :261 提取器匹配器标记去重排序 / :262 送 Repeater 关联 Evidence / :263 复现保存）；控制项 :265-272（:267 上限谱 / :269 取消后已发送仍记录 / :270 Rust 强制 / :271 默认低速 / :272 Out of Scope 不进主动 Fuzz 队列） | PLAN:218-272 |
| A3 §5.5 页面规格 | 四区结构（请求/Payload/运行/结果）:419-424；启动前请求数估算 + Scope 判定展示 :426——属工作台整体规格，本批端点/CLI 产出对齐其数据面 | PLAN:417-426 |
| A4 Rust-First 与模块边界 | 协议网络核心 = Rust（含 Fuzzer）:444；Python Worker 禁第二套 Fuzzer Job :454；Fuzz Engine 候选模块 = "Payload、Mutator、Processor、Job Scheduler、Result Analyzer" :465；新模块只补缺口不复制 :471；禁 Python/Node 实现 Fuzzer 调度 :760 | PLAN:444,454,465,471,760 |
| A5 状态机词表 | Fuzzer Job 九态 :502；Fuzzer Case 七态（含 SKIPPED_SCOPE/SKIPPED_BUDGET）:503；状态迁移 Rust 后端定义 :506 | PLAN:502-503,506 |
| A6 预算与 Scope | 控制项谱 :267-272（详见 A2）；最大 Fuzzer Case 数/总预算/最长运行时间 :537；上限 Rust 后端强制 :539；Scope 逐次校验 :523 + 重定向重校验 :524 + 越界可见 :528 | PLAN:267-272,523-528,537,539 |
| A7 恢复与验收 | 重启恢复不重复派发 :556；§11.3 :694-703（组合数估算正确 :699 / 速率并发总数时长后端生效 :700 / 暂停恢复重启无重复 :701 / Scope 外拒且可见 :702 / 区分响应差异网络错误超时限流 :703）；性能项"Fuzzer 调度吞吐和取消延迟" :751 + 数字实测禁预填 :754 | PLAN:556,694-703,751,754 |
| A8 数据归属 | Fuzzer Job 属工作台运行数据 :563；Fuzzer 结果导出 :591（本批不入，D10）；HTTP 原文属 Artifact :564 | PLAN:563-566,591 |
| A9 禁止与交付 | 禁简化成单一目录爆破 :758；禁孤立页面 :759；禁 Python/Node 调度 :760；禁第二套事实源/Registry :761；"Fuzzer 能构造受控变体、执行、取消和分析结果" :796；UI 参考 yakit-web-fuzzer.png :812；禁静态假数据 :436 | PLAN:758-768,796,812,436 |
| A10 Yaklang 裁决原文 | :34"目标不是逐项复制 Burp 或 Yakit 的全部实现，也不是引入 Yaklang Runtime"；:321"优先复用成熟工具或现有 RTIS 能力，不引入 Yaklang Runtime 作为平台核心依赖"；:606 不包含列"Yaklang 全语法兼容"——三处合读：语义面对齐、语法面（Yaklang `{{…}}` 模板引擎）显式排除 | PLAN:34,321,606 |
| A11 发送引擎 | `send_request` 入口 :720（全路径 doc :717-719）；模块 doc 四要点（serialize 单点 / 重定向逐跳 scope+IP 审计 / 跳数帽双源 min :739 / 超时三件 + watch 取消）:1-30；`SendPolicy` :64（connect 10s/read 30s/total 60s 默认 :86-100）；`SendOutcome` :137（state 词表 :139-141 / error_code 词形 :143 / outbound_bytes :157 / ttfb :163）；`SendError::Scope` :204；`persist_send_result` :873（outbound/请求/响应三 CAS :884-925 + exchange 行组装 :927-970 + next_sequence_no :927） | crates/wsw-runtime/src/sender.rs:1-30,64,137,204,720,739,873 |
| A12 Scope 闸 | `ScopeRules` 六字段 :139-161（include/exclude/allowed_ports/allowed_path_prefixes/allow_private_targets/max_redirects）+ `from_json` :179；`check` :296；`check_redirect_hop` :395；`audit_resolved` :423；`ScopeRejection::code()` :234；云元数据 `METADATA_HOSTS` :29 deny-never-allow | crates/wsw-runtime/src/scope.rs:29,139-186,234,296,395,423 |
| A13 persist 面 | `PersistStore::open_file/open_memory/run_migrations` :296/:306/:319；`ExchangeRow` :667 / `SendRow` :752 / `SendPatch` :777；`assert_watermark_25` **函数名 25 断言值 27**（:893-906，`v != 27` :899——函数名与断言值漂移既有债，本批改名去数字收口）；`insert_exchanges` :908 / `next_sequence_no` :1030 / `insert_send` :1186 / `advance_send` :1235 / `get_session` :1312；`SessionRow`（scope_rules_json 列）:147-161 | crates/wsw-runtime/src/persist.rs:147,296,306,319,667,752,777,893-906,899,908,1030,1186,1235,1312 |
| A14 CAS/blob 面 | `BodyRef`（sha256/uri/size/fidelity）:34-42；`store_body` :133（命中验证不假去重 :142-144 + temp+hardlink 原子写 :160-176）；`read_body` :265；`sha256_hex` :94；并发 CAS 去重原子性测试在位 cas.rs:388；023 `wsw_body_blob` DDL（sha256 PK + uri 固定相对路径 CHECK）:33-46 | crates/wsw-message/src/cas.rs:34-42,94,133,265,388；migrations/023_wsw_body_blob.sql:33-46 |
| A15 serialize/不变式 | `OutboundRequest` 七字段 :36-53；`serialize_request` :219（Outbound 唯一事实）；`from_parsed` 唯一 Parsed→编辑态入口 :343（不变式 2）；入出守卫（CR/LF 注入拒）:17-19 doc；OUTBOUND_CHUNK_SIZE :24 | crates/wsw-message/src/serialize.rs:17-19,24,36-53,219,343 |
| A16 exchange 词表与 025 | `MessageSource` 四值（ProxyCapture/Repeater/**Fuzzer**/Import）:47-56；`ScopeDecision` 三态 NULL=Unknown :58-70；025 `wsw_exchange.source` CHECK **已含 'fuzzer'** :54；025 四表 DDL 与索引 :34-169；wsw_repeater_send 六态 CHECK :146-149 | crates/wsw-message/src/exchange.rs:47-70,53；migrations/025_wsw_exchange.sql:54,146-149 |
| A17 rp-wsw CLI | 子命令分派 `listener/send/ca/db/redact-headers` :667-683；`run_send` :365（幂等防呆终态回显 :379-386 + `assert_watermark_25` :379 + 预飞面 rules 读取 default deny :407-415）；exchange 行 listener_id 常量 `"wsw-lis-repeater"` :505 | crates/wsw-runtime/src/bin/rp-wsw.rs:365,379,407-415,505,667-683 |
| A18 rp_server wsw 面 | 路由表 :4252-4261；`_wsw_ro_conn()` :3443（mode=ro + query_only，消费 :3553/:3672/:3715/:3779）；history 列表 :3552 / body 按需 :3710 / PATCH tags-notes :3743；POST send spawn rp-wsw 子进程 :4067（status_url 回执 :4118）+ send-status :4122 + cancel SIGTERM :4183；retention 统计 :3777 | apps/server/rp_server.py:3443,3552,3710,3743,3777,4067,4118,4122,4183,4252-4261 |
| A19 MA-2 预算面（agent 域，词表隔离样本） | `CallerBudget`（rate_window_ms/rate_max_steps/budget_max_steps_per_run）:77-89；`RunControlConfig.caller_budgets` HashMap :148；装配层读取 `load_caller_budgets` :174（引擎不直查配置表分层纪律 :69-72）；`CallerRateTracker` **私有**滑动窗口（AtomicUsize + Instant + Mutex，window 过期复位）:281-312；受理面跨 run 并发帽 :619-629；轮界 caller 预算面解析 :784-797 + caller 总预算闸（budget_caller_total）:1050-1066 + caller 速率闸（budget_caller_rate）:1072-1089 | crates/agent-engine/src/control.rs:69-89,148,174；crates/agent-engine/src/agent_loop.rs:281-312,619-629,784-797,1050-1066,1072-1089 |
| A20 wsw 面预算原语现状 | wsw-runtime 预算词形仅两处：`QueueBudget`（拦截队列容量原语，lib.rs:136-150）+ sender.rs read_budget 超时变量（:797 局部变量非预算面）；**全 crate grep `rate_limit|RateLimit|max_requests|rpm` 零命中**——fuzz 预算面纯新增，无既有词形冲突 | crates/wsw-runtime/src/lib.rs:136-150；crates/wsw-runtime/src/sender.rs:797；grep 实测 |
| A21 测试面形态 | wsw-runtime tests 六文件（authority_ca 143 行 / exchange_persist 459 / intercept_queue 303 / mitm_loopback 681 / scope_gate 393 / sender_send 622）；**真实靶场先例**：sender_send.rs `spawn_target`（TcpListener 127.0.0.1:0 随机端口 + Behavior 枚举 Marker/RedirectLoop/RedirectOut/RedirectRel/Silent）:55-80 起，14 个测试函数；exchange_persist 12 个；mitm_loopback TLS 夹具面；测试纪律 = `--test-threads=1` + loopback 禁外联（WSW-3-TASK §五.6 :129） | crates/wsw-runtime/tests/ 六文件实测行数与计数；docs/rip3/wsw/WSW-3-TASK.md:129 |
| A22 UI 面 | `docs/ui-ux/AI-IMPLEMENTATION-GUIDE.md` grep `fuzz|intruder` **零命中**（743 行 Figma 基线无 fuzzer 页）；websec 工作区 order 12 两 tab（workspaces.ts:326-342）+ App 路由（App.tsx:170-171）；workspaces.test.ts 路由钉值 22（:101 去重后 22 条 + :107-108 W3 批 20→22 记录）——本批零 web diff 钉值不动 | docs/ui-ux/AI-IMPLEMENTATION-GUIDE.md grep 实测；web/src/workspaces.ts:326-342；web/src/App.tsx:170-171；web/tests/workspaces.test.ts:101,107-108 |
| A23 diff 原语现状 | 全仓 grep `pub fn diff|struct.*Diff|enum.*Diff` **零命中**；wsw 面仅 serialize.rs 一处 "diff" 词形注释——响应差异面纯新增（D6）；PLAN §4.7 Comparer（:296-303）是 WSW-6 完整 UI 面 | grep 实测；PLAN:296-303 |
| A24 依赖面 | Cargo.lock `[[package]]` = 399（`grep -cE` 实测；HEAD `ae23a9d8` 对比基准）；reqwest = 0；`regex` 在位 lock:2229-2231 version 1.13.1；wsw-runtime Cargo.toml 头注释依赖红线（传递转正直接声明 + 等号钉既有版本口径）；wsw-message 无外部运行依赖（serde/serde_json + database workspace 面） | Cargo.lock:2229-2231；crates/wsw-runtime/Cargo.toml 头注 |
| A25 迁移链 | 链头 027（migrations/ ls 实测 001-027）；dev-b1/dev-b2/dev-b34-b3/dev-b34-b4/dev-b5/dev-p13/dev-p4/dev-p71 均 user_version=27（只读实测）；028 空位；编号冻结规则注释（"链尾演进至 027，编号冻结规则顺延"）migration_dev_copy_p7_3.rs:76 | migrations/ 目录 + sqlite3 ro 实测；crates/agent-engine/tests/migration_dev_copy_p7_3.rs:76 |
| A26 链尾十五硬钉（028 随更预登记） | ①crates/agent-engine/tests/migration_idempotent.rs:87 `assert_eq!(user_version, 27)`；②crates/database/tests/migrations_agent_data_plane.rs:100 `(27,"_caller_attribution")`+:110 user_version 断言；③crates/database/tests/migrations_runtime_node.rs:84-85,:90；④crates/database/tests/migrations_tool_contract.rs:149,:159,:175；⑤crates/database/tests/migrations_context_broker.rs:149,:159,:175；⑥crates/database/tests/backup_restore.rs:51 `Some(27)`；⑦crates/rp-mcp-parity/tests/caller_auth.rs:209 `Some(27)`；⑧crates/wsw-runtime/src/persist.rs:899（水位断言）；⑨crates/wsw-runtime/tests/exchange_persist.rs:3,:85；⑩crates/wsw-runtime/tests/intercept_queue.rs:47；⑪crates/wsw-message/src/store.rs:204；⑫crates/wsw-message/tests/cas_e2e.rs:140,:178；⑬crates/agent-engine/tests/migration_dev_copy_p7_3.rs:75,:85,:93；⑭crates/plugin-runtime/tests/lifecycle_full_path.rs:604-610（重放计数钉值，MA-2 修复批 76e0f559 先例形态）；⑮crates/workflow-engine/tests/migration_replay.rs:148-149。软引用（注释面，随更酌情）：crates/database/src/bin/rp-admin.rs:1070、crates/agent-engine/tests/{cb1_transcript_fold,ma2_parallel_budget}.rs:16/:18 头注、apps/server/selftest_ma2.sh:9 | grep `027\|Some(27)\|== 27\|(27,` 全量逐一复核 |
| A27 词形占用 | `fuzz/Fuzz` 词形：wsw 面仅 MessageSource::Fuzzer（exchange.rs:53）+ persist.rs/serialize.rs doc 注释，**无 fuzz 模块无 fuzz 函数**；`wsw-fzj-`/`wsw-fzc-` 前缀空闲（ids.rs 现有 ExchangeId `wsw-`/SessionId `wsw-ses-`/ListenerId `wsw-lis-` 等，:54 `wsw_id!` 宏生成侧承载）；agent 域 `caller_budget/FoldConfig` 词形不跨域（A19） | crates/wsw-message/src/ids.rs:54,115-135；grep 实测 |

## 2. 范围

### 2.1 允许改动（落点清单）

| 落点 | 类型 | 内容 | 步骤 |
|---|---|---|---|
| `migrations/028_wsw_fuzz.sql` | 新建 | D2：`wsw_fuzz_job`（job_id `wsw-fzj-` CHECK / session_id / listener_id / 状态九态 CHECK / spec_json 8MiB 帽 / estimated_requests / budget 字段组 / baseline_case_id / trace_id/task_id/evidence_id 三引用列 / started_at/completed_at/duration_ms / error_code / created_at）+ `wsw_fuzz_case`（case_id `wsw-fzc-` / job_id FK / ordinal / 状态七态 CHECK / payload 值+payload_sha256 / request_outbound_sha256 / exchange_id 引用 / response_status/len/sha256/mime/duration_ms/ttfb_ms / match_hits_json / diff_class+similarity / error_code / started_at/completed_at）+ 索引（job 维度 + status 维度）+ schema_version(28)/user_version=28 双写 | S3 |
| `crates/wsw-runtime/src/fuzz/`（mod.rs / spec.rs / mutator.rs / analyzer.rs / persist.rs） | 新建 | mod.rs = JobRunner（状态机 + Semaphore 并发 + FuzzBudget 四闸 + watch 取消 + 暂停恢复 + SIGTERM graceful）；spec.rs = §…§ 标记词法 + 七类选择器 + 组合估算纯函数 + fail-closed 预检；mutator.rs = 生成器/字典文件/范围生成/编码链/变换/regex + Processor 面；analyzer.rs = 分类/匹配/去重/baseline 对比（挂 wsw_message::diff）；persist.rs = job/case 行读写 + `persist_fuzz_case`（exchange source='fuzzer' + CAS） | S4-S7 |
| `crates/wsw-message/src/diff.rs` | 新建 | D6：行级 LCS diff + 相似度比率 + 归一化忽略规则（纯 std 零依赖） | S2 |
| `crates/wsw-message/src/ids.rs` | 修改 | `wsw_id!` 宏追加 `FuzzJobId("wsw-fzj-")` / `FuzzCaseId("wsw-fzc-")` 两枚（生成侧承载先例） | S2 |
| `crates/wsw-runtime/src/persist.rs` | 修改 | fuzz job/case store 接口（insert_job/update_job_state/list_jobs/get_job/insert_case/update_case/list_cases_by_job/case 计数）；`assert_watermark_25` **改名 `assert_watermark`**（去数字，断言值 27→28——函数名/断言值双漂移收口，调用面 rp-wsw.rs:379 随更） | S3 |
| `crates/wsw-runtime/src/bin/rp-wsw.rs` | 修改 | `fuzz` 子命令（--db/--session-id/--job-id/--spec-json/--policy/--upstream-ca/--pid-file；SIGTERM → CANCELLING→CANCELLED graceful；幂等防呆沿 run_send :379-386 形态）+ usage 更新 | S8 |
| `crates/wsw-runtime/Cargo.toml` | 修改 | `regex = "=1.13.1"` 传递依赖转正一行（D9；头注释登记转正口径） | S4 |
| `Cargo.lock` | 修改（仅依赖清单行） | `[[package]]` 总数 399 不变；仅 regex 进入 wsw-runtime/wsw-message 依赖清单行 | S4 |
| `crates/wsw-message/tests/diff_primitives.rs` | 新建 | diff/相似度/归一化正负例（长度同内容不同负例 PLAN :303） | S2 |
| `crates/wsw-runtime/tests/fuzz_spec.rs` + `crates/wsw-runtime/tests/fuzz_job.rs` | 新建 | spec 词法/选择器/估算负例 + JobRunner 全链（真实靶场 spawn_target 同型 + budget 截断 + scope 全拒 + 取消暂停恢复 + 重启防重派 + 状态词表） | S4-S7 |
| `apps/server/rp_server.py` | 修改 | D8 四端点（POST/GET/GET{id}/POST cancel；spawn + 只读面 `_wsw_ro_conn` 复用；Bearer 鉴权同既有面；既有端点零 diff 只增量） | S8 |
| `apps/server/selftest_wsw4.sh` | 新建 | S9 靶机实测收口脚本（真实 HTTP 靶机往返 + 判据数字回显） | S9 |
| A26 十五硬钉文件 | 修改（钉值随更） | 27→28 钉值四类形态逐文件（S1 实勘清单为准，不预设增减） | S3 |
| `docs/rip3/batches/WSW-4-BATCH-REPORT.md` + `docs/README.md` | 新增/additive | 批次报告（A-K 节惯例 + 坏消息单列 + 遗留登记）+ 索引登记 | S10 |

### 2.2 不修改

- `crates/wsw-runtime/src/sender.rs` / `scope.rs` / `listener.rs` / `listener/**` / `authority.rs` / `lib.rs` 既有面**零 diff**（D1 发送引擎与 scope 闸单点；QueueBudget 既有原语不动）。
- `crates/wsw-message/src/` 除 ids.rs 追加与新 diff.rs 外零 diff（parse/serialize/cas/exchange/types/redact/store/artifact 全不动——D7 只消费公开面）。
- `migrations/001-027` 零 diff；`migrations_pg/`、`schemas/` 零 diff；`migrations/platform*.db` 零触碰（只读反证进批次报告）。
- `web/**` 全目录零 diff（D8 UI 不入批；`workspaces.test.ts` 钉值 22 不动；零 npm 依赖）。
- `crates/agent-engine/**`、`crates/workflow-engine/**`、`crates/tool-engine/**`、`crates/tool-registry/**`、`crates/intel-db/**`（RTIS）、`crates/context-broker/**` 零 diff；caller_budget/FoldConfig 词形不跨域（A19/A27）。
- `apps/server/` 既有端点与既有 selftest 脚本（selftest_ma2.sh / selftest_phase7_agents.sh / selftest_cb1.sh 等）零改动；`rp_server.py` 既有 wsw 端点零 diff 只增量。
- `vendor/**` 零变更；workspace 根 `Cargo.toml` 零 diff；`Cargo.lock` 仅依赖清单行（D9）。
- `rip-data/` 运行时数据不入库（.gitignore 既有口径）；audit-engine Emit 不接线（W2-D9 延续）。

## 3. 实施步骤（S1→S10 串行，每步过门再进）

1. **S1 门禁基线核验**：`grep -cE '^\[\[package\]\]' Cargo.lock` = 399 断言 + `git diff HEAD -- Cargo.lock` 空 + `grep -c 'name = "reqwest"' Cargo.lock` = 0 + `git status --porcelain` 干净；`CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace` **现状全量一次基线**（2026-11-22 新纪律，全绿名单入报告）；A26 十五硬钉逐文件实勘（钉值形态四类核对，清单入报告 D 节）；dev-b1 副本 user_version=27 水位确认；A1-A27 锚点函数名重定位复核。
2. **S2 wsw-message diff + ids**：diff.rs（LCS/相似度/归一化）+ ids 两枚生成器 + `diff_primitives.rs`（正例：单行增删改/多块区间/归一化命中；负例：等长不同内容相似度<1 断言 PLAN :303）；门：`cargo test --offline -p wsw-message -- --test-threads=1` 全绿（cas_e2e/parse_conformance 零 diff 通过）。
3. **S3 028 迁移 + persist 接口**：028 SQL（D2 列 CHECK 逐列）+ fuzz store 接口 + assert_watermark 改名收口；测试内存库 + dev-b1 临时副本双跑 user_version=28；源库 SHA-256 前后不变（WSW-2 S5 范式）；**A26 十五硬钉 27→28 随更**（逐文件断言 + 注释同步；软引用酌情）；门：workspace 内 wsw-runtime/wsw-message/database/agent-engine/workflow-engine/plugin-runtime/rp-mcp-parity 迁移相关测试全绿。
4. **S4 spec 词法与估算**：regex 转正一行 + Cargo.lock 清单行；spec.rs = §…§ 标记解析（字节区间/嵌套交叉拒/未闭合拒）+ 七类选择器定位（query/header/cookie/form/json pointer/path_segment/multipart）+ 多位置组合 + 估算纯函数（笛卡尔积/zip/单位置三模式，u64 溢出饱和拒）；`fuzz_spec.rs` 落盘；门：单测全绿。
5. **S5 mutator/processor**：生成器（列表/字典文件/数字字母日期范围）+ 编码链（URL/Base64/Base64URL/Hex/HTML Entity）+ 变换（前后缀/大小写/替换/拼接/regex）+ Processor 面定界；单测逐变换 + 字典文件 BOM/空行/注释边界；门：单测全绿。
6. **S6 analyzer**：错误分类（SendError 词形 + 429/5xx 限流面）+ 关键字/regex 匹配 + sha256 分组去重 + baseline 对比（挂 diff 原语）+ similarity/diff_class 派生；单测（分类矩阵 + 去重负例）；门：单测全绿。
7. **S7 JobRunner 与落库**：mod.rs 状态机（九态迁移表 + 非法迁移拒）+ Semaphore 并发 + FuzzBudget 四闸（派发前逐 case 判定）+ watch 取消 + PAUSED/恢复（case 行 status 权威防重派）+ SIGTERM graceful；persist.rs = persist_fuzz_case（exchange source='fuzzer' + CAS 三面）+ job/case 读写；`fuzz_job.rs` 集成（spawn_target 真实靶场：marker echo 端点 payload 变体真实命中断言 + budget 截断 BUDGET_EXHAUSTED + scope 全拒零发送 SKIPPED_SCOPE + 取消终态 + kill/resume 防重派）；门：`cargo test --offline -p wsw-runtime -- --test-threads=1` 全绿（既有 76+ 项零 diff 通过）。
8. **S8 CLI + API**：rp-wsw `fuzz` 子命令（幂等防呆 + graceful）+ rp_server 四端点（spawn/status/cancel 只读面复用 `_wsw_ro_conn`）+ curl 实测回显进报告 E 节；门：端点手测 + 既有 wsw 端点回归（history/repeater 零 diff 通过）。
9. **S9 靶机实测面**：`apps/server/selftest_wsw4.sh`——本地真实 HTTP 靶机（python3 标准库 http.server echo/字典面端点，宿主零安装）：spec 落 job → spawn fuzz → case 全 RESPONDED + exchange(source=fuzzer) 行经 `/api/wsw/history` 可查 → marker 命中断言 → budget 截断回查 → cancel 回查；性能实测：调度吞吐（case/秒）+ 取消延迟（SIGTERM→CANCELLED ms）真实命令回显（PLAN :751/:754 禁预填）。
10. **S10 收口**：§5 全部命令实跑（含 workspace 全量一次）+ clippy -D warnings + deny 双检 + lock 399=399 + `git status` 复核只含 §2.1 落点 + 批次报告落盘 + docs/README.md 登记 + REDLINES 八步循环收口（显式 add 本批清单，禁 add -A）。

## 4. 完成标准（判据表；验收 = 命令实跑回显）

| # | 判据 | 验收 |
|---|---|---|
| 1 | 发送引擎单点 | fuzz 模块 grep `TcpStream|connect_stream|serialize` 无第二写路径（只有 `sender::send_request` 调用 + serialize_request 间接消费）；`git diff HEAD -- crates/wsw-runtime/src/sender.rs crates/wsw-runtime/src/scope.rs` 空；既有 sender_send.rs 14 项零 diff 通过 |
| 2 | 组合数估算正确（PLAN :699） | 三模式（单位置/笛卡尔/zip）估算 = 实际派发 case 数（集成断言）；负例：max_requests 截断后 estimated vs sent 关系可见 |
| 3 | Payload 位置面 | Raw §…§ 标记 + query/header/cookie/form/json 四类选择器各一正例（真实靶机 echo 回读 payload 命中）；未闭合/嵌套/交叉标记 → 预检拒绝 fail-closed（负例）；多位置组合单请求双位置正例 |
| 4 | Mutator/Processor | 手工列表/字典文件（BOM+空行+# 跳过）/数字范围生成器正例；URL/Base64 编码链 + 前后缀变换 + regex 变换正例；头注入 payload（CRLF）→ serialize 守卫拒（负例复用既有词形） |
| 5 | 预算帽 Rust 强制（PLAN :270,:700） | max_requests 截断 → job BUDGET_EXHAUSTED + 已发送 case 终态齐全（:269）；速率窗口超帽 → 超额 case SKIPPED_BUDGET 计数（负例可触发）；min_request_interval 生效（相邻 case started_at 间隔实测） |
| 6 | 状态机词表逐字（PLAN :502-503） | 028 CHECK 九态/七态逐列（非法状态 INSERT 拒断言）；非法迁移（QUEUED→COMPLETED 直跳等）Rust 侧拒断言 |
| 7 | Scope 后端强制（PLAN :702,:272） | default deny 会话 fuzz → 全 case SKIPPED_SCOPE + 靶机请求计数 = 0（负例实证零发送）；include 命中越界重定向 → SKIPPED_SCOPE/failed + scope 词形 error_code 可见 |
| 8 | 重启恢复无重复派发（PLAN :701,:556） | job 运行中 SIGKILL → 重启 resume → 已终态 case 不重派（case 行 ordinal/status 对账 = 派发集合无重）；PAUSED→resume 续派 PENDING 断言 |
| 9 | 响应差异面（PLAN :703,:303） | 网络错误/超时/限流（靶机 429 端点）/正常四类分类断言；body sha 分组去重（同内容 case 数可见）；baseline diff + similarity 列；**等长不同内容 → similarity<1 非同响应**（负例） |
| 10 | History 闭环（PLAN :563） | case 发送成功 → exchange(source='fuzzer') 行落库 + `/api/wsw/history` 可查（curl 回显）+ body 按需端点 CAS 逐字节回读；Send to Repeater 对 fuzzer 行既有行为零改动 |
| 11 | 真实靶机实测（禁模拟冒充） | selftest_wsw4.sh 全绿：python3 http.server 真实靶机 + marker 命中断言 + 全部数字来自命令回显；禁 Mock/预填（PLAN :436/:754） |
| 12 | 门禁与迁移纪律 | `cargo test --offline --workspace` 全量一次通过 + 两 crate 定向 `--test-threads=1` + clippy -D warnings + deny 双检 + Cargo.lock 399 恒定 + reqwest=0 + migrations 001-027 零 diff + 028 单文件 + A26 十五硬钉随更全绿 + `git status` 只含 §2.1 落点 + web 零 diff |

## 5. 测试面（收口命令）

```bash
# Rust 全量（2026-11-22 新纪律：清单式门禁对全量 CI 有覆盖差，workspace 一次必须跑）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace
# 本批两 crate 定向（串行口径；wsw 面集成测试含真实靶场 socket 面）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline -p wsw-runtime -- --test-threads=1
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline -p wsw-message -- --test-threads=1
CARGO_TARGET_DIR=/home/emo/rt3-target cargo clippy --offline -p wsw-runtime -p wsw-message --all-targets -- -D warnings
# deny / lock / 红线现状
cargo deny check advisories && cargo deny check licenses
grep -cE '^\[\[package\]\]' Cargo.lock    # 399（HEAD ae23a9d8 对比基准）
grep -c 'name = "reqwest"' Cargo.lock    # 0
# 迁移水位反证（platform*.db 只读不在写入面）
python3 -c "import sqlite3,glob
for db in sorted(glob.glob('migrations/*.db')):
    print(db, sqlite3.connect('file:'+db+'?mode=ro',uri=True).execute('PRAGMA user_version').fetchone()[0])"
# → dev-* 至少 dev-b1 为 28；platform*.db 维持 1/1/1/0/0 反证
# server 面（既有零改动回归）
bash apps/server/selftest_ma2.sh          # MA-2 既有 CLI 三硬验收回归（零改动通过）
# 本批新增：靶机实测收口（判据 5/7/8/9/11 数字来源）
bash apps/server/selftest_wsw4.sh
```

## 6. 红线（批内任何步骤不得越过）

1. **Rust-First 禁 Python/Node 实现 Fuzzer 核心**：Job 调度/位置解析/Mutator/Processor/结果分析全 Rust（PLAN :444/:454/:465/:760；REDLINES 红线 2）；rp_server Python 只做 spec 校验 + spawn + 只读读面（W3-D7 写权分治先例延续）；禁 reqwest（lock = 0 保持）。
2. **零新依赖**：Cargo.lock `[[package]]` 399 恒定；regex 仅传递依赖转正直接声明 `=1.13.1`（lock :2229-2231 实值；vendor 零变更）；diff/调度/预算全 std；S1/S7 双点复核；宿主机零系统级安装（REDLINES 红线 8——靶机走进程内 TcpListener 与 python3 标准库，禁 sudo 禁新增运行时）。
3. **不建第二发送引擎 / 第二事实源 / 第二 Registry**：发送唯一路径 `sender::send_request`（sender.rs:720，D1）；sender.rs/scope.rs 零 diff；job/case 只落 028 两表；fuzz exchange 行只走 025 既有面（source='fuzzer' 词形在位）+ CAS 既有面；禁第二套事件流水与 Registry（PLAN :761；REDLINES 红线 4）。
4. **历史迁移禁改 + 028 单文件纪律**：001-027 零 diff；028 纯新增零 ALTER 纯单版本推进；`schema_version`/`user_version` 双写 28；**A26 十五硬钉文件随更**（钉值形态四类，S1 实勘清单为准；实施中发现确需第十六处随更 → 按实勘形态补入报告，禁漏网）；migrations_pg 零 diff。
5. **生产库零触碰 + RTIS 零接触**：`migrations/platform*.db` 不写（user_version 维持 1/1/1/0/0 反证进报告 D 节）；只在测试内存库与开发库副本执行迁移；`crates/intel-db/**` 零 diff；漏洞事实不读写（REDLINES 红线 3；PLAN :244 RTIS POC 正文不复制进普通工作区库）。
6. **预算帽词表纪律**：Job 九态 / Case 七态 PLAN :502-503 词表逐字，禁造新状态词值；预算词形 `FuzzBudget/max_requests/max_concurrency/rate_window_ms/rate_max_requests/min_request_interval_ms/max_duration_ms/BUDGET_EXHAUSTED/SKIPPED_SCOPE/SKIPPED_BUDGET` 与 PLAN 词表绑定；**禁挪用 agent 域词形**（CallerBudget/CallerRateTracker/FoldConfig 是 agent-engine 内部面——D5 移植形态不 import）；error_code 沿 SendError/scope `code()` 既有词形（sender.rs:143/:204、scope.rs:234），新词形仅限 `fuzz_*` 前缀自别。
7. **测试纪律与真实靶机**：集成测试 `--test-threads=1` + timeout 保护；禁外联（靶场 loopback 随机端口；REDLINES 红线 8）；判据 11 禁模拟冒充——selftest 与集成测试靶机必须是真实 HTTP server（进程内 TcpListener / python3 标准库），禁 Mock 传输层、禁预填数字（PLAN :436/:754）；测试红只修代码不改钉值（REDLINES 红线 6）；同一失败方法最多两次（REDLINES 红线 7）。
8. **边界纪律（W4 不做 UI 除非计划明文）**：PLAN :606 交付列无 UI 面 → `web/**` 零 diff、零 npm 依赖、workspaces.test.ts 钉值 22 不动（D8）；§5.5 四区页面与 §11.5.7 验收面登记 §8 前置输出留 UI 批；WebSocket（W5）/Comparer 完整面（W6）/Fuzzer 结果导出（:591）/请求序列多包联动（:251）/HAR 导入导出不入批（D10，做了即越界）；禁静态假数据伪造运行状态（PLAN :436）；Out of Scope 目标不得进入主动 Fuzz 队列（PLAN :272，判据 7 实证）。

## 7. 回滚

- 单 commit 交付；`git revert` 整批撤销。028 为纯新增两表 → revert 后开发副本库残留 `wsw_fuzz_job`/`wsw_fuzz_case` 两表（终态行只读无害；如需干净可手动 `DROP TABLE`，登记说明不强制清理——025 残留面先例同款口径）。
- regex 转正随 revert 回退两 Cargo.toml 一行 + Cargo.lock 依赖清单行（[[package]] 399 全程不变，无解析树残留）。
- assert_watermark 改名随 revert 回退（函数名/断言值回到 25 名 27 值的既有漂移态，如实登记）。
- fuzz 子命令/四端点随 revert 消失；已产 exchange(source='fuzzer') 行与 case/job 行残留为运行数据（PLAN :563 工作台运行数据语义，无回滚面，rip-data/ 口径同 WSW-3-TASK :145）。
- 十五硬钉随更与 028 同 commit——revert 即同步回退，钉值面无半态。

## 8. 交付物

1. **028 迁移面**：`migrations/028_wsw_fuzz.sql`（wsw_fuzz_job/wsw_fuzz_case 两表 + 索引 + 双写 28）+ persist.rs fuzz store 接口 + assert_watermark 改名收口 + A26 十五硬钉 27→28 随更。
2. **fuzz 引擎面**：`crates/wsw-runtime/src/fuzz/` 五模块（mod/spec/mutator/analyzer/persist）——JobRunner 九态状态机 + Semaphore 并发 + FuzzBudget 四闸 + §…§ 标记与七类选择器 + 声明式 Mutator/Processor 链 + 结果分类/去重/baseline 对比 + persist_fuzz_case（exchange source='fuzzer' 闭环）。
3. **diff 原语面**：`crates/wsw-message/src/diff.rs`（LCS 行 diff + 相似度 + 归一化忽略规则，纯 std）+ ids.rs 两枚 ID 生成器。
4. **入口面**：rp-wsw `fuzz` 子命令 + rp_server 四端点（jobs create/list/status/cancel，spawn 先例同型）。
5. **测试面**：`diff_primitives.rs` + `fuzz_spec.rs` + `fuzz_job.rs`（真实靶场集成）+ `apps/server/selftest_wsw4.sh`（判据 5/7/8/9/11 数字实测）。
6. **文档面**：`docs/rip3/batches/WSW-4-BATCH-REPORT.md`（命令原文 + 数字实测 + 坏消息与遗留单列：请求序列 :251 / XML 深解析 / 结果导出 :591 / §5.5 UI 全部如实登记）+ docs/README.md 登记 + 本档锚点行号漂移勘误（如有）。
7. **WSW-5 前置输出登记**：①**UI 遗留面交接**——§5.5 四区页面规格落点 + §11.5.7 e2e 断言清单 + 本批四端点词形与响应形状（UI 批只做 `web/**`，后端零改动输入）；②**FuzzBudget 词形与四闸语义**（W5 帧重放/批量发送预算面继承输入；CallerRateTracker 同型滑动窗口实现备忘 agent_loop.rs:281-312）；③**Job/Case 状态机与 SKIPPED_* 语义**（W5 帧发送 scope/预算跳过语义同源；028 状态词表 CHECK 形态复制面）；④**`wsw_message::diff` API 签名与语义**（WSW-6 Comparer :296-303 复用输入——禁第二 diff 实现的约束面）；⑤**regex 转正先例**（lock 实值 =1.13.1 等号钉 + 转正口径注释，后续批正则面直接声明不重复依赖审计）。
