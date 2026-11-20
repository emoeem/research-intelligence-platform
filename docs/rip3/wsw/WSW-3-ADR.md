# WSW-3 批次裁决 · HTTP History + Repeater

- 文档类型：WSW-3 批次 ADR（承接 WSW-2-ADR W2-D4/D5/D9 遗留授权面 + DESIGN `docs/rip3/wsw/05-DESIGN-AND-THREAT-MODEL.md` §1.2/§3.2/§3.4 + PLAN `docs/rip3/WEB-SECURITY-WORKBENCH-PLAN.md` §4.2/§4.3/§5.3/§5.4/§6.3/§6.4）
- 日期：2026-11-20
- 状态：裁决定稿（WSW-3 设计裁决员产出；实施以 `WSW-3-TASK.md` 为准）
- 证据口径：引用 path:line 均为本会话回读核实；每条裁决后附实现入口位。
- 硬验收口径声明：原手册 `/media/emo/code/pensting/WSW-CONTINUATION-WORKFLOW.md` 已丢失（固化结论 `docs/rip3/wsw/WSW-CONTINUATION-REDLINES.md:3-5`），其"§4 W3 行"按 REDLINES 第 5 条从仓库内可考来源重建——承载源 = PLAN §九 :605（W3 行：History 列表详情、过滤搜索、请求编辑、重放、Tab 历史；不包含大规模 Fuzz）+ WSW-2-ADR W2-D4（"wsw_exchange 元数据表留 WSW-3"）/W2-D5 查点 B（"帽语义逐跳计数接线留 WSW-3"）/W2-D9 + DESIGN §3.2（wsw_exchange/wsw_repeater_send 归属 WSW-3/4）+ 本批任务书 §四。外部口径与本档冲突时，以 PLAN / WSW-2-ADR / 本档为准。

---

## 一、裁决总览（W3-D1 ～ W3-D8）

