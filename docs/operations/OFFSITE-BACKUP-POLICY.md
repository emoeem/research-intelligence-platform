# OFFSITE-BACKUP-POLICY（PB-005 · 2026-11-28）

状态：P2/Post-launch 策略正式化。当前生产数据量低（运营核 SQLite 数十 MB 级），
本政策在数据变贵或多用户前生效；脚本已可执行（见下），部署增长后按 cadence 挂定时即可。

## 数据分级与 RPO/RTO

| 库 | 内容 | RPO | RTO | 载体 |
|---|---|---|---|---|
| migrations/platform.db（运营核） | 任务/运行/审计 | 24h | 1h | rp-admin backup 归档 |
| migrations/platform-search.db | 检索投影 | 可重建（FTS rebuild） | 重建即得 | 不入异地（derived） |
| RTIS knowledge.db | 只读消费源 | 上游自有 | 上游自有 | 不复制 |
| PG 情报核（RIP3） | 文档/知识卡/证据 | 24h | 4h | pg_dump + rp-admin backup（extras） |
| rip-data/（对象库、插件、workspaces） | Artifact/Evidence 实体 | 7d | 4h | tar + sha256 manifest |

## 备份链（每日）

1. `rp-admin backup --db <db> --out <archive.tar> [--extra FILE]...`
   （在线快照 + 逐成员 sha256 manifest，源库只读零写入）
2. `rp-admin verify-archive --archive <archive.tar>`（不替换，纯校验）
3. 异地复制：`apps/server/selftest_offsite_backup.sh --offsite <target>`
   把归档同步到目标（rsync over ssh；部署时填 `[OFFSITE_HOST:PATH]`），
   并同步 sha256 清单；目标端至少保留最近 14 份日备 + 6 份月备。
4. 保留策略：`rp-admin reten-archive`（内置 external_api_audit 180 天等策略）。
5. 每季度 restore 演练：`rp-admin restore --archive --db <开发副本>` +
   integrity_check/user_version 校验 + 行数对账（先例：P10-1 fresh-clone roundtrip、
   PG template0 restore 2/2）。

## 凭据与传输红线

- 归档落盘权限 0600；异地传输走 ssh 密钥（非密码）；凭据不进仓库。
- 备份含凭据类证据（先例：platform-fullbatch.db 教训），异地桶/目录等同生产
  密级，禁公开可读。

## 执行入口

```bash
apps/server/selftest_offsite_backup.sh                        # 本地全链（vault 目录）
apps/server/selftest_offsite_backup.sh --offsite [OFFSITE_HOST:PATH]   # 真异地
```
