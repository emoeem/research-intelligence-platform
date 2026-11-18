# WSW-0 批次报告：Rust 网络栈审计、选型裁决与 MITM POC 收口（含评审批修订）

> 批次：WSW-0（Web Security Workbench Phase 0：只读审计 ×4 + 库选型裁决 + 网络栈 POC + 评审修订批）。
> 基线 commit = `32c452c7`（RIP3 Phase 5 RELEASE 收口）；接续 = P5-9c 六场景双绿收口。
> 证据标注口径：`【本会话复核】` = 本报告写作时实跑的命令与回显；`【POC 批留证】` = POC 批实跑留证、本报告引用其 evidence 原文（网络运行面未重跑，靶场/代理用完即停已清理）。

## A. 本批完成内容

1. **四份只读审计入库（用户四项 WSW 素材）**：
   - `00-RUST-NETWORK-AUDIT.md` 网络栈审计：POC 前全仓 HTTP/WS crate 零条目（00 §1.1 原文："`Cargo.lock` 共 285 个 package（python3 解析逐块计数），其中**网络协议栈相关 crate 零条目**"，grep 名称锚定零命中）；现有面 = document_fetch 纯 HTTP/1.0 无 TLS（`FetchRejection::SchemeUnsupported` "no TLS stack in this batch; http only"，document_fetch.rs:167）、EgressProxy CONNECT 盲字节泵（egress_proxy.rs:514-533）、rp_server stdlib `BaseHTTPRequestHandler`（rp_server.py:86/:3112）。
   - `01-LIBRARY-EVALUATION.md` 库评估：胜出 **hudsucker 0.25.0**（钉 `=0.25.0`，`default-features=false` + `features=["rcgen-ca","rustls-client","http2"]`）+ 随包底座（hyper 1.12.0 / hyper-util 0.1.21 / rustls =0.23.46 安全门 / tokio-rustls 0.26.6 / tokio-tungstenite 0.30.0 / rcgen 0.14.10 / h2 ≥0.4.16）。
   - `02-REUSE-MATRIX.md` 复用矩阵（含评审修订后的 WebSocket 缺口行，见 A2/F3）。
   - `03-UI-COMPONENT-AUDIT.md` UI 组件审计：全量读 `web/src`（58 文件），对照 WSW 十一子页面缺件清单，配套素材 `docs/ui-ux/AI-IMPLEMENTATION-GUIDE.md`。
2. **规划与裁决面落仓库**：`docs/rip3/WEB-SECURITY-WORKBENCH-PLAN.md` 新增（docs/README.md 索引 +1 行）；P5 计划 §4.10 改写为"完整 Web Security Workbench 的边界"并指向 WSW 扩展规划（tracked diff 14 行）；`04-ADR.md`（ADR-0004，裁决 D1–D5 + §2.3 差距七项全录 + §四.3 deny 红态补记 + §六 复现实录）；`05-DESIGN-AND-THREAT-MODEL.md`（消息模型四表示/Exchange 字段/数据归属双层/八条安全边界逐条映射）；`WSW-1-TASK.md`（S1→S8 步骤 + 完成标准 + 八条红线）。
3. **网络栈 POC（pass=true, inRepo=true）**：`crates/wsw-poc`（Cargo.toml + src/main.rs，workspace 成员注册 `Cargo.toml:24`【本会话复核】）。六项功能实测全 PASS：HTTP 明文走代理改写、HTTPS CONNECT + 动态证书 MITM、rcgen 运行时签发、真实互联网 HTTPS MITM（crates.io API 200/91KB）、WS 帧级 handler 挂载、回环守卫（详见 E 节 evidence 原文）。**fallback=无（回退未触发）**：vendor 增量一轮成功，未启用 /tmp/wsw-poc 独立项目回退路径。
4. **vendor 增量落库**：cargo vendor 全量重写 375 包；`Cargo.lock` 310→397 包条目（285→360 unique name，【本会话复核】`grep -cE '^\[\[package\]\]'` / `git show HEAD:Cargo.lock` 实测）；vendor `du -sh` = 386M / `du -sb` = 357,782,770 字节（【本会话复核】）；增量口径 = 97 包条目 / 74 全新 name / 167.6 MiB（POC 批 du -sb 实测，01 §0 勘误行回填）。
5. **评审批收口**：评审 10 issues（2 high / 5 medium / 3 low）→ 修订 F1–F10 **全部 applied，rejected=[]**；两处 high（deny 门红态、续作手册断链）均已闭环（详见 A2）。

