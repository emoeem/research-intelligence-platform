-- RIP3 情报核迁移链 V0005：知识卡片语义向量面（Phase 3 D4，03-DOMAIN-MODEL
-- 1.7 同构卡片级；chunk 级向量随 Phase 4 检索批）。

CREATE TABLE knowledge_card_vectors (
    card_id            TEXT PRIMARY KEY REFERENCES knowledge_cards(knowledge_id),
    embedding          vector(512) NOT NULL,        -- bge-small-zh-v1.5（D7 本地推理）
    embedding_model_id TEXT NOT NULL,               -- bge-small-zh-v1.5
    embedding_version  TEXT NOT NULL,               -- worker VERSION（模型升级即失效）
    dimension          INTEGER NOT NULL CHECK (dimension = 512),
    index_version      TEXT NOT NULL DEFAULT 'v1',  -- 重建/增量追踪（R2 6.2）
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_card_vectors_hnsw ON knowledge_card_vectors
    USING hnsw (embedding vector_cosine_ops);
