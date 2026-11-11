-- ============================================================================
-- Research Intelligence Platform : Phase 6 SK-1 通用知识来源数据面增量迁移
--   （P6-9 第 1/3/7 步 = ROADMAP §7.3 P6-9 + 安全知识源计划 §4/§8 P6-9；
--    ADR：docs/phase6/SK1-INGEST-ADR.md）
-- ============================================================================
-- 编号依据（ROADMAP §1.4 迁移编号冻结规则）：迁移编号按合入顺序递增，先到
-- 先得。本文件合入 011（合入前实测 ls migrations/*.sql 为 001 到 010，
-- 010_knowledge_ingest.sql 为当前最大号）。
--
-- 范围（两张新表，纯追加结构，不改既有表、不改 010 的 knowledge_ingest）：
--
--   knowledge_source_entry：通用知识来源条目表（SK-1 三类内容
--     catalog_entry / document_markdown / skill_candidate 的唯一权威落点；
--     RTIS 三类继续落 knowledge_ingest，本表 CHECK 词表虽收全六值，但 SK-1
--     的 RTIS 写面固定不接本表，Phase 7 正文模型收敛时再评估搬迁）。
--     去重语义（ADR 第四节）：UNIQUE(plugin_id, plugin_version, entry_kind,
--     item_key)；catalog_entry 的 item_key = 规范化 canonical URL，
--     document_markdown / skill_candidate 的 item_key = 包内相对路径；
--     content_sha256 独立列做内容变更侦测（键同哈希变 = skipped + changed
--     计数，不覆盖历史行，保住 Evidence 可复核性）。
--     审核闸（ADR 第七节）：review_status 初始 DISCOVERED；Search 第三源与
--     Context PluginSource 只收录 APPROVED；上游删除转 DEPRECATED 不物理删。
--
--   context_citation_evidence：引用证据固化表（P6-9 第 7 步：把实际给模型
--     的引用片段、内容哈希与来源位置固化，压缩/重试后凭哈希复核）。行只
--     追加；插件禁用/卸载不删行（历史 Evidence 可读验收项）。evidence 主体
--     表（001，evidence_type 五值 CHECK）不动：引用证据的 excerpt/溯源列族
--     与该 CHECK 词表不相容，且改 CHECK 需重建表，与本批红线冲突。
--
-- 红线（ROADMAP §13）：新迁移只在开发库副本与测试内存库执行，
-- migrations/platform.db 与 migrations/platform-dryrun.db 一律不写。
--
-- 幂等：CREATE TABLE IF NOT EXISTS + CREATE INDEX IF NOT EXISTS +
-- INSERT OR IGNORE + PRAGMA user_version = 11，可重复执行；由
-- crates/database 迁移执行器（migrate.rs）按 user_version 跳过。
-- ============================================================================

CREATE TABLE IF NOT EXISTS knowledge_source_entry (
  entry_id        INTEGER PRIMARY KEY AUTOINCREMENT,
  plugin_id       TEXT NOT NULL,               -- 溯源：来源插件（domain::plugin::PluginId）
  plugin_version  TEXT NOT NULL,               -- 溯源：来源插件版本
  corpus_id       TEXT NOT NULL,               -- 语料标识（manifest provides.corpus_id）
  entry_kind      TEXT NOT NULL
                    CHECK (entry_kind IN ('poc','skill_md','method_case',
                                          'catalog_entry','document_markdown','skill_candidate')),
                    -- 六值并集词表（010 三类 + SK-1 三类，domain::knowledge::EntryKind；
                    -- RTIS 三类权威落点仍是 knowledge_ingest，本表不接其写面）
  item_key        TEXT NOT NULL,               -- 稳定条目键（catalog=规范化 URL；其余=包内相对路径）
  title           TEXT NOT NULL,               -- 条目标题
  description     TEXT,                        -- 描述/摘要（metadata_only 可空）
  body            TEXT NOT NULL DEFAULT '',    -- 正文（catalog=条目行文本；document=去 frontmatter 全文）
  body_bytes      INTEGER NOT NULL DEFAULT 0,  -- 正文字节数
  content_sha256  TEXT NOT NULL,               -- 正文 sha256（变更侦测与 Evidence 复核键）
  metadata_sha256 TEXT NOT NULL,               -- 源位置/分类等元数据 sha256
  category_path_json TEXT NOT NULL DEFAULT '[]', -- 章节标题路径（自顶向下 JSON 数组）
  canonical_url   TEXT,                        -- 规范化 URL（catalog_entry 必有）
  source_type     TEXT NOT NULL DEFAULT '',    -- 来源类型（plugin source.type 原值）
  source_url      TEXT NOT NULL DEFAULT '',    -- 来源主页/仓库 URL（Source Snapshot）
  source_path     TEXT NOT NULL DEFAULT '',    -- 来源内解析入口（provides.kinds 声明值）
  source_commit   TEXT,                        -- 快照基准 Commit（Git 来源）
  license_status  TEXT NOT NULL DEFAULT 'UNKNOWN', -- 来源许可（SPDX/UNLICENSED 原值）
  rights_policy   TEXT NOT NULL DEFAULT 'catalog_only'
                    CHECK (rights_policy IN ('catalog_only','approved_only','metadata_only')),
                    -- SK-1 许可与持久化策略词表（ADR 第五节）
  review_status   TEXT NOT NULL DEFAULT 'DISCOVERED'
                    CHECK (review_status IN ('DISCOVERED','UNREVIEWED','APPROVED','REJECTED','DEPRECATED')),
                    -- 审核闸（计划 §7 第 4 条：初始 DISCOVERED，APPROVED 才进检索/上下文面）
  risk_flags_json TEXT NOT NULL DEFAULT '[]',  -- 审查风险标记（JSON 数组，审查流程回填）
  source_location_json TEXT NOT NULL DEFAULT '{}', -- 源位置（file/line/anchor JSON）
  meta_json       TEXT,                        -- 其余源侧元数据 JSON（raw_link_text 等）
  filtered_note   TEXT,                        -- 解析期过滤注记（scheme 拒绝计数等，可复核）
  first_seen_at   TEXT NOT NULL,               -- ISO8601 UTC（首次摄取）
  last_seen_at    TEXT NOT NULL,               -- ISO8601 UTC（最近摄取）
  upstream_missing INTEGER NOT NULL DEFAULT 0, -- 上游删除标记（1=来源树已无此条目）
  UNIQUE (plugin_id, plugin_version, entry_kind, item_key)
);

CREATE INDEX IF NOT EXISTS ix_kse_plugin
  ON knowledge_source_entry(plugin_id, plugin_version);
CREATE INDEX IF NOT EXISTS ix_kse_kind
  ON knowledge_source_entry(entry_kind);
CREATE INDEX IF NOT EXISTS ix_kse_review
  ON knowledge_source_entry(review_status);
CREATE INDEX IF NOT EXISTS ix_kse_url
  ON knowledge_source_entry(canonical_url);

CREATE TABLE IF NOT EXISTS context_citation_evidence (
  evidence_id     TEXT PRIMARY KEY,            -- ciev-<sha256 前 24 位>（内容寻址，重试幂等）
  manifest_id     TEXT NOT NULL,               -- 装配清单（context_manifest.manifest_id）
  item_id         TEXT NOT NULL,               -- 装配条目（context_manifest_item.item_id）
  event_kind      TEXT NOT NULL DEFAULT 'assembled'
                    CHECK (event_kind IN ('assembled','expanded')),
                    -- assembled = L2 摘要装入；expanded = L3 按需展开
  ref_source      TEXT NOT NULL,               -- 引用来源（008 source 词表：plugin:<id> 等）
  ref_id          TEXT NOT NULL,               -- 引用对象（kse 词形 <entry_kind>:<item_key>）
  ref_version     TEXT,                        -- 引用版本（插件版本）
  excerpt         TEXT NOT NULL DEFAULT '',    -- 实际给模型的引用片段（截断保存）
  excerpt_sha256  TEXT NOT NULL,               -- 片段 sha256（与 excerpt 互核）
  content_sha256  TEXT,                        -- 源侧完整内容 sha256（与来源行互核）
  source_id       TEXT NOT NULL DEFAULT '',    -- Source ID（= plugin_id）
  plugin_version  TEXT,                        -- Plugin Version（溯源三元组之一）
  source_commit   TEXT,                        -- 来源快照 Commit
  canonical_url   TEXT,                        -- 原始 URL
  section_anchor  TEXT,                        -- 章节锚点（GitHub slug）
  source_location_json TEXT NOT NULL DEFAULT '{}', -- 源位置 JSON（file/line）
  created_at      TEXT NOT NULL,               -- ISO8601 UTC（只追加，不更新）
  schema_version  INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS ix_ciev_manifest
  ON context_citation_evidence(manifest_id);
CREATE INDEX IF NOT EXISTS ix_ciev_ref
  ON context_citation_evidence(ref_source, ref_id);

-- ---------------------------------------------------------------------------
-- schema_version：Phase 6 SK-1 通用知识来源数据面变更登记（version=11；1..10 见各自登记行）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (11, 'V1 additive SK-1 knowledge sources (knowledge_source_entry + context_citation_evidence)',
   'Phase 6 P6-9 SK-1（安全知识源计划 §8 P6-9 七步，ADR docs/phase6/SK1-INGEST-ADR.md）：knowledge_source_entry 通用知识来源条目表（SK-1 三类 catalog_entry/document_markdown/skill_candidate 唯一权威落点；UNIQUE(plugin_id,plugin_version,entry_kind,item_key) 去重 + content_sha256 变更侦测 + review_status 审核闸初始 DISCOVERED）与 context_citation_evidence 引用证据固化表（实际给模型的引用片段 + excerpt/content sha256 + Source ID/Plugin/Commit/URL/锚点，压缩重试后凭哈希复核，只追加）；RTIS 三类与 knowledge_ingest（010）零改动，15,155 对账基线不动；编号按 ROADMAP §1.4 先到先得规则取 011；只在开发库副本与测试内存库执行，platform.db 与 platform-dryrun.db 不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '011_knowledge_sources.sql');

-- user_version 双写（DEC-002 条件，与 001_init.sql 尾部同款）
PRAGMA user_version = 11;
