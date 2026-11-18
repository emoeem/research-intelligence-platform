# WSW 前置审计（00）：Rust 网络栈现状实测基线

> 文档类型：审计基线 / WSW Proxy / MITM 批次的前置事实依据
>
> 日期：2026-11-18（全部结论为本日会话内 grep / read / cargo tree / python 解析 Cargo.lock 实测；未轻信任何文档描述）
>
> 上游规划：`docs/rip3/WEB-SECURITY-WORKBENCH-PLAN.md`（WSW-2 = Proxy / MITM Runtime：Listener、HTTP/HTTPS CONNECT、TLS CA、拦截队列、Forward / Drop，:604）
>
> 判定口径：**【已具备】**（有实现有测试）/ **【部分具备】**（有最小实现，能力面与 WSW 需求有明确缺口）/ **【未发现】**（实测 grep/read 零命中，禁止把"应该有"写成"有"）。
>
> 仓库根：`/media/emo/code/pensting/research-platform`；路径均为仓库相对路径。

---

## 一、Workspace 依赖钉值面（Cargo.toml / Cargo.lock 实测）

### 1.1 总图：网络栈 crate 覆盖面

`Cargo.lock` 共 285 个 package（python3 解析逐块计数），其中**网络协议栈相关 crate 零条目**：

```
grep -E '^name = "(hyper|axum|reqwest|rustls|native-tls|tungstenite|tokio-tungstenite|h2|http|http-body|tower|tower-http|socks|hyper-rustls|hyper-tls)"$' Cargo.lock
→ 零命中
```

存在的网络相关包只有运行时底座一族：

| package | 版本 | 谁在消费（`cargo tree -e features -f "{p}" -i socket2 --offline` 实测） |
| --- | --- | --- |
| tokio | 1.54.1 | mcp-server（rmcp 运行时面）+ intel-db 链（postgres 同步包装的内部 tokio-postgres） |
| tokio-macros | 2.7.2 | tokio |
| tokio-util | 0.7.20 | rmcp、tokio-postgres |
| tokio-postgres | 0.7.18 | postgres 0.19.14（同步阻塞包装） |
| mio | 1.2.5 | tokio（由 net feature 带入） |
| socket2 | 0.6.5 | tokio（由 net feature 带入） |

**核心结论：本仓库 Rust 侧没有任何 HTTP client/server、TLS、HTTP/2、WebSocket、SOCKS 库。** 所有 HTTP 行为都是 `std::net::TcpStream` 手写实现（见 §2）。

### 1.2 workspace 级钉值（Cargo.toml:33-66 `[workspace.dependencies]`）

| 依赖 | 钉值 | 特写 | 证据 |
| --- | --- | --- | --- |
| tokio | `"1"` | `default-features = false, features = ["rt","rt-multi-thread","macros","io-std","io-util","sync","time"]` —— **workspace 声明不含 `net`** | Cargo.toml:57 |
| rmcp | `"=3.10.0"`（等号钉死） | `default-features = false, features = ["server","transport-io"]`（不引入 base64/macros 与 client/auth/HTTP 传输） | Cargo.toml:54；实测锁定 3.10.0 |
| rusqlite | `"0.32"` | `bundled, backup` | Cargo.toml:39 |
| postgres | `"0.19"` | `with-serde_json-1`（同步栈，ADR-0002） | Cargo.toml:61 |
| r2d2 / r2d2_postgres | `0.8` / `0.18` | — | Cargo.toml:62-63 |
| refinery | `"0.8"` | `postgres` | Cargo.toml:65 |
| pgvector | `"0.4"` | `postgres` | Cargo.toml:67 |
| serde / serde_json / toml / serde_yaml_ng | `1.0` / `1.0` / `1.1` / `0.10` | — | Cargo.toml:40-47 |

crate 级二次钉值（四 crate 同钉一处版本、镜像滞后注释在案）：

