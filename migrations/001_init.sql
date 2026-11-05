-- ============================================================================
-- Research Intelligence Platform — Schema V1（M1 第一批，隔离环境）
-- ============================================================================
-- 范围裁定：M1_IMPLEMENTATION_PLAN.md 未裁定“8 个先落实体”，按任务口径回落为
--   6 个有现网数据/现网定义的核心实体：
--     Resource, Knowledge, Evidence, Tool, Skill, Review
--   外加 3 张基础设施表：entity_alias, audit_event, schema_version
--   另含 2 张从属于父实体域的子表（不单列为实体）：
--     resource_source（Resource 1:N 来源，落实“resource_sources → 来源”映射）
--     tool_version   （Tool 1:N 版本，落实评审修正 K-10：Tool↔ToolVersion 为 1:N）
--
-- 必须吸收的 M1 就绪评审修正（architecture-2.0/m1/M1_READINESS_REVIEW.md §2）：
--   K-07  Review 对 Resource 为 1:N（实测 review_log 165 行 > resources 81 条）
--   K-08  Knowledge 生命周期与治理状态双轴分离（lifecycle 列 + governance 列，
--         QUARANTINED/REJECTED/CONFLICTED 不再嵌入生命周期链）
--   K-09  别名机制四要素：source_system / namespace / valid_until(含 valid_from) /
--         冲突唯一约束 UNIQUE(source_system, namespace, alias_value, entity_type)
--   K-10  Tool↔ToolVersion 1:N（tool_version 子表 + UNIQUE(tool_id, version)）
--   K-16  Resource 生命周期补验证维度（FETCHED / VERIFIED 进入生命周期链）
--   K-17  反知识作为可查询治理对象：knowledge.record_kind='negative' + 专属列
--         （独立表建议留待 M1 第二批差异决策，本批以列承载、可查询）
--
-- 约定：
--   * 每表含 created_at / updated_at / schema_version（任务硬约束）。
--   * 干跑阶段不批量生成新平台 ID（计划 §5.4）：主键沿用现网稳定 ID
--     （gh:/url:、poc.code、'eri-catalog:review_log:<id>' 派生键），
--     并在 entity_alias 登记来源四要素。
--   * lifecycle / governance 双轴：lifecycle 为处理阶段链，governance 为治理状态
--     （ACTIVE/QUARANTINED/REJECTED/ARCHIVED），两轴正交（K-08）。
--   * 本文件幂等（IF NOT EXISTS + INSERT OR IGNORE），可重复执行。
-- ============================================================================

PRAGMA foreign_keys = ON;

-- ---------------------------------------------------------------------------
-- 基础设施 1/3：schema 版本登记（DEC-002 条件：schema_version 表 + user_version 双写）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS schema_version (
  version     INTEGER PRIMARY KEY,
  name        TEXT NOT NULL,
  description TEXT,
  applied_at  TEXT NOT NULL,
  applied_by  TEXT
);

INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (1, 'V1 init',
   'M1 第一批：6 核心实体（Resource/Knowledge/Evidence/Tool/Skill/Review）+ resource_source/tool_version 子表 + entity_alias/audit_event 基础设施；吸收评审修正 K-07/K-08/K-09/K-10/K-16/K-17',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '001_init.sql');

-- ---------------------------------------------------------------------------
-- 基础设施 2/3：实体别名（历史 ID / ExternalIdentifier，K-09 四要素）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS entity_alias (
  id            TEXT PRIMARY KEY,          -- 确定性键：md5(entity_type|source_system|namespace|alias_value)
  entity_type   TEXT NOT NULL CHECK (entity_type IN
                  ('resource','resource_source','knowledge','evidence',
                   'tool','tool_version','skill','review')),
  entity_id     TEXT NOT NULL,
  source_system TEXT NOT NULL,             -- 要素 1：来源系统（eri-catalog / rtis-knowledge / ...）
  namespace     TEXT NOT NULL,             -- 要素 2：来源命名空间（resource_id / poc.code / review_log.id / ...）
  alias_value   TEXT NOT NULL,             -- 来源系统内的原 ID 值
  valid_from    TEXT,                      -- 要素 3a：生效时间
  valid_until   TEXT,                      -- 要素 3b：失效时间（NULL = 长期有效）
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL,
  schema_version INTEGER NOT NULL DEFAULT 1,
  -- 要素 4：冲突唯一约束——同一来源系统+命名空间下的一个别名值
  -- 在同一实体类型内只能映射到一个实体，重复登记即冲突被拒绝。
  UNIQUE (source_system, namespace, alias_value, entity_type)
);

CREATE INDEX IF NOT EXISTS ix_alias_entity
  ON entity_alias(entity_type, entity_id);

