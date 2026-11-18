# WSW-0 · 01 候选库评估：MITM / HTTP / TLS / WebSocket 栈

- 文档类型：WSW-0 只读审计产物（WEB-SECURITY-WORKBENCH-PLAN.md §九 WSW-0 第 7/8 条）
- 日期：2026-11-18（所有 crates.io / GitHub / OSV 数据为当日在线核查值）
- 仓库：`research-platform`（只读审计；探针在 `/tmp/wsw0/`，不入库）
- 上位：`docs/rip3/WEB-SECURITY-WORKBENCH-PLAN.md` §十 候选表；`docs/rip3/decisions/ADR-0002` §四（增量 cargo vendor + OSV 复查 + 钉版本台账）
- 状态：评估定稿，裁决待设计评审；未锁定任何 crate，未改动 workspace

---

## 0. 结论速览

| 项 | 结论 |
|---|---|
| **第一选（MITM 主体）** | **hudsucker 0.25.0**，`default-features = false, features = ["rcgen-ca", "rustls-client", "http2"]`，钉 `=0.25.0` |
| **次选** | **http-mitm-proxy 0.18.0**（`default-features = false, features = ["rustls-client"]`） |
| **弃选** | slinger-mitm 0.0.5（能力面不足）；全自研 core 栈（违背复用优先）；slinger 0.2.15 仅留作 Repeater 保真方向的后续观察 |
| 底座（随第一选引入） | hyper 1.12.0 + hyper-util 0.1.21 + rustls **0.23.46**（安全门）+ tokio-rustls 0.26.6 + tokio-tungstenite 0.30.0 + rcgen 0.14.10 + h2 0.4.20 |
| vendor 增量（第一选最小化配置） | **51 个新 crate，约 84 MiB**（其中 aws-lc-sys 一项 63.6 MiB）；默认 feature 全开则 66 新 crate / 约 98 MiB |
| **勘误（2026-11-18 评审修订批回填）** | 实际落库 vendor 增量为 **97 包条目 / 74 全新 name / 167.6 MiB**（du -sb，ADR-0004 §2.3 差距 (e) 实测）——上行 51 crate / 84 MiB 是 `cargo tree` 运行时树口径，lock 全量条目额外含新栈 dev-deps（loom/tracing-subscriber/nu-ansi-term 等）与 windows-\* 目标 crate（vendored 但 Linux 构建不参与）；两套口径并存说明见 ADR-0004 §五，成本评估以 lock 口径为准 |
| 与 workspace 冲突面 | 版本漂移≈0（tokio 1.54.1 / tokio-util 0.7.20 / futures 0.3.34 全部复用）；tokio 需增 `net` feature（additive）；aws-lc-sys 为不可关闭的 C 构建面 |

---

## 1. 本仓底数（本地实测）

核查命令与结果（本会话实跑）：

```
$ grep -cE '^\[\[package\]\]' Cargo.lock            → 310（285 个唯一 name）
$ du -sh vendor/                                    → 212M
$ grep 'name = "(tokio|hyper|rustls|tungstenite|...)"' Cargo.lock
```

workspace Cargo.lock 中**不存在** `http / http-body / hyper / hyper-util / h2 / rustls / tokio-rustls / tungstenite / tokio-tungstenite / rcgen` —— 整个 MITM 栈是净新增，任何候选都要背全量增量。既有相关底数：

| crate | 现有锁定版本 | 备注 |
|---|---|---|
| tokio | 1.54.1 | features：rt, rt-multi-thread, macros, io-std, io-util, sync, time（`Cargo.toml:57`，**缺 net**，MITM 监听需 additive 增补） |
| tokio-util | 0.7.20 | |
| futures / futures-core / futures-util | 0.3.34 | 与候选栈要求完全兼容 |
| bytes | 1.12.1 | 探针解析到 1.12.2，同 semver req，现有 lock 保留不冲突 |
| mio | 1.2.5 | |
| tracing / async-trait / slab / pin-project-lite | 0.1.44 / 0.1.92 / 0.4.12 / 0.2.17 | 探针树中版本一致 |
| **rustc / cargo** | **1.99.1（2026-10-13）** | workspace `rust-version = "1.99"`（`Cargo.toml:31`） |