### A2. 坏消息单列（评审发现 ×10 与修订去向）

| # | 严重度 | 发现（评审原文摘要） | 修订 | 验证 |
| --- | --- | --- | --- | --- |
| 1 | high | S1 门禁 `cargo deny check licenses` 实测 FAILED 9 error：8 个 POC 批 vendor 进来的 crate 许可（ISC/BSD-2-Clause/BSD-3-Clause/CDLA-Permissive-2.0）不在 allow 清单 | F1：deny.toml 补录四项许可（带来源注释）+ ADR §四.3 第 4 条补账 + §六 复现块 + WSW-1-TASK S1 出处注；评审判 webpki-roots 为 ISC/BSD 类与实装不符（实装 CDLA-Permissive-2.0），按实测补录 | `cargo deny check licenses` → **licenses ok**【本会话复核】 |
| 2 | high | `WSW-CONTINUATION-WORKFLOW.md`（全程手册）磁盘不存在，workflow-drafts 与记忆指针引用断链 | F2：原文不可恢复（git 全历史/全盘 find/rollout 零命中）→ 新建固化稿 `docs/rip3/wsw/WSW-CONTINUATION-REDLINES.md`（文首明示"非原文恢复"，八步循环 + 九条红线逐条带出处）；workflow-drafts `:2` 与记忆指针 `:3/:14` 同步改指 | 固化稿在库（ls 实测，5372 字节）；引用面已改 |
| 3 | med | 02 §2.4 "workspace 唯一 HTTP 栈 = mcp-server 的 hyper"证据失实（mcp-server 无 hyper） | F3：改"POC 前全仓零 HTTP/WS crate（00 §1.1 实测）"，判定列"需新增"不变；名称锚定复核 `git show HEAD:Cargo.lock` 该组 crate = 0（评审原命令 `grep -cE 'hyper\|axum\|http'` = 289，命中的全是 source 行，属口径差） | 02 §2.4 修订行在库（grep 实测） |
| 4 | med | 05 §4.1 "八条一条不漏"缺 PLAN §7.2 首条（Scope 复用 ResearchTask）映射 | F4：05 §4.1 补"规则源"条目（ResearchTask 目标集 + 既有工具执行边界导入，挂 wsw_listener/wsw_session 配置 JSON；映射口径 WSW-3 定稿；过渡期 default deny fail-closed） | 05:163 修订行在库（grep 实测） |
| 5 | med | 许可纪律两层口径冲突（ADR-0002 记录制 vs deny fail-closed；slinger 观察候选与硬门矛盾） | F5：ADR §2.4 补调和声明（deny 门约束 vendor 树内 Rust crate，记录制适用于容器/镜像侧组件）；slinger 采纳前提 = deny.toml allow 变更 + 用户修订 ADR-0002 | ADR §2.4 声明段在库 |
| 6 | med | WSW-1-TASK §二依赖清单与 S6 矛盾（store.rs 需 SQLite、cas_e2e 需 database、红线 7 需 tool-engine envelope） | F6：Cargo.toml 行改"五钉版 + serde/serde_json + domain + database（rusqlite 经重导出，勿直连）+ tool-engine（仅 envelope 截断语义）"；"无 IO"改"无网络 IO、零 tokio" | 任务书修订行在库 |
| 7 | med | 性能基线归属矛盾（PLAN 字面要求 WSW-0 交性能基线 + 架构图/数据流图；ADR D5 裁决移交 WSW-2） | F7：PLAN §九 WSW-0 第 8 条（:623）与 §11.6（:740）各补勘误行；§11.6 正文改"WSW-2 建立以下基线（验收数字）" | 勘误行 grep 实测（:623/:740） |
| 8 | low | 01 §0 "51 crate/84 MiB"未回填真实增量 | F8：§0 补勘误行（97 条目/74 name/167.6 MiB；两套口径并存说明指向 ADR §五） | 01:20 勘误行在库 |
| 9 | low | scope_decision 三态枚举缺 PLAN History 表第三态 Unknown | F9：05 §1.2 补"Unknown 由 scope_decision 为 NULL 表达，词表不设 unknown 值，UI 层把 NULL 渲染为 Unknown" | 05:41 修订行在库 |
| 10 | low | 行号漂移（tokio :57 实为，文档写 :56 等 ±1） | F10：根因 = POC 批 members 插行 +1；00/01 行号逐条校正 + 两文档补"行号口径为审计时点快照（HEAD 32c452c7）"注 | 00:77 / 01:49 口径注在库；tokio :57 / rust-version :31 本会话 sed 实测 |

