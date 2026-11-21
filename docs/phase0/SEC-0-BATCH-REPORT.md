# SEC-0 批次验收报告（v2 复核收口版）：rusqlite 0.32.1→0.41.1（带动 libsqlite3-sys 0.39.1 / bundled SQLite 3.54.1）+ cargo-deny `[advisories]` 段引入

- 批次日期：2026-11-20（实施已入库：commit `76c6feb6`）；本报告：2026-11-21 复核收口会话撰写
- 状态：实施入库（76c6feb6）→ 评审通过（2 low 修订全部落地工作树）→ **CI 绿（run 44589236396 on 76c6feb6，v1 报告时点未知，已回填实测）** → 本会话（2026-11-21）fresh 全门复证通过
- 任务书：[SEC-0-TASK.md](SEC-0-TASK.md)（工作树修订版：§2.1.1 probe-cargo 登记 + §6 复核记录，+23/-0）
- 裁决书：[SEC-0-ADVISORIES-DECISION.md](SEC-0-ADVISORIES-DECISION.md)（工作树修订版：§3.1 行号锚定修准 + §3.2 fork 措辞按公告原文修准 + §7 复核记录，+32/-4）
- 遗留出处：`docs/rip3/wsw/WSW-1-BATCH-REPORT.md` §4「安全扫描遗留（不得误报为全绿）」
- v1→v2 变化说明：v1（随 76c6feb6 入库）写于实施未提交时点，其坏消息 1「CI 未跑未 push」已失效（HEAD=origin/main=061cfdef，`git log origin/main..HEAD` 实测空）；v2 以本会话实跑重写 C/E/E2 三节、坏消息单列新增 probe-cargo 登记面与「CI 无 deny 门」盲区，G 节复查日期改逐条行尾口径
- 结构说明：按 A-K 十一节标准模板（同 DI-1/RT-1/DI-2/WSW-3 批报口径），坏消息单列于文首

## 坏消息（单列）

1. **probe-cargo/（workspace 外）存在 RUSTSEC-2026-0459 受影响残留，且不被任何 advisories 门覆盖**：`probe-cargo/Cargo.toml:7` 独立钉 `rusqlite = { version = "0.32", features = ["bundled"] }`，其 `Cargo.lock` 实测 rusqlite 0.32.1 / libsqlite3-sys 0.30.1（受影响区间 0.6.0–0.38.2 内）；不在根 `Cargo.toml` members（:3-30 段 grep probe = 0 命中）、无 deny.toml、不在 `cargo deny check` 扫描面。定性 = DEC-001 一次性历史验证工件（PROBE-RESULT.md 留证后闲置），非生产面不发布；登记为遗留观察项（SEC-0-TASK.md §2.1.1，2026-11-21 补记，如实标注批次实施时 §2.1 只覆盖 workspace 面），不阻塞本批验收。
2. **CI 无 deny 步骤（依赖安全门是本地常驻门而非 CI 门）**：`.github/workflows/ci.yml` grep `deny` 零命中（本会话实测），CI 四 job = Rust workspace（测试与 clippy 零警告）/ Web 工作台 / 协议一致性载具 / Python worker 测试；工具链 `dtolnay/rust-toolchain@stable`（ci.yml:38/:71，无钉版）。deny advisories/licenses 双门的远端兜底不存在，双门绿面只存在于跑它的机器上——已登记 G.4 评估项。
3. **工作树修订面 4 文件未提交**：`deny.toml`（+5/-4，ignore 三条复查日期改逐条行尾）、`SEC-0-ADVISORIES-DECISION.md`（+32/-4）、`SEC-0-TASK.md`（+23/-0）、本报告（v2 重写）。本 ask 无 commit 步骤；评审修订与报告收口建议单独 commit（revert 面仅 deny.toml + 三份 docs）。
4. **批内转述的 database 套件计数「rp-admin 11」fresh 不复现（保留项，本会话再证）**：`grep -c '#\[test\]' crates/database/src/bin/rp-admin.rs` 实测 **0**；fresh 权威口径 = 8 targets 合计 22 passed 0 failed（E.2 逐 target）。
5. **任务书影响面分析两处漏项（评审 low×2，历史项已收口）**：§2.3 只 grep ToSql 绑定位漏 FromSql 读出位（retention.rs COUNT(*)→u64）、`column_names()` 返回类型变化未入对照表；实施侧均已正确补偿，任务书已回填。
6. **hashlink tarball 直连核对降级（历史 deviation）**：static.crates.io curl 超时 exit 28 → 按任务书 §4.5 改用 crates.io API checksum。本会话复核口径 = Cargo.lock == vendor `.cargo-checksum.json` == 任务书 §1.4 三方逐值相等（E.4），tarball sha256 直测未重做（如实标注）。
7. **vendor 目录数净减（375→374），方向与任务书 §2.4 预估「≈+3」相反**：hashbrown-0.14.5 随 hashlink 0.9.1 淘汰整目录移除；registry 增量仍恰为升级链内 3 包、零新增包（A 节逐包清单）。
8. **批内三 crate 定向套件本会话未复跑（如实标注）**：`agent-engine` 45 / `workflow-engine` 68 / `tool-engine` 126 passed 为实施会话记录；本会话以 workspace check（E.3）+ CI 44589236396「Rust workspace（测试与 clippy 零警告）」success（E2）承载全仓面。
9. **cargo-deny CLI 形参差异（历史 deviation）**：`cargo deny check licenses --offline` 不被识别（0.20.2 实测 error: unexpected argument exit 2）；改跑无旗标形态（vendored-sources 下本就离线）得 licenses ok。任务书该行建议随下批修订修准。

