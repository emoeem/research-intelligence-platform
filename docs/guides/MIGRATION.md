# MIGRATION：数据库迁移与对账（P10-4，ROADMAP P10-6 四文档之一）

> 状态：生效中；日期：2026-11-14；适用版本：Phase 10 批次工作树（未 commit）。
> 相关：docs/guides/INSTALL.md（路径 A/B 初始化）、docs/guides/BACKUP-RESTORE.md（升级前备份）、
> docs/guides/UPGRADE.md（平台升级流程）、crates/database/src/migrate.rs（执行器）、
> migrations/001_init.sql 至 019_external_api_audit.sql（迁移链）。

## 一、迁移链现状（019 个编号 SQL，版本升序）

| 迁移 | 内容 |
| --- | --- |
| 001_init.sql | 核心域表（resource/resource_source/review/knowledge/audit_event 等，M1） |
| 002_tasks.sql | research_task 任务面 |
| 003_tools_workflow.sql | 工具注册面与工作流雏形 |
| 004_fullbatch.sql | 全量批扩面 |
| 005_tool_contract.sql | Tool Contract（D-TC-01 至 06）落地表 |
| 006_agent_data_plane.sql | Agent 数据面 |
| 007_runtime_node.sql | 运行时节点注册面 |
| 008_context_broker.sql | 上下文装配面 |
| 009_plugin.sql | 插件注册面 |
| 010_knowledge_ingest.sql | 知识摄取 |
| 011_knowledge_sources.sql | 知识源 |
| 012_knowledge_document_fetch.sql | 文档抓取 |
| 013_agent_memory.sql | Agent 记忆 |
| 014_agent_run_control.sql | Agent 运行控制 |
| 015_trace_correlation.sql | 统一 trace 关联 |
| 016_workflow_definition.sql | Workflow 定义（graph-v1） |
| 017_workflow_run.sql | Workflow 运行 |
| 018_workflow_control_correlation.sql | Workflow 控制与关联 |
| 019_external_api_audit.sql | REST 网关访问审计（保留归档策略目标表） |

## 二、执行器语义（crates/database/src/migrate.rs）

1. 发现：`discover_migrations(dir)` 扫描目录内 `NNN_name.sql`（编号前缀决定顺序）。
2. 幂等：按 `PRAGMA user_version` 断点续跑（已应用版本跳过；重复执行零写）。
3. 断言：链内版本号必须连续递增；中断（缺号/回退版本）直接报错不部分应用。
4. 边界：database crate 不含业务 SQL 方言知识，表结构归 migrations/ 与各 engine 拥有
   （MODULE_BOUNDARIES §8.1）。注意：迁移链版本断言走 schema_version 体系，备份清单的
   `PRAGMA user_version` 记录源库实际值作对账（platform.db 链记录在 schema_version 表，
   user_version 停在 1，见 selftest_phase10_backup.sh 第 1 步注释口径）。

## 三、如何把迁移应用到目标库

~~~bash
# 1. 文件库全链初始化（INSTALL.md 路径 A）：
target/debug/rp-resolve --db migrations/platform.db --migrate-dir migrations --writable --keywords smoke --limit 5
# stderr: rp-resolve: applied 19 migration(s)
# 2. 运行库（tool_run/artifact/evidence/runtime_node）：rp-tool register 与 rp-mcp
#    启动时按 RP_MIGRATIONS_DIR（默认 migrations）对运行库全量迁移（幂等）：
target/debug/rp-tool register --manifest manifests/tools/rip.kb.keyword_search.json --db rip-data/tool-runs.db
# 3. 内存库（测试与演练，零生产触碰）：DbPath::Memory + 全链迁移（workspace 测试统一口径）
~~~

## 四、新增迁移的流程（纪律）

1. 新增编号文件 `020_name.sql`（只新增，不改历史文件；历史文件被已有库引用）。
2. 先在开发副本验证（不写生产 migrations/platform*.db，红线）：复制 platform.db 为
   dev 副本或用路径 A 空库，跑全链 migrate 确认零断言失败。
3. ETL 幂等核对：涉及数据回填的迁移在 etl 脚本同口径下重跑验证幂等（INSERT OR REPLACE
   确定性主键，重复执行零漂移）。
4. 对账：`RP_TARGET_DB=<副本> python3 -I migrations/verify.py` 全项一致才收口
   （verify 一致率 100% 是切流与推送的硬前提，README 口径）。
5. 生产切流另行评审（不随批次自动）；回滚方式 = 还原备份（docs/guides/BACKUP-RESTORE.md
   第四节），SQL 迁移不做 down 脚本（快照回滚语义）。

## 五、ETL 与对账

| 脚本 | 作用 | 产物 |
| --- | --- | --- |
| migrations/etl_dryrun.py | 三源快照（catalog/knowledge/ERI JSON）→ dryrun 库；幂等重跑 | migrations/platform-dryrun.db |
| migrations/etl_evidence.py | evidence 段增量 | evidence 计数对账 |
| migrations/etl_m6.py | 全量批（26 活动源库，维护机口径） | migrations/platform-fullbatch.db |
| migrations/verify.py | 对账红线（一致率必须 100%） | RP_TARGET_DB 可换目标库 |

fresh 环境口径：etl_dryrun.py 只读入库快照（migrations/sources-snapshot/），零外部依赖可跑；
全量批与 verify.py 的 evidence 段依赖维护机活动源库，属已知环境限制（BACKUP-RESTORE.md 第八节同口径）。
