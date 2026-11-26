-- RIP 032: dedicated Nuclei template content kind in the reviewable knowledge source.
-- Additive rebuild of knowledge_source_entry CHECK domain; preserve every existing row.
-- No production databases are modified by authoring this migration.
BEGIN IMMEDIATE;

CREATE TABLE knowledge_source_entry_new (
  entry_id        INTEGER PRIMARY KEY AUTOINCREMENT,
  plugin_id       TEXT NOT NULL,
  plugin_version  TEXT NOT NULL,
  corpus_id       TEXT NOT NULL,
  entry_kind      TEXT NOT NULL
                    CHECK (entry_kind IN ('poc','skill_md','method_case',
                                          'catalog_entry','document_markdown','skill_candidate',
                                          'nuclei_template')),
  item_key        TEXT NOT NULL,
  title           TEXT NOT NULL,
  description     TEXT,
  body            TEXT NOT NULL DEFAULT '',
  body_bytes      INTEGER NOT NULL DEFAULT 0,
  content_sha256  TEXT NOT NULL,
  metadata_sha256 TEXT NOT NULL,
  category_path_json TEXT NOT NULL DEFAULT '[]',
  canonical_url   TEXT,
  source_type     TEXT NOT NULL DEFAULT '',
  source_url      TEXT NOT NULL DEFAULT '',
  source_path     TEXT NOT NULL DEFAULT '',
  source_commit   TEXT,
  license_status  TEXT NOT NULL DEFAULT 'UNKNOWN',
  rights_policy   TEXT NOT NULL DEFAULT 'catalog_only'
                    CHECK (rights_policy IN ('catalog_only','approved_only','metadata_only')),
  review_status   TEXT NOT NULL DEFAULT 'DISCOVERED'
                    CHECK (review_status IN ('DISCOVERED','UNREVIEWED','APPROVED','REJECTED','DEPRECATED')),
  risk_flags_json TEXT NOT NULL DEFAULT '[]',
  source_location_json TEXT NOT NULL DEFAULT '{}',
  meta_json       TEXT,
  filtered_note   TEXT,
  first_seen_at   TEXT NOT NULL,
  last_seen_at    TEXT NOT NULL,
  upstream_missing INTEGER NOT NULL DEFAULT 0,
  UNIQUE (plugin_id, plugin_version, entry_kind, item_key)
);

INSERT INTO knowledge_source_entry_new (
  entry_id, plugin_id, plugin_version, corpus_id, entry_kind, item_key, title,
  description, body, body_bytes, content_sha256, metadata_sha256,
  category_path_json, canonical_url, source_type, source_url, source_path,
  source_commit, license_status, rights_policy, review_status, risk_flags_json,
  source_location_json, meta_json, filtered_note, first_seen_at, last_seen_at,
  upstream_missing
)
SELECT
  entry_id, plugin_id, plugin_version, corpus_id, entry_kind, item_key, title,
  description, body, body_bytes, content_sha256, metadata_sha256,
  category_path_json, canonical_url, source_type, source_url, source_path,
  source_commit, license_status, rights_policy, review_status, risk_flags_json,
  source_location_json, meta_json, filtered_note, first_seen_at, last_seen_at,
  upstream_missing
FROM knowledge_source_entry;

DROP TABLE knowledge_source_entry;
ALTER TABLE knowledge_source_entry_new RENAME TO knowledge_source_entry;

CREATE INDEX IF NOT EXISTS ix_kse_plugin
  ON knowledge_source_entry(plugin_id, plugin_version);
CREATE INDEX IF NOT EXISTS ix_kse_kind
  ON knowledge_source_entry(entry_kind);
CREATE INDEX IF NOT EXISTS ix_kse_review
  ON knowledge_source_entry(review_status);
CREATE INDEX IF NOT EXISTS ix_kse_url
  ON knowledge_source_entry(canonical_url);

INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by)
VALUES (
  32,
  'V1 dedicated Nuclei template knowledge kind',
  'Adds nuclei_template to knowledge_source_entry entry_kind CHECK by a preserving table rebuild. Existing SK-1 rows, review states, hashes, provenance and IDs are copied unchanged; RTIS knowledge_ingest remains untouched.',
  strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
  '032_nuclei_template_kind.sql'
);

PRAGMA user_version = 32;
COMMIT;