另登记（非修订项，ADR §2.3 差距 (c) 承载）：hudsucker 0.25.0 CA 类型实名 `RcgenAuthority`，01 曾写作 `RcgAuthority` 一处笔误，以 `crates/wsw-poc/src/main.rs:15` 导入名为准（ADR-0004 §2.3 (c) 行在库）。

## B. 架构变化和复用说明

### 选型与钉版（ADR-0004 D1）

MITM 主体钉 **hudsucker `=0.25.0`**（`default-features=false` + `["rcgen-ca","rustls-client","http2"]`，crates/wsw-poc/Cargo.toml 实测）+ 随包底座。lock 钉版表【本会话复核，逐条 grep Cargo.lock 实测】：

| crate | lock 版本 | crate | lock 版本 |
| --- | --- | --- | --- |
| hudsucker | 0.25.0 | tokio-rustls | 0.26.6 |
| rustls | 0.23.46（安全门，RUSTSEC-2026-0346~0349 硬线） | tokio-tungstenite | 0.30.0 |
| rcgen | 0.14.10 | h2 | 0.4.20 |
| hyper | 1.12.0 | hyper-rustls | 0.27.10 |
| hyper-util | 0.1.21 | hyper-tungstenite | 0.30.0 |
| aws-lc-sys | 0.47.0 | moka | 0.12.16 |

否决备选（D1/01 §6）：http-mitm-proxy（缺 WS 帧级与 CA trait）、slinger-mitm（GPL-3.0-only 撞 deny fail-closed）、全自研（复用优先）。

### 复用矩阵（02 修订后口径 + D7 八条边界映射）

| 能力 | POC 前现状（02/00 实测） | 复用物 | 判定 |
| --- | --- | --- | --- |
| MITM/拦截代理引擎 | 未发现（`grep mitm` crates 零命中） | 新 crate + hudsucker 组合面（D1） | 需新增 |
| WebSocket 客户端/帧引擎 | POC 前全仓零 HTTP/WS crate（00 §1.1） | hyper/h2/tokio-tungstenite 随 WSW-2 引入 | 需新增 |
| HTTP History 存储 | 无对应表 | 新迁移 SQLite 023+（wsw_body_blob），行面挂 artifact/evidence/trace_id | 需新增迁移 |
| Scope/重定向 | document_fetch `check_hop` 闸序（document_fetch.rs:558/563 + FetchPolicy :63-71） | 闸序移植 | 复用 |
| DNS rebinding/私网 | 全地址私网复核（document_fetch.rs:460-514）+ private_reason 词表（:433） | 移植 + 云元数据段扩展 | 复用 |
| 速率/并发/取消 | tool-engine 状态机/watchdog（state.rs:38/48、podman_host.rs:615-616） | 范式复用 | 复用 |
| 截断口径 | envelope 语义唯一挂点（tool-engine/src/envelope.rs:281-321） | WSW-1 红线 7 | 复用 |
| Body 存储双层 | 被动捕获走 wsw_body_blob CAS（SQLite 023+，正文落 RP_ARTIFACTS_DIR/wsw/<sha2>/）；主动发送经 Engine::submit 落 tool_run/artifact 既有链（05 §3.3/:143）——artifact 表 run_id NOT NULL（005_tool_contract.sql:78-90）不可承载无 tool_run 的 Exchange | D6 裁决 | 复用+新增 |
| Scope 规则源 | ResearchTask 目标集与既有工具执行边界（egress_proxy 白名单 / Manifest egress.ports / document_fetch allowlist）导入，过渡期 default deny（05:163，F4 修订） | 复用面 WSW-3 定稿 | 复用 |
| loopback 监听守卫 | POC 先例 main.rs:195-203（is_loopback 校验，非 loopback exit(2)，【本会话复核】sed 实测） | WSW-2 搬入 | 参照 |