vendor 纪律：`.cargo/config.toml` 走 `vendored-sources`（离线构建），vendor 更新走 `cargo vendor` 全量重生成（PROBE-RESULT.md 约定）；许可按 ADR-0002「记录不做淘汰门槛、能力优先」——两层口径调和见 ADR-0004 §2.4（该记录制适用于容器/镜像侧组件；vendor 树内 Rust crate 受 deny.toml 许可门 fail-closed 约束）。

> 行号口径注（2026-11-18 评审修订批补）：本节 `Cargo.toml:N` 原为审计时点快照（HEAD 32c452c7）；POC 批在 members 数组插入 `crates/wsw-poc` 一行使后续行整体 +1，修订批已按当前工作树逐条 `grep -n` 复核校正（tokio :57、rust-version :31 等）。后续批次再改 Cargo.toml 会再次漂移，引用前以实测为准。

---

## 2. 逐库档案

### 2.1 hudsucker（MITM 主体候选）

| 维度 | 实测值 | 来源 |
|---|---|---|
| 最新版 | **0.25.0**（2026-07-15 发布，v0.25.0 tag 同日） | crates.io API + GitHub releases |
| 发布节奏 | 0.25.0 → 2026-07-15；0.24.1 → 2026-05-04；0.24.0 → 2025-10-02；0.23.0 → 2024-09-22 | crates.io versions |
| 最近 commit | 2026-08-25 `refactor: remove IoStream adapter`（main，未发布） | GitHub API |
| 仓库活跃 | omjadas/hudsucker，pushed 2026-10-25，388★，16 open issues | GitHub API |
| 许可证 | crate：MIT OR Apache-2.0；仓库 LICENSE 识别 Apache-2.0 | crates.io / GitHub |
| MSRV | 1.86.0 < 本仓 1.99.1 ✓ | crates.io rust_version |
| 下载量 | 615,329 | crates.io |
| OSV 公告 | **0 条** | api.osv.dev |
| 平台 | 依赖面（tokio+hyper+rustls+rcgen+ring/aws-lc）均跨平台；本仓仅 Linux 实测，三平台构建留 WSW-9 | 本仓探针 |
| HTTP/1.1 + HTTPS CONNECT + TLS MITM | 核心能力；`HttpHandler`（请求/响应可改）+ `CertificateAuthority` trait（可插拔 CA）+ rcgen 签发 + moka 证书缓存（default `rcgen-ca`） | 源码 `src/lib.rs:111`、`src/certificate_authority/mod.rs:23` |
| HTTP/2 | opt-in `http2` feature：下游 `enable_http2()` + ALPN（`src/proxy/builder.rs:160`、`rcgen_authority.rs:117` cfg http2），上游经 `hyper-rustls?/http2` → 双向 h2 | 源码 + manifest |
| WebSocket | **帧级双向**：下游 hyper-tungstenite 升级检测 + 上游 tokio-tungstenite（`src/proxy/internal.rs:145`），`WebSocketHandler` trait（`src/lib.rs:175`）+ `with_websocket_handler`（builder.rs:273） | 源码 |
| Body 流式 | `Body` 内部枚举含 `Incoming`（hyper 流式）与 `BoxBody/StreamBody`，提供 `Body::from_stream`（`src/body.rs:11-58`）—— 库层面流式保持，内存边界由我方 History 层强制上限 | 源码 |
| 解压辅助 | `decoder` feature（async-compression brotli/gzip/zlib/**zstd**）+ `decode_request/decode_response` helpers（lib.rs:17-19） | manifest |
| 优雅停机 | 内置 tokio-graceful ^0.2（对齐计划 §7.5 Listener 终态） | manifest |
| 已知问题 | issue #142 `Host` 头被移除（保真面）、#133 `wss://` 拦截异常、#170 证书链（Node 场景）、#176 透明代理为未实现请求（计划本就不做第一版） | GitHub issues |
| 版本面 | hyper ^1.7.0、hyper-util ^0.1.3（server/http1/client-legacy）、tokio-rustls ^0.26.3（**默认 features 开 → aws_lc_rs provider 不可经 feature 关闭**）、tokio-tungstenite ^0.30、rcgen ^0.14（x509-parser）、futures ^0.3.11、http ^1.1 | vendored manifest |

### 2.2 http-mitm-proxy（MITM 主体次选）

| 维度 | 实测值 |
|---|---|
| 最新版 | 0.18.0（2026-01-24）；0.17.0 → 2026-01-17；0.16.0 → 2025-07-27 —— 2025-2026 持续发版 |
| 最近 commit | 2026-11-15（`cargo update`）—— 六库中除 hyper/rustls 外最活跃 |
| 仓库 | hatoo/http-mitm-proxy，65★，12 open issues，MIT |
| MSRV | 未声明（rust_version=None）；crate 自身以 tokio 1.44+ 编译面为准，1.99.1 无阻碍 |
| 下载量 | 48,425 |
| OSV | **0 条** |
| 平台 | 同 hudsucker 依赖面；default `native-tls-client` → Linux 需系统 OpenSSL（libssl-dev） |
| HTTP/1.1 + CONNECT MITM | 核心能力；`MitmProxy<I>` + tower `Service` 风格处理器（`src/lib.rs:36`、bind 泛型 S::ResBody=BoxBody） |
| HTTP/2 | **双向已具备**：下游 server_config.alpn=["h2","http/1.1"]（`src/lib.rs:294`）+ `alpn_protocol()==Some(b"h2")` 分支（lib.rs:205），上游 native-tls/rustls connector 带 `request_alpns(["h2","http/1.1"])`（default_client.rs:192-234） |
| WebSocket | **无帧级处理**：仅 hyper `with_upgrades()` + `upgrade::on()` CONNECT 隧道透传（lib.rs:106/155/214）；帧级拦截需自接 tokio-tungstenite（issue #66/#53） |
| Body | 泛型 ResBody（hyper 生态）可流式；无内置解压 helpers |
| CA | 直接暴露 `rcgen::Issuer` + `moka::Cache<String, CertifiedKeyDer>` 字段（lib.rs:37-42），无 CA 抽象 trait |
| 已知问题 | #82 rustls 去 native-tls（已有 `default-features=false + rustls-client` 路径）、#68 按域名直通、#66/#53 WS 文档面 |
| 版本面 | hyper ^1.0.1（http1+http2+server+client）、hyper-util ^0.1.11(tokio)、tokio-rustls ^0.26.1（**默认 features 开 → aws-lc-rs**）、rcgen ^0.14.3、moka ^0.12.8(sync)、tokio ^1.44.1、webpki-roots ^1.0.1(optional) |

### 2.3 slinger / slinger-mitm

| 维度 | 实测值 |
|---|---|
| slinger 最新版 | 0.2.15（2026-06-20），GPL-3.0-only，27,551 下载，"HTTP Client for Rust designed for hackers"，features：charset/cookie/dns(hickory)/gzip/http2(dep h2)/rustls/schema/serde |
| slinger-mitm 最新版 | **0.0.5**（2026-04-12），GPL-3.0-only，**1,684 下载**，38K 源码包 |
| 仓库 | emo-crab/slinger，pushed 2026-08-24（dependabot merge），32★，0 open issues |
| MSRV | 未声明；1.99.1 编译面无阻碍（探针解析成功） |
| OSV | 0 条 |
| HTTP/2 / WebSocket | **mitm 面两者皆无**：`grep -rn 'alpn|h2|websocket|upgrade' src/` 零命中（仅 server.rs:591 注释）；slinger 客户端 http2 仅经 h2 client 面 |
| Body | `MitmRequest::set_body(impl Into<Bytes>)` / `body() -> Option<&Body>` —— **缓冲式**，无流式面（interceptor.rs:145-150） |
| MITM API | Interceptor trait + MitmRequest/Response + ProxyServer/Socks5Server（2.1k 行总量）—— 面小、可用，但无拦截暂停语义、无 h2/WS |
| TLS provider | tokio-rustls **default-features=false + ring**（唯一避开 aws-lc-rs 的候选，探针 16 MiB 增量） |
| 结论 | 作为 MITM 主体：**弃选**（0.0.x + 无 h2/WS/流式 + 缓冲 Body + GPL）；slinger 客户端保留为 Repeater 原始保真（raw TCP/HTTP 重放）方向的观察候选，若未来采用另立 ADR 记录 GPL |

### 2.4 hyper / hyper-util（协议底座）

| 维度 | 实测值 |
|---|---|
| hyper 最新版 | 1.12.0（2026-10-06，release v1.12.0 同日），MIT，MSRV 1.63，1,142,241,783 下载 |
| hyper-util 最新版 | 0.1.21（2026-09-24），MIT，MSRV 1.85 |
| 仓库 | hyperium/hyper pushed 2026-11-17（当日活跃），16,418★；最新 commit 即一条保真修复：`fix(http1): preserve streaming bodies when coercing HTTP/2 requests (#4236)` |
| OSV | 14 条历史公告，最高修复位 0.14.12（RUSTSEC-2021-0078/0079、2022-0022）；**1.x 全部不受影响** |
| 平台 | 生态标准三平台 |
| 特性 | `full = client+http1+http2+server`；default 无特性，按需取 client/server/http1/http2 子集 |
| 角色 | **不单独自建 MITM，随 hudsucker/hmp 引入**；Repeater/Fuzzer 发送面也复用 hyper client（http1/http2 双栈） |

### 2.5 rustls / tokio-rustls（TLS 底座）

| 维度 | 实测值 |
|---|---|
| rustls 最新 stable | **0.23.46**（2026-10-14，release `v/0.23.46`）；另有 0.24.0-dev.3（2026-11-12）预发布——不采用 |
| tokio-rustls 最新版 | 0.26.6（2026-09-28），MIT OR Apache-2.0，MSRV 1.71 |
| 仓库 | rustls/rustls pushed 2026-11-16，7,758★；rustls MSRV 1.85 |
| OSV | rustls 14 条，其中 **5 条 2026 年新公告**（详见 §5）：RUSTSEC-2026-0285（fixed 0.23.45）、0346/0347/0348/0349（**全部 fixed 0.23.46**）→ **钉版门：rustls ≥0.23.46**；tokio-rustls 仅 RUSTSEC-2020-0019（0.17 时代），0.26.6 不受影响 |
| provider | 默认 aws_lc_rs（aws-lc-sys 63.6M vendor + C 构建）；可 `default-features=false + ring` 纯 Rust 路线（本仓 core 探针实测可行，无 aws-lc-rs）——但 hudsucker/hmp 自身 dep spec 锁 default features，组合面下 aws-lc-rs 不可关 |
| 平台 | 官方支持 Linux/macOS/Windows（ring 与 aws-lc-rs 双 provider）；aws-lc-sys Windows 需 NASM（prebuilt-nasm feature 可用预置目标码） |
| CA 面需配套 | rcgen 0.14.10（2026-08-28，MIT OR Apache-2.0，MSRV 1.88，0 条 OSV）用于 CA 签发；rustls-pemfile 2.x / rustls-native-certs 0.8.x 按需 |

### 2.6 tokio-tungstenite（WebSocket 底座）

| 维度 | 实测值 |
|---|---|
| 最新版 | 0.30.0（2026-07-11，与 tungstenite 0.30.0 同日），MIT，MSRV 1.85，366,338,577 下载 |
| 仓库 | snapview/tokio-tungstenite pushed 2026-10-24，2,524★ |
| OSV | tokio-tungstenite 0 条；tungstenite 仅 RUSTSEC-2023-0065（<0.20.1），0.30.0 不受影响 |
| 能力 | 帧级读写（Message 流）、connect（默认）、handshake、TLS：`rustls-tls-webpki-roots` / native-tls 等可选；tungstenite 的 rustls dep 为 `default-features=false, features=["std"]`（不强制 aws-lc-rs）——hudsucker 组合面下 WS 上游客户端直接可用 |
| 平台 | 依赖面跨平台 |

---

## 3. 比较矩阵

| 维度 | hudsucker 0.25 | http-mitm-proxy 0.18 | slinger-mitm 0.0.5 | 自研 core 栈 |
|---|---|---|---|---|
| 定位 | MITM proxy 库（Burp 类工作台后端） | MITM proxy 库（同向） | MITM + socks5（极早期） | 自己拼 hyper+rustls |
| HTTP/1.1 MITM | ✓（核心） | ✓（核心） | ✓ | 自建（CONNECT/签发/缓存全套） |
| HTTPS CONNECT + 证书签发 | ✓ CA trait + rcgen + moka 缓存 | ✓ rcgen Issuer + moka 字段（无抽象 trait） | ✓ CertificateManager | 自建 |
| HTTP/2 双向 MITM | ✓（`http2` feature：下游 ALPN+enable_http2，上游 hyper-rustls/http2） | ✓（下游 ALPN h2 + 上游 h2 connector） | ✗ | 自建（hyper-util http2 server + ALPN 手配） |
| WebSocket 帧级双向 | ✓（hyper-tungstenite + tokio-tungstenite + WebSocketHandler） | ✗（仅 CONNECT 隧道透传） | ✗ | 自建 |
| SSE | ✓（流式响应透传） | ✓ | 未知 | 自建 |
| Body 流式 | ✓（Incoming/StreamBody；tee 类辅助内部化） | ✓（泛型 ResBody） | ✗（Bytes 缓冲） | 自建 |
| 解压 helpers | ✓（decoder feature，含 zstd） | ✗ | ✗ | 自建 |
| CA 抽象 | ✓ trait 可插拔 | ✗ | ✓（内部） | 自建 |
| 优雅停机 | ✓ tokio-graceful | ✗（自行管理） | ✗ | 自建 |
| 维护（最近 commit） | 2026-08-25（crate 07-15 发版） | 2026-11-15（crate 01-24 发版） | 2026-08-24（crate 04-12 发版） | — |
| 许可证 | MIT OR Apache-2.0 | MIT | **GPL-3.0-only** | — |
| MSRV | 1.86 | 未声明 | 未声明 | — |
| OSV（最新版） | 0 | 0 | 0 | — |
| 下载量 | 615,329 | 48,425 | 1,684 | — |
| 新增 crate / vendor（相对本仓） | **66 / ~98 MiB**（最小化 51 / ~84 MiB） | 38 / ~81 MiB（rustls 变体 31 / ~81 MiB） | 35 / ~16 MiB | **29 / ~13 MiB** |
| 不可关闭的 C 面 | aws-lc-sys（63.6M）+（decoder 时）zstd-sys | aws-lc-sys（rustls 变体同样） | 无（ring） | 无（ring） |

---

## 4. vendor 增量成本评估（探针实测）

方法：`/tmp/wsw0/` 六个探针工程（不入仓），`cargo tree --edges normal --prefix none` 解析 + `cargo vendor` 计体积，与 workspace `Cargo.lock` 285 个 name 求差。本机 cargo 走 rsproxy.cn 镜像（与 PROBE-RESULT.md 同源）。探针含 `tokio full`（workspace 实际用 feature 子集，不影响 crate 计数口径）。

| # | 探针配置 | 树解析包数 | 新增 crate（相对 lock） | 新增 vendor 体积 |
|---|---|---|---|---|
| A | 自研 core：hyper 1.12 full + hyper-util(0.1.21, tokio/client-legacy/server/http1/http2) + http-body-util + rustls 0.23.46(ring) + tokio-rustls 0.26.6(ring) + rustls-pemfile + tokio-tungstenite 0.30(rustls-tls-webpki-roots) + rcgen 0.14.10 + futures-util | 85 | **29** | **~13 MiB** |
| B | hudsucker 0.25.0 default+http2（decoder+rcgen-ca+rustls-client） | 139 | **66** | **~98 MiB** |
| C | hudsucker-min：`default-features=false, features=["rcgen-ca","rustls-client","http2"]` | 124 | **51** | **~84 MiB** |
| D | hmp 0.18.0 default（native-tls-client） | 92 | **38** | **~81 MiB** |
| E | hmp-rustls：`default-features=false, features=["rustls-client"]` | 84 | **31** | **~81 MiB** |
| F | slinger-mitm 0.0.5（默认） | 98 | **35** | **~16 MiB** |

大头构成（du -sb 实测）：**aws-lc-sys 63.6 MiB**（vendor 内含完整 aws-lc C 源）、ring 7.4 MiB、zstd-sys 3.5 MiB、brotli 系 2.3 MiB、moka 1.4 MiB。C 配置（B）超出 C 配置的 47 新 crate 中主要是 async-compression/zstd/brotli 解压链；D/E 中 openssl 系（native-tls/tokio-native-tls/openssl-sys 等）在 rustls 变体消失（openssl 不 vendor 源码但需系统 libssl-dev）。

**版本冲突面（实测结论）**：

1. **双版本并存 = 0**。六棵探针树对共享 crate 解析出的版本与本仓 lock 完全一致：tokio 1.54.1、tokio-util 0.7.20、futures*/futures-core/futures-util 0.3.34、mio 1.2.5、async-trait 0.1.92、tracing 0.1.44、slab 0.4.12、pin-project-lite 0.2.17。唯一差异 bytes 1.12.2 vs lock 1.12.1（同 `^1` req，现行 lock 原样保留即无冲突）。
2. tokio 需在 `Cargo.toml:57` features 增补 **`net`**（TcpListener/TcpStream/MITM 监听必需）；additive，不破坏 rmcp 现有 feature 面。
3. **aws-lc-rs 无法经 feature 关闭**（第一/次选共同点）：hudsucker 0.25 与 hmp 0.18 的 `[dependencies.tokio-rustls]` 均未设 `default-features = false`，cargo feature 加和后 `aws_lc_rs` 必编译。缓解：aws-lc-sys 自带 cc_builder 回退（vendor 内 `builder/cc_builder.rs`）与预生成绑定（bindgen 为可选 feature），主 Containerfile builder 层（`Containerfile:21-25`，gcc + libc6-dev）**可能无需增补 cmake 即可构建**，WSW-2 POC 必须实测；若回退不可用，需在 builder 层增补 cmake（rip-pg17 镜像已有先例：`containerfiles/rip-pg17/Dockerfile:22`）。Windows 目标需 NASM（prebuilt-nasm）。
4. 第一选若全开 `decoder`，追加 zstd-sys（C）——建议最小化后由我方以 async-compression 按需取 gzip/brotli/zlib，zstd 不进第一版。
5. vendor 台账：按 ADR-0002 `cargo vendor` 全量重生成 + 钉 `=x.y.z` + digest 记录；第一选落库时 Cargo.lock 预计从 310 包增至 ~360+ 包。

