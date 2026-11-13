-- ============================================================================
-- Research Intelligence Platform : Phase 8 workflow 运行状态持久化增量迁移
--   （编排批次 P8-2 第 2/4 批，ROADMAP §9.3 P8-2：非智能节点执行器 +
--   WorkflowRun 节点状态落库）
-- ============================================================================
-- 编号依据（ROADMAP §1.4 迁移编号冻结规则）：016 已被 workflow_definition
-- 占用，本文件取下一可用编号 017。
--
-- 范围（三张新表 + 三个索引 + schema_version 登记；均为新增结构，零 ALTER
-- 语句，不改任何既有表与 CHECK 词值域）：
--   workflow_run        run 头行：run 状态机六态词域 + 输入/输出/错误
--                       载荷 + workflow_version 钉定列（run 钉死定义版本，
--                       版本管理按 (workflow_id, workflow_version) 回放，
--                       P8-1 已冻结的字段口径）；
--   workflow_run_node   节点状态行：(run_id, node_id) 复合主键，五态词域
--                       （pending/running/succeeded/failed/skipped，词值 =
--                       domain::WorkflowNodeRunState）+ attempt 计数列
--                       （失败重试递增）+ output/error 载荷；
--   workflow_checkpoint 节点完成即持久化快照（LangGraph/Temporal 思路：
--                       每步落库，恢复从最近 checkpoint 续），只追加语义
--                       （无 UPDATE 路径），快照 = 全量节点状态投影。
--
-- 词表来源（不新增第三处词表）：
--   workflow_run.state        六态 = domain::WorkflowRunState（P8-1 冻结
--                             词值 queued/running/paused/succeeded/failed/
--                             cancelled），跃迁合法性在执行器按
--                             WorkflowRunState::can_transition 把关；
--   workflow_run_node.state   五态 = domain::WorkflowNodeRunState（P8-1
--                             冻结词值 pending/running/succeeded/failed/
--                             skipped）。
--
-- 数据面约定：
--   output_json / error_json / input_json / snapshot_json 一律存 JSON 串；
--   attempt INTEGER NOT NULL CHECK (attempt >= 0)，0 = 未开始执行；
--   checkpoint.seq 自增单调，resume 取 MAX(seq) 行恢复。
--
-- 红线（ROADMAP §13）：新迁移只在开发库副本与测试内存库执行；
--   migrations/platform.db 与 platform-dryrun.db 一律不写。
-- ============================================================================

-- ---------------------------------------------------------------------------
-- workflow_run：run 头行（P8-2 状态机六态）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS workflow_run (
  run_id            TEXT NOT NULL,
                    -- domain::RunId（tool_run 同款形状，非空 <=128；本表
                    -- run_id 与 audit correlation_id / tool_run.trace_id 同链）
  workflow_id       TEXT NOT NULL,
                    -- domain::WorkflowId；逻辑外键 workflow_definition
                    -- （SQLite 不建硬外键，存在性由执行器受理面把关）
  workflow_version  INTEGER NOT NULL CHECK (workflow_version >= 1),
                    -- run 钉死的定义版本（P8-1 WorkflowRun.workflow_version
                    -- 字段口径），不漂移到最新版
  state             TEXT NOT NULL DEFAULT 'queued'
                    CHECK (state IN ('queued','running','paused','succeeded',
                                     'failed','cancelled')),
                    -- 词值 = domain::WorkflowRunState 六值；跃迁表
                    -- can_transition 在执行器把关
  input_json        TEXT NOT NULL,
                    -- run 输入载荷（JSON object 串，执行初始 state）
  output_json       TEXT,
                    -- 终态 run 输出（JSON object 串：合并后的最终 state 投影）
  error_json        TEXT,
                    -- 终态错误载荷（JSON 串：node_id / code / message）
  created_at        TEXT NOT NULL,
  started_at        TEXT,
  finished_at       TEXT,
  schema_version    INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (run_id)
);

CREATE INDEX IF NOT EXISTS ix_workflow_run_workflow
  ON workflow_run(workflow_id, workflow_version);

