# WSW-1 任务书 · HTTP Message Foundation

- 批次：WSW-1（PLAN §九 :603：Rust 消息模型、解析/序列化、Artifact Adapter、协议测试语料；**不包含 MITM UI**）
- 日期：2026-11-18
- 上位：ADR-0004（`docs/rip3/wsw/04-ADR.md`）、设计草案（`docs/rip3/wsw/05-DESIGN-AND-THREAT-MODEL.md`，下称 DESIGN）、四份审计 00-03
- 状态：**已完成（2026-11-18）**；用户明确授权执行，设计复核与批次结果见 `04-ADR.md` §七、`WSW-1-BATCH-REPORT.md`
- 开批门：已复核通过。ADR-0004 / DESIGN §1.2 字段冻结；rustls/h2/hyper/hudsucker OSV/RustSec 目标版本复核无匹配公告。全工作区 `cargo deny check advisories` 实际运行后失败，发现既有 `libsqlite3-sys` CVE-2026-119375 与 chrono/paste/unsafe-libyaml unmaintained 公告；它们不是 WSW-1 新增版本，需独立安全批处理，详见批次报告 §7。

---

## 一、目标（可验收定义）

1. **消息模型**：`crates/wsw-message` 提供四表示类型（Wire/Parsed/Display/Outbound，DESIGN §1.1）与 Exchange 结构（DESIGN §1.2 全字段），Wire 字节为唯一原始事实。
2. **HTTP/1.1 解析与序列化**：原始字节 ↔ `http` 类型双向往返；请求（origin/absolute-form、CONNECT 隧道解密后的明文、CL/chunked/无体语义）与响应（CL/chunked+trailer/HTTP/1.0 EOF）全支持；头序保持、重复头多值保留、`capture_fidelity`/`parse_status` 如实标注。请求无 TE/CL 时不吞并下一条 pipelined 请求。
3. **Artifact Adapter**：Body 正文 CAS 写入（sha256 寻址、去重、保留完整原始字节；cap 标记 fidelity，内联预览截断复用 envelope 协议）+ `wsw_body_blob` 登记行（023 迁移）；通过 trait/adapter 将 ToolRun/Exchange/Task/Trace 与既有 Artifact/Evidence Registrar 对接（本批不创建第二套 Artifact 表，具体 Registrar 实现由后续批次消费）。
4. **协议测试语料**：钉值 fixtures + 负例 fixtures 入库，成为 WSW-2/WSW-3/WSW-5 共用 conformance 面。

非目标：Listener/MITM 运行时（WSW-2）、UI、Repeater/Fuzzer 发送、HAR 导出（WSW-3/6）、h2 帧级消息（h2 经 hudsucker 组合面在 WSW-2 收口；本批消息模型对 `http::Version::HTTP_2` 只做标记不解析）。

## 二、文件清单草案