| # | 裁决 | 一句话 |
|---|---|---|
| D1 | **025 纯新增四表**：`wsw_exchange`（单行纪律：一 exchange 一行，请求+响应同行，响应未到 = 响应列 NULL）+ `wsw_repeater_tab` / `wsw_repeater_tab_version`（append-only 版本）+ `wsw_repeater_send`；Body 大内容不入行，只落 CAS BodyRef 派生列；链头 024→025 单文件单版本推进 | DESIGN §3.2 "WSW-2/3 拆批" + W2-D4 遗留授权收口 |
| D2 | **capture→落库异步化**：有界 mpsc（cap 2048）+ 单 worker + 批量事务（≤128 行或 50ms 窗口 flush）；满载 try_send 失败 → 丢行 + 计数 + 结构化日志，**绝不阻塞转发热路径**；WAL 启用（既有 `database::Pool::enable_wal`，`pool.rs:258`）；代理增量 <50ms 预算延续 W2 口径 | PLAN §11.6 :744-754 数字实测纪律；W2 handler "persist 失败放行" 先例 `listener/handler.rs:510` |
| D3 | **游标分页协议**：排序键 `(started_at DESC, exchange_id DESC)`；cursor = base64url(JSON{t,id,f})，f = 过滤指纹（归一化过滤参数 sha256 前 16 hex），指纹不匹配 → 400；limit 1..500 默认 100；禁 OFFSET、禁整表返回；列表端点元数据投影（不选 headers_json/notes 大列）；Body 按需独立端点（单响应帽 8MiB，超帽 413） | PLAN §4.2 :174"列表不能一次把全部 Body 载入内存"、:404"虚拟滚动、服务端分页或游标" |
| D4 | **发送引擎落位 = `wsw-runtime` 内 `sender.rs` 模块 + `rp-wsw send` 子命令**（不新建 crate）；发送路径 = `wsw_message::serialize::serialize_request` → 原始 h1 字节写（TcpStream / tokio-rustls 0.26.6）→ `parse_wire` 读回；**否决 hyper-util legacy client 写路径**（HeaderMap 迭代序 ≠ 编辑头序，违反 WSW-1 不变式 2）；hyper-rustls/tokio-rustls 只承担 TLS/connector 面；重定向逐跳 re-serialize + `scope::check_redirect_hop` 帽接线（W2-G10 遗留收口）+ 查点 C 复审 | W2-D1 单 crate 边界延续；PLAN :481-482 四表示纪律；serialize.rs:44 "编辑头序保持" |
| D5 | **编辑器/虚拟滚动选型 = 零新依赖**：Raw 编辑器 = 最小 textarea（等宽 + 既有 daisyUI 类）；结构化编辑 = 现有表单组件；虚拟滚动 = 自研窗口化列表（固定行高 + overscan）；否决 CodeMirror/monaco/@tanstack/react-virtual（03 UI 审计 :78 全缺件、:200 新依赖纪律，PLAN §十六.6 开放项保持开放登记 W4+ 复审） | `docs/rip3/wsw/03-UI-COMPONENT-AUDIT.md:78,171,200`；AI-IMPLEMENTATION-GUIDE :28 "不创建平行设计系统" |
| D6 | **Send to Repeater = 编辑态重建语义（非逐字节 Wire 重放）**：History 行 → OutboundRequest 草稿（headers JSON + CAS body 读回），fidelity 标注 `degraded_no_wire`（W2 捕获面 body-only wire，头区无原始字节）；source=repeater 的发送因 outbound bytes 已 CAS 可逐字节重发；UI 保真徽标禁假成功 | PLAN :482"不能声称逐字节原样重放"；W2-D9 捕获事实面实态 |
| D7 | **History API 写权分治 + 搜索面**：读面 rp_server 只读连接（mode=ro，`rp_server.py:3163-3165` intercept-queue 先例）；写权分治 = exchange/intercept/send 行 Rust-only（listener worker + rp-wsw send），tags/notes/tab/tab_version 小控制行 Python 直写（busy_timeout + WAL）；搜索 = 有界 `search_text` 投影 LIKE（ authority+path+头区+体前 4KiB），FTS5 显式缓议（可用但 CJK tokenizer/索引维护成本不值，p95 门红再升） | PLAN :571 搜索面 + §7.4 :543 原始证据/脱敏展示分离 |
| D8 | **UI 落位 = 新 `websec` 工作区两页**（order 12，entry `/websec/history`）：`/websec/history`（History）+ `/websec/repeater`（Repeater）——PLAN §5.1 :337"增加一个明确的 Web Security 工作区"的落位；W2 Settings 两 tab 不动；页面范式沿 WswProxyPage（useApi/apiSend/StatusBadge/data-testid） | `web/src/workspaces.ts:295-317` W2 注册先例；PLAN §5.3/§5.4 页面规格 |

---

## 二、逐项裁决

### W3-D1 025 迁移与 exchange 表形状

**决定**：`migrations/025_wsw_exchange.sql`，链头 024→025，纯新增零 ALTER（024 文件头 :5-6 纪律同源；`schema_version` INSERT OR IGNORE (25,…) + `PRAGMA user_version = 25` 双写，024 :87-93 同款）。

