# WSW-2 批次裁决 · Proxy / MITM Runtime

- 文档类型：WSW-2 批次 ADR（承接 ADR-0004 `docs/rip3/wsw/04-ADR.md` D2/D4/D5 与 DESIGN `docs/rip3/wsw/05-DESIGN-AND-THREAT-MODEL.md` §2/§3/§4）
- 日期：2026-11-19
- 状态：裁决定稿（WSW-2 设计裁决员产出；实施以 `WSW-2-TASK.md` 为准）
- 证据口径：引用 path:line 均为本会话回读核实；每条裁决后附实现入口位。
- 硬验收口径声明：原全程手册 `/media/emo/code/pensting/WSW-CONTINUATION-WORKFLOW.md` 已丢失（固化结论 `docs/rip3/wsw/WSW-CONTINUATION-REDLINES.md:3-5`：磁盘/find/git 历史/rollout 四路零命中），其"§4 W2 硬验收行"按 `REDLINES.md` 第 5 条口径从仓库内可考来源重建——承载源 = `.zcode/workflow-drafts/WSW-2-主体批次.dwf.ts:1`（稿首自注"手册 §4 W2 行"）+ `:77、:163-171`（实拍七项）+ `:114-135`（定向门禁五项）+ PLAN §九 :604 W2 行 + ADR-0004 D4/D5。外部口径与本档冲突时，以 PLAN / ADR-0004 / 本档为准。

---

## 一、裁决总览（W2-D1 ～ W2-D10）

