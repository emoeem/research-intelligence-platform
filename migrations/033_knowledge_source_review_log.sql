-- Durable, append-only reviewer decisions for KSE source-entry transitions.
BEGIN IMMEDIATE;

CREATE TABLE IF NOT EXISTS knowledge_source_review_log (
  review_id   INTEGER PRIMARY KEY AUTOINCREMENT,
  entry_id    INTEGER NOT NULL
                REFERENCES knowledge_source_entry(entry_id) ON DELETE RESTRICT,
  from_status TEXT NOT NULL
                CHECK (from_status IN ('DISCOVERED','UNREVIEWED','APPROVED','REJECTED','DEPRECATED')),
  to_status   TEXT NOT NULL
                CHECK (to_status IN ('DISCOVERED','UNREVIEWED','APPROVED','REJECTED','DEPRECATED')),
  reviewer    TEXT NOT NULL CHECK (length(trim(reviewer)) > 0),
  reason      TEXT NOT NULL CHECK (length(trim(reason)) > 0),
  created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS ix_kse_review_log_entry
  ON knowledge_source_review_log(entry_id, created_at, review_id);

CREATE TRIGGER IF NOT EXISTS knowledge_source_review_log_no_update
BEFORE UPDATE ON knowledge_source_review_log
BEGIN
  SELECT RAISE(ABORT, 'knowledge source review history is append-only');
END;

CREATE TRIGGER IF NOT EXISTS knowledge_source_review_log_no_delete
BEFORE DELETE ON knowledge_source_review_log
BEGIN
  SELECT RAISE(ABORT, 'knowledge source review history is append-only');
END;

INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by)
VALUES (
  33,
  'V1 durable knowledge source review history',
  'Adds an append-only, entry-linked log for explicit KSE review decisions. Existing entry statuses and source data are unchanged.',
  strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
  '033_knowledge_source_review_log.sql'
);

PRAGMA user_version = 33;
COMMIT;
