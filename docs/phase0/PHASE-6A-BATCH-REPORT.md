# PHASE-6A 批次报告 · Report IR + MD/HTML/JSON 渲染器（vendor+deny 政策 comrak 单 crate 跃迁）

- 批次：PHASE-6A（docs/phase0/PHASE-6A-TASK.md）
- 日期：2026-11-23（实施日同日；HEAD `1b5836f3` 起步）
- 状态：**实施完成，裁决 B 修订态**——原 D3 双 crate（comrak + docx-rs）于实施中触发 deny advisories 门不可达（docx-rs→zip 8.6.0 六条 vulnerability error 级 RUSTSEC），经裁决半撤 docx-rs 保 comrak 单 crate；IR schema 单源 + MD/HTML/JSON(SARIF) 三渲染器 + fixture e2e 全部交付；DOCX 冒烟（原判据 12）顺延 6b
- 实施纪律：禁 git commit/add/push（工作树留主会话审查收口）；构建 `CARGO_TARGET_DIR=/home/emo/rt3-target`；Rust 命令 `--offline`；定向测试 `--test-threads=1`；生产库五文件全程 mode=ro

---

## 1. 执行裁决记录（D3 双 crate → 裁决 B 单 crate 停批实录）

### 1.1 停批事实链（全部实测）

| # | 事实 | 证据 |
|---|---|---|
| 1 | S1 基线全绿：HEAD `1b5836f3`、lock 417/reqwest=1、migrations 30、vendor 392 目录 387M、workspace 全量 1339 passed 零红 EXIT=0、HEAD 态 deny advisories ok（git worktree `/tmp/p6a-deny-base` 实测 exit 0） | §4 命令 C1-C4 |
| 2 | 双 crate vendor 跃迁执行后 `cargo deny check advisories` FAILED（exit 1）：6 条 vulnerability error 级 **RUSTSEC-2026-0367/0368/0369/0429/0545/0546** 全部命中 `zip 8.6.0 ← docx-rs 0.4.22 ← report-engine` | §4 命令 C6 |
| 3 | advisory DB（本地最后同步 2026-11-19，早于任务书定稿）`crates/zip/` 实存 8 条 RUSTSEC，patched 全部 `>= 9.0.0`（0548 为 `>=9.0.5`），unaffected 区间均为老版本（`<0.6.0`/`<7.2.0`/`<5.1.0`）——**zip 8.x 全线 affected** | §4 命令 C7 |
| 4 | 不可达坐实：docx-rs 0.4.22 = crates.io latest（`cargo search --registry crates-io` 实测无更新版）且硬依赖 `zip = { version = "8.6.0" }`（caret ^8.6.0，非 optional，pack 必经）；zip latest 9.8.1 被 ^8.6.0 约束堵死；`[patch]` 强钉 9.x 不满足依赖约束不生效 | §4 命令 C8 |
| 5 | HEAD lock 417 中 zip 零存在——6 条红 100% 为本批 docx-rs 引入，非既有基线面 | `git show HEAD:Cargo.lock \| grep -c '^name = "zip"'` = 0 |
| 6 | deny `[advisories] ignore` 增行 = 违红线 2（「deny 行增量仅限 Unicode-DFS-2016 一行」）+ §2.2（[advisories] 零改动）；SEC-0 先例（docs/phase0/SEC-0-ADVISORIES-DECISION.md）是 unmaintained INFO 级，error 级 vulnerability 开 ignore 需独立安全裁决 | 任务书红线词形 |

### 1.2 裁决 B 执行结果（2026-11-23 实施日执行完毕）

