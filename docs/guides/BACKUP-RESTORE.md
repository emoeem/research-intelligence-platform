# BACKUP-RESTORE：备份与恢复（P10-4，ROADMAP P10-6 四文档之一）

> 状态：生效中；日期：2026-11-14；适用版本：Phase 10 批次工作树（未 commit）。
> 本文命令在维护机与 fresh clone 演练均已实跑（P10-1 批次 roundtrip 实演 + P10-4
> fresh clone 演练记录，见 docs/phase10/P10-4-BATCH-REPORT.md 第五节）。
> 相关：docs/guides/INSTALL.md、docs/guides/MIGRATION.md、docs/guides/UPGRADE.md、
> crates/database/src/backup.rs（实现）、apps/server/selftest_phase10_backup.sh（自验脚本）。

## 一、备份范围与归档形态

| 范围 | 载体 | 说明 |
| --- | --- | --- |
| 平台主库（platform.db 等） | `rp-admin backup` Online Backup API 快照 | 源库只读打开零写入，WAL 一致快照 |
| 附加文件（rip-data/tool-runs.db 等） | `--extra FILE`（可多个） | 逐成员 sha256 入 manifest |
| Artifact 与插件安装目录（rip-data/artifacts、rip-data/plugins） | rip-data 树随附打包 | 运行时数据不入库（.gitignore），备份时另行打包目录 |
| FTS 索引（/tmp/idx.db 等派生物） | 不备份 | 可由 `rp-search-index build` 全量重建（第五节） |

归档为 tar：成员 `db/platform.db`（快照）+ extras + `manifest.json`
（`format_version`（当前 1）、`db_sha256`、`db_bytes`、`user_version`（迁移链断言用）、
`extras` 逐条目 sha256）。实现与字段定义：crates/database/src/backup.rs
（`ARCHIVE_FORMAT_VERSION = 1`，`DB_MEMBER = "db/platform.db"`，`MANIFEST_MEMBER = "manifest.json"`）。

## 二、备份命令

~~~bash
cargo build --offline -p database --bin rp-admin
target/debug/rp-admin backup --db migrations/platform.db --out backups/p10-backup.tar \
  --extra rip-data/tool-runs.db
# 输出单行 JSON：db_sha256 / db_bytes / user_version / extras
# rip-data 目录树（artifacts/plugins）另行：
tar -czf backups/rip-data-$(date +%Y%m%d).tar.gz rip-data/
chmod 600 backups/*.tar backups/*.tar.gz     # 备份含业务数据，最小必要保存
~~~

## 三、备份校验（不替换目标库）

~~~bash
target/debug/rp-admin verify-archive --archive backups/p10-backup.tar
# 输出 JSON 回显 manifest 的 db_sha256/size/user_version，校验 tar 成员完整
# 与源库 sha256 对账一致（sha 不同 = 库在备份后有写入，重备）
~~~

## 四、恢复命令（离线契约：目标库不应有活跃写入方）

~~~bash
target/debug/rp-admin restore --archive backups/p10-backup.tar \
  --db migrations/platform.db --extras-out /tmp/extras-out
# 解包校验（逐成员 sha256/尺寸/integrity_check/user_version）后原子替换目标库；
# extras 落 /tmp/extras-out（按原相对路径），手工放回 rip-data/ 对应位置
~~~

恢复是整库原子替换：先校验全部成员 sha256 与完整性，再替换；校验不过不落盘。

## 五、恢复后对账（纪律：恢复前后计数一致才算恢复完成）

~~~bash
# 1. 健康口径（quick_check + resource/evidence/research_task 三计数，恢复前后一致）：
python3 - <<'EOF'
import sqlite3
c = sqlite3.connect("file:migrations/platform.db?mode=ro", uri=True)
print(c.execute("PRAGMA quick_check").fetchone()[0],
      c.execute("SELECT COUNT(*) FROM resource").fetchone()[0],
      c.execute("SELECT COUNT(*) FROM evidence").fetchone()[0],
      c.execute("SELECT COUNT(*) FROM research_task").fetchone()[0])
EOF
# 2. FTS 全量重建对账（恢复副本 rebuild 行数与抽样命中和基线索引一致）：
target/debug/rp-search-index build --rebuild --platform-db migrations/platform.db \
  --catalog-db migrations/sources-snapshot/catalog.db --index-db /tmp/idx-after.db
target/debug/rp-search-index query --index-db /tmp/idx-after.db --keywords <抽样词> --limit 100
# 3. verify.py 对账（对账口径沿 verify.py 纪律，RP_TARGET_DB 可换目标库）：
RP_TARGET_DB=migrations/platform.db python3 -I migrations/verify.py
~~~

## 六、数据保留归档（策略驱动，external_api_audit 保留 180 天）

~~~bash
target/debug/rp-admin reten-archive --db migrations/platform.db --archive-out backups/retention.db
# 默认内置策略：external_api_audit 表按 ts 保留 180 天，旧行入档后删除，
# 报告逐策略回 archived/deleted/rows_after 行数对账
target/debug/rp-admin reten-archive --db migrations/platform.db \
  --archive-out backups/retention.db --policies-json policies.json
# 自定义策略数组字段：table/time_column/keep_days/extra_guard_sql/max_archive_rows
~~~

## 七、演练记录

| 演练 | 环境 | 结果 |
| --- | --- | --- |
| P10-1 roundtrip 自验（备份 → 受控破坏中段 8KB → 恢复 → 四路对账：健康口径/FTS rebuild 行数与命中集/verify.py 口径/保留归档） | 维护机（生产库副本隔离） | `bash apps/server/selftest_phase10_backup.sh` exit 0（docs/phase10/P10-1-BATCH-REPORT.md） |
| P10-4 fresh clone roundtrip（备份 dryrun 库副本 → 中段 4KB 破坏 → 恢复 → 前后 81/23/42 计数一致 + extras sha 一致） | fresh clone（无生产库） | 手工命令序列 exit 0（批次报告第五节；本文第二至五节即该序列） |

## 八、已知限制

1. `apps/server/selftest_phase10_backup.sh` 的健康口径与 verify.py 段依赖维护机生产库
   （migrations/platform.db 的 81/700/42 计数与 26 个活动源库），fresh clone 无这些数据，
   该脚本在 fresh clone 环境 FAIL 属预期（fresh clone 用本文第二至五节命令序列 +
   ETL dryrun 库演练，见第七行第二行）。
2. Online Backup API 快照对同刻多库（platform.db + tool-runs.db）非跨库一致快照：
   两者各自一致，跨库事务边界外的写入窗口由 extras sha256 对账兜底。
3. 备份文件不入仓库（.gitignore + DOC-STANDARD 无生产数据入库纪律）；令牌与密钥永不入备份文档。
