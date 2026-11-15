-- RIP3 情报核迁移链 V0006：混合检索面（Phase 4 P4-1，R2 第六部分 6.2/6.4）。
-- BM25 面（ParadeDB pg_search 0.26：key_field 自 0.26 起弃用 no-op，不提供）。
CREATE INDEX IF NOT EXISTS idx_cards_bm25 ON knowledge_cards
    USING bm25 (knowledge_id, title, content);