- **Cargo.toml**：撤 docx-rs 钉行（comrak `=1.1.0` 保留），行注登记停批事实与 6b 重侦察路径
- **Cargo.lock**：`cargo update -w` 重解 + `cargo vendor --locked` 重生成 → **432 包**（417 + 14 第三方 + 1 report-engine 自身）、reqwest=1 恒定、**zip/docx-rs 计数 = 0**
- **vendor/**：406 目录（392 基线 + 14：12 新包目录 + phf-0.13.1/phf_shared-0.13.1 多版本拆分目录）；vendor/zip 随撤除删除
- **deny.toml**：Unicode-DFS-2016 行保留（finl_unicode 属 comrak 链），双检双 ok
- **comrak 链 deny advisories 复验 ok**——裁决 B 后全部门复绿
- **phf 双版本共存归属（cargo tree 实测）**：`phf 0.13.1 ← tokio-postgres`（既有 postgres/refinery 链）+ `phf 0.14.0 ← comrak`（本批引入）——vendor 目录命名规则（高版本占无后缀名 `phf`/`phf_shared`，低版本拆 `phf-0.13.1`/`phf_shared-0.13.1` 带后缀目录）→ 既有 `vendor/phf`/`vendor/phf_shared` 目录内容 0.13.1→0.14.0 置换（M 状态 19 文件）
- **DOCX 冒烟（原判据 12）顺延 6b**：届时重新侦察 zip 9.x 兼容的 DOCX crate 或 docx-rs 升级版；**侦察必答项新增：vendor 候选侦察必须跑 `cargo deny check advisories` probe**（本批教训）

## 2. 改动文件清单（路径 + 行数；git diff/add 落点核对见 §4 C15）

| 路径 | 类型 | 行数/说明 |
|---|---|---|
| `Cargo.toml` | 修改 | +11/-0（numstat 实测）：comrak exact 钉 + 行注（裁决 B 修订）+ members 增 `crates/report-engine` |
| `Cargo.lock` | 重生成 | +135/-2（numstat 实测）；417→432（净增 15：14 第三方 + report-engine 自身） |
| `deny.toml` | 修改 | +4/-0（numstat 实测）：Unicode-DFS-2016 一行 + 三行注（finl_unicode 唯一许可缺口） |
| `vendor/` | 增量 | +14 目录（caseless/comrak/entities/fastrand/fearless_simd/fearless_simd_macros/finl_unicode/memchr-n/phf-0.13.1/phf_codegen/phf_generator/phf_shared-0.13.1/rustc-hash/typed-arena）+ 既有 phf/phf_shared 两目录内容 0.13.1→0.14.0 置换（多版本共存命名规则）+ phf_shared 0.14.0 新增 hasher.rs/ptrhash.rs |
| `schemas/report-ir/report-ir-v1.schema.json` | 新建 | 315 行：Report IR v1 schema 单源（draft 2020-12，$id = `research-platform/schemas/report-ir/report-ir-v1.schema.json`） |
| `crates/report-engine/Cargo.toml` | 新建 | 19 行：publish=false + domain/serde/serde_json/tool-registry/comrak 依赖面（D2① 裁决 B 修订） |
| `crates/report-engine/src/lib.rs` | 新建 | 17 行：模块声明 + 四函数/validate_ir/from_json re-export |
| `crates/report-engine/src/ir.rs` | 新建 | 445 行：ReportIr/FindingIr/EvidenceRef 类型面 + IR_V1_SCHEMA include_str! + from_json/validate_ir（Rust 兜底先行 + schema 最后防线双通道） |
| `crates/report-engine/src/escape.rs` | 新建 | 21 行：escape_html 五字符单点（D5） |
| `crates/report-engine/src/render/mod.rs` | 新建 | 6 行 |
| `crates/report-engine/src/render/markdown.rs` | 新建 | 272 行：render_markdown（标题/清单/表格手写序列化） |
| `crates/report-engine/src/render/html.rs` | 新建 | 353 行：render_html（完整 `<!DOCTYPE html>` 文档，五段固定，markdown 片段经 comrak，文本/属性插值单点 escape_html） |
| `crates/report-engine/src/render/json.rs` | 新建 | 12 行：render_json（serde Serialize 直出） |
| `crates/report-engine/src/render/sarif.rs` | 新建 | 195 行：render_sarif（SARIF 2.1.0，D1 映射表逐条落） |
| `crates/report-engine/tests/fixtures/main-report.json` | 新建 | 109 行：2 Finding 正例（confirmed 带 evidence 六 ID + rtis://poc/42 + 双解释字段位 + high/medium 两级） |
| `crates/report-engine/tests/fixtures/mixed-report.json` | 新建 | 68 行：五态×五级覆盖（confirmed/high、false_positive/info、not_reproduced/medium、not_tested/low、inconclusive/critical） |
| `crates/report-engine/tests/consistency.rs` | 新建 | 237 行：判据 7/9/10 对账（7 测试） |
| `crates/report-engine/tests/sarif_structure.rs` | 新建 | 224 行：判据 8 结构断言（5 测试） |
| `crates/report-engine/tests/negative.rs` | 新建 | 225 行：判据 4 负例 + 词表单源对账（9 测试） |
| `crates/report-engine/tests/html_escape.rs` | 新建 | 138 行：判据 11 转义/中文/comrak 标签（5 测试） |
| `apps/server/selftest_p6a.sh` | 新建 | 112 行：ck/sk + SKIP 守卫七节（selftest_wsw8.sh:13-20 词形先例） |
| `docs/phase0/PHASE-6A-TASK.md` | 输入物 | 任务书（本批 untracked 输入，随批落盘） |
| `docs/phase0/PHASE-6A-BATCH-REPORT.md` | 新建 | 233 行：本报告 |
| `docs/README.md` | additive | +1 行（RIP3 批次表登记） |

## 3. 判据 1-14 逐条状态

| # | 判据（任务书原词形） | 状态 | 实测 |
|---|---|---|---|
| 1 | vendor 跃迁声明兑现（lock 442/净增 25） | **修订达标** | 裁决 B 态：lock **432**（417+14 第三方+1 自身）、reqwest=1、`git status --porcelain vendor/` 只增不删（0 删除）、comrak `=1.1.0` exact 在位；442/25 为原双 crate 探针预估，实测漂移坐实见 §8 坏消息 2/3 |
| 2 | deny 双检含新行 | **达标** | advisories ok + licenses ok 双 exit 0；`git diff deny.toml` = Unicode-DFS-2016 一行 + 行注；许可台账 §6.2（finl_unicode 实测 1.5.1） |
| 3 | 零迁移 + 编号登记 | **达标** | migrations/001-030 零 diff、migrations_pg V0001-V0007 零 diff、**035 不立项**（IR 内存模型零落库；033/034 空缺词形延续）；reports PG 表 = 6b V0008 候选 |
| 4 | IR schema 单源 | **达标** | include_str! 内嵌（ir.rs const 定义行 grep=1）；validate_ir(合法 fixture)=Ok；三负例（severity 词表外/五态词表外/rtis_ref 非 rtis://）逐个 Err 且错误信息含字段名（Rust 兜底通道显式拼字段名——tests/negative.rs 9 测试全绿） |
| 5 | IR 字段面冻结（顶层 12+4 容器 / FindingIr / EvidenceRef） | **达标** | D1 清单逐字段在 schema：顶层 16 键（12 内容字段 + findings/attack_paths/appendix/properties 四容器位——判据词「顶层 12 字段」按 D1 清单词形全落）；FindingIr 18 字段（判据词「19」与 D1 清单 18 项差 1 为计数口径漂移——对账表见 §9）；EvidenceRef 7 字段；$id 词形 = `research-platform/schemas/report-ir/report-ir-v1.schema.json` |
| 6 | 四渲染函数签名钉定 | **达标** | 四签名 grep 逐条对账（§9 对账表）；全零 I/O 反证：签名面无 `&mut`、源码无 `std::fs`/`std::process`/`Command` 词形（grep 反证 = 0） |
| 7 | 三格式内容一致性（正例） | **达标** | main fixture（2 Finding）：finding 数 2 三面一致；每 finding_id/title/severity/vuln_type 三面逐一出现；`rtis://poc/42` 与六 ID 四词形三面 grep 同值（tests/consistency.rs 实测绿） |
| 8 | SARIF 结构面 | **达标** | version="2.1.0"、results/rules/artifacts 长度 = findings 数、result 标准键齐（ruleId/level/kind/message.text/properties.rip）、severity→level 五级实测（high→error/medium→warning/low→note/critical→error/info→note）、kind 映射五行（confirmed/false_positive→fail，其余→review）、properties.rip.{result_state,six_ids,rtis_refs} 三键恒在位、artifacts 与 evidence uri+sha-256 对账（3 条去重实测） |
| 9 | 空 Finding 集边界 | **达标** | 零 findings IR：三渲染 exit 0；MD/HTML 合法空节（「（无 Finding）」词形）；SARIF results=[]/rules=[]/artifacts=[]；三面计数一致 = 0 |
| 10 | 确定性 | **达标** | 同一 IR 两次 render_markdown/render_html/render_json/render_sarif 字节一致（四断言实测绿，main+mixed 双 fixture） |
| 11 | HTML 转义与中文面 | **达标** | escape_html 五字符穷举（`<>&"'` → `&lt;&gt;&amp;&quot;&#39;`）+ 属性上下文注入不破结构；中文 title/解释字段 HTML/MD 后 UTF-8 原样；comrak `<h2>`/`<strong>` 标签产出实测；comrak 对 IR 内原始 HTML 转义（安全面强于透传） |
| 12 | docx 冒烟（6b 前置） | **顺延 6b** | 裁决 B：docx-rs 停批撤除（zip 8.x RUSTSEC 链）——DOCX 全量渲染器与冒烟顺延 6b 重新侦察；零新增 reqwest/TLS 词形反证保持（Cargo.lock reqwest=1） |
| 13 | workspace 全量与静态门零红 | **达标（钉值修订）** | workspace 全量 `cargo test --offline --workspace --no-fail-fast` = **1365 passed 零红**（基线 1339 + report-engine 26 = 1365 精确闭合，§4 C11）；clippy `--workspace --all-targets -D warnings` exit 0；fmt 存量红 943 hunks = HEAD 态 943 恒定（本批零贡献；新代码 fmt 自净 report-engine grep=0——「≤936」钉值过时 +7 坐实，§8 坏消息 6）；deny 双检 exit 0 |
| 14 | 收口零越界 | **达标（钉值修订）** | 跨轨 git status = 0 行（wsw/verification/agent-engine/web/workers/integrations/rtis/migrations/migrations_pg/.github）；工作树落点 = §2.1 清单核对（§4 C15）；生产库五文件 mode=ro 指纹 = **1/1/1/0/0**（各文件时代基线值坐实——判据词「=30」钉值过时，dev-b1.db 才是 30；全程只读零写入，§8 坏消息 5） |

## 4. 收口命令实测（§5 任务书命令面逐条：exit code + 关键输出行）

| # | 命令 | exit | 关键输出 |
|---|---|---|---|
| C1 | `git status --porcelain \| wc -l` | 0 | `1`（任务书 untracked 输入物——S1 起点） |
| C2 | `git log --oneline -1` | 0 | `1b5836f3 WSW-8 CI 修复批二补…` |
| C3 | `grep -cE '^\[\[package\]\]' Cargo.lock` | 0 | S1: `417` → S2 双 crate: `445` → 裁决 B: **`432`** |
| C4 | `grep -c 'name = "reqwest"' Cargo.lock` | 0 | `1`（全程恒定） |
| C5 | `ls migrations/*.sql \| wc -l` / `ls migrations_pg/*.sql \| wc -l` | 0 | `30` / `7` |
| C6 | `cargo deny check advisories` | 双 crate 态 `1`（6 条 RUSTSEC zip 8.6.0）/ 裁决 B 态 **`0`（ok）** | `RUSTSEC-2026-0367/0368/0369/0429/0545/0546 → zip 8.6.0 ← docx-rs 0.4.22 ← report-engine`；裁决 B 后 `advisories ok` |
| C7 | advisory DB 实存核对 | 0 | `ls ~/.cargo/advisory-dbs/advisory-db-*/crates/zip/` = 8 条 RUSTSEC（DB 同步 2026-11-19）；`git -C … log -1 --format=%cI` = `2026-11-19T17:35:07+01:00` |
| C8 | `cargo search docx-rs --registry crates-io --limit 1` | 0 | `docx-rs = "0.4.22"`（latest 无修复 patch）；`cargo search zip … ` = `zip = "9.8.1"`；docx-rs 约束 = `zip = { version = "8.6.0" }`（caret） |
| C9 | `cargo build --offline --workspace` | 0 | S2 门 `EXIT=0` |
| C10 | `cargo test -p report-engine --offline -- --test-threads=1` | 0 | **26 passed 0 failed**（consistency 7 + html_escape 5 + negative 9 + sarif_structure 5） |
| C11 | `cargo test --offline --workspace --no-fail-fast` | **0** | **passed=1365 failed=0**（190 test result: ok；= 基线 1339 + report-engine 26 精确闭合） |
| C12 | `cargo clippy --offline --workspace --all-targets -- -D warnings` | **0** | 零 warn |
| C13 | `cargo fmt --all -- --check` | 1（存量红） | **943 hunks = HEAD 态 943 恒定**（git worktree HEAD 实测 943——本批零贡献）；`grep -c "crates/report-engine" fmt输出 = 0`（新代码自净） |
| C14 | `cargo deny check advisories && cargo deny check licenses` | 0+0 | 双 ok（Unicode-DFS-2016 新行在位） |
| C15 | `git status --porcelain` 落点核对 | 0 | M: Cargo.toml/Cargo.lock/deny.toml/vendor{phf,phf_shared} ×2 目录 + ?? 新文件清单——**全部 = §2.1 落点 + §8 坏消息 3 的 phf 置换面**；跨轨（wsw/verification/agent-engine/web/workers/integrations/rtis/migrations/migrations_pg/.github）= 0 行 |
| C16 | `bash apps/server/selftest_p6a.sh` | 0 | **PASS=25 SKIP=0 FAIL=0**（七节全绿） |
| C17 | 生产库五文件 `sqlite3 "file:…?mode=ro" "PRAGMA user_version"` | 0 | platform.db=1 / platform-dryrun.db=1 / platform-fullbatch.db=1 / platform-governance.db=0 / platform-search.db=0（时代基线值坐实；dev-b1.db=30） |
| C18 | `cargo tree -i phf@0.13.1` / `-i phf@0.14.0` / `-i zip` | 0/0/err | `phf 0.13.1 ← tokio-postgres`（既有）；`phf 0.14.0 ← comrak`（本批）；`zip` 零存在（`did not match any packages`） |
| C19 | `sha256sum schemas/report-ir/report-ir-v1.schema.json` | 0 | `888e9210f1ba44a5706439adb7a486ee75f8c4b6cc3c871c6184b5eb577a499e` |

## 5. vendor 跃迁实测（原双 crate 探针 → 裁决 B 实施态）

### 5.1 数字对账

| 面 | 任务书 D3 预估（双 crate） | 双 crate 实测 | 裁决 B 终态 |
|---|---|---|---|
| Cargo.lock | 442（+25） | 445（+28） | **432（+15）** |
| vendor 目录 | +25 | +27（25 新包 + phf×2 拆分）+ 2 目录内容置换 | **+14 目录**（12 新包 + phf×2 拆分）+ 2 目录内容置换 |
| zip 面 | A13 声明 zip 8.6.0（未跑 advisories probe） | 6 条 RUSTSEC error | **0**（docx-rs 撤除） |
| reqwest | 1 恒定 | 1 | **1** |
| deny 增行 | Unicode-DFS-2016 一行 | 同 | **同**（finl_unicode 属 comrak 链保留） |

### 5.2 comrak 链 14 包许可台账（cargo deny licenses ok 全集 = 既有 allow + Unicode-DFS-2016）

| 包 | 版本 | 许可表达式 |
|---|---|---|
| comrak | 1.1.0 | BSD-2-Clause |
| caseless | 0.2.2 | MIT |
| entities | 1.0.1 | MIT |
| fastrand | 2.5.0 | Apache-2.0 OR MIT |
| fearless_simd | 1.2.0 | Apache-2.0 OR MIT |
| fearless_simd_macros | 0.1.0 | Apache-2.0 OR MIT |
| finl_unicode | 1.5.1 | (MIT OR Apache-2.0) AND Unicode-DFS-2016 ← **deny 唯一缺口，本批补行** |
| memchr-n | 0.1.10 | MIT OR Apache-2.0 |
| phf 0.14.0（无后缀目录） | 0.14.0 | MIT |
| phf_shared 0.14.0（无后缀目录） | 0.14.0 | MIT |
| phf-0.13.1（拆分目录） | 0.13.1 | MIT（既有 tokio-postgres 链） |
| phf_shared-0.13.1（拆分目录） | 0.13.1 | MIT（既有 tokio-postgres 链） |
| phf_codegen | 0.14.0 | MIT |
| phf_generator | 0.14.0 | MIT |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT |
| typed-arena | 2.0.2 | MIT |

（finl_unicode 版本：任务书 A12 预估 1.4.0，实测 1.5.1——comrak 1.1.0 实拉值，见 §8 坏消息 8。）

## 6. 三格式一致性对账数字（判据 7 实测，tests/consistency.rs 全绿）

| 对账项 | Markdown | HTML | JSON(native) | SARIF |
|---|---|---|---|---|
| finding 数（main fixture） | 2（`### find-` 标题计数） | 2（`class="finding "` 计数） | 2（findings[].len） | 2（runs[0].results.len） |
| finding_id（find-20261123-001/002） | 出现 | 出现 | findings[].finding_id 精确相等 | ruleId 精确相等 |
| title | 出现 | 出现 | 精确相等 | message.text 精确相等 |
| severity | 出现 | 出现 | 精确相等 | level 映射精确（high→error/medium→warning） |
| `rtis://poc/42` | 出现 | 出现 | findings[0].rtis_ref | properties.rip.rtis_refs[0] 精确相等 |
| 六 ID 四词形（re-20261123-001 / artifact-20261123-003 / toolrun-20261123-007 / trace-20261123-abc123） | 证据清单逐行出现 | 证据块逐行出现 | findings[0].evidence[] | properties.rip.six_ids 四键 + artifacts.uri+sha-256 对账 |
| content_hash | 出现 | 出现 | 精确相等 | artifacts.hashes["sha-256"] 精确相等 |
| mixed fixture 五态五级 | 渲染通过 validate_ir | 同 | 同 | level/kind 映射 5×2 精确对账 |

