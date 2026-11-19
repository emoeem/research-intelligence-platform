# SEC-0 批次验收报告：rusqlite 0.32.1→0.41.1（带动 libsqlite3-sys 0.39.1 / bundled SQLite 3.54.1）+ cargo-deny `[advisories]` 段引入

- 日期：2026-11-20
- 状态：批次收口（设计 → 实施 → 评审 通过，评审 2 low 修订全落地；本地离线验收面全绿；CI 未跑未 watch，E2 待回填）
- 任务书：[SEC-0-TASK.md](SEC-0-TASK.md)（评审修订后版本，fallible_uint / column_names 两处回填在位）
- 裁决书：[SEC-0-ADVISORIES-DECISION.md](SEC-0-ADVISORIES-DECISION.md)（三条 unmaintained 逐条裁决 + index-failure 登记）
- 遗留出处：`docs/rip3/wsw/WSW-1-BATCH-REPORT.md` §4「安全扫描遗留（不得误报为全绿）」
- 结构说明：按 A-K 十一节标准模板（同 DI-1/RT-1 批报口径），坏消息单列于文首

## 坏消息（单列）

1. **CI 本批未跑、未 watch**：全部改动尚在工作树（未 commit、未 push）。本报告会话实测 `git log -1` HEAD = `origin/main` = `16401fcb`，`git status -sb` 首行 `## main...origin/main`（无 ahead/behind）——CI 从未见过本批任何字节。E2 节为占位口径，push 后 watch 回填。
2. **批内转述的 database 套件计数「rp-admin 11」本会话 fresh 复跑不复现**：`cargo test -p database --offline` 中 rp-admin bin unittests 实测 **0 passed**，`grep -c '#\[test\]' crates/database/src/bin/rp-admin.rs` 实测 **0**（该文件不在批内改动面，HEAD 同值）。fresh 权威口径 = 8 targets 合计 22 passed 0 failed（E.2 逐 target 回显），本报告一律以 fresh 实测记账。
3. **任务书影响面分析两处漏项（评审 low×2）**：① §2.3 grep 只覆盖 ToSql 绑定位、漏 FromSql 读出位（retention.rs COUNT(*)→u64，需 `fallible_uint`）；② `column_names()` 返回类型变化未入对照表。两处漏项实施侧均已正确补偿并留痕，任务书已回填（SEC-0-TASK.md:75、:87-93、:132、:141、:238-241 本会话 Read 实测在位），属文档回填项不属实施缺陷。
4. **hashlink-0.12.2 tarball 直连核对降级**（批内 deviation）：static.crates.io 直连 curl 超时（exit 28）→ 按任务书 §4.5 指定改用 crates.io API checksum 完成。本报告会话复核口径 = Cargo.lock checksum == vendor `.cargo-checksum.json` package == 任务书 §1.4 预记录，三值逐一相等（E.4；tarball sha256 直测未重做，如实标注）。
5. **vendor 目录数为净减（375→374），方向与任务书 §2.4 预估「≈+3」相反**：hashbrown-0.14.5 随 hashlink 0.9.1 淘汰整个移除（多版本 disambiguation 目录消失）；registry 增量仍恰为升级链内 3 包、零新增包（C 节逐包清单）。
6. **workspace 全量回归（154 套件级）非本批复跑面**：本批验收面 = deny×2 + workspace check + database 定向套件（本会话 fresh）+ 三 crate 定向（批内记录）；CI 全量兜底待 push（同 DI-1 报告口径，如实）。

## A. 完成

**交付面**（任务书 §4 七步 vendor 流程 + §5 deny 段引入 + 评审 2 low 修订，全部落地）：