### 架构裁决要点（04-ADR / 05-DESIGN）

- **D2**：wsw-poc 不转正为生产实现，保留 workspace 成员作 conformance 载具与 WSW-2 实施参照（聚合缓冲读、无 Scope、无持久化三面与生产要求冲突；参照代码只读不搬）。
- **D3**：vendor 维持全量 `cargo vendor` 入库 + 等号钉版 + cargo deny/OSV 复查三件套；**WSW-1 零新增 vendor**（红线 1：lock 397 不变）。
- **D4**：IP SAN 缺口（hudsucker `gen_cert` 仅 `SanType::DnsName`，vendor/hudsucker/src/certificate_authority/rcgen_authority.rs:85-86【本会话复核】）生产面自实现 `CertificateAuthority` trait（WswAuthority），WSW-2 必做。
- **D5**：POC 只登记功能覆盖面与资源静态面，无吞吐/延迟/内存基线；正式基线 WSW-2 验收（PLAN §11.6 勘误 :740）。
- **消息模型**：四表示（Wire/Parsed/Display/Outbound）+ Exchange 18 字段草案冻结为 WSW-1 类型面；新 crate `crates/wsw-message` 纯解析层（无网络 IO、零 tokio，F6 修订口径）；事件由 Rust 后端定义、前端只展示。

## C. 修改文件清单（git status --porcelain 实测对照，2026-11-18【本会话复核】）

总量：**235 M / 5 D / 118 ?? = 358 条**。非 vendor 面 5 M + 4 新条目；其余全为 vendor 面（230 M + 5 D + 114 新目录）。

**Tracked 修改（5 项，逐文件）**：

| 文件 | 变化（diff 实测） |
| --- | --- |
| Cargo.toml | +1 行：members 注册 `"crates/wsw-poc"`（现 :24） |
| Cargo.lock | 全量重写：310→397 包条目 / 285→360 unique name（本会话对 HEAD 实测）；hudsucker/rustls/h2/rcgen/hyper 系新栈钉版入链 |
| deny.toml | +9 行：[licenses] allow 补录 ISC / BSD-2-Clause / BSD-3-Clause / CDLA-Permissive-2.0 + 来源注释（F1） |
| docs/README.md | +1 行：WSW 扩展规划索引（指向 rip3/WEB-SECURITY-WORKBENCH-PLAN.md） |
| docs/rip3/RIP3-PHASE5-…-PLAN.md | §4.10 改写（14 行 diff）："完整 Web Security Workbench 的边界" + 指向 WSW 扩展规划，"不得将当前 Phase 5 的轻量交付描述为已实现完整 MITM" |

**新增（untracked，逐文件）**：

| 条目 | 内容 |
| --- | --- |
| crates/wsw-poc/ | POC 代码：Cargo.toml（hudsucker =0.25.0 最小化 feature 集 + rcgen =0.14.10 + http-body-util 0.1 + hyper-rustls 0.27 + webpki-roots 1.0 + tokio workspace+net additive）+ src/main.rs（代理 + CA 生成 + loopback 守卫 + 错误链日志） |
| docs/rip3/WEB-SECURITY-WORKBENCH-PLAN.md | WSW 扩展规划（十一子批次 WSW-0~WSW-9，含 §11.6 勘误 :740 与 §九 勘误 :623） |
| docs/rip3/wsw/（8 文件） | 00-RUST-NETWORK-AUDIT.md / 01-LIBRARY-EVALUATION.md / 02-REUSE-MATRIX.md / 03-UI-COMPONENT-AUDIT.md（四项用户素材）+ 04-ADR.md / 05-DESIGN-AND-THREAT-MODEL.md / WSW-1-TASK.md（裁决与任务书）+ WSW-CONTINUATION-REDLINES.md（F2 固化稿） |
| docs/ui-ux/ | AI-IMPLEMENTATION-GUIDE.md（03 UI 审计配套素材） |

