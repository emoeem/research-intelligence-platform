-- Unified Knowledge Ingestion job protocol, first adapter: stored Research Report.
-- Raw content remains in its source authority; this table stores references and state.

CREATE TABLE knowledge_ingestions (
    ingestion_id TEXT PRIMARY KEY,
    contract_version TEXT NOT NULL DEFAULT '1.0',
    workspace_id TEXT NOT NULL CHECK (btrim(workspace_id) <> ''),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    request_sha256 TEXT NOT NULL CHECK (request_sha256 ~ '^[0-9a-f]{64}$'),
    source_kind TEXT NOT NULL CHECK (source_kind IN ('research_report')),
    source_ref TEXT NOT NULL REFERENCES reports(report_id) ON DELETE RESTRICT,
    request_json JSONB NOT NULL CHECK (jsonb_typeof(request_json) = 'object'),
    requester TEXT NOT NULL CHECK (btrim(requester) <> ''),
    run_id TEXT,
    trace_id TEXT,
    status TEXT NOT NULL DEFAULT 'PENDING'
        CHECK (status IN ('PENDING','PROCESSING','REVIEW_PENDING','COMPLETED','FAILED')),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    result_json JSONB CHECK (result_json IS NULL OR jsonb_typeof(result_json) = 'object'),
    error_code TEXT,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    UNIQUE (workspace_id, idempotency_key)
);

CREATE INDEX idx_knowledge_ingestions_workspace_status
    ON knowledge_ingestions(workspace_id, status, created_at DESC);
CREATE INDEX idx_knowledge_ingestions_source
    ON knowledge_ingestions(source_kind, source_ref);

CREATE TABLE knowledge_ingestion_outputs (
    ingestion_id TEXT NOT NULL
        REFERENCES knowledge_ingestions(ingestion_id) ON DELETE RESTRICT,
    output_kind TEXT NOT NULL CHECK (output_kind IN ('finding','knowledge_candidate')),
    output_id TEXT NOT NULL CHECK (btrim(output_id) <> ''),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (ingestion_id, output_kind, output_id)
);

CREATE INDEX idx_ingestion_outputs_output
    ON knowledge_ingestion_outputs(output_kind, output_id);
