-- Candidate embeddings and similarities are derived review aids. They never
-- publish, merge, approve, or delete knowledge.

ALTER TABLE knowledge_ingestion_candidates
    ADD CONSTRAINT uq_ingestion_candidate_semantic_scope
        UNIQUE (candidate_id, workspace_id, candidate_type),
    ADD CONSTRAINT uq_ingestion_candidate_semantic_source
        UNIQUE (candidate_id, ingestion_id, workspace_id, candidate_type);

CREATE TABLE knowledge_ingestion_candidate_vectors (
    candidate_id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    candidate_type TEXT NOT NULL CHECK (candidate_type = 'nuclei_template'),
    embedding vector(512) NOT NULL,
    embedding_model_id TEXT NOT NULL,
    embedding_version TEXT NOT NULL,
    embedding_input_sha256 TEXT NOT NULL
        CHECK (embedding_input_sha256 ~ '^[0-9a-f]{64}$'),
    dimension INTEGER NOT NULL CHECK (dimension = 512),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (candidate_id, workspace_id, candidate_type)
        REFERENCES knowledge_ingestion_candidates
            (candidate_id, workspace_id, candidate_type)
        ON DELETE RESTRICT
);
CREATE INDEX idx_ingestion_candidate_vectors_scope
    ON knowledge_ingestion_candidate_vectors(workspace_id, candidate_type);
CREATE INDEX idx_ingestion_candidate_vectors_hnsw
    ON knowledge_ingestion_candidate_vectors
    USING hnsw (embedding vector_cosine_ops);

CREATE TABLE knowledge_ingestion_candidate_similarity_suggestions (
    ingestion_id TEXT NOT NULL
        REFERENCES knowledge_ingestions(ingestion_id) ON DELETE RESTRICT,
    source_candidate_id TEXT NOT NULL,
    matched_candidate_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    candidate_type TEXT NOT NULL CHECK (candidate_type = 'nuclei_template'),
    cosine_similarity REAL NOT NULL CHECK (cosine_similarity BETWEEN -1 AND 1),
    threshold REAL NOT NULL CHECK (threshold BETWEEN 0 AND 1),
    embedding_model_id TEXT NOT NULL,
    embedding_version TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (
        ingestion_id, source_candidate_id, matched_candidate_id,
        embedding_model_id, embedding_version
    ),
    CHECK (source_candidate_id <> matched_candidate_id),
    FOREIGN KEY (
        source_candidate_id, ingestion_id, workspace_id, candidate_type
    ) REFERENCES knowledge_ingestion_candidates
        (candidate_id, ingestion_id, workspace_id, candidate_type)
        ON DELETE RESTRICT,
    FOREIGN KEY (matched_candidate_id, workspace_id, candidate_type)
        REFERENCES knowledge_ingestion_candidates
            (candidate_id, workspace_id, candidate_type)
        ON DELETE RESTRICT
);
CREATE INDEX idx_ingestion_candidate_similarity_source
    ON knowledge_ingestion_candidate_similarity_suggestions
        (ingestion_id, source_candidate_id, cosine_similarity DESC);
