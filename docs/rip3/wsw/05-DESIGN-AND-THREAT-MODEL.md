# WSW 设计与威胁模型（05）：消息四表示、状态机词表、数据归属草案、安全边界逐条映射

- 文档类型：WSW-1 设计评审输入（ADR-0004 的下游细化）
- 日期：2026-11-18
- 上位文档：`docs/rip3/WEB-SECURITY-WORKBENCH-PLAN.md`（下称 PLAN；§6.3 :473-495 四表示与 Exchange 字段、§6.4 :497-506 状态机、§七 :508-557 安全边界、§八 :559-594 数据归属）；ADR-0004（`docs/rip3/wsw/04-ADR.md`）；四份审计（00/01/02/03）
- 证据口径：每条裁决后附 path:line（本会话回读核实）或标注"草案待批次定稿"
- 章节说明：PLAN 中安全边界实体章节为 §七（7.1-7.5），数据归属为 §八；本文按裁决方要求的八条边界清单逐条映射（对应 PLAN §7.1-7.5 + §8.2），引用一律给 PLAN 实际行号。

---

## 一、HTTP 消息四表示与 Exchange 字段草案

### 1.1 四表示定义（PLAN §6.3 :477-482 逐条落地）

| 表示 | 内容 | 归属批次与 crate | 存储 |
|---|---|---|---|
| **Wire** | 捕获时的原始传输字节：CONNECT 建链后 TLS 明文的首个原始 HTTP 报文字节流（请求与响应各一份），保留原样头序、原始空格/制表、原始 chunked 分帧字节 | WSW-1 `crates/wsw-message`（类型）+ WSW-2 `crates/wsw-proxy`（采集） | 原始字节落 CAS 正文（§3.3），元数据表存 sha256 引用；超帽截断时 `capture_fidelity=truncated` + 原因（PLAN :482"记录明确的 Capture Fidelity 状态和原因"） |
| **Parsed** | 结构化表示：`http::Request`/`http::Response` 类型（lock 已有 http 1.5.0/http-body 1.1.0，ADR-0004 §2.1）+ 头序保持的 HeaderMap、方法、URI、status、体定界结果；供 UI 过滤/搜索/规则匹配 | WSW-1 | 数据库元数据列（exchange 表，§3.2） |
| **Display** | 展示内容：经解压（gzip/br/zlib 按需，zstd 不进第一版，ADR-0004 D1 代价注）、字符集转换、格式化（Pretty JSON/Hex/Rendered）；永不回写 | WSW-6 Decoder/Comparer 批次起建视图，WSW-1 只定义 conversion 接口位 | 不落库，即时转换 |
| **Outbound** | 实际发送到目标的序列化字节（Repeater/Fuzzer 编辑后的重序列化产物） | WSW-3 Repeater 起 | 发送前落 CAS（编辑产物可溯源），Outbound ≠ Wire 时 `edited=true` |

不变式（PLAN :482"四者不能混为一谈"的机制化）：
1. Wire 字节是唯一"原始事实"，Parsed 从 Wire 派生并记录 `parse_status`；解析失败不伪造 Parsed（`parse_status=failed` + `error_code`，字段见 1.2）。
2. Outbound 只能从用户编辑态重序列化产生，任何路径不得把 Display 直接发送（防解压后超帽体被原样上行）。
3. Display 生成过程不得改变 Wire/Parsed 存储（纯函数，输入字节→输出展示体）。

### 1.2 Exchange 字段草案（PLAN :484-493 全字段细化；归属=WSW-1 类型 `wsw_message::Exchange`，存储=§3.2 表）