## A. 完成

**交付面**（实施 commit `76c6feb6`，2026-11-20 入库；`git show --stat` 实测 164 files changed，+58717/−335418）：

| 交付物 | 内容 | 判据 |
|---|---|---|
| workspace 根 `Cargo.toml` bump 行（:50，:42-49 理由注释 8 行，本会话 sed 实测全文） | `rusqlite = { version = "0.41.1", default-features = false, features = ["bundled", "backup", "cache", "fallible_uint"] }` | 任务书 §1.3 |
| `Cargo.lock` | rusqlite 0.41.1 / libsqlite3-sys 0.39.1 / hashlink 0.12.2；hashbrown-0.14.5 块移除；sqlite-wasm-rs absent | 任务书 §2.4 |
| `deny.toml` `[advisories]` 段（+12 行，`[licenses]` 零改动） | version = 2 + 三条 ignore（0543/0436/0449）+ RUSTSEC-2026-0459 不 ignore 注释（deny.toml:49）；评审修订后复查日期在三条行尾（:45-47） | 裁决书 §4 |
| vendor 四目录全量重写 | rusqlite 70 / libsqlite3-sys 26 / hashlink 19 / hashbrown-0.14.5 41 D（`git show --name-status --format= 76c6feb6 -- vendor/<dir> \| wc -l` 本会话逐目录实测，合计 156） | 任务书 §4 |
| 两处 API 最小适配 | `crates/mcp-server/src/resources.rs:528-529/:538` 与 `crates/rp-mcp-parity/tests/parity.rs:422-423`（column_names 迭代器化，本会话 sed 实测注释+改写行在位）；database crate 零 diff（`git show --stat 76c6feb6 -- crates/database/` = 0 行） | 任务书 §2.2 |
| 三份 phase0 文档入库 + v2 收口 | SEC-0-TASK / SEC-0-ADVISORIES-DECISION（随 76c6feb6 入库，2026-11-21 评审修订在工作树）+ 本报告 | 批次账目 |

**registry 增量包清单（逐包列名；`git diff Cargo.lock` + 本会话 python 逐包抽取实测）**：

| 包 | 变更 | checksum 变化 |
|---|---|---|
| rusqlite | 0.32.1 → **0.41.1** | `7753b721…` → `da9bba6f3506135e120a6586df05be18b48d18896600646a4bec8f993d91a594` |
| libsqlite3-sys | 0.30.1 → **0.39.1** | `2e99fb7a…` → `14a5f01b87b92705d07b433c06f96cbc764637d81a0bffb5734ce17cf0f9a2b3` |
| hashlink | 0.9.1 → **0.12.2** | `6ba4ff71…` → `a596f1b20ed2cc5ecac41a164aaebc7258057060f06c0cf7a2ba3991ee7990fb` |
| hashbrown 0.14.5 | **移除**（随 hashlink 0.9.1 淘汰，多版本 disambiguation 目录消失） | `e5274423…` 块删除 |
| hashbrown 0.17.1 | **零改动**（既有包本批零 diff）；lock 内依赖引用 `"hashbrown 0.17.1"`→`"hashbrown"` 单版本化 | 不变 |
| sqlite-wasm-rs | **absent**（default-features=false 关 wasm 后端生效） | — |
| 新增包 | **0 个** | — |

registry 计数（本会话 fresh）：`grep -c '^source = '` = **374**（DI-1 收口口径 375，净 −1 = hashbrown-0.14.5）、`[[package]]` = **399**、`ls vendor | wc -l` = **374**。全部变更在 RUSTSEC-2026-0459 修复升级链内。

**本会话 fresh 复跑门禁（2026-11-21，仓库根，全部实跑实贴）**：