```sql
-- 四表形状（列 CHECK 词表逐列，ID 前缀沿 ids.rs 范式生成侧承载）：
wsw_exchange(
  exchange_id TEXT PK CHECK (exchange_id LIKE 'wsw-%'),   -- listener/handler.rs:392 new_exchange_id 生成面
  session_id  TEXT NOT NULL CHECK (session_id LIKE 'wsw-ses-%'),
  listener_id TEXT NOT NULL CHECK (listener_id LIKE 'wsw-lis-%'),
  sequence_no INTEGER NOT NULL,                            -- UNIQUE(listener_id, sequence_no)
  direction   TEXT NOT NULL CHECK (direction IN ('client_to_server','server_to_client')),
  source      TEXT NOT NULL CHECK (source IN ('proxy_capture','repeater','fuzzer','import')),
  protocol    TEXT NOT NULL,                               -- 'http'/'ws'（exchange.rs:90 词表）
  http_version TEXT, tls_version TEXT, alpn TEXT,
  method TEXT, scheme TEXT, authority TEXT, path TEXT,     -- 检索主投影
  response_status INTEGER,
  request_headers_json  TEXT,                              -- Parsed 头序保持投影（detail 端点专用，列表不选）
  response_headers_json TEXT,
  -- Body 大内容不入行：只落 CAS BodyRef 派生列（cas.rs:34-46 sha256/uri/size/fidelity）
  request_body_sha256  TEXT CHECK (NULL OR length=64 hex), request_body_uri TEXT,
  request_body_len INTEGER, request_mime TEXT, request_body_fidelity TEXT CHECK ('full','truncated'),
  response_body_sha256 TEXT, response_body_uri TEXT,
  response_body_len INTEGER, response_mime TEXT, response_body_fidelity TEXT,
  started_at TEXT, completed_at TEXT, duration_ms INTEGER, ttfb_ms INTEGER,  -- ISO8601ms GLOB 同 024 口径
  remote_ip TEXT,
  scope_decision TEXT CHECK (NULL OR IN ('in_scope','out_of_scope_passive','out_of_scope_blocked')),  -- NULL=Unknown（exchange.rs:58-61 第三态）
  edited INTEGER NOT NULL DEFAULT 0 CHECK (edited IN (0,1)),
  tags_json TEXT NOT NULL DEFAULT '[]', notes TEXT,
  trace_id TEXT, task_id TEXT, tool_run_id TEXT, artifact_id TEXT, evidence_id TEXT,  -- 六 ID 引用列
  capture_fidelity TEXT NOT NULL CHECK (IN ('full','truncated','degraded_no_wire','import')),
  parse_status TEXT NOT NULL CHECK (IN ('ok','partial','failed')),
  error_code TEXT,
  search_text TEXT                                         -- D7 有界搜索投影（≤8KiB，insert 侧生成）
)
-- 索引 = 检索查询面（D3 游标 + 过滤维度）：
UNIQUE(listener_id, sequence_no)
ix_wsw_exchange_cursor   (session_id, started_at DESC, exchange_id DESC)   -- 游标主序
ix_wsw_exchange_authority(authority, started_at DESC)                      -- 域名过滤
ix_wsw_exchange_method   (method), ix_wsw_exchange_status (response_status)
ix_wsw_exchange_source   (source, started_at DESC)                         -- 来源过滤
ix_wsw_exchange_started  (started_at DESC, exchange_id DESC)               -- 无 session 全局面游标

wsw_repeater_tab(
  tab_id TEXT PK CHECK (tab_id LIKE 'wsw-rpt-%'),
  session_id TEXT NOT NULL CHECK (session_id LIKE 'wsw-ses-%'),
  title TEXT NOT NULL,
  created_at / updated_at TEXT NOT NULL, closed_at TEXT       -- closed_at NULL=打开
)
wsw_repeater_tab_version(                                     -- append-only：禁 UPDATE 禁 DELETE（除 retention 批）
  tab_id TEXT NOT NULL REFERENCES wsw_repeater_tab(tab_id),
  version_no INTEGER NOT NULL,
  request_json TEXT NOT NULL CHECK (length(request_json) <= 8388608),  -- OutboundRequest serde JSON（体 base64 内联，8MiB 帽超帽 413 拒）
  source_exchange_id TEXT CHECK (NULL OR LIKE 'wsw-%'),
  origin TEXT NOT NULL CHECK (origin IN ('manual','from_history','restore','send_snapshot')),
  created_at TEXT NOT NULL,
  UNIQUE(tab_id, version_no)
)
wsw_repeater_send(
  send_id TEXT PK CHECK (send_id LIKE 'wsw-snd-%'),
  tab_id TEXT NOT NULL, version_no INTEGER NOT NULL,
  session_id TEXT NOT NULL,
  exchange_id TEXT CHECK (NULL OR LIKE 'wsw-%'),              -- 发送产出 exchange（source=repeater）
  state TEXT NOT NULL CHECK (state IN ('pending','sending','succeeded','failed','cancelled','timed_out')),  -- PLAN §6.4 :501 词表
  request_outbound_sha256 TEXT,                               -- OutboundBytes CAS（逐字节重发依据，D6）
  redirect_chain_json TEXT,                                   -- 逐跳 {status,location,url,remote_ip}
  response_status INTEGER, response_body_sha256 TEXT, response_body_len INTEGER, response_mime TEXT,
  remote_ip TEXT, started_at TEXT, completed_at TEXT, duration_ms INTEGER, ttfb_ms INTEGER,
  error_code TEXT, created_at TEXT NOT NULL
)
-- 索引：ix_wsw_repeater_send_tab (tab_id, created_at DESC)；ix_wsw_repeater_version (tab_id, version_no DESC)
```