- `jsonschema = { version = "0.56", default-features = false }`：tool-registry（Cargo.toml，注释：rsproxy.cn 镜像对 jsonschema 只镜像到 0.56.0）、research-engine、workflow-engine、verification-engine 四处同钉；default-features 关闭远程/文件 `$ref` 解析（fail-closed）。
- `tokenizers = "0.21.4"`（context-broker）：`default-features = false, features = ["fancy-regex"]`，关 http/progressbar。
- `sha2 = "0.10"`（tool-registry/database/intel-db/research-engine/verification-engine 五处）、`tar = "0.4"`（database/plugin-registry，无压缩 .tar 口径）、`data-encoding = "2"`（database）。

### 1.3 tokio `net` feature 的实际归因（cargo tree 实测）

workspace 声明不含 `net`，但 lock 树里 tokio 带着 mio+socket2。逐边归因：

```
vendor/tokio-postgres/Cargo.toml:43   default = ["runtime"]
vendor/tokio-postgres/Cargo.toml:48-51 runtime = ["tokio/net", "tokio/time"]
vendor/tokio/Cargo.toml:96-104        net = ["libc","mio/os-poll","mio/os-ext","mio/net","socket2",...]
```

即 **tokio "net"（mio 1.2.5 + socket2 0.6.5）是 `postgres` 0.19 → tokio-postgres 0.7.18 默认 `runtime` feature 间接带入的**，不是显式声明。rmcp 自身只要 `sync, macros, rt, time`（vendor/rmcp/Cargo.toml:908-914）+ transport-io 的 `tokio/io-util`。

**WSW 含义**：引入 hyper/hudsucker 后 tokio `net` 必须在 workspace 显式声明（当前是隐式传递，语义上不可依赖）；vendor 红线（.cargo/config.toml `vendored-sources` + vendor/ 289 目录，--offline 构建）意味着新增网络栈 crate 必须 cargo vendor 入库并过 deny.toml 门禁。

### 1.4 构建与目标纪律

- `.cargo/config.toml`：`replace-with = "vendored-sources"`，`directory = "vendor"`（--offline 红线，rp-mcp-parity/Cargo.toml 注释原文："vendor 无 HTTP client crate，引入即破坏 --offline 红线"）。
- `Cargo.toml:31` `rust-version = "1.99"`、edition 2021、resolver 2；`[profile.release] strip = "debuginfo"`（Cargo.toml:87-89）。

> 行号口径注（2026-11-18 评审修订批补）：本节 `Cargo.toml:N` 原为审计时点快照（HEAD 32c452c7）；POC 批在 members 数组插入 `crates/wsw-poc` 一行使本节及后续引用整体 +1，修订批已按当前工作树逐条 `grep -n` 复核校正（tokio :57、rust-version :31 等）。后续批次再改 Cargo.toml 会再次漂移，引用前以实测为准。

---

## 二、能力清单逐项带证据

### 2.1 Tokio 运行时版本与 feature ——【部分具备】

- 钉值与 feature 子集见 §1.2；tokio 1.54.1（Cargo.lock）。
- 消费面极窄：**只有 mcp-server**（rp-mcp 的 async main + rmcp stdio transport，mcp-server/Cargo.toml `tokio = { workspace = true }`）与 intel-db 链（postgres 同步包装内部使用）。**全部业务引擎（tool-engine/runtime-gateway/workflow-engine/agent-engine/…）零 tokio**，并发模型是同步 `std::thread` + rusqlite。
- mcp-server 侧用法：stdio serve（crates/mcp-server/src/bin/rp-mcp.rs:45 `use rmcp::service::serve_server;`，EOF 退出）；无 net/无 TCP/无 HTTP。

### 2.2 HTTP client / server 栈 ——【部分具备】

**没有任何 HTTP crate。** 现有 HTTP 实现四处，全部手写 std::net：

