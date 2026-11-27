-- Cross-authority knowledge identity references and reviewable evidence-backed
-- relations. Entity facts remain in their current authoritative stores.

CREATE TABLE knowledge_entity_refs (
    entity_ref_id TEXT PRIMARY KEY,
    entity_type TEXT NOT NULL CHECK (entity_type IN (
        'knowledge_card', 'vulnerability', 'detection_template', 'poc',
        'security_tool', 'technical_document', 'evidence'
    )),
    canonical_key TEXT NOT NULL CHECK (btrim(canonical_key) <> ''),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (entity_type, canonical_key)
);

CREATE TABLE knowledge_entity_sources (
    source_ref_id TEXT PRIMARY KEY,
    entity_ref_id TEXT NOT NULL REFERENCES knowledge_entity_refs(entity_ref_id) ON DELETE RESTRICT,
    source_system TEXT NOT NULL CHECK (btrim(source_system) <> ''),
    source_entity_id TEXT NOT NULL CHECK (btrim(source_entity_id) <> ''),
    source_version TEXT NOT NULL DEFAULT '',
    canonical_uri TEXT,
    source_commit TEXT,
    content_sha256 TEXT CHECK (
        content_sha256 IS NULL OR content_sha256 ~ '^[0-9a-f]{64}$'
    ),
    observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    source_state TEXT NOT NULL DEFAULT 'CURRENT' CHECK (
        source_state IN ('CURRENT', 'MISSING', 'DEPRECATED', 'UNVERIFIED')
    ),
    UNIQUE (entity_ref_id, source_system, source_entity_id, source_version),
    UNIQUE (source_ref_id, entity_ref_id)
);
CREATE INDEX idx_entity_sources_system_id
    ON knowledge_entity_sources(source_system, source_entity_id);
CREATE INDEX idx_entity_sources_commit
    ON knowledge_entity_sources(source_commit) WHERE source_commit IS NOT NULL;

-- Existing KnowledgeCard endpoints keep their IDs. Backfill identity references
-- before retargeting the relation foreign keys.
INSERT INTO knowledge_entity_refs (entity_ref_id, entity_type, canonical_key)
SELECT knowledge_id, 'knowledge_card', knowledge_id FROM knowledge_cards
ON CONFLICT (entity_type, canonical_key) DO NOTHING;

INSERT INTO knowledge_entity_sources (
    source_ref_id, entity_ref_id, source_system, source_entity_id, source_version
)
SELECT 'kesrc-' || md5(knowledge_id), knowledge_id, 'rip.intel-db', knowledge_id, ''
FROM knowledge_cards
ON CONFLICT (entity_ref_id, source_system, source_entity_id, source_version) DO NOTHING;

ALTER TABLE knowledge_relations
    DROP CONSTRAINT IF EXISTS knowledge_relations_src_id_fkey;
ALTER TABLE knowledge_relations
    DROP CONSTRAINT IF EXISTS knowledge_relations_dst_id_fkey;
ALTER TABLE knowledge_relations
    ADD CONSTRAINT fk_knowledge_relations_src_entity_ref
        FOREIGN KEY (src_id) REFERENCES knowledge_entity_refs(entity_ref_id) ON DELETE RESTRICT,
    ADD CONSTRAINT fk_knowledge_relations_dst_entity_ref
        FOREIGN KEY (dst_id) REFERENCES knowledge_entity_refs(entity_ref_id) ON DELETE RESTRICT;

ALTER TABLE knowledge_relations
    DROP CONSTRAINT IF EXISTS knowledge_relations_relation_type_check;
ALTER TABLE knowledge_relations
    ADD CONSTRAINT knowledge_relations_relation_type_check CHECK (relation_type IN (
        'SAME_CONTENT', 'NEAR_DUPLICATE', 'RELATED_TO', 'SUPPORTS',
        'CONTRADICTS', 'SUPERSEDES', 'FIXES', 'DETECTS', 'DERIVED_FROM',
        'REFERENCES', 'EXPLOITS', 'AFFECTS', 'FIXED_BY', 'ANALYZES',
        'MITIGATED_BY', 'USES_TOOL', 'VALIDATES'
    ));

ALTER TABLE knowledge_relations
    ADD COLUMN review_status TEXT NOT NULL DEFAULT 'CANDIDATE'
        CHECK (review_status IN ('CANDIDATE', 'APPROVED', 'REJECTED', 'DEPRECATED')),
    ADD COLUMN valid_from TIMESTAMPTZ,
    ADD COLUMN valid_to TIMESTAMPTZ,
    ADD CONSTRAINT knowledge_relations_validity_range_check
        CHECK (valid_to IS NULL OR valid_from IS NULL OR valid_to >= valid_from);

CREATE INDEX idx_relations_review_validity
    ON knowledge_relations(review_status, valid_from, valid_to);

CREATE TABLE knowledge_relation_evidence (
    relation_id TEXT NOT NULL REFERENCES knowledge_relations(relation_id) ON DELETE RESTRICT,
    evidence_entity_ref_id TEXT NOT NULL REFERENCES knowledge_entity_refs(entity_ref_id) ON DELETE RESTRICT,
    evidence_source_ref_id TEXT NOT NULL,
    evidence_role TEXT NOT NULL CHECK (
        evidence_role IN ('supports', 'contradicts', 'context')
    ),
    linked_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (relation_id, evidence_entity_ref_id, evidence_source_ref_id),
    FOREIGN KEY (evidence_source_ref_id, evidence_entity_ref_id)
        REFERENCES knowledge_entity_sources(source_ref_id, entity_ref_id) ON DELETE RESTRICT
);
CREATE INDEX idx_relation_evidence_entity
    ON knowledge_relation_evidence(evidence_entity_ref_id);

CREATE TABLE knowledge_relation_review_log (
    log_id BIGSERIAL PRIMARY KEY,
    relation_id TEXT NOT NULL REFERENCES knowledge_relations(relation_id) ON DELETE RESTRICT,
    from_status TEXT NOT NULL CHECK (
        from_status IN ('CANDIDATE', 'APPROVED', 'REJECTED', 'DEPRECATED')
    ),
    to_status TEXT NOT NULL CHECK (
        to_status IN ('CANDIDATE', 'APPROVED', 'REJECTED', 'DEPRECATED')
    ),
    reason TEXT NOT NULL CHECK (btrim(reason) <> ''),
    reviewer TEXT NOT NULL CHECK (btrim(reviewer) <> ''),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_relation_review_log_relation
    ON knowledge_relation_review_log(relation_id, created_at);
