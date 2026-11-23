# RELEASE：RIP3 Phase 7 + Phase 8

- 日期：2026-11-23
- 基线：`daecc2a0`
- 状态：实现与阶段验收完成，已提交并推送至 `main`；commit `0229bf01f1cc133e997f5bd644221c88da95de18`。
- GitHub Actions：[Run #180](https://github.com/emoeem/research-intelligence-platform/actions/runs/45141019769)，2026-11-23 08:54:28 UTC 完成，4/4 Jobs success。
- 生产数据：未修改 RTIS `knowledge.db`，未修改运营 `platform*.db`。

## Phase 7 — Knowledge Feedback & Maintenance

### 交付

- PostgreSQL additive migrations：
  - `migrations_pg/V0009__knowledge_feedback_maintenance.sql`
  - `migrations_pg/V0010__research_task_workspace.sql`
- Rust `crates/intel-db/src/maintenance.rs`：
  - Report IR v1 Schema 校验后才允许 ingest。
  - confirmed + Evidence 非空的 Finding 进入候选知识审核队列。
  - `research_tasks`、`research_task_reports`、`findings`、`finding_observations` 保存任务映射和历史观察。
  - Evidence 六 ID 溯源：Evidence / Artifact / Tool Run / Trace / URI / SHA-256。
  - 候选哈希去重；重复项人工 reuse，新增候选人工 promote，证据不足人工 reject。
  - promote 只创建 KnowledgeCard CANDIDATE，永不自动 APPROVED。
  - workspace_id 绑定任务，跨 workspace 复用 task_id fail-closed。
  - 质量巡检和完整维护报告读取。
- `rp-kc` 新命令：`feedback-ingest`、`feedback-list`、`feedback-show`、`feedback-promote`、`feedback-reuse`、`feedback-reject`、`maintenance-audit`、`maintenance-report`。
- `scripts/rip3_source_monitor.py`：ETag / Last-Modified / SHA-256 上游更新监测、HTTP 链接失效检测、URL 轮换重新建基线、RTIS POC 只读存在性/新鲜度/验证状态检查。
- Worker 默认只允许公网 HTTP(S)，拒绝私网/回环/链路本地地址和 URL 凭据，每一跳重定向重新校验。RTIS SQLite 以 `mode=ro&immutable=1` 打开，只读 POC ID 与可选元数据，不读取 POC 正文。

### 验收

- `intel-db` 单元：32 passed。
- PostgreSQL 集成：4 passed，覆盖 V0009/V0010 幂等、workspace 隔离、Report IR ingest、Evidence provenance、幂等重放、人工 promote/reject、维护报告。
- Python Worker 单元：4 passed。
- PostgreSQL 来源监测实测：UNCHANGED → CHANGED（ETag）→ MISSING（404）→ URL 轮换后重新 UNCHANGED，四条事件落库。
- RTIS POC 检查：临时 SQLite fixture 验证 missing/stale/unverified；生产 RTIS 库未打开、未写入。
- `cargo clippy --offline -p intel-db --all-targets -- -D warnings` passed。

详细说明：`docs/rip3/PHASE-7-KNOWLEDGE-FEEDBACK.md`。

## Phase 8 — Scale Testing & Benchmark

### 交付

- PostgreSQL 17.11 benchmark：100,000 documents / 1,000,000 chunks / 4,000 operational events。
- SQLite 3.54.1 WAL + FTS5 对照：相同语料与事件规模。
- 指标覆盖：1k/10k/100k 导入、索引构建、检索 p50/p95、内存/存储、16 并发读取、8 并发写入、迁移 DDL、索引重建、备份恢复。
- PostgreSQL pg_dump/pg_restore 与 SQLite online backup/restore 均逐表核验 100k / 1M / 4k 计数一致。
- 临时 PostgreSQL schema / restore database / SQLite 文件均在隔离测试环境；没有生产迁移或切库。

### 关键结果

| 指标 | PostgreSQL 17.11 | SQLite WAL + FTS5 |
|---|---:|---:|
| 检索 p50 / p95 | 6.588 / 7.368 ms | 0.169 / 0.253 ms |
| 16 并发读吞吐 | 990.85 q/s | 2,750.89 q/s |
| 8 并发写入 4k 事件 | 21,490.79 rows/s | 102,564.1 rows/s |
| 存储 | 527,728,640 bytes | 190,050,304 bytes |
| 索引构建 / 重建 | 5.594 / 5.604 s | 1.719 / 3.747 s |
| 备份 / 恢复 | pg_dump 0.862 s / pg_restore 14.340 s | 0.178 / 0.161 s |

PostgreSQL 检索计划命中 `chunks_search_gin`，Bitmap Index Scan 约 0.009 ms，整条 EXPLAIN ANALYZE 查询约 0.023 ms。端到端统计包含 TCP 连接开销。SQLite 内存为 Python+SQLite 进程 RSS；PostgreSQL 记录的是单 backend memory context + shared_buffers，二者不可直接作 RSS 对比。

### 迁移裁决

**运营核继续保留 SQLite；情报核继续使用 PostgreSQL。当前不迁移运营核。**

本机嵌入式工作负载下，SQLite 本次读取、写入、存储和恢复指标更优，且没有观察到足以证明必须迁移的写锁瓶颈。若实际生产库持续出现 `SQLITE_BUSY`、写锁等待 p95 > 50 ms、失败/重试率 > 1%，或出现跨主机共享/HA 需求，再基于脱敏生产数据做 shadow benchmark 与回滚演练。

原始数据：
- PostgreSQL：`docs/rip3/benchmarks/phase8-scale-20261123T082804Z.json`
- PostgreSQL 可读报告：`docs/rip3/benchmarks/phase8-scale-20261123T082804Z.md`
- SQLite：`docs/rip3/benchmarks/phase8-sqlite-20261123T082842Z.json`

详细说明：`docs/rip3/PHASE-8-SCALE-TESTING.md`。

## 最终状态与边界

- Phase 7 与 Phase 8 的定向门禁通过：`apps/server/selftest_p7p8.sh` 输出 `PHASE7_PHASE8_SELFTEST=PASS`。
- 全 Workspace 测试串行复验通过：1,396 passed，0 failed，3 ignored（包含已知 Chromium 截断保真环境敏感测试）；并行重复启动曾触发 browser_adapter 临时 user-data-dir 清理竞争，因此验收采用单实例串行结果。
- 全 Workspace Clippy：`cargo clippy --offline --workspace --all-targets -- -D warnings` passed；intel-db format check passed。
- GitHub Actions #180 四个 Job 均 success：Rust workspace 测试与 Clippy、Python worker、Web 构建与 Vitest、REST/MCP 协议一致性。
- **CI 覆盖限制**：`.github/workflows/ci.yml` 未配置 PostgreSQL service / `RP_TEST_PG_DSN`。`phase7_maintenance` 测试在 DSN 缺失时提前 return，因此 CI 绿色不代表 PG 集成断言已执行；本地专用 `rip-p7p8-pg` 容器实跑 4/4 通过。来源监测/RTIS smoke 与 Phase 8 规模基准也未纳入远程 CI。
- 推送后工作区干净；没有修改 RTIS 生产库、运营 SQLite 数据库；没有自动批准、合并或删除知识；没有进行生产数据库切换。