| 位置 | 形态 | 关键证据 |
| --- | --- | --- |
| `crates/tool-engine/src/document_fetch.rs` | 生产 HTTP/1.1 GET 客户端（绝对形式经本机 EgressProxy；手写响应解析：Content-Length 精确取 / chunked 解帧（RFC 7230，2026-11-12 冒烟修正）/ EOF；64 KiB 头帽；max_bytes 预截 `truncated_by_transport`） | document_fetch.rs:226-241（ProxyTransport::get）、:247-（read_http_response）、:259 头帽 |
| `crates/plugin-registry/src/source/http_api.rs` | 插件 archive 来源最小 HTTP/1.0 GET（仅 http://，"TLS 归后续批次"） | http_api.rs:11、:27、:46（TcpStream::connect） |
| `crates/rp-mcp-parity/tests/parity.rs` | 测试专用薄 REST 客户端（~80 行；"rp_server 响应恒带 Content-Length，无需 chunked 支持"） | parity.rs:25、:221-232 |
| `apps/server/rp_server.py` | 平台 REST server：stdlib `BaseHTTPRequestHandler + ThreadingHTTPServer`，`/api/*` REST + 静态资源 | rp_server.py:86、:3112（RpHandler）、:3405（main）；绑定 `RP_HOST=127.0.0.1 RP_PORT=7800`（:95-96） |

能力边界（对 WSW 重要的）：
- 手写解析只覆盖 GET / 绝对形式 / 三种体定界；无 keep-alive（`Connection: close` 强制重写，egress_proxy.rs:562）、无压缩解码（gzip/br）、无 Expect/100-continue、无 trailer、无 chunked 请求发送。
- Python 面是薄 REST，不做任何出站 HTTP（无 requests/httpx/urllib.request 导入，实测 grep）。

### 2.3 TLS ——【未发现】

- Cargo.lock 零 rustls/native-tls/openssl/ring 条目（§1.1 实测）。
- 生产抓取显式拒绝 https：`FetchRejection::SchemeUnsupported`，消息原文 `scheme {s:?} unsupported: no TLS stack in this batch; http only`（document_fetch.rs:167）。
- EgressProxy 的 CONNECT 隧道是**盲字节泵**：`tunnel()` 握手 200 后双向转发不解析（egress_proxy.rs:514-533）——字节级透传能力在，TLS 终结/校验/伪造面完全没有。

### 2.4 HTTP/2 ——【未发现】

- 无 h2 crate；手写解析固定 `HTTP/1.1` 首行（egress_proxy.rs:546、document_fetch.rs 状态行解析）；无 Upgrade/ALPN 处理。WSW 规划把 HTTP/2 兼容列为 POC 项（WEB-SECURITY-WORKBENCH-PLAN.md:620、:818），与现状一致：从零起。

### 2.5 WebSocket ——【未发现】

- Cargo.lock 零 tungstenite/tokio-tungstenite；全仓 grep `Sec-WebSocket|Upgrade|event-stream|websocket`（crates/ + apps/server/ + web/src/）零有效命中（仅 plugin upgrade 词面冲突）。

### 2.6 代理（HTTP/CONNECT/SOCKS）——【部分具备】

**已具备：EgressProxy（P3-2）出站白名单代理**，`crates/tool-engine/src/adapter/egress_proxy.rs`：

- run 生命周期监听 `127.0.0.1:<ephemeral>`；**CONNECT 隧道**（:323-365）+ **绝对形式 HTTP 转发**（:366-410，重写为源形式 + `Connection: close` :536-576）；双向泵带 250ms stop 轮询（:580-632）。
- 白名单 = host 精确匹配（无通配无后缀，`host_from_url` 派生自 input.url，:151-189）+ Manifest `egress.ports` 端口枚举（:414-424）；拒绝一律 403 + 结构化 `ProxyEvent::{TunnelAccepted,ForwardAccepted,Rejected}`（:105-131）。
- 容器接线：`--network pasta:-T,<port>` 端口转发 + `http(s)_proxy/HTTP(S)_PROXY` env 指向代理（podman_host.rs:378-390）；上游连接 `connect_timeout 10s`（:31、:499-511）。
- 边界声明（模块文档 :15-18 原文）：策略只约束遵守 env proxy 的客户端（自有可信工具面）；pasta 默认出站未网络层硬隔离——**P5-9c 实测 nuclei 不读 env proxy 直接绕过白名单**，修复 = wrapper 显式 `-proxy` 旗标（docs/rip3/batches/P5-9c-BATCH-REPORT.md §B.1）。这个"客户端自愿走代理"的根本约束对 WSW 是决定性信息：**WSW 是浏览器主动挂代理的显式代理场景，恰好落在同一边界模型上**。

