-- Mutable source-trust/license assessments are separated from immutable snapshots.

CREATE TABLE knowledge_entity_source_assessments (
    source_ref_id TEXT PRIMARY KEY
        REFERENCES knowledge_entity_sources(source_ref_id) ON DELETE RESTRICT,
    review_status TEXT NOT NULL DEFAULT 'UNREVIEWED'
        CHECK (review_status IN ('UNREVIEWED', 'APPROVED', 'REJECTED', 'QUARANTINED', 'DEPRECATED')),
    trust_class TEXT NOT NULL DEFAULT 'UNASSESSED'
        CHECK (trust_class IN (
            'UNASSESSED', 'OFFICIAL_PRIMARY', 'MAINTAINER_PRIMARY',
            'CURATED_SECONDARY', 'COMMUNITY', 'UNKNOWN'
        )),
    license_expression TEXT NOT NULL DEFAULT 'UNKNOWN'
        CHECK (btrim(license_expression) <> ''),
    license_status TEXT NOT NULL DEFAULT 'UNKNOWN'
        CHECK (license_status IN ('UNKNOWN', 'DECLARED', 'VERIFIED', 'RESTRICTED', 'UNLICENSED')),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO knowledge_entity_source_assessments (source_ref_id)
SELECT source_ref_id FROM knowledge_entity_sources
ON CONFLICT (source_ref_id) DO NOTHING;

CREATE TABLE knowledge_entity_source_review_log (
    review_id BIGSERIAL PRIMARY KEY,
    source_ref_id TEXT NOT NULL
        REFERENCES knowledge_entity_sources(source_ref_id) ON DELETE RESTRICT,
    from_status TEXT NOT NULL
        CHECK (from_status IN ('UNREVIEWED', 'APPROVED', 'REJECTED', 'QUARANTINED', 'DEPRECATED')),
    to_status TEXT NOT NULL
        CHECK (to_status IN ('UNREVIEWED', 'APPROVED', 'REJECTED', 'QUARANTINED', 'DEPRECATED')),
    from_trust_class TEXT NOT NULL
        CHECK (from_trust_class IN (
            'UNASSESSED', 'OFFICIAL_PRIMARY', 'MAINTAINER_PRIMARY',
            'CURATED_SECONDARY', 'COMMUNITY', 'UNKNOWN'
        )),
    to_trust_class TEXT NOT NULL
        CHECK (to_trust_class IN (
            'UNASSESSED', 'OFFICIAL_PRIMARY', 'MAINTAINER_PRIMARY',
            'CURATED_SECONDARY', 'COMMUNITY', 'UNKNOWN'
        )),
    from_license_expression TEXT NOT NULL,
    to_license_expression TEXT NOT NULL,
    from_license_status TEXT NOT NULL
        CHECK (from_license_status IN ('UNKNOWN', 'DECLARED', 'VERIFIED', 'RESTRICTED', 'UNLICENSED')),
    to_license_status TEXT NOT NULL
        CHECK (to_license_status IN ('UNKNOWN', 'DECLARED', 'VERIFIED', 'RESTRICTED', 'UNLICENSED')),
    reason TEXT NOT NULL CHECK (btrim(reason) <> ''),
    reviewer TEXT NOT NULL CHECK (btrim(reviewer) <> ''),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_entity_source_review_log
    ON knowledge_entity_source_review_log(source_ref_id, created_at);

CREATE INDEX idx_entity_source_assessment_review
    ON knowledge_entity_source_assessments(review_status, trust_class);

CREATE OR REPLACE FUNCTION prevent_knowledge_review_history_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'knowledge review history is append-only'
        USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER knowledge_entity_source_review_log_immutable
    BEFORE UPDATE OR DELETE ON knowledge_entity_source_review_log
    FOR EACH ROW EXECUTE FUNCTION prevent_knowledge_review_history_mutation();

CREATE TRIGGER knowledge_relation_review_log_immutable
    BEFORE UPDATE OR DELETE ON knowledge_relation_review_log
    FOR EACH ROW EXECUTE FUNCTION prevent_knowledge_review_history_mutation();