| 字段 | 类型草案 | 说明 |
|---|---|---|
| exchange_id | `ExchangeId` 新类型（`wsw-` 前缀，沿 `domain::ids` 生成范式 `crates/domain/src/ids.rs:10-17`） | 主键 |
| session_id / listener_id / sequence_no | 新类型 / 新类型 / u64（listener 内单调） | PLAN :486 |
| direction / source | enum（client_to_server / server_to_client 面向消息对；source=proxy_capture / repeater / fuzzer / import） | 导入数据必须 source=import（PLAN §8.3 :594"不能伪装成真实捕获"） |
| protocol / http_version / tls_version / alpn | 小写字符串（"http/1.1"/"h2"…；tls_version/alpn 明文捕获时才有值） | PLAN :490 |
| method / scheme / authority / path | 请求面四元组（authority 含 host:port） | PLAN :487；host 解析语义复用 `host_from_url`（`crates/tool-engine/src/adapter/egress_proxy.rs:151`） |
| request_headers / response_headers | 头序保持数组（name,value,cap） | Parsed 表示的落库投影 |
| request_body_artifact / response_body_artifact | CAS 引用（sha256 + uri + size + fidelity） | §3.3；PLAN :488 |
| response_status | u16 | |
| started_at / completed_at / duration_ms / ttfb_ms | 时间面 | PLAN :489 |
| remote_ip | IpAddr（连接实际对端） | DNS rebinding 审计用（§4.3） |
| scope_decision | enum（in_scope / out_of_scope_passive / out_of_scope_blocked） | PLAN :491 + §7.2 :528"Out of Scope 记录必须可见"；PLAN History 字段表第三态 Unknown（PLAN :165"In Scope / Out of Scope / Unknown"）由 **scope_decision 为 NULL（Scope 规则未命中）** 表达，词表不设 unknown 值，UI 展示层把 NULL 渲染为 Unknown——WSW-1 S2 字段定稿按此执行 |
| edited / tags / notes | bool / 数组 / 文本 | PLAN :491 |
| trace_id / task_id / tool_run_id / artifact_id / evidence_id | 可空，复用六 ID（`crates/domain/src/ids.rs:10-17`；015 迁移 trace 面先例 `migrations/015_trace_correlation.sql:53-57`） | PLAN :492 |
| capture_fidelity | enum（full / truncated / degraded_no_wire / import） | PLAN :493 |
| parse_status / error_code | enum（ok / failed / partial） / 小写词表 | PLAN :493 |

WSW-1 批内定稿动作：以上字段冻结为 Rust 结构 + serde 类型 + 单测钉值（WSW-1-TASK.md §四）。

---

## 二、五组状态机词表（PLAN §6.4 :499-506 逐组细化）

总则（PLAN :506 原文约束）：状态迁移由 Rust 后端定义，前端只展示和发起动作；事件必须具备事件 ID、时间、Trace 关联和幂等处理规则。

### 2.1 Listener（PLAN :499 六态）

```
STOPPED → STARTING → RUNNING ⇄ DEGRADED
RUNNING → STOPPING → STOPPED
STARTING → FAILED（bind 失败/CA 初始化失败）
STOPPING → FAILED（强制超时兜底）
```

| 跃迁 | 触发 | 入口位 |
|---|---|---|
| STOPPED→STARTING | UI/API start | `crates/wsw-proxy`（WSW-2 新建，Listener 生命周期面）；loopback 校验失败即进 FAILED（守卫逻辑参照 `crates/wsw-poc/src/main.rs:195-203`） |
| STARTING→RUNNING | bind+CA ready | WSW-2；CA 面挂 WswAuthority（ADR-0004 D4） |
| RUNNING→DEGRADED | 证书缓存异常/TLS 错误率阈 | WSW-2；可诊断错误清单 PLAN §7.5 :555 |
| RUNNING→STOPPING→STOPPED | stop 请求；拦截队列未处理项必须先收口（§2.2 EXPIRED） | WSW-2；PLAN §7.5 :553 |
| 任意→FAILED | 崩溃恢复面：重启后对账持久态 | WSW-2；PLAN §7.5 :556（Fuzzer 侧恢复）同范式 |

### 2.2 Intercept Item（PLAN :500 五态）

```
QUEUED → FORWARDED | DROPPED | MODIFIED_FORWARDED | EXPIRED
```

- QUEUED：命中拦截规则暂停（规则引擎 WSW-2）。队满进 EXPIRED（PLAN §7.3 :536"最大拦截队列长度"）。
- MODIFIED_FORWARDED：编辑后转发，触发 Outbound 重序列化 + Scope 二次校验（§4.1）。
- 终态唯一：五态均不可再迁移；Listener 停止时所有 QUEUED 强制 EXPIRED（PLAN §7.5 :553）。
- 事件面复用 ProxyEvent 形状范式（accept/reject 结构化事件，`crates/tool-engine/src/adapter/egress_proxy.rs:105-131`；但落库不进 RUN_EVENTS 内存台账——那是进程内测试面 `egress_proxy.rs:133-134`，WSW 面走 SQLite 事件表 §3.2）。