**未发现：SOCKS（socks4/4a/5）任何实现**；未发现代理级联/上游代理（proxy chain）支持；未发现透明代理/TPROXY（规划 :112 明确留后续）。

### 2.7 流式 body 处理 ——【部分具备】

- 无流式 API 概念：document_fetch 全量缓冲进 `Vec<u8>` + `max_bytes` 预截（TransportResponse.truncated_by_transport，document_fetch.rs:186-197）；代理泵是 8KiB 缓冲盲拷贝（egress_proxy.rs:605），无逐段观察钩子（WSW 拦截队列需要）。
- 结果面截断协议已成型：`envelope::truncate`（§5.4）超帽降级为截断说明对象 + 原始字节 sha256 + 建议 artifact_id（envelope.rs:279-321）。
- 进程管道防死锁：podman stdout/stderr spawn 后立即并发读线程排空（podman_host.rs:461-481，P4-4 实录注释）。

### 2.8 artifact 落盘与内容寻址 ——【已具备】

- 表面：`artifact` 表（005_tool_contract.sql:78-90），`sha256 TEXT NOT NULL CHECK(length=64)` + `size_bytes` + `uri` + role 词表。
- 收集：容器 out/ 全产物 sha256/size 逐文件实算登记，`artifact_id = art-{run_id}-{idx}-{name}`，超时/取消/失败路径统一收集并标注 partial（podman_host.rs:651-693）。
- 内容寻址 ID：信封截断产物 `art-<sha256(run_id:serialized)[..16]>`（envelope.rs:297-300）；抓取记录 `fdf-<sha256[..24]>` / 按 `record_url|sha|fetched_at` 派生（document_fetch.rs:809-811、:846）。
- 落盘根：`RP_ARTIFACTS_DIR`（默认 rip-data/artifacts，rp-mcp.rs 模块文档 :2-…）；EvidenceRegistrar 给 artifacts_dir 时正文落 `<dir>/<tool_run_id>/<artifact_id>.json`，缺省登记 `ripartifact://` 引用 uri（agent-engine/src/evidence.rs:7-13，"缺省登记引用 uri…如实标注不做伪装"）。
- 镜像面内容寻址：`image-ledger.json` digest 台账（pin/update-先验证再切换/rollback，image_ledger.rs:1-21）；镜像引用必须 `@sha256:<64hex>`（podman_host.rs:263-266）。

### 2.9 任务取消与超时 ——【已具备】

- 状态机：ToolRun 十三跃迁 `is_legal`（tool-engine/src/state.rs:38）+ `cancel_requestable`（:48，RUNNING/WAITING_FOR_RUNNER 可发起）；`request_cancel` 写 `cancel_requested_at`（tool-engine/src/lib.rs:326-336）。
- 容器 watchdog：`try_wait` 轮询，超时 `podman kill --signal TERM` → 2s 宽限 → `kill --signal KILL`；协作取消同链 3s 宽限（podman_host.rs:483-512、:577-593）；wait_child 有界轮询（:622-632）。
- 预算帧注入：容器 stdin 请求帧携带 `timeout_seconds/max_output_bytes`（podman_host.rs:443-459、:610-618）。
- WAITING_FOR_RUNNER：Runner 离线任务进等待不伪装失败（runtime-gateway，GatewayConfig/RecoveryReport，runtime-gateway/src/lib.rs:49-99）。
- workflow/agent 面：018 `workflow_run_control` pause/cancel 请求位 consume-on-observe（workflow-engine/src/control.rs:125-153）；014 agent_run_control 复用同语义；rp-workflow CLI cancel/resume/retry 与 MCP `rip.cancel_run`（mcp-server/src/dispatch.rs:366-377）同一 `request_cancel` 实现。
- 心跳探针自带 5s 超时守卫 kill（runtime-gateway/src/heartbeat.rs CommandProbe），探测周期默认 600s（heartbeat.rs:22）。