| 交付物 | 内容 | 判据 |
|---|---|---|
| workspace 根 `Cargo.toml` bump 行（:50，含 :42-49 理由注释） | `rusqlite = { version = "0.41.1", default-features = false, features = ["bundled", "backup", "cache", "fallible_uint"] }`；git diff 实测 +10 行注释 -1 行旧钉版 | 任务书 §1.3 |
| `Cargo.lock` | rusqlite 0.41.1 / libsqlite3-sys 0.39.1 / hashlink 0.12.2；hashbrown-0.14.5 块移除；sqlite-wasm-rs 实测 absent（python 逐包抽取，本会话） | 任务书 §2.4 |
| `deny.toml` `[advisories]` 段（:39-49，+12 行，`git diff` 实测零其它改动） | version = 2 + 三条 ignore（RUSTSEC-2026-0543 / 2024-0436 / 2024-0449，理由注释 + 复查 2027-05-20）；RUSTSEC-2026-0459 不 ignore 注释在位；`[licenses]` 段零改动 | 裁决书 §4 |
| vendor 重写（目录级，C 节登记） | rusqlite 0.32.1→0.41.1 / libsqlite3-sys 0.30.1→0.39.1 / hashlink 0.9.1→0.12.2 三目录全量重写 + hashbrown-0.14.5 移除；vendor 目录 374（`ls vendor \| wc -l` 实测） | 任务书 §4 |
| 两处 API 最小适配（批内适配面，评审 low #2 登记） | `crates/mcp-server/src/resources.rs:528-529/:538` 与 `crates/rp-mcp-parity/tests/parity.rs:422-423`（column_names 迭代器化，git diff 实测各 2 hunk：注释 + 去 `.iter()`/解括号，无无关重构） | 任务书 §2.2 |
| 两份侦察批文档入库 | `docs/phase0/SEC-0-TASK.md`（修订后）+ `docs/phase0/SEC-0-ADVISORIES-DECISION.md`（porcelain ?? 实测） | 批次账目 |
| 本批次报告 | `docs/phase0/SEC-0-BATCH-REPORT.md`（本文件，落盘后计入 porcelain ?? 面） | 批次账目 |

**registry 增量包清单（逐包列名，`git diff Cargo.lock` + 本会话 python 逐包抽取实测）**：

| 包 | 变更 | checksum 变化（Cargo.lock diff 实测） |
|---|---|---|
| rusqlite | 0.32.1 → **0.41.1** | `7753b721…` → `da9bba6f…` |
| libsqlite3-sys | 0.30.1 → **0.39.1** | `2e99fb7a…` → `14a5f01b…` |
| hashlink | 0.9.1 → **0.12.2** | `6ba4ff71…` → `a596f1b2…` |
| hashbrown 0.14.5 | **移除**（随 hashlink 0.9.1 淘汰） | `e5274423…` 块删除 |
| hashbrown 0.17.1 | 零改动（既有包，vendor/hashbrown tracked 50 文件 HEAD 即 0.17.1，不在 porcelain）；lock 内依赖引用由 `"hashbrown 0.17.1"` 归并为 `"hashbrown"`（单版本化） | 不变 |
| sqlite-wasm-rs | **absent**（default-features=false 生效） | — |

registry 计数：`grep -c '^source = ' Cargo.lock` 实测 **374**（DI-1 收口口径 375，净 −1 = hashbrown-0.14.5）；`[[package]]` 399。零新增包，全部变更在 RUSTSEC-2026-0459 修复升级链内。

**本会话 fresh 复跑门禁（2026-11-20，仓库根，全部实跑实贴）**：

```
$ cargo deny check advisories        → advisories ok   exit 0（零 error 零 warning，两轮复跑一致）
$ cargo deny check licenses          → licenses ok     exit 0
$ cargo check --workspace --all-targets --offline → Finished，exit 0
$ cargo test -p database --offline   → 8 targets 22 passed 0 failed，exit 0（E.2 逐 target）
```

批内记录（实施会话，本报告会话未复跑、如实标注）：`cargo test -p agent-engine --offline` 45 passed、`cargo test -p workflow-engine --offline` 68 passed、`cargo test -p tool-engine --offline` 126 passed；vendor 七步流程（含联网窗口纪律、三方 checksum、374/374 checksum 文件零缺失）；`--offline` 三轮构建收敛。

## B. 架构与复用（单点钉版 × 重导出模式 × 既有 vendor/deny 流程复用）