### 2.3 Repeater Send（PLAN :501 六态）

```
PENDING → SENDING → SUCCEEDED | FAILED | CANCELLED | TIMED_OUT
SENDING → CANCELLED（取消请求位消费后中止）
SENDING → TIMED_OUT（单请求超时帽 §4.4）
```

- 取消语义复用 tool-engine 先例：cancel 请求位 + 协作中止（`crates/tool-engine/src/state.rs:38,48` 十三跃迁 + `cancel_requestable`；`crates/tool-engine/src/lib.rs:326-336` request_cancel）。
- SUCCEEDED 后产物（响应 Wire/Outbound）落 CAS 并回填 exchange 引用。

### 2.4 Fuzzer Job（PLAN :502 九态）

```
CREATED → QUEUED → RUNNING → PAUSED ⇄ RUNNING → CANCELLING → CANCELLED
RUNNING → COMPLETED | FAILED | BUDGET_EXHAUSTED
```

- Job 一切执行动作经 `Engine::submit` 落 tool_run（02 矩阵红线：`crates/tool-engine/src/lib.rs:158` submit、`lib.rs:253` INSERT tool_run；禁止旁路子进程直跑）。
- BUDGET_EXHAUSTED：预算（case 数/时长/字节）耗尽即终态，不得静默续跑（PLAN §7.3 :537）。
- 崩溃恢复：重启后 RUNNING→CANCELLING 对账，不重复派发已发送 Case（PLAN §7.5 :556）——case 幂等键 = job_id+case_seq。

### 2.5 Fuzzer Case（PLAN :503 七态）

```
PENDING → SENT → RESPONDED | TIMEOUT | NETWORK_ERROR
PENDING → SKIPPED_SCOPE | SKIPPED_BUDGET
```

- SKIPPED_*：跳过也是结果，必须可见（PLAN §7.2 :528 同源原则）。
- Case 状态写面走 tool_run 信封（result.jsonl artifact 承载先例，00 §2.11）。

---

## 三、数据归属与存储决策草案（PLAN §8 逐条；定稿留 WSW-1 批次迁移文件）

### 3.1 归属总表

| 数据 | 归属 | 依据 |
|---|---|---|
| Listener / Session / Exchange / Intercept 事件 / Repeater Tab / Fuzzer Job+Case 元数据 | **SQLite 运营核**，既有链 023+ 增量迁移，唯一执行器 `database::migrate`（`crates/database/src/migrate.rs:115`；链头 022 自钉 `migrations/022_coverage_matrix.sql:56`） | 02 矩阵"禁止新 .db 文件当事实源"；PLAN §8.1 :563"工作台运行数据" |
| HTTP 请求/响应原文（Wire/Outbound/超帽 Parsed 体） | **Artifact/CAS 正文**，落 `RP_ARTIFACTS_DIR` 约定目录（00 §2.8：默认 rip-data/artifacts），登记见 §3.3 | PLAN §8.1 :564"HTTP 请求响应原文属于 Artifact，不是 KnowledgeCard" |
| Passive Audit 命中 | 候选发现表（WSW-6 批次），与正式 Finding 分开建模（PLAN :504） | §8.1 :565 |
| 正式 Finding/POC/漏洞等级/攻击链 | **RTIS 唯一事实源**，经 rt.py / RTIS MCP 接口，平台面只存 evidence 引用 | PLAN §8.1 :566；02 矩阵 RTIS Adapter 行 |
| 文档/知识/向量 | intel-db + PG，WSW 不新增 | 02 矩阵 Database（PG 情报核）行 |

### 3.2 新表面（草案；迁移文件号 023+，WSW-1 起写，开发库副本验证，不碰生产 `migrations/platform*.db`——022 文件头纪律 `migrations/022_coverage_matrix.sql:17-19`）

- `wsw_listener`（六态 + 配置 JSON）
- `wsw_session`（会话/工作区范围）
- `wsw_exchange`（§1.2 字段；索引：session_id+sequence_no、host、started_at、method、response_status、trace_id）
- `wsw_intercept_event`（Intercept Item 五态迁移事件，幂等键 event_id）
- `wsw_body_blob`（CAS 去重表，见 §3.3）
- `wsw_repeater_send` / `wsw_fuzzer_job` / `wsw_fuzzer_case`（§2.3/2.4/2.5 状态面）