### 2.10 事件推送（SSE/WebSocket/轮询）——【未发现（轮询具备）】

- SSE / WebSocket / EventSource / socket.io：全仓零命中（apps/server + web/src + crates 实测）。
- 前端纯轮询：`setInterval`（web/src/routes/WorkflowsPage.tsx:140，activeState ? 2000ms : 800ms；web/src/components/Dialogs.tsx:339）。
- REST 面 `ThreadingHTTPServer` 无长连接/推送端点；run 状态查询走 `/api/runs` 等只读 GET。

### 2.11 tool-engine 的 podman 容器派发与信封协议 ——【已具备】

- **派发**：`PodmanHostAdapter::run_command` 约束清单逐条注入（podman_host.rs:354-427）：`run --rm -i --name rip-tool-<run_id> --label rip.runtime=rip-tool` + 网络（`none` | `pasta:-T,<port>`）+ `--read-only --userns=keep-id --cap-drop ALL --pids-limit --cpus --memory` + 白名单挂载（in 只读 / out 可写）+ `--tmpfs /tmp size,nosuid,nodev`。
- **镜像钉值**：`@sha256:<64hex>` 二道复核（podman_host.rs:263-266、:732 错误消息）；FamilyLimiter 并发族帽（:101-172）。
- **信封协议（D-TC-03）**：stdin 一行 JSON `{run_id, input, budget}` 写毕即关（podman_host.rs:449-459）；stdout **首行必须是 `status=result` 结果帧**（:519-549；P5-9c 实录：nuclei 命中行污染 stdout 导致帧解析失败，修复 = stdout 静默 + result.jsonl artifact 承载）；stderr 诊断、exit code 映射 EXECUTION/-32006。
- **信封结构（TOOL-CONTRACT §5）**：`ToolRunResult{protocol_version, run_id, tool_id, tool_version, status, result, summary, error, artifacts, evidence_refs, usage{elapsed_ms,output_bytes,truncated,cpu_ms,max_rss_bytes}, environment{runtime_type,runtime_node_id,image_digest,worker_protocol_version,host_fingerprint_hash}, replay}`（envelope.rs:103-131 与测试夹具）；`validate`/`to_value`（:133、:199）；截断 §5.4（:296）。
- **python_worker**：同款「stdin/stdout 各一行 JSON」帧协议（adapter/python_worker.rs:11），长驻子进程 + cancel 请求帧（:149、:173）。
- **Gateway 心跳**：podman 本机命令可用性探针（`podman info --format {{.Host.Security.Rootless}}`），ok/degraded/broken 三态映射（heartbeat.rs:1-17、:60-…）。

### 2.12 附加面（WSW 相关的既有防护）

- **SSRF 私网闸**（document_fetch）：闸序 scheme → host 解析 → 主机名内网先拒 → 端口 → **DNS 全地址私网复核**（IPv4 RFC1918/loopback/link-local + IPv6 loopback/ULA/link-local/文档段/multicast/NAT64/`::ffff:` mapped 递归复核，document_fetch.rs:460-514）→ allowlist；私网闸先于 allowlist（check_hop 注释 :553-560）。重定向逐跳复核 + `max_redirects`（FetchPolicy 结构 document_fetch.rs:63-71）。
- **平台 REST 鉴权**：Bearer Token（RP_AUTH_TOKEN/FILE），constant-time 比较（hmac.compare_digest），失败统一 401 + 节流，写端点强制（rp_server.py:21、:3150-3163）。
- **MCP 授权边界**：`RP_MCP_TOKEN` fail-closed（缺/空 exit 2 拒启，rp-mcp.rs:50-59）。