---

## 5. 安全公告核查（api.osv.dev，2026-11-18）

| crate | 公告数 | 与候选版本相关项 | 判定 |
|---|---|---|---|
| hudsucker / hyper-util / tokio-tungstenite / rcgen / slinger / slinger-mitm | 0 | — | 干净 |
| http-mitm-proxy | 0 | — | 干净 |
| hyper | 14 | RUSTSEC-2021-0078/0079（fixed 0.14.10）、2022-0022（fixed 0.14.12）——均为 0.14.x 时代 | 1.12.0 不受影响 |
| http | 4 | RUSTSEC-2019-0033/0034（0.x 时代） | 1.5.0 不受影响 |
| h2 | 7 | RUSTSEC-2023-0034(<0.3.17)、2024-0003(<0.4.2)、2024-0332(<0.4.4)、**2026-0258(<0.4.16，unbounded empty DATA frames DoS)** | 钉 0.4.20 ✓（lock 面经 hyper/http2 传递，需显式锁 ≥0.4.16） |
| rustls | 14 | RUSTSEC-2024-0336(<0.23.5)、2024-0399(0.23.13–0.23.18)、**2026-0285（0.23.13–0.23.44，TLS1.3 跨加密级握手消息，fixed 0.23.45）**、**2026-0346（Debug impl 泄露 TLS secrets，fixed 0.23.46）**、**2026-0347（ECH 拒绝后缓冲数据外泄，0.23.44–0.23.45，fixed 0.23.46）**、**2026-0348（Brotli 证书解压预认证 1GiB 分配，fixed 0.23.46）**、**2026-0349（未认证 ServerHello 降级经典 KEX，fixed 0.23.46）** | **钉 =0.23.46 为硬门**；四条 2026 公告 11-08/11-09 才挂 OSV，检查节奏：实施批次 OSV 复查 + 上线门 cargo audit |
| tokio-rustls | 2 | RUSTSEC-2020-0019（0.17 时代） | 0.26.6 不受影响 |
| tungstenite | 2 | RUSTSEC-2023-0065（<0.20.1，CVE-2023-43669 DoS） | 0.30.0 不受影响 |