**vendor 变更（349 条：230 M + 5 D + 114 新目录，目录级口径）**：

- 新目录 114 个：hudsucker/hyper/hyper-util/hyper-rustls/hyper-tungstenite/h2/http/http-body/http-body-util/httparse/httpdate/rustls/tokio-rustls/tokio-tungstenite/tungstenite/rcgen/ring/aws-lc-rs/aws-lc-sys/rustls-webpki/rustls-pki-types/webpki-roots(-0.26.11)/moka/subtle/untrusted/zeroize/sha1/pem/x509-parser/der-parser/asn1-rs(-derive/-impl)/oid-registry/rusticata-macros/yasna/email_address/bstr/loom/matchers/nu-ansi-term/tracing-log/tracing-subscriber/sharded-slab/thread_local/valuable/…及 windows-* 目标 crate 全家（vendored 但 Linux 构建不参与）。
- 修改 230 条 + 删除 5 条 = cargo vendor 全量重写既有目录（版本内升级与文件重排，checksum 重算）；删除清单【逐条实测】：vendor/bit-vec/crusader.sh、vendor/email_address/LICENSE、vendor/windows-core/src/handles.rs、vendor/windows-core/src/imp/com_bindings.rs、vendor/windows-core/src/ref.rs。双版本目录并存（base64 与 base64-0.22.1、bit-vec 与 bit-vec-0.8.0、synstructure-0.13.2 等）系 lock 同名多版本，cargo vendor 规范命名，非手工产物。
- vendor 完整性：checksum 三方校验 PASS【POC 批留证原文】："lock registry pkgs: 375 / vendor dirs identified: 375 / lock→vendor 缺失: 0 / vendor→lock 多余: 0 / checksum 记录文件: 19010 / 磁盘实文件: 19385 (差值=375 个 .cargo-checksum.json) / THREE-WAY CHECK: PASS"；嵌套 .gitignore 零残留（`find vendor -name '.gitignore' … wc -l = 0`）。
- **生产 migrations/ 与 migrations_pg/ 零改动**（git status 零条目）。

## D. 数据库迁移、Schema 和兼容性变化

**零迁移。** SQLite 链头 `migrations/022_coverage_matrix.sql` 未动（`ls migrations/*.sql` 最大 022【本会话复核】）；PG 链头 `migrations_pg/V0006__search_hybrid.sql` 未动（最大 V0006【本会话复核】）。生产 `platform*.db` 零触碰。WSW-1 S6 才引入 023（wsw_body_blob CAS 表，测试内存库 + dev-b1 副本验证）。

兼容性面：workspace +1 成员（Cargo.toml:24）；tokio `net` feature 为 wsw-poc 自身 Cargo.toml additive 增补（`crates/wsw-poc/Cargo.toml:38`，workspace 底座 features 不变，WSW-1 红线 8 明确"不动 tokio feature 面"）；deny.toml allow 补录四项许可；既有 crate 无损回归【本会话复核】`cargo check -p context-broker --offline` → "Finished `dev` profile … in 0.16s"（POC 批首测 9.09s，evidence 原文）。

## E. 测试命令及真实结果（2026-11-18 实测）

### E1 构建与静态门（本会话复核实跑）

```
$ cargo clippy -p wsw-poc --all-targets --offline
    Checking wsw-poc v0.2.0 (…/crates/wsw-poc)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.01s   ← 零告警零 error
$ cargo check -p context-broker --offline
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.16s   ← 既有 crate 无损
$ cargo deny check licenses
    licenses ok                                                           ← F1 修复后绿
```

POC 批首建留证（evidence 原文）：`cargo build -p wsw-poc --offline` → "Compiling hudsucker v0.25.0 ... Compiling wsw-poc v0.2.0 ... Finished `dev` profile … in 22.96s"（首次全量，含 aws-lc-sys C 编译，全离线 vendored 源）；clippy 复跑两次均零告警。

### E2 POC 六项功能实测（POC 批留证，evidence 原文引用；网络面未重跑——靶场/代理用完即停已清理，本报告如实标注）

