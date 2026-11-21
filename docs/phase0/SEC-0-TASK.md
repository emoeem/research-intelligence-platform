# SEC-0 升级方案任务书：rusqlite/libsqlite3-sys 修复线 bump + cargo-deny advisories 引入

批次：SEC-0（依赖安全裁决员产出，实施批次待排）
日期：2026-11-20
裁决书：`docs/phase0/SEC-0-ADVISORIES-DECISION.md`（unmaintained 三条 ignore 裁决与 deny.toml 建议文本）
遗留原文出处：`docs/rip3/wsw/WSW-1-BATCH-REPORT.md` §4「安全扫描遗留（不得误报为全绿）」

---

## 0. 背景（基线实测）

WSW-1 §4 遗留原文：

> `cargo deny check advisories` 在工作区级扫描中失败，命中既有依赖公告：
> - `libsqlite3-sys 0.30.1`：RUSTSEC-2026-0459 / CVE-2026-119375（bundled SQLite 版本低于公告修复线）。
> - `chrono 0.4.45`：RUSTSEC-2026-0543（unmaintained）。
> - `paste 1.0.15`：RUSTSEC-2024-0436（unmaintained）。
> - `unsafe-libyaml 0.2.11`：RUSTSEC-2024-0449（unmaintained）。
> - 对 `email_address` 与 `filetime` 的 registry/yanked 查询还有 index-failure 警告，不能视为已完成核验。
> 上述依赖版本不是 WSW-1 新增；本批为遵守零 registry/vendor 增量及依赖冻结，没有擅自升级。

本批在仓库根复跑基线（cargo-deny 0.20.2，`~/.cargo/bin/cargo deny check advisories`）：

```
error[unmaintained]: chrono is unmaintained            （RUSTSEC-2026-0543, chrono 0.4.45）
error[vulnerability]: `libsqlite3-sys` via C SQLite CVE-2026-119375   （RUSTSEC-2026-0459, libsqlite3-sys 0.30.1）
error[unmaintained]: paste - no longer maintained       （RUSTSEC-2024-0436, paste 1.0.15）
error[unmaintained]: unsafe-libyaml is unmaintained     （RUSTSEC-2024-0449, unsafe-libyaml 0.2.11）
warning[index-failure]: unable to check for yanked crates
advisories FAILED
```

`deny.toml` 现状：只有 `[graph]` 与 `[licenses]` 段，**没有 `[advisories]` 段**——本批要新引入。

漏洞内容（RUSTSEC-2026-0459 / CVE-2026-119375 / GHSA-72w2-rjgg-q729，公告原文与
https://rustsec.org/advisories/RUSTSEC-2026-0459.html 核实一致）：SQLite < 3.54.0 在打开
数据库文件 + 页尺寸不匹配的同名 `-wal` 文件时，普通查询可返回页缓存未初始化内存（陈旧进程内存字节）。
攻击向量为本地、需受害者打开攻击者可写的两文件。`libsqlite3-sys` 的 `bundled` feature
（rusqlite 的 `bundled` 开启）随构建打包 SQLite 源码，落在受影响区间。
CVSS 6.8（Medium），机密性影响高。本仓 workspace 根钉 `features = ["bundled", "backup"]`
（`Cargo.toml:42`，DEC-001/PROBE-RESULT 约定免系统 sqlite），即实际命中 bundled 路径。

本仓环境事实：rustc 1.99.1（2026-10-13），满足目标线 MSRV；`.cargo/config.toml` 将
crates.io replace-with = vendored-sources（`directory = "vendor"`），任何版本 bump 必须
走 vendor 重写流程（§4），不允许只动 Cargo.lock。

---

## 1. 修复路径：目标版本与依据

### 1.1 目标版本