---

## 6. 选型裁决

### 6.1 第一选：hudsucker 0.25.0（最小化 feature 集）

```
hudsucker = { version = "=0.25.0", default-features = false, features = ["rcgen-ca", "rustls-client", "http2"] }
```

理由（按计划 §十 硬要求逐条对照）：

1. **协议覆盖最全且是六候选中唯一"HTTP/1.1+HTTP/2 双向 MITM+WS 帧级双向"三者齐备的库**（§3 矩阵）；SSE 经流式 Body 天然透传。
2. **面向 Burp 类工作台设计**（crates.io 描述："intended to be a backend of application like Burp proxy" 同类的另一实现正是 http-mitm-proxy，但仅 hudsucker 有拦截所需的完整抽象：HttpHandler / WebSocketHandler / CertificateAuthority trait + tokio-graceful）。
3. **Body 流式**：`Body` 保 `hyper Incoming`/`StreamBody`（body.rs:11-58），配合我方 Artifact CAS 尺寸上限即可满足 §6.3 大流量内存边界。
4. **维护与安全**：2026-07 发版、08 月仍有 commit；0 条 OSV；MIT OR Apache-2.0（记录面）；MSRV 1.86 < 1.99.1。
5. **零版本漂移**：探针 C 树与本仓 lock 共享 crate 版本全一致；vendor 增量 51 crate / ~84 MiB（aws-lc-sys 63.6M 为主，见 §4.3）。
6. 解码（decoder）按需外置，不扭曲我方"Wire/Parsed/Display/Outbound 四表示"模型（计划 §6.3）—— 恰好强制我们自控解压面。