1. **HTTP 明文走代理 + 响应改写**：直连 vs 显式代理对照——直连 `curl -sS -D - http://127.0.0.1:8443/p59c-marker` → "HTTP/1.0 200 OK ... RIP3-P59C-LOCAL-RANGE-MARKER"（无注入头）；`curl -sS -D - -x http://127.0.0.1:3128 …` → 同 200 marker 但多 "X-Wsw-Poc: mitm-ok"。poc.log：" [WSW-POC][REQ] client=127.0.0.1:60418 method=GET uri=http://127.0.0.1:8443/p59c-marker …" + "[WSW-POC][RES] injected header X-WSW-POC: mitm-ok"；靶场 range.log 同步 "GET /p59c-marker HTTP/1.1 200"。**主判据 PASS**。
2. **HTTPS CONNECT + 动态证书 MITM**：`curl -sS -D - -x http://127.0.0.1:3128 --cacert /tmp/wsw-poc-run/ca.pem https://localhost:9443/p59c-marker` → "HTTP/1.1 200 OK"（CONNECT 隧道）+ "HTTP/2 200 ... x-wsw-poc: mitm-ok ... RIP3-P59C-LOCAL-RANGE-MARKER"（下游 ALPN h2，动态证书被 curl 经我方 CA 置信）。
3. **动态签发 TLS 层**：`echo | openssl s_client -proxy 127.0.0.1:3128 -connect localhost:9443 -servername localhost` → "subject=CN=localhost / issuer=CN=WSW-POC-ROOT-CA"（运行时 rcgen 根 CA 即时签发非预置）。
4. **真实互联网 HTTPS MITM（webpki 公共根链）**：`curl -sS -x http://127.0.0.1:3128 --cacert /tmp/wsw-poc-run/ca.pem https://crates.io/api/v1/crates/hudsucker` → "crate: hudsucker | max_version: 0.25.0"；poc.log "[WSW-POC][RES] status=200 body_len=91069"（91KB 响应体明文解密入日志）。
5. **WS 帧级 handler 挂载**：WebSocketHandler::handle_message 透传+日志已挂载，但无 WS 端点**未实测**（如实差距，见 G）。
6. **回环守卫**：`ss -tln` 双进程均只绑 loopback（"127.0.0.1:8443 … python3" / "127.0.0.1:3128 … wsw-poc"）；代理源码对非 loopback --listen 直接 exit(2)（main.rs:195-203【本会话复核】）。

### E3 vendor 增量口径实跑（POC 批留证原文）

`mv .cargo/config.toml /tmp 备份 → cargo vendor vendor → EXIT=0 → 恢复 config.toml（逐字还原）→ find vendor -name '.gitignore' -print -delete | wc -l = 0`；`python3 -I /tmp/wsw-checksum-verify.py` → 三方 PASS（数字见 C 节）；精确增量 = 新包条目 97 / 全新 name 74 / du -sb 175,723,876 字节（167.6 MiB）。

### CI（E2，按 ask 口径）

**待 watch，终态回填。** 本批 commit 后 `gh run watch` 收口，红则修复批；终态记入批次报告与 HANDOVER。

## F. 性能数据

**本批无吞吐/延迟/内存基线（未验证，禁预填）。** 按 ADR-0004 D5 + PLAN §11.6 勘误（:740"WSW-0 未建立性能基线——POC 六项实测为功能覆盖面证据，未压测；本节基线由 WSW-2 建立并作验收数字"），POC 只登记：

- 资源静态面：lock 397 包条目 / vendor 386M（du -sb 357,782,770）【本会话复核】；增量 97 条目/74 name/167.6 MiB（POC 批 du -sb 实测）。
- 构建面：首次全量 `cargo build -p wsw-poc --offline` 22.96s（含 aws-lc-sys C 编译，POC 批实测）；clippy 0.31s（POC 批）/ 1.01s（本会话复跑）。
- 正式基线（Proxy Throughput / 单请求延迟增量 / 并发连接 / History 写入吞吐 / 大 Body 内存峰值 / Repeater/Fuzzer 延迟等九项）= WSW-2 验收数字（PLAN §11.6），所有数字必须来自真实命令，不得预填。

