-- Admit static Nuclei template metadata candidates. Template request content
-- is never stored as candidate metadata or executed by ingestion.

ALTER TABLE knowledge_ingestion_candidates
    DROP CONSTRAINT IF EXISTS knowledge_ingestion_candidates_candidate_type_check;

ALTER TABLE knowledge_ingestion_candidates
    ADD CONSTRAINT knowledge_ingestion_candidates_candidate_type_check
        CHECK (candidate_type IN (
            'vulnerability_identifier',
            'weakness_identifier',
            'nuclei_template'
        ));