| crate | 现版 | 目标 | 依据 |
|---|---|---|---|
| `libsqlite3-sys` | 0.30.1 | **0.39.1**（经 rusqlite 传递，不单独钉） | 公告 patched = ≥0.39.0；0.39.0 bundles SQLite 3.54.0、0.39.1 bundles **3.54.1**。本批已下载 0.39.1 tarball 实测 `sqlite3/sqlite3.h:149`：`#define SQLITE_VERSION "3.54.1"` |
| `rusqlite` | 0.32.1 | **0.41.1** | crates.io API 实测各线依赖：0.39.0→`libsqlite3-sys ^0.37.0`、0.40.x→`^0.38.0` 均在受影响区间 0.6.0–0.38.2；**0.41.x（`^0.39.0`）是第一条带动修复线的 rusqlite 线**。0.41.1 = 0.41 线最新 patch（2026-10-28），实测其 `Cargo.toml` `rust-version = "1.89.0"` |

### 1.2 版本选择依据（为何不是 0.39/0.40，也不是 0.42/0.43）

- **不能低于 0.41**：rusqlite 0.39.0 → libsqlite3-sys ^0.37（bundles 3.53.x，受影响）；0.40.x →
  ^0.38（bundles 3.53.1，受影响）。只有 0.41.x 起 requirement 为 `^0.39.0`，cargo 解析才会
  取到 0.39.1。这与公告原文配对声明一致："released with rusqlite 0.41"。
- **不取 0.42.0/0.43.0**（2026-11-11/15）：requirement `^0.40.0` 同样已修复，但多跳两代
  minor 无安全增益，只扩大回归面；最小充分升级原则钉在公告关联线 0.41 的最新 patch。
- **不单独给 libsqlite3-sys 打 [patch] 钉版**：rusqlite 0.41.1 的 requirement `^0.39.0` 天然
  解析到 0.39.1，无需补丁机制，vendor 结构保持标准。

### 1.3 features 变更（workspace 根 `Cargo.toml` rusqlite 行；HEAD 基线 :42，bump 后 :50）

基线（HEAD）：`rusqlite = { version = "0.32", features = ["bundled", "backup"] }`

目标（实测 0.41.1 feature 表后，与最终 feature 集一致）：

```toml
rusqlite = { version = "0.41.1", default-features = false, features = ["bundled", "backup", "cache", "fallible_uint"] }
```

- `bundled`（0.41.1 中 = `["libsqlite3-sys?/bundled", "modern_sqlite"]`）与 `backup`（`[]`）
  两个 feature 实测仍存在且语义不变（tarball `Cargo.toml` feature 表核对）。
- **`default-features = false` 的理由**：0.41.1 的 `default = ["cache", "ffi-sqlite-wasm-rs"]`，
  其中 `ffi-sqlite-wasm-rs = ["dep:sqlite-wasm-rs"]` 会把 `sqlite-wasm-rs`（wasm 备用后端，
  本仓无 wasm target，`Cargo.toml` grep 实测零 wasm 条目）拉进解析图 → 无谓 vendor 增量。
  关掉 default 换取最小增量。
- **必须补 `cache`**：`cache = ["hashlink"]` 提供 `prepare_cached`。default-features=false
  会失去它，而 `crates/plugin-runtime/src/ingest.rs:407`、`:458` 两处 `prepare_cached` 调用
  会编译失败。显式补回。
- **必须补 `fallible_uint`**：0.38.0 起默认关闭 `u64`/`usize` 的 FromSql **读出侧**实现
  （vendored `vendor/rusqlite/src/types/from_sql.rs:143-146`：`impl FromSql for u64/usize`
  挂在 `#[cfg(feature = "fallible_uint")]` 下；feature 表 `vendor/rusqlite/Cargo.toml:112`
  `fallible_uint = []`，默认不含）。本仓读出位 `crates/database/src/retention.rs:299`、
  `:324`——`rows_before`/`rows_after: u64` 经 `query_row(..., |r| r.get(0))` 从 `COUNT(*)`
  读出（FromSql 侧，非 ToSql 绑定位），不带该 feature 编译失败。显式补回恢复 0.32 语义
  （`> i64::MAX` 时 fallible，与 0.32 相同）。
- MSRV：rusqlite 0.41.1 = 1.89.0（tarball 实测），libsqlite3-sys 0.39 = 1.88（公告）；
  本机 rustc 1.99.1 满足。CI 若有独立 MSRV job 需同步核对该 job 工具链版本。