```
$ cargo deny check advisories                        → advisories ok   exit 0
$ cargo deny check licenses                          → licenses ok     exit 0
$ cargo check --workspace --all-targets --offline    → Finished 1.77s  exit 0
$ cargo test -p database --offline                   → 8 targets 22 passed 0 failed  exit 0（E.2 逐 target）
```

## B. 架构与复用（单点钉版 × 重导出模式 × 既有 vendor/deny 流程复用）

- **唯一 sqlite 入口单点化**（本会话 sed 复测）：全 workspace 仅 `crates/database/Cargo.toml:11` 声明 rusqlite（`workspace = true`），`crates/database/src/lib.rs:31` `pub use rusqlite;` 重导出，消费 crate 全走 `use database::{rusqlite, Pool}`。bump 落点 = workspace 根一行 + lock + vendor，消费面 API 语义零变化（两处签名适配见 D 节）；本批零新增 crate（`[[package]]` 399 全部为既有 registry 面或 workspace 成员）。
- **feature 钉版语义等价策略**：default-features=false 关掉 0.41 默认 `ffi-sqlite-wasm-rs`（lock 实测 sqlite-wasm-rs absent）；`cache` 显式补回（`crates/plugin-runtime/src/ingest.rs:407`/`:458` 两处 `prepare_cached` 本会话实测在位）；`fallible_uint` 恢复 u64/usize FromSql 读出侧（`vendor/rusqlite/src/types/from_sql.rs:143-146` `#[cfg(feature = "fallible_uint")]` 本会话实测）——消费代码零改动，语义与 0.32 相同（`> i64::MAX` fallible）。
- **复用既有流程，不建新机制**：vendor 增量走任务书 §4 七步口径；许可证面复用 `[licenses]` 既有 allow 清单，hashlink 0.12.2 = MIT OR Apache-2.0 落清单 manifest 零补录（deny.toml `[licenses]` 段零改动）。
- **红线不碰**：database crate 零 diff、无迁移面、无 SQL 改动、无插件/工具登记面变化——纯依赖面批次（D 节逐项断言）。

## C. 文件清单（两个事实面：实施批入库面 + 当前工作树 porcelain，均实测）

**面 1：实施批 `76c6feb6`（已入库，`git show --stat` 实测 164 files / +58717 / −335418）**——非 vendor 逐文件：

```
Cargo.lock                                  ±27 行（三包 version+checksum、hashbrown-0.14.5 块 −8 行、依赖引用单版本化 4 行）
Cargo.toml                                  +10/-1（:42-49 理由注释 + :50 钉版行）
crates/mcp-server/src/resources.rs          column_names() 迭代器化适配
crates/rp-mcp-parity/tests/parity.rs        column_names() 迭代器化适配
deny.toml                                   +12 行（[advisories] 段，[licenses] 零改动）
docs/phase0/SEC-0-TASK.md                   新增（259 行）
docs/phase0/SEC-0-ADVISORIES-DECISION.md    新增（171 行）
docs/phase0/SEC-0-BATCH-REPORT.md           新增（v1，247 行）
```

**面 1 vendor（按目录级登记，`git show --name-status --format= 76c6feb6 -- vendor/<dir> | wc -l` 本会话逐目录实测）**：

| 目录 | 条目 | 内容 |
|---|---|---|
| `vendor/rusqlite` | 70（63 M + 7 新增） | 0.32.1→0.41.1 全量重写；新增 `bindings.md`、`examples/owning_{rows,statement}.rs`、`src/bind.rs`、`src/types/jiff.rs`、`src/wal.rs`、`types.md` |
| `vendor/libsqlite3-sys` | 26（15 M + 8 D + 3 新增） | 0.30.1→0.39.1；bundled SQLite 3.54.1（`sqlite3/sqlite3.h:149` 本会话 sed 实测）；删 bindgen_3.14.0{,_ext}.rs、sqlcipher 四文件、upgrade{,_sqlcipher}.sh；新增 bindgen_3.45.3{,_ext}.rs、Cargo.lock |
| `vendor/hashlink` | 19（15 M + 1 D + 3 新增） | 0.9.1→0.12.2；删 `.circleci/config.yml`；新增 `.github/workflows/ci.yml`、`Cargo.lock`、`deny.toml` |
| `vendor/hashbrown-0.14.5` | 41 D | 整目录移除（本会话 python 实测 ABSENT） |
| `vendor/hashbrown`（0.17.1） | **0 条** | 既有 tracked 目录本批零 diff |

**面 2：当前工作树 porcelain（2026-11-21 `git status --porcelain` 实测逐文件；评审修订 + 本报告落盘后 = 4 M）**：

