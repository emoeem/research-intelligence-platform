# ADR-0004 · WSW MITM/HTTP 栈选型、POC 转正与 vendor 策略裁决

- 编号：ADR-0004（承接 `docs/rip3/decisions/ADR-0002` 依赖台账纪律、`ADR-0003` 执行面口径）
- 日期：2026-11-18
- 状态：裁决定稿（WSW-0 收口物；WSW-1 经设计评审后开批）
- 输入：`docs/rip3/wsw/00-RUST-NETWORK-AUDIT.md`（00）、`01-LIBRARY-EVALUATION.md`（01）、`02-REUSE-MATRIX.md`（02）、`03-UI-COMPONENT-AUDIT.md`（03）四份审计 + WSW-2 POC 实拍（pass=true，inRepo=true，`crates/wsw-poc`）
- 本裁决人核实的仓库现状：本会话实跑命令与回显见 §六"复现与实核记录"，引用 path:line 均为本次回读核实。

---

## 一、裁决总览（D1–D5）

| # | 裁决 | 一句话 |
|---|---|---|
| D1 | MITM/HTTP 栈 = **hudsucker 0.25.0**（`default-features=false, features=["rcgen-ca","rustls-client","http2"]`） | 01 评估第一选经 POC 六项实测证实，钉版表落 lock 全部兑现 |
| D2 | **POC 代码保留为 conformance 载具，不转正为生产实现** | wsw-poc 是验证器不是产品：聚合缓冲、无 Scope/持久化/预算；生产面在 WSW-1/WSW-2 新 crate，CA/loopback/connector 装配代码作为实施参照搬运 |
| D3 | **vendor 策略 = 全量 cargo vendor 入库 + 钉版表 + deny 门禁三件套**，POC 期增量（310→397 包）维持现状不再扩张，WSW-1 零新增 vendor | 离线红线不可破；aws-lc-sys C 构建面宿主机已实测可行，容器内留 WSW-2 |
| D4 | **IP SAN 缺口（hudsucker gen_cert 仅 DNS SAN）= 生产面自实现 `CertificateAuthority` trait（WswAuthority）** | vendor/hudsucker rcgen_authority.rs:85-86 只签 `SanType::DnsName`；IP 直连目标（https://127.0.0.1:9443）主机名校验必败，POC 以 localhost 名绕过不构成生产答案 |
| D5 | **性能基线登记：POC 实测面为"功能覆盖面 + 资源静态面"，无吞吐基线；正式性能基线留 WSW-2 验收（规划 §11.6）** | POC 未跑压测，不得把"能跑通"登记为性能数字 |

---

## 二、D1：MITM/HTTP 栈选型

### 2.1 决定

```
hudsucker = { version = "=0.25.0", default-features = false, features = ["rcgen-ca", "rustls-client", "http2"] }
```

随包底座钉版表（01 §6.4 原表，本次逐项对 Cargo.lock 复核一致）：

| crate | lock 实值 | 钉版依据 |
|---|---|---|
| hudsucker | 0.25.0 | 01 §2.1：2026-07-15 发版、0 条 OSV、MIT OR Apache-2.0、MSRV 1.86 < 本仓 1.99 |
| hyper / hyper-util | 1.12.0 / 0.1.21 | 1.x 无 OSV（01 §5） |
| rustls | **0.23.46** | **安全硬门**：RUSTSEC-2026-0346/0347/0348/0349 全部 fixed 0.23.46（01 §5） |
| tokio-rustls | 0.26.6 | 2026-09-28；aws_lc_rs provider 经组合面不可关（01 §4.3） |
| tokio-tungstenite / tungstenite | 0.30.0 / 0.30.0 | hudsucker dep req 锁死；0 条 OSV |
| h2 | 0.4.20 | ≥0.4.16（RUSTSEC-2026-0258 fixed 0.4.16，01 §5） |
| rcgen | 0.14.10 | CA 签发；POC 实拍运行时签发可用 |
| hyper-rustls | 0.27.10 | POC 自定义上游 connector（wsw-poc/Cargo.toml:30-35） |
| aws-lc-rs / aws-lc-sys | 1.19.0 / 0.47.0 | 不可关闭的 C 构建面（01 §4.3），宿主机 POC 已实测构建通过 |
| http / http-body / http-body-util | 1.5.0 / 1.1.0 / 0.1.5 | WSW-1 消息模型类型面（本 ADR §四） |
| moka | 0.12.16 | 证书缓存（随 hudsucker rcgen-ca） |
| httparse | 1.10.1 | hyper 传递依赖已在 lock，WSW-1 wire 级解析零新增 vendor |
| bytes | 1.12.2 | 01 §4.2 预判"1.12.1→1.12.2 同 semver req 无冲突"——实际落 lock 1.12.2，预判兑现 |

