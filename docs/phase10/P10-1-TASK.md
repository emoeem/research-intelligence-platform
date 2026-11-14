# 批次任务书：P10-1（Phase 10 第 1/4 批，可靠性基座：备份/恢复、FTS 重建校验、数据保留归档）

| 项 | 值 |
| --- | --- |
| 状态 | 已实施并验收（见 P10-1-BATCH-REPORT.md） |
| 日期 | 2026-11-14 |
| 判据来源 | docs/phase0/PHASE-ROADMAP.md §11.3 P10-5 行；docs/SESSION-HANDOFF.md §十「Phase 10 新增」三项（backup/restore API、FTS 重建校验、数据保留归档） |
| 相关 | docs/phase4/P4-1-BATCH-REPORT.md（迁移载具先例）；migrations/verify.py（对账口径）；scripts/rip_etl_status.py（健康口径） |

## 一、批次目标与范围

Phase 10 首批补齐声明未实现的可靠性能力（盘点结论「backup()/restore() 是声明未实现的能力，P10-5 是最后实现窗口」）：

1. `database` crate 补 `backup()` / `restore()`：真实实演「备份 -> 受控破坏副本 -> 恢复 -> 校验一致」全链。
2. 恢复后 FTS 全量 rebuild 对账校验：恢复副本重建索引，行数、文档行集、抽样命中与基线索引逐项一致。
3. 数据保留归档策略落地：策略驱动的旧行入档（独立归档库，DDL 复刻，行数对账），CLI 可执行。

非目标（留给后续批次）：备份调度与轮转策略（P10-5 收口批次定稿命令形态）、Artifact 存储的对象级增量备份（本批以「附加文件」维度覆盖单文件场景）、压缩面（见第四节）。

## 二、设计：备份归档格式与恢复语义

备份 = 两段式：先快照后打包。

1. 快照：rusqlite `backup` feature（Online Backup API，`Backup::run_to_completion`）把源库按页复制出 WAL 一致的独立快照。源连接以 `SQLITE_OPEN_READ_ONLY` 打开，对源库零写入（红线 ROADMAP §13 的前提）。快照先落归档同目录临时文件（`TempFileGuard` 看护，错误路径不泄漏）。
2. 归档：tar 容器（`tar` 0.4，与 plugin-registry P6-4 同款无压缩口径）。成员三个维度：
   - `db/platform.db`：库快照（固定逻辑名 `DB_MEMBER`，与源库文件名无关）；
   - `extra/<文件名>`：附加文件（备份范围含 rip-data、Artifact 存储时逐文件加入；重名拒绝，非普通文件拒绝）；
   - `manifest.json`：清单（`format_version`、`created_at_unix_ms`、逐成员 sha256 + 字节数、`user_version`、extras 表）。
3. 恢复（`restore_from_archive`）：解包到目标同目录暂存目录 -> 成员名安全校验（拒绝绝对路径与 `..`，归档按不可信输入处理）-> 逐成员 sha256 与尺寸对账 -> 库快照 `PRAGMA integrity_check` 必须为 `ok` 且 `user_version` 与清单一致 -> 同文件系统临时名 + rename 原子替换目标库 -> 清理旧 `-wal`/`-shm`/`-journal` 侧车。全部通过前目标库零改动（测试固化：篡改归档被拒时目标库 sha 不变）。
4. 只读校验入口 `verify_archive`：同一解包校验路径，不替换任何文件（CLI `verify-archive`）。

恢复是离线操作契约：目标库不应有活跃写入方（sidecar 清理窗口的文档化约定，写入方在库时的语义不在本批范围）。

## 三、设计：数据保留归档（retention.rs）

