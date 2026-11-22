-- ============================================================================
-- Research Intelligence Platform : RIP3 MA-2 Caller 归属列与 per-caller 预算面
--   增量迁移（MA-2-TASK.md §0 D1/D2；MULTI-AGENT-ACCESS-PLAN.md §四 MA-2 行 :46）：
--
--   纯增量：四表各加一列 caller_id（可空 TEXT，NULL = 历史行/legacy 不回填，
--   018:85 「既有行不回填」手法同源）+ 四索引 + 一张新配置表 caller_budget
--   （026:5-6 「确需 ALTER 既有表，另立 027+，禁改历史迁移」的兑现——026 两表
--   行形状冻结不动，caller/api_key 写面零改动）。
--
--   归属列（D1）：
--   - research_task.caller_id / workflow_run.caller_id：本批落列零写面
--     （写面随 MA-3 节点角色绑定与 RTIS 派发面接线；MA-1-TASK:32 A8 承诺的
--     是归属列结构）。
--   - agent_run.caller_id / tool_run.caller_id：本批接线（agent-engine
--     caller 通道 D4 + tool-engine submit_for_caller D5.2/D7）。跨 run 并发帽
--     记账 = COUNT agent_run WHERE caller_id AND state IN ('QUEUED','RUNNING')
--     （D1）；速率帽窗口计数数据源 = tool_run.caller_id 行（D5.2）。
--   - 不建硬 FK（017:49-50 workflow_id 口径同源：SQLite 不建硬外键，存在性
--     由受理面把关）；caller_budget.caller_id 例外建真 FK→caller（026:52
--     api_key 先例）。
--
--   caller_budget（D2）：per-caller 预算配置（唯一写入口 = rp-admin
--   budget-set/budget-clear，D6；rp_server.py 校验路径零写）。
--   - max_parallel_subtasks：fan-out 并发帽，0 = 禁并行（缺行同）——并行是
--     新增放开面，缺省最严（RunControlConfig 默认六字段语义零改动）。
--   - max_concurrent_runs：跨 run 受理并发帽，0 = 不限——并发/速率帽是新增
--     限制面，缺省不限（既有 caller 行为零变化，显式 budget-set 才收紧）。
--   - rate_window_ms：速率窗口毫秒，> 0（缺省 60000）。
--   - rate_max_steps：窗口速率帽，0 = 不限。
--   - budget_max_steps_per_run：caller 级单 run 总步数帽，0 = 沿用
--     LoopConfig.max_tool_calls 既有。
--   caller 停用（revoked_at 非空）即其 budget 行随之失效（校验面 join 判定
--   语义沿用 026 D1 口径；行保留不删除，rp-admin caller-revoke 幂等）。
--
-- DDL 说明（如实记录，018:50-53 同款）：SQLite 无 ADD COLUMN IF NOT EXISTS
-- 条件 DDL，本文件的 ALTER TABLE 语句依赖迁移执行器（crates/database
-- migrate.rs）的 user_version 单次执行语义：版本号已达 27 的库不会重放本
-- 文件；同一库手工重复执行会报 duplicate column name，属预期守卫而非缺陷。
-- ALTER 语句逐行单行书写（migrate.rs:62 守卫按行识别的迁移文件纪律）。
--
-- 红线（ROADMAP §13）：新迁移只在开发库副本与测试内存库执行；
--   migrations/platform*.db 与 platform-dryrun.db 一律不写。
--
-- 幂等：CREATE TABLE IF NOT EXISTS + CREATE INDEX IF NOT EXISTS +
-- INSERT OR IGNORE + PRAGMA user_version = 27；ALTER TABLE 部分见上方
-- DDL 说明，由执行器 user_version 语义保证单次执行。
-- ============================================================================