### 2.2 理由（POC 实测面，六项）

POC 实拍（2026-11-18 会话，三后台进程用完即停，代码 `crates/wsw-poc`，workspace 注册 `research-platform/Cargo.toml:24`）：

1. **CONNECT 劫持**：hudsucker `should_intercept_connect` 默认全收，`curl -v` 实见 `HTTP/1.1 200 OK` 隧道响应。
2. **动态证书**：运行时 rcgen 0.14.10 生成根 CA（CN=WSW-POC-ROOT-CA，PEM 落盘供 `curl --cacert`），`RcgenAuthority` 按 SNI/host 即时签发 + moka 缓存；`openssl s_client` 实见 issuer=WSW-POC-ROOT-CA。装配代码 `crates/wsw-poc/src/main.rs:206-224`。
3. **下游 ALPN h2**：curl 走 HTTP/2 200；由 hudsucker http2 feature 在 CA ServerConfig 广告 h2+http/1.1（`vendor/hudsucker/src/certificate_authority/rcgen_authority.rs:117-121`）。
4. **上游 TLS 双链**：hyper-rustls 0.27.10 webpki 公共根对真实站（crates.io API 200/91KB）与 `--upstream-ca` 追加自签根对回环 TLS 端点均通；connector 装配 `crates/wsw-poc/src/main.rs:229-240`。
5. **明文解密 + 改写面双向实证**：请求/响应日志 + 响应注入 `X-WSW-POC: mitm-ok`（`crates/wsw-poc/src/main.rs:102-107`）。
6. **WS 能力挂载**：`WebSocketHandler::handle_message` 帧级透传+日志（`crates/wsw-poc/src/main.rs:131-140`）已挂载（无 WS 端点未实测；wss 上游走 hudsucker 内建 tokio-tungstenite Connector，未实测）——挂载面证明 WSW-5 WS 批次不需要换库。

### 2.3 如实差距（裁决的边界条件，七项全录）

