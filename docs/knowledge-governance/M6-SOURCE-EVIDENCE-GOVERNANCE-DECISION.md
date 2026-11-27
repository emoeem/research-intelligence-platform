# M6 — Source trust and evidence governance decision

Status: **Decision recorded and implemented; see [M6 closeout](./M6-CLOSEOUT-AND-REMAINING-ISSUES.md)**

## Current-state findings

- Plugin knowledge entries already retain plugin/version/corpus identity, source type/URL/path/commit, content and metadata hashes, license declaration, rights policy, review state, risk flags and first/last-seen timestamps. Ingestion starts `DISCOVERED`, changed content does not overwrite the old versioned row, upstream removal is retained as `upstream_missing`, and Search/Context only expose approved material.
- The controlled SQLite review API currently updates review state and emits a best-effort audit event, but does not require a reviewer rationale, does not persist old/new decisions in the same transaction, and can report success when a key does not exist.
- The M1 PostgreSQL source registry retains source ID/version/URI/commit/hash/observation time and current/missing state, but has no assessed source-trust class, license assessment or source-review state. Relation approval currently requires only a current hashed Evidence source; a newly registered, unreviewed Evidence reference could therefore satisfy that gate.
- Existing `SourceTrust` (`external_import`, `rtis_snapshot`, `agent_derived`, `manual`) describes record origin, not source credibility. Plugin `TrustLevel` (`official`, `community`, `local`, `remote`) primarily governs installation confirmation and is not evidence that a specific source claim is true. Evidence levels K0–K4 and relation confidence are also distinct from source trust.
- Authority boundary: plugin KSE review remains in its SQLite authority; cross-authority identity/source assessments and relation Evidence remain in the M1 PostgreSQL authority. No source body or evidence payload is copied into the graph.

## Decision

1. Keep raw source snapshots immutable. Add a separate current assessment per source snapshot, with an append-only decision history.
2. Keep four axes separate:
   - source state (`CURRENT`/missing/deprecated/unverified);
   - review state (`UNREVIEWED`/`APPROVED`/`REJECTED`/`QUARANTINED`/`DEPRECATED`);
   - qualitative trust class (`UNASSESSED`/`OFFICIAL_PRIMARY`/`MAINTAINER_PRIMARY`/`CURATED_SECONDARY`/`COMMUNITY`/`UNKNOWN`);
   - declared license expression and license assessment (`UNKNOWN`/`DECLARED`/`VERIFIED`/`RESTRICTED`/`UNLICENSED`).
   No numeric trust score is introduced. Trust classification is a review assertion, not a vulnerability or detection verdict.
3. Default every existing/new PostgreSQL source assessment to `UNREVIEWED` and `UNASSESSED`; historical KSE review states are preserved as-is. Do not auto-approve based on a plugin manifest, source URL, hash, parser result, relation confidence or source category.
4. An explicit source approval must include a reviewer, non-empty reason and an assessed trust class. Evidence-source approval additionally requires a current, non-empty URI and a well-formed content SHA-256. Relation approval must require at least one linked Evidence source whose current source assessment is approved; no change to the rule that candidates may exist without Evidence.
5. Add a durable, transactional KSE source review log alongside the existing global audit event. Require a reason; fail the full operation if any requested entry is missing; keep the API restricted to `knowledge_source_entry` (RTIS `knowledge_ingest` remains outside this review surface).
6. License assessment is not a license grant. A source may be provenance-reviewed while its license remains `UNKNOWN`; rights policy still controls what content can be persisted or exposed.

## Candidate comparison and mature-library review

| Candidate | Assessment | Decision |
|---|---|---|
| Put assessment fields directly on immutable source snapshots | Would mix source facts with mutable reviewer conclusions and erase prior assessments | Reject |
| Add a second audit/event service | Duplicates the existing AuditEngine and splits authority | Reject |
| Use the existing PostgreSQL/Rusqlite stores, refinery/SQLite migrations, typed Rust vocabularies and AuditEngine, with an append-only review log in each owning database | Matches current transaction/migration boundaries; no new protocol or serialization library is needed | Adopt |
| Add a numeric reputation/trust scoring library | No verified, domain-specific calibration set exists; a number would imply unsupported precision | Reject |

No dependency is added. Existing PostgreSQL and SQLite drivers, `refinery`, the project migration runner and AuditEngine are reused.

## Migration and rollback boundary

- PostgreSQL changes are an additive V0012 migration. Existing assessments are inserted as unreviewed/unassessed; old source snapshots and relation statuses are not rewritten or approved.
- SQLite changes are additive migration 033, creating a review log only. It does not update KSE rows or touch `migrations/dev-p6.db`.
- Verify forward migration, idempotency, append-only history, invalid input rejection, failed-batch atomicity, source-state/evidence approval gates and pre-migration backup restore on disposable databases.
- For any persistent deployment, make and verify a pre-migration database backup first; rollback is restore into a clean database. No down migration will erase review history.

## M6 implementation outcome

- PostgreSQL V0012 and SQLite migration 033 are additive and were exercised only on disposable databases.
- Source assessments and review history are available through typed PostgreSQL read/write functions; KSE decisions are exact-version scoped and durable.
- Targeted integration tests, Clippy and the V032 backup/restore rollback drill passed. No CI result is claimed.
- The review-center UI and HTTP integration remain scheduled for M5; remaining semantic/coverage limits are recorded in the closeout report.
