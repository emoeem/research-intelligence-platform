# RIP 1.0 最终验收报告

> 状态：按现有选定范围验收完成。日期：2026-11-28。

## 一、验收口径

验收依据是用户确认的范围和仓库实测证据，不包括未授权的多租户、像素级 Figma、自动人工标签或默认 SPA 切流。

| 领域 | 状态 | 证据 |
|---|---|---|
| Rust | 已通过 | 全 workspace 测试、构建、Clippy、格式检查 |
| Python | 已通过并记录跳过项 | Workers、server、治理和安全元数据测试 |
| Web | 已通过并记录非像素验收边界 | build、Vitest、Playwright、360/360 响应式组合 |
| PostgreSQL | 已通过本地隔离容器门禁 | 备份恢复、Phase 7 集成、source monitor |
| 数据治理 | 已通过实现验收 | M1 到 M6 治理报告和 closeout |
| 检索 | 用户采样接受 | M4 批次、人工标签仍为零 |
| UI | reference-based 关闭 | M7 报告 |
| 默认入口 | staged | M8 不切流 |

## 二、业务验收

知识系统、POC、Nuclei、工具、来源和证据均已有治理面和审计记录。未审核、CANDIDATE 或 DISCOVERED 资产不得自动提升为 VERIFIED。

Agent 和 Workflow 已实现持久化、轨迹、暂停恢复、批准闸门和工具派发；真实外部 Provider E2E 仍不计为通过。

## 三、发布判定

不得声明以下项无条件通过：

- 人工 cross-source 100+ qrel 数值指标。
- POC 或模板的真实目标执行验证。
- 多租户 RBAC。
- 像素级 Figma parity。
- Lighthouse 和冷网络性能。
- 反向代理、TLS、真实回滚和生产切流。

## 四、可复现证据索引

关键证据应从仓库中的报告和命令复核：

- Rust 全 workspace：`cargo test --offline --workspace`
- Rust 静态检查：`cargo clippy --offline --workspace --all-targets -- -D warnings`
- Rust 离线构建：`cargo build --offline --workspace`
- Python：`python3 -m unittest discover -s workers/python/tests`
- Web：`cd web && npm run build && npm run test`
- Web 视觉：`cd web && npx playwright test`
- PostgreSQL recovery gate：以正式部署 DSN 之外的独立 disposable DSN 执行
  `scripts/rip_pg_recovery_gate.py`
- SQLite/备份恢复：`apps/server/selftest_phase10_backup.sh`
- 项目收口数字和非 claims：[PROJECT-CLOSEOUT-2026-11-27.md](../PROJECT-CLOSEOUT-2026-11-27.md)
- M4 采样验收：[M4-USER-ACCEPTANCE-2026-11-27.md](../knowledge-governance/M4-USER-ACCEPTANCE-2026-11-27.md)
- M8 staged 状态：[M8-ENTRY-MIGRATION-READINESS.md](../ui-redesign/M8-ENTRY-MIGRATION-READINESS.md)
- 2026-11-28 隔离 systemd/PostgreSQL/recovery 演练：真实 user unit 在 7815
  启动并注入 600 权限 DSN，batch API 返回 200；template0 隔离恢复副本行数为 2。

报告中的 PASS 数字来自当日本地或隔离环境。测试时间、环境版本、SKIP 和
IGNORED 必须随实际运行日志保留，不应只复制数字进入新发布日志。

## 五、签收结论

在上述边界内，本项目已达到文档化 closeout 状态。任何试图用 AI 评分、静态解析、API 200 或来源 APPROVED 替代人工或执行证据的发布，不予接受。
