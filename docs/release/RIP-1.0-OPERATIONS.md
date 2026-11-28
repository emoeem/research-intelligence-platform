# RIP 1.0 运维手册

> 状态：现行运维入口汇总。日期：2026-11-28。

## 一、启动

主入口由 `apps/server/rp_server.py` 提供，默认监听 `127.0.0.1:7800`。完整安装步骤见 [INSTALL.md](../guides/INSTALL.md)。

```bash
apps/server/rp_service.sh gen-token
mkdir -p rip-data/dev
cat > rip-data/dev/rip-platform-dsn.env <<'EOF'
RP_PLATFORM_PG_DSN=postgresql://USER:PASSWORD@127.0.0.1:55432/rip_platform
EOF
chmod 600 rip-data/dev/rip-platform-dsn.env
apps/server/rp_service.sh install
apps/server/rp_service.sh status
```

审核批次和正式 PostgreSQL 治理功能需要在安装前配置 DSN：

```bash
mkdir -p rip-data/dev
install -m 600 /dev/null rip-data/dev/rip-platform-dsn.env
printf 'RP_PLATFORM_PG_DSN=postgresql://USER:PASSWORD@127.0.0.1:55432/rip_platform\n' \
  > rip-data/dev/rip-platform-dsn.env
chmod 600 rip-data/dev/rip-platform-dsn.env
```

不要把文件或 DSN 写入仓库、日志或文档。DSN 文件只能包含一行
`RP_PLATFORM_PG_DSN=...`，模式必须为 600。

正式 `rp_service.sh install` 不允许缺少有效 DSN，否则按原因退出：

- 1：DSN 文件不存在。
- 2：文件权限不是 600。
- 3：变量为空。
- 4：文件包含额外或不受支持的行。

开发环境可使用 `dev_up.sh`；缺少 DSN 时只允许 SQLite 工作台启动，
PostgreSQL 审核接口保持 503 fail-closed。

`rp_service.sh` 会把 600 权限文件作为 systemd `EnvironmentFile=-...` 注入。

同一配置也可供 `dev_up.sh` 使用；开发启动缺 DSN 时允许 SQLite 工作台启动，
正式 systemd 安装缺 DSN 时拒绝启动。

令牌、密钥和生产数据库不得进入仓库或文档。

## 二、健康检查

```bash
apps/server/rp_service.sh status
apps/server/rp_service.sh logs
```

受保护 API 必须使用有效 Bearer。/api/* 缺失或无效令牌应返回认证失败，未知 API 不应回退为 HTML。

## 三、备份

备份、归档校验、恢复和恢复后对账见 [BACKUP-RESTORE.md](../guides/BACKUP-RESTORE.md)。

恢复前后至少核对：

- SQLite quick_check
- resource、evidence 和 research_task 计数
- 搜索派生索引重建行数
- migration/manifest 和 sha256
- verify 对账

搜索索引不得当作事实数据备份，它可以重建。

## 四、升级

升级流程见 [UPGRADE.md](../guides/UPGRADE.md) 和 [MIGRATION.md](../guides/MIGRATION.md)。

升级前先备份；迁移先在副本执行；离线全 workspace 测试和 Clippy 通过后才重启；verify 对账低于 100% 不切流。

## 五、故障处理

```bash
apps/server/rp_service.sh restart
apps/server/rp_service.sh logs
```

数据库异常时停止写入并恢复快照。搜索异常时重建派生索引，不直接修改 PostgreSQL 权威记录。工具异常时保留 run、artifact、evidence 和 trace，不重跑未知副作用工具。
