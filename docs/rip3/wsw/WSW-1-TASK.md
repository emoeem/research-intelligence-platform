# WSW-1 任务书 · HTTP Message Foundation

- 批次：WSW-1（PLAN §九 :603：Rust 消息模型、解析/序列化、Artifact Adapter、协议测试语料；**不包含 MITM UI**）
- 日期：2026-11-18
- 上位：ADR-0004（`docs/rip3/wsw/04-ADR.md`）、设计草案（`docs/rip3/wsw/05-DESIGN-AND-THREAT-MODEL.md`，下称 DESIGN）、四份审计 00-03
- 状态：待设计评审通过后开批（PLAN §九 :624"经设计评审后才进入 WSW-1"）
- 开批门（缺一不开）：ADR-0004 评审通过；OSV 复查（rustls/h2/hyper/hudsucker，ADR-0004 §四.3）；本任务书字段落 DESIGN §1.2 定稿确认

---

## 一、目标（可验收定义）

1. **消息模型**：`crates/wsw-message` 提供四表示类型（Wire/Parsed/Display/Outbound，DESIGN §1.1）与 Exchange 结构（DESIGN §1.2 全字段），Wire 字节为唯一原始事实。
2. **HTTP/1.1 解析与序列化**：原始字节 ↔ `http` 类型双向往返；请求（含绝对形式、CONNECT 后明文、chunked/CL/EOF 三种体定界）与响应（CL/chunked+trailer/HTTP/1.0 EOF）全支持；头序保持、重复头多值保留、`capture_fidelity`/`parse_status` 如实标注。
3. **Artifact Adapter**：Body 正文 CAS 写入（sha256 寻址、去重、超帽截断复用 envelope 协议）+ `wsw_body_blob` 登记行（023 迁移），主动面 tool_run/artifact 链接口打通（本批只定 trait 与 adapter 函数，发送面 WSW-3 消费）。
4. **协议测试语料**：钉值 fixtures + 负例 fixtures 入库，成为 WSW-2/WSW-3/WSW-5 共用 conformance 面。

非目标：Listener/MITM 运行时（WSW-2）、UI、Repeater/Fuzzer 发送、HAR 导出（WSW-3/6）、h2 帧级消息（h2 经 hudsucker 组合面在 WSW-2 收口；本批消息模型对 `http::Version::HTTP_2` 只做标记不解析）。

## 二、文件清单草案

| 文件 | 动作 | 内容 |
|---|---|---|
| `Cargo.toml` | 修改 | `[workspace.dependencies]` 增补 `http = "=1.5.0"`、`http-body = "=1.1.0"`、`http-body-util = "=0.1.5"`、`httparse = "=1.10.1"`、`bytes = "=1.12.2"`（全部已在 lock，**零 vendor 增量**，ADR-0004 §四.5） |
| `crates/wsw-message/Cargo.toml` | 新建 | 依赖：上述五项 + serde/serde_json（workspace）+ domain（workspace 内部）+ database（workspace；rusqlite 经重导出使用，勿直连——先例 `crates/database/src/lib.rs:30-31`）+ tool-engine（workspace；仅 envelope 截断语义复用 `crates/tool-engine/src/envelope.rs:281-321`，红线 7 依赖面）；**不含 tokio/不含 rcgen/不含 rustls**（无网络 IO、零 tokio——tool-engine 自身零 tokio 依赖实测 `crates/tool-engine/Cargo.toml`，红线 8 不受影响） |
| `crates/wsw-message/src/lib.rs` | 新建 | 模块声明 + 职责注释（对齐 DESIGN §1.1 不变式 3 条） |
| `crates/wsw-message/src/types.rs` | 新建 | `WireBytes`（新类型）、`ParsedMessage`、`DisplayPayload`、`OutboundBytes`、`CaptureFidelity`、`ParseStatus` |
| `crates/wsw-message/src/ids.rs` | 新建 | `ExchangeId`/`SessionId`/`ListenerId` 生成与校验（沿 `crates/domain/src/ids.rs` 范式；六 ID 引用 domain 再导出面） |
| `crates/wsw-message/src/parse.rs` | 新建 | httparse 驱动的 HTTP/1.1 wire→Parsed：请求行/状态行/头序保持/重复头多值/三体定界；头帽 64KiB（先例 `crates/tool-engine/src/document_fetch.rs:259`，00 §2.2） |
| `crates/wsw-message/src/serialize.rs` | 新建 | Parsed/编辑态→Outbound 字节（重序列化；含编辑头序保持与体重定界） |
| `crates/wsw-message/src/redact.rs` | 新建 | `redact` 接口：脱敏规则输入→Display 输出（DESIGN §4.5 唯一挂点；本批落规则词表与函数，UI 消费 WSW-6） |
| `crates/wsw-message/src/cas.rs` | 新建 | Artifact Adapter：`store_body(bytes, cap) -> BodyRef{sha256,uri,size,fidelity}`；正文落 `<RP_ARTIFACTS_DIR>/wsw/<sha256 前2>/<sha256>`；截断调 envelope 语义（`crates/tool-engine/src/envelope.rs:281-321` 口径：超帽降级 + 原始 sha256） |
| `crates/wsw-message/src/exchange.rs` | 新建 | Exchange 结构（DESIGN §1.2 全字段）+ serde round-trip |
| `crates/wsw-message/src/store.rs` | 新建 | `wsw_body_blob` 登记/查询/GC 引用计数接口（rusqlite 经 database 重导出，勿直连；`database::migrate` 执行器消费） |
| `migrations/023_wsw_body_blob.sql` | 新建 | CAS 表 + 索引；文件尾自钉 `PRAGMA user_version = 23`（链头纪律先例 `migrations/022_coverage_matrix.sql:56`）；**只在开发库副本与测试内存库执行**（`migrations/022_coverage_matrix.sql:17-19` 纪律原文） |
| `crates/wsw-message/tests/fixtures/wire/` | 新建目录 | 钉值语料（§四.1）+ 负例语料（§四.2），每文件一例 + 期望 JSON |
| `crates/wsw-message/tests/parse_conformance.rs` | 新建 | 钉值 + 负例测试（对齐 tool-engine 集成测试范式 `crates/tool-engine/tests/` 13 文件先例） |
| `crates/wsw-message/tests/cas_e2e.rs` | 新建 | artifact 落档实测（§四.3）+ 023 迁移内存库验证 |
| `docs/rip3/wsw/WSW-1-BATCH-REPORT.md` | 新建 | 批次报告：实测命令与回显、lock diff（应为零包变更）、fixtures 清单 |