-- ---------------------------------------------------------------------------
-- workflow_run_node：节点状态行（五态 + attempt）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS workflow_run_node (
  run_id            TEXT NOT NULL,
  node_id           TEXT NOT NULL,
                    -- domain 图节点 id（check_node_id 词形，非空 <=64）
  node_kind         TEXT NOT NULL
                    CHECK (node_kind IN ('start','end','tool','knowledge',
                                         'agent','human','condition','parallel')),
                    -- 词值 = domain::WorkflowNodeKind 八值（P8-2 执行器
                    -- 落 tool/knowledge/condition/parallel 四类；P8-3 落
                    -- agent 执行器与 human 人工介入节点（批内扩展：017 尚
                    -- 未入库提交，dev 副本可重建，CHECK 词值随 domain 词表
                    -- 扩展补入本文件，不新增第三处词表）；start/end 为图边界）
  state             TEXT NOT NULL DEFAULT 'pending'
                    CHECK (state IN ('pending','running','succeeded',
                                     'failed','skipped')),
                    -- 词值 = domain::WorkflowNodeRunState 五值
  attempt           INTEGER NOT NULL CHECK (attempt >= 0),
                    -- 已开始的执行次数（进入 running 时 +1；失败重试只
                    -- 重跑目标节点，attempt 递增，已完成节点不变）
  output_json       TEXT,
                    -- 终态成功输出（JSON object 串，按 node_id 并入 run state）
  error_json        TEXT,
                    -- 终态失败载荷（JSON 串：code/message；超时 = code
                    -- "node_timeout"）
  started_at        TEXT,
  finished_at       TEXT,
  updated_at        TEXT NOT NULL,
  schema_version    INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (run_id, node_id)
);

CREATE INDEX IF NOT EXISTS ix_workflow_run_node_run
  ON workflow_run_node(run_id, state);

-- ---------------------------------------------------------------------------
-- workflow_checkpoint：节点完成即持久化快照（只追加）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS workflow_checkpoint (
  seq               INTEGER NOT NULL,
                    -- 自增序号（AUTOINCREMENT 语义见下方定义；resume 取
                    -- MAX(seq) 行）
  run_id            TEXT NOT NULL,
  node_id           TEXT NOT NULL,
                    -- 触发本 checkpoint 的节点（该节点刚到达终态）
  node_state        TEXT NOT NULL
                    CHECK (node_state IN ('succeeded','failed','skipped')),
                    -- 触发时该节点的终态（checkpoint 只在节点到达终态时写）
  attempt           INTEGER NOT NULL CHECK (attempt >= 0),
  snapshot_json     TEXT NOT NULL,
                    -- 全量节点状态投影（JSON 串：{nodes:{<node_id>:
                    -- {state,attempt,output,error}}}，恢复 = 整体回放）
  created_at        TEXT NOT NULL,
  PRIMARY KEY (seq AUTOINCREMENT)
);

CREATE INDEX IF NOT EXISTS ix_workflow_checkpoint_run
  ON workflow_checkpoint(run_id, seq);

-- ---------------------------------------------------------------------------
-- schema_version：Phase 8 workflow 运行持久化变更登记（version=17；1..16 见各自登记行）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (17, 'V1 additive Phase 8 workflow run persistence (workflow_run/workflow_run_node/workflow_checkpoint)',
   'Phase 8 P8-2（ROADMAP §9.3 P8-2）：workflow_run 头行（run 六态 CHECK 域 = domain::WorkflowRunState，workflow_version 钉定列）+ workflow_run_node 节点状态行（五态 CHECK 域 = domain::WorkflowNodeRunState + attempt 计数 + output/error 载荷）+ workflow_checkpoint 节点终态快照只追加表（LangGraph/Temporal 思路：每步落库，恢复取 MAX(seq) 行续跑）；三个索引；词表不新增第三处（P8-1 domain::workflow 冻结词值即 CHECK 域）；Tool/Knowledge/Condition/Parallel 四类节点执行器在 crates/workflow-engine exec 模块落实现（Agent 节点留 P8-3）；node_kind 词域 P8-3 批内扩展补入 human（017 未入库提交，dev 副本可重建）；只在开发库副本与测试内存库执行，platform*.db 不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '017_workflow_run.sql');

-- user_version 双写（DEC-002 条件，与 001_init.sql 尾部同款）
PRAGMA user_version = 17;
