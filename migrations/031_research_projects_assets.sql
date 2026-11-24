-- RIP UI M4 domain completion: independent Projects and Research Assets.
-- Operational/research workspace data lives in SQLite 020+; this does not
-- duplicate RTIS Findings, document_assets, or immutable verification Evidence.
-- All records are workspace-scoped. Asset observations are append-only facts.

CREATE TABLE IF NOT EXISTS research_project (
  project_id       TEXT PRIMARY KEY,
  workspace_id     TEXT NOT NULL DEFAULT 'default',
  name             TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 160),
  slug             TEXT NOT NULL CHECK (length(trim(slug)) BETWEEN 1 AND 180),
  summary          TEXT NOT NULL DEFAULT '',
  objective        TEXT NOT NULL DEFAULT '',
  status           TEXT NOT NULL DEFAULT 'draft'
                   CHECK (status IN ('draft','active','paused','completed','archived')),
  priority         TEXT NOT NULL DEFAULT 'normal'
                   CHECK (priority IN ('low','normal','high','critical')),
  scope_json       TEXT NOT NULL DEFAULT '{}'
                   CHECK (json_valid(scope_json) AND json_type(scope_json) = 'object'),
  tags_json        TEXT NOT NULL DEFAULT '[]'
                   CHECK (json_valid(tags_json) AND json_type(tags_json) = 'array'),
  owner_id         TEXT NOT NULL DEFAULT 'owner-local',
  created_at       TEXT NOT NULL,
  updated_at       TEXT NOT NULL,
  archived_at      TEXT,
  schema_version   INTEGER NOT NULL DEFAULT 1,
  UNIQUE (workspace_id, slug)
);
CREATE INDEX IF NOT EXISTS ix_research_project_workspace_status
  ON research_project(workspace_id, status, updated_at DESC);

-- Project/task is an explicit relation. research_task remains an immutable RTIS
-- mirror and is not altered or repurposed as a Project record.
CREATE TABLE IF NOT EXISTS research_project_task (
  project_id       TEXT NOT NULL REFERENCES research_project(project_id),
  task_id          TEXT NOT NULL REFERENCES research_task(task_id),
  relation_role    TEXT NOT NULL DEFAULT 'research'
                   CHECK (relation_role IN ('research','validation','monitoring')),
  linked_at        TEXT NOT NULL,
  linked_by        TEXT NOT NULL DEFAULT 'owner-local',
  PRIMARY KEY (project_id, task_id)
);
CREATE INDEX IF NOT EXISTS ix_project_task_task ON research_project_task(task_id);

-- Canonical research target identity; deliberately distinct from document_assets.
CREATE TABLE IF NOT EXISTS research_asset (
  asset_id          TEXT PRIMARY KEY,
  workspace_id      TEXT NOT NULL DEFAULT 'default',
  asset_type        TEXT NOT NULL
                    CHECK (asset_type IN ('domain','ip','url','service','repository','application','other')),
  canonical_value   TEXT NOT NULL CHECK (length(trim(canonical_value)) > 0),
  display_name      TEXT NOT NULL DEFAULT '',
  description       TEXT NOT NULL DEFAULT '',
  lifecycle_status  TEXT NOT NULL DEFAULT 'active'
                    CHECK (lifecycle_status IN ('active','inactive','archived')),
  first_seen_at     TEXT NOT NULL,
  last_seen_at      TEXT NOT NULL,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  schema_version    INTEGER NOT NULL DEFAULT 1,
  UNIQUE (workspace_id, asset_type, canonical_value)
);
CREATE INDEX IF NOT EXISTS ix_research_asset_workspace_type
  ON research_asset(workspace_id, asset_type, lifecycle_status, last_seen_at DESC);

-- Endpoint is a normalized reachable surface under an Asset.
CREATE TABLE IF NOT EXISTS research_asset_endpoint (
  endpoint_id       TEXT PRIMARY KEY,
  asset_id          TEXT NOT NULL REFERENCES research_asset(asset_id),
  scheme            TEXT NOT NULL DEFAULT '',
  host              TEXT NOT NULL,
  port              INTEGER CHECK (port IS NULL OR port BETWEEN 1 AND 65535),
  protocol          TEXT NOT NULL DEFAULT 'unknown',
  path              TEXT NOT NULL DEFAULT '/',
  lifecycle_status  TEXT NOT NULL DEFAULT 'active'
                    CHECK (lifecycle_status IN ('active','inactive','archived')),
  first_seen_at     TEXT NOT NULL,
  last_seen_at      TEXT NOT NULL,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  UNIQUE (asset_id, scheme, host, port, protocol, path)
);
CREATE INDEX IF NOT EXISTS ix_asset_endpoint_asset
  ON research_asset_endpoint(asset_id, lifecycle_status, last_seen_at DESC);