| # | 差距 | 处置裁决 |
|---|---|---|
| (a) | IP SAN 缺口：hudsucker 0.25.0 `gen_cert` 只发 DNS SAN（`vendor/hudsucker/src/certificate_authority/rcgen_authority.rs:85-86`），IP 直连目标 curl 主机名校验失败；POC 以 localhost DNS 名绕过 | **D4**：生产面自实现 `CertificateAuthority` trait 补 `SanType::IpAddress`，对应上游 issue #170 面；WSW-2 必做项，不是可选项 |
| (b) | 上游 h2 未实测：0.25.0 自定义 connector 已 `enable_http2`（wsw-poc/src/main.rs:239），但夹具与 crates.io 均回落 http/1.1 | WSW-2 conformance 清单加"纯 h2-only 上游目标"实测项；不阻塞选型 |
| (c) | hudsucker 0.25.0 CA 类型实名 `RcgenAuthority`（01 评估文档曾写作 `RcgAuthority`，一处笔误） | 文档勘误即本行；代码以 `crates/wsw-poc/src/main.rs:15` 导入名为准 |
| (d) | POC handler 为聚合缓冲读（`BodyExt::collect`，预览 200 字节帽，wsw-poc/src/main.rs:59-69、:87-93），流式 tee + 尺寸上限未做 | D2 的核心理由之一：生产面 WSW-1/WSW-2 必须改流式 + 上限（规划 §6.3 :495"Body 大小必须受限"） |
| (e) | vendor 真实增量 97 新包条目 / 74 全新 name / 167.6 MiB（du -sb）超 01 评估口径 51 crate / 84 MiB | 口径澄清见 §五；非选型错误——评估按 cargo tree 运行时树计，lock 面额外含新栈 dev-deps（loom/tracing-subscriber/nu-ansi-term 等）与 windows-* 目标 crate（vendored 但 Linux 构建不参与） |
| (f) | aws-lc-sys 仅宿主机实测（gcc/perl 路径免 cmake）；Containerfile builder 层容器内构建未做 | WSW-2 POC 必测项（01 §7.1 原判不变）；失败回退 = builder 层增补 cmake（rip-pg17 镜像先例） |
| (g) | 测试夹具侧曾遇 rustls 拒 CA 证书作端实体（`invalid peer certificate: CaUsedAsEndEntity`），以 openssl 重签 CA:FALSE+serverAuth 夹具证书解决 | 属夹具配置问题；代理错误链日志（`handle_error` 全链打印，wsw-poc/src/main.rs:110-127）曾精确定位此因——该日志范式进 WSW-2 生产面 |

### 2.4 否决备选（引用 01 §6 评估 + 本裁决补充）

| 备选 | 否决理由 |
|---|---|
| **http-mitm-proxy 0.18.0**（01 §6.2 次选） | 缺 WS 帧级处理与 CA 抽象 trait（01 §3 矩阵）：拦截暂停队列、Frame History 全部自建，WSW-5 工作量上移；POC 已证实 hudsucker WS handler 挂载面可用，切换条件未触发。切换保留条款：hudsucker 在 WSW-2 中 h2/WS 或保真问题不可修复时启用（钉 `=0.18.0` + rustls-client 变体） |
| **slinger-mitm 0.0.5** | 0.0.x、无 h2/WS/流式（Body 缓冲式）、GPL-3.0-only；deny.toml:13 附近"任何 GPL/AGPL/LGPL 类强著作权一律 deny"（fail-closed）为硬门 |
| **slinger 0.2.15（客户端）** | 同 GPL 门；raw-socket 语义保留为 Repeater 保真方向的观察候选，若采用另立 ADR。**采纳前提（评审修订批补写）**：deny.toml `[licenses]` allow 清单变更 + 用户修订 ADR-0002 §三裁决 2（见下方调和声明）——硬门之下该候选不满足任一前提即不可被采纳 |
| **全自研 core 栈**（01 §4 探针 A：29 crate/~13 MiB） | 最省但要重造 CONNECT MITM、证书签发缓存、ALPN h2、WS 升级、优雅停机全套语义，违背复用优先（WSW-PLAN §九 WSW-0 第 9 条 :623）；hyper/rustls/tokio-tungstenite/rcgen 本就随 hudsucker 组合面进入 |

**许可纪律两层口径调和声明（评审修订批补写，2026-11-18）**：`ADR-0002-phase0-freezes-and-library-ledger.md:45` 用户裁决"许可证不设淘汰门槛、能力优先（AGPL 组件在容器/服务侧使用并记录）"与本 ADR 引用的 deny.toml"任何 GPL/AGPL/LGPL 类强著作权一律 deny（fail-closed）"**不冲突，是两层**——deny 门约束的是 **vendor 树内随构建进入的 Rust crate**（编译产物即承载，fail-closed 防强著作权代码进 vendor 树）；ADR-0002 记录制适用于**容器/镜像侧组件**（ParadeDB、PyMuPDF 先例，服务/容器边界隔离并逐条记录）。因此本表对 slinger-mitm / slinger 的 GPL 弃选理由成立于 vendor 门层面；slinger 0.2.15 观察候选要被采纳，必须同时满足：①deny.toml allow 变更（许可门放行）②用户修订 ADR-0002 §三裁决 2（淘汰门槛语义变更）——两前提缺一不可，在此声明之前任何批次不得引用"记录制"为由绕过 deny 门。