```
 M deny.toml                                  # +5/-4：ignore 三条行尾追加「；复查 2027-05-20」，块头「复查统一」语义移除
 M docs/phase0/SEC-0-ADVISORIES-DECISION.md   # +32/-4：§3.1 行号锚定修准（去 :50 写死）+ §3.2 fork 措辞按公告原文修准 + §7 复核记录
 M docs/phase0/SEC-0-TASK.md                  # +23/-0：§2.1.1 probe-cargo workspace 外残留登记 + §6 两条 2026-11-21 复核记录
 M docs/phase0/SEC-0-BATCH-REPORT.md          # 本报告 v2（落盘后计入；v1 已随 76c6feb6 入库）
```

无其它漂移（本会话实测均为 0 行）：`vendor/`、`.cargo/`、`integrations/rtis/`、`migrations/`、`web/`、`apps/` porcelain 全零；工作树与 origin/main 同步（`git log origin/main..HEAD` 空）。

## D. 迁移与兼容

**零迁移，逐项实测（本会话）**：

- `git status --porcelain -- migrations/` = 0 行：无 schema/迁移面改动，`platform*.db` 五库 mtime 实测 2026-11-06/07 全部早于批次日（红线达成）；`dev-*.db` 为 gitignored 运行时副本（`git check-ignore` rc=0 实测），其 11-20/21 mtime 属后继批次运行面与本批无关。
- 全仓代码/测试 SQLite 版本钉值 grep（`3\.46|3\.54`，`crates/ tests/ apps/ --include='*.rs'`）= **0 命中**（本会话实测）：升级测试全走内存库/副本，零测试值改动。

**rusqlite 版本面升级兼容断言（全部本会话逐项取证）**：

1. **u64/usize FromSql 读出侧**（0.38 起默认关闭）：命中位 = `crates/database/src/retention.rs:299`/`:324`（COUNT(*)→u64 本会话 grep 实测）；消除 = workspace 钉版补 `fallible_uint`（Cargo.toml:50），**零代码改动**；语义核实 = `vendor/rusqlite/src/types/from_sql.rs:143-146` `#[cfg(feature = "fallible_uint")] from_sql_integral!(u64/usize)` 实测。
2. **`column_names()` 返回类型**：0.41 起返回 `impl DoubleEndedIterator<Item = &str> + ExactSizeIterator`（`vendor/rusqlite/src/column.rs:80` 实测）；两处最小适配 `crates/mcp-server/src/resources.rs:528-529/:538` 与 `crates/rp-mcp-parity/tests/parity.rs:422-423` 实测在位（注释 + 去 `.iter()`/解括号）。
3. **`prepare_cached` 语句缓存**（0.38 起改 feature `cache`）：消费位 `crates/plugin-runtime/src/ingest.rs:407`/`:458` 实测在位；default-features=false 失去默认 cache → 显式补回。
4. **`execute` 多语句限制**（0.35.0）：迁移执行全走 `execute_batch`（`crates/database/src/migrate.rs:126` 实测），多语句合法入口不受影响。
5. **bundled SQLite 跨度 3.46.x→3.54.1**：`vendor/libsqlite3-sys/sqlite3/sqlite3.h:149` `#define SQLITE_VERSION "3.54.1"`（本会话 sed 实测，公告修复线 3.54.0 之上）；备份恢复（Online Backup API）、四套迁移回放、retention 测试全绿（E.2）承载兼容判定。公告补充细节（SEC-0-TASK.md §6 复核记录）：0.39.x `bundled-sqlcipher` feature 仍捆绑 sqlcipher-src 419.0.6（SQLite 3.53.4，受影响），本仓 features 未启用 sqlcipher 分支，不受影响。
6. **MSRV**：rusqlite 0.41.1 = 1.89.0、libsqlite3-sys 0.39 = 1.88.0、hashlink 0.12.2 = 1.85（vendor Cargo.toml :14 本会话实测）；workspace `rust-version = "1.99"`（Cargo.toml:34）、本机 rustc 1.99.1 满足。**CI 无独立 MSRV job**（E2 四 job 实测），工具链 `@stable` 随远端漂移——MSRV 断言由本地工具链承载，CI runner 是否 ≥1.89 未单独核（如实）。

**database crate 零 diff**：`git show --stat 76c6feb6 -- crates/database/` = 0 行（本会话实测）——升级全部代码代价 = 两个消费 crate 各 1 处签名适配。

## E. 测试真实结果

### E.1 deny advisories/licenses 前后对照