## 7. IR 冻结面声明（6b 前置输出——§8⑥ 核心）

- **schema 文件**：`schemas/report-ir/report-ir-v1.schema.json`，sha256 = `888e9210f1ba44a5706439adb7a486ee75f8c4b6cc3c871c6184b5eb577a499e`（C19 行实测）
- **$id**：`research-platform/schemas/report-ir/report-ir-v1.schema.json`（document-ir 先例同构词形）
- **字段清单快照**：
  - 顶层 16 键（required 6）：`schema_version`(const "1") / `report_id`(rep- 前缀) / `report_kind`(三值词表) / `title` / `generated_at`(ISO8601) / `language`(default zh) / `task_ref{task_id?,workflow_run_id?}` / `executive_summary`(md) / `scope{authorization?,assets[]}` / `methodology`(md) / `timeline[{date,event}]` / `coverage{dimensions[{dimension(七维词表),total,tested,confirmed}]}` / `findings[]` / `attack_paths[{path_id,title,steps[],evidence_refs[]}]` / `appendix`(md) / `properties`(自由扩展)
  - FindingIr 18 字段（required 5）：`finding_id`(find- 前缀) / `rtis_ref`(rtis://poc/<id>，可选) / `title` / `severity`(五值词表) / `result_state`(五态词表) / `location{artifact_uri?,start_line?,end_line?,snippet?}` / `vuln_type` / `affected_scope[]` / `preconditions`(md) / `principle`(md) / `repro_steps`(md) / `request_response[{direction(request|response),content}]` / `evidence[]` / `impact` / `remediation{priority(四值词表),suggestions[]}` / `technical_explanation` / `plain_explanation` / `properties`
  - EvidenceRef 7 字段（required 6）：`evidence_id`(re- 前缀) / `artifact_id` / `tool_run_id` / `trace_id` / `uri`(ripartifact://|rtis://) / `content_hash`(sha256 hex64) / `evidence_type?`(三值词表)
  - 词表五处枚举：report_kind / severity / result_state / evidence_type / remediation.priority + coverage.dimension 七维（022 直投影）
- **四函数签名钉定**（crates/report-engine 公开面）：
  ```rust
  pub fn render_markdown(ir: &ReportIr) -> String
  pub fn render_html(ir: &ReportIr) -> String
  pub fn render_json(ir: &ReportIr) -> serde_json::Value
  pub fn render_sarif(ir: &ReportIr) -> serde_json::Value
  pub fn validate_ir(instance: &Value) -> Result<(), String>
  pub fn from_json(instance: &Value) -> Result<ReportIr, String>
  ```
- **演进规则**：后续批禁改字段语义，只 additive（04 号冻结物先例词形）；SARIF level/kind 映射词形与 `properties.rip.*` 键面为本批冻结面的一部分

## 8. 坏消息单列（九项，坐实 + 处置）

1. **docx-rs 停批撤除（D3 裁决修订）**：双 crate 计划中止于 deny advisories 门——docx-rs 0.4.22（crates.io latest）硬依赖 zip ^8.6.0，6 条 vulnerability error 级 RUSTSEC（patched ≥9.0.0 全堵死），deny [advisories] ignore 违红线 2。裁决 B 半撤保 comrak。判据 12 顺延 6b。**侦察教训进必答项：vendor 候选侦察必须跑 deny advisories probe**（本批探针 /tmp/p6a-probe 漏跑此面——advisory DB 2026-11-19 已可查，侦察时点可发现）。
2. **lock 数字漂移（双 crate 口径）**：任务书 D3② 预估 442/+25，双 crate 实测 445/+28——根因 = phf 0.14.0/phf_shared 0.14.0 与既有 0.13.1 多版本共存（探针对账按包名粗了一格，lock 按 (name,version) 对计）+ report-engine 自身条目。裁决 B 终态 432/+15。
3. **vendor「既有目录零修改」条款事实修订**：cargo vendor 多版本命名规则（高版本占无后缀名）→ 既有 `vendor/phf`/`vendor/phf_shared` 目录内容 0.13.1→0.14.0 置换（M 状态 19 文件 + 2 新文件）。既有 392 目录中其余 390 目录零修改零删除坐实；phf 两目录置换为 comrak 实拉 0.14.0 的不可避结果（tokio-postgres 链 0.13.1 不可删、comrak 1.1.0 钉 0.14）。
4. **任务书基线 1337 → 实测 1339**：S1 workspace 全量实测 1339 passed 零红（HEAD 1b5836f3 与任务书写作同日同链；+2 为口径漂移）。收口态 1365 passed = 1339 + report-engine 26 精确闭合（awk `/^test result:/` 汇总口径，failed=0）。
5. **判据 14「生产库五文件 user_version = 30」钉值过时**：实测五文件 = 1/1/1/0/0（platform.db 17 表 M4+M5 时代 / platform-governance.db 1 表 / platform-search.db 7 表——各文件时代基线值；30 只属 dev-b1.db 64 表）。任务书写作时点未逐文件实测。处置：selftest_p6a.sh 按真实指纹值钉（1/1/1/0/0），全程 mode=ro 零写入，批次报告如实登记。
6. **判据 13「fmt ≤936 hunks」钉值过时**：HEAD 态（git worktree 实测）= 943 hunks——1b5836f3 在链面自身漂移 +7，与本批无关（本批新代码 fmt 自净 grep=0，既有文件零 diff）。真实门 = 943 恒定本批零贡献。
7. **report-engine 测试计数口径**：26 = 四个 integration test 面（consistency 7 + html_escape 5 + negative 9 + sarif_structure 5），crate 内零 #[cfg(test)] 单测——测试全走 fixtures 驱动的 e2e 面（D6 fixture 驱动语义），计数与 workspace 累计精确闭合。
8. **comrak 链版本面微漂**：finl_unicode 实拉 1.5.1（任务书 A12 预估 1.4.0）；memchr-n 0.1.10；phf 0.14.0——comrak =1.1.0 exact 钉下的传递版本以 vendor 实装为准。
9. **SARIF kind=review 时携带 level 的规范取舍**：严格 SARIF 2.1.0 建议 kind≠fail 时省略 level；判据 8 要求 severity→level 逐 Finding 对账可见 → 按 D1 映射表字面全量携带。差距登记：6b 导出质检批（§8⑦）vendor 官方 SARIF schema 校验时对表收口。

## 9. 判据 5/6 对账表（字段/签名 grep 逐条）

**判据 5 字段对账**（D1 清单 vs schema，tests/negative.rs schema_enums_match_rust_wordlists 钉词表单源）：

- 顶层：schema_version/report_id/report_kind/title/generated_at/language/task_ref/executive_summary/scope/methodology/timeline/coverage/findings/attack_paths/appendix/properties = 16 键（判据词「顶层 12」= 12 内容字段 + 4 容器位口径）
- FindingIr：finding_id/rtis_ref/title/severity/result_state/location/vuln_type/affected_scope/preconditions/principle/repro_steps/request_response/evidence/impact/remediation/technical_explanation/plain_explanation/properties = 18 字段（判据词「19」与 D1 清单 18 项差 1 = 计数口径漂移，按 D1 清单词形全落）
- EvidenceRef：evidence_id/artifact_id/tool_run_id/trace_id/uri/content_hash/evidence_type = 7 字段 ✓

**判据 6 签名反证**：

```
grep -n "render_markdown\|render_html\|render_json\|render_sarif" crates/report-engine/src/lib.rs
  → pub use render::html::render_html; / json::render_json / markdown::render_markdown / sarif::render_sarif（4 行）
grep -rn "std::fs\|std::process\|Command\|&mut" crates/report-engine/src/render/ → 0 命中（四渲染零 I/O 反证）
grep -rn "unwrap()" crates/report-engine/src/ → 0 命中（expect 仅 render_json serialize infallible 一处）
```

## 10. 观察池八面（D7 全留 6b，本批零实现零触碰）

| # | 面 | 6b 输入 |
|---|---|---|
| 1 | PDF 面全零（typst 选型未立项） | PLAN :537 只列格式未指定引擎；6b 立项选型 |
| 2 | DOCX 全量渲染器 | docx-rs 已撤（zip RUSTSEC）——重新侦察 zip 9.x 兼容 crate（zip 9.8.1 在 crates.io）或 docx-rs 升级版；侦察必答项：deny advisories probe |
| 3 | Diagram Engine（:531 §8.4） | IR attack_paths 字段位已承载引用面 |
| 4 | 双解释生成器（:525 §8.3 七问） | IR technical_explanation/plain_explanation 字段位已承载 |
| 5 | 导出质量检查（:543 §8.6） | 含官方 SARIF schema vendor 候选与本批结构断言面的差距收口（坏消息 9） |
| 6 | IR 装配面（读库构造 ReportIr） | verification-engine store / agent-engine evidence → ReportIr；IR 冻结面 §7 为输入契约 |
| 7 | reports PG 落库 | migrations_pg V0008 立项候选（report_ir jsonb + format_exports）；035 不立项结论不变 |
| 8 | rp_server 端点 / web 报告页 / PPTX/XLSX | rp_server.py 6282 行零 diff 保持 |

## 11. 复现（收口态复核命令与预期回显）

```bash
cd /media/emo/code/pensting/research-platform
grep -cE '^\[\[package\]\]' Cargo.lock          # 432
grep -c 'name = "reqwest"' Cargo.lock           # 1
grep -c 'name = "zip"' Cargo.lock               # 0
ls vendor/ | wc -l                              # 406
cargo deny check advisories                     # ok（exit 0）
cargo deny check licenses                       # ok（exit 0）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test -p report-engine --offline -- --test-threads=1
                                                # 26 passed 0 failed
bash apps/server/selftest_p6a.sh                # PASS=25 SKIP=0 FAIL=0
for f in migrations/platform.db migrations/platform-dryrun.db migrations/platform-fullbatch.db migrations/platform-governance.db migrations/platform-search.db; do sqlite3 "file:$f?mode=ro" "PRAGMA user_version"; done
                                                # 1 1 1 0 0（时代基线值；全程 mode=ro）
sha256sum schemas/report-ir/report-ir-v1.schema.json
                                                # 888e9210f1ba44a5706439adb7a486ee75f8c4b6cc3c871c6184b5eb577a499e
git status --porcelain -- crates/wsw-runtime/ crates/verification-engine/ crates/agent-engine/ web/ workers/ integrations/rtis/ migrations/ migrations_pg/ .github/ | wc -l   # 0
```