- **唯一 sqlite 入口单点化**（任务书 §2.1，本会话复核）：全 workspace 仅 `crates/database/Cargo.toml:11` 声明 rusqlite（`workspace = true`），`crates/database/src/lib.rs:31` `pub use rusqlite;` 重导出，全部消费 crate 走 `use database::{rusqlite, Pool}`。因此 bump 落点 = workspace 根一行 + lock + vendor，消费面 API 语义零变化（两处签名适配见 D 节）。本批零新增 crate（lock `[[package]]` 399 全部为既有 registry 面或 workspace 成员）。
- **feature 钉版语义等价策略**：default-features=false 关掉 0.41 默认 `ffi-sqlite-wasm-rs`（wasm 后端，本仓无 wasm target；lock 实测 sqlite-wasm-rs absent），`cache` 显式补回（`crates/plugin-runtime/src/ingest.rs:407/:458` 两处 `prepare_cached` 实测在位），`fallible_uint` 恢复 0.38 起默认关闭的 u64/usize FromSql 读出侧（`vendor/rusqlite/src/types/from_sql.rs:143-146` `#[cfg(feature = "fallible_uint")]` 实测）——消费代码零改动，语义与 0.32 相同（`> i64::MAX` fallible）。
- **复用既有流程，不建新机制**：vendor 增量走任务书 §4 七步既定口径（联网窗口纪律：`.cargo/config.toml` 移开复原，porcelain 实测 `.cargo/` 零条目 = byte-identical）；许可证面复用 deny.toml `[licenses]` 既有 allow 清单，hashlink 0.12.2 `license = "MIT OR Apache-2.0"`（vendor/hashlink/Cargo.toml 实测）落入清单，manifest 零补录（`git diff deny.toml` 实测仅 +12 行 [advisories] 段）。
- **红线不碰**：database crate 结构零 diff（porcelain 无 crates/database 条目）；无迁移面、无 SQL 改动、无插件/工具登记面变化——本批是纯依赖面批次，D 节逐项断言。

## C. 文件清单（`git status --porcelain=v1 -uall` 实测逐文件，2026-11-20，本文件落盘前 163 项 / 落盘后 164 项）

**非 vendor（逐文件）**：

```
 M Cargo.lock                                   # ±27 行：rusqlite/libsqlite3-sys/hashlink 三块 + hashbrown-0.14.5 块移除 + 依赖引用单版本化
 M Cargo.toml                                   # +10 行注释 -1 行：rusqlite 钉版行（:50）
 M crates/mcp-server/src/resources.rs           # column_names() 迭代器化适配（2 hunk，批内适配面）
 M crates/rp-mcp-parity/tests/parity.rs         # column_names() 迭代器化适配（2 hunk，批内适配面）
 M deny.toml                                    # +12 行：[advisories] 段（[licenses] 零改动）
?? docs/phase0/SEC-0-ADVISORIES-DECISION.md     # 侦察批产出入库
?? docs/phase0/SEC-0-TASK.md                    # 侦察批产出入库（评审修订后）
?? docs/phase0/SEC-0-BATCH-REPORT.md            # 本文件（落盘后计入）
```

**vendor（按目录级登记，合计 156 条 porcelain = 93 M + 50 D + 13 ??；`git status --porcelain -- <dir> | wc -l` 逐目录实测）**：

| 目录 | 条目 | 内容 |
|---|---|---|
| `vendor/rusqlite` | 70（63 M + 7 新增） | 0.32.1→0.41.1 全量重写；新增 `bindings.md`、`examples/owning_rows.rs`、`examples/owning_statement.rs`、`src/bind.rs`、`src/types/jiff.rs`、`src/wal.rs`、`types.md` |
| `vendor/libsqlite3-sys` | 26（15 M + 8 D + 3 新增） | 0.30.1→0.39.1 全量重写；bundled SQLite 3.54.1（`sqlite3/sqlite3.h:149` 实测）；删 `bindgen-bindings/bindgen_3.14.0{,_ext}.rs`、`sqlcipher/{LICENSE,sqlite3.c,sqlite3.h,sqlite3ext.h}`、`upgrade{,_sqlcipher}.sh`；新增 `bindgen-bindings/bindgen_3.45.3{,_ext}.rs`、`Cargo.lock` |
| `vendor/hashlink` | 19（15 M + 1 D + 3 新增） | 0.9.1→0.12.2 全量重写；删 `.circleci/config.yml`；新增 `.github/workflows/ci.yml`、`Cargo.lock`、`deny.toml` |
| `vendor/hashbrown-0.14.5` | 41 D | 整目录移除（随 hashlink 0.9.1 淘汰；`find vendor -name .gitignore -delete` 后同批口径无残留 .gitignore） |
| `vendor/hashbrown`（0.17.1） | **0 条** | 既有 tracked 目录（50 文件，HEAD 即 0.17.1）本批零 diff——与 hashbrown-0.14.5 是 cargo vendor 多版本 disambiguation 命名并存的历史形态，升级后仅剩单版本 |