| # | 裁决 | 一句话 |
|---|---|---|
| D1 | 运行时落位：**不新建 crate，`crates/wsw-runtime` 原地扩展为五模块**（根原语不动 + listener/authority/scope/persist），hudsucker 0.25.0 组装在此；零新外部包（lock 400 不变），tokio 仅增补 `net` feature | 任务面收敛进一个 crate，门禁面（`-p wsw-runtime`）与 DI-1 既有 lock 400 门零摩擦 |
| D2 | **WswAuthority 自实现 `CertificateAuthority` trait**：host 为 IpAddr 时签 `SanType::IpAddress`（v4/v6，`::ffff:` mapped 归一 v4），否则 `SanType::DnsName`；moka 缓存 ServerConfig（cap=1000），条目 TTL=证书有效期半程；CA 持久化复用 | ADR-0004 D4 的实施裁决：vendor `rcgen_authority.rs:85-86` 只发 DNS SAN 不可复制转正，自实现补齐 |
| D3 | CA 存储位 = `rip-data/wsw-ca/`（私钥+证书 PEM）：文件 0600 / 目录 0700 原子创建；不入 Git、不入日志、不进普通 Artifact；指纹展示 = SHA-256(证书 DER)；重生成/导出/信任全为用户显式动作，禁静默安装、禁默认改系统代理 | PLAN §7.1 :516-517 红线的存储面落点；`rip-data/` 已被 `.gitignore:47` 覆盖 |
| D4 | **024 迁移纯新增三表** `wsw_listener` / `wsw_session` / `wsw_intercept`（拦截队列为行式状态机：五态列 + TTL 列 + 终态列，不建独立事件流水表）；链头 023→024，`schema_version` INSERT OR IGNORE + `PRAGMA user_version=24` 双写；ALTER 不入本批；`wsw_exchange` 元数据表留 WSW-3 | DESIGN §3.2 表归属 WSW-2 项收口；拦截"行即事实"，事件流水属 WSW-3 History 落库批再裁 |
| D5 | **Scope 闸闸序 = 后端强制双查**：上游连接前（CONNECT host / 明文 authority）与重定向逐跳后（Location 目标）都过 `scope::check`；闸序 scheme→host(include/exclude)→port→path→解析 IP 审计（`private_reason` 全分支 + 云元数据段）；无规则 default deny；私网/回环目标仅显式 allow_private + include 规则可过且留痕，云元数据段 deny-never-allow 任何策略不可放行 | PLAN §7.2 :521-528 逐条机制化；闸序范式沿 `crates/tool-engine/src/document_fetch.rs:553-563` |
| D6 | 拦截队列 = 内存 `PendingQueue`（既有原语）+ persist 双面；TTL 到期/队满/Listener 停机均强制 EXPIRED；重启对账（QUEUED 未过期回载、过期标 EXPIRED）；动作集 Forward/Drop/Modified-Forward（后者触发 Outbound 重序列化 + Scope 二次校验）；默认关拦截 | 既有 `wsw-runtime` PendingQueue 五态语义直接消费；PLAN §7.5 :553 停机终态红线 |
| D7 | **错误五态映射**：`scope_blocked`（403）/ `upstream_connect`（502，含超时）/ `upstream_tls`（含证书固定检出）/ `downstream_tls`（CA 签发失败/缓存异常/客户端握手失败）/ `parse_framing`（解析/分帧/帽值超限）；结构化字段落日志与行面，禁整头/整体进日志；POC 错误链全打印收敛为 `source()` 链式字段输出 | PLAN §7.5 :555 可诊断错误 + DESIGN §4.5 凭据不进日志；POC `wsw-poc/src/main.rs:110-127` 范式收敛 |
| D8 | **UI 最小面 = 两页**：「代理状态」（Listener 六态/地址端口/连接数/排队数/捕获数/Intercept 开关）+「证书状态」（指纹/有效期/导出 PEM 显式按钮/重生成显式按钮/信任安装说明）；History/拦截编辑器/Repeater/Fuzzer UI 全留 W3+；API 走 rp_server 既有 Bearer 鉴权 | PLAN §九 :605 W3 行归属 + §4.1 :119"用户必须主动执行信任安装"；本批验收焦点在运行时与安全边界 |
| D9 | WSW-2 捕获事实面 = Wire→CAS + `wsw_body_blob`（023 已备）+ 024 拦截行 + 结构化运行日志；`wsw_exchange` 表与 audit-engine Emit 接线留 WSW-3 History 落库批（audit-engine `AuditEvent` 是插件生命周期词表，`crates/audit-engine/src/lib.rs:4,76`，混用即污染）；脱敏挂 `wsw_message::redact` 只在展示/日志面 | DESIGN §3.2 "wsw_exchange WSW-2/3 拆批" 明文授权；不建第二套事件体系 |
| D10 | W0 遗留两项验收：上游**纯 h2-only** 本地夹具实测（禁外联）+ **容器内 aws-lc-sys 构建**（podman build 主 `Containerfile` builder 层；失败回退 = builder 层增补 cmake 如实记录）；性能以"单请求代理延迟增量"实测面登记，PLAN §11.6 其余项无基线如实写缺测 | ADR-0004 差距 (b)/(f) 与 D5；PLAN :754"数字必须来自真实命令，不能预填" |

---

## 二、逐项裁决

### W2-D1 运行时落位与模块边界

**决定**：`crates/wsw-runtime` 原地扩展，不新建 `wsw-proxy` crate。模块边界：

```
crates/wsw-runtime/src/
  lib.rs          # 既有根原语原样保留：ListenerConfig / ListenerState /
                  #   InterceptState / PendingQueue / is_loopback_peer
                  #   （src/lib.rs:6-32/45-77/84-107/149-250/41）；更新 crate 级
                  #   doc（现"no socket IO"声明随本批作废，改为模块化声明）
  listener.rs     # socket 运行时：bind 守卫（loopback 拒绑 + accept 源二次校验）
                  #   + hudsucker Proxy 组装 + 生命周期驱动 ListenerState 六态
                  #   + listener::handler 子模块（HttpHandler/WebSocketHandler
                  #   实现：流式读 + 帽值 + 拦截队列 + scope 调用 + 错误五态）
  authority.rs    # WswAuthority（D2/D3）
  scope.rs        # ScopeRules + check 闸序 + audit_resolved（D5）
  persist.rs      # 024 三表 store 接口（enqueue/resolve/重启对账）（D4/D6）
```

