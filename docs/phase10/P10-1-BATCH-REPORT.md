# 批次验收报告：P10-1（Phase 10 第 1/4 批：可靠性基座，backup/restore、FTS 重建校验、数据保留归档）

| 项 | 值 |
| --- | --- |
| 状态 | 已实施并验收（本报告即验收快照） |
| 日期 | 2026-11-14 |
| 批次 | P10-1（第 1/4 批），覆盖 ROADMAP §11.3 P10-5 与 SESSION-HANDOFF §十三项判据 |
| 数据库迁移 | 0 个新增（本批零迁移；迁移下一可用编号仍为 020，`ls migrations/*.sql` 实证 001..019） |
| 生产库 | migrations/platform*.db 五个库（platform / platform-dryrun / platform-fullbatch / platform-governance / platform-search）指纹前后一致零写入（selftest 4d 步骤 PASS） |
| 第三方依赖 | 0 个新增 vendor（rusqlite 启用既有 `backup` feature；tar 0.4 / sha2 0.10 / data-encoding 2 / serde_json 均为已 vendor 既有面；`git status` 实证 vendor/ 零新增，Cargo.lock 仅 +5 行依赖边） |
| 任务书 | docs/phase10/P10-1-TASK.md（七小节：目标范围 / 备份设计 / 归档设计 / 选型决策 / 实施清单 / 验收纪律 / 风险回滚） |

## 判据逐条（全部本机实测）

1. **backup/restore 真实实演（备份 -> 受控破坏副本 -> 恢复 -> 校验一致）**：`bash apps/server/selftest_phase10_backup.sh` exit 0（PASS=16 FAIL=0）。四步命令原文与回显摘要：
   - 备份：`cp migrations/platform.db /tmp/rp_p10_backup.XXXX/platform.iso.db` 后 `target/debug/rp-admin backup --db <隔离副本> --out p10-backup.tar --extra rip-data/tool-runs.db`，回显 JSON：`user_version=1`（与源库实际值一致，platform.db 迁移链记录走 schema_version 表，user_version 停在 1）、`db_bytes=3751936`、`extra_count=1`；`rp-admin verify-archive` 回显 manifest db_sha256 与备份报告一致。
   - 受控破坏：`python3` 对副本中段（offset 1875968）覆写 8192 字节 0xFF，`sha256(damaged)=a23a6213...` 与 manifest `db_sha256=aec4a45a...` 偏离留证；副本 `PRAGMA quick_check` 回显 `*** in database main ***`（完整性探测确认损伤）。
   - 恢复：`target/debug/rp-admin restore --archive p10-backup.tar --db <受损副本> --extras-out <dir>` 回显 `integrity=ok`；恢复后文件 sha256 与备份快照逐字节一致；附加文件（tool-runs.db）恢复 sha256 与原件一致。
   - 校验：四路对账见 3 到 6 条。
2. **Rust 集成测试（roundtrip、篡改拒绝、缺成员拒绝、retention 对账）**：`crates/database/tests/backup_restore.rs` 四用例全过：真实迁移链（001..019）载具上备份/破坏/恢复 roundtrip（行数、user_version、抽样中文描述、附加文件逐项一致，篡改归档被 sha256 门禁拒绝且目标库零改动）；retention 用例（external_api_audit 旧行 2 条入档、archived=deleted=2、归档库 DDL 复刻行数=2）。
3. **点名门禁**：`cargo test --offline -p database -p search-engine` exit 0，12 个测试二进制 43 passed 0 failed（本批新增 6：backup_restore 4 + rebuild_after_restore 2）。
4. **恢复后 FTS 全量 rebuild 对账**：selftest 4a，`rp-search-index build`（隔离副本 + M8 快照 catalog.db 只读）基线索引 doc_count/fts_rows = 81/81；对受损副本 `rp-admin restore` 后 `rp-search-index build --rebuild`（恢复副本）= 81/81，行数一致 PASS；抽样关键词（实测选中 `shell`）对两个索引 query 命中集 diff 为空（4 条命中一致）PASS。Rust 侧 `crates/search-engine/tests/rebuild_after_restore.rs` 双用例（行数/文档行集/ASCII+CJK 抽样命中一致 + 破坏副本恢复后 rebuild 回到基线）全过。
5. **健康口径前后一致**：点名判据 `python3 -I scripts/rip_etl_status.py`（默认口径，生产 platform.db 只读）演练前后各跑一次，输出均为 `platform.db: quick_check=ok | resources=81 evidence=700 tasks=42`，diff 为空 PASS。隔离副本口径用同一查询集的内联实现（`db_health`）：备份前、恢复后均为 `quick_check=ok | resources=81 evidence=700 tasks=42`，diff 为空 PASS。
6. **verify.py 口径对账**：`RP_TARGET_DB=<工作目录基线副本>` 与 `RP_TARGET_DB=<备份后受损再恢复的全量批副本>` 各跑一次 `python3 -I migrations/verify.py`，stdout 与 stderr 逐字节 diff 为空 PASS（[PASS] 项 55 项，两侧 exit=1 对称崩溃于同一检查点）。环境限制如实声明：evidence 全量对账段需 26 个活动源库（/home/emo/.dsh/redteam/engagements/），本机不存在（M8 快照仅锁定 catalog/knowledge/tasks，2026-11-06 生成）；未做本批改动前基线实测同样崩溃于 `verify_evidence` 打开源库处（`sqlite3.OperationalError: unable to open database file`），与本批无关，故本批对账口径取「恢复副本与基线副本逐项一致」。
7. **数据保留归档**：selftest 4b，真实 019 DDL 建隔离库灌 2 旧（2020）+ 2 新（2099）行，`target/debug/rp-admin reten-archive --db <隔离库> --archive-out audit-archive.db`（内置策略 external_api_audit 保留 180 天）回显 JSON：`cutoff=2026-05-18T07:39:32Z`、`archived=2 deleted=2 rows_after=2`；归档库行数=2（DDL 复刻 + 索引复刻）PASS。
8. **生产库零写红线**：selftest 前后对 migrations/platform*.db 五库逐文件 sha256 指纹 diff 为空 PASS。
9. **全仓守卫**：`cargo test --offline --workspace` exit 0（615 passed，0 failed，含 tool-engine first_batch_b2 受保护脚本守卫 2 passed）；`cargo clippy --offline --workspace --all-targets -- -D warnings` exit 0。

