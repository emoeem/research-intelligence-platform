# RIP 1.0 发布报告

> 状态：文档交付完成，运行状态按选定验收范围收口。日期：2026-11-28。

| 项目 | 结论 |
|---|---|
| 仓库基线 | `main`，HEAD `3055d7aaef62cf24b13cacf4be1c10a9d88028c4`，其中 closeout 验证基线为 `0d83a685` |
| 工作区边界 | 保留未跟踪 `test-results/` 和本地数据；本批不改数据库 |
| 正式知识存储 | SQLite 兼容运行数据；PostgreSQL 作为正式知识治理和关系数据方案 |
| Web 入口 | 7800 旧静态入口保留；React SPA staged 在 7801，不切默认入口 |
| 发布结论 | 已完成文档化交付；不声明未验收项全部通过 |

## 一、交付范围

本报告汇总项目实现、验收和已知边界。详细证据分布如下：

- 项目收口：[PROJECT-CLOSEOUT-2026-11-27.md](../PROJECT-CLOSEOUT-2026-11-27.md)
- 跨会话状态：[SESSION-HANDOFF.md](../SESSION-HANDOFF.md)
- 最终验收：[RIP-1.0-ACCEPTANCE-REPORT.md](RIP-1.0-ACCEPTANCE-REPORT.md)
- 运维：[RIP-1.0-OPERATIONS.md](RIP-1.0-OPERATIONS.md)
- 已知限制：[RIP-1.0-KNOWN-LIMITATIONS.md](RIP-1.0-KNOWN-LIMITATIONS.md)

## 二、核心能力

已实现并保留验收证据的能力包括 Rust Core、Python Workers、REST/MCP、知识摄取治理、Nuclei 与 POC 治理、统一检索、Agent、Workflow、Web Workbench、备份恢复和三语界面。

交付不是以代码数量判断，而以真实流程、数据来源、人工审核、权限隔离和恢复能力判断。

## 三、已验收事实

- Rust workspace：格式、离线全量测试、Clippy 和离线构建通过。
- PostgreSQL：Podman 17 验收门通过一次性数据库和临时角色测试。
- Python：Workers、server、安全元数据、POC 和 M4 工具测试通过，环境相关项明确跳过。
- Web：TypeScript、构建、Vitest、视觉/axe 和响应式矩阵通过。
- 备份恢复：独立 PostgreSQL 容器、SQLite 快照、备份恢复和保留归档有自验证据。

## 四、发布边界

以下事项不计为无条件生产通过：

- M4 采用用户采样接受，不是 140 条人工 qrel 完整标注。
- M7 按 reference-based 范围关闭，不声明像素级 Figma 复刻或真实设备 Core Web Vitals。
- M8 保持 staged，不切 7800 默认入口。
- 真实外部 LLM Provider E2E 仍待凭据注入。
- POC 和 Nuclei 不因静态审查或来源批准而标记为已执行验证。

## 五、结论

文档交付完整且与仓库事实一致。正式上线应另按本报告和已知限制逐项签收，不得用局部测试替代生产部署、人工评测或故障恢复验收。