**依赖方向**：`scope ← handler ← listener`；`authority ← listener`；`persist ← handler`；全部可消费 `wsw-message`（消息四表示/CAS/脱敏公开面 `crates/wsw-message/src/lib.rs:40-48`）。`wsw-poc` 不转正、不删除、不重构（ADR-0004 D2），CA 生成/loopback 守卫/connector 装配/错误链四段只读参照搬运：`crates/wsw-poc/src/main.rs:206-224（CA）/195-203（守卫）/229-240（connector）/110-127（错误链）`。

**依赖面（全部已在 lock/vendor，等号钉既有版本，registry package 零增量）**：

| 依赖 | lock 实值（本会话 grep 实测） | 用途 |
|---|---|---|
| hudsucker | 0.25.0（`default-features=false, features=["rcgen-ca","rustls-client","http2"]`，同 POC `crates/wsw-poc/Cargo.toml:14-18`） | Proxy 组装 / handler trait |
| rcgen | 0.14.10（经 `hudsucker::rcgen` 再导出，POC 先例 `main.rs:21-24`） | CA 与动态证书 |
| moka | 0.12.16 | 证书缓存（直接依赖声明，传递依赖转正口径同 WSW-1 sha2/base64 先例 `Cargo.toml:81-86`） |
| time | 0.3.55 | 证书 not_before/not_after（vendor `rcgen_authority.rs:24` 同款） |
| rand | 0.9.5（lock 双版本 0.9.5/0.10.4，取 hudsucker 0.25.0 所用 API 面 `RngExt` = 0.9） | 证书序列号（或以 std hasher 派生则不声明，实施时二选一） |
| hyper-rustls | 0.27.10 | 上游 connector（POC 先例 `crates/wsw-poc/Cargo.toml:30-35`） |
| webpki-roots | 1.0.12（POC 用 1.0 req 面命中 1.0.12） | 上游公共根 |
| http-body-util | 0.1.5 | 流式 body 处理 |
| database / domain / wsw-message / serde / serde_json | workspace | persist（rusqlite 经 database 重导出）/ ID / 消息面 |

tokio：workspace features（`Cargo.toml:60`：rt/rt-multi-thread/macros/io-std/io-util/sync/time，无 net）不动；`crates/wsw-runtime/Cargo.toml` 内 `tokio = { workspace = true, features = ["net"] }` **additive 增补**，先例 = POC `crates/wsw-poc/Cargo.toml:37-38`（"MITM 监听必需，additive 增补"原注释）。Cargo.lock 仅 wsw-runtime 包的依赖清单行变化，`[[package]]` 总数 400 不变（本会话 `grep -cE '^\[\[package\]\]' Cargo.lock` → 400）。

**否决备选**：新建 `wsw-proxy` crate（DESIGN §2.1 入口位曾写此名）——否决理由：工作流门禁/收口面已按 `-p wsw-runtime` 固化（`.zcode/workflow-drafts/WSW-2-主体批次.dwf.ts:116-119`），且 DESIGN §3.2 明文"不得为了提前创建空表/空 crate 把后续批次面塞进 023/新 crate"；单 crate 内模块边界足以承载，模块级边界由本档 D1 固化替代 crate 级。

### W2-D2 WswAuthority：证书缓存与 IP SAN 签发策略

**决定**：`WswAuthority` 实现 `hudsucker::certificate_authority::CertificateAuthority`（唯一方法 `async fn gen_server_config(&self, authority: &Authority) -> Arc<ServerConfig>`，vendor trait 面 `vendor/hudsucker/src/certificate_authority/mod.rs`）。

1. **SAN 策略**（D4 落地）：`authority.host()` 先按 IpAddr 解析——
   - `Ipv4Addr` → `SanType::IpAddress(v4)`；`Ipv6Addr` → `SanType::IpAddress(v6)`；`::ffff:a.b.c.d` mapped 先归一为 V4 再签（复用 `crates/tool-engine/src/document_fetch.rs:486-499` mapped 复核语义）；
   - 解析失败 → `SanType::DnsName`（Ia5String，vendor 先例 `rcgen_authority.rs:85-86` 同形）；
   - CN 恒为 host 字符串；KeyUsage=DigitalSignature、EKU=ServerAuth、`use_authority_key_identifier_extension=true`（vendor `:87-92` 同形，不复制实现，按 trait 自写）。