workspace 成员注册：`Cargo.toml` members 数组加 `"crates/wsw-message"`（现行 members 见 `research-platform/Cargo.toml:3-26`，wsw-poc 在 :24）。

## 三、批内步骤（S1→S8，每步过门再进）

1. **S1 底座钉版**：workspace.dependencies 五条钉版 + members 注册；`cargo check --offline` 全 workspace 绿；`grep -cE '^\[\[package\]\]' Cargo.lock` 应仍为 **397**（零 vendor 增量实证）；`cargo deny check licenses` 绿（deny.toml allow 清单已随评审修订批补录 ISC/BSD-2-Clause/BSD-3-Clause/CDLA-Permissive-2.0，红态与修复实录见 ADR-0004 §四.3 第 4 条）。
2. **S2 类型与 ID**：types.rs/ids.rs/exchange.rs + serde round-trip 单测；DESIGN §1.2 字段逐项对照（缺一即任务书执行偏差，回改 DESIGN 而非私改字段）。
3. **S3 解析器**：parse.rs 三体定界 + 头序/重复头；先跑 §四.1 钉值 → 再跑 §四.2 负例；`parse_status`/`error_code` 词表落地。
4. **S4 序列化**：serialize.rs 往返测试（Wire→Parsed→Outbound 语义等价；编辑面字节级 diff 断言）。
5. **S5 脱敏接口**：redact.rs 词表（authorization/cookie/api-key/token/password/set-cookie 七头 + 自定义规则位）+ 单测（原始字节不动，仅 Display 变换）。
6. **S6 CAS Adapter + 023 迁移**：cas.rs/store.rs；023 迁移在测试内存库执行验证（`database::migrate` 唯一执行器，`crates/database/src/migrate.rs:115`）；开发库副本 dev-b1 跑迁移 + 水位 23 断言。
7. **S7 conformance 语料整理**：fixtures 目录命名规约 + README（每文件：来源、期望、适用批次）。
8. **S8 收口**：批次报告 + 本任务书勾验 + DESIGN 指向定稿迁移号。

## 四、完成标准（逐条可验收；验收 = 命令实跑回显）

### 4.1 解析钉值测试（fixtures/wire/，最少 10 例）

| fixture | 断言要点 |
|---|---|
| `req_simple_get.http` | 请求行/头序/Host/CL=0 解析字段与期望 JSON 逐值相等 |
| `req_post_chunked.http` | chunked 解帧字节精确重组（含最后 0 块） |
| `req_absolute_form.http` | 绝对形式 URI（egress 语义先例 `egress_proxy.rs:151` host 派生一致） |
| `req_multi_header.http` | 重复 Set-Cookie/多值头保序保留为多值 |
| `req_mixed_case_headers.http` | 头名大小写原样保留（Parsed 层不规范化，Display 层才归一） |
| `req_ws_upgrade.http` | Upgrade/Connection 头透传保真（WSW-5 语料前置） |
| `res_cl_body.http` | CL 精确取体（先例 `document_fetch.rs:247-` 三体定界） |
| `res_chunked_trailer.http` | chunked + trailer 区分（trailer 不混入头表） |
| `res_http10_eof.http` | HTTP/1.0 EOF 定界 |
| `res_multi_status_headers.http` | 多同名响应头保序 |

