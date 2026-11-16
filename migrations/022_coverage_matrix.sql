-- ============================================================================
-- Research Intelligence Platform : RIP3 Phase 5 P5-8 覆盖矩阵与自适应研究
--   增量迁移（补充计划 §7 P5-8 + §5.10 自适应研究引擎）：
--
--   1. research_plan_item 追加 dimension 列：七维覆盖矩阵归属
--      （Assets / API Endpoints / Identity & Permissions / Input Points /
--      Business Flows / File Processing / Data Flows，§5.10 词表；缺省
--      assets 与既有行兼容，additive ALTER，执行器守卫防 duplicate column）。
--   2. research_hypothesis 表：Research Hypothesis 持久层（§5.10 假设生成
--      必含字段全落：内容/来源依据/关联原因/前置条件/预期观察/负向对照/
--      建议验证方式/预估成本风险/停止条件 + 轮次与状态机）。纪律：假设
--      没有证据时不能生成 Finding（§5.10 原文）——hypothesis 是研究中间
--      面，CONFIRMED 只能经验证计划项的五态判定走到 finding-link。
--
--   幂等：CREATE TABLE IF NOT EXISTS + ALTER ADD COLUMN 行守卫（执行器）
--   + INSERT OR IGNORE（002/018/020/021 先例）。
--   只在开发库副本与测试内存库执行；migrations/platform*.db 生产文件不写。
-- ============================================================================

ALTER TABLE research_plan_item ADD COLUMN dimension TEXT NOT NULL DEFAULT 'assets' CHECK (dimension IN ('assets','api_endpoints','identity_permissions','input_points','business_flows','file_processing','data_flows'));

CREATE TABLE IF NOT EXISTS research_hypothesis (
  hypothesis_id  TEXT PRIMARY KEY,    -- hi-YYYYMMDD-HHMMSS-ms-rand
  plan_id        TEXT NOT NULL REFERENCES research_plan(plan_id),
  dimension      TEXT NOT NULL
                 CHECK (dimension IN ('assets','api_endpoints','identity_permissions','input_points','business_flows','file_processing','data_flows')),
  content        TEXT NOT NULL,       -- 假设内容
  source_evidence TEXT NOT NULL,      -- 来源依据（知识条目/画像/盲区信号；无证据不生成）
  association_reason TEXT NOT NULL,   -- 与目标的关联原因
  preconditions  TEXT,                -- 前置条件
  expected_observation TEXT,          -- 预期观察（正向判据）
  negative_control TEXT,              -- 负向对照（预期不出现的形态）
  suggested_verification TEXT,        -- 建议验证方式（Provider/参数面）
  cost_risk      TEXT,                -- 预估成本与风险
  stop_condition TEXT NOT NULL,       -- 停止条件（研究 Trace 可见）
  round_number   INTEGER NOT NULL DEFAULT 1,  -- 生成轮次（预算 max_rounds 守卫）
  status         TEXT NOT NULL DEFAULT 'proposed'
                 CHECK (status IN ('proposed','in_plan','validated','dropped')),
  schema_version INTEGER NOT NULL DEFAULT 1,
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS ix_research_hypothesis_plan ON research_hypothesis(plan_id);
CREATE INDEX IF NOT EXISTS ix_research_hypothesis_dim ON research_hypothesis(plan_id, dimension);

-- ---------------------------------------------------------------------------
-- schema_version：RIP3 Phase 5 研究编排域登记（version=22；1..21 见各自登记行）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (22, 'V1 additive RIP3 Phase 5 coverage matrix and adaptive research (dimension column + research_hypothesis)',
   'RIP3 P5-8（补充计划 §7 P5-8 + §5.10）：七维覆盖矩阵归属列（research_plan_item.dimension，ALTER ADD COLUMN additive，词表七值缺省 assets 与既有行兼容）+ Research Hypothesis 持久层（假设生成必含字段全落 + round_number 预算 max_rounds 守卫 + 状态机 proposed/in_plan/validated/dropped）。纪律：假设没有证据时不能生成 Finding（hypothesis 是研究中间面，CONFIRMED 只能经计划项五态判定走 finding-link）；研究停止原因经 stop_condition 与轮次守卫留痕（清晰可见）。只在开发库副本与测试内存库执行，platform*.db 生产文件不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '022_coverage_matrix.sql');

-- user_version 双写（DEC-002 条件，001/018 先例）
PRAGMA user_version = 22;