2. **签发链**：根 CA 持久化复用（D3），leaf 每请求域即时签发；签发参数对齐 vendor 先例：serial=u64 随机、`not_before=now-60s`、有效期 `TTL_SECS=365d`（vendor `mod.rs:15-17` 常量口径）。
3. **缓存**：`moka::future::Cache<Authority, Arc<ServerConfig>>`，cap=1000（POC `main.rs:224` 同值）、条目 TTL = `TTL_SECS/2`（vendor `CACHE_TTL` 口径 `mod.rs:16`）；ALPN 广告 `h2 + http/1.1`（vendor `rcgen_authority.rs:117-121` 同形，hudsucker http2 feature 面由 POC 已实证）。
4. **拒绝面**：host 非法（空/超长/控制字符）→ 签发错误入 `downstream_tls` 五态（D7），不静默回落 localhost 名（POC localhost 绕过不构成生产答案，ADR-0004 差距 (a) 原文）。

**测试面**：IP SAN 单测（127.0.0.1/[::1]/v6 纯/mapped 四例断言 SAN 类型）、DNS SAN 单测、缓存命中单测（同 Authority 二次取同 Arc）、CA 重启复用单测。

### W2-D3 CA 存储位与管理面

**决定**：

| 面 | 裁决 |
|---|---|
| 存储位 | `<数据根>/rip-data/wsw-ca/`（默认相对仓库根；env `RP_WSW_CA_DIR` 可覆盖）。`rip-data/` 全目录已被 `.gitignore:47` 覆盖，先例注释"rip-data 为运行时数据不入库"（`.gitignore:44-47`） |
| 文件 | `ca.key.pem`（PKCS8）+ `ca.cert.pem`：私钥文件 0600、证书 0644、目录 0700；创建走 `OpenOptions::new().mode(0600).create_new(true)` 原子 no-clobber（DESIGN §3.3 CAS 0600/0700 同纪律） |
| 不入面 | 不入 Git（gitignore + `git status` 验收断言）、不入日志（日志只落指纹前 16 hex）、不进普通 Artifact 表（私钥不是证据，PLAN §4.1 :121 原文"不得写入 Git、日志、截图或普通 Artifact"） |
| 指纹 | 展示 = SHA-256(证书 DER) 小写 hex（005 迁移 sha256 口径 `migrations/005_tool_contract.sql:85` 同形）+ 有效期 + CN |
| 生命周期 | 启动：目录内已有 CA → 加载复用（POC 每次重生成不构成生产答案——重启会作废浏览器已导入信任）；无 CA → 生成并落盘。重生成/导出 PEM/信任库安装：**全部用户显式动作**（API 带 confirm 语义 + UI 按钮），禁静默安装、禁默认改系统代理（PLAN §7.1 :516-517；DESIGN §4.6"禁止任何 networksetup/gsettings/注册表写面调用"） |
| 会话绑定 | Listener 令牌/会话绑定面（PLAN :514）本批落**后端 accept 源 loopback 二次校验 + rp_server API 面 Bearer 鉴权**（`apps/server/rp_server.py:3157-3168` 先例），代理口本机匿名访问令牌留 WSW-3（如实登记，不冒充已做） |

### W2-D4 024 迁移与持久化形状

**决定**：`migrations/024_wsw_intercept.sql`，链头 023→024，纯新增零 ALTER（若批内发现确需 ALTER 既有表，另立 025+，禁改历史迁移，023 文件头纪律同源）。