### 3.3 Body → Artifact/CAS 决策

**问题**：既有 `artifact` 表 `run_id TEXT NOT NULL REFERENCES tool_run(run_id)`（`migrations/005_tool_contract.sql:78-90` 实读），被动捕获的 Exchange 没有 tool_run，直接写 artifact 表破坏外键语义。

**裁决（双层）**：
1. **被动捕获面（History）**：新增 `wsw_body_blob` CAS 表（sha256 主键 + uri + size_bytes + mime + first_seen_exchange_id），正文落 `<RP_ARTIFACTS_DIR>/wsw/<sha256 前2>/<sha256>`；同 sha256 去重复用（PLAN §8.2 :576"内容寻址、哈希校验和去重"）。截断协议复用 envelope 语义（`crates/tool-engine/src/envelope.rs:281-321` TruncationOutcome：超帽降级 + 原始 sha256 + 建议引用）。
2. **主动发送面（Repeater/Fuzzer/Scanner）**：一切执行动作经 `Engine::submit` 落 tool_run，产物走既有 `insert_artifact`（`crates/tool-engine/src/lib.rs:795`）+ artifact 表 + EvidenceRegistrar（`crates/agent-engine/src/evidence.rs:7-13`）链，零新体系。

### 3.4 History 保留策略（草案，PLAN §6.3 :495"容量上限、保留策略和用户可控清理"）

- 容量帽：History 元数据行数 + 正文体积双帽（配置面 Settings，默认值 WSW-3 定稿）；超帽按 started_at LRU 淘汰，淘汰动作写审计事件。
- 正文 GC：`wsw_body_blob` 引用计数（exchange/用户收藏 pin），零引用超期清理。
- 用户清理：按 session/host/时间范围删除 + "清空全部"；删除含正文（CAS 引用归零即删文件），元数据行保留 scope_decision 级别摘要或按用户选择全删。
- 不写明文凭据进正文以外的一切面（§4.5）。

---

## 四、威胁模型与安全边界逐条映射（八条，一条不漏）

威胁模型骨架：资产=本机凭据/Cookie/CA 私钥/目标数据/平台库；信任边界=loopback 监听口（唯一入口）、上游目标（不可信内容）、UI/API 面（既有 Bearer 鉴权 `apps/server/rp_server.py:3157-3168`）；对手=恶意目标站（响应驱动攻击）、本机其他进程、误操作用户。每条边界给：威胁 → 缓解 → 实现入口位（既有代码锚点或新 crate 落点）。

### 4.1 Scope include/exclude 与重定向二次校验（PLAN §7.2 :521-524）

- 威胁：主动请求/Fuzz 越界打无授权目标；重定向把 Out-of-Scope 请求带出边界。
- 缓解：Scope 规则（Scheme/Host/Port/Path + Include/Exclude）；**每次主动发送与每个 Fuzzer Case 发送前由后端重校验**（前端校验不作数）；重定向逐跳重校验 + `max_redirects` 帽；out-of-scope 被动捕获可见（scope_decision 落 exchange 行）。
- 规则源（PLAN §7.2 首条 :521"Scope 复用 ResearchTask 的目标和现有工具执行边界"）：WSW Scope 不另起目标体系——规则初始化从 ResearchTask 目标集与既有工具执行边界导入（既有面 = egress_proxy 白名单 / Manifest `egress.ports` 端口枚举 / document_fetch allowlist 语义，00 §2.6、§2.12），导入位挂 §3.2 `wsw_listener` / `wsw_session` 配置 JSON 字段；ResearchTask 目标字段→Scope 规则的映射口径**复用面 WSW-3 定稿**，过渡期（WSW-1/WSW-2）未配置规则的 Listener/会话一律 **default deny**（fail-closed）。
- 入口位：规则求值新函数 `wsw_message::scope::check`（WSW-1 定义，语义同构 `crates/tool-engine/src/document_fetch.rs:558,563` `check_hop`/`check_hop_with` 闸序范式：scheme→host→port→allowlist）；重定向逐跳复核范式与 `max_redirects` 帽直接参照 `crates/tool-engine/src/document_fetch.rs:63-71`（FetchPolicy.max_redirects）与 `:757,775`（MIN(cap) 与 hop 计数）；发送面执行 WSW-3，Fuzzer case 侧 = §2.5 SKIPPED_SCOPE。

