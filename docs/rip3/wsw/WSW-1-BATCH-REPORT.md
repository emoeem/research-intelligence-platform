# WSW-1 批次收口报告

- 批次：WSW-1 — HTTP Message Foundation
- 日期：2026-11-18
- 基线 HEAD：`4e7b4c693762`
- 收口方式：本地工作树实施与验证；未 commit、未 push
- 状态：**本地验收通过；GitHub CI 未触发，安全公告门禁存在已登记的工作区遗留**

## 1. 本批交付

### 1.1 Rust crate

新增 `crates/wsw-message`，包含：

- `types.rs`：Wire / Parsed / Display / Outbound 四表示及 CaptureFidelity、ParseStatus 等类型。
- `ids.rs`、`exchange.rs`：WSW 标识符与 Exchange 结构。
- `parse.rs`：HTTP/1 请求/响应解析、头序与重复头保留、CL/chunked/EOF framing 处理、fail-closed 负例。
- `serialize.rs`：Parsed/编辑态到 Outbound 的受控序列化，保留不能安全重定界的 trailer。
- `redact.rs`：Display 侧脱敏；不回写 Wire 原始字节。
- `cas.rs`：SHA-256 内容寻址、原子 no-clobber 写入、已有对象完整性校验、路径与符号链接防护、Unix 私有权限。
- `store.rs`：通过 database 已有重导出面登记/查询 `wsw_body_blob`，提供引用检查与 GC 原语。
- `artifact.rs`：BodyArtifactAdapter 复用现有 database Pool，执行 CAS → BodyBlob 登记 → 现有 Artifact/Evidence Registrar trait；使用 ToolRun + 方向 + SHA-256 幂等键，并保留 Exchange/Task/Trace 关联。

**边界说明：** WSW-1 定义并测试现有 Artifact/Evidence Registrar 的桥接契约，不创建第二套 Artifact/Evidence/ToolRun 表；具体生产 Registrar 实现及 HTTP 捕获调用由后续批次接入。不能将本批描述为已完成 Listener/MITM 运行时集成。

### 1.2 Migration 与测试语料

- 新增唯一迁移 `migrations/023_wsw_body_blob.sql`，只创建 `wsw_body_blob` 及必要索引，并登记 schema_version/user_version 23。
- 10 个正向与 7 个负向 wire fixtures；每个 fixture 有 SHA-256 钉值和完整 ParseOutcome expected JSON。
- 新增 CAS / 迁移端到端测试；覆盖 CAS 去重、读取完整性、BodyBlob 登记、迁移副本重放及源库不变性。

### 1.3 设计文档

同步更新：

- `docs/rip3/wsw/WSW-1-TASK.md`
- `docs/rip3/wsw/05-DESIGN-AND-THREAT-MODEL.md`
- `docs/rip3/wsw/04-ADR.md`

迁移边界明确为 023 只增加 `wsw_body_blob`。Listener、Session、Exchange、Intercept、Repeater、Fuzzer 等后续表不提前塞入本迁移。

## 2. 本地验证结果

以下命令在目标设备、项目目录执行；全部成功：

| 检查 | 实际结果 |
|---|---|
| `cargo fmt -p wsw-message -- --check` | 通过 |
| 9 个既有迁移测试文件 `rustfmt --edition 2021 --check` | 通过 |
| `cargo test -p wsw-message --offline` | 70 unit + 2 CAS E2E + 3 parse conformance；75 passed，0 failed |
| `cargo check --offline --workspace` | 通过 |
| `cargo test --offline --workspace` | 142 个测试组；850 passed，0 failed，2 ignored；退出码 0 |
| `cargo clippy --offline --workspace --all-targets -- -D warnings` | 通过 |
| `cargo deny check licenses` | `licenses ok` |
| `git diff --check` | 通过 |

工作区测试结果来自最终代码状态（包含 BodyArtifactAdapter 的错误类型装箱修正）复跑，不是修正前的旧结果。

## 3. 依赖与红线核算

- Cargo.lock：397 → 398 个 package 条目；新增的唯一条目是本地 workspace package `wsw-message`。
- Registry package：375 → 375；逐项 `(name, version, source, checksum)` 与基线一致（已在 ADR-0004 §7.3 记录）。
- `vendor/`：Git 状态无变更。
- `wsw-message` 不引入 tokio、rcgen、rustls、reqwest；没有修改 Workspace tokio feature 面。
- 不修改生产数据库 `migrations/platform*.db`。dev-b1 验证在临时副本执行：副本回退 023 对应对象/版本后，由正式 migration executor 重放；源文件 SHA-256 前后相同。
- 未运行 `cargo clean`，未删除 target 产物，未清理 Podman 镜像/容器/卷。
- 未安装系统软件包，未执行 Git commit/push。

## 4. 安全扫描遗留（不得误报为全绿）

`cargo deny check advisories` 在工作区级扫描中失败，命中既有依赖公告：

- `libsqlite3-sys 0.30.1`：RUSTSEC-2026-0459 / CVE-2026-119375（bundled SQLite 版本低于公告修复线）。
- `chrono 0.4.45`：RUSTSEC-2026-0543（unmaintained）。
- `paste 1.0.15`：RUSTSEC-2024-0436（unmaintained）。
- `unsafe-libyaml 0.2.11`：RUSTSEC-2024-0449（unmaintained）。
- 对 `email_address` 与 `filetime` 的 registry/yanked 查询还有 index-failure 警告，不能视为已完成核验。

上述依赖版本不是 WSW-1 新增；本批为遵守零 registry/vendor 增量及依赖冻结，没有擅自升级。rustls/h2/hyper/hudsucker 目标版本的手工复核与自动扫描结果详见 `04-ADR.md` §7.2。应另开数据库/依赖安全批次处理，不能将整个仓库宣称为 advisory 全绿。

## 5. GitHub CI 状态

（WSW-2 批次终态回填 2026-11-19：本节收口时"未 commit 未触发 CI"的状态已由后续批次兑现，按实际 CI run 改写。）

- WSW-1 收口批已提交：commit `077a8dd4`（"WSW-1 message model and WSW-2 runtime foundation"），对应 GitHub Actions run **44293771053 conclusion=success**（headSha=077a8dd422e00b4a411bffeb27a0b83f96cade9c，`gh run view 44293771053` 实核）。
- DI-1 后续 CI 修复批 `2cef9439`（"CI 修复：DI-1 missing_capability … + 批次报告 E2 回填"）对应 run **44394568183 conclusion=success**（headSha=2cef943998efd6a7c4038c77b47346363f7d8cd5，`gh run view 44394568183` 实核）。
- 即：本批 GitHub CI 已绿，原先"不能声称 GitHub CI 已通过"的保留状态解除。

## 6. 工作树与后续边界

（WSW-2 批次终态回填 2026-11-19，仅更新事实态，不改本节其余裁决语义。）

- 本批内容已随收口批 `077a8dd4` 提交入库（WSW-1 代码、Cargo.toml/lock、`crates/wsw-message/`、migration 023 与本报告），CI 终态绿（§5）。
- WSW-2 已按总路线授权开批并实施：Listener 运行时（`crates/wsw-runtime` 原地扩展）+ WswAuthority IP SAN + 024 拦截队列持久化 + Scope 闸 + CA 管理面，验收见 `docs/rip3/batches/WSW-2-BATCH-REPORT.md`。

**结论（回填后）：** WSW-1 的本地代码与迁移验收完成且已入库，GitHub CI（run 44293771053）绿；工作区 advisory 扫描的既有依赖安全遗留记录保留原状。
