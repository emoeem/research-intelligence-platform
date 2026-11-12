-- ============================================================================
-- Research Intelligence Platform : Phase 7 trace 统一关联增量迁移
--   （编排批次 P7-5 第 5/7 批，SESSION-HANDOFF §十追加项 1：trace_id/task_id/
--   agent_run_id/tool_run_id/artifact_id/evidence_id 六 ID 统一关联与跨栈查询；
--   六 ID 类型集中定义见 crates/domain/src/ids.rs）
-- ============================================================================
-- 编号依据（ROADMAP §1.4 迁移编号冻结规则）：按实际合入顺序递增。014 已被
-- agent_run_control 实际占用（上一批先到先得），本文件实际取 015。
--
-- 范围（五张既有表各追加一列 trace_id + 五个索引 + 一个只读视图；均为
-- 追加结构，不改既有列、不改任何 CHECK 词值域）：
--   audit_event / tool_run / artifact / evidence / agent_run 追加
--   trace_id TEXT 列：链路追踪 ID（domain::ids::TraceId），一次研究动作
--   跨实体族的关联锚。写入面（P7-5 同批）：agent-engine run 受理时生成
--   或继承（RunControl::retry 链继承源 run 的 trace_id），随 tool_run 提交
--   （tool-engine submit_correlated）、artifact/evidence 登记
--   （agent-engine EvidenceRegistrar）、audit 事件（audit-engine
--   correlation_id 体系扩展）统一注入。
--   关联拓扑（行级）：
--     agent_run.run_id --1:N--> tool_run（agent 步骤驱动，trace_id 同源）
--     tool_run.run_id --1:N--> artifact（trace_id 同源直落 + run_id FK）
--     tool_run.run_id --1:N--> evidence（source_pk = run_id；trace_id 同源）
--     context_manifest.task_id --> agent_run.task_id 或 run_id（无任务时
--       装配以 run_id 兜底为 task_id，意图/上下文面经此关联）
--     audit_event.correlation_id = run_id（run 内事件链主键不变）+
--     audit_event.trace_id = trace_id（跨实体族关联锚，P7-5 扩展列）
--
-- DDL 说明（如实记录，002 迁移同款先例）：SQLite 无 ADD COLUMN IF NOT
-- EXISTS 条件 DDL，本文件的 ALTER TABLE 语句依赖迁移执行器
-- （crates/database migrate.rs）的 user_version 单次执行语义：版本号已达
-- 15 的库不会重放本文件；同一库手工重复执行本文件会报 duplicate column
-- name，属预期守卫而非缺陷。
--
-- 反重复建模约束：
--   - 不给 context_manifest/agent_run_step/agent_run_control 追加 trace_id：
--     manifest 经 task_id 关联（见上），step/control 经 run_id FK 关联
--     （014），单锚传播，不复制关联列。
--   - tool_run/artifact/evidence/audit_event 的 trace_id 全部可空：Phase 0-6
--     既有行与生产库（platform*.db 本迁移不落）维持 NULL，只读查询面按
--     NULL 如实呈现，不回填伪造。
--
-- 红线（ROADMAP §13）：新迁移只在开发库副本与测试内存库执行，
-- migrations/platform.db 与 migrations/platform-dryrun.db 一律不写。
--
-- 幂等：CREATE INDEX IF NOT EXISTS + CREATE VIEW IF NOT EXISTS + INSERT OR
-- IGNORE + PRAGMA user_version = 15；ALTER TABLE 部分见上方 DDL 说明，
-- 由执行器 user_version 语义保证单次执行。
-- ============================================================================

-- ---------------------------------------------------------------------------
-- 五面 trace_id 列（全部可空，TEXT，无默认值：既有行不回填）
-- ---------------------------------------------------------------------------
ALTER TABLE audit_event ADD COLUMN trace_id TEXT;
ALTER TABLE tool_run ADD COLUMN trace_id TEXT;
ALTER TABLE artifact ADD COLUMN trace_id TEXT;
ALTER TABLE evidence ADD COLUMN trace_id TEXT;
ALTER TABLE agent_run ADD COLUMN trace_id TEXT;