---

## 三、D2：POC 代码转正裁决

**决定：不转正；保留为 workspace 成员作 conformance 载具与实施参照。**

- `crates/wsw-poc` 维持 workspace 成员（`research-platform/Cargo.toml:24`）与现有钉版（`crates/wsw-poc/Cargo.toml:14-18`），角色重定义为：WSW-2 实施时的最小可运行参照（CA 生成、loopback 守卫、上游 connector 装配、错误链日志四段代码可直接搬运）+ hudsucker 升级时的回归探针（0.26 出现后改钉版跑同一 POC 即知破坏面）。
- 不转正的理由（逐条与证据对应）：
  1. handler 聚合缓冲读整段 body（差距 (d)），与规划 §6.3 :495"Body 大小必须受限，较大内容进入 Artifact/CAS"直接冲突；
  2. 无 Scope 校验、无拦截队列、无持久化（POC 纪律"不接 UI、不碰数据库"，wsw-poc/src/main.rs:11），WSW-2 需要的正是这三面（00 §三 G5/G6/G7）；
  3. 日志走 stdout println（wsw-poc/src/main.rs:41），生产面走 audit-engine Emit 链（02 矩阵 G6"可复用 audit-engine Emit 与 ProxyEvent 形状"）；
  4. loopback 硬守卫（wsw-poc/src/main.rs:195-203）语义正确但只有启动时检查，生产面还要加"Listener 令牌/会话绑定"（规划 §7.1 :514）。
- 因此 WSW-1 不以 wsw-poc 为起点，新建 `crates/wsw-message`（见 WSW-1-TASK.md）；wsw-poc 不删除、不重构、不加功能。

## 四、D3：vendor 策略

1. **全量 `cargo vendor` 入库不变**（`.cargo/config.toml` `vendored-sources` → `directory = "vendor"`，--offline 红线）；当前实态：`Cargo.lock` 397 个 `[[package]]`（POC 前 310，01 §1.1），`vendor/` du -sh 386M / du -sb 357,782,770 字节（≈341 MiB apparent）。
2. **钉版表即 D1 §2.1 表**；新增 workspace 依赖一律 `=x.y.z` 等号钉 + lock 增量记录进 ADR 台账（承接 ADR-0002 纪律）。
3. **OSV 门**：WSW 每个实施批次开批前对 rustls/h2/hyper/hudsucker 四 crate 复查 api.osv.dev + `cargo deny check licenses`（离线可跑）；rustls ≥0.23.46 为持续硬门（01 §5 四条 2026 公告 11-08/11-09 才挂 OSV，节奏必须短于 OSV 挂牌滞后）。
4. **deny licenses 门红态与修复实录（2026-11-18 评审修订批补账）**：POC 批 vendor 增量未随批同步 deny.toml，评审实测 `cargo deny check licenses` **FAILED 9 error**——8 个第三方 crate 许可不在 allow 清单（当时仅 MIT/MIT-0/Apache-2.0/Unicode-3.0/Zlib）。逐包 license 字段实测（vendor/ manifest）：aws-lc-rs 1.19.0 `ISC AND (Apache-2.0 OR ISC)`、aws-lc-sys 0.47.0 `ISC AND (Apache-2.0 OR ISC) AND Apache-2.0 AND MIT AND BSD-3-Clause AND (Apache-2.0 OR ISC OR MIT) AND (Apache-2.0 OR ISC OR MIT-0)`、rustls-webpki 0.103.17 / untrusted 0.9.0 `ISC`、ring 0.17.14 `Apache-2.0 AND ISC`、subtle 2.6.1 `BSD-3-Clause`、hyper-tungstenite 0.30.0 `BSD-2-Clause`、webpki-roots 0.26.11 / 1.0.12 `CDLA-Permissive-2.0`（**实装为数据许可，评审原判"ISC/BSD 类"与实装不符，以此为准**）。修复 = deny.toml `[licenses] allow` 补录 **ISC / BSD-2-Clause / BSD-3-Clause / CDLA-Permissive-2.0** 四项（含 lock 内组合表达式所需原子许可），复跑同命令 → `licenses ok`；WSW-1-TASK §三 S1 该门据此由红转绿，复现见 §六。
5. **WSW-1 零新增 vendor**：消息模型只消费已在 lock 的 http/http-body/http-body-util/bytes/httparse/rcgen/rustls 类型与函数，不引新 crate（含 reqwest 一类高层客户端——Repeater 发送面 WSW-3 用 hyper client，随现有栈）。
6. **windows-\*/loom 等非 Linux 目标 crate**：vendored 保留（cargo vendor 全量重生成口径，01 §4.6/§4 差距 (e)），不手工裁剪 vendor 目录——手工裁剪会在下次 `cargo vendor` 全量重生时回弹且破坏 lock 一致性校验。