- **单行纪律**：一 exchange 一行（请求+响应同行；响应未到 = 响应列 NULL + `parse_status='partial'`，连接中断如实落行不伪造完成态）；Body 大内容走 CAS（023 `wsw_body_blob` 已备）不入行——行内只保留 sha256/uri/len/mime/fidelity 派生列，Body 字节读回走 D3 按需端点。链头 024→025 **单文件单版本**推进，禁拆多迁移、禁改历史迁移。
- **迁移钉值债（批内必做）**：025 入链需把迁移钉值 24→25 全跟上。钉值形态 = `vec![1,2,…,24]` 显式链清单 + 链尾断言 `(24, "_wsw_intercept")`/`Some(24)` + `001..024` 注释 + user_version 断言（5d146e24 实录：五文件清单 + 漏网两文件共七文件；该批修复前全仓扫描模式 = "1, 2, … 22, 23" 链清单 + `001..024`/chain tail 024/`must land 024`/`user_version must be 24` 消息模式，同模式批内重扫，不预设文件数）。
- **否决备选**：请求/响应拆两行（Exchange 类型 request/response 同体 exchange.rs:76-149，拆行破坏单行纪律与 duration/ttfb 语义）；headers 拆副表（detail 按需 JSON 列即可，副表 join 不值）。

### W3-D2 capture→落库异步化与背压

**决定**：

1. **组装点**：handler 请求入口 snapshot（method/scheme/authority/path/头投影/exchange_id，`listener/handler.rs:391-392` 已有 ID 生成面）→ 响应流末与 `capture_finish` 同点（`listener/handler.rs:707`）join 成 Exchange → push channel。响应中断/连接异常 → 立即落 partial 行（completed_at NULL）。
2. **通道**：`tokio::sync::mpsc::channel(2048)` 有界；push 用 `try_send`，**满载 → 丢弃该行 + `persist_dropped_total` 原子计数 + tracing_error 结构化日志**，不等待不重试——转发热路径零阻塞（W2 先例：`listener/handler.rs:510` "persist 失败可见但放行"）。
3. **worker**：listener 进程内单 worker task；批量事务落行（每批 ≤128 行或 50ms 无新到即 flush）；SQLite 单写者天然串行，批量降 fsync 次数。CAS 写（体字节）维持既有流末 spawn 异步面（`handler.rs:674-675` → `capture_finish` :707）不动，exchange 行与 blob 行均由 worker/流末异步完成。
4. **WAL**：`PersistStore::open_file` 后调 `enable_wal()`（`crates/database/src/pool.rs:252-260` 既有 API，P1-1 先例）——rp_server 只读连接（D7）与 Rust 写者并发不互锁；`-wal/-shm` 文件随 `*.db` gitignore（`.gitignore:35`）不入库。
5. **保真词表落列**：头区来自 hudsucker http 视图（非原始 wire 字节）→ `capture_fidelity='degraded_no_wire'`（`wsw-message types.rs:255` 词表消费，`W2-D9` 实态如实落库）；体 fidelity 从 BodyRef.c fidelity 继承。
6. **性能门**：代理增量 <50ms（直连 vs 经代理 time_total 中位数差，W2-TASK §4.5 口径延续）；落库失败不改变客户端可见行为（只计数）。

