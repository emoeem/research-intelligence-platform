# M6 — Source trust and evidence governance closeout

Status: **Complete for the scoped governance layer**

## Delivered

- Added PostgreSQL V0012 source assessments, defaulting both existing and new source snapshots to `UNREVIEWED` / `UNASSESSED`. Immutable source snapshots remain separate from mutable trust, license and review decisions.
- Added transactional, append-only PostgreSQL source-review history and read APIs for the current assessment and prior decisions.
- Evidence-source approval requires a current snapshot, non-empty canonical URI and SHA-256; every approval requires a reviewer, reason and assessed trust class.
- Relation approval now requires linked Evidence whose source assessment is approved. Withdrawing that approval invalidates linked approved relations in the same transaction. Source snapshots and evidence payloads are not rewritten or copied.
- Added SQLite migration 033 with a durable KSE review log and immutable-history triggers. Existing KSE review states are unchanged.
- KSE review now scopes decisions to an exact plugin version, requires reviewer and reason, records state changes transactionally, and rolls back an entire batch if any key is missing. CLI review requires `--version` and `--reason`. The existing global AuditEngine event remains best-effort; the entry-linked review log is the durable decision record.
- Updated direct SQLite chain-head assertions from 032 to 033 and added a backup/restore test for rollback to a clean pre-migration database.
- No dependencies were added. Existing PostgreSQL, rusqlite, migration, typed-domain and AuditEngine components are reused.

## Verification

All commands were run locally against disposable PostgreSQL databases, in-memory SQLite databases, or temporary database copies. No CI result is implied.

| Gate | Result |
|---|---:|
| PostgreSQL `knowledge_governance` and `unified_knowledge_relations` tests | 8/8 passed |
| SQLite KSE `sk1_extended_ingest`, `sk2_source_ingest`, `sk3_source_sync` tests | 14/14 passed |
| `cargo test --offline -p domain` | 82/82 passed |
| `cargo test --offline -p database --test backup_restore --test migrations_runtime_node` | 7/7 passed, including V033 forward migration and restore to V032 |
| `cargo test --offline -p agent-engine --test migration_idempotent` | 1/1 passed |
| `cargo test --offline -p wsw-runtime --test exchange_persist --test intercept_queue` | 19/19 passed |
| Scoped Clippy for `domain`, `intel-db`, `plugin-runtime` with `--all-targets -- -D warnings` | Passed |
| `git diff --check` | Passed |

The rollback drill backed up a disposable V032 SQLite database containing a `DISCOVERED` KSE row, applied V033, verified the log table and migration idempotency, then restored the archive over a disposable migrated copy. The restored database reported `user_version=32`, `integrity=ok`, retained the row as `DISCOVERED`, and did not contain the V033 review-log table.

PostgreSQL migration tests verified legacy identity/source backfill without approving any source, source-review history reads, append-only enforcement, evidence approval gates and relation invalidation when source approval is withdrawn.

## Remaining issues and boundaries

- Trust classes are qualitative human assessments, not calibrated reputation scores. No scoring is inferred from source type, URL, hash or plugin trust level.
- License expression and assessment are retained separately; M6 does not validate SPDX syntax or grant reuse rights. `UNKNOWN` remains allowed and rights policy continues to control persistence/exposure.
- The PostgreSQL review/read functions are persistence-layer APIs. The source-review center, filters, pagination, user feedback and evidence-tracing UI are deferred to M5.
- The SQLite review-history API is available to the controlled runtime, but a general review-history HTTP endpoint is not part of M6.
- The separate KSE and PostgreSQL authorities remain intentionally separate; no cross-database transaction is claimed.
- Full workspace tests, full formatting, CI, frontend behavior and later-phase acceptance remain for M7. No POC or Nuclei template was executed, and no real target was contacted.

## Data safety

Migrations were tested only on disposable PostgreSQL schemas/databases, in-memory SQLite fixtures and temporary copies. `migrations/dev-p6.db`, `integrations/rtis/knowledge.db`, platform databases and production data were not migrated or written by this phase. Rollback is restore-based; no destructive down migration is provided.
