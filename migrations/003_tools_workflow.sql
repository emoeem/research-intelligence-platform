-- ============================================================================
-- Research Intelligence Platform : M6 增量迁移（Tool & Workflow Platform，隔离环境 dry-run）
-- ============================================================================
-- 范围（计划 §M6 / 任务书 M6）：
--   1. workflow 表：RTIS workflows.yaml（13 个工作流定义）→ Workflow Definition 实体，
--      workflow_id 主键沿用源 id（WF-00..WF-12）。trigger/requires/related_skills/
--      knowledge_queries/phases/stop_conditions/excluded_activities 以 JSON 保真承载。
--   2. workflow_registry_meta：registry 级元数据（version / last_reviewed /
--      default_workflow），单行。M0 报告 §3.2 声明 WF-00 为 default_workflow，
--      以此表 + workflow 行存在性在 verify §11 验证。
--   3. tool_execution 表：空结构（exec_id/tool_id/argv_hash/exit_code/duration_ms/
--      started_at/permission_ctx），备 M5 工具执行记录使用，本阶段 0 行。
--   4. etl_run_log 表：etl_m6.py 的库内运行日志（每次运行一行，JSON 记录内容摘要/
--      行数/统计/drift/隔离清单）。运行时刻证据只进本表与 stdout，不进内容行，
--      保证内容确定性（幂等）。
--   5. schema_version 登记 version=3。
--
-- 说明（版本策略与守卫 DDL，如实记录，与 002_tasks.sql 同一口径）：
--   * 本文件只包含幂等 DDL（IF NOT EXISTS / INSERT OR IGNORE），可重复执行。
--   * 三处 M6 增量 DDL 因 SQLite 不支持条件 DDL（无 ADD COLUMN IF NOT EXISTS），
--     由 etl_m6.py 守卫执行，本文件为权威记录：
--       a) tool 表追加列 purpose TEXT / provenance_ref TEXT / health_note TEXT
--          （purpose 为 registry 源字段；provenance_ref 指向 registry 条目；
--            health_note 承载健康注记，如 fscan/suo5 坏版本探测说明）。
--       b) tool_version 重建：version 由 NOT NULL 放宽为 NULL-able（承接
--          "版本探测 broken → version 置 NULL"，fscan/gogo/pd-httpx/rtis.py/
--          mcp/datastore 类无版本输出），并追加 health 列
--          （CHECK 域 unknown|ok|degraded|broken）。健康状态与安装状态分离，
--          落实评审要求"定义/安装/健康/授权四态分离"。重建时表为空（0 行），无数据损失。
--       c) entity_alias.entity_type CHECK 域扩展加入 'workflow'
--          （同 002 对 research_task 的重建扩展模式）。
-- ============================================================================

-- ---------------------------------------------------------------------------
-- Workflow Definition（源：RTIS /home/emo/.dsh/redteam/rtis/data/registry/workflows.yaml）
--   方法与组织层定义：什么证据触发、分几阶段、用哪些技能/知识查询、何时停。
--   不是自动攻击脚本。JSON 字段序列化规则与 etl_tasks.py 一致：
--   json.dumps(ensure_ascii=False, sort_keys=True)。
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS workflow (
  workflow_id         TEXT PRIMARY KEY,      -- 源 id 沿用：WF-00..WF-12
  name                TEXT NOT NULL,
  description         TEXT,
  trigger_json        TEXT,                  -- trigger 块 JSON（dimensions/signals/business/always）
  requires_json       TEXT,                  -- requires 维度数组 JSON（缺失不可判定维度）
  related_skills      TEXT,                  -- skill name 数组 JSON
  knowledge_queries   TEXT,                  -- knowledge-first gate 固定查询数组 JSON
  phases              TEXT,                  -- 阶段数组 JSON（phase/role/action/output）
  stop_conditions     TEXT,                  -- 必停条件数组 JSON
  excluded_activities TEXT,                  -- 明确不做的事数组 JSON（防越界）
  source_system       TEXT NOT NULL,         -- 'rtis-workflows'
  source_file         TEXT NOT NULL,         -- 源 YAML 文件绝对路径
  created_at          TEXT NOT NULL,         -- 源文件 mtime（UTC，确定性）
  updated_at          TEXT NOT NULL,
  schema_version      INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS ix_workflow_name ON workflow(name);

-- ---------------------------------------------------------------------------
-- Workflow registry 级元数据（单行，id=1）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS workflow_registry_meta (
  id              INTEGER PRIMARY KEY CHECK (id = 1),
  registry_version TEXT,
  last_reviewed   TEXT,
  default_workflow TEXT,
  source_file     TEXT NOT NULL,
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL,
  schema_version  INTEGER NOT NULL DEFAULT 1
);

-- ---------------------------------------------------------------------------
-- ToolExecution（空结构，备 M5）
--   工具执行记录：M6 仅建结构不写数据。argv_hash = 规范化 argv 的确定性哈希
--   （完整 argv 与目标不落库，避免在执行记录中固化靶标信息）。
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS tool_execution (
  exec_id        TEXT PRIMARY KEY,          -- 全局 execution 链 ID（M5 定义生成规则）
  tool_id        TEXT NOT NULL REFERENCES tool(tool_id),
  argv_hash      TEXT,                      -- 规范化 argv 的 sha256（hex）
  exit_code      INTEGER,
  duration_ms    INTEGER,
  started_at     TEXT,                      -- ISO8601 UTC
  permission_ctx TEXT,                      -- 执行时的权限上下文（T0/T1/T2 + 授权引用）
  schema_version INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS ix_tool_execution_tool ON tool_execution(tool_id);
CREATE INDEX IF NOT EXISTS ix_tool_execution_ts   ON tool_execution(started_at);

-- ---------------------------------------------------------------------------
-- etl_run_log：etl_m6.py 库内运行日志（任务书写路径约束：运行日志不落新文件，
-- 入库表承载；每次运行追加一行，内容摘要只覆盖本脚本职责域内容表）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS etl_run_log (
  run_seq         INTEGER PRIMARY KEY AUTOINCREMENT,
  etl_script      TEXT NOT NULL,
  etl_version     TEXT NOT NULL,
  started_at      TEXT NOT NULL,
  finished_at     TEXT NOT NULL,
  content_digests TEXT NOT NULL,            -- JSON：{table: digest}
  row_counts      TEXT NOT NULL,            -- JSON：{table: n}
  stats           TEXT NOT NULL,            -- JSON：统计（probe 分布/drift 数/隔离数等）
  detail          TEXT                      -- JSON：探测证据/drift 清单/隔离清单
);

-- ---------------------------------------------------------------------------
-- schema_version：M6 增量变更登记（version=3）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (3, 'V1 additive M6 (tools/workflow/skill data plane)',
   'M6：workflow + workflow_registry_meta + tool_execution + etl_run_log 建表；tool 追加 purpose/provenance_ref/health_note 列、tool_version 重建（version NULL-able + health 列）、entity_alias 类型域扩展 workflow 三项守卫 DDL 由 etl_m6.py 执行（SQLite 无条件 DDL），本行仅为版本登记',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '003_tools_workflow.sql');