1. 策略结构：`RetentionPolicy { table, time_column, keep_days, extra_guard_sql, max_archive_rows }`；截止时刻由 SQLite `strftime('%Y-%m-%dT%H:%M:%SZ','now','-N days')` 计算（成熟实现不手写日历算术），与行内 ISO8601 Z 文本逐字可比（`domain::time::now_iso` 同款格式）。
2. 执行语义：单连接 ATTACH 打通双库（main = 归档库，`rip_src` = 源库），一个事务内 `INSERT INTO 归档 SELECT * FROM 源 WHERE 守卫` + `DELETE FROM 源 WHERE 守卫`，`archived == deleted` 否则回滚报错；执行后再做 `before - deleted == after` 与归档库 `quick_check` 对账。
3. 归档库 schema：源表与索引的 `sqlite_master.sql` 原文复刻（含 CHECK 约束），表已存在则跳过 DDL（幂等）。
4. fail-closed 守卫四条：
   - 表不在源库：该策略记 `skipped`（一库多策略场景合法）；
   - 时刻列缺失或标识符非 `[A-Za-z0-9_]+`：Validation 错误；
   - 入站外键引用扫描：任何表的外键指向目标表即跳过该策略（连接级 `foreign_keys=OFF` 的补偿守卫，杜绝静默孤儿）；
   - `rowid` 不可用（WITHOUT ROWID）：跳过（批量上限依赖 rowid）。
5. 内置策略：`external_api_audit`（019_external_api_audit.sql，REST 网关访问审计，全 migrations 零入站外键，`ts` 带索引）保留 180 天。`tool_run` 不进内置集：被 `artifact.run_id` 引用且复刻 DDL 含对 `tool` 表的父引用，自定义策略由入站扫描拒绝并说明原因。
6. 自定义策略经 `--policies-json` 传入（数组，字段同序列化面），库层 API 恒为结构化 `RetentionPolicy`。

## 四、成熟库选型与依赖决策（复用优先纪律）

| 能力 | 选型 | 决策依据 |
| --- | --- | --- |
| 在线快照 | rusqlite `backup` feature（Online Backup API） | SQLite 官方一致快照语义；vendor/rusqlite/src/backup.rs 已在库，feature 门控为纯 Rust 模块，bundled libsqlite3-sys 内置完整 SQLite，零新增 vendor、零新增 C 依赖 |
| 归档容器 | `tar` 0.4（已 vendor 0.4.46，plugin-registry 同款） | 维护中的成熟 tar 实现不手写解析器；无压缩口径与 P6-4 fixture 一致（免引 flate2） |
| 成员校验 | `sha2` 0.10（已 vendor 0.10.9） | 不手写哈希 |
| hex 编码 | `data-encoding` 2（已 vendor 2.11.1） | `HEXLOWER`，不手写编码 |
| 清单序列化 | `serde`/`serde_json`（workspace 既有） | 不手写 JSON |
| 截止时刻 | SQLite `strftime` | 不手写日历算术 |
| 压缩 | 不引入（zstd/flate2 均未 vendor） | 归档完整性由逐成员 sha256 承担，压缩非本批判据；若后续需要，走增量 vendor 流程（保 --offline）并在 P10-5 收口批次评估，本批不阻塞 |

零新增 vendor 目录（git status 实证），Cargo.lock 仅追加 database 的新直接依赖边。

## 五、实施步骤与文件清单