**porcelain 面 vs 任务书 §5.2-5 预期**（修订后版本，:238-241 实测）：恰好 = deny.toml + Cargo.toml（rusqlite 行 + 上方理由注释）+ Cargo.lock + vendor 四目录 + 两适配源文件 + 三份 phase0 文档；无其它漂移（`.cargo/config.toml`、`integrations/rtis/`、`migrations/`、`manifests/`、`schemas/` porcelain 均零条目实测）。

## D. 迁移与兼容

**零迁移，逐项实测**：

- `git status --porcelain -- migrations/` = 0 行：无任何 DB schema/迁移面改动，无新迁移文件，`schema_version`/`PRAGMA user_version` 链不动；无平台库写入（红线，`migrations/platform*.db` mtime 实测 2026-11-06/07，早于批次日）。
- 全仓代码/测试 grep 实测零 SQLite 版本钉值（`grep -rn '3\.46\|3\.54' crates/ tests/ apps/ --include='*.rs'` 零命中）：升级测试全走内存库/副本，零测试值改动。

**rusqlite 版本面升级兼容断言（本会话逐项取证）**：

1. **u64/usize FromSql 读出侧**（0.38 起默认关闭）：命中位 = `crates/database/src/retention.rs:299`/`:324`（`rows_before`/`rows_after: u64` 经 `query_row("SELECT COUNT(*) …", |r| r.get(0))` 读出，sed 实测）。消除方式 = workspace 钉版补 `fallible_uint` feature（Cargo.toml:50），**零代码改动**；语义核实 = vendored `vendor/rusqlite/src/types/from_sql.rs:143-146` `#[cfg(feature = "fallible_uint")] from_sql_integral!(u64/usize)`，feature 表 `vendor/rusqlite/Cargo.toml:112` `fallible_uint = []`（默认不含，故必须显式补），`> i64::MAX` fallible 与 0.32 相同。
2. **`column_names()` 返回类型**：0.41 起返回 `impl DoubleEndedIterator<Item = &str> + ExactSizeIterator`（不再是 `&[&str]`；`vendor/rusqlite/src/column.rs:80` 实测）。两处最小适配（git diff 实测各 2 hunk：1 行注释 + 去 `.iter()`/去解括号）：`crates/mcp-server/src/resources.rs:528-529`（`.iter().enumerate()`→`.enumerate()`、`(*col)`→`col`）、`crates/rp-mcp-parity/tests/parity.rs:422-423`（`.iter().map`→`.map`）。
3. **`prepare_cached` 语句缓存**（0.38 起改 feature `cache`）：消费位 `crates/plugin-runtime/src/ingest.rs:407`、`:458` 两处实测在位；default-features=false 失去默认 cache → 显式补回。
4. **`execute` 多语句限制**（0.35.0）：迁移执行全部走 `execute_batch`（`crates/database/src/migrate.rs:126`，任务书 §2.3 实测口径），多语句合法入口不受影响。
5. **bundled SQLite 大版本跨度 3.46.x→3.54.1**：`vendor/libsqlite3-sys/sqlite3/sqlite3.h:149` `#define SQLITE_VERSION "3.54.1"`（本会话 sed 实测，公告修复线 3.54.0 之上）；备份恢复（Online Backup API）、四套迁移回放、retention/WAL 检查点测试全绿（E.2）承载兼容判定。
6. **MSRV**：rusqlite 0.41.1 = 1.89.0、libsqlite3-sys 0.39 = 1.88、hashlink 0.12.2 = 1.85（vendor Cargo.toml 实测）；workspace `rust-version = "1.99"`（Cargo.toml:34）与本机 rustc 均满足。CI 独立 MSRV job 工具链版本核对留 E2 watch 项。

