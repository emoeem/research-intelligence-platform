# RIP3 Phase 7：Knowledge Feedback & Maintenance

状态：完成（2026-11-23）。实现使用 PostgreSQL V0009 + V0010 两个 additive 迁移，V0001-V0008 保持不变。V0010 为 research_tasks 增加 workspace_id，确保 Finding 与 RTIS 监测严格按 workspace 隔离。RTIS 继续是漏洞事实与 POC 正文的唯一生产源，本阶段没有向 RTIS 写入。

## 交付范围

| 计划子项 | 实现 |
|---|---|
| Finding → 候选知识 | Report IR v1 先走 `report_engine::validate_ir`；仅 `result_state=confirmed` 且 Evidence 非空的 Finding 进入队列 |
| Evidence 溯源 | 持久化 evidence_id、artifact_id、tool_run_id、trace_id、URI、SHA-256、evidence_type |
| 上游更新监测 | ETag、Last-Modified、受限内容 SHA-256；保留每次检查事件 |
| 失效链接检查 | HTTP 404/410 标记 MISSING，其他异常记 ERROR，不吞掉错误 |
| 旧 POC 检查 | RTIS SQLite 以 `mode=ro&immutable=1` 打开；只读取 POC id、验证状态和日期字段，不读取 POC 正文 |
| 自动质量巡检 | 审核超期、Evidence 溯源缺失、来源 URL 缺失、上游变化/失效、重复内容组 |
| 维护报告 | `maintenance_runs` + `maintenance_findings` 持久化，支持读取完整报告 |

## 数据模型

`migrations_pg/V0009__knowledge_feedback_maintenance.sql` 新增反馈、证据、来源监测和维护台账；`migrations_pg/V0010__research_task_workspace.sql` 为任务投影增加 workspace 归属并建立索引。

- `research_tasks` / `research_task_reports`：任务与报告映射。
- `findings` / `finding_observations`：当前 Finding 投影与按报告追加的历史观察。
- `knowledge_feedback_candidates`：人工审核队列，状态为 `PENDING_REVIEW`、`DUPLICATE_REVIEW`、`PROMOTED`、`REJECTED`。
- `knowledge_evidence_provenance`：六 ID 溯源与内容哈希。
- `source_monitor_state` / `source_monitor_events`：来源当前状态与不可覆盖的检查历史。
- `maintenance_runs` / `maintenance_findings`：巡检运行与报告明细。

同一 report_id 重放是幂等的。跨报告的 Finding 观察保留在 `finding_observations`。候选内容按规范化 SHA-256 检测重复；重复命中已批准卡片时进入 `DUPLICATE_REVIEW`，必须人工选择复用或拒绝。

## 人工审核流程

以下命令由 `rp-kc` 提供，数据库连接使用 `RP_PG_DSN`：

```bash
# 把一个已落库的 Report IR 导入审核队列
cargo run --offline -p intel-db --bin rp-kc -- feedback-ingest \
  --report-id rep-... --workspace default

# 查看审核队列与单条候选的完整 Evidence 链
cargo run --offline -p intel-db --bin rp-kc -- feedback-list \
  --workspace default --status PENDING_REVIEW
cargo run --offline -p intel-db --bin rp-kc -- feedback-show --candidate-id kfc-...

# 人工决定：新知识只进入 KnowledgeCard CANDIDATE，不会自动 APPROVED
cargo run --offline -p intel-db --bin rp-kc -- feedback-promote \
  --candidate-id kfc-... --reviewer operator --reason "证据与影响范围已复核"

# 重复项复用现有 APPROVED 卡片，或拒绝候选
cargo run --offline -p intel-db --bin rp-kc -- feedback-reuse \
  --candidate-id kfc-... --reviewer operator --reason "复用现有知识卡"
cargo run --offline -p intel-db --bin rp-kc -- feedback-reject \
  --candidate-id kfc-... --reviewer operator --reason "证据不足"

# 质量巡检和完整维护报告
cargo run --offline -p intel-db --bin rp-kc -- maintenance-audit --workspace default
cargo run --offline -p intel-db --bin rp-kc -- maintenance-report --run-id mrun-...
```

已提升候选的最终批准仍由既有 KnowledgeCard 审核闸门处理。对已提升的候选，不能绕过卡片审核直接拒绝。

## 来源与 RTIS 维护 Worker

```bash
RP_PG_DSN="$RP_PG_DSN" \
RTIS_KNOWLEDGE_DB="/path/to/RTIS/knowledge.db" \
python3 scripts/rip3_source_monitor.py --workspace default \
  --timeout 12 --interval-hours 24 --limit 500
```

Worker 会初始化尚未登记的 URL/GitHub 来源、检查到期来源、写入检查事件，并对 Finding 中的 `rtis://poc/<id>` 做只读存在性/过期/未验证检查。默认拒绝私网、回环、链路本地及非公网 DNS 地址；每一跳重定向都重新校验目标地址；不接受 URL 内嵌凭据。`RIP3_SOURCE_MONITOR_ALLOW_PRIVATE=1` 只用于本地测试，生产环境不得设置。

检查 POC 时只选取 `poc.id` 以及存在的 `last_verified_at` / `updated_at` / `created_at`、`verified` 列。不会读取 `code`、请求响应或其他正文列，也不会写入 RTIS SQLite。

## 验收

- Rust `intel-db` 单元测试：32 passed，0 failed。
- PostgreSQL 集成测试：4 passed，覆盖 V0009/V0010 幂等与 workspace 列、Report IR 校验、Finding 入队、Evidence 溯源、重复导入幂等、人工提升到 CANDIDATE、拒绝终态、维护报告读取、跨 workspace 任务写入拒绝。
- Python Worker 单元测试：4 passed，覆盖 ETag、HEAD→GET 回退、私网拒绝、重定向 SSRF 拦截。
- PostgreSQL 来源监测实测：UNCHANGED → CHANGED（ETag 变化）→ MISSING（404）→ URL 轮换后重新建立 UNCHANGED 基线，四条事件均落库。
- RTIS POC 检查实测：使用临时 SQLite fixture 验证 missing/stale/unverified 规则；生产 RTIS 数据库未打开、未写入。
- `cargo clippy --offline -p intel-db --all-targets -- -D warnings`：通过。
- 全 Workspace 串行回归：1,396 passed，0 failed，3 ignored；全 Workspace Clippy：`cargo clippy --offline --workspace --all-targets -- -D warnings` 通过。

可重复门禁：`apps/server/selftest_p7p8.sh`。本阶段没有修改 RTIS 生产库、没有自动批准知识、没有自动合并/删除重复知识。
