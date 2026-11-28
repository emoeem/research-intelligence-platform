# RIP Production Baseline

> Freeze date: 2026-11-28
> Status: P0 complete; P1 infrastructure and blocker governance in progress
> Git baseline: `main` at `3055d7aa`, with existing dirty work preserved

## 1. Decision

RIP is **not Production Ready**. It has a broad implemented workspace and
substantial local verification, but the production parser corpus, human
retrieval labels, real AI-provider E2E, formal backup/restore policy, production
TLS/cutover, and authorized POC/Nuclei execution gates remain open.

Non-essential feature expansion is frozen until P0 blockers are assigned and
P1-P2 recovery work begins. This document does not authorize commit, push,
migration, backup replacement, or production cutover.

## 2. Repository and worktree state

- Branch: `main`
- Remote tracking: `origin/main`
- Existing dirty state: 28 modified files and 9 untracked paths
- Worktree changes are pre-existing and must not be reset, stashed, deleted,
  committed, or overwritten.
- Key current work is in ingestion, MCP, PostgreSQL migrations V0015-V0018,
  parser/embedding tests, API, and knowledge-governance documentation.

## 3. Implementation inventory

| Area | Baseline status | Evidence |
|---|---|---|
| Rust workspace | Implemented; 25 crates under workspace | 1,485 workspace tests pass locally at the last handoff |
| SQLite application data | Implemented; multi-migration SQLite path | 33 SQL migration files; main/auxiliary dev databases exist |
| PostgreSQL data plane | Implemented; disposable integration evidence | 18 migrations; formal production backup/cutover open |
| MCP | Implemented with rmcp migration and parity tests | MCP protocol/dispatch/parity evidence exists |
| Tool Engine/Registry | Implemented; native/Python/Podman paths | 53 tool manifests; execution/audit tests exist |
| Workflow Engine | Implemented; persistent runs/checkpoints/control | 6 workflow migration families and local selftests exist |
| Document parsing/IR | Partial implementation; real corpus blocked | Synthetic IR tests pass; pinned parser image absent |
| Knowledge ingestion/governance | Partial implementation | Raw/Candidate/Published model and review batch implemented; publication/refresh unverified |
| Search | Implemented with local/PG/hybrid projections | Human labels and independent metrics absent |
| API | Implemented read/write route surface | 46 static API routes plus dynamic read/write paths |
| Web | Implemented React workbench and visual gate | 85 visual/accessibility tests pass; format drift is unrelated |
| Backup/restore | Implemented SQLite mechanism; formal operation open | Local roundtrip evidence; PostgreSQL policy and independent restore open |

## 4. Verification-state taxonomy

- **Implemented**: code and persisted model exist.
- **Local test passed**: an isolated or synthetic test passed.
- **Real environment passed**: an operator verified against real resources.
- **Production accepted**: an accountable owner signed the real operational gate.

No current result may be promoted across categories automatically.

## 5. P0 blockers

See `PRODUCTION-BLOCKERS.md`. The first blocking sequence is parser image/corpus,
human semantic and retrieval labels, formal backups/restores, real-provider E2E,
production cutover/TLS, interruption recovery, and authorized security-assets
execution.

P0 is formally closed as an audit phase: the baseline and blocker register are
complete, and non-essential expansion is frozen. P0 closure does not change
the `Production Ready = false` decision.

## Scope correction: deployment requirements

Initial platform operation is not gated on:

- a 40+ real-corpus acceptance suite;
- human relevance/duplicate annotation before ingestion;
- an external DeepSeek/OpenAI-compatible credential;
- a public domain, certificate or reverse proxy;
- formal offsite PostgreSQL backup, RTO or RPO.

These items are optional or later-scale requirements. Automatic ingestion,
local parsing, Candidate generation, local search, evidence logging and
workflow recovery can operate without them. A formal public or production
service requires the deployment controls applicable to that service.

## Automation-first acceptance policy

- Document acquisition, parsing, hashing, deduplication, AI extraction,
  candidate creation and AI-assisted ranking run without waiting for human
  annotation.
- Human annotation is a feedback and repair mechanism, not a batch ingestion
  prerequisite.
- AI-assisted metrics are characterization metrics only. They are never
  promoted to human Ground Truth.
- Human review is required for high-risk publication/conflict cases according
  to the existing Candidate/Published and review policies, but it does not
  block normal candidate ingestion.
- Provider credentials, production DNS/certificate, reverse proxy and backup
  policy are deployment configuration, not per-result human work.

## 6. P0 exit condition

P0 exits only when every blocker has an ID, severity, reproduction method,
owner field, impact, fix, test, evidence, and state in `PRODUCTION-BLOCKERS.md`
and in the repository session task ledger.
