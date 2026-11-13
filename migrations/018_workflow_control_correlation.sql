-- ============================================================================
-- Research Intelligence Platform : Phase 8 workflow 控制面与六 ID 关联增量迁移
--   （编排批次 P8-3 第 3/4 批，ROADMAP §9.3 P8-3「Agent 节点 + 长任务执行
--   （暂停/恢复/失败重试/版本管理）」：workflow 级协作暂停/恢复控制面 +
--   人工介入（awaiting_approval）记录 + workflow_run 六 ID 关联列）
-- ============================================================================
-- 编号依据（ROADMAP §1.4 迁移编号冻结规则）：017 已被 workflow_run 实际占用
-- （P8-2 先到先得），本文件实际取 018。
--
-- 范围（一张新表 + 八个追加列 + 三个索引 + schema_version 登记；控制表为
-- 新增结构，追加列全部可空 NULL 不回填既有行，零既有列改动、零 CHECK 词值
-- 域改动）：
--   workflow_run_control  run 控制请求与人工介入记录（1:1 对 workflow_run，
--                         014 agent_run_control 同款 consume-on-observe 语义，
--                         不重复实现控制面：agent 节点的暂停/取消/重试一律
--                         经 agent-engine RunControl 与 014 控制行，本表只
--                         承载 workflow 级请求位与 awaiting_approval 记录）；
--   workflow_run          追加六 ID 关联列（015 trace 面同款列名）：trace_id
--                         / task_id / agent_run_id / tool_run_id / artifact_id
--                         / evidence_id，全部可空；
--   workflow_run_node     追加实体链接列：tool_run_id / agent_run_id（节点
--                         与实体 1:1，节点级对账锚）。
--
-- 关联拓扑（015 trace 面对账，行级）：
--   workflow_run.trace_id = run_id（本链锚，创建即写；P8-2 已有惯例：
--   tool_run.trace_id = workflow run_id、Broker 装配 correlation_id = run_id，
--   本列把该惯例补全为可查列）；
--   workflow_run.task_id  = run 输入载荷顶层 task_id（字符串即取，无则 NULL）；
--   workflow_run.agent_run_id / tool_run_id / artifact_id / evidence_id =
--   run 内最近一个实体（1:N 全集经 trace_id 查 v_trace_chain / /api/traces，
--   单列承载最近值做快速对账锚，不复制全集）；
--   workflow_run_node.tool_run_id = tool 节点的 tool_run（submit_key
--   wf:<run_id>:<node_id>:a<attempt> 精确对账）；
--   workflow_run_node.agent_run_id = agent 节点的 agent_run（agent 循环
--   trace_id = workflow run_id，014 控制行随 agent run 落）。
--
-- 控制语义（D-TC-04 协作式，consume-on-observe，先例 014 / tool_run.cancel_requested_at）：
--   pause_requested_at    写入即请求，run 循环波间消费（等当前节点完成并落
--                         checkpoint 后转 paused，运行中节点不抢占）；
--   cancel_requested_at   取消请求留痕（run 头行直接跃迁 cancelled，本列记
--                         请求时刻与向运行中实体传播的事实）；
--   approval_node_id / approval_request_json / approval_json
--                         人工介入记录：human 节点执行即写前两列（停在
--                         awaiting_approval = approval_node_id 非空且
--                         approval_json 为空），批准参数经 resume 注入后写
--                         approval_json（ approves 完成记录，不清空
--                         approval_node_id，留节点对账锚）。
--
-- DDL 说明（如实记录，002/015 迁移同款先例）：SQLite 无 ADD COLUMN IF NOT
-- EXISTS 条件 DDL，本文件的 ALTER TABLE 语句依赖迁移执行器
-- （crates/database migrate.rs）的 user_version 单次执行语义：版本号已达
-- 18 的库不会重放本文件；同一库手工重复执行会报 duplicate column name，
-- 属预期守卫而非缺陷。
--
-- 红线（ROADMAP §13）：新迁移只在开发库副本与测试内存库执行；
--   migrations/platform.db 与 platform-dryrun.db 一律不写。
--
-- 幂等：CREATE TABLE IF NOT EXISTS + CREATE INDEX IF NOT EXISTS +
-- INSERT OR IGNORE + PRAGMA user_version = 18；ALTER TABLE 部分见上方
-- DDL 说明，由执行器 user_version 语义保证单次执行。
-- ============================================================================