### 4.2 DNS rebinding（PLAN §7.2 :525）

- 威胁：目标域名两次解析返回不同地址，第二跳把请求/探测带进私网。
- 缓解：DNS 解析结果变化即重检目标地址；**连接建立以实际解析 IP 为准记录 remote_ip，并把该 IP 过私网闸**（不只看域名字符串）；Pinned-IP 连接（解析一次、按 IP 连接、Host 头保持域名）。
- 入口位：既有私网闸全地址复核实现 `crates/tool-engine/src/document_fetch.rs:460-514`（DNS 全地址 IPv4/IPv6 逐址复核 + `::ffff:` mapped 递归复核 `:486-499`）直接移植为 `wsw_message::net::audit_resolved`；exchange 行 remote_ip 列（§1.2）作为事后审计锚。

### 4.3 私网/回环/链路本地/云元数据策略（PLAN §7.2 :526）

- 威胁：SSRF 打 169.254.169.254 等云元数据、127.0.0.1 服务、RFC1918 内网。
- 缓解：默认策略表（deny 私网/回环/link-local/multicast/NAT64/mapped + 云元数据 IP 段显式列 deny-never-allow）；仅用户显式任务策略可放行且逐条留痕；代理自身监听面与被测目标面分离（对 127.0.0.1 目标的 MITM 由用户显式配置）。
- 入口位：词表与判定函数现成——`private_reason`（`crates/tool-engine/src/document_fetch.rs:433` 起，含 IPv6 ULA/link-local/NAT64/文档段/multicast/mapped 全分支 `:455-514`）；WSW-1 引用 + 追加云元数据段常量（169.254.169.254、metadata.google.internal 等）为 `metadata_endpoint` 词表项；默认闸序"私网闸先于 allowlist"沿用（`document_fetch.rs:553-560` 注释原语义）。

### 4.4 速率/并发/预算/超时/取消（PLAN §7.3 :531-538）

- 威胁：代理或 Fuzzer 打挂目标或拖死本机；取消后请求继续飞。
- 缓解与上限（全部 Rust 后端强制，PLAN :539）：全局与单任务并发帽、每秒速率与最小间隔、单请求连接/读取/总超时、请求/响应体与 Header 数量/长度帽、拦截队列与 History 容量帽、Fuzzer case 数/预算/时长帽；取消必须尽快停止后续请求。
- 入口位：帽值进 Listener/Fuzzer 配置 JSON（§3.2 wsw_listener 表）；执行侧并发/取消复用 tool-engine 状态机与 watchdog 范式（`crates/tool-engine/src/state.rs:38,48` is_legal/cancel_requestable、`crates/tool-engine/src/lib.rs:326-336` request_cancel 实读、`crates/tool-engine/src/adapter/podman_host.rs:483-512` try_wait 有界轮询 watchdog）；预算语义复用信封 `budget{timeout_seconds,max_output_bytes}` 注入位（`crates/tool-engine/src/adapter/podman_host.rs:615-616` 实读；00 审计 §2.9 记 :443-459 帧注入位）；Fuzzer Job 编排走 workflow-engine checkpoint 而非自制调度（02 矩阵 Workflow Engine 行）。

### 4.5 敏感数据脱敏（PLAN §7.4 :542-548）

- 威胁：凭据经日志/导出/截图泄漏；CA 私钥进 Git。
- 缓解：展示层脱敏规则（Cookie/Authorization/API Key/Token/密码/PII 名单 + 自定义规则）；原始证据与脱敏展示分离，原始数据访问走既有权限边界；导出 HAR/JSON 支持脱敏预览；凭据不进日志/错误信息/截图/导出元数据；CA 私钥不落 Git（PEM 只落运行时数据目录，gitignore 面）。
- 入口位：Display 转换层（§1.1）为唯一脱敏挂点——WSW-1 定义 `wsw_message::redact` 接口（规则输入→Display 输出），WSW-6 Decoder 批次落 UI；导出面 WSW-3+ 走 PLAN §8.3 :585-592 四格式 + 预览；CA 私钥目录纪律进 WSW-2（对齐 00 §2.8 artifacts_dir 模式与"敏感文件权限 600"纪律）；错误链打印范式（wsw-poc/src/main.rs:110-127）在 WSW-2 收敛为结构化字段，禁止整头/整体进日志。

