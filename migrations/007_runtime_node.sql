-- ============================================================================
-- Research Intelligence Platform : Phase 2 RuntimeNode 数据面增量迁移
--   （P2-4a 批次，ROADMAP §3.3 P2-4「RuntimeNode.health 落库」的内部拆分
--    + 计划 §12:696 实体行；设计见 docs/phase2/PHASE2-DESIGN.md §5.1.1）
-- ============================================================================
-- 编号依据（ROADMAP §1.4 迁移编号冻结规则）：迁移编号按合入顺序递增，先到先得。
-- 本文件合入 007（本批实测 ls migrations/*.sql 为 001 到 006，005/006 已占用）。
--
-- 范围（RuntimeNode 节点表一张，追加结构，不改既有表）：
--   runtime_node：RuntimeNode（id、kind、endpoint、health、version；计划
--   §12:696）。节点级健康由 Phase 2 Tool Gateway 心跳维护（TOOL-CONTRACT
--   §7.1:334，计划 §5.6:335），表只承载状态身份与心跳时间戳，WAITING_FOR_RUNNER
--   判定逻辑属 runtime-gateway（P2-4b）。
--   health 词表沿用 tool_version.health 的 CHECK 域 unknown|ok|degraded|broken
--   （D-TC-07 不新增状态，003 重建节 b 同款域）。
--   kind 词表 = 计划 §6 Runtime 类型表本阶段落面三值
--   python_worker|podman_host|podman_vm：podman_host 为 ADR §3 决定第 1 条
--   过渡 Runner；podman_vm 仅保留目标拓扑词表身份，VM 执行面挂起
--   （docs/phase2/PHASE2-DESIGN.md §3.1）；remote_runner 按 D-TC-01 不入词表。
--   词表扩展沿用 003 的 CHECK 域重建模式（SQLite 不支持 ADD CONSTRAINT）。
--
-- 红线（ROADMAP §13）：新迁移只在开发库副本与测试内存库执行，
-- migrations/platform.db 与 migrations/platform-dryrun.db 一律不写。
--
-- 幂等：CREATE TABLE IF NOT EXISTS + CREATE INDEX IF NOT EXISTS +
-- INSERT OR IGNORE + PRAGMA user_version = 7，可重复执行；
-- 由 crates/database 迁移执行器（migrate.rs:67）按 user_version 跳过。
-- ============================================================================

-- ---------------------------------------------------------------------------
-- runtime_node：RuntimeNode 节点表（计划 §12:696 五字段 + 心跳时间戳）
--   last_heartbeat_at 承载 Gateway 心跳落点（ROADMAP P2-4「心跳探测」的
--   存储面）；NULLable = 节点注册后尚未收到首次心跳（health 恒 unknown）。
--   node_id 长度口径（非空且不超过 128 字符）属 domain::node 校验面与
--   Gateway 写入口径，表列不设 CHECK 长度（与 tool_run.run_id 同款处理）。
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS runtime_node (
  node_id           TEXT PRIMARY KEY,             -- Gateway 登记生成或 Runner 自报稳定 id
  kind              TEXT NOT NULL
                      CHECK (kind IN ('python_worker','podman_host','podman_vm')),
  endpoint          TEXT,                         -- 可达地址（本地 runner 可空；VM 节点为 Gateway 通道地址）
  health            TEXT NOT NULL DEFAULT 'unknown'
                      CHECK (health IN ('unknown','ok','degraded','broken')),
  version           TEXT,                         -- Runner 版本指纹（如 podman version 输出；未取得对应 degraded）
  last_heartbeat_at TEXT,                         -- 最近一次心跳时刻（ISO8601 UTC；未收到心跳为 NULL）
  created_at        TEXT NOT NULL,                -- 登记时刻（ISO8601 UTC）
  updated_at        TEXT NOT NULL,                -- 最后一次状态变更时刻（ISO8601 UTC）
  schema_version    INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS ix_runtime_node_kind ON runtime_node(kind);

-- ---------------------------------------------------------------------------
-- schema_version：Phase 2 RuntimeNode 数据面变更登记（version=7；1..6 见各自登记行）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (7, 'V1 additive Phase 2 runtime node (runtime_node)',
   'Phase 2 P2-4a：runtime_node（计划 §12:696 id/kind/endpoint/health/version + last_heartbeat_at 心跳时间戳）建表；health 词表沿用 D-TC-07 四值（003 同款域），kind 词表取计划 §6 Runtime 类型表本阶段落面三值（remote_runner 按 D-TC-01 不入词表，podman_vm 为挂起拓扑的词表身份）；编号按 ROADMAP §1.4 先到先得规则取 007；只在开发库副本与测试内存库执行，platform.db 与 platform-dryrun.db 不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '007_runtime_node.sql');

-- user_version 双写（DEC-002 条件，与 001_init.sql 尾部同款）
PRAGMA user_version = 7;
