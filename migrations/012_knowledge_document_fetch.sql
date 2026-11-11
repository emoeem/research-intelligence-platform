-- ============================================================================
-- Research Intelligence Platform : Phase 6 SK-3 文档抓取审计数据面增量迁移
--   （P6-11 第 2 步 = ROADMAP §7.3 P6-11 + 安全知识源计划 §6.2C；
--    实现面 crates/tool-engine/src/document_fetch.rs）
-- ============================================================================
-- 编号依据（ROADMAP §1.4 迁移编号冻结规则）：迁移编号按合入顺序递增，先到
-- 先得。本文件合入 012（合入前实测 ls migrations/*.sql 最大号为
-- 011_knowledge_sources.sql）。
--
-- 范围（一张新表，纯追加结构，不改既有表、不改 011 两表）：
--
--   knowledge_document_fetch：rip.knowledge.document.fetch 抓取审计表。
--     ok 与 rejected 都落行（拒绝面审计复核）；行只追加，不更新不删除。
--     内容寻址：fetch_id = fdf-<sha256(url|final|content_sha|时刻) 前 24 位>
--     （ok 行）；拒绝行按 sha256(url|reason|时刻) 同词形生成。本表唯一写面
--     是 document_fetch.rs::record_fetch（受控写面纪律，grep 断言锁定）。
--     正文默认不落盘（只落哈希与元数据），计划 §6.2C「内容哈希与抓取时间
--     记录」的持久面即本表。
--
-- 红线（ROADMAP §13）：新迁移只在开发库副本与测试内存库执行，
-- migrations/platform*.db 一律不写。
--
-- 幂等：CREATE TABLE IF NOT EXISTS + CREATE INDEX IF NOT EXISTS +
-- INSERT OR IGNORE + PRAGMA user_version = 12，可重复执行；由
-- crates/database 迁移执行器（migrate.rs）按 user_version 跳过。
-- ============================================================================

CREATE TABLE IF NOT EXISTS knowledge_document_fetch (
  fetch_id       TEXT PRIMARY KEY,            -- fdf-<sha256 前 24 位>（内容寻址，重试幂等）
  source_id      TEXT NOT NULL DEFAULT '',    -- 来源插件 id（FetchInput.source_id）
  requested_url  TEXT NOT NULL,               -- 首跳 URL（approved 闸核对面）
  final_url      TEXT NOT NULL,               -- 末跳 URL（重定向后）
  hops_json      TEXT NOT NULL DEFAULT '[]',  -- 逐跳 URL 链（每跳过 allowlist + 私网复核）
  status         TEXT NOT NULL
                   CHECK (status IN ('ok','rejected')),
  reject_reason  TEXT,                        -- 结构化错误码（private_network_blocked 等）
  reject_detail  TEXT,                        -- 人类可读详情
  content_sha256 TEXT,                        -- 响应体 sha256（ok 行必有）
  content_bytes  INTEGER,                     -- 响应体字节数
  mime_type      TEXT,                        -- Content-Type 主值（MIME 白名单核对面）
  fetched_at     TEXT NOT NULL,               -- ISO8601 UTC 抓取时刻
  elapsed_ms     INTEGER NOT NULL DEFAULT 0,  -- 耗时
  policy_json    TEXT NOT NULL DEFAULT '{}',  -- allowlist/限额策略快照
  created_at     TEXT NOT NULL                -- 行创建时刻（只追加，不更新）
);

CREATE INDEX IF NOT EXISTS ix_kdf_status
  ON knowledge_document_fetch(status);
CREATE INDEX IF NOT EXISTS ix_kdf_source
  ON knowledge_document_fetch(source_id);

-- ---------------------------------------------------------------------------
-- schema_version：Phase 6 SK-3 文档抓取审计数据面变更登记（version=12）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (12, 'V1 additive SK-3 document fetch audit (knowledge_document_fetch)',
   'Phase 6 P6-11 SK-3（安全知识源计划 §6.2C）：rip.knowledge.document.fetch 抓取审计表 knowledge_document_fetch（ok/rejected 双态只追加，fetch_id 内容寻址，hops_json 逐跳复核链，content_sha256/content_bytes/mime_type/fetched_at 记录面，policy_json allowlist 快照；唯一写面 document_fetch.rs::record_fetch；本批传输面仅 http 明文，https 按 scheme_unsupported 结构化拒绝；只在开发库副本与测试内存库执行，platform*.db 不写）',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '012_knowledge_document_fetch.sql');

-- user_version 双写（DEC-002 条件，与 001_init.sql 尾部同款）
PRAGMA user_version = 12;