| 文件 | 动作 | 内容 |
|---|---|---|
| `Cargo.toml` | 修改 | `[workspace.dependencies]` 增补五项协议钉版 `http = "=1.5.0"`、`http-body = "=1.1.0"`、`http-body-util = "=0.1.5"`、`httparse = "=1.10.1"`、`bytes = "=1.12.2"`，另将已在 lock/vendor 的 `sha2 = "=0.10.9"`、`base64 = "=0.22.1"` 声明为本 crate 直接依赖；**registry package 零增量**（ADR-0004 §四.5） |
| `crates/wsw-message/Cargo.toml` | 新建 | 依赖：五项协议钉版 + 已在 lock 的 sha2/base64 + serde/serde_json + domain + database（rusqlite 经重导出，勿直连）+ tool-engine（仅复用 envelope 截断语义）；**不含 tokio/rcgen/rustls/reqwest** |
| `crates/wsw-message/src/lib.rs` | 新建 | 模块声明 + 职责注释（对齐 DESIGN §1.1 不变式 3 条） |
| `crates/wsw-message/src/types.rs` | 新建 | `WireBytes`（新类型）、`ParsedMessage`、`DisplayPayload`、`OutboundBytes`、`CaptureFidelity`、`ParseStatus` |
| `crates/wsw-message/src/ids.rs` | 新建 | `ExchangeId`/`SessionId`/`ListenerId` 生成与校验（沿 `crates/domain/src/ids.rs` 范式；六 ID 引用 domain 再导出面） |
| `crates/wsw-message/src/parse.rs` | 新建 | httparse 驱动的 HTTP/1.1 wire→Parsed：请求行/状态行/头序保持/重复头多值/三体定界；头帽 64KiB（先例 `crates/tool-engine/src/document_fetch.rs:259`，00 §2.2） |
| `crates/wsw-message/src/serialize.rs` | 新建 | Parsed/编辑态→Outbound 字节（重序列化；含编辑头序保持与体重定界） |
| `crates/wsw-message/src/redact.rs` | 新建 | `redact` 接口：脱敏规则输入→Display 输出（DESIGN §4.5 唯一挂点；本批落规则词表与函数，UI 消费 WSW-6） |
| `crates/wsw-message/src/cas.rs` | 新建 | CAS 文件层：`store_body(bytes, cap) -> BodyRef{sha256,uri,size,fidelity}`；原子 no-clobber 落档、读回 hash 校验、路径/符号链接防护；完整原始字节保留，cap 标记 fidelity |
| `crates/wsw-message/src/exchange.rs` | 新建 | Exchange 结构（DESIGN §1.2 全字段）+ serde round-trip |
| `crates/wsw-message/src/artifact.rs` | 新建 | `BodyArtifactAdapter`：CAS 写入 → 通过 database Pool 登记 `wsw_body_blob` → 以幂等键调用现有 Artifact/Evidence Registrar trait，关联 ToolRun/Exchange/Task/Trace；不新建 Artifact/Evidence 表 |
| `crates/wsw-message/src/store.rs` | 新建 | `wsw_body_blob` 登记/查询/GC 引用计数接口（rusqlite 经 database 重导出，勿直连；`database::migrate` 执行器消费） |
| `migrations/023_wsw_body_blob.sql` | 新建 | CAS 表 + 索引；文件尾自钉 `PRAGMA user_version = 23`（链头纪律先例 `migrations/022_coverage_matrix.sql:56`）；**只在开发库副本与测试内存库执行**（`migrations/022_coverage_matrix.sql:17-19` 纪律原文） |
| `crates/wsw-message/tests/fixtures/wire/` | 新建目录 | 钉值语料（§四.1）+ 负例语料（§四.2），每文件一例 + 期望 JSON |
| `crates/wsw-message/tests/parse_conformance.rs` | 新建 | 钉值 + 负例测试（对齐 tool-engine 集成测试范式 `crates/tool-engine/tests/` 13 文件先例） |
| `crates/wsw-message/tests/cas_e2e.rs` | 新建 | CAS/BodyBlob 落档、去重、读回 hash、迁移副本重放与源库不变性实测（§四.3） |
| `docs/rip3/wsw/WSW-1-BATCH-REPORT.md` | 新建 | 批次报告：实测命令与结果、registry lock diff（零新增 package；总数仅加本地 crate）、advisory 遗留、fixtures 清单 |

workspace 成员注册：`Cargo.toml` members 数组加 `"crates/wsw-message"`（现行 members 见 `research-platform/Cargo.toml:3-26`，wsw-poc 在 :24）。

## 三、批内步骤（S1→S8，每步过门再进）

1. **S1 底座钉版（完成）**：workspace 成员注册；五项协议依赖 + sha2/base64 等号钉版；Cargo.lock 总条目 397→398（仅新增本地 workspace `wsw-message` 包），registry 条目 375→375 且 `(name,version,source,checksum)` 全量元组一致；vendor 零变更；`cargo deny check licenses` 绿。
2. **S2 类型与 ID（完成）**：types.rs/ids.rs/exchange.rs、serde round-trip；Exchange 字段逐项对照 DESIGN §1.2。
3. **S3 解析器（完成）**：parse.rs 请求/响应 framing、头序/重复头、冲突 CL 与非支持 TE fail-closed；10 正向 + 7 负向 golden fixtures 全过。
4. **S4 序列化（完成）**：serialize.rs 往返测试；chunked 无 trailer 可重定界为 CL，有 trailer 则保留 chunked trailer，不静默丢数据。
5. **S5 脱敏接口（完成）**：七头词表 + 自定义规则；仅 Display 输出变换，Wire/Parsed 原始内容不回写。
6. **S6 CAS Adapter + 023 迁移（完成）**：CAS 原子落位、0600/0700 权限、读回 hash/URI 校验；BodyArtifactAdapter 将 CAS、wsw_body_blob、现有 Artifact/Evidence Registrar trait 与 ToolRun/Exchange/Task/Trace 关联。现有 dev-b1 源文件本身已是 schema_version 23；测试仅在临时副本回退 023 表/版本记录至 22，再由正式 executor 重放 023；源 dev-b1 SHA-256 前后相同。
7. **S7 conformance 语料整理（完成）**：10 正向 + 7 负向 fixture，每个 wire 文件有 SHA-256 固定值与完整 ParseOutcome expected JSON；README 说明合成来源、覆盖意图和更新纪律。
8. **S8 收口（完成）**：批次报告、任务书、DESIGN、ADR 与总路线已同步；023 是 WSW-1 唯一迁移。

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

### 4.3 CAS / BodyBlob / Artifact 桥接实测（2 项 CAS E2E + artifact.rs 单测）

1. `store_body(≥帽字节)` → 正文文件落 `<RP_ARTIFACTS_DIR>/wsw/<前2>/<sha256>` 且读回字节 sha256 与登记值全等（sha256 小写 hex，005 口径 `migrations/005_tool_contract.sql:86`）。
2. 同字节二次 `store_body` → 返回同 BodyRef（去重实证，PLAN :576）。
3. 023 迁移后 `wsw_body_blob` 行存在且 sha256/size/uri 列断言；测试内存库 + 开发库副本两处跑通。

