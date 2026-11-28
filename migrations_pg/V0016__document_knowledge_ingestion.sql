-- Extend the ingestion protocol to parsed document revisions.
-- Source bodies remain owned by the Document Domain and its CAS store.

ALTER TABLE knowledge_ingestions
    ALTER COLUMN source_ref DROP NOT NULL;
ALTER TABLE knowledge_ingestions
    DROP CONSTRAINT knowledge_ingestions_source_ref_fkey;
ALTER TABLE knowledge_ingestions
    DROP CONSTRAINT knowledge_ingestions_source_kind_check;
ALTER TABLE knowledge_ingestions
    ADD COLUMN source_document_revision_id TEXT
        REFERENCES document_revisions(revision_id) ON DELETE RESTRICT;
ALTER TABLE knowledge_ingestions
    ADD CONSTRAINT knowledge_ingestions_source_kind_check
        CHECK (source_kind IN ('research_report','document_revision'));
ALTER TABLE knowledge_ingestions
    ADD CONSTRAINT knowledge_ingestions_source_shape_check
        CHECK (
            (source_kind = 'research_report'
                AND source_ref IS NOT NULL
                AND source_document_revision_id IS NULL)
            OR
            (source_kind = 'document_revision'
                AND source_ref IS NULL
                AND source_document_revision_id IS NOT NULL)
        );
CREATE INDEX idx_ingestions_document_revision
    ON knowledge_ingestions(source_document_revision_id)
    WHERE source_document_revision_id IS NOT NULL;

CREATE TABLE knowledge_ingestion_evidence (
    evidence_id TEXT PRIMARY KEY,
    ingestion_id TEXT NOT NULL
        REFERENCES knowledge_ingestions(ingestion_id) ON DELETE RESTRICT,
    document_id TEXT NOT NULL REFERENCES documents(document_id) ON DELETE RESTRICT,
    revision_id TEXT NOT NULL REFERENCES document_revisions(revision_id) ON DELETE RESTRICT,
    block_id TEXT NOT NULL REFERENCES document_blocks(block_id) ON DELETE RESTRICT,
    block_sha256 TEXT NOT NULL CHECK (block_sha256 ~ '^[0-9a-f]{64}$'),
    location_json JSONB NOT NULL CHECK (jsonb_typeof(location_json) = 'object'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (ingestion_id, block_id, block_sha256)
);
CREATE INDEX idx_ingestion_evidence_revision
    ON knowledge_ingestion_evidence(revision_id, block_id);

CREATE TABLE knowledge_ingestion_candidates (
    candidate_id TEXT PRIMARY KEY,
    ingestion_id TEXT NOT NULL
        REFERENCES knowledge_ingestions(ingestion_id) ON DELETE RESTRICT,
    workspace_id TEXT NOT NULL CHECK (btrim(workspace_id) <> ''),
    candidate_type TEXT NOT NULL CHECK (candidate_type IN ('vulnerability_identifier','weakness_identifier')),
    canonical_key TEXT NOT NULL CHECK (btrim(canonical_key) <> ''),
    normalized_hash TEXT NOT NULL CHECK (normalized_hash ~ '^[0-9a-f]{64}$'),
    status TEXT NOT NULL DEFAULT 'PENDING_REVIEW'
        CHECK (status IN (
            'PENDING_REVIEW','DUPLICATE_REVIEW','VERSION_CHANGE_REVIEW',
            'EXISTING_ENTITY','REVIEWED','REJECTED','DEFERRED'
        )),
    duplicate_of TEXT REFERENCES knowledge_ingestion_candidates(candidate_id) ON DELETE RESTRICT,
    prior_candidate_id TEXT
        REFERENCES knowledge_ingestion_candidates(candidate_id) ON DELETE RESTRICT,
    existing_entity_ref_id TEXT REFERENCES knowledge_entity_refs(entity_ref_id) ON DELETE RESTRICT,
    review_batch_id TEXT REFERENCES review_batch(batch_id) ON DELETE RESTRICT,
    review_item_id BIGINT REFERENCES review_batch_item(item_id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (ingestion_id, candidate_type, canonical_key),
    CHECK (status <> 'DUPLICATE_REVIEW' OR duplicate_of IS NOT NULL),
    CHECK (status <> 'VERSION_CHANGE_REVIEW' OR prior_candidate_id IS NOT NULL),
    CHECK (status <> 'EXISTING_ENTITY' OR existing_entity_ref_id IS NOT NULL)
);
CREATE INDEX idx_ingestion_candidates_workspace_status
    ON knowledge_ingestion_candidates(workspace_id, status, created_at DESC);
CREATE INDEX idx_ingestion_candidates_identity
    ON knowledge_ingestion_candidates(workspace_id, candidate_type, canonical_key);

CREATE TABLE knowledge_ingestion_revision_diffs (
    ingestion_id TEXT PRIMARY KEY
        REFERENCES knowledge_ingestions(ingestion_id) ON DELETE RESTRICT,
    document_id TEXT NOT NULL REFERENCES documents(document_id) ON DELETE RESTRICT,
    revision_id TEXT NOT NULL REFERENCES document_revisions(revision_id) ON DELETE RESTRICT,
    previous_revision_id TEXT
        REFERENCES document_revisions(revision_id) ON DELETE RESTRICT,
    added_block_hashes JSONB NOT NULL CHECK (jsonb_typeof(added_block_hashes) = 'array'),
    removed_block_hashes JSONB NOT NULL CHECK (jsonb_typeof(removed_block_hashes) = 'array'),
    unchanged_block_count INTEGER NOT NULL CHECK (unchanged_block_count >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_ingestion_revision_diffs_document
    ON knowledge_ingestion_revision_diffs(document_id, revision_id);

CREATE TABLE knowledge_ingestion_candidate_evidence (
    candidate_id TEXT NOT NULL
        REFERENCES knowledge_ingestion_candidates(candidate_id) ON DELETE RESTRICT,
    evidence_id TEXT NOT NULL
        REFERENCES knowledge_ingestion_evidence(evidence_id) ON DELETE RESTRICT,
    char_start INTEGER NOT NULL CHECK (char_start >= 0),
    char_end INTEGER NOT NULL CHECK (char_end > char_start),
    matched_text TEXT NOT NULL CHECK (btrim(matched_text) <> ''),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (candidate_id, evidence_id, char_start, char_end),
    CHECK (char_end - char_start <= 100)
);
CREATE INDEX idx_ingestion_candidate_evidence_block
    ON knowledge_ingestion_candidate_evidence(evidence_id, char_start);

ALTER TABLE knowledge_ingestion_outputs
    DROP CONSTRAINT knowledge_ingestion_outputs_output_kind_check;
ALTER TABLE knowledge_ingestion_outputs
    ADD CONSTRAINT knowledge_ingestion_outputs_output_kind_check
        CHECK (output_kind IN ('finding','knowledge_candidate','document_revision','document_evidence'));