**database crate 零 diff**：porcelain 无 crates/database 条目——升级的全部代码代价 = 两个消费 crate 各 1 处签名适配，database 结构零重构。

## E. 测试真实结果

### E.1 deny advisories/licenses 前后对照

| 时点 | 配置 | lock | 结果 | 出处 |
|---|---|---|---|---|
| 前（批内基线复跑记录） | deny.toml 无 `[advisories]` 段 | 旧（libsqlite3-sys 0.30.1） | exit 1：4 error（RUSTSEC-2026-0459 vulnerability + chrono/paste/unsafe-libyaml unmaintained）+ 1 warning（index-failure） | 任务书 §0 转录，**批内会话执行，本报告会话未复跑（需回滚 lock，如实标注）** |
| 对照实验（本会话实跑） | HEAD deny.toml（无 `[advisories]` 段） | 新（0.39.1） | **exit 1：恰 3 error**（chrono/paste/unsafe-libyaml unmaintained），**RUSTSEC-2026-0459 vulnerability error 已消失、index-failure warning 已消失** | `git show HEAD:deny.toml > /tmp/deny-head.toml && cargo deny --config /tmp/deny-head.toml check advisories` |
| 后（本会话 fresh 两轮） | 批内 deny.toml（3 条 ignore） | 新（0.39.1） | **exit 0，`advisories ok`，零 error 零 warning** | `cargo deny check advisories` |
| licenses（本会话 fresh） | 既有 `[licenses]` 段（零改动） | 新 | **exit 0，`licenses ok`**；hashlink 0.12.2 = MIT OR Apache-2.0 落既有 allow 清单，manifest 零补录 | `cargo deny check licenses` |

对照实验回显（`cargo deny --version` = 0.20.2 实测）：

```
$ cargo deny --config /tmp/deny-head.toml check advisories   # HEAD deny.toml + 新 lock
error[unmaintained]: chrono is unmaintained            ├ RUSTSEC-2026-0543
error[unmaintained]: paste - no longer maintained      ├ RUSTSEC-2024-0436
error[unmaintained]: unsafe-libyaml is unmaintained    ├ RUSTSEC-2024-0449
advisories FAILED                                      （exit 1）
```

——证明两件事各自独立成立：① RUSTSEC-2026-0459 由 bump 单独消除（HEAD 配置下 vulnerability error 已不在）；② 三条 unmaintained 的消音来自 `[advisories] ignore` 三条而非配置缺省（无 ignore 时 exit 1）。index-failure warning 消失为环境面（advisory-db/registry index fetch 成功），与配置无关（HEAD 配置下同样无 warning）。

### E.2 database 定向套件（本会话 fresh，`cargo test -p database --offline`，exit 0）

```
Running unittests src/lib.rs (database)                        11 passed; 0 failed
Running unittests src/bin/rp-admin.rs                           0 passed; 0 failed   ← 批内转述"11"不复现，见坏消息 2
Running tests/backup_restore.rs                                 4 passed; 0 failed   （Online Backup API 对 3.54.1）
Running tests/migrations_agent_data_plane.rs                    1 passed; 0 failed
Running tests/migrations_context_broker.rs                      2 passed; 0 failed
Running tests/migrations_runtime_node.rs                        2 passed; 0 failed
Running tests/migrations_tool_contract.rs                       2 passed; 0 failed
Doc-tests database                                              0 passed; 0 failed
合计：8 targets，22 passed / 0 failed（备份恢复/迁移回放/retention 对 bundled SQLite 3.46→3.54.1 全绿）
```

### E.3 workspace check（本会话 fresh，exit 0）

```
$ cargo check --workspace --all-targets --offline
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.15s
```

16+ 经 `database::rusqlite` 消费的 crate 编译面一次过（增量缓存热跑，批内三轮冷收敛记录见 A 节批内记录）。

### E.4 checksum 三方核对（本会话复核）+ 批内记录（未复跑，如实标注）