| 时点 | 配置 | lock | 结果 | 出处 |
|---|---|---|---|---|
| 前（批内基线记录） | deny.toml 无 `[advisories]` 段 | 旧（libsqlite3-sys 0.30.1） | exit 1：4 error（0459 vulnerability + 三 unmaintained）+ 1 warning（index-failure） | 实施会话执行，**本会话未复跑（需回滚 lock，如实标注）** |
| 对照实验（**本会话 fresh**） | `76c6feb6^`（=16401fcb，无 `[advisories]` 段）deny.toml + 新 lock | 新（0.39.1） | **exit 1：恰 3 error**（chrono/paste/unsafe-libyaml unmaintained），**0459 vulnerability error 消失、index-failure warning 消失** | `git show 76c6feb6^:deny.toml > /tmp/deny-head.toml && cargo deny --config /tmp/deny-head.toml check advisories` |
| 后（**本会话 fresh**） | deny.toml（3 条 ignore，行尾复查日期修订版） | 新（0.39.1） | **exit 0，`advisories ok`，零 error 零 warning** | `cargo deny check advisories` |
| licenses（**本会话 fresh**） | 既有 `[licenses]` 段（零改动） | 新 | **exit 0，`licenses ok`**；hashlink 0.12.2 MIT OR Apache-2.0 落既有清单 | `cargo deny check licenses` |

对照实验 error 回显（cargo-deny 0.20.2）：

```
error[unmaintained]: chrono is unmaintained        ├ RUSTSEC-2026-0543
error[unmaintained]: paste - no longer maintained  ├ RUSTSEC-2024-0436
error[unmaintained]: unsafe-libyaml is unmaintained├ RUSTSEC-2024-0449
advisories FAILED                                  （exit 1）
```

——两件事各自独立成立：① 0459 由 bump 单独消除（pre-batch 配置下 vulnerability error 已不在）；② 三条 unmaintained 消音来自 ignore 而非配置缺省（无 ignore 时 exit 1）。index-failure warning 消失为环境面（advisory-db fetch 成功），pre-batch 配置下同样无 warning。

### E.2 database 定向套件（本会话 fresh，`cargo test -p database --offline`，exit 0）

```
Running unittests src/lib.rs (database)                 11 passed; 0 failed
Running unittests src/bin/rp-admin.rs                    0 passed; 0 failed   ← 批内转述"11"不复现，坏消息 4
Running tests/backup_restore.rs                          4 passed; 0 failed   （Online Backup API 对 3.54.1）
Running tests/migrations_agent_data_plane.rs             1 passed; 0 failed
Running tests/migrations_context_broker.rs               2 passed; 0 failed
Running tests/migrations_runtime_node.rs                 2 passed; 0 failed
Running tests/migrations_tool_contract.rs                2 passed; 0 failed
Doc-tests database                                       0 passed; 0 failed
合计：8 targets，22 passed / 0 failed
```

### E.3 workspace check（本会话 fresh，exit 0）

```
$ cargo check --workspace --all-targets --offline
    Checking database v0.2.0 (…/crates/database)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.77s
```

### E.4 checksum 三方核对（本会话 python 复测 PASS）+ 批内记录（未复跑，如实标注）

- **三方核对（本会话实测逐值）**：Cargo.lock `checksum` == vendor `.cargo-checksum.json` `package` == 任务书 §1.4 预记录，三包三值逐一相等（python assert 通过，打印 three-way ok ×3，全值见 A 节清单）。vendor 卫生：`.gitignore` 残留 0、`vendor/hashbrown-0.14.5` ABSENT、三链目录在位。tarball sha256 直测为实施会话执行（hashlink 直连超时降级，坏消息 6），本会话未重新下载。
- **三条 ignore 消费面复测（本会话 fresh）**：`cargo tree -i chrono`（rmcp 3.10.0 ← mcp-server ← rp-mcp-parity + schemars 1.2.3 ← rmcp）/ `-i paste`（tokenizers 0.21.4 ← context-broker，proc-macro）/ `-i unsafe-libyaml`（serde_yaml_ng 0.10.0 ← intel-db、mcp-server 等上游）与裁决书 §1.1/§2.1/§3.1 逐值一致；workspace 源码直用 grep：chrono 0 文件、paste 0 文件、serde_yaml_ng 10 文件。
- **批内记录（本会话未复跑）**：`cargo test -p agent-engine --offline` 45 passed / `workflow-engine` 68 / `tool-engine` 126；vendor 七步流程含联网窗口纪律（`.cargo/config.toml` 移开复原）。全仓测试+clippy 面由 CI 44589236396「Rust workspace」job success 兜底（E2）。

## E2. CI（v1「待 watch 回填」已到位，本节为实测回填）