### 1.4 tarball SHA-256 三方核对参考值（本批已实测下载计算）

```
rusqlite-0.41.1.crate          da9bba6f3506135e120a6586df05be18b48d18896600646a4bec8f993d91a594
libsqlite3-sys-0.39.1.crate    14a5f01b87b92705d07b433c06f96cbc764637d81a0bffb5734ce17cf0f9a2b3
```

来源 https://static.crates.io/crates/<crate>/<crate>-<ver>.crate，与 crates.io API
`versions[].checksum` 及 bump 后 `Cargo.lock` 的 checksum 字段三方比对。

---

## 2. 升级影响面（grep 实测）

### 2.1 唯一 sqlite 入口：database crate

- 全 workspace **只有 `crates/database/Cargo.toml:11` 声明 rusqlite**（`rusqlite = { workspace = true }`）。
  其余出现 rusqlite 字样的 manifest（mcp-server/wsw-message/workflow-engine/wsw-runtime）经核对
  全部是注释（"rusqlite 经 database 重导出使用，不直连"）。
- `crates/database/src/lib.rs:31` `pub use rusqlite;` 重导出；全部消费 crate 以
  `use database::{rusqlite, Pool}` 模式使用（实测样例 `crates/agent-engine/src/store.rs:16-17`）。
- 因此 bump 只改 workspace 根一行钉版 + Cargo.lock，代码修改面集中在 API 差异处（§2.2）。

### 2.1.1 workspace 外独立工件：probe-cargo/（不在本批范围，登记为遗留观察项）

- `probe-cargo/`（rp_probe 0.1.0，DEC-001 构建准入验证工件，CURRENT-ARCHITECTURE-INVENTORY.md §10
  处置=保留历史验证证据）是 **workspace 外独立工程**：不在根 `Cargo.toml` members（:3-30 实测），
  自带 `Cargo.lock`（rusqlite 0.32.1 / libsqlite3-sys 0.30.1，:88-90/:145-147 实测）、自带
  `vendor/` 与 `.cargo/config.toml`（vendored-sources），**无 deny.toml、不在根
  `cargo deny check advisories` 扫描面**。
- 即根 lock 过修复线后，仓库内仍有一处 libsqlite3-sys 0.30.1（受影响区间）bundled 残留，
  只是不被 advisories 门覆盖。定性：一次性验证工件（PROBE-RESULT.md 构建实测留证后即闲置），
  非生产面、不发布、不参与 workspace 构建。
- 处置登记（遗留观察项，不阻塞本批验收）：probe-cargo 若复活为活跃工程，须按本任务书 §1/§4
  同口径 bump 至 rusqlite 0.41.1 + vendor 重写；若长期闲置，保留现状并接受其不受 deny 门
  覆盖的事实（下次依赖安全批次开场核对一次其 lock 是否有变化）。本登记 2026-11-21 复核新增
  （批次实施时 §2.1 只覆盖 workspace 面，评审后复核发现此遗漏，如实补记）。

### 2.2 rusqlite API 使用面（全仓 grep 实测汇总）

```
rusqlite::params / params_from_iter   ≈109 处   稳定 API
rusqlite::Error（含 Error::ToSqlConversionFailure / QueryReturnedNoRows / FromSqlConversionFailure）≈170 处   稳定
rusqlite::Result                       ≈52 处   稳定
rusqlite::Connection（含 ::open / ::open_with_flags）   稳定
rusqlite::ToSql、rusqlite::Row、rusqlite::OptionalExtension   稳定
rusqlite::types::{Value, ValueRef, Type::Text}               稳定
rusqlite::OpenFlags::{SQLITE_OPEN_READ_WRITE, _READ_ONLY, _CREATE, _NO_MUTEX}   稳定
rusqlite::backup::Backup::new（crates/database/src/backup.rs）  backup feature 保留，稳定
prepare_cached ×2（crates/plugin-runtime/src/ingest.rs:407,458）→ §1.3 补 cache feature 后稳定
column_names()（Statement::column_names）  0.41 起返回 impl DoubleEndedIterator<Item=&str>（不再是 &[&str]，vendor/rusqlite/src/column.rs:80）；命中 2 处批内最小适配：crates/mcp-server/src/resources.rs:529（.iter().enumerate()→.enumerate()、(*col)→col）、crates/rp-mcp-parity/tests/parity.rs:423（.iter().map→.map）
```

