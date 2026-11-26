# Knowledge plugin install-path repair（2026-11-26）

## 结论

已修复 CISA KEV、Nuclei、PoC Index 三个 active knowledge plugin 的失效安装目录引用。原始 KSE 数据、审核状态和 Search v6 索引均未重建或重写。

问题来自此前隔离 cutover clone 中的插件 `installed_dir` 指向临时 `/tmp/rip-cisa-kev.qfBoNo/...` 目录。clone 被切换为持久化开发数据库后，数据库行仍保留旧临时路径。KSE API 与搜索索引当时仍可用，因为知识记录和派生索引已持久化；但插件后续 ingest/sync 会因找不到安装包目录而失败。

## 修复前状态

| Plugin | Version | State | Active | 原路径存在 |
|---|---:|---|---:|---:|
| `knowledge.cisa-kev` | 0.1.2 | ENABLED | yes | no |
| `knowledge.nuclei-templates` | 0.1.1 | ENABLED | yes | no |
| `knowledge.pocindex-kev` | 0.1.0 | ENABLED | yes | no |

## 实现

新增 `rp-plugin repair-path` 受控修复命令，以及 `PluginRuntime::repair_install_path` / `PluginStore::repair_installed_path` 唯一写面。

修复守卫：

1. 仅允许 active 且处于 `REGISTERED` 或 `ENABLED` 的插件。
2. 旧安装目录必须确实不存在；旧目录仍存在时拒绝覆盖。
3. 插件不能处于 `source_uninstalled` 状态。
4. 传入 Manifest 的 plugin id、version 必须与 active plugin 完全一致。
5. 传入 Manifest 必须与数据库登记的 Manifest 完全一致，staged package 内的 `manifest.yaml` 也必须一致。
6. 包重新通过现有七条静态 Manifest 校验，并重建 `checksums.sha256` 与 `plugin.lock`。
7. 数据库只更新 `installed_dir` 和 `lock_sha256`；不改变 plugin version、state、active pointer、capabilities、KSE 数据或 review 状态。
8. 通过现有 AuditEngine 写入 `plugin.install_path.repaired` 事件。

命令示例：

```bash
target/debug/rp-plugin \
  --db migrations/dev-p6.db \
  --migrations migrations \
  --data-dir rip-data \
  --plugins-dir rip-data/plugins \
  --data-root rip-data \
  repair-path knowledge.nuclei-templates \
  --manifest manifests/plugins/knowledge.nuclei-templates.yaml \
  --package target/nuclei-metadata-package
```

## 正式开发库修复结果

修复前先使用 SQLite online backup API 创建独立备份：

- 路径：`/home/emo/backups/research-platform/20261126-182822-dev-p6.db.pre-plugin-path-repair.bak`
- SHA-256：`6a5c05fcd9ca92b54d0da187ea5e05b4447f12cf602428f84a5047894fea24a7`
- `PRAGMA integrity_check=ok`
- 文件权限：0600

修复后的持久化路径与包指纹：

| Plugin | Version | 持久化安装目录 | `plugin.lock` / package SHA-256 |
|---|---:|---|---|
| `knowledge.cisa-kev` | 0.1.2 | `rip-data/plugins/installed/knowledge.cisa-kev/0.1.2` | `f292e839c17067d281bf6efae31def1f109f141fa8ec349064b4b0e6ba335934` |
| `knowledge.nuclei-templates` | 0.1.1 | `rip-data/plugins/installed/knowledge.nuclei-templates/0.1.1` | `0a47454e9bd5e997c4cb62e4c1c01050f5894305edbe4578ee3d75035f4cb775` |
| `knowledge.pocindex-kev` | 0.1.0 | `rip-data/plugins/installed/knowledge.pocindex-kev/0.1.0` | `ab3a0415cb8353ccf2e5bb25281e269574d3cee84797345b9e42335721db4862` |