- **SEC-0 提交自身 CI 绿**：`gh run list` 实测 run **44589236396**，headSha = `76c6feb6`，conclusion = **success**；`gh run view` 四 job 全 success：`Rust workspace（测试与 clippy 零警告）`/`Web 工作台（构建与 vitest）`/`协议一致性载具（REST vs MCP 双通道）`/`Python worker 测试`。前批基线 44573324921（16401fcb）success 同在。
- **后续轨迹如实**：44614722971（22c7adb0）与 44615850943（2f25787f）两次 failure 属 DI-2 归档面宿主绑定环境面（commit message 登记 host_binary_pin_guard SKIP 收口，非本批回归）；0339a1eb 修复后 44616753166 → 44759852926 → 44768344622（**HEAD 061cfdef**）全绿——SEC-0 字节在 HEAD 终态 CI 面内持续绿。
- **CI 盲区如实**（坏消息 2）：ci.yml 无 deny 步骤、无独立 MSRV job、工具链 `@stable` 无钉版——依赖安全双门与 MSRV 下限都不在 CI 兜底内。
- 回退面（任务书 §5.3）：`git revert` 一体回滚 Cargo.toml/Cargo.lock/deny.toml/适配两文件/vendor 四目录。

## F. 性能（升级面无吞吐测量，如实）

- **本批未建吞吐基线、未跑任何吞吐/延迟基准（未建即写未建）**：改动面 = 编译期 bundled SQLite 替换（3.46.x→3.54.1）+ rusqlite API 签名适配，无 SQL 语义、连接池、事务行为改动；「升级面无吞吐变化」没有测量证据支撑，本报告不以其为断言，只以下列功能级证据承载兼容判定：database 8 targets 22 passed（E.2）+ workspace check exit 0（E.3）+ CI Rust workspace job success（E2）。
- **相邻面区分（防冒认）**：autoresearch `assemble_ms` 面 keep 记录 1.863 vs 基线 2.399 属 context-broker tokenizer 计量链（.auto 记账），与本批 sqlite 升级面零交集，不作为本批性能证据。
- `cache` feature（`prepare_cached`）语义与 0.32 相同（ingest.rs 两处调用点零改动）；bundled 3.54 vs 3.46 查询计划差异未单独测量，如出现 DB 延迟面需求另开基准批次。WSW-3 批次的 `exchange_persist` 吞吐实测（中位 10091 rows/s，36638fe5）为后继批次独立实测，非本批证据。

## G. 遗留与风险

1. **三条 ignore 复查 2027-05-20（deny.toml:45-47 行尾，评审修订后单条调整只动该行）**，触发条件见裁决书各节：
   - RUSTSEC-2026-0543（chrono 0.4.45）：触发 = rmcp 发版去 chrono/换 jiff，或 chrono 实质漏洞公告（unmaintained+漏洞组合不可 ignore）；动作 = 重跑 `cargo tree -i chrono`，解则删 ignore 行并确认 advisories 仍 exit 0（裁决书 §1.3）。
   - RUSTSEC-2024-0436（paste 1.0.15）：触发 = tokenizers 升级批次落地时顺带核对（裁决书 §2.3）。
   - RUSTSEC-2024-0449（unsafe-libyaml 0.2.11）：**替换候选标记**（三条中唯一低成本路径：API 面 10 文件/20 行实测，但解析语义面需跨 5 crate 回归）；触发 = 新增 YAML 解析消费点、unsafe-libyaml 实体漏洞公告、serde_yaml_ng 发布去 unsafe-libyaml 版本（裁决书 §3.3）。
2. **index-failure 复查项：实质已提前闭环**——本会话 fresh `cargo deny check advisories` 零 warning（yanked 核验完成，email_address 0.2.10 / filetime 0.2.30 零命中）。保留下次依赖安全批次开场复核一句，不再作为独立待办。
3. **probe-cargo 遗留观察项**（坏消息 1 / SEC-0-TASK.md §2.1.1）：复活为活跃工程须按任务书 §1/§4 同口径 bump 至 rusqlite 0.41.1 + vendor 重写；长期闲置则下次依赖安全批次开场核对一次其 lock 是否变化。
4. **CI 无 deny 门（坏消息 2）**：deny 双门当前是本地常驻纪律；建议下批评估 ci.yml 增补 deny step（成本 = run 时长，收益 = 远端兜底 + MSRV 显性化），未评估前不得把「CI 绿」误读为「双门远端已过」。
5. **`[advisories]` 段是本批新增的常驻门**：后续任何依赖面批次（含 H 节三批若新增依赖）在 vendor 七步外必须过 `cargo deny check advisories/licenses` 双门；ignore 清单增长走裁决书流程（理由 + 替代评估 + 复查日期），不允许静默加行。
6. **工作树 4 文件未提交**（坏消息 3）：评审修订 + 本报告待单独 commit 收口。

## H. 下一批依赖（RT-2 / DI-2 / WSW-3）

三批与 SEC-0 零前置耦合（纯依赖面批次，无 API/登记面产物供下游消费）；共享写面只有 `Cargo.lock` / `vendor/`——任一批新增依赖必须复用七步 vendor 流程 + deny 双门（G.5），并行时注意 lock/vendor 冲突面。**现状（本会话实测）**：