### 4.4 测试面

- 单测：types/ids/parse/serialize/redact/cas/store/artifact 八模块 `#[cfg(test)]`（round-trip、词表、帽值边界、CAS 原子性与 Artifact 桥接）。
- 集成：`tests/parse_conformance.rs`（10 正向 + 7 负向完整 golden）+ `tests/cas_e2e.rs`（2 项 CAS/迁移端到端测试）。
- 门禁命令（必须执行并记录结果；工作区 advisories 有继承性失败，不能伪报全绿）：
  ```bash
  cd /media/emo/code/pensting/research-platform
  cargo fmt -p wsw-message -- --check          # WSW crate 格式检查绿
  rustfmt --edition 2021 --check \\
    crates/agent-engine/tests/migration_dev_copy_p7_3.rs \\
    crates/agent-engine/tests/migration_idempotent.rs \\
    crates/database/tests/backup_restore.rs \\
    crates/database/tests/migrations_agent_data_plane.rs \\
    crates/database/tests/migrations_context_broker.rs \\
    crates/database/tests/migrations_runtime_node.rs \\
    crates/database/tests/migrations_tool_contract.rs \\
    crates/plugin-runtime/tests/lifecycle_full_path.rs \\
    crates/workflow-engine/tests/migration_replay.rs  # 绿
  cargo test -p wsw-message --offline          # 70 unit + 2 CAS E2E + 3 parse conformance 全绿
  cargo test --offline --workspace             # 全 workspace tests 全绿（最终复跑）
  cargo check --workspace --offline            # 全 workspace check 绿
  cargo clippy --offline --workspace --all-targets -- -D warnings  # 绿
  cargo deny check licenses                    # 绿
  cargo deny check advisories                   # 当前失败：既有 libsqlite3-sys CVE 与 3 项 unmaintained，详见 ADR-0004 §七.2
  cargo metadata --locked --offline --no-deps --format-version 1 >/dev/null  # 绿
  grep -cE '^\[\[package\]\]' Cargo.lock       # 398（397 基线 + 1 个本地 workspace package）
  grep -c '^source = "registry+' Cargo.lock     # 375（registry package 零增量）
  git diff --check                              # 绿
  python3 -c "import sqlite3; p='migrations/dev-b1.db'; c=sqlite3.connect('file:'+p+'?mode=ro',uri=True); print('user_version',c.execute('PRAGMA user_version').fetchone()[0], 'schema_max',c.execute('SELECT MAX(version) FROM schema_version').fetchone()[0])"  # 源 dev-b1 已是 23；测试仅在临时副本回退并重放 023，源 SHA 不变
  ```
  说明：`cargo test -p wsw-message` 是本批验收测试面全量，不是抽样；workspace check 覆盖 wsw-poc 在内全部成员。

## 五、红线清单（批内任何步骤不得越过）

1. **零 vendor/registry 增量**：registry package 375 个且版本/source/checksum 元组与 WSW-0 HEAD 完全一致；Cargo.lock 总数增加的唯一条目是本地 workspace `wsw-message`（397→398）；vendor 目录无变更。
2. **Rust-First**：解析/序列化/存储核心全 Rust；Python/Node 不出现在本批任何实现路径（PLAN §6.1 :444；00 差距 G12）。
3. **不碰生产库**：`migrations/platform*.db` 不写（022 纪律 `migrations/022_coverage_matrix.sql:17-19`）；RTIS 不直写（`crates/intel-db/src/lib.rs:10` 红线）。
4. **不建第二套事实体系**：六 ID/trace_id/artifact/evidence 引用全走既有链（DESIGN §3.1；02 矩阵红线）；新增表只在 023+ 增量迁移。
5. **不转正 wsw-poc**：本批不复制 POC handler 代码为生产实现（ADR-0004 D2）；loopback 守卫等参照只读不搬（WSW-2 才搬）。
6. **不引高层客户端**：reqwest/任何 HTTP client 不进 wsw-message（发送面 WSW-3 用 hyper client，随现有栈）。
7. **截断口径唯一**：内联预览/输出超帽走 envelope 语义（`crates/tool-engine/src/envelope.rs:281-321`），不另造截断字段；CAS 保留完整原始正文，`BodyRef.fidelity` 只标记采集是否超 cap。
8. **不动 tokio feature 面**：wsw-message 不依赖 tokio；workspace tokio features 增补 `net` 是 WSW-2 的事（ADR-0004 POC 先例 `crates/wsw-poc/Cargo.toml:38`），本批禁改。

## 六、退出条件与回退

- 任一门禁红且两次修复无新信息 → 批次挂起，批次报告如实记录挂起点（不允许降标准换"通过"）。
- 023 迁移若与 022 链冲突：迁移文件重新编号（024+），DESIGN §3.2 同步改号，禁止改历史迁移文件。