**否决备选**：同步落库（阻塞转发，直接违反预算）；无界 channel（慢库时内存无帽）；独立持久化进程（跨进程 IPC 面 W3 不值得，worker 在 listener 进程内）。

### W3-D3 游标分页协议

**决定**：

| 面 | 裁决 |
|---|---|
| 排序键 | `(started_at, exchange_id)` DESC 复合键（时间同毫秒由 exchange_id 决胜，游标稳定）；`exchange.sequence_no` 留给 per-listener 对账面不作为默认排序键 |
| cursor 编码 | `base64url(JSON{t: started_at, id: exchange_id, f: 指纹})`，对客户端不透明、服务端不校验有效期；f = 归一化过滤参数字典序拼接后 sha256 前 16 hex |
| 指纹守卫 | cursor.f ≠ 当前请求过滤指纹 → `400 filter_cursor_mismatch`（防过滤变更下游标漏行/重行） |
| 分页量 | `limit` 1..500 默认 100；响应 `{rows, next_cursor, has_more}`；`next_cursor` 空 = 末页；**禁 OFFSET**（深分页退化 + 整表遍历面） |
| 列表投影 | 列表 SQL 禁选 `request_headers_json/response_headers_json/search_text/notes` 大列；行对象 = 元数据投影（method/scheme/authority/path/status/len/mime/tls/duration/source/scope/edited/tags/时间列）——PLAN :404"不允许一次性加载所有 Body"的 SQL 面落地 |
| Body 按需 | `GET /api/wsw/history/{id}/body?direction=request\|response`：查 `wsw_body_blob` → 按 uri 读 CAS 文件（`cas.rs:265 read_body` 同型路径校验）；单响应帽 8MiB 超帽 413；响应带 `x-wsw-body-sha256` 校验头；列表/详情端点零 body 字节 |
| 详情 | `GET /api/wsw/history/{id}` 返回全元数据 + headers JSON + body 引用列，**不含 body 字节**；`PATCH /api/wsw/history/{id}` 只接受 tags/notes 两字段（D7 写权面） |

**否决备选**：OFFSET 分页（不稳定排序 + 深页慢）；keyset-only on sequence_no（全局面无 listener 维度语义）；body 内嵌 detail 响应（违反按需纪律）。

### W3-D4 Repeater 发送引擎落位

**决定**：

