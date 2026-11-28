# RIP 1.0 灾难恢复

> 状态：恢复程序和已测边界。日期：2026-11-28。

## 一、恢复目标

优先恢复权威数据和运行控制面，再重建派生索引。已知备份范围包括平台 SQLite、工具运行库、artifact/plugin 数据以及配置。派生搜索索引应重新构建。

## 二、恢复前检查

1. 停止或隔离所有写入方。
2. 核对归档成员、manifest 和 sha256。
3. 在隔离副本恢复并运行完整性检查。
4. 验证 migration 版本、关键表计数和业务查询。
5. 只有恢复副本通过后才替换生产路径。

## 三、恢复步骤

完整命令见 [BACKUP-RESTORE.md](../guides/BACKUP-RESTORE.md)。

```bash
target/debug/rp-admin verify-archive --archive backups/rip-backup.tar
target/debug/rp-admin restore --archive backups/rip-backup.tar \
  --db migrations/platform.db --extras-out /tmp/rip-extras
```

恢复完成后重建搜索派生索引并重启服务。

## 四、PostgreSQL 正式知识库恢复

正式知识库 `rip_platform` 不使用 SQLite 归档替代恢复。运维必须保留独立、
加密或受最小权限保护的自定义格式 PostgreSQL dump：

```bash
install -d -m 700 backups/postgresql
pg_dump --format=custom --no-owner --no-acl \
  --dbname="$RP_PLATFORM_PG_DSN" \
  --file="backups/postgresql/rip-platform-$(date +%Y%m%d-%H%M%S).dump"
chmod 600 backups/postgresql/rip-platform-*.dump
```

生产备份频率至少每天一次，密钥轮换前后至少保留一次可恢复快照；
异地保存策略由部署环境签收。数据库备份与应用配置分开保存，
备份目录不得进入 git、public artifact 或公开日志。

恢复前先停写、创建独立测试数据库和低权限恢复角色，在恢复副本中执行：

```bash
createdb -T template0 rip_platform_restore
pg_restore --exit-on-error --no-owner --no-acl \
  --dbname="postgresql://USER:PASSWORD@127.0.0.1:55432/rip_platform_restore" \
  backups/postgresql/rip-platform-<timestamp>.dump
```

恢复副本至少核对：

- PostgreSQL migration/user-version 和 schema 对账。
- knowledge、entity/relation、source、evidence、review batch 和审计记录计数。
- known workspace 隔离查询和关键外键完整性。
- 代表性知识卡、来源、证据、待审核批次和审计记录可读。
- 与恢复前记录的 sha256、计数和业务快照一致。

只有恢复副本通过，才停止生产写入并切换已验证副本或按维护窗口恢复原库。
恢复失败不得执行删除式 down-migration 回退；回滚方式是恢复升级前快照。

SQLite 和 PostgreSQL 不要求跨库瞬时事务一致。恢复顺序应先恢复 PostgreSQL
权威治理数据，再恢复 SQLite 运行兼容库，最后重建派生搜索索引并做联合对账。

## 五、已验证的恢复证据

- SQLite 备份和恢复门通过。
- PostgreSQL disposable database 完成 Phase 7 集成、source monitor 和 backup/recovery roundtrip。
- `platform.db` 不可只靠备份命令成功判定恢复，必须做实际恢复和对账。
- `scripts/rip_pg_recovery_gate.py` 是迁移和恢复门禁载具，不是正式生产库备份制度。
- 正式 `rip_platform` 的定时备份、异地保存和驻留策略仍需由部署环境签收。
- 2026-11-28 隔离演练：在 `rip-pg17` 上创建 `rip_dsn_drill_src` 和
  `rip_dsn_drill_restore`，执行 custom dump、恢复副本和行数对账，恢复结果为 2/2。
- 同次演练启动端口 7815 的真实 user systemd 服务，`/api/review-batches`
  注入 PostgreSQL DSN 后返回 HTTP 200，演练后已卸载服务并删除两数据库和临时文件。

## 六、回滚约束

迁移不采用 down script 风险回退；恢复使用升级前快照。插件升级失败时保留旧 active 指针，镜像升级失败时恢复旧 digest。

如果恢复后搜索、工具或工作流错误率异常，先回滚二进制和入口，不直接删历史数据。