## G. 未解决问题与风险（如实差距清单，全录）

1. **IP SAN 缺口**（已裁决 D4）：hudsucker 0.25.0 `gen_cert` 只发 DNS SAN（rcgen_authority.rs:85-86【本会话复核】），对 `https://127.0.0.1:9443` 类 IP 直连目标 curl 主机名校验必败——POC 以 localhost DNS 名绕过；生产面自实现 `CertificateAuthority` trait（WswAuthority）补 `SanType::IpAddress`，WSW-2 必做项（对应上游 issue #170）。
2. **上游纯 h2 未实测**：0.25.0 自定义 connector 已 enable_http2（ALPN 可协商），但回环夹具与 crates.io 实际均回落 http/1.1——h2-only 上游目标**未验证**。
3. **WS 面未实测**：WS 帧级 handler 已挂载但无 WS 端点；wss 上游走 hudsucker 内建 tokio-tungstenite Connector，未实测。
4. **HTTP/2 帧级细节仅验证到依赖/源码级**：连接可建立、版本面正确；多路复用流取消、CONTINUATION 边界、SSE-over-h2 需 WSW-1 POC conformance 实测。
5. **aws-lc-sys 构建面**：hudsucker/hmp 均不可经 feature 关闭（63.6M vendor + C 构建）；宿主机 gcc/perl 免 cmake 路径实测通过（build output 含 `cargo:rustc-cfg=universal` 与 `cargo:rustc-link-lib=static=aws_lc_0_47_0_crypto`，POC 批留证），**主 Containerfile builder 层容器内未实测**，Windows 目标需 NASM（prebuilt-nasm）——平台矩阵（Windows/macOS）按计划 WSW-9 验收，本次仅本机 Linux 解析级证据。
6. **上游 API 稳定性**：hudsucker main 已有 0.25 之后未发布重构（2026-08-25 remove IoStream adapter），0.26 可能破坏性变更——钉 =0.25.0，升级走独立 POC。
7. **保真已知缺陷**：hudsucker issue #142（Host 头被移除）、#133（wss 拦截异常）、#170（证书链）——WSW-1 conformance 与 WSW-5 验收必须覆盖，必要时上层重注入或提补丁。
8. **POC handler 聚合缓冲读**（BodyExt::collect，预览 200 字节）：流式 tee + 尺寸上限留 WSW-3+；POC 无 Scope、无持久化，均不转正（D2）。
9. **续作手册原文不可恢复**：固化稿为"非原文恢复"性质，条款从仓库内可考来源逐条固化；原手册九条红线精确原文无从对照，冲突时以 PLAN/ADR/任务书为准（固化稿文首声明）。
10. **QUIC/H3**：按计划只做识别透传，不做拦截（PLAN 既定范围）。
11. **评估口径差**：01 原 "51 crate/84 MiB" 系 cargo tree 运行时树口径，lock 全量条目（97/74/167.6 MiB，含 dev-deps 与 windows-* 目标 crate）为落库真实成本——已勘误回填（01:20），成本评估以 lock 口径为准。

## H. 下一批依赖和明确的非目标（WSW-1 入口）

**依赖面**：
- 载体：新 crate `crates/wsw-message`（依赖 = 五钉版 + serde/serde_json + domain + database（rusqlite 经重导出，勿直连，先例 crates/database/src/lib.rs:30-31）+ tool-engine（仅 envelope 截断语义 envelope.rs:281-321）——F6 修订后口径）；类型面 = 消息模型四表示 + Exchange 18 字段（05 §1.2 冻结稿，含 Unknown=NULL 表达）。
- S1→S8 步骤与完成标准（解析钉值 10 例 + 乱码/截断负例 7 例 + artifact 落档 3 断言）见 `WSW-1-TASK.md`；S1 门禁：五钉版 + members 注册，`cargo check --offline` 全 workspace 绿，**lock 397 零增量**，`cargo deny check licenses` 绿（allow 清单已补录，红态实录 ADR §四.3 第 4 条）。
- conformance 载具 = wsw-poc 保留成员（D2）；Scope 过渡期未配置规则一律 default deny（05:163）；八条红线（WSW-1-TASK §五）生效：零 vendor 增量 / Rust-First / 不碰生产库 / 不建第二事实体系 / 不转正 wsw-poc / 不引高层客户端 / 截断口径唯一 / 不动 tokio feature 面。
- 启动机制 = 固化稿 `WSW-CONTINUATION-REDLINES.md`（八步循环 + 九条红线 + 启动词模板）。