-- ---------------------------------------------------------------------------
-- workflow_run_control：run 控制请求与人工介入记录（1:1 对 workflow_run 行）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS workflow_run_control (
  run_id               TEXT NOT NULL,
                       -- domain::RunId（workflow_run.run_id 同链）
  pause_requested_at   TEXT,          -- 协作式暂停请求时刻（消费即清空）
  cancel_requested_at  TEXT,          -- 取消请求留痕（不消费，历史面）
  approval_node_id     TEXT,          -- 人工介入节点（awaiting/approved 对账锚）
  approval_request_json TEXT,         -- 人工介入请求载荷（prompt/请求形状）
  approval_json        TEXT,          -- 批准参数（resume 注入后写入，NULL = 未批准）
  approval_updated_at  TEXT,          -- 人工介入记录最近更新时刻
  created_at           TEXT NOT NULL,
  updated_at           TEXT NOT NULL,
  schema_version       INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (run_id)
);

CREATE INDEX IF NOT EXISTS ix_workflow_run_control_awaiting
  ON workflow_run_control(approval_node_id, approval_json);

-- ---------------------------------------------------------------------------
-- workflow_run 六 ID 关联列（全部可空 TEXT，无默认值：既有行不回填）
-- ---------------------------------------------------------------------------
ALTER TABLE workflow_run ADD COLUMN trace_id TEXT;
ALTER TABLE workflow_run ADD COLUMN task_id TEXT;
ALTER TABLE workflow_run ADD COLUMN agent_run_id TEXT;
ALTER TABLE workflow_run ADD COLUMN tool_run_id TEXT;
ALTER TABLE workflow_run ADD COLUMN artifact_id TEXT;
ALTER TABLE workflow_run ADD COLUMN evidence_id TEXT;

CREATE INDEX IF NOT EXISTS ix_workflow_run_trace ON workflow_run(trace_id);
CREATE INDEX IF NOT EXISTS ix_workflow_run_task  ON workflow_run(task_id);

-- ---------------------------------------------------------------------------
-- workflow_run_node 实体链接列（可空 TEXT：节点与实体 1:1）
-- ---------------------------------------------------------------------------
ALTER TABLE workflow_run_node ADD COLUMN tool_run_id TEXT;
ALTER TABLE workflow_run_node ADD COLUMN agent_run_id TEXT;

CREATE INDEX IF NOT EXISTS ix_workflow_run_node_agent ON workflow_run_node(agent_run_id);

-- ---------------------------------------------------------------------------
-- schema_version：Phase 8 workflow 控制面与关联变更登记（version=18；1..17 见各自登记行）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (18, 'V1 additive Phase 8 workflow control plane and six-ID correlation (workflow_run_control + correlation columns)',
   'Phase 8 P8-3（ROADMAP §9.3 P8-3）：workflow_run_control 控制表（1:1 控制请求与人工介入记录：pause/cancel 请求位 consume-on-observe（D-TC-04 语义同款 014 先例，agent 节点控制面复用 agent-engine RunControl 与 014 控制行不重复实现）+ approval_node_id/approval_request_json/approval_json 人工介入 awaiting_approval 记录与批准注入）+ workflow_run 追加六 ID 关联列（trace_id/task_id/agent_run_id/tool_run_id/artifact_id/evidence_id，015 trace 面同款列名，1:N 全集经 trace_id 查 v_trace_chain，单列承载最近值）+ workflow_run_node 追加 tool_run_id/agent_run_id 节点级实体链接列；三个索引；ALTER TABLE 依赖执行器 user_version 单次执行语义（SQLite 无条件 DDL，002/015 先例）；只在开发库副本与测试内存库执行，platform*.db 不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '018_workflow_control_correlation.sql');

-- user_version 双写（DEC-002 条件，与 001_init.sql 尾部同款）
PRAGMA user_version = 18;