1. **落位**：`crates/wsw-runtime/src/sender.rs` 模块 + `crates/wsw-runtime/src/bin/rp-wsw.rs` 增 `send` 子命令（rp_server 子进程先例 `rp_server.py:3256` Popen spawn rp-wsw listener；W2-D1 "不新建 crate" 边界延续）。入口签名：`rp-wsw send --db <WSW_DB> --session-id <ses> --tab-id <rpt> --send-id <snd> --version-no <n> --request-json <file> --policy <file> [--upstream-ca <pem>]`。
2. **发送路径（Rust-First，禁 reqwest）**：`OutboundRequest` → `wsw_message::serialize::serialize_request`（`serialize.rs:219`，唯一 Outbound 事实，不变式 2 serialize.rs doc）→ **原始 h1 字节写**：明文 `tokio::net::TcpStream` / TLS `tokio_rustls::TlsConnector`（tokio-rustls `=0.26.6` 已直依赖 `crates/wsw-runtime/Cargo.toml:66`；client config 组装沿 `listener.rs:412-424` 同型面）→ 响应字节缓冲（帽 max_body_bytes）→ `wsw_message::parse::parse_wire`。outbound bytes 全量落 CAS（`wsw_repeater_send.request_outbound_sha256`）。
3. **否决 hyper-util legacy client 写路径**：`http::HeaderMap` 迭代序 ≠ 编辑头序（WSW-1 serialize "编辑头序保留重复头与头序" serialize.rs:44；PLAN :202 "保留重复 Header、重复参数和参数顺序的表达能力"）——hyper 写路径会静默重排/重定界用户构造的请求，违反四表示纪律（PLAN :481-482"发送时重新序列化的内容"必须是显式 Outbound）。hyper-rustls/tokio-rustls 仅承担 TLS/connector 面。
4. **重定向策略**：`none` / `follow(max_redirects=10)`；跟随 = 逐跳从响应 Location 构造下一跳 OutboundRequest + `serialize_request` 重序列化（每跳都是显式 Outbound）+ **`scope::check_redirect_hop` 帽逐跳计数接线**（W2-D5 查点 B 修订批口径的 W3 收口项，`scope.rs` 帽入口已单测级就位）+ 每跳过 AuditedConnector 同款查点 C IP 审计；越界跳 → state=failed + error_code=scope_blocked + redirect_chain 可见（PLAN :528）。
5. **超时/取消**：connect/read/total 三超时（policy JSON，默认 connect 10s / read 30s / total 60s）；取消 = rp_server 对子进程 SIGTERM → 子进程 graceful handler 落 `state='cancelled'`（非终态时），PLAN §6.4 :501 六态词表全收敛。
6. **落库**：send 子命令自落 `wsw_repeater_send` 行（pending→sending→终态）+ 产出 exchange 行（source=repeater，经 parse_wire 解析、头投影入行、体走 CAS）；scope 规则从 `wsw_session.scope_rules_json` 读（default deny 同 W2；out-of-scope 主动发送一律拒，PLAN :527"主动面不自动扩大范围"）。

**否决备选**：独立 wsw-sender crate（W2-D1 门禁面按 `-p wsw-runtime` 固化）；hyper client 写路径（D4.3 头序）；reqwest（REDLINES 红线 2 Rust-First 同族 + 头序/保真不可控，`grep -c 'name = "reqwest"' Cargo.lock` 当前 = 0 保持）。

### W3-D5 编辑器与虚拟滚动组件选型

**决定**：

| 面 | 裁决 | 依据 |
|---|---|---|
| Raw 编辑器 | 最小 `<textarea>`（等宽字体 + 现有 daisyUI 类 + 换行控制开关），无行号无高亮 | 03 审计 :78 "现编辑形态仅 `<pre>` 只读与表单 input/textarea"；:200 新依赖（CodeMirror 等）须走依赖审计 + 大文本性能验证——本批验收面（编辑→发送往返）不要求语法高亮 |
| 结构化编辑 | 现有表单/表格组件（Headers/Query/Params 逐行编辑） | PLAN §5.6 :431"表格、过滤器……沿用现有组件"；AI-GUIDE :28 不建平行设计系统 |
| 详情视图 | Raw/Pretty/Headers/Params/Body/Hex 六视图，从 ResearchEvidencePage Raw/Pretty `<pre>` 雏形抽共享组件 | 03 审计 :171 明文"需抽成通用 MessageViewer/MessageEditor" |
| 虚拟滚动 | 自研窗口化列表（固定行高 + scrollTop 换算 + 上下 overscan 各 5 行），零新 npm 依赖 | 否决 @tanstack/react-virtual（同 :200 纪律；e2e 断言面"仅视口渲染"自研列表可测：DOM 行节点数 ≤ 窗口+overscan） |
| 大文本 | textarea 只挂当前编辑 Tab；详情 Body >256KiB 时 Raw/Pretty 折叠为按需加载提示（防一次性注入 DOM） | PLAN :433 大文本性能要求的最低满足面；Hex 视图分块渲染 |