-- ---------------------------------------------------------------------------
-- 五面 trace_id 索引（关联查询主通道；索引名沿用 ix_<table>_trace 既有惯例）
-- ---------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS ix_audit_event_trace ON audit_event(trace_id);
CREATE INDEX IF NOT EXISTS ix_tool_run_trace    ON tool_run(trace_id);
CREATE INDEX IF NOT EXISTS ix_artifact_trace    ON artifact(trace_id);
CREATE INDEX IF NOT EXISTS ix_evidence_trace    ON evidence(trace_id);
CREATE INDEX IF NOT EXISTS ix_agent_run_trace   ON agent_run(trace_id);

-- ---------------------------------------------------------------------------
-- v_trace_chain：统一只读关联视图（SESSION-HANDOFF §十追加项 1「等价 SQL
-- 视图」落面）：按 trace_id 汇出六面行（五类链：任务面经 agent_run.task_id
-- 列透出，工具/证据/审计/运行行直透），行序 = 时间面（ts 升序）。
--   entity_type 词表：agent_run / tool_run / artifact / evidence / audit_event。
--   context_id 列承载行内父引用（artifact/evidence 指向 run_id，
--   audit_event 指向 correlation_id），供按行回溯父实体。
-- 视图是只读查询入口之一（另一入口 rp-agent trace 子命令，读面 SQL 与本
-- 视图同拓扑并叠加 research_task/context_manifest join）；本视图不承载
-- research_task 与 context_manifest（两表与 trace 无列级关联，按 task_id
-- join 归 rp-agent trace 子命令实现）。
-- ---------------------------------------------------------------------------
CREATE VIEW IF NOT EXISTS v_trace_chain AS
SELECT 'agent_run' AS entity_type, run_id AS entity_id, trace_id, task_id AS task_ref,
       created_at AS ts, NULL AS context_id, state AS detail
  FROM agent_run WHERE trace_id IS NOT NULL
UNION ALL
SELECT 'tool_run' AS entity_type, run_id AS entity_id, trace_id, task_id AS task_ref,
       created_at AS ts, NULL AS context_id, status AS detail
  FROM tool_run WHERE trace_id IS NOT NULL
UNION ALL
SELECT 'artifact' AS entity_type, artifact_id AS entity_id, trace_id, NULL AS task_ref,
       created_at AS ts, run_id AS context_id, role AS detail
  FROM artifact WHERE trace_id IS NOT NULL
UNION ALL
SELECT 'evidence' AS entity_type, evidence_id AS entity_id, trace_id, task_ref,
       created_at AS ts, source_pk AS context_id, evidence_type AS detail
  FROM evidence WHERE trace_id IS NOT NULL
UNION ALL
SELECT 'audit_event' AS entity_type, CAST(event_id AS TEXT) AS entity_id, trace_id,
       NULL AS task_ref, ts, correlation_id AS context_id, action AS detail
  FROM audit_event WHERE trace_id IS NOT NULL;

-- ---------------------------------------------------------------------------
-- schema_version：Phase 7 trace 关联变更登记（version=15；1..14 见各自登记行）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (15, 'V1 additive Phase 7 trace correlation (trace_id columns + indexes + v_trace_chain view)',
   'Phase 7 P7-5（SESSION-HANDOFF §十追加项 1）：audit_event/tool_run/artifact/evidence/agent_run 五表追加可空 trace_id 列与索引 + 只读视图 v_trace_chain（六面行按 trace_id 汇出）；六 ID 类型集中定义在 crates/domain/src/ids.rs（TraceId 新类型 + TaskId/EvidenceId 新类型 + AgentRunId/ToolRunId 别名收口）；ALTER TABLE 依赖执行器 user_version 单次执行语义（SQLite 无条件 DDL，002 先例）；只在开发库副本与测试内存库执行，platform.db 与 platform-dryrun.db 不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '015_trace_correlation.sql');

-- user_version 双写（DEC-002 条件，与 001_init.sql 尾部同款）
PRAGMA user_version = 15;