## 五、性能基线登记

| 项 | 实测值（POC 实拍面） | 状态 |
|---|---|---|
| 功能覆盖面 | §2.2 六项全过（CONNECT/动态证书/下游 h2/上游双链/解密改写/WS 挂载） | 已登记 |
| 静态资源面 | vendor 增量 97 包条目/74 name/167.6 MiB（du -sb）；总面 397 包/341 MiB | 已登记 |
| 构建面 | `cargo check -p wsw-poc --offline` 通过（本会话复跑 Finished dev profile 0.54s）；aws-lc-sys 宿主机构建免 cmake | 已登记 |
| 容器构建面 | Containerfile builder 层（gcc+libc6-dev）内 aws-lc-sys 构建 | **未实测**（差距 (f)） |
| 吞吐/延迟/内存 | POC 未压测 | **无基线**；正式基线 WSW-2 按 WSW-PLAN §11.6（:737）建：拦截开关全关吞吐、全开延迟、大流量内存帽 |

裁决含义：任何"POC 已证明性能"的表述禁止出现；WSW-2 验收以 §11.6 三项数字为准，POC 六项只作为功能可行性证据引用。

## 六、复现与实核记录（本会话实跑）

```bash
cd /media/emo/code/pensting/research-platform

# lock 包数（397）与钉版面
grep -cE '^\[\[package\]\]' Cargo.lock                      # → 397
grep -E '^name = "(hudsucker|rustls|h2|rcgen|hyper|aws-lc-sys|httparse|bytes)"$' -A1 Cargo.lock
# → hudsucker 0.25.0 / rustls 0.23.46 / h2 0.4.20 / rcgen 0.14.10 / hyper 1.12.0 / aws-lc-sys 0.47.0 / httparse 1.10.1 / bytes 1.12.2

# vendor 体量
du -sh vendor/ && du -sb vendor/                            # → 386M / 357782770

# IP SAN 缺口与 ALPN 面回读
sed -n '85,86p' vendor/hudsucker/src/certificate_authority/rcgen_authority.rs   # → SanType::DnsName(Ia5String…) push
sed -n '117,121p' vendor/hudsucker/src/certificate_authority/rcgen_authority.rs # → alpn_protocols = [h2, http/1.1]

# POC 离线构建
cargo check -p wsw-poc --offline                            # → Finished dev profile … 0.54s

# workspace 成员注册
grep -n 'wsw-poc' Cargo.toml                                # → 24: "crates/wsw-poc",

# deny licenses 门（评审修订批修复实录，§四.3 第 4 条）
cargo deny check licenses                                   # 修复前：licenses FAILED（9 error，8 crate 许可不在 allow）
                                                            # 修复后（deny.toml allow 补录四许可）：licenses ok
```

预期回显与本文引用一致；deny 门禁面另见 `deny.toml:13`（GPL 类 fail-closed）与 `.cargo/config.toml`（vendored-sources）。