- **RT-2（Python Worker 原生环境）**：定义 = PHASE-ROADMAP.md:786「项目级 venv、依赖锁、启动探测、版本/哈希记录、协议一致性测试」。**未启动**（docs/phase0 无 RT-2 文件，ls 实测 0）。Python 侧依赖锁面与 Rust vendor 流程互不触达；SEC-0 对 python_worker 零触碰（porcelain 无该面）。
- **DI-2（Office/OCR 链路）**：**已收口入库**——`docs/phase0/DI-2-BATCH-REPORT.md` 在位；commits `22c7adb0`（收口批）+ `2f25787f`/`0339a1eb`（CI 宿主绑定守卫修复）。SEC-0 与其共享写面（lock/vendor）在 DI-2 实施期间保持一致（其 CI 修复批 lock 未动，failure→success 轨迹见 E2），无冲突发生。
- **WSW-3（HTTP History + Repeater）**：**已收口入库**——commits `9aff134a`（WIP 快照）→ `36638fe5`（收口批，含 `docs/rip3/batches/WSW-3-BATCH-REPORT.md` 215 行）→ `a6aea6a4`（实拍与联动追加）→ `6aa95bb6`（评审修订）→ `061cfdef`（E2 终态回填）。其新增依赖面在本地门禁记录中含 `deny licenses` + `lock 399=HEAD`（36638fe5 commit message 实录）——SEC-0 立的「lock 计数不变即零新增包」核对口径已被下游复用。

## I. 浏览器截图

**本批无 UI 面，零截图（如实）**：改动面 = manifest/lock/deny + vendor + 两处后端源文件（C 节实测清单），无任何 web/UI 条目（`web/`、`apps/` porcelain 本会话实测 0 行）；无浏览器 e2e 用例故无截图可附。WSW-3 批次的 7 张实拍截图属该批账目，与本批无关。

## J. RTIS 对账

**零接触，实测**：

- `git status --porcelain -- integrations/rtis/ migrations/ scripts/rtis_kb_worker.py .cargo/` = **0 行**（本会话实测）；全仓 porcelain 无任何 rtis 命中（红线达成）。
- mtime 实测全部早于批次日 2026-11-20：`integrations/rtis/knowledge.db` = 11-18 05:36、`integrations/rtis/rtis.py` = 11-09 09:45、`scripts/rtis_kb_worker.py` = 11-09 21:55、`migrations/platform*.db` 五库 = 11-06/07（本会话 ls 实测）；实施批 `76c6feb6` 文件清单内 rtis/platform 零命中（git show --stat 实测）。
- 本批不触知识摄取/检索链路；RTIS 旧链条数对账**未复跑**（无触碰面故无需，同 DI-1 §J 口径），本报告不以其为门。

## K. 教训

1. **影响面 grep 必须双向（ToSql 绑定位 + FromSql 读出位）**：任务书 §2.3 只 grep `params![` 绑定位，漏 `get::<u64>` 读出位（retention.rs COUNT(*)）——0.38 起 FromSql u64 默认关闭直接编译断。类型参数的 grep 要覆盖 trait 两侧实现，不只是调用宏。
2. **签名返回类型变化按 API 全表对照 release notes，不能只按使用点 grep**：`column_names()` 从 `&[&str]` 改 iterator，「预期 git status 面」未登记导致验收自检漏项（评审 low #2 根因）；以「回填任务书 + 批报登记批内适配面」双落点收口。
3. **default-features=false 不是免费的**：0.41 default 带两个隐式功能（cache、ffi-sqlite-wasm-rs），关 default 换 vendor 最小化必须逐项显式回补（cache）与语义面（fallible_uint），理由写进钉版行注释——否则下次升级者按旧文档去 feature 就编译断。
4. **cargo vendor 多版本命名形态预先认识**：同名包多版本时 vendor 出 `hashbrown` 与 `hashbrown-0.14.5` 双目录；升级收敛单版本后 suffix 目录整体消失，vendor 目录数是**净减**不是净增——升级前用 `cargo tree -i <crate>` 摸清同名包全消费面再预估。
5. **cargo-deny CLI 形态先验证再写判据**：`check` 子命令不接受 `--offline`（0.20.2 实测）；判据里的命令先在本机跑通再落笔。
6. **网络瞬态与「不可达 ≠ 已核验」闭环要跟踪到消失**：advisory-db TLS 偶发失败重跑即恢复，index-failure warning 随 fetch 成功消失——登记项的目标状态是「warning 消失 + 核验零命中」，不是「命令通过」。
7. **checksum 三方核对要有降级路径并如实记账**：tarball 直连可超时，crates.io API checksum 是预指定的合规替代源；降级发生时批报单列，不冒充三方全测。
8. **deny 扫描面以 workspace 根为界——「全绿」是扫描面内的全绿**：workspace 外独立 lock 工程（probe-cargo）自带 lock/vendor、无 deny.toml，门不覆盖；影响面分析必须显式列出「不被门覆盖的 lock 面」（SEC-0-TASK.md §2.1.1 即此教训产物，2026-11-21 补记）。
9. **文档行号引用不写死**：裁决书 §3.1 引 `Cargo.toml:50` 钉 serde_yaml_ng，SEC-0 在 :42-49 插入 8 行注释后声明漂移至 :58——行号引用改用段位锚点（`[workspace.dependencies]` 段）表述，修订已落地。
10. **ignore 元数据放行尾，不放块头**：块头「复查统一 2027-05-20」在单独调整某条复查日期时牵动块头语义；修订后复查日期在三条行尾、块头只留裁决出处与规则说明（deny.toml:43-47）。
11. **CI 绿 ≠ 门全绿**：CI 只测它配置了的 job；deny 双门、MSRV job、工具链钉版都不在 ci.yml 内（本会话 grep 实测），把「Rust workspace job success」当「依赖安全已过」是本批明确写进坏消息的反面口径。

