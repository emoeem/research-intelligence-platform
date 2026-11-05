-- ============================================================================
-- Research Intelligence Platform — M1 第二批增量迁移（隔离环境 dry-run）
-- ============================================================================
-- 范围（任务书第二批）：
--   1. research_task 表：RTIS task JSON（35 个）→ ResearchTask 实体，
--      task_id 主键沿用现网 ID（rtis-YYYYMMDD-HHMMSS-ms）。
--   2. schema_version 登记 version=2（第二批追加变更的版本记录）。
--
-- 说明（版本策略，如实记录）：
--   * PRAGMA user_version 在 M1 dry-run 阶段锚定 V1 基线（001_init.sql 每次幂等
--     执行都会重申 user_version=1）；本文件的追加变更以 schema_version 表
--     version=2 行登记。正式的 user_version 递增机制随 M2 Rust migration
--     框架统一落地，不在 dry-run 阶段引入双源真相。
--   * 另有两处第二批增量 DDL 因 SQLite 不支持条件 DDL（无 ADD COLUMN IF NOT
--     EXISTS / 无条件 CHECK 扩域），由脚本守卫执行、本文件作权威记录：
--       a) evidence 表追加 payload_json TEXT 列（etl_evidence.py ensure 逻辑），
--          用于全字段保真承载 assets.db 真实证据行结构；
--       b) entity_alias 的 entity_type CHECK 域扩展加入 'research_task'
--          （etl_tasks.py 重建表守卫逻辑），使任务行可按 K-09 四要素登记别名。
--
-- 幂等：CREATE TABLE IF NOT EXISTS + INSERT OR IGNORE，可重复执行。
-- ============================================================================

-- ---------------------------------------------------------------------------
-- ResearchTask（源：RTIS /home/emo/.dsh/redteam/rtis/data/tasks/*.json，35 个）
--   研究状态字段语义原样保留：state 列取源 task.status 原值
--   （CREATED / AUTHORIZATION_REVIEW / LOCAL_KNOWLEDGE_RETRIEVAL /
--    AUTHORIZED_EXECUTION / COMPLETED），不做归一化映射。
--   status_history 为源 JSON 数组原样序列化（json.dumps ensure_ascii=False,
--   sort_keys=True），迁移无语义损失。
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS research_task (
  task_id        TEXT PRIMARY KEY,       -- 现网稳定 ID：rtis-YYYYMMDD-HHMMSS-ms
  title          TEXT,
  objective      TEXT NOT NULL,
  target         TEXT,
  task_type      TEXT,                   -- recon | vuln | full（源值直通）
  state          TEXT NOT NULL,          -- 源 task.status 原值（研究状态，不归一化）
  status_history TEXT NOT NULL,          -- 源 task.status_history JSON 数组（保真）
  source_file    TEXT NOT NULL,          -- 源 JSON 文件名（追溯线索）
  source_system  TEXT NOT NULL,          -- 'rtis-tasks'
  created_at     TEXT NOT NULL,          -- 源 task.created_at
  updated_at     TEXT NOT NULL,          -- 源 task.updated_at
  schema_version INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS ix_research_task_state ON research_task(state);
CREATE INDEX IF NOT EXISTS ix_research_task_type  ON research_task(task_type);

-- ---------------------------------------------------------------------------
-- schema_version：第二批追加变更登记（version=2）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (2, 'V1 additive batch2 (research_task)',
   'M1 第二批：research_task 建表（35 个 RTIS task 映射）；evidence.payload_json 追加列与 entity_alias 类型域扩展由 etl_evidence.py / etl_tasks.py 守卫执行（SQLite 无条件 DDL），本行仅为版本登记',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '002_tasks.sql');