- **本会话复核（python 实测逐值）**：Cargo.lock `checksum` == vendor `<crate>/.cargo-checksum.json` `package` == 任务书 §1.4 预记录，三包三值逐一相等：rusqlite `da9bba6f…594` / libsqlite3-sys `14a5f01b…b3` / hashlink `a596f1b2…0fb`。tarball sha256 直测为批内会话执行（rusqlite/libsqlite3-sys 两包 tarball 实测成功且与预记录一致；hashlink 直连超时改用 crates.io API checksum，坏消息 4），本报告会话未重新下载 tarball。
- **批内记录（本报告会话未复跑）**：`cargo test -p agent-engine --offline` 45 passed；`cargo test -p workflow-engine --offline` 68 passed；`cargo test -p tool-engine --offline` 126 passed；`cargo check --workspace --all-targets --offline` 冷启动三轮收敛；vendor `find vendor -name .gitignore` 残留 0、374/374 checksum 文件零缺失零多余。

## E2. CI（待 push watch 回填）

- **本批未 commit、未 push**（坏消息 1）：HEAD = `origin/main` = `16401fcb`，CI 从未见过本批任何字节；`.github/workflows/ci.yml` 在位（ls 实测）。
- **回填口径**：push 后单 commit（回退 = `git revert` 一体回滚 Cargo.toml/Cargo.lock/deny.toml/适配两文件/vendor 四目录，任务书 §5.3），CI 终态 + 是否暴露异机面（Linux runner 无系统 sqlite → bundled 路径正好自持，风险反低；CI rustc 版本对 MSRV 1.89 下限）随 watch 记账于本节。
- **预期绿面**：本地定向门（E 节）为 CI 的子集；CI 全量兜底（含三 crate 定向 + parity + clippy）为最终判据之一（任务书 §3 步骤 4 原口径）。

## F. 性能（升级面无吞吐测量，如实）

- **本批未建吞吐基线、未跑任何吞吐/延迟基准（如实，未建即写未建）**：本批改动面 = 编译期 bundled SQLite 替换（3.46.x→3.54.1）+ rusqlite API 签名适配，无 SQL 语义、连接池、事务行为改动；「升级面无吞吐变化」没有测量证据支撑，本报告不以其为断言，只以下列功能级证据承载兼容判定：database 8 targets 22 passed（含备份恢复/迁移回放，E.2）+ workspace check exit 0（E.3）。
- **相邻面区分（防冒认）**：autoresearch `assemble_ms` 面最新 keep 记录 1.863 vs 基线 2.399 属 context-broker tokenizer 计量链（仓库 .auto 记账），与本批 sqlite 升级面零交集，不作为本批性能证据。
- `cache` feature（`prepare_cached` 语句缓存）语义与 0.32 时代相同（ingest.rs 两处调用点零改动）；bundled SQLite 3.54 与 3.46 的查询计划差异未单独测量，如后续出现 DB 延迟面需求另开基准批次。

## G. 遗留与风险

1. **三条 ignore 复查日期统一 2027-05-20**（deny.toml:41-47 注释 + 裁决书，触发条件各自登记）：
   - RUSTSEC-2026-0543（chrono 0.4.45）：触发 = rmcp 发版去 chrono/换 jiff，或 chrono 出现实质漏洞公告（届时 unmaintained+漏洞组合不可 ignore）；动作 = 重跑 `cargo tree -i chrono`，解则删 ignore 行并确认 advisories 仍 exit 0（裁决书 §1.3）。
   - RUSTSEC-2024-0436（paste 1.0.15）：触发 = tokenizers 升级批次落地时顺带核对是否去 paste（裁决书 §2.3）。
   - RUSTSEC-2024-0449（unsafe-libyaml 0.2.11）：**替换候选标记**（三条中唯一低成本路径，API 面 10 文件/20 行）；触发 = 新增 YAML 解析消费点、unsafe-libyaml 实体漏洞公告、或 serde_yaml_ng 发布去 unsafe-libyaml 版本（届时低成本 bump 解除）；替换 = serde-saphyr 系 + 跨 5 crate 解析语义回归，独立批次候选（裁决书 §3.3）。
