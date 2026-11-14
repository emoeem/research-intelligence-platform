# UPGRADE：升级与回滚（P10-4，ROADMAP P10-6 四文档之一）

> 状态：生效中；日期：2026-11-14；适用版本：Phase 10 批次工作树（未 commit）。
> 相关：docs/guides/INSTALL.md、docs/guides/BACKUP-RESTORE.md、docs/guides/MIGRATION.md、
> docs/phase10/P10-2-BATCH-REPORT.md（插件升级回滚与镜像 digest 回滚实演）。

## 一、平台升级（git pull → 离线门禁 → 重启服务）

~~~bash
cd research-platform
git pull --ff-only

# 1. 升级前备份（必须，先备份再构建）：
apps/server/rp_service.sh stop
target/debug/rp-admin backup --db migrations/platform.db --out backups/pre-upgrade-$(date +%Y%m%d-%H%M).tar
# 2. 离线全量门禁（vendor 已入库，正常升级零网络；依赖变更见第二节）：
cargo test --offline --workspace
cargo clippy --offline --workspace --all-targets -- -D warnings
python3 -m unittest discover -s workers/python/tests
# 3. 数据库迁移核对（新迁移链在副本上先验证，见 MIGRATION.md 第四节）：
cp migrations/platform.db /tmp/upgrade-check.db
target/debug/rp-resolve --db /tmp/upgrade-check.db --migrate-dir migrations --writable --keywords smoke --limit 5
# 4. 对账（一致率 100% 才放行）：
RP_TARGET_DB=/tmp/upgrade-check.db python3 -I migrations/verify.py
# 5. 重建运行库二进制并重启服务：
cargo build --offline --workspace
apps/server/rp_service.sh start && apps/server/rp_service.sh status
# 6. web/ 面若发布：cd web && npm install && npm run build（npm 包只进 web/，红线）
~~~

## 二、依赖变更时的增量 vendor（保 --offline）

新 Rust 依赖的引入流程（红线：vendor/ 与 Cargo.lock 同批入库，构建离线可复现）：

~~~bash
# 在 Cargo.toml 锁定版本（先评估维护状态与许可证，deny.toml 门禁拒绝即不引入）：
cargo deny check licenses               # 许可证合规门禁（docs/phase10/DEPENDENCIES-RUST.md 口径）
cargo vendor vendor/                    # 增量拉取（需网络，仅此一步）；确认 diff 只含新依赖目录
cargo test --offline --workspace        # 增量后干净态离线复验
git add Cargo.toml Cargo.lock vendor/   # 三件同批提交
~~~

npm 面同理：`cd web && npm install <pkg>`，`git add web/package.json web/package-lock.json`
（node_modules 不入库；许可证逐包审计口径见 docs/phase10/DEPENDENCIES-WEB.md）。

## 三、插件升级与失败回滚（P10-2 实演口径）

~~~bash
# 升级（新 Manifest + 新包）：
target/debug/rp-plugin upgrade <plugin-id> --manifest m2.json --package pkg-v2
# 校验失败自动回滚：VALIDATED 链 fail-closed（validation_failed），active 指针不切换，
# 0.2.0 行零落库（事务回滚），audit 记 result=rolled_back
# 回滚窗口（D6）：窗口内可手动回退，结束且用户确认后清理：
target/debug/rp-plugin rollback-window <plugin-id>          # 查看窗口内旧版本
target/debug/rp-plugin cleanup-rollback <plugin-id> --confirm   # 确认后物理清理（红线守卫：data_root 外不删）
# 目录发布面：catalog-publish → catalog-verify（sha 校验，篡改拒绝）
~~~

## 四、镜像 digest 升级与回滚（ADR §6 镜像回滚语义）

~~~bash
# 初次钉版：
target/debug/rp-image-pin pin --image localhost/rip-tool-webresearch --digest sha256:<digest>
# 升级（三方对账：Manifest validate_static + 台账 pin 值 + 传入 digest；不一致自动回滚旧 digest）：
target/debug/rp-image-pin update --image localhost/rip-tool-webresearch --digest sha256:<new> --manifest tool-manifest.json
# 显式回滚（previous_digest 留作再滚回位，history 全程留痕）：
target/debug/rp-image-pin rollback --image localhost/rip-tool-webresearch
~~~

## 五、RTIS 集成回滚（生产 Resolve 路径）

~~~bash
RTIS_RESOLVER_MODE=legacy       # 三态：rp | legacy | off；legacy 走旧解析路径
# 或还原 integrations/rtis/backups 下备份（bootstrap.sh 可从备份重建 knowledge.db）
~~~

## 六、失败回滚判据速查

| 场景 | 判据 | 回滚 |
| --- | --- | --- |
| cargo test/clippy 不过 | 构建或测试 exit 非 0 | `git revert` 升级提交；服务不重启（旧二进制继续跑） |
| 迁移断言失败 | migrate 报错（版本断言/连续性） | 还原 pre-upgrade 备份（BACKUP-RESTORE.md 第四节） |
| verify.py 对账 < 100% | 一致率非全过 | 还原备份；切流挂起（README 纪律） |
| 服务启动失败 | systemctl --user status 非 active | `RP_DB_PATH` 指回旧库；回滚二进制（git revert + rebuild） |
| 插件升级失败 | validation_failed / update_check 拒绝 | 自动回滚（指针不切换）；显式 rollback-window |
| 镜像 digest 不一致 | rp-image-pin verified=false rolled_back | 自动回滚旧 digest；必要时 rollback 显式拨回 |