**非目标**：不接 UI（十一子页面 WSW-2 起）；不建生产 Listener/CA 面（WSW-2）；不碰数据库链头与 platform*.db；不做性能基线（WSW-2 验收）；零 vendor 新增；不做 QUIC/H3 拦截；不改 P5 既有工具链。

## I. UI 截图与浏览器验收证据

**本批无 UI 验收面。** WSW-0 交付物为审计文档、裁决与 CLI 形态 POC：运行证据为命令行实拍（curl 直连/走代理对照、openssl s_client、ss -tln、poc.log 与靶场 range.log 原文，见 E 节），无浏览器截图产物。`docs/ui-ux/AI-IMPLEMENTATION-GUIDE.md` 是 03 UI 审计的配套规划素材（非截图证据）；WSW 子页面 UI 验收自 WSW-2 起按 PLAN 各批完成标准执行。

## J. RTIS 对账和数据安全回归结果

**RTIS 零接触**：`git status --porcelain integrations/rtis/` = 0 行【本会话复核】；本批零 RTIS 调用、零直写（intel-db lib.rs:10 红线未触碰面）。数据安全面：POC 不落库（无 tool_run/artifact 写面，D2 三冲突之一）；生产 `platform*.db` 与 migrations/ 零改动（git status 实测）；靶场/代理只绑 loopback（ss 实测，E2 第 6 项）且用完即停（POC 批声明三后台进程全部 stop）；CA 仅 PEM 落盘供显式 `--cacert`/`--upstream-ca` 置信，不自动安装、不静默信任、不改系统代理（PLAN 禁止事项 7 红线在案）。

## K. 本批教训实录

1. **成本评估必须双口径登记**：`cargo tree` 运行时树（51 crate/84 MiB）与 lock 全量条目（97/74/167.6 MiB，含 dev-deps 与 windows-* 目标 crate）差约 2 倍；只读评估文档会低估 vendor 成本（01:20 勘误行 + ADR §五 两口径并存说明）。
2. **deny licenses 门必须随 vendor 变更同批实跑**：POC 批未跑导致 9 error 红态潜伏到评审才发现，WSW-1 开批即卡 S1；修复 = allow 补录带来源注释 + ADR §四.3 补账，且评审对 webpki-roots 许可的推断（ISC/BSD 类）与实装（CDLA-Permissive-2.0）不符——许可字段以 vendor manifest 实测为准，不凭评审印象。
3. **仓库外手册不可持久**：`WSW-CONTINUATION-WORKFLOW.md` 全历史/全盘/rollout 三路零命中，原文永久丢失；红线与启动机制必须固化进仓库并同步全部引用面（workflow-drafts + 记忆指针），固化稿自带"非原文恢复"性质声明防伪托。
4. **行号引用是快照不是锚**：Cargo.toml 插一行使全文档行号 +1（F10 根因实测：HEAD 态 rust-version :30 / tokio :56，工作树态 :31/:57）；两审计文档已补"引用前以实测为准"注。
5. **证据命令要名称锚定**：评审原命令 `grep -cE 'hyper|axum|http'` 在 lock 上命中 289（全是 `source = registry+https://…` 行），名称锚定（`^name = "…"$`）才是 0——grep 口径决定结论，复核时先看锚定面。
6. **rustls 拒 CA 证书作端实体**：夹具用 CA 证书直接当服务端证书报 `invalid peer certificate: CaUsedAsEndEntity`——属夹具配置问题非代理缺陷；hudsucker handle_error 错误链全打印一次定位，openssl 重签 CA:FALSE+serverAuth 解决。
7. **cargo vendor 增量流程**：备份 .cargo/config.toml → 全量 vendor → 逐字还原 → `find vendor -name '.gitignore' -delete` 清嵌套残留 → checksum 三方脚本核账；"一轮成功"也要回退路径在案（本批未触发）。
