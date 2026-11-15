-- RIP3 情报核迁移链 V0003：解析管线（P2-1，04 号冻结物 + R2 4.6 实施化）。
-- document_assets 表（04 号 IR assets 清单关系化）+ document_revisions 解析列
-- （IR 凭证 CAS 键 / extraction_quality / 九态 parse_state）。
-- 词表：parse_state = R2 4.6 九态 + FAILED/CANCELLED/QUARANTINED 失败态。

-- 提取物清单（图片/表格/图表；本体进 CAS，行引用账面）。
CREATE TABLE document_assets (
    asset_id        TEXT PRIMARY KEY,            -- as- 前缀（IR asset_id 提交侧重映射）
    document_id     TEXT NOT NULL REFERENCES documents(document_id),
    revision_id     TEXT NOT NULL REFERENCES document_revisions(revision_id),
    kind            TEXT NOT NULL CHECK (kind IN ('image','table','chart')),
    content_hash    TEXT NOT NULL REFERENCES content_objects(content_hash),
    storage_uri     TEXT NOT NULL,               -- cas://<hash>（Adapter 寻址）
    source_block_id TEXT REFERENCES document_blocks(block_id),
    mime_type       TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_assets_revision ON document_assets(revision_id);
CREATE INDEX idx_assets_block ON document_assets(source_block_id);

-- 结构化载荷（table rows/code source 子对象；V0002 blocks 表无此列，
-- 04 号 IR 关系化的载荷侧；image_ref 由 document_assets.source_block_id 承载）。
ALTER TABLE document_blocks ADD COLUMN structured_payload JSONB;

-- 解析列：IR JSON 文本进 CAS 的凭证键（可重放原始产物）；质量自检快照；
-- 解析任务九态（状态机权威在 workflow run，本列为文档侧视图，ADR-0003 裁决 4）。
ALTER TABLE document_revisions
    ADD COLUMN ir_content_hash   TEXT REFERENCES content_objects(content_hash),
    ADD COLUMN extraction_quality JSONB,
    ADD COLUMN parse_state TEXT NOT NULL DEFAULT 'DISCOVERED' CHECK (parse_state IN
        ('DISCOVERED','VALIDATING','QUEUED','PARSING','NORMALIZING','EXTRACTING',
         'QUALITY_CHECK','REVIEW_PENDING','APPROVED','FAILED','CANCELLED','QUARANTINED'));
CREATE INDEX idx_revisions_parse_state ON document_revisions(document_id, parse_state);