2. **index-failure 复查项：实质已提前闭环**——裁决书 §5 的「index 恢复后复跑核验」在本批终跑 + 本会话 fresh 两轮中均零 warning（yanked 核验完成，email_address 0.2.10 / filetime 0.2.30 零命中）。保留下次依赖安全批次（SEC-1 或等价）开场复核一句（advisories 是否仍零 warning），不再作为独立待办。
3. **CI 未跑未 push**（坏消息 1 / E2）：push 后 watch 回填为批次最终判据缺口；MSRV job 核对（D.6）随 watch。
4. **rcmcp/rmcp 生态 chrono 切换**为仓外演进观察项，随 #1 复查合并执行。
5. **`[advisories]` 段是本批新增的常驻门**：后续任何依赖面批次（含 H 节三批若新增依赖）在 vendor 七步外必须过 `cargo deny check advisories/licenses` 双门；ignore 清单增长须走裁决书流程（理由 + 替代评估 + 复查日期），不允许静默加行。

## H. 下一批依赖

三批与 SEC-0 零前置耦合（SEC-0 是纯依赖面批次，无 API/登记面产物供下游消费）；共享写面只有 Cargo.lock / vendor/——三批任一若新增依赖，必须复用本批七步 vendor 流程 + E.1 双 deny 门（G.5），并行时注意 lock/vendor 冲突面。

- **RT-2（Python Worker 原生环境）**：roadmap 定义 =「项目级 venv、依赖锁、启动探测、版本/哈希记录、协议一致性测试」（PHASE-ROADMAP.md:786 实测）。Python 侧依赖锁面与 Rust vendor 流程互不触达；SEC-0 对 python_worker 零触碰（porcelain 无该文件）。
- **DI-2（Office/OCR 链路，能力面扩全）**：定义落点 = DI-0 ADR:93「DI-2 Office/OCR 链路」+ DI-1-TASK.md:21/:97/:165（消费 pdf.pdftoppm / ocr.tesseract 与 Router 通道表扩展缝）。纯 Rust 面新增 crate/Manifest，若新增第三方依赖同样走 vendor 流程；DI-1 报告 H 节已备好扩展缝，与 SEC-0 无共享写面冲突。
- **WSW-3（HTTP History + Repeater）**：定义 = WEB-SECURITY-WORKBENCH-PLAN.md:605（History 列表详情、过滤搜索、请求编辑、重放、Tab 历史）；承接缝 = WSW-2-ADR D4（`wsw_exchange` 元数据表留 WSW-3）与 D9（audit-engine Emit 接线留 WSW-3）。新增依赖概率高（导出/解析面），须按本批口径过 deny 双门；unsanitized 请求体回放面沿用 wsw_message 脱敏边界（WSW-2-ADR D9）。

## I. 浏览器截图

**本批无 UI 面，零截图（如实）**：改动面 = manifest/lock/deny + vendor + 两处后端源文件（C 节实测清单），无任何 web/UI/前端条目（porcelain 无 web/ 下命中）；无浏览器 e2e 用例故无截图可附。

## J. RTIS 对账

**零接触，实测**：

- `git status --porcelain -- integrations/rtis/ migrations/ scripts/rtis_kb_worker.py .cargo/` = **0 行**（本会话实测）；全仓 porcelain 无任何 `rtis` 命中（红线达成）。
- mtime 实测全部早于批次日 2026-11-20：`integrations/rtis/knowledge.db` = 2026-11-18 05:36、`integrations/rtis/rtis.py` = 2026-11-09 09:45（rtis_kb.mjs / rtis_mcp.mjs / rt.py 同批 11-09）、`migrations/platform.db` = 2026-11-07 00:53（platform-dryrun/fullbatch/governance/search 同域 11-06/07）、`scripts/rtis_kb_worker.py` = 2026-11-09 21:55。
- 本批不触知识摄取/检索链路；RTIS 旧链条数对账**未复跑**（无触碰面故无需，同 DI-1 §J 口径），本报告不以其为门。

## K. 教训

