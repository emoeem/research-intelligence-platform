-- ============================================================================
-- Research Intelligence Platform : Phase 7 Agent 数据面增量迁移
--   （P7-1 提前通道批次，PHASE-ROADMAP §8.3 P7-1 + 计划 §12:699-700 实体行）
-- ============================================================================
-- 编号依据（ROADMAP §1.4 迁移编号冻结规则）：迁移编号按合入顺序递增，先到
-- 先得。本文件实际合入 006（实测 ls migrations/*.sql 为 001 到 005）；
-- PHASE-ROADMAP §5.3 P4-1 曾预计 Context Broker 落地时用 006，按同规则让位
-- 于实际合入顺序，Phase 4 落地时取当时下一个可用号。
--
-- 范围（Agent 数据面两表，均为追加结构，不改既有表）：
--   1. agent_definition：AgentDefinition（agent_id、version、context_policy、
--      capability_refs；计划 §12:699）。版本化定义，主键 (agent_id, version)，
--      UNIQUE 天然防重；capability_refs 以 JSON 数组存本表，绑定引用校验
--      （指向不存在工具即拒绝）在 domain::agent 纯函数 + P7-2 Registry
--      接线层执行，不走 DB 外键（工具存在性属 tool/tool_version 语义面）。
--   2. agent_run：AgentRun（run_id、task_id、agent_id、state、memory_refs；
--      计划 §12:700）。run 钉定定义 (agent_id, agent_version)，FK 指向
--      agent_definition，定义存在才可建 run（数据面完整性）。
--
-- 红线（ROADMAP §13）：新迁移只在开发库副本与测试内存库执行，
-- migrations/platform.db 与 migrations/platform-dryrun.db 一律不写。
--
-- 幂等：CREATE TABLE IF NOT EXISTS + CREATE INDEX IF NOT EXISTS +
-- INSERT OR IGNORE + PRAGMA user_version = 6，可重复执行；
-- 由 crates/database 迁移执行器（migrate.rs）按 user_version 跳过。
-- ============================================================================

-- ---------------------------------------------------------------------------
-- agent_definition：Agent 定义（版本化）
--   capability_refs_json 形状：[{"tool_id": "rip.doc.parse", "version": "0.1.0"|null}, ...]
--   （domain::agent::CapabilityRef 序列化；绑定校验见其纯函数与 P7-2 接线）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS agent_definition (
  agent_id             TEXT NOT NULL,
  version              TEXT NOT NULL,
  name                 TEXT NOT NULL,                 -- 人类可读名
  context_policy       TEXT,                          -- 上下文策略原文（JSON 文本，Phase 4 契约对齐前透传）
  capability_refs_json TEXT NOT NULL DEFAULT '[]',    -- 绑定引用数组 JSON
  created_at           TEXT NOT NULL,                 -- ISO8601 UTC
  updated_at           TEXT NOT NULL,
  schema_version       INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (agent_id, version)
);

-- ---------------------------------------------------------------------------
-- agent_run：Agent 运行
--   state CHECK 六值与 domain::agent::AgentRunState 一致（P7-2 运行时扩展
--   该词表时须升迁移；P7-1 已冻结保守核心面）。
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS agent_run (
  run_id          TEXT PRIMARY KEY,                    -- RunId（全局唯一，审计链 correlation_id）
  task_id         TEXT,                                -- 关联研究任务（audit 链；无 FK，与 tool_run 同口径）
  agent_id        TEXT NOT NULL,
  agent_version   TEXT NOT NULL,                       -- 本次运行钉定的定义版本（重放依据）
  state           TEXT NOT NULL DEFAULT 'QUEUED'
                   CHECK (state IN ('QUEUED','PAUSED','RUNNING','SUCCEEDED','FAILED','CANCELLED')),
  memory_refs_json TEXT NOT NULL DEFAULT '[]',         -- 记忆引用数组 JSON（P7-4 Memory 面预留）
  created_at      TEXT NOT NULL,
  started_at      TEXT,                                -- RUNNING 进入时刻
  finished_at     TEXT,                                -- 终态进入时刻
  updated_at      TEXT NOT NULL,
  schema_version  INTEGER NOT NULL DEFAULT 1,
  FOREIGN KEY (agent_id, agent_version)
    REFERENCES agent_definition (agent_id, version) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS ix_agent_run_agent  ON agent_run(agent_id);
CREATE INDEX IF NOT EXISTS ix_agent_run_task   ON agent_run(task_id);
CREATE INDEX IF NOT EXISTS ix_agent_run_state  ON agent_run(state);

-- ---------------------------------------------------------------------------
-- schema_version：Phase 7 agent 数据面变更登记（version=6；1..5 见各自登记行）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (6, 'V1 additive Phase 7 agent data plane (agent_definition/agent_run)',
   'Phase 7 P7-1 提前通道：agent_definition（版本化定义 + capability_refs_json 绑定面，计划 §12:699）+ agent_run（钉定定义版本的运行表，state 六值 CHECK，计划 §12:700）建表；编号按 ROADMAP §1.4 实际合入顺序取 006；只在开发库副本与测试内存库执行，platform.db 与 platform-dryrun.db 不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '006_agent_data_plane.sql');

-- user_version 双写（DEC-002 条件，与 001_init.sql 尾部同款）
PRAGMA user_version = 6;