```sql
-- 三表形状（列细节批内定稿，CHECK 词表逐列）：
wsw_listener(
  listener_id TEXT PK,              -- wsw-lis- 前缀（wsw-message ids.rs:130-135）
  name TEXT NOT NULL,
  bind TEXT NOT NULL,               -- 必为 loopback，后端校验双保险
  state TEXT NOT NULL CHECK(state IN ('stopped','starting','running','degraded','stopping','failed')),
  config_json TEXT NOT NULL,        -- ListenerConfig JSON + scope 规则引用
  created_at / updated_at TEXT
)
wsw_session(
  session_id TEXT PK,               -- wsw-ses- 前缀
  title TEXT NOT NULL,
  scope_rules_json TEXT NOT NULL,   -- include/exclude/allow_private/policy（D5 规则源）
  out_of_scope_policy TEXT NOT NULL CHECK(out_of_scope_policy IN ('blocked','passive_forward')),
  created_at / updated_at TEXT
)
wsw_intercept(
  intercept_id TEXT PK,             -- wsw-itc- 前缀（批内定稿，沿 ids.rs 范式）
  event_id TEXT NOT NULL UNIQUE,    -- 幂等键（DESIGN §3.2 wsw_intercept_event 幂等语义并入行）
  listener_id / session_id TEXT NOT NULL,
  exchange_id TEXT,                 -- wsw- 前缀引用（元数据表留 WSW-3，D9）
  direction TEXT NOT NULL CHECK(direction IN ('request','response')),
  state TEXT NOT NULL CHECK(state IN ('queued','forwarded','dropped','modified_forwarded','expired')),
  host / method / path TEXT,        -- 队列列表过滤最小投影
  queued_at TEXT NOT NULL,
  expires_at TEXT NOT NULL,         -- TTL（ListenerConfig.queue_ttl_seconds 1..3600）
  resolved_at TEXT,                 -- 终态时刻
  reason TEXT                       -- 拒绝/过期/丢弃原因（错误五态 class 可入）
)
```

- **行式状态机裁决**：`wsw_intercept_event`（DESIGN §3.2 草案名）不单建——拦截 Item 的持久事实就是队列行本身，五态迁移落在行的 `state` 列 + `resolved_at`；终态唯一性由既有 `InterceptState::transition`（`src/lib.rs:92-106`）+ persist 层 SQL `WHERE state='queued'` 双面强制。事件流水（谁在何时改的）属审计增强，WSW-3 History 落库批按需另立迁移，本批不预建。
- **schema_version 登记**：INSERT OR IGNORE (24, …) + `PRAGMA user_version = 24` 双写（023 `:52-58` 同款）。验收水位：开发库副本 user_version=24（工作流门禁 `.zcode/workflow-drafts/WSW-2-主体批次.dwf.ts:124-126`）；生产 `platform*.db` 零触碰（023 文件头 `:30` 纪律）。
- **对账语义**（D6 一起落）：启动时 SELECT queued 行 → `expires_at` 已过 → UPDATE 为 expired；未过 → 回载内存 `PendingQueue`（`expires_at` 按 `queued_at+ttl` 重算）。停机时全部 queued 强制 expired（PLAN §7.5 :553）。

### W2-D5 Scope 闸闸序（连接前 / 重定向后双查）

**决定**：闸只在 Rust 后端（`scope::check`），前端校验一律不作数（PLAN :523 原文）。两处强制查点 + 一处 IP 审计：

| 查点 | 时机 | 拒绝动作 |
|---|---|---|
| 查点 A：连接前 | CONNECT host / 明文请求 authority，在**建立上游连接之前** | out_of_scope_policy=blocked（默认）→ 403 + `scope_decision=out_of_scope_blocked` 行记录；passive_forward → 放行不拦截改写 + `out_of_scope_passive` 记录 |
| 查点 B：重定向后 | 每跳 Location 目标（协议+host+port+path 逐跳），`max_redirects` 帽（`document_fetch.rs:63-71` FetchPolicy 先例） | 同查点 A；越界重定向必须可见（PLAN :524/:528） |
| 查点 C：解析 IP 审计 | 上游 DNS 解析完成、TCP 连接前，实际 IP 过 `audit_resolved`（`document_fetch.rs:460-514` 全地址复核移植）；Pinned-IP 连接（解析一次按 IP 连接、Host 头保持域名） | 私网/回环/链路本地/multicast/mapped/NAT64 命中 → 拒 + remote_ip 落行（DNS rebinding：域名串过闸≠IP 过闸，PLAN :525） |