---

## 三、对 WSW Proxy / MITM 的差距清单

对照 `WEB-SECURITY-WORKBENCH-PLAN.md`：WSW-1 HTTP Message Foundation（:603）、WSW-2 Proxy/MITM Runtime = Listener + HTTP/HTTPS CONNECT + TLS CA + 拦截队列 + Forward/Drop（:604）、§5.2 六态 Listener（STOPPED/STARTING/RUNNING/DEGRADED/STOPPING/FAILED :499）、候选实现 Hudsucker/http-mitm-proxy/slinger-mitm（:632-634）、§2.3 边界（Rust-First、Python/Node 不得成为代理/HTTP 核心运行时 :444、:460）。

| # | 差距 | 现状证据 | WSW 需求 | 量级判断 |
| --- | --- | --- | --- | --- |
| G1 | **TLS 栈为零** | 无 rustls/rcgen/ring；https 显式拒绝（document_fetch.rs:167）；CONNECT 盲泵 | TLS MITM：动态 CA、按 SNI 签发叶子证书、终结/重签、证书信任链管理与 UI（:111-112） | 最大单项：新增 TLS 依赖链 + CA 生命周期 + vendor 落库 |
| G2 | **HTTP/1.1 解析器不可复用于 WSW** | 手写 GET/绝对形式/三体定界/头帽；无 keep-alive、压缩、Expect、方法全量、二进制 body 语义（egress_proxy.rs、document_fetch.rs） | WSW-1 消息模型：完整请求/响应解析序列化、保真（raw bytes 级）、Repeater/Fuzzer 全方法 | 需要正式解析库（httparse/http 类 crate）或升级手写面；现两处解析器互不共享 |
| G3 | **HTTP/2 未发现** | 无 h2；HTTP/1.1 固定 | 规划 §11 POC 项（:620、:818 边界待定） | 后置批次；候选库自带 h2 feature |
| G4 | **WebSocket 未发现** | 零 WS 依赖与代码 | WebSocket Workbench（握手/帧捕获/重放，:24） | 后置批次 |
| G5 | **Listener 面缺失** | EgressProxy 是 run 级临时代理：127.0.0.1 ephemeral、allowlist 双闸、Drop 即停、事件仅进程内存（RUN_EVENTS static Mutex，egress_proxy.rs:133-134） | 持久可配置 Listener（多监听端口、六态生命周期、落库、UI 启停） | EgressProxy 可作参考实现，但生命周期/持久化/UI 三面需新建（复用判定：需 Adapter 级重写，非直接复用） |
| G6 | **拦截队列/规则面缺失** | 现仅 accept / 403 两分支（:323-410），无 pause/modify/drop、无规则引擎 | Intercept Queue（按规则暂停、放行、丢弃、修改）+ Passive Rules | 全新面；可复用 audit-engine Emit 与 ProxyEvent 形状 |
| G7 | **流量历史持久化缺失** | ProxyEvent 只进内存台账（不落盘不入库，:133-134 注释原文）；抓取记录表（012 knowledge_document_fetch）是业务面非通用流量面 | HTTP History：Exchange 级请求/响应持久化 + 检索 + send-to-X 流转（:97 对象流转图） | 表面可循 005 artifact 惯例新增迁移（023+ 号段），不建第二套事实体系 |
| G8 | **事件推送缺失** | 无 SSE/WS，前端 setInterval 轮询（§2.10） | Proxy/拦截队列 UI 需实时性（Listener 状态、队列进出） | 引入代理运行时同批决策：SSE（同源 HTTP，std 面可实现）或随 tokio/hyper 引 WS |
| G9 | **tokio net 非显式** | tokio/net 由 tokio-postgres 默认 feature 隐式带入（§1.3） | hyper/hudsucker 需要 async 网络面 | workspace 显式声明 features + vendor + deny 门禁三步走 |
| G10 | **请求方法/体转发语义窄** | forward_http 只重写头（删 proxy-*/connection/keep-alive，:536-576），体透传但无流式观察点；https 绝对形式不支持（parse_absolute_http :441-465） | Repeater/Fuzzer 任意方法任意体 | 随 G2 一起解决 |
| G11 | **既有代理边界模型要写进设计** | 策略执行只在客户端自愿走代理时生效（egress_proxy.rs:15-18 + P5-9c nuclei 绕行实录） | WSW 显式代理场景与该模型同构（浏览器/工具主动挂代理）——设计文档须显式继承该边界，TLS MITM 后的"强制可见性"承诺不能超出 | 文档面（零代码） |
| G12 | **Python 边界** | rp_server.py 承担 REST + 静态面（stdlib） | 规划 :444/:460：Proxy Runtime（监听器、连接生命周期、TLS MITM、协议转发）归 Rust | 约束性差距：现状合规，新批次必须守 |