**否决备选**：CodeMirror/monaco（PLAN §十六.6 开放项**保持开放**——本批不拍板引入，登记 W4 Fuzzer 批复审：Fuzzer 面高亮需求更强时一并走依赖审计）；虚拟库引入（依赖审计周期 > 本批收益）。

### W3-D6 Send to Repeater 请求构造语义

**决定**：**编辑态重建，非逐字节 Wire 重放**。

1. History 行动作 "Send to Repeater" → 后端从 `request_headers_json`（Parsed 头序投影）+ CAS body 读回组装 `OutboundRequest` 草稿 → 落 `wsw_repeater_tab_version`（origin='from_history'，source_exchange_id 引用）→ UI 打开 `/websec/repeater?tab=<id>`。
2. **保真如实标注**：W2 捕获面是 body-only wire（`listener/handler.rs` TeeBody 只 tee 体字节，`W2-D9` 实态），请求头区无原始 wire 可回放——重建草稿 fidelity = `degraded_no_wire`，UI 徽标明示"重建自 History：头序/绝对形式不保证逐字节"（PLAN :482"不能声称是逐字节原样重放"禁假成功）。
3. **精确重发面只对 source=repeater 的发送开放**：其 outbound bytes 已 CAS（D4.2），发送历史版本行提供"按 OutboundBytes 重发"入口——逐字节保真路径存在且引用明确，两语义分层不混用。
4. 草稿首版 `edited=0` 语义 = 与 History 原始投影一致；用户任何改动 → 新版本行（append-only，D1 版本表）+ 发送时 exchange 行 `edited=1` 判定 = outbound ≠ 源投影。

**否决备选**：捕获期全量 tee 请求 wire（须改 hudsucker handler 预读整请求——与流式转发/大 Body 内存边界冲突，W3 不做，登记 fidelity 债）；假成功直发 headers JSON 当原始 wire（禁）。

### W3-D7 History API 写权分治与搜索面

**决定**：

| 写面 | 执行者 | 理由 |
|---|---|---|
| `wsw_exchange` 行（capture 通道） | Rust listener worker（D2） | 高频批量面；Python 单行 insert 逐条开销不值 |
| `wsw_exchange` 行（send 通道） | rp-wsw send 子命令（D4.6） | Rust-First 发送事实面 |
| `wsw_intercept` 行 | Rust listener（W2 现状不动） | W2-D6 双面语义延续 |
| `wsw_repeater_send` 行 | rp-wsw send 子命令 | 同 send 通道 |
| tags/notes PATCH、tab/tab_version 行 | rp_server Python 直写（busy_timeout 5s + WAL） | 用户显式编辑动作非热路径；subprocess 每编辑一次不值；WAL 下小事务与 worker 批量写竞争窗口毫秒级 |

- **读面**：history/list、detail、body、send-status 等全部只读连接 `mode=ro` + `PRAGMA query_only`（`rp_server.py:3163-3165` intercept-queue 先例）。
- **搜索面**：`search_text` 有界投影（authority + path + 头名/头值 + 体前 4KiB UTF-8 lossy 文本化，二进制体 → 空投影，总帽 8KiB）+ SQL `LIKE` 大小写折叠；**FTS5 显式缓议**——bundled SQLite 有 FTS5（`vendor/libsqlite3-sys/build.rs:177 SQLITE_ENABLE_FTS5`）但 CJK tokenizer 缺省不可用 + 索引维护进 D2 批量写路径 + 行帽（DESIGN §3.4 retention）下 LIKE 全扫毫秒级：p95<300ms 门红连续两轮再升 FTS5 另立裁决。
- **敏感面**：detail/body 走既有 Bearer 鉴权（`rp_server.py:3157-3168` 先例）；UI 展示投影挂 `wsw_message::redact`（`redact.rs:55` 唯一挂点，W2-D7 日志面同款），原始行面与脱敏展示分离（PLAN :543）；search_text 生成时**排除 Authorization/Cookie/Set-Cookie 头值**（可搜头名不搜凭据值，DESIGN §4.5）。
- **retention 边界**：本批只落 DESIGN §3.4 的行数帽查询面（`wsw_exchange` 行数 + blob 计数暴露 status 端点），超帽 LRU 淘汰执行器**不属本批**（留 retention 批，023 注释 "GC 执行位归 WSW-3 retention" 口径如实在批次报告登记为遗留）。

