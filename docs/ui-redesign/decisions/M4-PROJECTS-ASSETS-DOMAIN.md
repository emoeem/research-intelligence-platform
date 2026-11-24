# M4 Domain Decision — Projects and Research Assets

Date: 2026-11-24
Status: Accepted from the user's explicit decision; implementation in progress.

## Product decisions

- Projects are an independent domain, not a view over Research Tasks.
- Research Assets are an independent domain, not a view over Knowledge Documents or document extraction assets.
- M7 visual acceptance uses the supplied RIP Figma screenshots as the reference standard.
- M8 is staged: prepare and validate a parallel SPA entry, retain the old entry, and wait for explicit user acceptance before switching the default root.
- The supplied Figma file is the canonical design source; four exported screenshots are currently available in this conversation.

## Domain boundaries

### Project

A Project is a durable research container with its own identity, workspace, name, slug, objective, summary, lifecycle status, priority, scope, tags, owner, and timestamps.

Project-to-Task is an explicit many-to-many relation. The RTIS-derived `research_task` row remains an imported mirror and is not mutated into a Project. Project membership is represented by a separate relation table.

Project-to-Asset is an explicit many-to-many relation with an in-scope / out-of-scope / watchlist role. Task-to-Asset is also explicit.

### Research Asset

A Research Asset is a canonical research target identity, distinct from:

- `documents` and `document_assets` (document/CAS extraction domain);
- Knowledge Cards and Knowledge Sources;
- RTIS Finding facts;
- Verification Evidence snapshots.

Initial identity types: domain, IP, URL, service, repository, application, other.

An Asset can own normalized Endpoints. Discovery and fingerprinting are represented as append-only Observations with source type, source reference, evidence reference, observation time, SHA-256 content hash, and immutable JSON payload.

Technology Profile entries must reference the Observation that established them. A hostname or URL string alone must never create a technology claim.

### Storage and service boundary

Projects, Project/Task relations, Research Asset identities, Endpoints, Observations, Technology Profile entries, and Project/Task/Asset relations are operational research-workspace state. They belong on the SQLite 020+ operational migration chain, alongside Research Plans and Evidence, rather than adding tables to the PostgreSQL intelligence-core migration chain.

The HTTP server must preserve its existing boundary: Python dispatches domain commands to a Rust CLI subprocess; Python does not gain direct SQL write paths. Read APIs may use the existing server connection conventions only if they preserve workspace scoping and data provenance.

The existing RTIS task mirror is read-only. No migration backfills Project records by guessing from task titles or target strings. No Asset or Technology Profile rows are fabricated from unverified input.

## Initial persistence contract

Migration `031_research_projects_assets.sql` introduces:

- `research_project`
- `research_project_task`
- `research_asset`
- `research_asset_endpoint`
- `research_asset_observation` (append-only triggers)
- `research_asset_technology`
- `research_project_asset`
- `research_task_asset`

The migration does not alter or rewrite existing research tasks, RTIS records, document assets, or verification evidence.

## API contract direction

- `GET /api/projects?workspace_id=&status=&q=&limit=&cursor=`
- `POST /api/projects`
- `GET /api/projects/{project_id}`
- `PATCH /api/projects/{project_id}`
- `POST /api/projects/{project_id}/tasks`
- `DELETE /api/projects/{project_id}/tasks/{task_id}`
- `POST /api/projects/{project_id}/assets`
- `DELETE /api/projects/{project_id}/assets/{asset_id}`
- `GET /api/research/assets?workspace_id=&type=&status=&q=&project_id=&task_id=&limit=&cursor=`
- `POST /api/research/assets` (manual import must identify its source as user_import/manual)
- `GET /api/research/assets/{asset_id}` (asset + endpoints + observations + evidence-backed technology profile + related projects/tasks)
- `POST /api/research/assets/{asset_id}/observations` (append-only)
- `POST /api/research/assets/{asset_id}/endpoints`

Writes require the existing Bearer authentication policy, a real authenticated context, request validation, and audit records. Destructive deletion is not part of the first cut; lifecycle/archive operations are preferred.

## Visual reference inventory

The four supplied desktop screenshots establish these canonical patterns:

1. Tools & agents: dense registry list on the left; Agent Runtime and Execution Guardrails on the right.
2. Report studio: document outline on the left; evidence-led report canvas on the right.
3. Research investigation: objective/status header, task board, evidence ledger, and technical findings.
4. Workflow builder: horizontal connected stages and a selected-node configuration panel.

All four use the same fixed navigation and top workspace bar, dark navy surfaces, subtle borders, restrained violet primary actions, and semantic green/amber/neutral statuses.

The screenshots do not include Projects or Research Assets pages, light theme, or mobile layouts. Those views must be checked against the existing approved RIP page specifications/prototype and shared design tokens; they must not be invented from the four screenshots alone.

## Non-goals and safety constraints

- No duplication of RTIS Finding facts or Knowledge Document content.
- No inferred technologies without an observation and evidence reference.
- No direct Python SQL writes.
- No migration or backfill against the production database during implementation.
- No disabling or replacing the old static UI before the M8 gate and explicit acceptance.
- No Git reset, clean, stash, checkout, commit, or push unless explicitly requested.
