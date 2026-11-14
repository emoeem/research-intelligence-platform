-- RIP3 情报核迁移链 V0002：Document 域（03-DOMAIN-MODEL.md 定稿，P1-2）。
-- content_objects / source_records / documents / document_revisions /
-- document_blocks 五表；KnowledgeCard/Relation/向量列属 Phase 3/4 批次。

-- 内容寻址对象账面（R2 10.2：对象本体在 Object Storage，此处为账面与引用计数）。
CREATE TABLE content_objects (
    content_hash   TEXT PRIMARY KEY,            -- sha256 hex（CAS 寻址键）
    size_bytes     BIGINT NOT NULL CHECK (size_bytes >= 0),
    mime_type      TEXT NOT NULL,               -- 魔法数检测值（13-SECURITY）
    storage_uri    TEXT NOT NULL,               -- cas://<hash>（Adapter 解析，业务零拼路径）
    ref_count      BIGINT NOT NULL DEFAULT 0 CHECK (ref_count >= 0),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 来源记录（R2 5.5：内容复用与来源分离，同一对象 N 来源独立保留）。
CREATE TABLE source_records (
    source_id      TEXT PRIMARY KEY,            -- src- 前缀
    content_hash   TEXT NOT NULL REFERENCES content_objects(content_hash),
    source_type    TEXT NOT NULL CHECK (source_type IN ('git','github','url','upload')),
    source_url     TEXT NOT NULL DEFAULT '',
    source_commit  TEXT,                        -- Git 面 pinned commit（可空）
    license_status TEXT NOT NULL DEFAULT 'UNKNOWN',  -- 记录制（用户裁决：许可不做淘汰门槛）
    workspace_id   TEXT NOT NULL DEFAULT 'default',
    imported_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_source_records_hash ON source_records(content_hash);

-- 文档主表（03-DOMAIN-MODEL 1.1；软删除优先）。
CREATE TABLE documents (
    document_id       TEXT PRIMARY KEY,         -- doc- 前缀
    title             TEXT NOT NULL,
    document_type     TEXT NOT NULL CHECK (document_type IN
        ('book','pdf','docx','pptx','xlsx','html','markdown','code','image','dataset','other')),
    original_filename TEXT NOT NULL,
    mime_type         TEXT NOT NULL,
    file_size         BIGINT NOT NULL CHECK (file_size >= 0),
    storage_uri       TEXT NOT NULL,            -- 当前版本对象 cas://（冗余视图，加速读）
    sha256            TEXT NOT NULL REFERENCES content_objects(content_hash),
    owner_id          TEXT NOT NULL DEFAULT 'owner-local',
    workspace_id      TEXT NOT NULL DEFAULT 'default',
    visibility        TEXT NOT NULL DEFAULT 'private' CHECK (visibility IN ('private','shared','public')),
    language          TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    lifecycle_status  TEXT NOT NULL DEFAULT 'ACTIVE' CHECK (lifecycle_status IN ('ACTIVE','DELETED'))
);
CREATE INDEX idx_documents_workspace ON documents(workspace_id, visibility, lifecycle_status);
CREATE INDEX idx_documents_sha ON documents(sha256);

-- 版本链（R2 3.1：旧版本不因上传新版本被删除；revision 复用对象）。
CREATE TABLE document_revisions (
    revision_id     TEXT PRIMARY KEY,           -- drev- 前缀
    document_id     TEXT NOT NULL REFERENCES documents(document_id),
    revision_number BIGINT NOT NULL CHECK (revision_number >= 1),
    content_hash    TEXT NOT NULL REFERENCES content_objects(content_hash),
    parser_version  TEXT,
    source_version  TEXT,                       -- 上游 pinned commit/etag（可空）
    revision_status TEXT NOT NULL DEFAULT 'CURRENT' CHECK (revision_status IN ('CURRENT','SUPERSEDED')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (document_id, revision_number)
);
CREATE INDEX idx_revisions_document ON document_revisions(document_id, revision_number DESC);

-- 结构块（04-DOCUMENT-IR-SCHEMA block 词表 14 值；父子结构禁止压平）。
CREATE TABLE document_blocks (
    block_id              TEXT PRIMARY KEY,     -- blk- 前缀
    document_id           TEXT NOT NULL REFERENCES documents(document_id),
    revision_id           TEXT NOT NULL REFERENCES document_revisions(revision_id),
    parent_block_id       TEXT REFERENCES document_blocks(block_id),
    block_type            TEXT NOT NULL CHECK (block_type IN
        ('document_title','heading','paragraph','list','table','code','image','figure',
         'formula','quote','reference','page','chapter','section')),
    order_index           BIGINT NOT NULL CHECK (order_index >= 0),
    content               TEXT NOT NULL DEFAULT '',
    page_number           INTEGER,
    location_metadata     JSONB,                -- bbox/字符偏移（source_mapping 存储侧）
    extraction_method     TEXT NOT NULL CHECK (extraction_method IN
        ('docling','pymupdf','tesseract','native_markdown','manual')),
    extraction_confidence REAL
);
CREATE INDEX idx_blocks_revision ON document_blocks(revision_id, parent_block_id, order_index);
CREATE INDEX idx_blocks_type ON document_blocks(revision_id, block_type);
CREATE INDEX idx_blocks_location ON document_blocks USING GIN (location_metadata);