**闸序**：scheme → host（include/exclude，exclude 优先）→ port → path → IP 审计；`document_fetch.rs:553-563` "私网闸先于 allowlist"语义沿用的前提修正为：**用户显式 `allow_private_targets=true` 且该目标命中 include 规则时放行私网/回环/链路本地并逐条留痕（本机靶场 127.0.0.1:8443 靠此面通过）**；**云元数据段（169.254.169.254/::ffff:169.254.169.254、metadata.google.internal、169.254.170.2 等）deny-never-allow，任何 allow_private/include 组合不可放行**（DESIGN §4.3 词表项原文）。规则源：`wsw_session.scope_rules_json`（PLAN :521"复用 ResearchTask 目标与工具执行边界"的导入位，过渡期未配置 = default deny，DESIGN §4.1 原文）。

**否决备选**：前端闸（PLAN :523 明禁）；透明拦截全部 out-of-scope 流量（违背 PLAN :527"主动面不自动扩大范围"）。

### W2-D6 拦截队列语义

**决定**：Intercept 默认关闭（PLAN :133）；开启后命中请求入内存 `PendingQueue`（`src/lib.rs:149-250` 原语，五态/TTL/容量全已单测）+ 同步落 024 行。动作集三件：Forward（原样）、Drop（终态 dropped）、Modified-Forward（编辑态经 `wsw_message::serialize` 重序列化为 Outbound + **Scope 二次校验**（查点 A 重跑）+ 终态 modified_forwarded；DESIGN §2.2 原文）。Send to Repeater 属 WSW-3。队满（capacity）→ 直接 expired + 可见记录（PLAN :536"最大拦截队列长度"）；TTL 到期 expired；停机全量 expired。命中规则匹配器本批最小实现 = host/method/path 前缀匹配（结构化规则面 WSW-3 扩全），拦截规则命中标记落行 reason。

### W2-D7 错误五态映射

**决定**：`ProxyErrorClass` 五值，全链路（handler 错误/签发失败/闸拒绝/解析失败）收敛到该词表：

