-- RIP3 Phase 7 follow-up: keep research task / Finding / RTIS checks workspace-scoped.
-- Additive only; existing task rows are assigned to the legacy default workspace.
ALTER TABLE research_tasks
    ADD COLUMN workspace_id TEXT NOT NULL DEFAULT 'default';
CREATE INDEX idx_research_tasks_workspace ON research_tasks(workspace_id, task_id);
