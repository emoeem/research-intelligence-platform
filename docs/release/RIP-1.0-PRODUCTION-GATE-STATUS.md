# RIP 1.0 生产验收门禁状态

> 状态：冻结审查快照。日期：2026-11-28。

## 一、冻结基线

以下 DSN 与恢复模块及文档冻结，不再修改或解释性重写：

| 文件 | SHA-256 |
|---|---|
| `apps/server/platform_env.sh` | `24ccc8858c1173aa2ad8d24b30fcbf95191c6b33b706496c6337ca43d9268ea4` |
| `apps/server/rp_service.sh` | `284a2fb14f4faf81e9f9344e14e793f75b8e26a2d89b670c8a39d04b261783e7` |
| `apps/server/dev_up.sh` | `b3e9566263fbf352b06492def7b7b85f8af4f0584662d0553de1e9913c3acca4` |
| `scripts/rip_pg_recovery_gate.py` | `42fafaf18f76a3a8c086d62d1b238e1ecd9d3e485a2f3b3859d2b2e6036dc26e` |
| `docs/guides/INSTALL.md` | `951e89240365dba30cd9029ebd9efbe57d9b49b1668b55145af45ed965281b91` |
| `docs/release/RIP-1.0-OPERATIONS.md` | `31a41772d79b6e0efbb26620818d67f7a855387add04758099bb4352a98eae9e` |
| `docs/release/RIP-1.0-DISASTER-RECOVERY.md` | `3bce3203b09fb6a8f0320ba259de233441e53da966b34c9f8eb5e6239a6c9425` |

冻结证据包括：

- DSN 缺文件、权限、空值和额外行退出码已实测为 1、2、3、4。
- 缺 DSN 的 `rp_service.sh install` 实测退出码为 1。
- 隔离 template0 PostgreSQL 源库完成 custom dump 和恢复副本，恢复行数为 2/2。
- 真实 user systemd 服务在端口 7815 注入 DSN 后启动，`/api/review-batches`
  返回 HTTP 200；演练后已卸载服务并删除临时数据库和文件。

## 二、门禁台账

| 门禁 | 当前状态 | 当前证据 |
|---|---|---|
| DSN 统一注入与 systemd 启动 | Local PASS | 隔离 7815 systemd 演练通过 |
| PostgreSQL 隔离 dump/restore | Local PASS | 2/2 行恢复对账通过 |
| PostgreSQL 正式生产备份制度 | Open | 生产库日备份、异地保存、密钥和驻留策略未签收 |
| M4 人工检索指标 | Open | 140/140 候选，0 人工标注，metrics null |
| M8 默认入口切换 | Open | 7800 为旧入口存活；7801 未监听；回滚和生产 TLS 未验收 |
| 真实外部 LLM Provider E2E | Open | DeepSeek、OpenAI、Moonshot、Kimi 环境变量均未配置 |
| Nuclei 工程验证 | Open | 本机 `/usr/bin/nuclei` 为 v3.11.1；未经授权靶机验证未执行 |
| POC 真实执行验证 | Open | 静态与治理证据不能替代目标验证 |
| 正式反向代理/TLS | Open | 未配置真实外部部署拓扑 |
| 多租户权限 | Out of scope | 当前个人工作区验收，不声明多租户完成 |

## 三、当前运行状态快照

- 7800 正监听，7801 未监听。
- `rip-pg17` 和 `rip-m4-personal-pg` 两个 Podman 容器正在运行。
- 正式 `rp-workbench.service` 不存在；7815 演练服务已卸载。
- M4 CSV 已核对：140 行，人工 grade 0。

## 四、下一验收顺序

1. 由部署负责人签收正式 `rip_platform` 备份频率、异地保存、权限和密钥策略。
2. 完成 M8 真实 Bearer、写操作、反向代理/TLS、旧入口回滚和观察期。
3. 注入真实 Provider 凭据后执行一次外部 Provider E2E，不得记录到仓库或文档。
4. 在授权实验环境完成 POC 和 Nuclei 的独立执行、证据、复测和负例。
5. 补齐恢复证据、版本、日志、构建和生产切流台账后，重新签收总体状态。