| class | 触发面 | 对客户端响应 | 记录面 |
|---|---|---|---|
| `scope_blocked` | 查点 A/B/C 拒绝 | 403 | 行 scope_decision/reason + 日志 |
| `upstream_connect` | TCP 连接失败/超时/DNS 失败 | 502 | 错误五态 + 日志（cause 摘要） |
| `upstream_tls` | 上游握手失败/**证书固定检出**（PLAN :123"清晰状态不是假成功"） | 502 | 错误五态 + reason=certificate_pinned 等 |
| `downstream_tls` | CA 签发失败/证书缓存异常/下游握手失败 | 握手层失败（连接关） | RUNNING→DEGRADED 触发评估（DESIGN §2.1）+ 日志 |
| `parse_framing` | 解析失败/头帽/体帽/chunked 非法 | 400/502 按 cause | parse_status=failed + error_code（wsw-message 词表） |

日志字段 = `{class, error_code, cause_chain_summary}`；`cause_chain_summary` 由 `Error::source()` 链式拼接错误串（POC `main.rs:110-127` 范式收敛），**禁整头/整体 body 进日志**（DESIGN §4.5）。Listener 侧连续 downstream_tls/错误率 → RUNNING→DEGRADED（既有 ListenerState::transition `src/lib.rs:61` 合法跃迁）。

### W2-D8 UI 最小面清单

**决定**：两页，全在既有 Web 壳层内（PLAN §5.1 :337"不另建 SPA"），数据源走 rp_server 新增 WSW 端点（Bearer 鉴权同既有面）：

1. **代理状态页**：Listener 六态徽标、bind 地址端口、Start/Stop 按钮、Intercept On/Off、当前连接数/排队数/累计捕获数、最近错误摘要（五态计数）。对应 PLAN §5.2 :357-364 顶部状态栏子集。
2. **证书状态页**：CA 指纹、有效期、CN、PEM 导出按钮（显式）、重新生成按钮（显式 + confirm）、浏览器信任安装说明文案（各平台手动步骤，PLAN :122）、信任状态展示位。对应 03 审计 §6 页面 11 缺件项。

**不做（明文）**：History 列表/详情、拦截队列编辑器、Repeater、Fuzzer、Decoder/Comparer、Passive Audit——全留 W3+（PLAN §九 :605-608）。理由：WSW-2 验收焦点 = 运行时正确性 + 安全边界负例矩阵；UI 多做即提前消费 W3 设计面且无数据面支撑（`wsw_exchange` 表 D9 留 W3）。

### W2-D9 捕获事实面归属（WSW-2 份内）

**决定**：WSW-2 捕获事实 = ①Wire 字节 → `wsw_message::cas::store_body`（CAS 文件 + 023 `wsw_body_blob` 行）；②拦截请求 → 024 `wsw_intercept` 行（含 exchange_id 引用）；③结构化运行日志（五态 + 捕获计数 + 请求行摘要，脱敏经 `wsw_message::redact`，`crates/wsw-message/src/redact.rs:55` 只在 Display/日志投影）。`wsw_exchange` 元数据表留 WSW-3（DESIGN §3.2 "WSW-2/3 拆批"授权）；浏览器实拍"捕获证据"以日志面 + `wsw_body_blob` 行 + 截图承载，批次报告如实写此边界。audit-engine Emit 不接线（其 `AuditEvent` 为插件生命周期词表 `crates/audit-engine/src/lib.rs:4,21`，混用即第二体系污染），WSW 事件出口 WSW-3 裁决。

### W2-D10 W0 遗留验收与性能基线口径

**决定**：

1. **上游纯 h2-only 目标实测**（ADR-0004 差距 (b)）：本地 h2-only 服务夹具（hyper server `http2_only` 或等价 tokio 面，绑 127.0.0.1 随机高位端口），POC connector 已 `enable_http2`（`main.rs:239`）——本批经 wsw-runtime 代理请求该夹具，断言响应成功且 exchange/日志记录 alpn=h2。禁外联。做不到如实记录差距与原因（例如 hudsucker 0.25.0 上游 h2 路径缺陷），不冒充 PASS。
2. **容器内 aws-lc-sys 构建**（差距 (f)）：`podman build` 主 `Containerfile`（builder 层现为 gcc+libc6-dev 无 cmake，`Containerfile:23,31` 本会话实读）；失败回退 = builder 层 apt 增补 cmake 并如实记录（rip-pg17 镜像先例，ADR-0004 差距 (f) 原文）。宿主机零系统级安装（REDLINES 红线 8）。
3. **性能基线**：本批登记"单请求代理延迟增量"实测面（直连 vs 经代理同请求中位数差，repeat≥3）；PLAN §11.6 :744-752 其余项（吞吐/并发/内存/History/Repeater/Fuzzer）中属于后续批次工具面的如实写"缺测"，属于本批可达的（并发连接数/大 Body 内存峰值）随批尽量实测——**全部数字只来自真实命令回显，禁预填**（PLAN :754）。

---

## 三、复现命令（本会话实跑核实）

```bash
cd /media/emo/code/pensting/research-platform
grep -cE '^\[\[package\]\]' Cargo.lock                        # → 400（零新外部包基线）
sed -n '85,86p' vendor/hudsucker/src/certificate_authority/rcgen_authority.rs   # → 仅 SanType::DnsName（D2 依据）
grep -n 'TTL_SECS\|CACHE_TTL' vendor/hudsucker/src/certificate_authority/mod.rs # → 365d / 半程（D2 缓存口径）
sed -n '37,38p' crates/wsw-poc/Cargo.toml                     # → tokio net additive 先例（D1）
grep -n 'tokio = ' Cargo.toml | head -1                       # → workspace features 无 net（D1 增补面）
grep -n 'pub fn transition' crates/wsw-runtime/src/lib.rs     # → 既有状态机原语（D4/D6 消费面）
grep -E '^name = "(moka|time|rand)"$' -A1 Cargo.lock          # → moka 0.12.16 / time 0.3.55 / rand 0.9.5+0.10.4
grep -n 'rip-data' .gitignore                                 # → 47 行 rip-data/（D3 存储位）
python3 -c "import sqlite3; c=sqlite3.connect('file:migrations/dev-b1.db?mode=ro',uri=True); print(c.execute('PRAGMA user_version').fetchone()[0])"  # → 23（024 链头推进起点）
sed -n '23p;31p' Containerfile                                # → gcc libc6-dev 无 cmake（D10 回退依据）
```