## 执行中修正记录（如实声明）

1. **受保护脚本守卫（设计冲突，按守卫纪律收口）**：初稿给 `scripts/rip_etl_status.py` 加了 RP_RUN_DB 覆盖，`cargo test --workspace` 实测被 tool-engine `first_batch_b2::b2_protected_scripts_untouched_by_tests` 拒绝（该守卫断言 scripts/ 不得有跟踪态修改，rip_etl_status.py 在保护清单）。处置：还原该脚本（本批零改动），点名判据按默认口径（生产库 mode=ro 只读）前后各跑一次，隔离副本健康口径用同一查询集内联实现（selftest `db_health`）。守卫测试复位后 2 passed。
2. **rustfmt 版本漂移**：`cargo fmt --all` 对 40 个本批未触碰文件产生重排（本机 rustfmt 与 HEAD/CI 侧版本格式不一致，HEAD 由 CI 校验通过；`cargo fmt --all -- --check` 在 HEAD 态实测 308 处 diff）。处置：全部还原（`git checkout --`），保持批次 diff 范围；本批新文件按本机 rustfmt 归一。`cargo fmt --all -- --check` 在本机对未触碰文件仍有 diff，属环境漂移非本批引入（任务书 §七.7 已记录）。
3. **Online Backup API 快照等价性**：快照与源文件不逐字节相同（目的库头部字段如 change counter 归位），roundtrip 断言从「快照 sha == 源文件 sha」修正为「恢复产物 sha == manifest db_sha256 + 行数/抽样对账」，与 API 语义一致。
4. **user_version 断言**：platform.db 的 `user_version` 实际为 1（迁移链记录走 schema_version 表），selftest 断言从硬编码 19 修正为与源库实际值对账；Rust 载具（fresh 迁移 001..019）仍断言链尾 19。

## 未执行的测试

- verify.py evidence 全量对账段：26 个活动源库本机缺失（既有环境限制，基线态相同），无法执行，已以「恢复副本与基线副本逐项一致」口径替代并如实标注。
- zstd 压缩面：未引入（非判据；归档完整性由逐成员 sha256 承担，任务书 §四留 P10-5 收口批次评估）。
- `cargo fmt --all -- --check`：未作为本批门禁（本机 rustfmt 与 HEAD 漂移，见修正记录 2），本批新文件已按本机 rustfmt 归一。

## 对 ERI / RTIS / KB / Skills / Tools 的影响

- RTIS 本体零改动（红线）；ERI/KB 无接触；Tool Registry 登记面零改动；scripts/ 三个受保护知识脚本零改动（守卫测试复位实证）。
- 生产 migrations/platform*.db 与 RTIS 生产行为零接触（指纹核对 + 全部实演走 mktemp 隔离副本）。

## 风险与已知限制

见任务书 §七（快照头部字段差异、verify.py 证据段环境限制、restore 离线操作契约、retention rowid 依赖与入站外键守卫、rustfmt 漂移）。

## 回滚方式

- 代码：`git revert <批次 commit>` 整批撤销；backup.rs / retention.rs / rp-admin.rs / 两测试 / selftest / phase10 文档为整文件新增，revert 即移除；rusqlite `backup` feature 为纯开关回退零风险。
- 数据：全部实演走 mktemp 工作目录（脚本 EXIT trap 自清理）；生产库零写入，无生产回滚面。

## 后续工作

- P10-2（第 2/4 批，按编排任务书执行）。
- P10-5 收口批次（发布文档面）：备份/恢复/迁移/升级四文档定稿时引用本批 CLI 形态，按需评估压缩面增量 vendor。