### W3-D8 UI 落位与路由注册

**决定**：

1. `web/src/workspaces.ts` 增工作区 `{ id: "websec", order: 12, label: "Web Security", entry: "/websec/history" }`，tabs：`/websec/history`（HTTP History，PLAN §5.3 规格）+ `/websec/repeater`（Repeater，PLAN §5.4 规格）；`web/src/App.tsx` 增两路由；Settings 工作区 11.2/11.3（W2-D8）零改动。
2. 页面范式沿 `web/src/routes/WswProxyPage.tsx`（useApi/apiSend/ApiError/StatusBadge/data-testid 命名 `wsw-history-*` / `wsw-repeater-*`）；`web/tests/workspaces.test.ts` 路由钉值 20→22 更新（5d146e24/16401fcb 钉值更新先例，如实记录）。
3. History 页布局 = PLAN §5.3 :396-404：顶部过滤构建器（条件行：域/方法/路径/状态/MIME/来源/标签/时间 + 关键词搜索框，防抖 300ms）→ 主体虚拟滚动列表（默认时间倒序）→ 下方详情区（六视图 + 行操作 Send to Repeater/复制）。Repeater 页 = PLAN §5.4 :406-415：顶部 Tab 栏（新建/重命名/关闭/版本历史下拉回退）→ 左 Request 编辑器（Raw/结构化切换）右 Response 视图（状态/耗时/长度/协议头 + Raw/Pretty/Hex）→ Send/Cancel（发送中态 + 版本快照）。

---

## 三、复现命令（本会话实跑核实）

```bash
cd /media/emo/code/pensting/research-platform
grep -cE '^\[\[package\]\]' Cargo.lock                        # → 399（工作树 = HEAD 0339a1eb；WSW-2 档 400 为历史快照，SEC-0 76c6feb6 淘汰 hashbrown 后实态）
git show HEAD:Cargo.lock | grep -cE '^\[\[package\]\]'        # → 399（lock 红线对比基准 = HEAD）
grep -c 'name = "reqwest"' Cargo.lock                         # → 0（禁 reqwest 红线现状）
grep -n 'SQLITE_ENABLE_FTS5' vendor/libsqlite3-sys/build.rs   # → :177（D7 FTS5 可用性依据，缓议非引入）
grep -n 'pub fn enable_wal' crates/database/src/pool.rs       # → :258（D2 WAL 既有 API）
grep -n 'pub fn serialize_request' crates/wsw-message/src/serialize.rs   # → :219（D4 Outbound 唯一事实面）
sed -n '76,149p' crates/wsw-message/src/exchange.rs           # → Exchange 全字段（D1 表列映射源）
sed -n '506,512p' crates/wsw-runtime/src/listener/handler.rs  # → persist 失败放行先例（D2.2）
sed -n '3159,3168p' apps/server/rp_server.py                  # → ro 连接先例（D7 读面）
grep -n 'capture_finish' crates/wsw-runtime/src/listener/handler.rs | head -2  # → :656/:707（D2.1 组装点）
python3 -c "import sqlite3; c=sqlite3.connect('file:migrations/dev-b1.db?mode=ro',uri=True); print(c.execute('PRAGMA user_version').fetchone()[0])"  # → 24（025 链头推进起点，本会话实测）
grep -rn "1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24" crates/ --include="*.rs" -l | wc -l   # → vec 链清单钉值待跟面（D1 钉值债实勘入口之一；另扫 001..024/Some(24)/must land 024/user_version must be 24 四模式）
grep -rn "001\.\.024" crates/ --include="*.rs" | wc -l        # → 链注释钉值面
```
