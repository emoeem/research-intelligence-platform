-- ============================================================================
-- Research Intelligence Platform : Phase 7 运行控制与 AgentRun 持久化轨迹增量迁移
--   （编排批次 P7-4 第 4/7 批，内容对应 ROADMAP §8.3 P7-5「运行控制」行：
--   暂停、恢复、取消、重试、停滞检测、重复调用检测、目标偏移检测、
--   证据不足提示、预算与资源限制；计划 §9:474 能力清单同批落地）
-- ============================================================================
-- 编号依据（ROADMAP §1.4 迁移编号冻结规则）：按实际合入顺序递增，本文件
-- 实际取 014（实测 ls migrations/*.sql 最新为 013_agent_memory.sql）。
--
-- 范围（两张新表，均为追加结构，不改既有表、不改 006 agent_run 的 state
-- CHECK 词值域：六值域 QUEUED/PAUSED/RUNNING/SUCCEEDED/FAILED/CANCELLED
-- 已覆盖全部控制语义，控制请求不新增状态词值）：
--   1. agent_run_control：run 控制请求与续接上下文（1:1 对 agent_run）。
--      协作式取消与暂停（D-TC-04 语义，先例 tool_run.cancel_requested_at，
--      migrations/005_tool_contract.sql）：控制面只写请求时刻，执行循环在
--      步间观察（consume-on-observe，消费即清空，一次性信号）；直接取消
--      路径（QUEUED/PAUSED）不走请求位，走 state 跃迁 CAS。
--      goal_text：run 目标原文落库（resume 状态续接的装配输入，run 行
--      本体不带 goal，落在本表避免动 006 表形状）。
--      retry_of_run_id：重试链（重试 = 新 run 重放同定义版本，domain
--      跃迁表不复活终态行，见 domain::agent LEGAL_TRANSITIONS 注）。
--   2. agent_run_step：run 内步骤轨迹（逐步骤追加，resume 状态续接依据：
--      暂停后续跑只执行轨迹之外的步骤，已完成步骤不重跑）。
--
-- 轨迹口径（ROADMAP §8.4 事件链）：agent_run_step 是持久化轨迹的权威面
-- （audit_event 是旁路日志，emit_best_effort 可缓冲不承诺持久）；两者同
-- correlation_id（run_id）可互相对账。
--
-- 反重复建模约束：
--   - agent_run_step 不重复 agent_task_memory（013）载荷：本表是控制面
--     轨迹（resume 续接依据），Task Memory 是记忆面（压缩/驱逐生命周期），
--     两表独立演进，互不引用。
--   - 不设 evidence 实体字段（captured_at/sensitivity/governance 等），
--     证据唯一登记面仍是 evidence 表（001_init.sql:250）。
--
-- 红线（ROADMAP §13）：新迁移只在开发库副本与测试内存库执行，
-- migrations/platform.db 与 migrations/platform-dryrun.db 一律不写。
--
-- 幂等：CREATE TABLE IF NOT EXISTS + CREATE INDEX IF NOT EXISTS +
-- INSERT OR IGNORE + PRAGMA user_version = 14，可重复执行；
-- 由 crates/database 迁移执行器（migrate.rs）按 user_version 跳过。
-- ============================================================================

-- ---------------------------------------------------------------------------
-- agent_run_control：run 控制请求与续接上下文（1:1 对 agent_run 行）
--   pause_requested_at / cancel_requested_at：协作式请求时刻（D-TC-04 语义，
--     先例 tool_run.cancel_requested_at）。执行循环步间消费（置 NULL），
--     请求只在状态机允许的发起态可写（写入面校验：pause 仅 RUNNING，
--     cancel 仅 RUNNING；QUEUED/PAUSED 的取消走直接跃迁）。
--   goal_text：目标原文（resume 重装配输入；fresh run 创建即写）。
--   retry_of_run_id：重试源 run（FAILED 重试 = 新 run 重放同定义版本）。
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS agent_run_control (
  run_id              TEXT PRIMARY KEY
                        REFERENCES agent_run(run_id),  -- RESTRICT：run 不删
  goal_text           TEXT NOT NULL DEFAULT '',        -- 目标原文（resume 装配输入）
  pause_requested_at  TEXT,                            -- 协作式暂停请求时刻（消费即清空）
  cancel_requested_at TEXT,                            -- 协作式取消请求时刻（消费即清空）
  retry_of_run_id     TEXT,                            -- 重试链：本 run 由哪个 FAILED run 重放而来
  created_at          TEXT NOT NULL,                   -- ISO8601 UTC
  updated_at          TEXT NOT NULL,
  schema_version      INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS ix_agent_run_control_retry
  ON agent_run_control(retry_of_run_id);

-- ---------------------------------------------------------------------------
-- agent_run_step：run 内步骤轨迹（逐步骤追加，resume 续接的持久化权威面）
--   seq：run 内步序（0 起，UNIQUE(run_id, seq) 防重放重复写）。
--   status：步骤终态词形（ToolRunStatus 词形；受理拒绝记 SUBMIT_REJECTED，
--     与 agent_loop::StepRecord.status 同词形）。
--   input_json：计划入参快照（轨迹可复算面；重复调用检测按 (tool_id, input)
--     规范键对账，resume 后重建已执行键集合）。
--   content_hash：登记产物 sha256（停滞检测钉面：连续步无新哈希即无新证据）。
--   tool_run_id 不设 FK：SUBMIT_REJECTED 步骤无 tool_run 行（受理期拒绝
--   不落 run 行，tool-engine 受理校验口径），FK 会拒绝该轨迹。
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS agent_run_step (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id        TEXT NOT NULL
                  REFERENCES agent_run(run_id),   -- RESTRICT：run 不删
  seq           INTEGER NOT NULL,                 -- run 内步序（0 起）
  round         INTEGER NOT NULL DEFAULT 0,       -- 产生轮次（0 起）
  tool_id       TEXT NOT NULL,
  tool_version  TEXT NOT NULL DEFAULT '',
  tool_run_id   TEXT NOT NULL DEFAULT '',         -- SUBMIT_REJECTED 为空串（无 run 行）
  status        TEXT NOT NULL,
  artifact_id   TEXT,
  evidence_id   TEXT,
  input_json    TEXT NOT NULL DEFAULT '{}',       -- 计划入参快照（规范键复算输入）
  content_hash  TEXT,                             -- 登记 artifact sha256（停滞检测钉面）
  summary       TEXT,
  error         TEXT,
  elapsed_ms    INTEGER NOT NULL DEFAULT 0,
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL,
  schema_version INTEGER NOT NULL DEFAULT 1,
  UNIQUE (run_id, seq)
);

CREATE INDEX IF NOT EXISTS ix_agent_run_step_run
  ON agent_run_step(run_id, seq);

-- ---------------------------------------------------------------------------
-- schema_version：Phase 7 运行控制数据面变更登记（version=14；1..13 见各自登记行）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (14, 'V1 additive Phase 7 run control (agent_run_control/agent_run_step)',
   'Phase 7 P7-4（编排第 4/7 批，ROADMAP §8.3 P7-5 内容行）：运行控制与 AgentRun 持久化轨迹：agent_run_control（1:1 控制请求与续接上下文：协作式 pause/cancel 请求位 consume-on-observe，D-TC-04 语义先例 tool_run.cancel_requested_at + goal_text resume 装配输入 + retry_of_run_id 重试链）+ agent_run_step（步骤轨迹，resume 续接权威面，已完成步骤不重跑，UNIQUE(run_id,seq)）；六值 state CHECK 不变，控制请求不新增状态词值；只在开发库副本与测试内存库执行，platform*.db 不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '014_agent_run_control.sql');

-- user_version 双写（DEC-002 条件，与 001_init.sql 尾部同款）
PRAGMA user_version = 14;
