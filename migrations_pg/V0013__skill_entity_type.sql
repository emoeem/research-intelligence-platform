-- Skills are first-class graph identities, but their authoritative body and
-- execution metadata remain in the existing Skill registry. No Skill rows are
-- copied or promoted by this schema-only migration.

ALTER TABLE knowledge_entity_refs
    DROP CONSTRAINT IF EXISTS knowledge_entity_refs_entity_type_check;

ALTER TABLE knowledge_entity_refs
    ADD CONSTRAINT knowledge_entity_refs_entity_type_check CHECK (entity_type IN (
        'knowledge_card', 'vulnerability', 'detection_template', 'poc',
        'security_tool', 'technical_document', 'evidence', 'skill'
    ));

CREATE INDEX IF NOT EXISTS idx_knowledge_entity_refs_skill_key
    ON knowledge_entity_refs(canonical_key)
    WHERE entity_type = 'skill';