### 2.3 0.32 → 0.41 breaking 变化逐项对照（rusqlite release notes 实测）

| 版本 | breaking 项 | 本仓命中？ |
|---|---|---|
| 0.33.0 | 移除 `release_memory` feature；MSRV 提升 | grep 实测零处使用 `release_memory` ✅ |
| 0.35.0 | `Connection::execute` 拒绝带尾语句的 SQL；`prepare` 拒绝多语句 | 迁移执行全部走 `execute_batch`（`crates/database/src/migrate.rs:126`，多语句合法入口）✅；业务 `execute(` 约 224 处，grep 未发现显式多语句/尾语句样本，由测试面（§3）兜底判定 |
| 0.38.0 | `u64/usize` 的 ToSql/FromSql 默认关闭（需 `fallible_uint` 类 feature）；statement cache 改为 feature `cache`（默认开启）；注册 Closure hook 要求 Connection 所有权 | ToSql 侧：grep 实测 `params![`/`params_from_iter` 绑定参数位含 `u64/usize` 为 **0 处**；FromSql 读出侧命中 `crates/database/src/retention.rs:299`、`:324` 两处 `get::<u64>`（COUNT(*) → rows_before/rows_after），需 `fallible_uint` 恢复 0.32 语义（§1.3 已补）✅；prepare_cached 见 §1.3 ✅；全仓无 Closure hook 使用 ✅ |
| 0.40.0 | VTab 宏改为构造器 / best_index / connect 签名修正 | 全仓 grep 实测零 vtab 使用 ✅ |
| 0.41.0 | minimal SQLite 3.45.3；MSRV 1.88/1.89；bundled 3.54.0 | bundled 随新链 ✅；迁移 SQL 均为现代 SQLite 可执行语法（历史迁移以既有 DB 文件回放验证，§3） |

bundled SQLite 0.32→0.41 跨度：3.46.x → 3.54.1。迁移与查询面为常规 SQL（建表/索引/CRUD），
跨大版本 SQLite 兼容风险低；备份/恢复（backup.rs Online Backup API）、retention（WAL 检查点）
在 §3 测试面覆盖。

### 2.4 Cargo.lock / vendor 增量清单

- 版本更新：`rusqlite 0.32.1 → 0.41.1`、`libsqlite3-sys 0.30.1 → 0.39.1`。
- 版本新增：`hashlink 0.12.x`（rusqlite 0.41.1 `cache` feature 要求 ^0.12；现存 0.9.1 由
  其它消费方保留或被 cargo 归并，以 bump 后 `Cargo.lock` 实际为准）。
- 无需变动（版本已满足 requirement，实测 lock）：`cc 1.11.2`（≥1.2.27）、`bitflags 2.13.2`、
  `smallvec 1.16.5`、`fallible-iterator 0.3.0`、`fallible-streaming-iterator`、`pkg-config`、`vcpkg`。
- 预期 vendor 目录增量 ≈ 3 个（rusqlite、libsqlite3-sys、hashlink-0.12.x），前提按 §1.3
  关掉 default 避免引入 `sqlite-wasm-rs` 链。

---

## 3. 测试面

按序执行，全部在仓库根：

1. **database crate 全套**：`cargo test -p database --offline` —— 连接池（pool.rs）、
   迁移执行器（migrate.rs 全套迁移回放 + 幂等）、备份恢复（backup.rs Online Backup API，
   对 bundled SQLite 大版本跨度最敏感）、retention。
2. **workspace check**：`cargo check --workspace --all-targets --offline` —— 16+ 经
   `database::rusqlite` 消费的 crate 编译面一次过（含 tests/ rp-eri-parity）。