-- ---------------------------------------------------------------------------
-- 四表 caller 归属列（可空 TEXT，无默认值：既有行不回填；027 一批四表一次
-- 到位，避免 028/029 反复动迁移链——D1）
-- ---------------------------------------------------------------------------
ALTER TABLE research_task ADD COLUMN caller_id TEXT;
ALTER TABLE workflow_run ADD COLUMN caller_id TEXT;
ALTER TABLE agent_run ADD COLUMN caller_id TEXT;
ALTER TABLE tool_run ADD COLUMN caller_id TEXT;

CREATE INDEX IF NOT EXISTS ix_research_task_caller ON research_task(caller_id);
CREATE INDEX IF NOT EXISTS ix_workflow_run_caller ON workflow_run(caller_id);
CREATE INDEX IF NOT EXISTS ix_agent_run_caller ON agent_run(caller_id);
CREATE INDEX IF NOT EXISTS ix_tool_run_caller ON tool_run(caller_id);

-- ---------------------------------------------------------------------------
-- caller_budget：per-caller 预算配置（D2；唯一写入口 rp-admin budget-set/
-- budget-clear；帽值由装配层（rp-agent 启动时）读入内存映射，引擎不直查
-- 本表——D7 分层纪律）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS caller_budget (
  caller_id               TEXT PRIMARY KEY REFERENCES caller(caller_id),
  max_parallel_subtasks   INTEGER NOT NULL DEFAULT 0
                            CHECK (max_parallel_subtasks >= 0),  -- 0 = 禁并行（缺行同）
  max_concurrent_runs     INTEGER NOT NULL DEFAULT 0
                            CHECK (max_concurrent_runs >= 0),    -- 0 = 不限
  rate_window_ms          INTEGER NOT NULL DEFAULT 60000
                            CHECK (rate_window_ms > 0),          -- > 0（D2）
  rate_max_steps          INTEGER NOT NULL DEFAULT 0
                            CHECK (rate_max_steps >= 0),         -- 0 = 不限
  budget_max_steps_per_run INTEGER NOT NULL DEFAULT 0
                            CHECK (budget_max_steps_per_run >= 0), -- 0 = 沿用 LoopConfig.max_tool_calls
  updated_at              TEXT NOT NULL CHECK (updated_at GLOB
                            '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z')
);

-- ---------------------------------------------------------------------------
-- schema_version：MA-2 Caller 归属列与 per-caller 预算面持久化登记
-- （version=27；链头 026_caller.sql 推进 26→27）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (27, 'V1 additive MA-2 caller attribution columns and caller_budget table',
   'MA-2（docs/rip3/MA-2-TASK.md §0 D1/D2/D3 + MULTI-AGENT-ACCESS-PLAN.md §四 MA-2 行）：纯增量四表 caller_id 可空列（research_task/workflow_run/agent_run/tool_run，NULL=历史行不回填，018 手法）+ 四索引 + caller_budget 配置表（caller_id 真 FK→caller，026:52 先例；max_parallel_subtasks 0=禁并行缺省最严 / max_concurrent_runs 0=不限 / rate_window_ms>0 缺省 60000 / rate_max_steps 0=不限 / budget_max_steps_per_run 0=沿用 LoopConfig.max_tool_calls——不对称是有意的：并行是新增放开面缺省关，帽是新增限制面缺省不限，既有 caller 行为零变化）。归属列不建硬 FK（017:49-50 口径）；workflow_run/research_task 写面随 MA-3 节点角色绑定与 RTIS 派发面接线（本批落列零写面），agent_run/tool_run 本批接线（agent-engine D4 通道 + tool-engine submit_for_caller D5.2）。写权分治（D6）：caller_budget 唯一写入口 rp-admin budget-set/budget-clear/caller-revoke；caller 停用即名下 budget 行失效（join 判定，026 口径）。JSON 冻结契约零改动（caller 走 Rust 参数面不进 ToolCallRequest，D4）。只在开发库副本与测试内存库执行，platform*.db 生产文件不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '027_caller_attribution.sql');

-- user_version 双写（DEC-002 条件，001/018/022/023/024/025/026 先例）
PRAGMA user_version = 27;