### 4.6 CA 与系统代理红线（PLAN §7.1 :516-517）

- 威胁：隐式改系统代理劫持用户全部流量；静默装/信任根 CA 把本机置于无感 MITM 下。
- 缓解：**不自动修改系统代理设置**；**不静默安装或信任根 CA**——CA PEM 导出 + 用户手动信任（curl --cacert / 浏览器导入由用户动作完成，POC 实拍即此模式：main.rs:217 写 PEM 落盘、:219 日志提示路径）；证书信任状态在 Settings 显式展示（03 §6 页面 11"证书状态面"缺件项）。
- 入口位：红线清单进 WSW-2 Listener 启动代码（禁止任何 `networksetup`/gsettings/注册表写面调用）；CA 生命周期函数位 = WswAuthority（ADR-0004 D4）+ 私钥读写位；UI 面挂 Settings（03 §6 页面 11）。

### 4.7 loopback 监听（PLAN §7.1 :512-515）

- 威胁：代理开放到局域网/公网成为匿名开放代理或本机滥用点。
- 缓解：默认只听 loopback，非 loopback 绑定直接拒绝（已实证：`crates/wsw-poc/src/main.rs:195-203` 拒绝逻辑 + exit 2）；禁止匿名开放代理（拒绝所有未经本机客户端标识的请求是默认姿态）；可选本地访问令牌/会话绑定（PLAN :514）；Listener 停止即关连接释放端口（:515）。
- 入口位：守卫代码范式 = wsw-poc main.rs:195-203（WSW-2 搬入 `wsw-proxy::listener::bind_guard` 并加监听后二次校验 accept 源地址仍为 loopback）；令牌面挂 rp_server 既有 Bearer 鉴权模式（`apps/server/rp_server.py:3157-3168` constant-time 比较 + 失败统一 401）。

### 4.8 Artifact 保留（PLAN §7.4 :547 + §8.2 :575-580）

- 威胁：Body 正文无限累积；已删除会话正文残留；artifact 引用悬空。
- 缓解：§3.4 双帽 + GC + 用户清理；artifact 访问权限/保留期/删除策略按 §3.3 双层归属执行（被动面 wsw_body_blob GC，主动面 artifact 表既有 run 生命周期）；数据库只存引用不复制大块二进制（PLAN :577）。
- 入口位：保留策略配置面 = Settings（03 §6 页面 11 设置端点缺口）；GC 执行位 = WSW-3 History 批次新增 `wsw_message::store::retention`；既有信封截断协议（`envelope.rs:281-321`）为"超帽降级 + 原始字节 sha256"唯一口径，不另造截断语义。

---

## 五、边界继承声明（G11）

既有代理边界模型必须写进 WSW 设计与实现文档：策略执行只在**客户端自愿走代理**时生效（egress_proxy 模块文档 `crates/tool-engine/src/adapter/egress_proxy.rs:15-18`；P5-9c nuclei 绕行实录 00 §2.6）。WSW 显式代理场景（浏览器/工具主动挂代理）与该边界同构：**TLS MITM 后的"强制可见性"承诺不得超出"自愿经过代理的流量"范围**；透明代理按 PLAN :176/WSW-PLAN §十 明确不做第一版（hudsucker issue #176 未实现项与本边界一致）。

## 六、复现命令（本会话实跑核对）

```bash
cd /media/emo/code/pensting/research-platform
sed -n '78,90p' migrations/005_tool_contract.sql            # artifact 表 run_id NOT NULL REFERENCES tool_run
sed -n '105,135p' crates/tool-engine/src/adapter/egress_proxy.rs   # ProxyEvent 三变体
sed -n '433p;558p;563p' crates/tool-engine/src/document_fetch.rs   # private_reason / check_hop / check_hop_with
sed -n '281,300p' crates/tool-engine/src/envelope.rs        # TruncationOutcome 截断协议
sed -n '499,506p' docs/rip3/WEB-SECURITY-WORKBENCH-PLAN.md  # 五组状态机词表原文
sed -n '510,549p' docs/rip3/WEB-SECURITY-WORKBENCH-PLAN.md  # §7.1-7.4 边界原文
sed -n '195,203p' crates/wsw-poc/src/main.rs                # loopback 拒绝守卫
```