判定：每例输入字节 sha256 固定入库，期望 JSON 与解析输出全等断言；头序断言逐位。

### 4.2 乱码/截断负例（fixtures/wire/negative/，最少 7 例，全部必须"如实标注"而非"修复"）

| fixture | 期望 |
|---|---|
| `body_invalid_utf8.bin` | Parsed 体保留原始字节；`preview` 走 lossy（POC 先例 `crates/wsw-poc/src/main.rs:45-47`）；`parse_status=ok`、fidelity=full |
| `trunc_chunked_no_term.http` | `parse_status=partial` + `error_code=truncated_chunk` |
| `trunc_header_no_crlf.http` | `parse_status=partial` + `error_code=truncated_header` |
| `trunc_body_short_cl.http` | CL 声明 > 实收：`parse_status=partial`、fidelity=truncated、记录 declared/received 差值 |
| `oversize_header_64k.http` | 头帽拒：`error_code=header_cap_exceeded`（帽值常量随 document_fetch 64KiB 先例） |
| `oversize_body.http` | 体帽：CAS 走截断协议（fidelity=truncated + envelope 语义 sha256 落档） |
| `req_no_host.http` | HTTP/1.1 请求无 Host：`error_code=missing_host`（fail-closed，不猜 authority） |

### 4.3 artifact 落档实测（cas_e2e.rs，3 断言）

1. `store_body(≥帽字节)` → 正文文件落 `<RP_ARTIFACTS_DIR>/wsw/<前2>/<sha256>` 且读回字节 sha256 与登记值全等（sha256 小写 hex，005 口径 `migrations/005_tool_contract.sql:86`）。
2. 同字节二次 `store_body` → 返回同 BodyRef（去重实证，PLAN :576）。
3. 023 迁移后 `wsw_body_blob` 行存在且 sha256/size/uri 列断言；测试内存库 + 开发库副本两处跑通。

### 4.4 测试面

- 单测：types/ids/parse/serialize/redact/cas 六模块 `#[cfg(test)]`（round-trip、词表、帽值边界）。
- 集成：`tests/parse_conformance.rs`（钉值 10 + 负例 7）+ `tests/cas_e2e.rs`（3 断言 + 迁移验证）。
- 门禁命令（全部须绿并贴回显进批次报告）：
  ```bash
  cd /media/emo/code/pensting/research-platform
  cargo test -p wsw-message --offline          # 全绿
  cargo check --workspace --offline            # 全 workspace 绿（wsw-poc 不受影响）
  cargo deny check licenses                    # 绿
  grep -cE '^\[\[package\]\]' Cargo.lock       # 397（零 vendor 增量）
  python3 -c "import sqlite3,glob; [print(db, sqlite3.connect('file:'+db+'?mode=ro',uri=True).execute('PRAGMA user_version').fetchone()[0]) for db in ['migrations/dev-b1.db']]"  # 开发库副本 23（若副本文件名不同以实测 ls migrations/*.db 为准）
  ```
  说明：`cargo test -p wsw-message` 是本批验收测试面全量，不是抽样；workspace check 覆盖 wsw-poc 在内全部成员。

## 五、红线清单（批内任何步骤不得越过）

1. **零 vendor 增量**：Cargo.lock 包数 397 不变；新增 workspace 依赖只能是 lock 已有 crate 的钉版声明（ADR-0004 §四.5）。
2. **Rust-First**：解析/序列化/存储核心全 Rust；Python/Node 不出现在本批任何实现路径（PLAN §6.1 :444；00 差距 G12）。
3. **不碰生产库**：`migrations/platform*.db` 不写（022 纪律 `migrations/022_coverage_matrix.sql:17-19`）；RTIS 不直写（`crates/intel-db/src/lib.rs:10` 红线）。
4. **不建第二套事实体系**：六 ID/trace_id/artifact/evidence 引用全走既有链（DESIGN §3.1；02 矩阵红线）；新增表只在 023+ 增量迁移。
5. **不转正 wsw-poc**：本批不复制 POC handler 代码为生产实现（ADR-0004 D2）；loopback 守卫等参照只读不搬（WSW-2 才搬）。
6. **不引高层客户端**：reqwest/任何 HTTP client 不进 wsw-message（发送面 WSW-3 用 hyper client，随现有栈）。
7. **截断口径唯一**：一切超帽截断走 envelope 语义（`crates/tool-engine/src/envelope.rs:281-321`），不另造截断字段。
8. **不动 tokio feature 面**：wsw-message 不依赖 tokio；workspace tokio features 增补 `net` 是 WSW-2 的事（ADR-0004 POC 先例 `crates/wsw-poc/Cargo.toml:38`），本批禁改。

## 六、退出条件与回退

- 任一门禁红且两次修复无新信息 → 批次挂起，批次报告如实记录挂起点（不允许降标准换"通过"）。
- 023 迁移若与 022 链冲突：迁移文件重新编号（024+），DESIGN §3.2 同步改号，禁止改历史迁移文件。