-- Observation is immutable provenance: no technology inference from a hostname.
CREATE TABLE IF NOT EXISTS research_asset_observation (
  observation_id    TEXT PRIMARY KEY,
  asset_id          TEXT NOT NULL REFERENCES research_asset(asset_id),
  endpoint_id       TEXT REFERENCES research_asset_endpoint(endpoint_id),
  observation_type  TEXT NOT NULL
                    CHECK (observation_type IN ('discovery','http_response','tls','service_probe','technology_fingerprint','manual_import','other')),
  source_type       TEXT NOT NULL
                    CHECK (source_type IN ('provider','tool_run','user_import','manual','external_feed')),
  source_ref        TEXT NOT NULL DEFAULT '',
  evidence_ref      TEXT NOT NULL DEFAULT '',
  observed_at       TEXT NOT NULL,
  content_hash      TEXT NOT NULL CHECK (length(content_hash) = 64),
  payload_json      TEXT NOT NULL
                    CHECK (json_valid(payload_json) AND json_type(payload_json) = 'object'),
  created_at        TEXT NOT NULL,
  UNIQUE (asset_id, content_hash)
);
CREATE INDEX IF NOT EXISTS ix_asset_observation_asset_time
  ON research_asset_observation(asset_id, observed_at DESC);
CREATE INDEX IF NOT EXISTS ix_asset_observation_endpoint_time
  ON research_asset_observation(endpoint_id, observed_at DESC);
CREATE INDEX IF NOT EXISTS ix_asset_observation_evidence
  ON research_asset_observation(evidence_ref);

-- Technology profile entries must cite the observation that established them.
CREATE TABLE IF NOT EXISTS research_asset_technology (
  technology_id     TEXT PRIMARY KEY,
  asset_id          TEXT NOT NULL REFERENCES research_asset(asset_id),
  endpoint_id       TEXT REFERENCES research_asset_endpoint(endpoint_id),
  observation_id    TEXT NOT NULL REFERENCES research_asset_observation(observation_id),
  technology        TEXT NOT NULL CHECK (length(trim(technology)) > 0),
  version           TEXT NOT NULL DEFAULT '',
  category          TEXT NOT NULL DEFAULT 'application',
  confidence        REAL NOT NULL CHECK (confidence >= 0 AND confidence <= 1),
  evidence_ref      TEXT NOT NULL DEFAULT '',
  first_seen_at     TEXT NOT NULL,
  last_seen_at      TEXT NOT NULL,
  UNIQUE (asset_id, endpoint_id, technology, version, observation_id)
);
CREATE INDEX IF NOT EXISTS ix_asset_technology_asset
  ON research_asset_technology(asset_id, technology);

CREATE TABLE IF NOT EXISTS research_project_asset (
  project_id       TEXT NOT NULL REFERENCES research_project(project_id),
  asset_id         TEXT NOT NULL REFERENCES research_asset(asset_id),
  relation_role    TEXT NOT NULL DEFAULT 'in_scope'
                   CHECK (relation_role IN ('in_scope','out_of_scope','watchlist')),
  linked_at        TEXT NOT NULL,
  linked_by        TEXT NOT NULL DEFAULT 'owner-local',
  PRIMARY KEY (project_id, asset_id)
);
CREATE INDEX IF NOT EXISTS ix_project_asset_asset ON research_project_asset(asset_id);

CREATE TABLE IF NOT EXISTS research_task_asset (
  task_id          TEXT NOT NULL REFERENCES research_task(task_id),
  asset_id         TEXT NOT NULL REFERENCES research_asset(asset_id),
  relation_role    TEXT NOT NULL DEFAULT 'target'
                   CHECK (relation_role IN ('target','observed','related')),
  linked_at        TEXT NOT NULL,
  PRIMARY KEY (task_id, asset_id)
);
CREATE INDEX IF NOT EXISTS ix_task_asset_asset ON research_task_asset(asset_id);

-- Protect observation provenance from accidental in-place mutation.
CREATE TRIGGER IF NOT EXISTS trg_asset_observation_no_update
BEFORE UPDATE ON research_asset_observation
BEGIN
  SELECT RAISE(ABORT, 'research_asset_observation is append-only');
END;
CREATE TRIGGER IF NOT EXISTS trg_asset_observation_no_delete
BEFORE DELETE ON research_asset_observation
BEGIN
  SELECT RAISE(ABORT, 'research_asset_observation is append-only');
END;

INSERT OR IGNORE INTO schema_version
  (version, name, description, applied_at, applied_by)
VALUES
  (31, 'V1 additive research projects and assets',
   'Independent workspace-scoped Projects and Research Assets; explicit Project/Task/Asset relations; endpoint and evidence-backed technology profile; append-only observation provenance. Does not duplicate RTIS Findings or document_assets.',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '031_research_projects_assets.sql');

PRAGMA user_version = 31;
