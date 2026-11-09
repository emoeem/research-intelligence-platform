-- ============================================================================
-- Research Intelligence Platform : Phase 1 Tool Contract 增量迁移
--   （TOOL-CONTRACT-DRAFT §3.3 / §5.2 机械化，ROADMAP §2.3 P1-3）
-- ============================================================================
-- 编号依据（ROADMAP §1.4 迁移编号冻结规则）：迁移编号按合入顺序递增，先到先得。
-- 本文件合入 005（005 编号在 Phase 1 空闲，实测 ls migrations/*.sql 为 001 到 004）；
-- CONTEXT-BROKER-DESIGN §6.1 草案中的 005_context_broker.sql 编号让位于本规则，
-- Phase 4 落地时改用当时下一个可用编号。
--
-- 范围（Phase 1 工具运行数据面两表，均为追加结构，不改既有表）：
--   1. tool_run：ToolRun 生命周期主表（TOOL-CONTRACT §3 状态机八态 + §4.1
--      调用对象落地字段 + §3.3 与 tool_execution 的映射口径）。tool_execution
--      保持不动（003 建的空结构），合并方式待 Phase 1 迁移对账清单裁决。
--   2. artifact：运行产物登记表（TOOL-CONTRACT §5.2 + 计划 §12:698 字段清单
--      artifact_id / run_id / uri / hash / mime / size / provenance）。
--
-- 红线（ROADMAP §1.4）：新迁移只在开发库副本与测试内存库执行，
-- migrations/platform.db 与 migrations/platform-dryrun.db 一律不写。
--
-- 幂等：CREATE TABLE IF NOT EXISTS + CREATE INDEX IF NOT EXISTS +
-- INSERT OR IGNORE + PRAGMA user_version = 5，可重复执行；
-- 由 crates/database 迁移执行器（migrate.rs:32-80）按 user_version 跳过。
-- ============================================================================

-- ---------------------------------------------------------------------------
-- tool_run：ToolRun 生命周期主表
--   status 八态 CHECK 与 TOOL-CONTRACT §3.1 状态清单一一对应；
--   跃迁合法性校验属 tool-engine（P1-5），表只承载状态身份与时间戳。
--   列与 §3.3 映射口径对齐：exec_id 对应 run_id；argv_hash 对应
--   input_hash（规范化 input 脱敏后 sha256，D-TC-06）；duration_ms 对应
--   usage.elapsed_ms；started_at 对应 RUNNING 进入时刻；permission_ctx 存
--   permission_level + authorization_ref。
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS tool_run (
  run_id             TEXT PRIMARY KEY,             -- 服务端生成全局唯一（RunId，审计链 correlation_id）
  tool_id            TEXT NOT NULL REFERENCES tool(tool_id),
  tool_version       TEXT,                         -- 实际执行版本（§5.1，重放依据；Registry 解析后回填）
  task_id            TEXT,                         -- 关联研究任务（审计链用，§4.1 可空）
  workspace_id       TEXT NOT NULL,                -- 工作区隔离边界（§4.1 必填）
  submit_key         TEXT,                         -- 幂等提交键（§4.1；python_worker 直通 idempotency_key）
  input_ref          TEXT,                         -- 提交 input 的存储引用（计划 §12:697）
  input_refs_json    TEXT,                         -- §4.1 input_refs 数组 JSON（artifact_id+mount_as）
  input_hash         TEXT,                         -- 规范化 input 脱敏后 sha256 hex（§3.3 argv_hash 对应物）
  status             TEXT NOT NULL DEFAULT 'QUEUED'
                       CHECK (status IN ('QUEUED','WAITING_FOR_RUNNER','PREPARING','RUNNING',
                                         'SUCCEEDED','FAILED','TIMED_OUT','CANCELLED')),
  permission_ctx     TEXT,                         -- permission_level + authorization_ref（§3.3）
  budget_json        TEXT,                         -- §4.1 budget（timeout_seconds/max_output_bytes）JSON
  error_category     TEXT
                       CHECK (error_category IN ('VALIDATION','INPUT','TIMEOUT','CANCELLED',
                                                 'EXECUTION','POLICY_DENIED','DEPENDENCY','INFRA',
                                                 'RUNNER_UNAVAILABLE')
                              OR error_category IS NULL),   -- §5.3 九类，FAILED 期填写
  error_json         TEXT,                         -- §5.1 error 对象 JSON（category/code/message/retryable/detail_ref）
  duration_ms        INTEGER,                      -- usage.elapsed_ms 折叠列（§3.3 duration_ms 对应物）
  usage_json         TEXT,                         -- §5.1 usage 对象 JSON（elapsed_ms/output_bytes/truncated/cpu_ms/max_rss_bytes）
  environment_json   TEXT,                         -- §5.1 environment 版本与环境指纹 JSON
  replay_json        TEXT,                         -- §5.1 replay 对象 JSON（input_sanitized/submit_key/cached）
  created_at         TEXT NOT NULL,                -- QUEUED 落库时刻（ISO8601 UTC）
  started_at         TEXT,                         -- RUNNING 进入时刻（§3.3）
  cancel_requested_at TEXT,                        -- 取消请求时刻（§3.2 D-TC-04）
  finished_at        TEXT,                         -- 终态进入时刻（§3.2 终态不可迁出）
  updated_at         TEXT NOT NULL,                -- 最后一次状态跃迁时刻
  schema_version     INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS ix_tool_run_tool    ON tool_run(tool_id);
CREATE INDEX IF NOT EXISTS ix_tool_run_status  ON tool_run(status);
CREATE INDEX IF NOT EXISTS ix_tool_run_task    ON tool_run(task_id);
CREATE INDEX IF NOT EXISTS ix_tool_run_submit  ON tool_run(submit_key);

-- ---------------------------------------------------------------------------
-- artifact：运行产物登记表（§5.2 + 计划 §12:698 字段清单）
--   结构化协议行以外的 stdout/stderr 一律入本表（role=stdout/stderr），
--   不进入 result；sha256 小写 hex（与 schemas/tool-contract/result-v1.json
--   $defs.artifact 口径一致，长度 64 由 CHECK 兜底）。
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS artifact (
  artifact_id   TEXT PRIMARY KEY,                   -- 服务端登记时生成（ArtifactId）
  run_id        TEXT NOT NULL REFERENCES tool_run(run_id),
  role          TEXT
                  CHECK (role IN ('primary','attachment','stdout','stderr','log')
                         OR role IS NULL),          -- §5.2 role 词表
  uri           TEXT NOT NULL,                      -- Artifact Manager 存储地址（任务结束归档）
  sha256        TEXT NOT NULL CHECK (length(sha256) = 64),   -- 收集时计算登记（小写 hex）
  size_bytes    INTEGER NOT NULL CHECK (size_bytes >= 0),
  mime_type     TEXT NOT NULL,
  provenance    TEXT NOT NULL,                      -- 生成方式描述（工具输出 / 平台采集）
  created_at    TEXT NOT NULL,                      -- 登记时刻（ISO8601 UTC）
  schema_version INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS ix_artifact_run  ON artifact(run_id);
CREATE INDEX IF NOT EXISTS ix_artifact_role ON artifact(role);

-- ---------------------------------------------------------------------------
-- schema_version：Phase 1 tool contract 增量变更登记（version=5；
-- version=1..4 见 001/002/003/004 各自登记行）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (5, 'V1 additive Phase 1 tool contract (tool_run/artifact)',
   'Phase 1 P1-3：tool_run（ToolRun 生命周期八态主表，TOOL-CONTRACT §3.1/§3.3/§4.1 字段落地）+ artifact（运行产物登记表，§5.2/计划 §12:698 字段清单）建表；编号按 ROADMAP §1.4 先到先得规则取 005；只在开发库副本与测试内存库执行，platform.db 与 platform-dryrun.db 不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '005_tool_contract.sql');

-- user_version 双写（DEC-002 条件，与 001_init.sql 尾部同款）
PRAGMA user_version = 5;