1. **影响面 grep 必须双向（ToSql 绑定位 + FromSql 读出位）**：任务书 §2.3 的「u64/usize 0 处」只 grep 了 `params![`/`params_from_iter` 绑定位，漏了 `get::<u64>` 读出位（retention.rs COUNT(*)）——0.38 起 FromSql u64 默认关闭直接编译断。类型参数的 grep 要覆盖 trait 的两侧实现，不只是调用宏。
2. **签名返回类型变化要按 API 全表对照 release notes，不能只按使用点 grep**：`column_names()` 从 `&[&str]` 改 iterator，grep `column_names` 能找到使用点，但「预期 git status 面」若不把它登记进去，验收自检口径就会漏项（评审 low #2 即此根因）。任务书漏项以「回填任务书 + 批报登记批内适配面」双落点收口（SEC-0-TASK.md:132/:238-241 已回填）。
3. **default-features=false 不是免费的**：0.41 的 default 带着两个隐式功能（cache 语句缓存、ffi-sqlite-wasm-rs），关 default 换 vendor 最小化的同时必须逐项显式回补消费面（cache）与语义面（fallible_uint），并把理由写进钉版行注释——否则下次升级者按旧文档去掉 feature 就编译断。
4. **cargo vendor 多版本命名形态要预先认识**：同名包多版本时 vendor 出 `hashbrown`（0.17.1）与 `hashbrown-0.14.5` 两个目录；升级收敛到单版本后 suffix 目录整体消失，vendor 目录数是**净减**不是净增——升级前用 `cargo tree -i <crate>` 摸清同名包全消费面，预估口径才不反向。
5. **cargo-deny CLI 形态先验证再写判据**：`check` 子命令不接受 `--offline`（0.20.2 实测 error: unexpected argument）；任务书判据里的命令要先在本机跑通一遍再落笔，licenses 检查本身纯本地读 lock+vendor，等价可跑。
6. **网络瞬态与「不可达 ≠ 已核验」的闭环要跟踪到消失**：advisory-db github TLS 偶发失败重跑即恢复；index-failure warning 随 fetch 成功自然消失——登记类复查项的目标状态是「warning 消失 + 核验零命中」，不是「命令通过」；本批把该登记项提前闭环并保留开场复核一句，是登记项的正确收尾形态。
7. **checksum 三方核对要有降级路径并如实记账**：tarball 直连可超时（hashlink exit 28），crates.io API checksum 是任务书预先指定的合规替代源；降级发生时在批报单列（坏消息 4），不冒充三方全测。

---

## 复现

```bash
cd /media/emo/code/pensting/research-platform
# deny 双门（预期：advisories ok / licenses ok，双 exit 0，零 error 零 warning）
cargo deny check advisories
cargo deny check licenses
# 对照实验：HEAD deny.toml（无 [advisories] 段）+ 新 lock（预期 exit 1 恰 3 unmaintained error，无 vulnerability）
git show HEAD:deny.toml > /tmp/deny-head.toml && cargo deny --config /tmp/deny-head.toml check advisories
# 编译与定向测试（预期 exit 0；database 8 targets 22 passed 0 failed）
cargo check --workspace --all-targets --offline
cargo test -p database --offline
# 静态门（预期：374 / 399 / 0 / 0；vendor 目录 374）
grep -c '^source = ' Cargo.lock
grep -c '^\[\[package\]\]' Cargo.lock
git status --porcelain -- integrations/rtis/ migrations/ scripts/rtis_kb_worker.py .cargo/ | wc -l
git status --porcelain -- vendor/ | wc -l
ls vendor | wc -l
# 修复线实证（预期 SQLITE_VERSION "3.54.1"）
sed -n '149p' vendor/libsqlite3-sys/sqlite3/sqlite3.h
# checksum 三方（预期三包 lock == vendor json == 任务书 §1.4）
python3 - <<'EOF'
import json, re
lock = open('Cargo.lock').read()
for name, ver, want in [('rusqlite','0.41.1','da9bba6f3506135e120a6586df05be18b48d18896600646a4bec8f993d91a594'),
                        ('libsqlite3-sys','0.39.1','14a5f01b87b92705d07b433c06f96cbc764637d81a0bffb5734ce17cf0f9a2b3'),
                        ('hashlink','0.12.2','a596f1b20ed2cc5ecac41a164aaebc7258057060f06c0cf7a2ba3991ee7990fb')]:
    m = re.search(r'\[\[package\]\]\nname = "%s"\nversion = "%s"\nsource = "[^"]*"\nchecksum = "([0-9a-f]+)"' % (name, ver), lock)
    v = json.load(open('vendor/%s/.cargo-checksum.json' % name))['package']
    assert m.group(1) == v == want, name
    print(name, 'three-way ok')
EOF
```
