-- Batch review ledger (M10): decisions are recorded here first; authoritative
-- stores (knowledge_relations, KSE source entries, RTIS k_meta) are only
-- touched by an explicit `apply` step. Nothing in this schema approves or
-- promotes any candidate by itself.

CREATE TABLE review_batch (
    batch_id      TEXT PRIMARY KEY,
    domain        TEXT NOT NULL,
    source_file   TEXT NOT NULL,
    source_sha256 TEXT NOT NULL,
    item_count    INTEGER NOT NULL DEFAULT 0,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    schema_version TEXT NOT NULL DEFAULT '1'
);

CREATE TABLE review_batch_item (
    item_id       BIGSERIAL PRIMARY KEY,
    batch_id      TEXT NOT NULL REFERENCES review_batch(batch_id) ON DELETE CASCADE,
    candidate_id  TEXT NOT NULL,
    priority_rank INTEGER,
    priority_band TEXT,
    decision      TEXT NOT NULL DEFAULT 'PENDING'
                  CHECK (decision IN ('PENDING','APPROVED','REJECTED','DEFERRED')),
    reviewer      TEXT,
    reason        TEXT,
    decided_at    TIMESTAMPTZ,
    applied_at    TIMESTAMPTZ,
    payload       JSONB NOT NULL DEFAULT '{}'::jsonb,
    UNIQUE (batch_id, candidate_id)
);

CREATE INDEX IF NOT EXISTS idx_review_batch_item_batch_decision
    ON review_batch_item(batch_id, decision);

-- Append-only decision history; `undo` writes the inverse as a new row and
-- never edits or deletes existing rows.
CREATE TABLE review_batch_decision_log (
    log_id       BIGSERIAL PRIMARY KEY,
    item_id      BIGINT NOT NULL REFERENCES review_batch_item(item_id) ON DELETE CASCADE,
    action       TEXT NOT NULL CHECK (action IN ('decide','undo','apply','apply_undo')),
    from_decision TEXT NOT NULL,
    to_decision   TEXT NOT NULL,
    reviewer     TEXT NOT NULL,
    reason       TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_review_batch_decision_log_item
    ON review_batch_decision_log(item_id, log_id DESC);
