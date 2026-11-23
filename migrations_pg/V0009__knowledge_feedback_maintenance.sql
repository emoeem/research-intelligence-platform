-- RIP3 Phase 7: research feedback, provenance, upstream monitoring and maintenance ledger.
-- Additive only: reports / Report IR v1 / RTIS source-of-truth remain unchanged.
-- Findings are immutable observations/projections; RTIS owns vulnerability facts.

CREATE TABLE research_tasks (
    task_id TEXT PRIMARY KEY,
    latest_report_id TEXT REFERENCES reports(report_id),
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    report_count BIGINT NOT NULL DEFAULT 0 CHECK (report_count >= 0)
);
CREATE INDEX idx_research_tasks_latest_report ON research_tasks(latest_report_id);

CREATE TABLE research_task_reports (
    task_id TEXT NOT NULL REFERENCES research_tasks(task_id),
    report_id TEXT NOT NULL REFERENCES reports(report_id),
    observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (task_id, report_id)
);
CREATE INDEX idx_task_reports_report ON research_task_reports(report_id);

CREATE TABLE findings (
    task_id TEXT NOT NULL REFERENCES research_tasks(task_id),
    finding_id TEXT NOT NULL,
    rtis_ref TEXT,
    title TEXT NOT NULL,
    severity TEXT NOT NULL,
    result_state TEXT NOT NULL,
    finding_snapshot JSONB NOT NULL,
    evidence_refs JSONB NOT NULL DEFAULT '[]'::jsonb,
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    observation_count BIGINT NOT NULL DEFAULT 1 CHECK (observation_count >= 1),
    PRIMARY KEY (task_id, finding_id),
    CHECK (jsonb_typeof(evidence_refs) = 'array')
);
CREATE INDEX idx_findings_rtis_ref ON findings(rtis_ref) WHERE rtis_ref IS NOT NULL;

CREATE TABLE finding_observations (
    report_id TEXT NOT NULL REFERENCES reports(report_id),
    task_id TEXT NOT NULL,
    finding_id TEXT NOT NULL,
    finding_snapshot JSONB NOT NULL,
    evidence_refs JSONB NOT NULL DEFAULT '[]'::jsonb,
    observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (report_id, finding_id),
    CHECK (jsonb_typeof(evidence_refs) = 'array')
);
CREATE INDEX idx_finding_observations_identity ON finding_observations(task_id, finding_id, observed_at DESC);

CREATE TABLE knowledge_feedback_candidates (
    candidate_id TEXT PRIMARY KEY,
    report_id TEXT NOT NULL REFERENCES reports(report_id),
    task_id TEXT NOT NULL,
    finding_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL DEFAULT 'default',
    candidate_hash TEXT NOT NULL,
    candidate_snapshot JSONB NOT NULL,
    evidence_refs JSONB NOT NULL,
    review_status TEXT NOT NULL DEFAULT 'PENDING_REVIEW'
        CHECK (review_status IN ('PENDING_REVIEW','DUPLICATE_REVIEW','PROMOTED','REJECTED')),
    knowledge_id TEXT REFERENCES knowledge_cards(knowledge_id),
    duplicate_of TEXT REFERENCES knowledge_cards(knowledge_id),
    reviewer TEXT,
    decision_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (report_id, finding_id),
    CHECK (jsonb_typeof(evidence_refs) = 'array' AND jsonb_array_length(evidence_refs) > 0)
);
CREATE INDEX idx_feedback_review ON knowledge_feedback_candidates(workspace_id, review_status, created_at);
CREATE INDEX idx_feedback_hash ON knowledge_feedback_candidates(workspace_id, candidate_hash);

CREATE TABLE knowledge_evidence_provenance (
    provenance_id TEXT PRIMARY KEY,
    candidate_id TEXT NOT NULL REFERENCES knowledge_feedback_candidates(candidate_id),
    report_id TEXT NOT NULL REFERENCES reports(report_id),
    task_id TEXT NOT NULL,
    finding_id TEXT NOT NULL,
    evidence_id TEXT NOT NULL,
    artifact_id TEXT NOT NULL,
    tool_run_id TEXT NOT NULL,
    trace_id TEXT NOT NULL,
    evidence_uri TEXT NOT NULL,
    content_hash TEXT NOT NULL CHECK (content_hash ~ '^[0-9a-f]{64}$'),
    evidence_type TEXT,
    captured_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (candidate_id, evidence_id, content_hash)
);
CREATE INDEX idx_evidence_provenance_finding ON knowledge_evidence_provenance(task_id, finding_id);
CREATE INDEX idx_evidence_provenance_trace ON knowledge_evidence_provenance(trace_id);

CREATE TABLE source_monitor_state (
    source_id TEXT PRIMARY KEY REFERENCES source_records(source_id),
    canonical_url TEXT NOT NULL,
    etag TEXT,
    last_modified TEXT,
    last_checked_at TIMESTAMPTZ,
    http_status INTEGER,
    observed_hash TEXT,
    previous_hash TEXT,
    monitor_state TEXT NOT NULL DEFAULT 'PENDING'
        CHECK (monitor_state IN ('PENDING','UNCHANGED','CHANGED','MISSING','ERROR','SKIPPED')),
    next_check_at TIMESTAMPTZ,
    last_error TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_source_monitor_due ON source_monitor_state(next_check_at, monitor_state);

CREATE TABLE source_monitor_events (
    event_id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL REFERENCES source_records(source_id),
    checked_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    http_status INTEGER,
    etag TEXT,
    last_modified TEXT,
    content_hash TEXT,
    previous_hash TEXT,
    outcome TEXT NOT NULL CHECK (outcome IN ('UNCHANGED','CHANGED','MISSING','ERROR','SKIPPED')),
    detail TEXT NOT NULL DEFAULT ''
);
CREATE INDEX idx_source_monitor_events_source ON source_monitor_events(source_id, checked_at DESC);

CREATE TABLE maintenance_runs (
    run_id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ,
    status TEXT NOT NULL DEFAULT 'RUNNING' CHECK (status IN ('RUNNING','COMPLETED','FAILED')),
    summary JSONB NOT NULL DEFAULT '{}'::jsonb
);
CREATE INDEX idx_maintenance_runs_workspace ON maintenance_runs(workspace_id, started_at DESC);

CREATE TABLE maintenance_findings (
    finding_row_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES maintenance_runs(run_id),
    workspace_id TEXT NOT NULL,
    subject_type TEXT NOT NULL CHECK (subject_type IN ('knowledge_card','source_record','document','finding','evidence')),
    subject_id TEXT NOT NULL,
    check_code TEXT NOT NULL,
    severity TEXT NOT NULL CHECK (severity IN ('INFO','WARN','ERROR')),
    detail JSONB NOT NULL,
    detected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at TIMESTAMPTZ
);
CREATE INDEX idx_maintenance_findings_run ON maintenance_findings(run_id, severity, check_code);
CREATE INDEX idx_maintenance_findings_subject ON maintenance_findings(subject_type, subject_id, detected_at DESC);