1. `crates/database/src/backup.rs`（新增）：backup/restore/verify_archive 三入口 + manifest 结构 + 暂存守卫。
2. `crates/database/src/retention.rs`（新增）：策略结构 + builtin_policies + run_retention + 入站外键扫描。
3. `crates/database/src/error.rs`：`Backup(String)` / `Restore(String)` 两变体。
4. `crates/database/src/lib.rs`：模块登记与重导出。
5. `crates/database/src/bin/rp-admin.rs`（新增）：`backup` / `restore` / `verify-archive` / `reten-archive` 四子命令，JSON 输出，exit 0/2，帮助面口径与 rp-search-index 同款（无参 = usage 到 stderr + exit 2）。
6. `crates/database/Cargo.toml`：依赖登记 + `[[bin]]`；根 `Cargo.toml`：rusqlite 增 `backup` feature（注释写明理由）。
7. `crates/database/tests/backup_restore.rs`（新增）：真实迁移链（001..019）载具，四用例（roundtrip+受控破坏恢复、篡改归档拒绝、缺库快照成员拒绝、retention 入档对账）。
8. `crates/search-engine/tests/rebuild_after_restore.rs`（新增）：恢复副本 rebuild 对账（行数、文档行集、ASCII/CJK 抽样命中）。
9. `apps/server/selftest_phase10_backup.sh`（新增）：四步实演脚本（隔离库 + 生产库指纹守卫）。
10. `scripts/rip_etl_status.py` 零改动：该脚本是受保护脚本（tool-engine `first_batch_b2` 守卫 `b2_protected_scripts_untouched_by_tests` 断言 scripts/ 不得有跟踪态修改）。点名判据（恢复前后计数一致且 quick_check=ok）按默认口径（生产 platform.db，mode=ro 只读）演练前后各跑一次；隔离副本的健康口径用同一查询集（quick_check + resource/evidence/research_task 三计数）的内联实现（selftest 内 `db_health` 函数）。
11. `docs/phase10/P10-1-TASK.md`（本文件）与 `P10-1-BATCH-REPORT.md`。

## 六、验收命令与隔离纪律

验收命令（全部实测，输出见批次报告）：

```
cargo test --offline -p database -p search-engine
bash apps/server/selftest_phase10_backup.sh
python3 -I scripts/rip_etl_status.py            # 点名判据，默认口径（生产 platform.db 只读），演练前后各一次
RP_TARGET_DB=<工作目录副本> python3 -I migrations/verify.py     # 对账口径（副本上执行）
cargo clippy --offline --workspace --all-targets -- -D warnings
```

隔离纪律：备份、破坏、恢复、归档全部走 mktemp 工作目录与隔离副本；`migrations/platform*.db` 只读指纹核对（脚本前后各一次，diff 为空即零写）；verify.py 的 `RP_TARGET_DB` 只指向工作目录副本（verify.py 以读写模式 open 目标连接，生产库不进此路径）。

## 七、风险、已知限制与回滚

1. Online Backup API 快照与源文件不必逐字节相同（目的库头部字段如 change counter 归位），等价性判据落在恢复产物（sha 对账 + 行数/抽样对账），测试按此固化。
2. verify.py 证据段（evidence 全量对账）依赖 26 个活动源库（/home/emo/.dsh/redteam/engagements/），本机不存在（M8 快照只锁定 catalog/knowledge/tasks，2026-11-06 生成）；本批对账口径取「恢复副本与基线副本输出逐字节一致」，崩溃点两侧对称，属基线既有环境限制，与本批改动无关（基线实测同样崩溃于 verify_evidence 打开源库处）。
3. 恢复替换后清理旧 sidecar 存在文档化的离线操作契约（目标库有活跃写入方时不得执行 restore）。
4. retention 批量上限依赖 rowid（WITHOUT ROWID 表跳过）；归档库为结构复刻而非部分库语义，跨库父表不受 FK 保护（入站扫描守卫兜底，custom 策略作者自担 guard 语义）。
5. 平台库 `user_version` 停在 1（迁移链记录走 schema_version 表），备份清单按源库实际值对账，不硬编码链尾号（selftest 已按实际值断言）。
6. 回滚：`git revert <批次 commit>` 整批撤销；新增文件（backup.rs、retention.rs、rp-admin.rs、两测试、selftest、phase10 文档）随 revert 移除；rusqlite `backup` feature 回退零风险（纯开关）。
7. rustfmt 版本漂移：本机 rustfmt 与 HEAD 格式不一致（对未触碰文件也有 diff，HEAD 由 CI 侧 rustfmt 校验通过）；本批不重排未触碰文件（保持批次 diff 范围），本批新文件按本机 rustfmt 归一。
