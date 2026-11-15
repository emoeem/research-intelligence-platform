-- RIP3 情报核迁移链 V0004：知识治理域（Phase 3，03-DOMAIN-MODEL 1.5/1.6 +
-- R2 第五部分 + P3-1）。KnowledgeCard / document_cards N:M / KnowledgeRelation
-- 10 型 / 审核日志面；向量列（D4 语义候选）随 P3-3 批次。

-- 知识卡片（方法论知识；漏洞条目/POC/链属 RTIS，03 号冻结物第六节边界）。
CREATE TABLE knowledge_cards (
    knowledge_id        TEXT PRIMARY KEY,            -- kc- 前缀
    knowledge_type      TEXT NOT NULL CHECK (knowledge_type IN
        ('vuln_principle','vuln_signature','exploit_condition','detection_method',
         'fix_method','security_technique','tool_usage','research_case')),
    title               TEXT NOT NULL,
    summary             TEXT NOT NULL DEFAULT '',
    content             TEXT NOT NULL DEFAULT '',    -- 卡片正文（D2 规范化哈希的输入）
    structured_content  JSONB,                       -- 类型化载荷（子 Schema 随 Phase 3 ADR）
    confidence          REAL NOT NULL DEFAULT 0.5 CHECK (confidence >= 0 AND confidence <= 1),
    quality_score       REAL CHECK (quality_score >= 0 AND quality_score <= 1),
    verification_status TEXT NOT NULL DEFAULT 'UNVERIFIED' CHECK (verification_status IN ('UNVERIFIED','VERIFIED')),
    review_status       TEXT NOT NULL DEFAULT 'DISCOVERED' CHECK (review_status IN
        ('DISCOVERED','PARSED','CANDIDATE','APPROVED','QUARANTINED','REJECTED','DEPRECATED')),
    evidence_level      TEXT CHECK (evidence_level IN ('K0','K1','K2','K3','K4')),
    source_trust        TEXT NOT NULL DEFAULT 'external_import' CHECK (source_trust IN
        ('external_import','rtis_snapshot','agent_derived','manual')),
    ai_derived          BOOLEAN NOT NULL DEFAULT FALSE,  -- 原则 C：AI 派生必标
    normalized_hash     TEXT,                        -- D2 规范化哈希（去重键，可空）
    quality_flags       TEXT[] NOT NULL DEFAULT '{}',  -- R2 5.3 七类风险标记
    workspace_id        TEXT NOT NULL DEFAULT 'default',
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_cards_review ON knowledge_cards(review_status, knowledge_type);
CREATE INDEX idx_cards_norm ON knowledge_cards(normalized_hash);
CREATE INDEX idx_cards_workspace ON knowledge_cards(workspace_id, review_status);

-- 卡片与文档 N:M（03-DOMAIN-MODEL 1.5 文档关联；版本锚定）。
CREATE TABLE document_cards (
    document_id          TEXT NOT NULL REFERENCES documents(document_id),
    knowledge_id         TEXT NOT NULL REFERENCES knowledge_cards(knowledge_id),
    document_revision_id TEXT NOT NULL REFERENCES document_revisions(revision_id),
    PRIMARY KEY (document_id, knowledge_id, document_revision_id)
);
CREATE INDEX idx_doc_cards_card ON document_cards(knowledge_id);

-- 知识关系（R2 3.1 十型；每条关系必有独立证据与来源；D4 只产候选边）。
CREATE TABLE knowledge_relations (
    relation_id       TEXT PRIMARY KEY,            -- krel- 前缀
    src_id            TEXT NOT NULL REFERENCES knowledge_cards(knowledge_id),
    dst_id            TEXT NOT NULL REFERENCES knowledge_cards(knowledge_id),
    relation_type     TEXT NOT NULL CHECK (relation_type IN
        ('SAME_CONTENT','NEAR_DUPLICATE','RELATED_TO','SUPPORTS','CONTRADICTS',
         'SUPERSEDES','FIXES','DETECTS','DERIVED_FROM','REFERENCES')),
    relation_evidence JSONB,
    relation_source   TEXT NOT NULL,               -- deducer 名 + 版本
    confidence        REAL CHECK (confidence >= 0 AND confidence <= 1),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (src_id, dst_id, relation_type)
);
CREATE INDEX idx_relations_src ON knowledge_relations(src_id, relation_type);
CREATE INDEX idx_relations_dst ON knowledge_relations(dst_id, relation_type);

-- 审核日志（R2 5.4：审核原因、审核历史、状态回退可追溯；append-only）。
CREATE TABLE knowledge_review_log (
    log_id        BIGSERIAL PRIMARY KEY,
    knowledge_id  TEXT NOT NULL REFERENCES knowledge_cards(knowledge_id),
    from_status   TEXT NOT NULL,
    to_status     TEXT NOT NULL,
    decision      TEXT NOT NULL,                   -- approve/quarantine/reject/flag/restore 等
    reason        TEXT NOT NULL DEFAULT '',
    reviewer      TEXT NOT NULL DEFAULT 'manual',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_review_log_card ON knowledge_review_log(knowledge_id, created_at);