三个插件均保持 `ENABLED`、`is_active=1`、`source_uninstalled=0`。安装目录全部存在。三条修复动作均留下 `plugin.install_path.repaired` 审计事件。随后检查全部七个 active knowledge plugins（Awesome-Hacking、CISA KEV、Nuclei、PoC Index、RTIS Redteam、SecSkills、Tongui），七个安装目录全部存在，没有剩余 active plugin path 缺失。

## 验证

### 自动化测试

- `cargo test --offline -p plugin-runtime`：85 passed / 0 failed / 3 ignored。
- `cargo clippy --offline -p plugin-runtime --all-targets -- -D warnings`：通过。
- 新增 `repair_missing_install_path_preserves_plugin_state_and_capabilities`：验证旧目录仍存在时拒绝覆盖；旧目录丢失后修复成功；插件状态、active pointer、capability rows 保持不变；写入修复审计事件。
- 在 disposable clone 中用不匹配 Manifest 验证失败路径：命令明确拒绝，且自动清理本次刚 staging 的目录，避免失败重试被 orphan directory 阻塞。
- `cargo check --offline -p plugin-runtime --bin rp-plugin`：通过。
- 在 disposable DB clone 中通过 CLI 对三个插件逐一执行 `repair-path`，三个目录均成功落位；clone 中 KSE 总数仍为 18,102，修复审计事件为 3。
- 完整 `cargo test --offline --workspace`：通过。首轮全工作区测试曾暴露 MCP 子任务测试夹具只设置 `RP_PLUGIN_DB`（MCP catalog），未设置 Agent 实际读取的 `RIP_PLUGIN_DB`，从而意外挂载本机 dev-p6.db 并超过测试 token budget；现已在该测试中显式设置不存在的隔离路径，确保不受开发机知识库规模影响。
- 最终 `cargo test --offline -p plugin-runtime`（CLI 清理分支加入后复跑）：85 passed / 0 failed / 3 ignored；全工作区 Clippy `-D warnings`、Python package tests、Knowledge Review selftest、POC Search selftest 均通过。

### 持久化开发库只读/干跑验收

三个插件均执行 `rp-plugin ingest --dry-run`：

| Plugin | 已存在记录 | Changed | Skipped |
|---|---:|---:|---:|
| CISA KEV | 1,805 | 0 | 1,805 |
| Nuclei | 14,111 | 0 | 14,111 |
| PoC Index | 1,296 | 0 | 1,296 |

这证明插件安装包可从持久化目录读取，且当前数据与固定快照一致；dry-run 没有新增或修改 KSE 行。

- `migrations/dev-p6.db`：schema 32，KSE 18,102，`integrity_check=ok`；修复与干跑后的 SHA-256：`0c1091dd3ade02cb6ab13e1fb2a8743db80302ac61606a7db0ce70221e1e19ac`。
- Nuclei review state：552 `APPROVED`、13,559 `DISCOVERED`，未变化。
- `migrations/platform-search.db` SHA-256 仍为 `2da952cad715e179ee2626b79b55784ab014897e270d18c65c05d1587dd43d54`，未重建、未修改。
- 原始 RTIS `integrations/rtis/knowledge.db` 未修改。
- 没有执行真实 POC、Nuclei 模板或任何外部目标请求。

## 后续

- 正常的插件更新应继续通过 `rp-plugin upgrade`；`repair-path` 只用于修复已登记版本的丢失安装目录，不是版本升级或数据重摄取入口。
- 保留上述备份，不自动清理。
- 后续 POC 工作继续聚焦证据缺口和修复状态核验；没有原始证据的条目保持待补，不得推断为已验证。


## GitHub CI

修复代码提交：b9db4453（fix(plugins): repair orphaned install paths）。推送后的 GitHub Actions run 45804184849 全部 8/8 jobs success，包含 Rust workspace 离线测试与 Clippy 零警告、REST/MCP parity、Browser visual/accessibility、PostgreSQL integration、Web build/Vitest、Python worker、安全检查和 SQLite performance smoke。