---

## 复现

```bash
cd /media/emo/code/pensting/research-platform
# deny 双门（预期：advisories ok / licenses ok，双 exit 0，零 error 零 warning）
cargo deny check advisories && cargo deny check licenses
# 对照实验：pre-batch deny.toml（76c6feb6^，无 [advisories] 段）+ 新 lock
# （预期 exit 1 恰 3 unmaintained error：0543/0436/0449；无 vulnerability、无 index-failure warning）
git show 76c6feb6^:deny.toml > /tmp/deny-head.toml && cargo deny --config /tmp/deny-head.toml check advisories
# 编译与定向测试（预期 exit 0；database 8 targets 22 passed 0 failed）
cargo check --workspace --all-targets --offline
cargo test -p database --offline
# 静态门（预期：374 / 399 / 374 / 0 / 0）
grep -c '^source = ' Cargo.lock; grep -c '^\[\[package\]\]' Cargo.lock; ls vendor | wc -l
git status --porcelain -- vendor/ integrations/rtis/ migrations/ .cargo/ | wc -l
# 修复线实证（预期 SQLITE_VERSION "3.54.1"）
sed -n '149p' vendor/libsqlite3-sys/sqlite3/sqlite3.h
# probe-cargo workspace 外残留（预期 rusqlite 0.32 bundled / lock 0.32.1 / libsqlite3-sys 0.30.1 / members 零命中）
grep -n rusqlite probe-cargo/Cargo.toml
grep -A1 'name = "libsqlite3-sys"' probe-cargo/Cargo.lock | head -2
sed -n '3,30p' Cargo.toml | grep -c probe
# checksum 三方 + vendor 卫生（预期 three-way ok ×3，hashbrown-0.14.5 ABSENT，.gitignore 残留 0）
python3 - <<'EOF'
import json, re, os, subprocess
lock = open('Cargo.lock').read()
for name, ver, want in [('rusqlite','0.41.1','da9bba6f3506135e120a6586df05be18b48d18896600646a4bec8f993d91a594'),
                        ('libsqlite3-sys','0.39.1','14a5f01b87b92705d07b433c06f96cbc764637d81a0bffb5734ce17cf0f9a2b3'),
                        ('hashlink','0.12.2','a596f1b20ed2cc5ecac41a164aaebc7258057060f06c0cf7a2ba3991ee7990fb')]:
    m = re.search(r'\[\[package\]\]\nname = "%s"\nversion = "%s"\nsource = "[^"]*"\nchecksum = "([0-9a-f]+)"' % (name, ver), lock)
    v = json.load(open('vendor/%s/.cargo-checksum.json' % name))['package']
    assert m.group(1) == v == want, name
    print(name, 'three-way ok')
print('hashbrown-0.14.5', 'ABSENT' if not os.path.isdir('vendor/hashbrown-0.14.5') else 'PRESENT')
print('.gitignore residue:', subprocess.run(['find','vendor','-name','.gitignore'],capture_output=True,text=True).stdout.count('\n'))
EOF
# CI 终态（预期 44589236396 76c6feb6 success；HEAD 061cfdef 亦有 success run）
gh run list --limit 12
# 三条 ignore 消费面（预期：rmcp/schemars←rmcp 链、tokenizers←context-broker、serde_yaml_ng←多上游）
cargo tree -i chrono | head -6; cargo tree -i paste | head -3; cargo tree -i unsafe-libyaml | head -3
```