代价：51 新 crate（含 aws-lc-sys C 构建面）、上游 main 分支已有未发布重构（升级需 POC）、issue #142/#133/#170 列入 conformance 测试清单。

### 6.2 次选：http-mitm-proxy 0.18.0（rustls 变体）

切换条件：hudsucker 在 WSW-2 POC 中 h2/WS 或保真问题不可修复，或希望 API 更薄（tower Service 直加）。

理由：维护最勤（2026-11-15 commit）、31 新 crate 与 hudsucker-min 同量级、h2 双向 MITM 已实现（源码级确认）；但**缺 WS 帧级处理与 CA 抽象**，拦截暂停队列、Frame History 全部自建（WSW-5 工作量上移），且其 WS 边界有未收口 issue（#66/#53）。若采用：`http-mitm-proxy = { version = "=0.18.0", default-features = false, features = ["rustls-client"] }` + 自接 tokio-tungstenite 0.30。

### 6.3 弃选及理由

| 弃选对象 | 理由 |
|---|---|
| slinger-mitm 0.0.5 | 无 HTTP/2、无 WebSocket、Body 缓冲式（set_body(Bytes)）、0.0.x（1.7K 下载）、GPL-3.0-only；能力面不满足 §4.1 第二优先级，且 vendor 优势（16 MiB）不足以抵消全部自建面 |
| slinger 0.2.15（客户端） | 同上许可与成熟度；其 raw-socket 语义对 Repeater 原始保真有独特价值，**保留为观察候选**，若 hyper client 保真不足再立 ADR 评估。许可口径以 ADR-0004 §2.4 调和声明为准（deny 门约束 vendor 树 Rust crate，ADR-0002 记录制适用于容器/镜像侧组件，两层不冲突）；采纳前提 = deny.toml allow 变更 + 用户修订 ADR-0002 §三裁决 2 |
| 全自研 core 栈（探针 A） | 13 MiB/29 crate 最省，但要重造 CONNECT MITM、证书签发缓存、ALPN h2、WS 升级、优雅停机全套语义，违背计划 §二 2.3-8「先审计、复用、Adapter 化」与 §九 WSW-0 第 9 条；core 栈结论是**以 hudsucker 组合面引入**（hyper/rustls/tokio-tungstenite/rcgen 全部随包进入） |

