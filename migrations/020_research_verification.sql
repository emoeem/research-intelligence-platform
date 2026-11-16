-- ============================================================================
-- 020_research_verification.sql
-- RIP3 Phase 5（P5-2）研究编排域：Verification Plan / Item / Event 三表。
--
-- 存储判定（P5-0-B3 冻结）：研究编排执行态与 workflow_run/tool_run/agent_run
-- 同族（运营面），按铁律落 SQLite 020+ 小迁移；情报核（migrations_pg）本
-- 阶段零新表。RTIS 红线：research_plan_item.rtis_poc_id 只存 RTIS POC 数字
-- ID 引用（不复制 POC 正文/利用细节进 3.0 库）；Finding 事实不经本域落库
-- （五态 result_status 是研究结论状态，不是 RTIS Finding 事实）。
--
-- 幂等：CREATE TABLE IF NOT EXISTS + INSERT OR IGNORE（002/018 先例）。
-- 只在开发库副本与测试内存库执行；migrations/platform*.db 生产文件不写。
-- ============================================================================

CREATE TABLE IF NOT EXISTS research_plan (
  plan_id        TEXT PRIMARY KEY,       -- vp-YYYYMMDD-HHMMSS-ms-rand（domain ids 生成）
  task_ref       TEXT,                   -- RTIS research_task.task_id 引用（可空：独立验证计划）
  workspace_id   TEXT NOT NULL DEFAULT 'default',
  title          TEXT,
  target_summary TEXT,                   -- 目标摘要（host/url/画像指纹说明，展示用）
  status         TEXT NOT NULL DEFAULT 'draft'
                 CHECK (status IN ('draft','ready','executing','paused','completed','cancelled')),
  budget_json    TEXT,                   -- {"max_requests":N,"timeout_s":N,"concurrency":1,"max_rounds":N}
  schema_version INTEGER NOT NULL DEFAULT 1,
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS ix_research_plan_task ON research_plan(task_ref);
CREATE INDEX IF NOT EXISTS ix_research_plan_status ON research_plan(status);

CREATE TABLE IF NOT EXISTS research_plan_item (
  item_id             TEXT PRIMARY KEY,   -- vi-YYYYMMDD-HHMMSS-ms-rand
  plan_id             TEXT NOT NULL REFERENCES research_plan(plan_id),
  knowledge_ref       TEXT,               -- 3.0 knowledge 条目引用（knowledge_id）
  rtis_poc_id         INTEGER,            -- RTIS POC 数字 ID 引用（正文/细节不复制）
  provider_id         TEXT NOT NULL,      -- rtis-poc | nuclei | rip.tool.<id>（Provider 词表随 P5-5 冻结）
  provider_version    TEXT,
  target_ref          TEXT NOT NULL,      -- 规范化目标（host/scheme/port/url）
  dedup_fingerprint   TEXT NOT NULL,      -- sha256(target|poc/template|provider|params|authctx)（P5-4 去重键）
  applicability_status TEXT NOT NULL DEFAULT 'needs_context'
                      CHECK (applicability_status IN ('applicable','excluded','needs_context')),
  applicability_reasons_json TEXT NOT NULL DEFAULT '[]',
  risk_class          TEXT NOT NULL DEFAULT 'low_impact'
                      CHECK (risk_class IN ('passive','low_impact','active_controlled')),
  request_budget      INTEGER NOT NULL DEFAULT 10,
  timeout_budget_s    INTEGER NOT NULL DEFAULT 60,
  execution_order     INTEGER NOT NULL DEFAULT 0,
  expected_observation TEXT,              -- 预期观察（正向判据说明）
  -- 五态研究结论（Phase 5 补充计划 §5.7）：NULL = 计划未定稿；
  -- 工具退出码 0 ≠ CONFIRMED；执行失败/超时 ≠ NOT_REPRODUCED。
  result_status       TEXT
                      CHECK (result_status IS NULL OR result_status IN
                        ('CONFIRMED','NOT_REPRODUCED','NOT_TESTED','INCONCLUSIVE','FALSE_POSITIVE')),
  result_reason       TEXT,               -- 判定依据（CONFIRMED/FALSE_POSITIVE 必填，应用层守卫）
  error_category      TEXT
                      CHECK (error_category IS NULL OR error_category IN
                        ('timeout','network','auth','tool_unavailable','assertion','budget_exhausted','unknown')),
  tool_run_id         TEXT,               -- tool_run.run_id 关联（执行面 P5-6 派发）
  trace_id            TEXT,               -- 六 ID 关联锚（015 trace_correlation 同源）
  schema_version      INTEGER NOT NULL DEFAULT 1,
  created_at          TEXT NOT NULL,
  updated_at          TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS ix_research_plan_item_plan ON research_plan_item(plan_id);
CREATE INDEX IF NOT EXISTS ix_research_plan_item_fp   ON research_plan_item(dedup_fingerprint);
CREATE INDEX IF NOT EXISTS ix_research_plan_item_run  ON research_plan_item(tool_run_id);
CREATE INDEX IF NOT EXISTS ix_research_plan_item_state ON research_plan_item(result_status);

CREATE TABLE IF NOT EXISTS research_plan_event (
  event_id    TEXT PRIMARY KEY,           -- ve-YYYYMMDD-HHMMSS-ms-rand
  plan_id     TEXT,
  item_id     TEXT,
  tool_run_id TEXT,                       -- 派发后的 tool_run 关联
  trace_id    TEXT,
  event_type  TEXT NOT NULL,              -- plan_created|plan_status|item_planned|item_skipped|
                                          -- dispatched|result_assigned|budget_updated|cancelled
  payload_json TEXT,
  created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS ix_research_plan_event_plan ON research_plan_event(plan_id);
CREATE INDEX IF NOT EXISTS ix_research_plan_event_item ON research_plan_event(item_id);

-- ---------------------------------------------------------------------------
-- schema_version：RIP3 Phase 5 研究编排域登记（version=20；1..19 见各自登记行）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (20, 'V1 additive RIP3 Phase 5 research orchestration (research_plan + research_plan_item + research_plan_event)',
   'RIP3 P5-2（补充计划 §7 P5-2）：研究编排域三表（Verification Plan/Item/Event；编排容器按 P5-0-B3 判定落运营面，情报核零新表）。五态研究结论 CONFIRMED/NOT_REPRODUCED/NOT_TESTED/INCONCLUSIVE/FALSE_POSITIVE 与任务状态分离（result_status 列 CHECK 词表；CONFIRMED/FALSE_POSITIVE 的 reason 必填由应用层守卫承载）；rtis_poc_id 只存 RTIS POC 数字 ID 引用（POC 正文/利用细节零复制，RTIS 红线）；dedup_fingerprint 为 P5-4 四层去重的验证层键（sha256(target|poc|provider|params|authctx)）；tool_run_id/trace_id 六 ID 关联锚沿用 015 面；只在开发库副本与测试内存库执行，platform*.db 生产文件不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '020_research_verification.sql');

-- user_version 双写（DEC-002 条件，001/018 先例）
PRAGMA user_version = 20;