### 可直接复用清单（WSW 起步资产）

1. `host_from_url`（host 解析语义 + 测试，egress_proxy.rs:151-189、:641-659）——WSW Scope/目标派生同源。
2. `EgressProxy` 双向泵/403/事件形状（:105-131、:580-632）——Listener 内核参考实现与测试范式（tests/podman_host_egress.rs 真容器链路）。
3. `read_http_response` 三体定界 + 头帽 + 截断（document_fetch.rs:247-…）——消息模型起步语料。
4. envelope/artifact/evidence 链（§2.8、§2.11）——Exchange 落 Artifact/Evidence/Trace 六 ID 的既有通道，零新体系。
5. cancel/timeout 机制（§2.9）——Proxy 长连接任务的取消语义先例。
6. rp-mcp-parity 的 std TcpStream 薄 HTTP 客户端（parity.rs:221-232）——自测载具可再利用。

---

## 四、复现

```bash
cd /media/emo/code/pensting/research-platform

# 1) 网络栈 crate 覆盖面（应为零命中）+ tokio 版本
grep -E '^name = "(hyper|axum|reqwest|rustls|native-tls|tungstenite|tokio-tungstenite|h2|http|http-body|tower|tower-http|socks|hyper-rustls|hyper-tls)"$' Cargo.lock
grep -A1 '^name = "tokio"$' Cargo.lock

# 2) tokio net feature 归因
cargo tree -e features -f "{p}" -i socket2 --offline
grep -n -A4 '^runtime = ' vendor/tokio-postgres/Cargo.toml

# 3) 代理/HTTP 手写实现定位
grep -rn "TcpStream" crates/ --include="*.rs"
grep -n "no TLS stack in this batch" crates/tool-engine/src/document_fetch.rs

# 4) SSE/WS/TLS/SOCKS 零命中面
grep -rn -i "websocket\|Sec-WebSocket\|event-stream\|socks5\|rustls\|native-tls" crates/ apps/server/rp_server.py web/src/ --include="*.rs" --include="*.py" --include="*.tsx" | grep -v node_modules

# 5) 前端轮询与 Python server 形态
grep -n "setInterval" web/src/routes/WorkflowsPage.tsx web/src/components/Dialogs.tsx
sed -n '86p;3405p' apps/server/rp_server.py
```

预期回显：1) 第一条零输出、第二条 `version = "1.54.1"`；2) 树根 socket2→tokio←（mcp-server / postgres→tokio-postgres 链）与 `runtime = ["tokio/net","tokio/time"]`；3) 四处 TcpStream 文件列表 + document_fetch 拒绝消息行号；4) 零有效命中；5) 两处 setInterval 行 + `from http.server import …` 与 `ThreadingHTTPServer((HOST, PORT), RpHandler)`。
