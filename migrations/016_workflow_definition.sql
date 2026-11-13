-- ============================================================================
-- Research Intelligence Platform : Phase 8 workflow 定义版本化增量迁移
--   （编排批次 P8-1 第 1/4 批，ROADMAP §9.3 P8-1：Workflow DSL 与定义版本化）
-- ============================================================================
-- 编号依据（ROADMAP §1.4 迁移编号冻结规则）：015 已被 trace_correlation
-- 占用，本文件取下一可用编号 016。
--
-- 范围（一张新表 + 一个索引 + schema_version 登记；均为新增结构，零
-- ALTER 语句，不改任何既有表与 CHECK 词值域）：
--   workflow_definition 表：definition-v1 图 DSL 的版本化定义。
--     主键 (workflow_id, version) 复合键：同 workflow 多版本并存，
--     put 语义为每版追加（存储侧分配版本号 = 现有最大版本 + 1），
--     历史行不可变（无 UPDATE 路径），get <id> --version N 回读历史版
--     definition_json 原文逐字节一致。
--   不动 003 workflow 表（反重复建模约束，见下方块）。
--
-- 反重复建模约束（与 003 workflow 表的边界）：
--   003 workflow 表承载 RTIS 方法层定义（实查 13 行 WF-00..WF-12：
--   trigger/requires/related_skills/knowledge_queries/phases/
--   stop_conditions/excluded_activities JSON），语义是「什么证据触发、
--   分几阶段、用哪些技能、何时停」，不是可执行节点图（无 nodes/edges）。
--   本表承载 definition-v1 可执行图 DSL（nodes/edges + input_schema）。
--   两表语义不同，不合并不复制列；存量行经 rp-workflow import-legacy 以
--   compat='legacy-method-reference' 只读参考导入（空图 + 原文快照逐字段
--   落 legacy_json 列），不为兼容旧形态扭曲 DSL（ROADMAP §9.5 风险 2）。
--   compat CHECK 域两值与 domain::WorkflowCompat 词表一一对应。
--
-- WorkflowRun 持久化面（run 表 + 节点状态表）不在本迁移：属 P8-2/P8-3
--   批次（迁移 17 号段待占）；run 状态词表与跃迁表已冻结在
--   crates/domain/src/workflow.rs（P8-1 域模型先于持久化落面）。
--
-- DDL 说明（如实记录）：本文件全部语句幂等（IF NOT EXISTS / INSERT OR
--   IGNORE），可在同一库重复执行（P8-1 自验第 4 条：临时库重放两次
--   user_version 与表清单两次一致）；执行器（crates/database migrate.rs）
--   按 user_version 单次执行语义跳过已达版本的库。
--
-- 红线（ROADMAP §13）：新迁移只在开发库副本与测试内存库执行；
--   migrations/platform.db 与 platform-dryrun.db 一律不写。
-- ============================================================================

-- ---------------------------------------------------------------------------
-- workflow_definition：definition-v1 版本化定义（P8-1）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS workflow_definition (
  workflow_id       TEXT NOT NULL,              -- domain::WorkflowId（非空 <=128）
  version           INTEGER NOT NULL CHECK (version >= 1),
                    -- 存储侧分配（现有最大版本 + 1），每版追加不可变
  name              TEXT NOT NULL,
  description       TEXT,
  definition_json   TEXT NOT NULL,              -- definition-v1 文档原文（put 不规整，回读逐字节一致）
  input_schema_json TEXT,                       -- 展开列（查询面用），与 definition_json 内一致
  node_count        INTEGER NOT NULL CHECK (node_count >= 0),
  edge_count        INTEGER NOT NULL CHECK (edge_count >= 0),
  compat            TEXT NOT NULL DEFAULT 'graph-v1'
                    CHECK (compat IN ('graph-v1','legacy-method-reference')),
                    -- 词表 = domain::WorkflowCompat；legacy 参考行空图 + legacy_json 快照
  source            TEXT NOT NULL DEFAULT 'rp-workflow-put',
                    -- rp-workflow-put | rp-workflow-import-legacy
  legacy_json       TEXT,                       -- 003 存量行原文快照（仅 legacy-method-reference 行有值）
  created_at        TEXT NOT NULL,
  schema_version    INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (workflow_id, version)
);

CREATE INDEX IF NOT EXISTS ix_workflow_definition_workflow
  ON workflow_definition(workflow_id);

-- ---------------------------------------------------------------------------
-- schema_version：Phase 8 workflow 定义版本化变更登记（version=16；1..15 见各自登记行）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (16, 'V1 additive Phase 8 workflow definition versioning (workflow_definition)',
   'Phase 8 P8-1（ROADMAP §9.3 P8-1）：workflow_definition 表（workflow_id+version 复合主键，put 每版追加历史不可变，definition_json 存 put 原文回读逐字节一致）+ workflow_id 索引；compat CHECK 域两值（graph-v1 | legacy-method-reference）与 domain::WorkflowCompat 词表一一对应；003 workflow 表不动（实查 13 行为 RTIS 方法层定义非可执行图，经 rp-workflow import-legacy 只读参考导入，legacy 原文快照落 legacy_json 列，DSL 不扭曲）；WorkflowRun 持久化面属 P8-2/P8-3（17 号段待占）；只在开发库副本与测试内存库执行，platform*.db 不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '016_workflow_definition.sql');

-- user_version 双写（DEC-002 条件，与 001_init.sql 尾部同款）
PRAGMA user_version = 16;
