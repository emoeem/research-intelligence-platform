-- ============================================================================
-- Research Intelligence Platform : Phase 6 Knowledge 摄取数据面增量迁移
--   （P6-5 批次 = ROADMAP §7.3 P6-5 + PLUGIN-CONTRACT §2 原则 2（第一红线：
--    知识入主库只能经摄取管道并保留 plugin_id 溯源）+ §3.2 Knowledge Plugin）
-- ============================================================================
-- 编号依据（ROADMAP §1.4 迁移编号冻结规则）：迁移编号按合入顺序递增，先到
-- 先得。本文件合入 010（本批实测 ls migrations/*.sql 为 001 到 009，
-- 009_plugin.sql 为当前最大号）。
--
-- 范围（knowledge_ingest 单表，追加结构，不改既有表）：
--   knowledge_ingest：Knowledge 插件摄取落地表（P6-5 ingest.rs 唯一写者）。
--   每 = 一个语料条目（RTIS knowledge.db poc 行 / skills md / pocs 正文），
--   带 plugin_id + plugin_version + corpus_id 溯源三元组（§3.2：保留
--   plugin_id 溯源；卸载插件不删已入主库知识，P6-2 §5.4 已定）。
--   content_sha256 语义：正文按字节哈希（确定性幂等键的一半），重跑摄取对
--   已存在且内容不变的行 no-op（M6 ETL Replay 口径）。
--   source_pk 保留源侧主键（RTIS poc.id），对账与孤儿来源清单（§5.3）用。
--
-- 红线（ROADMAP §13）：新迁移只在开发库副本与测试内存库执行，
-- migrations/platform.db 与 migrations/platform-dryrun.db 一律不写。
--
-- 幂等：CREATE TABLE IF NOT EXISTS + CREATE INDEX IF NOT EXISTS +
-- INSERT OR IGNORE + PRAGMA user_version = 10，可重复执行；
-- 由 crates/database 迁移执行器（migrate.rs）按 user_version 跳过。
-- ============================================================================

CREATE TABLE IF NOT EXISTS knowledge_ingest (
  ingest_id      INTEGER PRIMARY KEY AUTOINCREMENT,
  plugin_id      TEXT NOT NULL,              -- 溯源：来源插件（domain::plugin::PluginId）
  plugin_version TEXT NOT NULL,              -- 溯源：来源插件版本
  corpus_id      TEXT NOT NULL,              -- 语料标识（manifest capabilities[].provides.corpus_id）
  content_kind   TEXT NOT NULL
                   CHECK (content_kind IN ('poc','skill_md','method_case')),
                   -- v1.json capability content_kinds 三类词表（§10.1）
  title          TEXT NOT NULL,              -- 条目标题
  source_path    TEXT NOT NULL,              -- 包内相对路径（knowledge.db:/poc 或文件路径）
  source_pk      TEXT,                       -- 源侧主键（RTIS poc.id；md/文件无主键为 NULL）
  body           TEXT NOT NULL,              -- 正文（poc content / md 全文）
  body_bytes     INTEGER NOT NULL,           -- 正文字节数（规模对账）
  content_sha256 TEXT NOT NULL,              -- 正文 sha256（幂等键一半）
  meta_json      TEXT,                       -- 源侧元数据 JSON（kind/category/cve/component/tags/verified 等）
  ingested_at    TEXT NOT NULL,              -- ISO8601 UTC
  UNIQUE (plugin_id, plugin_version, content_kind, content_sha256)
);

CREATE INDEX IF NOT EXISTS ix_knowledge_ingest_plugin
  ON knowledge_ingest(plugin_id, plugin_version);
CREATE INDEX IF NOT EXISTS ix_knowledge_ingest_kind
  ON knowledge_ingest(content_kind);

-- ---------------------------------------------------------------------------
-- schema_version：Phase 6 Knowledge 摄取数据面变更登记（version=10；1..9 见各自登记行）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (10, 'V1 additive Phase 6 knowledge ingest (knowledge_ingest)',
   'Phase 6 P6-5（ROADMAP §7.3）：knowledge_ingest 摄取落地表建表（PLUGIN-CONTRACT §2 原则 2 第一红线：知识入主库只能经摄取管道并保留 plugin_id 溯源）；溯源三元组 plugin_id/plugin_version/corpus_id + content_kind 三类词表（§10.1 content_kinds）+ UNIQUE(plugin_id,plugin_version,content_kind,content_sha256) 幂等键（M6 ETL Replay 口径：内容不变行 no-op）+ source_pk 源侧主键对账；crates/plugin-runtime ingest.rs 为唯一写者；编号按 ROADMAP §1.4 先到先得规则取 010；只在开发库副本与测试内存库执行，platform.db 与 platform-dryrun.db 不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '010_knowledge_ingest.sql');

-- user_version 双写（DEC-002 条件，与 001_init.sql 尾部同款）
PRAGMA user_version = 10;