3. **三个重度 sqlite 消费 crate 定向测试**（grep 实测 `rusqlite::` 源文件数最多的三个）：
   - `cargo test -p agent-engine --offline`（11 文件，store/trace/memory 全套）
   - `cargo test -p workflow-engine --offline`（10 文件，store/state/control/exec + 迁移回放）
   - `cargo test -p tool-engine --offline`（9 文件，含 document_fetch 与 e2e）
   - 次重度面（mcp-server、plugin-runtime、search-engine、context-broker、wsw-message、
     wsw-runtime、runtime-gateway）由 workspace check + 步骤 4 兜底；若任一失败升级为定向测试。
4. **CI 全量兜底**：推送后 CI 全量跑（含 Clippy 与既有 parity 测试），以 CI 绿为最终判据之一。

---

## 4. vendor 增量口径（严格按序）

本仓 `.cargo/config.toml` 指向 vendored-sources，bump 必须配套 vendor 重写。既定口径：

1. **移开 config 开联网窗口**：`mv .cargo/config.toml .cargo/config.toml.sec0-bak`
   （窗口期内禁止执行任何非 vendor 目的的 cargo 命令，防止锁外下载物混入）。
2. **改钉版**：workspace 根 `Cargo.toml:42` 按 §1.1/§1.3 改行；`cargo update -p rusqlite`
   （或直接 `cargo check` 触发解析）生成新 `Cargo.lock`。核对 lock 中
   `rusqlite 0.41.1`、`libsqlite3-sys 0.39.1`、`hashlink 0.12.x` 与 §2.4 清单一致，
   且**无** sqlite-wasm-rs 条目。
3. **全量重写 vendor**：`cargo vendor vendor`（网络窗口内，重生成全部 vendor/ 目录；
   覆盖旧目录，不是追加）。
4. **清嵌套 .gitignore**：`find vendor -name '.gitignore' -delete`
   （cargo vendor 在每个 vendor/<crate>/ 写入 .gitignore；本仓口径不入库——
   实测当前 vendor/ 中为 0 个 .gitignore，即历史口径已清理，重写后必须重清）。
5. **checksum 三方核对**：
   - `Cargo.lock` 各更新包 `checksum` 字段
   - == §1.4 预记录 tarball sha256（`sha256sum` 对重新下载物计算）
   - == `cargo vendor` 产物 `vendor/<crate>/.cargo-checksum.json` 的 `package` 字段
   三方逐一相等；hashlink-0.12.x 的 checksum 以 crates.io API `versions[].checksum` 为准核对。
6. **恢复 config**：`mv .cargo/config.toml.sec0-bak .cargo/config.toml`。
7. **--offline 构建验证**：
   - `cargo check --workspace --all-targets --offline`
   - `cargo test -p database --offline`（§3 步骤 1 在 offline 模式重跑）
   - `cargo deny check licenses --offline`（vendor 变更后许可证面复核，新增 hashlink 0.12.x
     许可证 MIT/Apache 系应落在既有 allow 清单内，若报错按 licenses 流程补录并记录批次报告）
   - `cargo deny check advisories`（§5 验收）

---

## 5. deny.toml [advisories] 段引入 + 验收判据

### 5.1 deny.toml 新增段（语法已在本批实测验证）

在 `deny.toml` 末尾追加（完整理由注释版见裁决书 §4）：

```toml
[advisories]
version = 2
ignore = [
  "RUSTSEC-2026-0543", # chrono 0.4.45 unmaintained：纯传递依赖（rmcp 3.10.0），workspace 源码零直用；替代=跨 rmcp 生态大改，不属本批。复查 2027-05-20（docs/phase0/SEC-0-ADVISORIES-DECISION.md §1）
  "RUSTSEC-2024-0436", # paste 1.0.15 unmaintained：纯传递依赖（tokenizers 0.21.4），workspace 源码零直用。复查 2027-05-20（同上 §2）
  "RUSTSEC-2024-0449", # unsafe-libyaml 0.2.11 unmaintained：经 serde_yaml_ng 0.10.0（直接依赖）；实测 serde_yaml_ng 全版本仍挂 unsafe-libyaml，替换=跨 5 crate YAML 语义回归，独立批次候选。复查 2027-05-20（同上 §3）
]
```

