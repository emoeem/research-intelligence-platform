# MA-4 任务书：事件推送与跨 AI 端到端验收（rp_server SSE 多客户端订阅推送面 + caller 维度列与过滤 Runs 页扩展 + 截图入库 + 四形态端到端验收脚本 selftest_ma4.sh + N=4 并发压测——MA 轨收官批）

- 日期：2026-11-22
- 状态：任务书已定稿（2026-11-22 侦察后落稿；实现批次按本任务书执行，实施走 Agent 主会话路径）
- 依据：[MULTI-AGENT-ACCESS-PLAN.md](MULTI-AGENT-ACCESS-PLAN.md) §四 MA-4 行（:48，「SSE/WS 事件推送面（多客户端订阅）+ 四形态端到端验收脚本 + 截图/实测入库」；硬验收「四形态各一条端到端实测（含多 AI 并发压测小规模 N=4）+ 事件推送实时性断言」）、§五 集成面（:52，「不另建控制台：多 AI 并发状态在既有 Web 工作台 Research/Runs 页扩展 caller 维度列与过滤」——**计划明文 UI 授权，本批 web 入批的唯一依据**）、§六 禁止事项（:58/:60/:61）、§八 完成定义（:76 禁模拟冒充）；[MA-DSH-REFERENCE.md](MA-DSH-REFERENCE.md) R2（:14，「事件推送面（MA-4 SSE）推投递通知」）+ R9（:21，「MA-4 Runs 页扩展（caller/成员维度 + DAG 状态）」）+ dsh-im 行（:67，「IM webhook 作为可选输出通道……登记 MA-4 扩展项不进必交付」）+ E1/E4/E5（:33/:36/:37，Plugin Center 观察池）；[MA-3-BATCH-REPORT.md](MA-3-BATCH-REPORT.md) §六.3 MA-4 移交五件（:148）+ §六.2 phase6 既有红（:147）；[MA-3-TASK.md](MA-3-TASK.md) §0 D3（sweep = SSE 通知源输入）/D7（R4 建队/R7 异构/Runs 页 UI 移交本批）+ §8.4 前置输出五件（:175）；[MA-2-BATCH-REPORT.md](MA-2-BATCH-REPORT.md) §七.1/.2 预算闸语义备忘（:126-128）
- 前置：MA-3 已收口（HEAD `0f2922cb` 含 CI 修复批；029 入链、task_claim/ma_message 与 rp-collab/MCP 七工具/REST /api/subtasks 在跑、供体 PG V0007 收口）；SQLite **030 空位在且本批不立项**（D6：零迁移）；PG **V0008 空位在且本批不立项**（D6）；web/** 自 WSW-3 后零 diff（WSW-4 D8 web 零 diff 裁决 WSW-4-TASK.md:24——其前提「PLAN 交付列无 UI 面」对 MA-4 不成立：计划 :52/:48 明文 UI 与截图授权，见 D5）
- 设计裁决：本批特有裁决见 §0（不另立 ADR——设计增量可溯源至规划文档与 MA-3 移交面；如有分歧以规划文档为准并回改本档，MA-3-TASK 头注同款）
- 现状锚点：§1 A1-A16 均为 2026-11-22 本会话实测行号（HEAD `0f2922cb`、工作树干净 `git status --porcelain` = 0 行实测），实施时若行号漂移按函数名重定位并以本档词形为准

---

## 0. 设计裁决（本批特有，ADR 内嵌）

| # | 裁决 | 理由 |
|---|---|---|
| D1 | **SSE 单面落地，WS 零实现**：推送面 = rp_server 既有 `RpHandler`（rp_server.py:4557）新增 `GET /api/events` 专用分支（do_GET :4683-4765 的 /api/ 分支内、API_ROUTES 字典分发之前——SSE 需要裸 wfile 流不进 send_json），形状 = `send_response(200)` + `Content-Type: text/event-stream` + `Cache-Control: no-store` + **不发 Content-Length** + 逐事件 `self.wfile.write(frame)` + flush——rp_server **protocol_version 全文件零设置**（grep 实测零命中），BaseHTTPRequestHandler 缺省 HTTP/1.0 connection-close 形态即 close-delimited 流（客户端读到连接关闭 = 流终；EventSource/fetch 均可消费），零协议面改动。**WS 三路全否**：①Python stdlib 无 WS server（hand-roll RFC6455 握手/帧掩码 = 第二协议实现风险面）；②tokio-tungstenite 虽在 lock（Cargo.lock:3008，wsw-runtime listener 与 wsw-poc 消费实测）但 Rust WS server = 第二进程第二端口第二事件面（违红线 1；axum grep 零命中不存在第三路）；③升 `protocol_version="HTTP/1.1"` = 全端点 keep-alive 语义变更（每响应必须 Content-Length），否。计划 :48 措辞「SSE/WS」是手段列举非双交付——硬验收字面是「事件推送实时性断言」，SSE 承载即达标；IM webhook 渠道（MA-DSH-REFERENCE:67）按其原文登记观察池不进必交付 | stdlib 约束下 SSE 是唯一零新依赖零协议变更形状；WSW-4 D8（WSW-4-TASK.md:24）「UI 多做即提前消费」同款裁决心法反用于推送面：多客户端订阅 = 计划 :48 明文交付，WS 不在明文词面内（「SSE/WS」列举中 WS 无独立硬验收） |
| D2 | **事件源 = 服务端 DB tail 轮询三形态 cursor（零引擎零 hook）**：引擎无 hook/回调点（实测：workflow-engine src grep audit_event 零命中——workflow 状态跃迁无审计行；agent-engine 事件经 audit-engine Emit 落 audit_event 表即事实源，audit-engine/src/lib.rs:76/:177）——建 hook = 引擎核心改动，违 MA-4 收官批定位（MA-3 D7 同款「做了即越界」逻辑）。三形态：①**append-only cursor**（audit_event.event_id——001:80 `INTEGER PRIMARY KEY AUTOINCREMENT`；ma_message/task_claim 的 rowid）②**updated_at 窗口重查**（agent_run/tool_run——trace.rs:634 列序实证 created_at/updated_at 在位；task_claim 029:63-64）③**workflow_run 活跃态快照 diff**（017:44-70 **无 updated_at 列**——仅 created/started/finished，轮询 `state IN ('queued','running','paused')` 行 + 进程内快照 diff 发跃迁事件）。**PG knowledge 面不进 SSE**（双库分工 029:121 原文「PG = 情报核」；形态④供体验收不依赖 SSE，如实声明）。029 sweep 面（rp-collab msg-sweep 输出 `{undelivered_total, items[]}`，rp-collab.rs:418-434）= 推送失败重投输入（MA-3 D3 原文「MA-4 SSE 推送面通知源输入，推送通道本批零实现」——本批把通道接上；sweep CLI 本体零改动复用） | 引擎零 diff 是 MA-3 收口后最大资产（MA-3-BATCH-REPORT:54 agent-engine 零 diff 实证）；tail 轮询读既有表 = 零写面零迁移零引擎改动三合一；ma_message delivered/read 两列分记（029:78-83）即推送去重键（MA-3-BATCH-REPORT:148 移交①原文） |
| D3 | **多客户端订阅模型 = 进程内注册表 + 单 tail 轮询 daemon 线程扇出（零持久化零迁移）**：订阅注册表 = 模块级 `{queue: deque, filters}` 列表 + `threading.Lock`；首个订阅者惰性启动一个 daemon 轮询线程（RP_SSE_POLL_MS 缺省 500ms；无订阅者自动退出），每周期按 D2 三形态 cursor 扫描并扇出到各订阅者队列；每条 SSE 连接 = ThreadingHTTPServer 工作线程（daemon_threads=True stdlib 缺省）阻塞读队列写帧。事件帧 = `id: <source>:<cursor>` 单调 + `event: <词形>`（run_state/claim_state/message/guardrail/audit additive 词表）+ `data: {ts, kind, entity_id, action, caller_id, run_id, detail}` JSON；心跳 `: ping` 注释行（RP_SSE_HEARTBEAT_S 缺省 15s）保活探活；连接帽 RP_SSE_MAX_CLIENTS 缺省 16 超限 503 `sse_capacity`；`Last-Event-ID` 头（SSE 标准）= 起始 cursor 重放位（回看窗 RP_SSE_REPLAY_MAX 缺省 500 行防重放放大）。订阅态纯进程内存——进程重启 = 客户端 EventSource 自动重连 + Last-Event-ID 重放，零补写状态 | 计划 :48「多客户端订阅」硬面；N=4 并发压测 = 4 订阅线程 + 4 任务进程共 8 工作线程，ThreadingHTTPServer 线程每连接模型余量充足（A5）；限频 check_rate_limit（:298-305）每 HTTP 请求计一次——长连接只计连接时刻一次，N=4 订阅 + 心跳零限频冲突（A6）；零持久化 = 030 不立项的直接依据 |
| D4 | **四形态端到端验收 = selftest_ma4.sh 单脚本分节**（selftest_ma3.sh 442 行 8 节先例同型；**不分脚本**——四形态共享夹具（caller/key 预置、工具/Agent 登记、临时库），分脚本会四份夹具漂移）：§2 形态①（单 caller N=4 并行子代理，rp-agent `--allow-parallel`）§3 形态②（N=4 caller 并发各自任务，POST /api/subtasks × 4 真进程同刻）§4 形态③（workflow 双 caller 节点 + claim 交接 + ma_message → SSE 推送通知闭环——MA-3 面止于 fetch，本批接推送）§5 形态④（N=4 donor 并发治理，隔离 PG 容器，podman 缺席 V-9 SKIP）§6 SSE 推送实时性专节 §7 N=4 并发压测汇总 §8 生产库指纹。**V-9 SKIP 守卫预挂**（MA-3 CI 修复批 0f2922cb 教训原文：python worker keyword_search 硬编码只读打开 migrations/platform.db，gitignore 运行时数据 CI 无文件 → run FAILED exit 1；修法 = platform.db 不在 → SKIP return，first_batch_b2.rs:103-109 同口径）——凡 spawn rp-agent/rp-mcp/rp-kc 的节先探 bin 在位 + platform.db 载具在场，缺 → SKIP 计数 + 脚本仍 exit 0（SKIP≠FAIL 如实分开计数） | 计划 :48「四形态端到端验收脚本」单数措辞 + selftest_ma3.sh 单脚本 8 节先例（:88/:171/:246/:331/:373/:430 节结构实测）；V-9 守卫 = 本仓库 CI 修复批实证先例（0f2922cb commit 原文），预挂成本一条 if 远低于红一轮 CI |
| D5 | **caller 维度列扩展 = Python server 读面 additive + web 前端 additive，Rust 零 diff，web/** 入批（计划明文授权面）**：①server：`run_list_columns`（:746-750）has_column 条件追加 caller_id（027:56 tool_run.caller_id 已落列——trace 徽标列 P7-5 同款「按库水位条件出现、旧库不伪造」先例，RunsPage.tsx:7-9 文档原文）+ `/api/runs`、`/api/agent-runs`、`/api/workflow-runs` 三端点 `?caller=` 等值过滤 additive（agent-runs caller 列 MA-3 已入 agent_run_list_sql——MA-3-BATCH-REPORT:47；workflow-runs caller_id 投影 MA-3 已入 rp_server.py:2673 `_wf_col`）；②web：RunsPage caller 徽标列 + caller 过滤下拉（沿既有 status 过滤同型 :55-64）+ AgentsPage 运行记录区 caller 列（:308 数据已有）+ RunDetailPage 概要区 caller 行 + RunPanel run 头 caller 徽标（run 行 caller_id 投影已在，节点级 caller 聚合不做）+ api.ts 类型 caller_id 可选字段 additive；零新路由（workspaces.test.ts:101-108 钉 22 不动）。**R9 深化（可交互 DAG 重排/成员树/模型标注）不进本批**（登记 §8 前置输出；计划 :52 授权字面 =「caller 维度列与过滤」，本批 DAG 面 = 既有 FlowCanvas/RunPanel 零结构改动 + run 头标注）。**Figma 基线核对**按 AI-IMPLEMENTATION-GUIDE :23（视觉基准）+:91-97 规则执行：caller 列 = 既有表格 additive 列非新画板，沿 trace 徽标列既有词形；Figma MCP/额度不可用时先实现 + 登记待同步项（:95 明文）——核对结论入批次报告 | 计划 :52 明文 = web 入批唯一授权（WSW-4 D8 前提「PLAN 无 UI 面」对 MA-4 不成立）；计划 :48「截图/实测入库」明文 = 截图产物入库授权；R9（MA-DSH-REFERENCE:21）映射原文「caller/成员维度 + DAG 状态」——caller 维度 = :52 字面落地，DAG 状态 = 既有 RunPanel 状态面已承载 + 深化登记 |
| D6 | **零迁移 + 零新依赖 + Rust 全 crate 零 diff**：①030/V0008 空位**不立项**（SSE = 既有表 ro tail 零持久化，D3；029:121 schema 行明文「MA-4 扩词值须升 030+」——本批不扩 kind 词表故 030 不动）→ A15 十六硬钉 **29 钉值零随更**，S1/S7 双点断言 =29 保持；②Cargo.lock 399 恒定（实测 HEAD `0f2922cb`）+ reqwest=0 + sha2 转正面 MA-3 已完成零再动；③**Rust 全 crate 零 diff**：SSE 在 Python server、caller 列/过滤在 Python 读面 + web、验收在 shell——rp-agent（`--caller-id`/`--allow-parallel`）/rp-collab（claim/msg 十一子命令）/rp-workflow（`--caller-id`）/rp-kc（donor-submit）/rp-mcp（七工具）五 bin 全复用 MA-1/2/3 既有 flags；④web 零 npm 依赖（package.json dependencies 零改动） | 收官批最小面：MA-1/2/3 已把四形态的引擎/CLI/契约面全部落库（A12），MA-4 本体 = 推送面（server）+ 观测面（web）+ 验收面（shell）——任何 Rust 改动都是越界信号（S7 git status 落点核对拦截） |
| D7 | **N=4 并发压测口径（双口径 + SSE 并发观测）**：①**跨 caller 口径**：N=4 caller（caller-ma4-a..d，rp-admin 预置四 key）× 各 1 子任务，4 真进程同刻 POST /api/subtasks——断言 4 行 agent_run.caller_id 互异且全 SUCCEEDED + 提速比 = 并发墙钟/串行墙钟 **≤ 0.7**（MA-2 判据 2 <0.7 硬界口径延续，selftest_ma2.sh:129 原文；串行基线 = 同夹具顺序 4 次）+ 超帽负例（caller-ma4-cap 跨 run 帽 1 + 占位 run → BudgetDenied 硬拒，词形 `budget denied` + `max_concurrent_runs`，MA-3 报告 :101 实测词形）。②**单 caller 口径**：N=4 并行子代理（`--allow-parallel` 四步 fan-out，MA-2 N=3→N=4 升档）——提速比 <0.7 硬界由既有 Rust e2e ma2_parallel_budget 承载口径，CLI 数字如实入报告（MA-2 备忘 :129 同款声明）。③**SSE 并发观测**：压测期间 N=4 SSE 订阅客户端（4 独立流读进程）全部收齐且事件 id 序列严格一致（多客户端订阅硬面）。**实时性口径（判据 6 数字化）**：RP_SSE_POLL_MS=500 下，事件投递延迟 =（事件源行落库时刻 → 客户端帧到达时刻）单事件 **≤ 2s**、连续 ≥10 事件 **p95 ≤ 2s**、事件计数**零丢失**（源触发 N = 客户端收 N）、心跳 ≤ RP_SSE_HEARTBEAT_S+1s 在位——落库时刻由夹具在 payload/写库前后 clock 采样对照（真实流计时，禁预填） | 计划 :48「含多 AI 并发压测小规模 N=4」+「事件推送实时性断言」双硬验收数字化；0.7 界 = MA-2 实测 0.339 余量内的既有判据词形（selftest_ma2.sh:129）；实时性 2s = 4× 轮询周期余量（500ms 轮询 + 轮询/扇出/网络开销），WSW-3 性能门（WSW-3-TASK.md:112 首 3s/交互 1s/p95 300ms）管 UI 页面管不到流式推送——推送面单独数字化不套 p95<300ms（流式端点无「响应完成」语义） |
| D8 | **WSW-3 性能门对本批 web 面的适用口径**：web/** 入批（D5）则 Runs/Agents 页沿用 WSW-3 门（WSW-3-TASK.md:112：首屏 <3s / 交互 <1s / API p95 <300ms）——本批新断言 = Runs 页首屏（列表行在屏）<3s + caller 过滤交互 <1s + `/api/runs?caller=` p95 <300ms（20 采样 × 1 新过滤参数端点，WSW-3 S9 :61 口径缩面：本批无新端点只有过滤参数 + SSE 专用端点单独按 D7 口径）；承载 = selftest_ma4.sh §6/§7 节数字回显（**不新建 browser e2e 脚本**——截图 spec（S5）管渲染断言，性能数字由 shell 计时采样，WSW-3 rip3_wsw3_browser_e2e.mjs 全套仅在其 UI 大批复刻） | WSW-3-TASK.md:61/:112/:118 实测口径原文；MA-4 是收官批不是 UI 大批——门适用但载具缩面（shell 计时即可数字化，免维护第二份 browser e2e） |

## 1. 现状锚点（2026-11-22 本会话实测，A1-A16）

| 锚点 | 事实 | 出处 |
|---|---|---|
| A1 批次授权 | MA-4 行：主要交付「SSE/WS 事件推送面（多客户端订阅）+ 四形态端到端验收脚本 + 截图/实测入库」；硬验收「四形态各一条端到端实测（含多 AI 并发压测小规模 N=4）+ 事件推送实时性断言」；前置 MA-3 | MULTI-AGENT-ACCESS-PLAN.md:48（:42-48 批次表） |
| A2 UI 授权与禁止 | §五 :52「不另建控制台：多 AI 并发状态在既有 Web 工作台 Research/Runs 页扩展 caller 维度列与过滤（沿 docs/ui-ux 体系）」；禁止 1 禁 per-caller 分库 :58、禁止 3 caller 不进 Registry :60、禁止 4 禁自由写竞争 :61；§八 :76 禁模拟冒充 | MULTI-AGENT-ACCESS-PLAN.md:52,58,60,61,76 |
| A3 DSH 输入 | R2（:14）ma_message 未投递行由 sweep 重投 + 「事件推送面（MA-4 SSE）推投递通知」；R9（:21）「MA-4 Runs 页扩展（caller/成员维度 + DAG 状态）；沿 docs/ui-ux 体系不另建」；dsh-im（:67）IM webhook = 「MA-4 事件推送的通知渠道路径……登记 MA-4 扩展项不进必交付」；E1/E4/E5（:33/:36/:37）Plugin Center 观察池；brain-mark 收据（:57）已由 MA-3 落 | MA-DSH-REFERENCE.md:14,21,33,36,37,57,67 |
| A4 MA-3 移交五件 | ①ma_message 行形状与 sweep 语义（delivered/read 两列分记即事件去重键）；②claim 队列词形与 CAS 原语签名（kind 词表 agent_subtask/review 冻结，扩词值须升 030+）；③节点 caller 绑定双层解析序（params > run 级 > None，params 开放面挂点零迁移）；④caller 预算面实测语义备忘（速率窗口秒级粒度/总预算闸整轮硬停/BudgetDenied 词形 `budget denied: caller ... at cap ...`）；⑤供体收据词形 `{knowledge_id, review_status, normalized_hash}` | MA-3-BATCH-REPORT.md:148 + MA-3-TASK.md §8（:175） |
| A5 rp_server HTTP 层形状 | `from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer` :91；`ThreadingHTTPServer((HOST, PORT), RpHandler)` :4985（线程每连接，daemon_threads stdlib 缺省 True）；**protocol_version 全文件零设置**（grep 零命中 → 缺省 HTTP/1.0 connection-close）；RpHandler :4557；send_json :4576-4589（每响应 Content-Length 形状）；do_GET /api/ 分支 :4688-4765（API_ROUTES 字典 :4515-4549 共 **31 路由** + 动态路径分支）——SSE 专用分支落点 = 字典分发之前先判 `/api/events` | apps/server/rp_server.py:91,4557,4576-4589,4515-4549,4688-4765,4985 |
| A6 鉴权与限频审计 | require_auth 三态（legacy default / 多 key caller_id / 未过 None→401 + AUTH_FAIL_DELAY）:4605-4627；`_resolve_caller_key`（sha256→api_key JOIN caller 吊销判定只读直查）:209-248；全局限频 check_rate_limit（RP_RATE_LIMIT_RPM 缺省 600 req/60s，0 关闭）:267-305、do_GET 每请求计一次 :4689——长连接只计连接时刻一次；审计收尾 _finish_audit :4657-4681（handler 返回后落 JSONL——SSE 流 duration = 流生命周期，如实落不截断；keep-alive 同连接身份消费即清防串 :4679-4681） | apps/server/rp_server.py:209-248,267-305,4605-4627,4657-4681,4689 |
| A7 只读连接与零写纪律 | connect_ro :359-371（URI mode=ro + `PRAGMA query_only=ON` + attach_aux 只读挂载——SSE tail 轮询与 caller 读面复用此形状）；audit_api_access :314-329（「本文件零 SQL 写」纪律原文——审计走 JSONL）；写面 subprocess 委派纪律 :2540-2546；phase6 静态断言「rp_server.py 全文件无 SQL 写语句」= 既有红（3 处 wsw_repeater INSERT，HEAD d664ceef 同败，MA-1/2/3 报告同登记——本批命中数不增不减） | apps/server/rp_server.py:314-329,359-371,2540-2546；MA-3-BATCH-REPORT.md:147 |
| A8 轮询端点清单（客户端现怎么拉状态） | `/api/runs` :1055（ToolRun；detail SELECT * :1069 caller_id 自含；**list 列 RUN_LIST_COLUMNS :740-743 + run_list_columns 条件 trace_id :746-750——caller_id 缺席**；list 过滤分支 :1141-1176 status/tool_id）；`/api/agent-runs` :2371（caller 列 MA-3 已入 list SQL）；`/api/workflow-runs` :2655（`_wf_col` caller_id 投影 :2673 MA-3 已入）；`/api/traces` :1818（audit_event 多 schema 归并先例）；WSW 面 :3623 起独立 WSW_DB；web 拉取 = react-query useApi（hooks.ts:40-79；`refetchIntervalMs` 轮询先例 ActivityPlansLive.tsx:17=5000ms、WswProxyPage.tsx:64/:67=3000/5000ms）——本批 SSE 到达后 web 侧既有轮询面零改动（SSE 是增量观测面不是轮询替换面） | apps/server/rp_server.py:740-750,1055,1069,1141-1176,1818,2371,2655,2673,3623；web/src/lib/hooks.ts:37,40-79 |
| A9 事件源与写入方 | audit_event 表 001:79-95（**event_id INTEGER PRIMARY KEY AUTOINCREMENT** = append-only 天然 cursor + ix_audit_entity/ix_audit_ts :97-99）；写入方 = audit-engine Emit（lib.rs:61/:76/:90/:97/:177 INSERT，写失败缓冲重试）经 agent_loop.rs:2283 audit_emit / context-broker intent.rs:430 / assemble.rs:91 / plugin-runtime upgrade.rs:162；audit 词形实测（agent_loop.rs grep）：`agent.run.accepted` / `agent.run.finished` / `agent.step.completed` + actions GUARDRAIL（含 budget_caller_total/rate，:1058-1089）/PARALLEL_PLAN/CANCELLED/PAUSED/RESUMED/TRANSCRIPT_FOLD；**workflow-engine src grep audit_event 零命中**（workflow 跃迁无审计行）；workflow_run 无 updated_at（017:44-70 仅 created/started/finished）；tool_run/agent_run created_at/updated_at 在位（trace.rs:634 INSERT 列序实证） | migrations/001_init.sql:79-99；crates/audit-engine/src/lib.rs:61,76,177；crates/agent-engine/src/agent_loop.rs:1040-1401,2283；crates/workflow-engine/src（grep 零命中）；migrations/017_workflow_run.sql:44-70；crates/agent-engine/src/trace.rs:634 |
| A10 MA-3 协作面（复用） | collab.rs ClaimStore/MessageStore 全 API（claim 单语句 CAS :522 / complete :599 / fail :606 / transfer :674 / show :711；send :813 / fetch :885 / read :929 / **sweep :972**）；rp-collab 十一子命令（用法区 rp-collab.rs:5-18；:18 原文「msg-sweep …… MA-4 SSE 输入源」；:27 platform*.db 守卫）；msg-sweep 输出形状 `{undelivered_total, items[]}`（rp-collab.rs:418-434）；029 表（task_claim :41-64 五态 CHECK / ma_message :66-88 delivered_at+read_at 分记 / 五索引 :90-99 / schema_version(29) :113-118 + user_version 双写 :122）；kind 词表冻结声明（029:121 schema 行「MA-4 扩词值须升 030+」） | crates/workflow-engine/src/collab.rs:522-972；crates/workflow-engine/src/bin/rp-collab.rs:5-27,418-434；migrations/029_collab_protocol.sql:41-88,90-99,113-122 |
| A11 caller 预算面（MA-2 复用） | caller_budget 七帽列（027:68-82）；rp-agent `--caller-id`/`--allow-parallel` + caller_budget 装配（rp-agent.rs 既有）；受理面 BudgetDenied run 不落行 + 帽拒词形实测（selftest：node2 failed `code=agent_failed` message 含 **`budget denied`** 与 `max_concurrent_runs`，MA-3-BATCH-REPORT:101）；总预算闸整轮硬停 / 速率窗口秒级粒度（MA-2-BATCH-REPORT §七.1/.2 :126-128）；ma2_parallel_budget.rs **800 行**（:335 判据 2 硬验收① N=3 并行提速，模拟驱动 <0.7 硬界）+ selftest_ma2.sh **214 行**（:103 N=3 CLI 计时 + :129 「<0.7 硬界由 Rust e2e 承载，CLI 数字如实入报告」口径） | migrations/027_caller_attribution.sql:68-82；crates/agent-engine/tests/ma2_parallel_budget.rs:335（800 行）；apps/server/selftest_ma2.sh:103,129；MA-2-BATCH-REPORT.md:126-128；MA-3-BATCH-REPORT.md:101 |
| A12 四形态既有支撑与缺口 | ①单 AI 多子代理并行：ma2_parallel_budget.rs + selftest_ma2.sh N=3——缺口 = N=4 升档重测 + PARALLEL_PLAN 事件 SSE 可见；②多 AI 各自任务：selftest_auth.sh **:71-109 #8**（MA-1 多 key 块：caller-add/key-gen 预置双 caller 双 key + 互不串扰审计三态 + 停用 key 401）——缺口 = #8 是鉴权层非任务层，MA-4 需 N=4 caller **真实子任务并发**（/api/subtasks MA-3 已落：MA-3-BATCH-REPORT:111 REST 契约实测 200/403 词形）；③多 AI 协作同任务：node_caller_binding.rs **675 行**（双 caller 节点 + budget denied 词形 + 零自动建 claim）+ collab_claim.rs **389 行**（N=8 真线程 CAS）+ selftest_ma3.sh:88-245——缺口 = claim→message→SSE 推送闭环（MA-3 邮箱止于 fetch）；④多 AI 供知识：donor_governance.rs **276 行**（N=6 PG 真线程全 CANDIDATE + SAME_CONTENT N-1 边 + 双闸）+ selftest_ma3.sh:373-429（隔离 PG 容器 N=6 进程 188ms）——缺口 = 无（N=4 口径复用）；**边界：PG 面不进 SSE**（D2） | crates/agent-engine/tests/ma2_parallel_budget.rs；apps/server/selftest_auth.sh:71-109；crates/workflow-engine/tests/node_caller_binding.rs（675 行）；crates/workflow-engine/tests/collab_claim.rs（389 行）；crates/intel-db/tests/donor_governance.rs（276 行）；apps/server/selftest_ma3.sh:88-245,373-429；MA-3-BATCH-REPORT.md:96-118 |
| A13 web 现状 | RunsPage.tsx **155 行**（:7-9 文档「P7-5：trace_id 在场时行内渲染 trace 徽标链接……列按库水位条件出现、旧库不伪造」= caller 列同款先例；:64 `useApi<RunList>(\`/api/runs?…\`)`；status 过滤下拉既有；**未用 VirtualList**——简单列表）；RunDetailPage.tsx 318 行；AgentsPage.tsx（:308 `/api/agent-runs?limit=50` + :312 详情）；WorkflowsPage.tsx **575 行**（FlowCanvas/NodeInspector/RunPanel/VersionList :11-14）+ RunPanel.tsx **316 行**；VirtualList.tsx 83 行（WSW-3 交付）；**web/src grep "caller" 零命中**（caller 维度 UI 零存在）；runs-smoke.test.tsx 9 用例（stub 无 caller 字段）；workspaces.test.ts:101-108 路由钉 **22**（新增路由即钉红——本批零新路由） | web/src/routes/RunsPage.tsx:7-9,64；web/src/routes/AgentsPage.tsx:308,312；web/src/routes/WorkflowsPage.tsx:11-14；web/tests/runs-smoke.test.tsx；web/tests/workspaces.test.ts:101-108 |
| A14 截图管线与 Figma 规则 | selftest_phase6.sh:219-238：RP_P6_E2E=1 → `npx playwright test tests/plugins.spec.ts -c playwright.plugins.config.ts` → 截图落 web/qa/plugins/e2e-*.png + 在位断言（:237）；playwright.plugins.config.ts：双模式（visual-server.mjs :4178 静态 mock / RP_P6_E2E=1 真实后端 baseURL+token 环境注入）+ chromium executablePath 环境覆盖；web/qa/visual/pages.spec.ts 视觉矩阵先例；计划 :48「截图/实测入库」= 产物入库授权（对照 MA-3 无 UI 批「截图副作用 git checkout 还原」MA-3-BATCH-REPORT:58——本批截图是**有意交付**非副作用）；Figma 规则：AI-IMPLEMENTATION-GUIDE.md :23「Figma 是视觉基准，不是业务事实来源」+:28「新增功能时优先在现有组件体系中补充，不创建平行设计系统」+:95「Figma MCP、权限或额度不可用……可以先实现代码并记录待同步项」 | apps/server/selftest_phase6.sh:219-238；web/playwright.plugins.config.ts；MULTI-AGENT-ACCESS-PLAN.md:48；docs/ui-ux/AI-IMPLEMENTATION-GUIDE.md:23,28,95 |
| A15 链尾硬钉 29 实勘（零随更清单） | 十六文件 29 钉位全清单（四类钉值形态逐行实测）：①agent-engine/tests/migration_idempotent.rs:87（assert 29）；②agent-engine/tests/migration_dev_copy_p7_3.rs:76（013..029 注释）+:85（013..029 幂等 msg）+:92（assert 29）；③database/tests/migrations_agent_data_plane.rs:83（vec 1..29）+:100（(29,"_collab_protocol")）+:109（assert 29）；④database/tests/migrations_tool_contract.rs:107（vec）+:150-157（尾元组 …(29,"_collab_protocol")）+:159-162（assert 29）；⑤database/tests/migrations_runtime_node.rs:72+:84+:90；⑥database/tests/migrations_context_broker.rs:108+:150+:160；⑦database/tests/backup_restore.rs:51（Some(29)）；⑧rp-mcp-parity/tests/caller_auth.rs:209（Some(29)）；⑨plugin-runtime/tests/lifecycle_full_path.rs:604（链尾注释）+:610（010..029 回放）；⑩workflow-engine/tests/migration_replay.rs:148+:149；⑪wsw-message/src/store.rs:203；⑫wsw-message/tests/cas_e2e.rs:132（<29 门）+:139+:177+:190（assert 29）；⑬wsw-runtime/src/persist.rs:895-899（「029 chain head expected user_version=29」断言——MA-3 后行号自 :899 漂移至 :895 起段）；⑭wsw-runtime/tests/exchange_persist.rs:3（doc「MA-3 后链尾 029」）+:85（assert 29）；⑮wsw-runtime/tests/intercept_queue.rs:47（assert 29 全词形注释）；⑯wsw-runtime/tests/fuzz_job.rs:145（watermark 29）。**两假阳性排除**：tool-engine/tests/policy_resolver_manifests.rs:253/:556 的「29 份派发面」= manifest 计数巧合非迁移钉；wsw-runtime/src/bin/rp-wsw.rs:375/:524 = assert_watermark **调用点**非钉值。本批零迁移 → 全清单零随更，S1/S7 双点复核 =29 | grep `\b29\b|Some\(29\)|1\.\.29|013\.\.029|010\.\.029|assert_watermark` 全量逐一实勘（本会话；MA-3-TASK A18 十六文件清单 :48 前身 + WSW-4 第十六处教训同款全形态扫描） |
| A16 依赖/迁移水位/CI 教训 | Cargo.lock `[[package]]` **399**（grep 实测）；reqwest **0**；tokio :2932 / hyper :1076 / **tokio-tungstenite :3008 / tungstenite :3218 在树**（wsw-runtime listener handler.rs + wsw-poc main.rs 消费实测——WS 能力在但 D1 裁决不启用为 server）；axum 零命中；030 空位（migrations/ 001-029 连续 ls 实测）+ PG V0008 空位（V0001-V0007 连续）；水位探针实测：dev-p13/dev-p4/dev-p71/dev-b1/dev-b2/dev-b34-b3/dev-b34-b4/dev-b5 = **29**，dev-b34=5/dev-p7=12/dev-p8=18（历史断面），**platform.db=1 / platform-dryrun.db=1 / platform-fullbatch.db=1 / platform-governance.db=0 / platform-search.db=0**（生产零触碰反证基线）；HEAD `0f2922cb`（MA-3 CI 修复批）工作树干净；V-9 守卫先例（commit 原文：rip_kb_search.py:14 硬编码只读打开 migrations/platform.db + first_batch_b2.rs:103-109 同口径 SKIP） | grep 实测 + python3 sqlite3 ro 探针（本会话输出）；git log --oneline -1 = 0f2922cb；0f2922cb commit message |

## 2. 范围

### 2.1 允许改动（落点清单）

| 落点 | 类型 | 内容 | 步骤 |
|---|---|---|---|
| `apps/server/rp_server.py` | 修改 | D1/D2/D3：`GET /api/events` SSE 专用分支（do_GET /api/ 分支内字典分发前；require_auth 复用 :4605-4627 + check_rate_limit 既有调用序）+ 订阅注册表（threading.Lock + deque + filters）+ 单 tail 轮询 daemon 线程（D2 三形态 cursor；RP_SSE_POLL_MS 缺省 500 / RP_SSE_HEARTBEAT_S 缺省 15 / RP_SSE_MAX_CLIENTS 缺省 16 / RP_SSE_REPLAY_MAX 缺省 500 四 env 常量 + 头注文档区 env 说明追加）+ 事件帧格式化（id/event/data + 心跳注释行）+ Last-Event-ID 重放；**零写语句**（全经 connect_ro 形状只读） | S2 |
| `apps/server/rp_server.py` | 修改 | D5①：`run_list_columns`（:746-750）has_column 条件追加 caller_id（trace_id 先例同款）+ `/api/runs`（:1141-1176）、`/api/agent-runs`、`/api/workflow-runs` 三端点 `?caller=` 等值过滤 additive（参数化绑定 SQL；未知 caller → 空集非错误）——既有端点零 diff 只增量 | S3 |
| `web/src/routes/RunsPage.tsx` | 修改 | D5②：caller 徽标列（caller_id 在场渲染徽标，trace 徽标列 P7-5 同款形状）+ caller 过滤下拉（数据 = 列表 items caller 去重；服务端 ?caller= 参数，status 过滤同型） | S4 |
| `web/src/routes/AgentsPage.tsx` | 修改 | D5②：运行记录区 caller 列（数据 caller_id MA-3 已在 list SQL） | S4 |
| `web/src/routes/RunDetailPage.tsx` | 修改 | D5②：概要区 caller 行（run.caller_id 展示） | S4 |
| `web/src/components/workflow/RunPanel.tsx` | 修改 | D5②：run 头 caller 徽标（run 行 caller_id 投影 MA-3 已在；节点级聚合不做——D5 R9 深化登记） | S4 |
| `web/src/lib/api.ts` | 修改 | RunList/AgentRunList/WorkflowRunList 行类型 caller_id 可选字段 additive | S4 |
| `web/tests/runs-smoke.test.tsx` | 修改 | stub 行 additive caller 字段 + caller 列/过滤断言追加（既有 9 用例零改动前提 additive） | S4 |
| `web/playwright.runs.config.ts` | 新建 | D5②截图配置（playwright.plugins.config.ts 同款双模式：visual-server 静态 mock / RP_P6_E2E=1 真实后端） | S5 |
| `web/tests/runs-caller.spec.ts` | 新建 | Runs/Agents 页 caller 列与过滤渲染断言 + 截图 ≥4 帧落 `web/qa/runs/`（首屏/过滤态/详情/Agents 页）——计划 :48「截图入库」载体 | S5 |
| `apps/server/selftest_ma4.sh` | 新建 | D4：收口脚本（四形态各一节 + SSE 实时性专节 + N=4 压测汇总 + V-9 SKIP 守卫预挂 + 生产库指纹；真实进程/真实 HTTP/真实 SSE 流/真实 MCP stdio，数字回显） | S6 |
| `docs/rip3/MA-4-BATCH-REPORT.md` + `docs/README.md` | 新增/additive | 批次报告（命令原文 + 数字实测 + 坏消息单列 + MA 轨收官记账节 + Plugin Center 观察池登记提醒节）+ 索引登记（docs/README.md:87-89 表行同款） | S7 |

### 2.2 不修改

- **`crates/**` 全部零 diff**（D6：Rust 面零改动——rp-agent/rp-collab/rp-workflow/rp-kc/rp-mcp 五 bin 既有 flags 复用；SSE/事件源零引擎 hook；tokio-tungstenite 在 lock 也不启用为 WS server）。
- `migrations/001-029` 与 `migrations_pg/V0001-V0007` 零 diff；**030/V0008 不立项**（D6）；A15 十六硬钉 29 钉值零随更。
- `Cargo.lock` 与 workspace 根 `Cargo.toml` 零 diff（399 恒定 + reqwest=0）；web `package.json` dependencies 零改动（零 npm 依赖）。
- `web/tests/workspaces.test.ts:101-108` 路由钉 22 不动（零新路由）；`web/qa/plugins/**`、`web/qa/visual/**` 既有截图零改动。
- rp_server.py 既有 31 路由与写面零改动（/api/events 与 ?caller= 纯 additive；protocol_version 零设置保持——D1；send_json/do_POST/PATCH/PUT 形状不动）；既有 selftest 脚本零改动（phase7_agents 405 钉值不动；phase6 静态断言命中数与 HEAD 全等）。
- web 既有轮询面零改动（useApi/refetchIntervalMs 面零 diff——SSE 是增量观测面不是轮询替换面）。
- `migrations/platform*.db` / `platform-dryrun.db` 零触碰（只读指纹反证进报告）；`integrations/rtis/**` 与 `rip-data/**` 零触碰；`web/node_modules` 零触碰。

## 3. 实施步骤（S1→S7 串行，每步过门再进）

1. **S1 门禁基线核验**：`grep -cE '^\[\[package\]\]' Cargo.lock` = 399 + `grep -c 'name = "reqwest"' Cargo.lock` = 0 + `git status --porcelain` 干净 + HEAD = `0f2922cb`；`CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast` **现状全量一次基线**（预期唯一红 = document-router di2_media×4 环境面，MA-3 报告 :77 同水位；红项逐一反证登记）；A15 十六硬钉逐文件实勘复核（=29；假阳性两处排除声明入报告）；A1-A16 锚点函数名重定位复核；V-9 载具确认（migrations/platform.db 在位 + target/debug 五 bin 在位 + python_worker 可跑——CI/新 checkout 无 platform.db 时 S6 各节 SKIP 路径同步验证）。
2. **S2 SSE 推送面落形**：/api/events 分支 + 注册表 + tail 轮询 daemon + 三形态 cursor + 心跳/帽/Last-Event-ID（D1/D2/D3 逐条）+ 四 env 常量与头注 env 文档；门 = 手工 curl -N 流回显事件帧（id/event/data 三行帧）+ 触发 rp-collab msg-send → 客户端 ≤2s 收到 message 事件 + 双客户端并发同序 + Last-Event-ID 重放回看窗生效 + 超帽 503 负例（RP_SSE_MAX_CLIENTS=1 时第二连接 503）+ 未授权 401。
3. **S3 caller 读面 additive**：run_list_columns caller_id 条件投影 + 三端点 ?caller= 过滤；门 = curl 三端点过滤正例（预置 caller 行返回正确子集）/ 负例（未知 caller 空集非错误）/ 既有列零回归（/api/runs 不带参数回显与 HEAD 基线一致）+ `bash apps/server/selftest_auth.sh` 与 selftest_phase7_agents.sh 零改动通过。
4. **S4 web caller 维度**：四文件列/徽标/过滤 + api.ts 类型 additive + runs-smoke stub additive；门 = `cd web && npm run typecheck && npm test`（vitest run 全绿；workspaces.test.ts 钉 22 通过实证零新路由）+ 既有 runs-smoke 9 用例零改动通过。
5. **S5 截图与浏览器 e2e**：playwright.runs.config.ts + runs-caller.spec.ts（双模式）+ RP_P6_E2E=1 真实后端跑通；门 = `npx playwright test tests/runs-caller.spec.ts -c playwright.runs.config.ts` 全过 + `web/qa/runs/` 截图 ≥4 帧在位 + 非产物副作用（test-results/）git checkout 还原。
6. **S6 selftest_ma4.sh**：D4 八节（V-9 SKIP 守卫预挂每个 spawn 节）+ D7 双口径压测 + 实时性专节；门 = 全节 PASS（SKIP 与 PASS 分开计数）exit 0 + 既有 selftest 回归七套零改动（auth/ma2/ma3/phase6/phase7_agents/phase8_workflow/phase9_mcp）。
7. **S7 收口**：§5 全部命令实跑（含 workspace 全量一次）+ `git status --porcelain` 只含 §2.1 落点核对（crates/ 零 diff 实证 + Cargo.lock 零 diff + 030/V0008 空位仍在 + A15 十六硬钉 =29 复核）+ 批次报告落盘（含 MA 轨收官记账节 + 观察池登记提醒节）+ docs/README.md 登记（显式 add 本批清单，禁 add -A）。

## 4. 完成标准（判据表；验收 = 命令实跑回显）

| # | 判据 | 验收 |
|---|---|---|
| 1 | **硬验收①形态一：单 AI N=4 并行子代理** | 同一 caller（caller-ma4-a）`rp-agent --allow-parallel` 四步 fan-out 实跑：4 子步并行峰值为 4（库直查 tool_run 并发窗口）+ PARALLEL_PLAN 审计行落库 + SSE 订阅者收到对应 audit 事件（caller_id=caller-ma4-a）+ 提速比 <0.7 硬界由既有 ma2_parallel_budget 口径声明、CLI 数字如实入报告（N=3→N=4 升档） |
| 2 | **硬验收②形态二：N=4 caller 并发各自任务** | caller-ma4-a..d 四 key（rp-admin 预置）× 4 真进程同刻 POST /api/subtasks：4 行 agent_run.caller_id 互异且全 SUCCEEDED（库直查）+ 提速比 = 并发墙钟/串行墙钟 ≤0.7（数字回显）+ 四 caller 审计行互不串扰（JSONL caller 四态各归各）+ legacy token 403 负例（MA-3 :112 词形复证） |
| 3 | **硬验收③形态三：双 AI 协作同任务 + 推送闭环** | workflow 定义两 agent 节点分属两 caller（MA-3 面复用）实跑 + claim 交接（transfer → 败方接新 attempt）+ ma_message send → SSE 订阅者收到 message 事件（to_caller 过滤命中）→ 收件方 fetch（delivered_at 落）→ read → 不再推送重投行——claim/message 状态跃迁全程 SSE 可见且与库直查一致 |
| 4 | **硬验收④形态四：N=4 供知识并发治理** | 隔离 PG 容器（podman 在场）：N=4 并发 rp-kc donor-submit 同 content → 4 行全 CANDIDATE + donor_caller_id 逐行正确 + SAME_CONTENT 候选边 ≥ N-1 + review 双闸负例 + approve 正例；PG 缺席 V-9 SKIP exit 0；**声明：PG knowledge 面不进 SSE 事件源**（D2 双库分工） |
| 5 | **SSE 多客户端订阅面** | N=4 订阅客户端压测期间全部收齐且事件 id 序列严格一致；?caller= 过滤（订阅者 A 只收 caller-a 事件）/ ?kinds= 词表过滤各一正例；Last-Event-ID 重放（断连重连补发回看窗内事件）；心跳 ≤15s+1s 在位；RP_SSE_MAX_CLIENTS 超限 503 `sse_capacity`；未授权 401；客户端断连后注册表条目清理（连接数回落） |
| 6 | **推送实时性断言（数字化）** | RP_SSE_POLL_MS=500：事件投递延迟（源行落库 → 客户端帧到达）单事件 ≤2s + 连续 ≥10 事件 p95 ≤2s + 事件计数零丢失（触发 N = 收到 N）——全部来自真实流计时回显（msg-send/claim-claim 真实触发，禁预填） |
| 7 | **caller 维度列与过滤（计划 :52）** | /api/runs list caller_id 投影（027 库）+ 三端点 ?caller= 过滤正负例；web RunsPage caller 徽标列 + 过滤下拉 + AgentsPage caller 列 + RunDetailPage caller 行 + RunPanel run 头徽标（Playwright/vitest 断言在屏）；旧库（无 027 列）caller 列不出现不伪造（has_column 守卫——P7-5 trace 列同款） |
| 8 | **截图入库（计划 :48）** | web/qa/runs/ ≥4 帧（首屏/过滤态/详情/Agents 页）入库 + selftest 内在位断言 + workspaces 路由钉 22 不动（零新路由实证） |
| 9 | **零迁移零新依赖零 Rust diff** | Cargo.lock 399 恒定 + reqwest=0 + `git status` 实证 crates/** 零文件 + Cargo.lock 零 diff + 030/V0008 空位仍在（ls 反证）+ A15 十六硬钉 29 双点复核零随更 + web 零 npm 依赖 |
| 10 | **server 零直连写 + SSE 只读流** | /api/events 与 ?caller= 全部 connect_ro 形状（mode=ro + query_only）；phase6 静态断言「无 SQL 写语句」命中行与 HEAD 全等（既有 3 处红不增不减）；platform*.db 五库指纹前后一致（1/1/1/0/0） |
| 11 | **既有回归零改动** | selftest_auth.sh / selftest_ma2.sh / selftest_ma3.sh / selftest_phase6.sh（既有红口径 48/1 不变）/ selftest_phase7_agents.sh（405 钉值不动）/ selftest_phase8_workflow.sh / selftest_phase9_mcp.sh 七套零改动通过 + `cargo test --offline --workspace` 全量一次通过（唯一红 = di2_media×4 环境面基线同败） |
| 12 | **真实实测（禁模拟冒充）** | 判据 1/2/3/4/5/6 数字全部来自真实进程/真实 HTTP/真实 SSE 流/真实 MCP stdio/真实 PG 回显（计划 :76；SSE 延迟 = 真实流计时非估算） |

## 5. 测试面（收口命令）

```bash
# Rust 全量（2026-11-22 新纪律：workspace 一次必须跑；本批 Rust 零 diff = 基线复证）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast
# 零 diff 复证 + 门禁现状（本批无 Rust 改动，clippy/deny 为基线复证）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo clippy --offline -p workflow-engine -p mcp-server -p intel-db --all-targets -- -D warnings
cargo deny check advisories && cargo deny check licenses
grep -cE '^\[\[package\]\]' Cargo.lock    # 399（HEAD 0f2922cb 对比基准恒定）
grep -c 'name = "reqwest"' Cargo.lock    # 0
ls migrations/030* migrations_pg/V0008* 2>&1 | grep -c "No such file"  # 2（030/V0008 空位仍在反证）
# A15 十六硬钉 =29 双点复核（抽查四类形态各一 + 全清单 grep 计数与 S1 基线一致）
grep -c "Some(29)" crates/database/tests/backup_restore.rs            # 1
grep -rn "assert_watermark" crates/wsw-runtime/src/persist.rs | wc -l # 与 S1 基线计数一致
# 迁移水位反证（platform*.db 只读不在写入面）
python3 -c "import sqlite3,glob
for db in sorted(glob.glob('migrations/*.db')):
    print(db, sqlite3.connect('file:'+db+'?mode=ro',uri=True).execute('PRAGMA user_version').fetchone()[0])"
# → dev-p13/dev-p4/dev-p71 等为 29；platform*.db 维持 1/1/1/0/0 反证
# server 面（既有零改动回归）
bash apps/server/selftest_auth.sh               # MA-1 多 key 面回归
bash apps/server/selftest_ma2.sh                # MA-2 三硬验收回归
bash apps/server/selftest_ma3.sh                # MA-3 三硬验收回归（SSE 复用的协作面基线）
bash apps/server/selftest_phase6.sh             # SQL 写静态断言既有红口径不变（48/1）
bash apps/server/selftest_phase7_agents.sh      # 405 钉值不动
bash apps/server/selftest_phase8_workflow.sh    # workflow 既有面回归
bash apps/server/selftest_phase9_mcp.sh         # MCP 目录面回归
# 本批新增：事件推送与四形态端到端收口（判据 1-8 数字来源；V-9 场景预挂 SKIP 守卫）
bash apps/server/selftest_ma4.sh
# web 面（D5/D8：typecheck + vitest + Playwright 截图）
cd web && npm run typecheck && npm test && cd ..
RP_P6_E2E=1 npx --prefix web playwright test tests/runs-caller.spec.ts -c web/playwright.runs.config.ts
ls web/qa/runs/*.png | wc -l                    # ≥4（截图入库在位）
```

## 6. 红线（八条）

1. **不建第二事件面/Runtime/事实源**：SSE = 既有 SQLite 表只读 tail（D2 三形态 cursor），禁 EventBus 新表、禁第二推送进程/端口、禁 WS server（tokio-tungstenite 在 lock 也不启用——D1 三路全否）；R9 完整 DAG/成员树/模型标注深化只登记不实现（D5；计划 :52 授权字面 = caller 维度列与过滤）。
2. **零新依赖零迁移**：Cargo.lock `[[package]]` 399 恒定 + reqwest=0 + sha2 转正面零再动；030/V0008 不立项（029:121 kind 词表冻结声明——本批不扩词值）；web 零 npm 依赖；S1/S7 双点复核。
3. **server 零直连 SQL 写**：/api/events 与 caller 读面全部 connect_ro 形状（:359-371 mode=ro + query_only 先例）；phase6 静态断言命中行与 HEAD 全等（既有 3 处 wsw_repeater INSERT 红不增不减）；审计照旧走 JSONL（:314-329 零 SQL 写纪律原文）。
4. **生产库零触碰 + V-9 守卫**：`migrations/platform*.db` 五库指纹前后一致（1/1/1/0/0 反证进报告 D 节）；selftest_ma4.sh 凡 spawn rp-agent/rp-mcp/rp-kc 节预挂 V-9 SKIP 守卫（platform.db/bin 缺 → SKIP 计数 + exit 0——MA-3 CI 修复批 0f2922cb 教训原文，first_batch_b2.rs:103-109 同口径）。
5. **caller 不进冻结契约**：SSE 事件 payload 词形与 ?caller= 参数纯 additive；ToolCallRequest/MCP 目录契约零 diff；web 路由钉 22 不动（workspaces.test.ts:101-108）。
6. **web/** 入批最小面纪律：web 面唯一授权 = 计划 :52（caller 维度列与过滤）+ :48（截图/实测入库）——非授权 web 面（虚拟滚动改造/新页/新路由/新依赖）零改动；Figma 基线核对按 AI-IMPLEMENTATION-GUIDE :23/:28/:95 执行（additive 列沿 trace 徽标既有词形；Figma 不可用先实现 + 待同步登记）；截图产物 web/qa/runs/ 显式 add 入库、test-results/ 等非产物副作用 git checkout 还原。
7. **禁模拟冒充**：四形态端到端 / N=4 压测 / SSE 延迟与零丢失 / 提速比数字全部来自真实进程/真实流计时回显（计划 :76）；禁预填数字、禁 Mock 传输层；SKIP 与 PASS 分开计数不混报。
8. **边界纪律（做了即越界）**：WS 服务面、IM webhook 推送通道（MA-DSH-REFERENCE:67 登记不进必交付）、R9 DAG 深化/成员树/模型标注、成员建队与 R4 toolPolicies→RT-3 constraints 绑定、R7 异构成员、dsh 弱点 4 预算化嵌套传递、workflow 执行器自动建 claim、research_task.caller_id 写面、api_key.last_used_at 写入、RT-3 constraints caller 维度、Runs 页虚拟滚动改造、web 既有轮询面替换——全部不进本批（越界即停批上报）。

## 7. 回滚

- 单 commit 交付；`git revert` 整批撤销。/api/events 分支、订阅注册表与四 env 常量随 revert 消失零残留；?caller= 过滤与 run_list_columns caller_id 投影随 revert 回退（trace 条件列先例同款 additive，既有列零半态）。
- web additive（四文件列/徽标/过滤 + api.ts 类型 + runs-smoke stub 字段）随 revert 回退零残留；web/qa/runs/ 截图目录随 revert 删除；playwright.runs.config.ts 与 runs-caller.spec.ts 随 revert 消失。
- **零迁移零库残留**（SSE 无持久化状态、订阅态进程内存——revert 后无任何 DROP/清理步骤，MA-3 回滚的库残留说明面本批不存在）。
- selftest_ma4.sh 随 revert 消失零残留；既有 selftest 七套零改动 = 回归面无半态。
- A15 十六硬钉零随更 = revert 无钉值面回退动作。

## 8. 交付物

1. **SSE 推送面**：`GET /api/events`（多客户端订阅 + D2 三形态 cursor 事件源 + D3 订阅模型 + 心跳/连接帽/Last-Event-ID 重放 + 四 env 常量与头注文档）——零迁移零引擎改动零写语句。
2. **caller 读面 additive**：/api/runs caller_id 投影 + /api/runs|agent-runs|workflow-runs `?caller=` 过滤（server 只增量）。
3. **web caller 维度与截图入库**：RunsPage/AgentsPage/RunDetailPage/RunPanel caller 列/徽标/过滤 + api.ts 类型 + runs-smoke additive + playwright.runs.config.ts + runs-caller.spec.ts + `web/qa/runs/` ≥4 帧入库。
4. **测试面**：`apps/server/selftest_ma4.sh`（四形态端到端各一节 + SSE 实时性专节 + N=4 双口径压测 + V-9 SKIP 守卫预挂 + 生产库指纹）。
5. **文档面**：`docs/rip3/MA-4-BATCH-REPORT.md`（命令原文 + 数字实测 + 坏消息单列 + 锚点勘误）+ docs/README.md 登记。
6. **MA 轨收官记账**（批次报告专节）：§八完成定义七项逐条对账表（四形态各一条真实 e2e / per-caller 预算硬拒 / 双 AI 协作 workflow 节点 / 并发知识写入冲突治理 / 事件推送实时性 / 多 key 审计三态 / 禁模拟）→ 逐项指向 MA-1/MA-2/MA-3/MA-4 批次报告数字原文 + MA-DSH-REFERENCE R1-R9 落地对照表（R1/R2/R3 MA-3、R5/R7 MA-2、R6 MA-3 词值、R8 设计注意点、R4 建队与 R9 深化登记）——MA 轨至此收官，完成定义未达标项如实单列。
7. **Plugin Center 观察池登记提醒**（批次报告遗留节显式登记，不在 MA 轨实现）：E1/E4/E5（目录站分类/市场内浏览/单插件形态，MA-DSH-REFERENCE:33/:36/:37）+ dsh-im IM webhook 通知渠道（:67）+ R9 DAG/成员树/模型标注深化 + WSW-5 UI 遗留面（WSW-4-BATCH-REPORT:278 §5.5 四区页面交接）——后续批若立项按观察池入口核对。
