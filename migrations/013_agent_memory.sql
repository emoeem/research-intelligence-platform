-- ============================================================================
-- Research Intelligence Platform : Phase 7 Agent Memory 数据面增量迁移
--   （编排批次 P7-3 第 3/7 批，内容对应 ROADMAP §8.3 P7-4「三类 Memory」行
--   + SESSION-HANDOFF §十 Context Memory 雏形追加项；批次序号错位说明见
--   docs/releases/RELEASE_2026-11-12_PHASE7-P7-3.md）
-- ============================================================================
-- 编号依据（ROADMAP §1.4 迁移编号冻结规则）：按实际合入顺序递增，本文件
-- 实际取 013（实测 ls migrations/*.sql 最新为 012_knowledge_document_fetch.sql）。
--
-- 范围（三张新表，均为追加结构，不改既有表）：
--   1. agent_task_memory：Task Memory（run 内轮次工作记忆，易失面；
--      压缩留摘要、驱逐留墓碑行）。
--   2. agent_evidence_memory：Evidence Memory（证据/产物引用台账，防丢失
--      机制面；原始证据与产物正文永远只在 evidence/artifact 既有表，本表
--      只存指针与登记时刻快照）。
--   3. agent_research_memory：Research Memory（Context Memory 雏形：可追溯
--      研究成果，refs_json 引用 manifest item/artifact/evidence id 可解析
--      回链；独立表、独立生命周期，不复用任何会话/上下文存储）。
--
-- 反重复建模约束（ROADMAP §8.5 风险 4，评审记录
-- docs/phase7/AGENT-MEMORY-REVIEW.md）：
--   - 三表均不承载 evidence 实体字段（captured_at/captured_by/task_ref/
--     permission_level/sensitivity/governance/source_system 等一律不设列），
--     原始证据的唯一登记面仍是 evidence 表（001_init.sql:250）。
--   - agent_evidence_memory.content_hash 与 storage_ref 是登记时刻快照
--     （校验钉面），先例为 008 context_manifest_item.content_hash；解析
--     与哈希校验时以 evidence/artifact 既有表为权威，快照用于漂移检测。
--
-- 契约（计划 §9:478 原文「原始证据不能因上下文压缩而丢失」）：
--   - 压缩/驱逐只作用于 agent_task_memory（EVICTED）与 agent_research_memory
--     （COMPRESSED 置空详情），evidence/artifact 既有表零改动；
--   - agent_evidence_memory 无 EVICTED/ARCHIVED 词值（CHECK 兜底），证据
--     指针台账永不驱逐；
--   - 压缩/驱逐后 Evidence 与 Artifact 引用仍可解析（回链行存在）且
--     content_hash 校验通过（快照 = evidence.content_hash = artifact.sha256，
--     落盘产物逐字节复算）。
--
-- 红线（ROADMAP §13）：新迁移只在开发库副本与测试内存库执行，
-- migrations/platform.db 与 migrations/platform-dryrun.db 一律不写。
--
-- 幂等：CREATE TABLE IF NOT EXISTS + CREATE INDEX IF NOT EXISTS +
-- INSERT OR IGNORE + PRAGMA user_version = 13，可重复执行；
-- 由 crates/database 迁移执行器（migrate.rs）按 user_version 跳过。
-- ============================================================================

-- ---------------------------------------------------------------------------
-- agent_task_memory：Task Memory（run 内工作记忆，生命周期随压缩/驱逐推进）
--   content_json 形状：轮次工作记忆载荷 JSON（步骤、信封摘要、登记 id 等，
--   由写入面组装，域层不解析内部形状）。
--   压缩：content_json 置空 + summary 留确定性摘要（细节丢弃，计数留存）。
--   驱逐：content_json 与 summary 全置空，行留墓碑（EVICTED，审计可查）。
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS agent_task_memory (
  memory_id      TEXT PRIMARY KEY,            -- mem-t-<hex24>（引擎生成）
  run_id         TEXT NOT NULL
                   REFERENCES agent_run(run_id),   -- 记忆归属 run（RESTRICT：run 不删）
  task_id        TEXT,                        -- 关联研究任务（agent_run.task_id 透传，查询面）
  round          INTEGER NOT NULL DEFAULT 0,  -- 产生轮次（0 起）
  content_json   TEXT NOT NULL DEFAULT '{}',  -- 工作记忆载荷（压缩/驱逐后置空）
  summary        TEXT NOT NULL DEFAULT '',    -- 压缩摘要（COMPRESSED 留存面，EVICTED 置空）
  lifecycle      TEXT NOT NULL DEFAULT 'ACTIVE'
                   CHECK (lifecycle IN ('ACTIVE','COMPRESSED','EVICTED')),
  created_at     TEXT NOT NULL,               -- ISO8601 UTC
  updated_at     TEXT NOT NULL,
  schema_version INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS ix_agent_task_memory_run
  ON agent_task_memory(run_id);

-- ---------------------------------------------------------------------------
-- agent_evidence_memory：Evidence Memory（证据/产物引用台账，防丢失机制面）
--   反重复建模：本表不设 captured_at/captured_by/permission_level/sensitivity/
--   governance 等 evidence 实体字段；content_hash/storage_ref 为登记时刻
--   快照（快照必须等于既有表权威值，写入面强校验），权威面始终是
--   evidence/artifact 表。
--   UNIQUE(run_id, evidence_id)：同一证据在同一 run 只登记一条指针。
--   无 EVICTED/ARCHIVED：证据指针永不驱逐（契约面，domain 跃迁表同口径）。
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS agent_evidence_memory (
  memory_id      TEXT PRIMARY KEY,            -- mem-e-<hex24>（引擎生成）
  run_id         TEXT NOT NULL
                   REFERENCES agent_run(run_id),
  evidence_id    TEXT NOT NULL
                   REFERENCES evidence(evidence_id),  -- 指针：权威行在 001 evidence 表
  artifact_id    TEXT
                   REFERENCES artifact(artifact_id),  -- 同哈希对账面（005 artifact 表）
  content_hash   TEXT NOT NULL,               -- 登记时刻快照 sha256 hex（漂移检测钉面）
  storage_ref    TEXT NOT NULL,               -- 登记时刻快照解析入口（= evidence.storage_ref）
  lifecycle      TEXT NOT NULL DEFAULT 'ACTIVE'
                   CHECK (lifecycle IN ('ACTIVE','COMPRESSED')),
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL,
  schema_version INTEGER NOT NULL DEFAULT 1,
  UNIQUE (run_id, evidence_id)
);

CREATE INDEX IF NOT EXISTS ix_agent_evidence_memory_run
  ON agent_evidence_memory(run_id);
CREATE INDEX IF NOT EXISTS ix_agent_evidence_memory_evidence
  ON agent_evidence_memory(evidence_id);

-- ---------------------------------------------------------------------------
-- agent_research_memory：Research Memory（Context Memory 雏形，可追溯成果）
--   独立于聊天/会话记忆：独立表、独立生命周期，不复用 context_manifest 等会
--   话存储（SESSION-HANDOFF §十原句）。每条记忆至少一条可解析回链
--   （refs_json 非空由域层校验，表列不设文本 CHECK）。
--   refs_json 形状（MemoryObjectRef 序列化，回链三型）：
--     [{"ref_type":"manifest_item","manifest_id":"ctxm-..","item_id":"ki-.."},
--      {"ref_type":"artifact","ref_id":"art-.."},
--      {"ref_type":"evidence","ref_id":"ev-.."}]
--   压缩：content_json 置空，summary/title/refs_json 原样保留（成果可追溯面）。
--   归档：refs_json 原样保留；无 EVICTED 词值（成果永不驱逐）。
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS agent_research_memory (
  memory_id      TEXT PRIMARY KEY,            -- mem-r-<hex24>（引擎生成）
  run_id         TEXT NOT NULL
                   REFERENCES agent_run(run_id),
  task_id        TEXT,                        -- 关联研究任务
  agent_id       TEXT NOT NULL,               -- 产出者 agent id（agent_definition.agent_id 透传）
  title          TEXT NOT NULL,               -- 成果标题（非空语义由域层校验）
  summary        TEXT NOT NULL,               -- 成果摘要（压缩后留存面）
  content_json   TEXT NOT NULL DEFAULT '{}',  -- 详情载荷（压缩后置空）
  refs_json      TEXT NOT NULL DEFAULT '[]',  -- 回链数组（压缩/归档不动；域层强制非空）
  lifecycle      TEXT NOT NULL DEFAULT 'ACTIVE'
                   CHECK (lifecycle IN ('ACTIVE','COMPRESSED','ARCHIVED')),
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL,
  schema_version INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS ix_agent_research_memory_run
  ON agent_research_memory(run_id);
CREATE INDEX IF NOT EXISTS ix_agent_research_memory_task
  ON agent_research_memory(task_id);

-- ---------------------------------------------------------------------------
-- schema_version：Phase 7 Agent Memory 数据面变更登记（version=13；1..12 见各自登记行）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (13, 'V1 additive Phase 7 agent memory (agent_task_memory/agent_evidence_memory/agent_research_memory)',
   'Phase 7 P7-3（编排第 3/7 批，ROADMAP §8.3 P7-4 内容行）：三类 Memory 建表：agent_task_memory（run 内工作记忆，ACTIVE/COMPRESSED/EVICTED 压缩留摘要驱逐留墓碑）+ agent_evidence_memory（证据/产物引用台账，指针+登记时刻快照，UNIQUE(run_id,evidence_id)，无 EVICTED 词值，原始证据唯一登记面仍是 001 evidence 表）+ agent_research_memory（Context Memory 雏形：可追溯研究成果，refs_json 引用 manifest item/artifact/evidence id 回链，独立表独立生命周期不复用会话存储，无 EVICTED 词值）；反重复建模约束见 docs/phase7/AGENT-MEMORY-REVIEW.md；只在开发库副本与测试内存库执行，platform*.db 不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '013_agent_memory.sql');

-- user_version 双写（DEC-002 条件，与 001_init.sql 尾部同款）
PRAGMA user_version = 13;