### 6.4 底座钉版表（随第一选落库，ADR 引用本表）

| crate | 钉版 | 钉版理由 |
|---|---|---|
| hudsucker | =0.25.0 | 最新发布；main 未发布重构隔离在外 |
| hyper | =1.12.0 | 2026-10-06 stable，1.x 无 OSV |
| hyper-util | 0.1.21 | server/http1/http2/client-legacy 面 |
| rustls | =0.23.46 | **安全门**：RUSTSEC-2026-0346/0347/0348/0349 fixed 0.23.46（§5） |
| tokio-rustls | 0.26.6 | 2026-09-28 |
| tokio-tungstenite / tungstenite | =0.30.0 | hudsucker dep req 锁死 0.30；0 条 OSV |
| h2 | ≥0.4.16（lock 0.4.20） | RUSTSEC-2026-0258 fixed 0.4.16 |
| rcgen | =0.14.10 | CA 签发；hudsucker req ^0.14 |
| tokio | features +`net` | additive |

---

## 7. 风险（评审用 ≤5 条）

1. **aws-lc-sys 构建面**：经 hudsucker/hmp 的 tokio-rustls 默认 features 不可关闭；63.6M vendor + C 构建。缓解：cc_builder 回退 + 预生成绑定大概率免 cmake，但 WSW-2 POC 必须在主 Containerfile builder 层实测；Windows 需 NASM。
2. **上游 API 稳定性**：hudsucker main 已有 0.25 之后的重构 commit（2026-08-25），0.26 可能破坏性变更；钉 `=0.25.0`，升级走独立 POC。
3. **保真已知缺陷**：hudsucker issue #142（Host 头被移除）、#133（wss 拦截）、#170（证书链）—— WSW-1 消息模型 conformance 与 WSW-5 WS 验收必须覆盖，必要时上层重注入 Host 或向上游提补丁。
4. **HTTP/2 帧级细节**：h2 MITM 只验证到"连接可建立、版本面正确"（源码/依赖级），多路复用流取消、CONTINUATION 边界、SSE-over-h2 需 WSW-0 POC conformance 实测；QUIC/H3 按计划只做识别透传。
5. **平台矩阵未实测**：本次仅本机 Linux（cargo tree/vendor 解析级），Windows/macOS 构建与运行留 WSW-9 按计划 §十一.9 验收。