- **RUSTSEC-2026-0459 不进 ignore**：vulnerability 类且有修复版本，必须走 §1 bump 消除。
- 本批已用临时追加 + 复原方式实测该段语法：cargo-deny 0.20.2 接受 `version = 2` + ignore
  三条，运行后 3 条 unmaintained error 全部消失，仅剩 libsqlite3-sys vulnerability error 与
  index-failure warning——证明 ignore 清单生效、剩余失败即本批 bump 的验收对象。

### 5.2 验收判据（全部满足才算完成）

1. `cargo deny check advisories` **exit 0**（ignore 清单生效 + libsqlite3-sys 过修复线后）。
   预期零 error；index-failure warning 允许存在但登记复查项（裁决书 §5）。
2. §3 测试面全绿（database 全套、workspace check、三 crate 定向、CI 全量）。
3. §4 vendor 流程七步完成，`--offline` 构建与测试通过，三方 checksum 核对记录进批次报告。
4. `cargo deny check licenses` 不回归（exit 0，若因 hashlink 0.12.x 需补录按既有流程记录）。
5. `git status` 面：deny.toml、Cargo.toml（rusqlite 行 + 上方理由注释，HEAD 基线 :42 →
   bump 后 :50）、Cargo.lock、vendor/（±3 目录）、crates/mcp-server/src/resources.rs 与
   crates/rp-mcp-parity/tests/parity.rs（§2.2 `column_names()` 两处批内最小适配，
   批次报告须登记为批内适配面）、新增本任务书与裁决书；无其它漂移。

### 5.3 回退

单 commit 交付；回退 = `git revert`（Cargo.lock/vendor/deny.toml/Cargo.toml 一体回滚）。
`.cargo/config.toml` 移开是临时态，任何中断先恢复该文件再谈其它。

---

## 6. 附：公告核实记录（本批执行）

- RUSTSEC-2026-0459：https://rustsec.org/advisories/RUSTSEC-2026-0459.html（fetch 成功）
  —— affected libsqlite3-sys 0.6.0–0.38.2（bundles SQLite 3.13.0–3.53.2）；patched ≥0.39.0
  （0.39.1 = 3.54.1）；rusqlite 0.41.0 关联；MSRV 1.88/1.89；CVSS 6.8。
- crates.io API（curl https://crates.io/api/v1/crates/rusqlite[/.../dependencies]）：
  rusqlite 0.41.0/0.41.1 → `libsqlite3-sys ^0.39.0`；0.39.0 → ^0.37.0；0.42.0/0.43.0 → ^0.40.0。
- tarball 实测：libsqlite3-sys-0.39.1 `sqlite3/sqlite3.h:149 SQLITE_VERSION "3.54.1"`；
  0.39.0 同表 `3.54.0`；rusqlite-0.41.1 `rust-version = "1.89.0"`。
- cargo-deny 0.20.2 `[advisories] v2 ignore` 语法：本批临时追加实测（§5.1）。
- 复核（2026-11-21，裁决员会话）：rustsec.org RUSTSEC-2026-0459 页面重新 fetch 核实，affected
  0.6.0–0.38.2（bundles SQLite 3.13.0–3.53.2）、patched ≥0.39.0（0.39.0=3.54.0、0.39.1=3.54.1）、
  CVSS 6.8 MEDIUM（AV:L/AC:L/AT:N/PR:N/UI:P/VC:H）、CVE-2026-119375 / GHSA-72w2-rjgg-q729、
  关联 rusqlite 0.41.0、MSRV 1.88/1.89——与本节及 §1.1 全部一致。补一点公告细节：`bundled-sqlcipher`
  feature 在 0.39.0/0.39.1 仍捆绑 sqlcipher-src 419.0.6（SQLite 3.53.4）同样受影响；本仓 features
  钉 `bundled, backup, cache, fallible_uint`（Cargo.toml:50），未启用 sqlcipher 分支，不受该细节影响。
- 复核（2026-11-21，裁决员会话）：`cargo deny check advisories` 实跑 `advisories ok` exit 0
  （实施批次收口后终态确认）；probe-cargo 遗漏面登记见 §2.1.1。