-- ---------------------------------------------------------------------------
-- 基础设施 3/3：审计事件（只追加；M1 第一批不迁移历史事件流，表先就位）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS audit_event (
  event_id      INTEGER PRIMARY KEY AUTOINCREMENT,
  ts            TEXT NOT NULL,
  actor         TEXT NOT NULL,
  action        TEXT NOT NULL,
  entity_type   TEXT,
  entity_id     TEXT,
  correlation_id TEXT,
  detail        TEXT,
  permission_level TEXT,
  source_system TEXT,
  source_pk     TEXT,
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL,
  schema_version INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS ix_audit_entity ON audit_event(entity_type, entity_id);
CREATE INDEX IF NOT EXISTS ix_audit_ts     ON audit_event(ts);

-- ---------------------------------------------------------------------------
-- 实体 1：Resource（源：ERI catalog.db resources，81 条）
--   K-16：生命周期链含验证维度 FETCHED / VERIFIED；
--   K-08：治理轴独立 governance 列。
--   quality_scores（1:1）按任务裁定折入质量字段列。
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS resource (
  resource_id     TEXT PRIMARY KEY,        -- 现网稳定 ID：gh:owner/repo | url:hash16
  identity        TEXT UNIQUE,
  canonical_url   TEXT NOT NULL,
  resource_type   TEXT NOT NULL,
  name            TEXT NOT NULL,
  description     TEXT,
  owner           TEXT,
  repo_name       TEXT,
  lifecycle       TEXT NOT NULL DEFAULT 'DISCOVERED'
                  CHECK (lifecycle IN
                    ('DISCOVERED','FETCHED','PARSED','CANDIDATE','VERIFIED','INDEXED','STALE')),
  governance      TEXT NOT NULL DEFAULT 'ACTIVE'
                  CHECK (governance IN ('ACTIVE','QUARANTINED','REJECTED','ARCHIVED')),
  source_status   TEXT,                    -- 源 resources.status 原值（保真线索）
  quality_score   REAL,                    -- 源 resources.quality_score（P1 引擎）
  trust_level     TEXT,                    -- 源 resources.trust_level（A/B/C/D）
  -- 折入字段：catalog.quality_scores（1:1，任务裁定“quality_scores → 质量字段”）
  quality_relevance        REAL,
  quality_reliability      REAL,
  quality_maintenance      REAL,
  quality_maturity         REAL,
  quality_complementarity  REAL,
  quality_license_clarity  REAL,
  quality_docs_testing     REAL,
  quality_final_score      REAL,
  quality_grade            TEXT,
  quality_adoption_blocked INTEGER,
  quality_reasons          TEXT,
  quality_computed_at      TEXT,
  quality_engine_version   TEXT,
  first_seen        TEXT,
  last_seen         TEXT,
  last_checked      TEXT,
  last_source_commit TEXT,
  source_system   TEXT NOT NULL,
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL,
  schema_version  INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS ix_resource_type ON resource(resource_type);
CREATE INDEX IF NOT EXISTS ix_resource_lifecycle ON resource(lifecycle);
CREATE INDEX IF NOT EXISTS ix_resource_governance ON resource(governance);

-- 子表：Resource 1:N 来源（源：catalog.resource_sources；“resource_sources → 来源”）
CREATE TABLE IF NOT EXISTS resource_source (
  id               INTEGER PRIMARY KEY,    -- 源 resource_sources.id 原值（单源系统内稳定）
  resource_id      TEXT NOT NULL REFERENCES resource(resource_id),
  source_id        TEXT NOT NULL,
  source_repository TEXT NOT NULL,
  source_file      TEXT NOT NULL,
  source_section   TEXT NOT NULL,
  source_position  INTEGER,
  source_commit    TEXT NOT NULL,
  source_url       TEXT NOT NULL,
  first_seen       TEXT NOT NULL,
  last_seen        TEXT NOT NULL,
  source_system    TEXT NOT NULL,
  created_at       TEXT NOT NULL,          -- 源无 created_at：取 first_seen（确定性派生，见报告）
  updated_at       TEXT NOT NULL,          -- 源无 updated_at：取 last_seen
  schema_version   INTEGER NOT NULL DEFAULT 1,
  UNIQUE (resource_id, source_section, source_url)
);

CREATE INDEX IF NOT EXISTS ix_rsource_rid ON resource_source(resource_id);

-- ---------------------------------------------------------------------------
-- 实体 2：Knowledge（源：knowledge.db poc × k_meta 28 条自有 + k_negative 17 条反知识）
--   K-08：lifecycle（处理阶段链）与 governance（治理状态）双轴分离；
--   K-17：反知识以 record_kind='negative' + 专属列承载，可查询。
--   生命周期映射（保真 + 纯化，见报告）：k_meta.lifecycle 原值保留于 source_lifecycle，
--     Field-validated / Lab-validated → VERIFIED；Researched → CANDIDATE。
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS knowledge (
  knowledge_id    TEXT PRIMARY KEY,        -- poc.code（own）；'neg:'||父 code（negative）
  record_kind     TEXT NOT NULL CHECK (record_kind IN ('own','negative')),
  lifecycle       TEXT NOT NULL
                  CHECK (lifecycle IN
                    ('DISCOVERED','FETCHED','PARSED','CANDIDATE','VERIFIED','INDEXED','STALE')),
  governance      TEXT NOT NULL DEFAULT 'ACTIVE'
                  CHECK (governance IN ('ACTIVE','QUARANTINED','REJECTED','ARCHIVED')),
  title           TEXT NOT NULL,
  content_kind    TEXT,                    -- 源 poc.kind：poc|exp|script|template|payload
  category        TEXT,
  cve             TEXT,
  component       TEXT,
  versions        TEXT,
  severity        TEXT,
  language        TEXT,
  source          TEXT,
  source_url      TEXT,
  description     TEXT,
  usage           TEXT,
  content         TEXT,
  path            TEXT,
  verified        INTEGER CHECK (verified IN (0,1) OR verified IS NULL),
  verified_note   TEXT,
  hit_count       INTEGER,
  used_on         TEXT,
  engagement_id   TEXT,
  engagement_name TEXT,
  asset_target    TEXT,
  found_by_agent  TEXT,
  tags            TEXT,
  -- k_meta 分层字段（仅 record_kind='own'）
  original_category   TEXT,
  normalized_type     TEXT,
  archetypes          TEXT,
  technology_tags     TEXT,
  method_tags         TEXT,
  evidence_level      TEXT CHECK (evidence_level IN ('K0','K1','K2','K3','K4')
                                  OR evidence_level IS NULL),
  verification_basis  TEXT,
  dedup_key           TEXT,
  meta_version        INTEGER,
  last_reviewed       TEXT,
  meta_updated_at     TEXT,                -- 源 k_meta.updated_at
  source_lifecycle    TEXT,                -- 源 k_meta.lifecycle 原值（保真线索）
  -- 反知识专属字段（仅 record_kind='negative'，源 k_negative）
  neg_do_not_conclude       TEXT CHECK (record_kind <> 'negative'
                                 OR neg_do_not_conclude IS NOT NULL),
  neg_false_positive_cases  TEXT,
  neg_known_failures        TEXT,
  neg_stop_conditions       TEXT,
  neg_not_applicable_when   TEXT,
  neg_source_knowledge_id   TEXT,          -- 源 k_negative.knowledge_id（poc.id，保真线索）
  negates_knowledge_id      TEXT REFERENCES knowledge(knowledge_id),
                                 -- 目标库内可解析的父知识；父未迁移时为 NULL（1/17，见报告）
  source_system   TEXT NOT NULL,
  created_by      TEXT,
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL,
  schema_version  INTEGER NOT NULL DEFAULT 1,
  CHECK (record_kind <> 'negative' OR (neg_source_knowledge_id IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS ix_knowledge_kind      ON knowledge(record_kind);
CREATE INDEX IF NOT EXISTS ix_knowledge_lifecycle ON knowledge(lifecycle);
CREATE INDEX IF NOT EXISTS ix_knowledge_component ON knowledge(component);
CREATE INDEX IF NOT EXISTS ix_knowledge_cve       ON knowledge(cve);

-- ---------------------------------------------------------------------------
-- 实体 3：Evidence（M1 第一批不迁移数据：28 个 assets.db 留第二批做 Evidence 映射）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS evidence (
  evidence_id      TEXT PRIMARY KEY,
  evidence_type    TEXT NOT NULL CHECK (evidence_type IN
                     ('http_pair','screenshot','file','log','hash')),
  storage_ref      TEXT,
  content_hash     TEXT,
  captured_at      TEXT,
  captured_by      TEXT,
  task_ref         TEXT,
  permission_level TEXT,
  sensitivity      TEXT,
  lifecycle        TEXT CHECK (lifecycle IN ('RAW','VERIFIED','REFERENCED','ARCHIVED')
                               OR lifecycle IS NULL),
  governance       TEXT NOT NULL DEFAULT 'ACTIVE'
                    CHECK (governance IN ('ACTIVE','QUARANTINED','REJECTED','ARCHIVED')),
  source_system    TEXT,
  source_pk        TEXT,
  created_at       TEXT NOT NULL,
  updated_at       TEXT NOT NULL,
  schema_version   INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS ix_evidence_task ON evidence(task_ref);

-- ---------------------------------------------------------------------------
-- 实体 4：Tool（M1 第一批不迁移数据：tools.json → Tool 属 M6，表先就位）
--   R-08 教训：定义 / 安装状态 / 健康状态分离。
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS tool (
  tool_id          TEXT PRIMARY KEY,       -- tool:<name>
  name             TEXT NOT NULL UNIQUE,
  kind             TEXT,
  lifecycle        TEXT CHECK (lifecycle IN
                     ('REGISTERED','VALIDATED','ENABLED','DISABLED','DEPRECATED')
                     OR lifecycle IS NULL),
  governance       TEXT NOT NULL DEFAULT 'ACTIVE'
                    CHECK (governance IN ('ACTIVE','QUARANTINED','REJECTED','ARCHIVED')),
  install_state    TEXT,
  health_state     TEXT,
  permission_level TEXT CHECK (permission_level IN ('T0','T1','T2')
                               OR permission_level IS NULL),
  input_schema     TEXT,
  output_schema    TEXT,
  timeout_s        INTEGER,
  binary_ref       TEXT,
  source_system    TEXT,
  source_pk        TEXT,
  created_at       TEXT NOT NULL,
  updated_at       TEXT NOT NULL,
  schema_version   INTEGER NOT NULL DEFAULT 1
);

-- 子表：Tool 1:N 版本（K-10 评审修正：Tool↔ToolVersion 为 1:N，不是 1:1）
CREATE TABLE IF NOT EXISTS tool_version (
  tv_id          INTEGER PRIMARY KEY AUTOINCREMENT,
  tool_id        TEXT NOT NULL REFERENCES tool(tool_id),
  version        TEXT NOT NULL,
  provenance_json TEXT,
  content_hash   TEXT,
  validated_at   TEXT,
  status         TEXT CHECK (status IN ('current','superseded') OR status IS NULL),
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL,
  schema_version INTEGER NOT NULL DEFAULT 1,
  UNIQUE (tool_id, version)
);

CREATE INDEX IF NOT EXISTS ix_tool_version_tool ON tool_version(tool_id);

-- ---------------------------------------------------------------------------
-- 实体 5：Skill（M1 第一批不迁移数据：125 个 Skill 文件属后续批次，表先就位）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS skill (
  skill_id        TEXT PRIMARY KEY,        -- skill:<name>
  name            TEXT NOT NULL UNIQUE,
  version         TEXT,
  roles           TEXT,                    -- 逗号分隔角色（源为单维 role，M6 规范化）
  description     TEXT,
  when_to_use     TEXT,
  required_tools  TEXT,
  knowledge_refs  TEXT,
  body_ref        TEXT,
  enabled         INTEGER CHECK (enabled IN (0,1) OR enabled IS NULL),
  generator       TEXT,
  lifecycle       TEXT CHECK (lifecycle IN ('DRAFT','ACTIVE','DEPRECATED','SPLIT')
                               OR lifecycle IS NULL),
  governance      TEXT NOT NULL DEFAULT 'ACTIVE'
                   CHECK (governance IN ('ACTIVE','QUARANTINED','REJECTED','ARCHIVED')),
  source_system   TEXT,
  source_pk       TEXT,
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL,
  schema_version  INTEGER NOT NULL DEFAULT 1
);

-- ---------------------------------------------------------------------------
-- 实体 6：Review（源：catalog.review_log，165 行）
--   K-07 评审修正：Review 对 Resource（及任意目标）为 1:N，多态引用
--   (target_type, target_id)；target_type 取源 queue 原值（d5|license|mapping）。
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS review (
  review_id      TEXT PRIMARY KEY,         -- 'eri-catalog:review_log:<id>'（确定性派生，不生成新平台 ID）
  target_type    TEXT NOT NULL,
  target_id      TEXT NOT NULL,            -- 源 item_key 原值
  decision       TEXT NOT NULL,
  confidence     REAL,
  reason         TEXT,
  reviewer       TEXT,
  reviewed_at    TEXT,
  source_system  TEXT NOT NULL,
  source_pk      TEXT NOT NULL,
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL,
  schema_version INTEGER NOT NULL DEFAULT 1,
  UNIQUE (source_system, source_pk)
);

CREATE INDEX IF NOT EXISTS ix_review_target ON review(target_type, target_id);

-- ---------------------------------------------------------------------------
-- user_version 双写（DEC-002 条件；R-01 教训：源库 user_version 全 0）
-- ---------------------------------------------------------------------------
PRAGMA user_version = 1;