---

## 8. 复现命令（本会话已实跑）

```bash
# 1) 底数
cd /media/emo/code/pensting/research-platform
grep -cE '^\[\[package\]\]' Cargo.lock                      # 310
du -sh vendor/                                              # 212M
grep -E '^name = "(tokio|hyper|rustls|tungstenite|rcgen|h2|http)"' Cargo.lock   # 仅 tokio 1.54.1 命中

# 2) crates.io / GitHub / OSV（示例）
curl -sA "ua" "https://crates.io/api/v1/crates/hudsucker" | python3 -m json.tool | head
curl -s -X POST "https://api.osv.dev/v1/query" -d '{"package":{"name":"rustls","ecosystem":"crates.io"}}' | grep -o 'RUSTSEC-[0-9-]*'

# 3) 依赖树探针（任选其一，配置见 §4 表）
mkdir -p /tmp/wsw0/p-hud-min/src && printf 'fn main(){}\n' > /tmp/wsw0/p-hud-min/src/main.rs
cat > /tmp/wsw0/p-hud-min/Cargo.toml <<'EOF'
[package]
name = "p_hud_min"
version = "0.1.0"
edition = "2021"
[dependencies]
tokio = { version = "1", features = ["full"] }
hudsucker = { version = "0.25.0", default-features = false, features = ["rcgen-ca", "rustls-client", "http2"] }
EOF
cd /tmp/wsw0/p-hud-min && cargo tree --edges normal --prefix none | sort -u | wc -l   # 124
cargo vendor vendor && du -sh vendor                                                  # ~148M（新增部分 ~84M）

# 预期：tree 行数与 §4 表一致；vendor 新增体积 ~84 MiB；tokio 解析 1.54.1 与 workspace 一致
```

失败回退：探针失败不影响仓库（/tmp 隔离）；本仓文件零改动，本文档为唯一新增。
