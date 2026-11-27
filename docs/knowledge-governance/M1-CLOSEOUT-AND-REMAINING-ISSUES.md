# M1 — Unified entity and relation model closeout

Status: **Complete — schema, domain model and persistence verified**

## Delivered

- Added typed identity references for KnowledgeCards, vulnerabilities, detection templates, POCs, security tools, technical documents and Evidence.
- Kept authoritative facts in their existing stores. The PostgreSQL graph stores identity/source pointers and relationship assertions rather than copied POC, template, document or Evidence bodies.
- Extended the existing `knowledge_relations` authority with the requested relation vocabulary, endpoint references, relation review status, validity interval and append-only review log.
- Added versioned source pointers with source system, source ID, version, URI, commit, SHA-256, observation time and current/missing/deprecated/unverified state.
- Kept confidence separate from review/verification. New and historical relations remain `CANDIDATE`; approval requires a current Evidence source snapshot with a URI and SHA-256 and writes a reviewer decision event.
- Integrated KnowledgeCard create/relation/detail paths with the unified registry without replacing their existing storage or changing the public relation CLI flow.

Architecture and migration reasoning are recorded in [M1-UNIFIED-ENTITY-RELATION-ADR.md](./M1-UNIFIED-ENTITY-RELATION-ADR.md).

## Migration, backup and rollback evidence

`V0011__unified_knowledge_entities_and_relations.sql` was applied only to disposable PostgreSQL databases and an isolated schema. It backfills KnowledgeCard identities before changing endpoint foreign keys. A migration test verified preservation of the old relation ID, endpoints, relation type, JSON evidence, source string and confidence; the old edge remains a candidate and receives no fabricated review event.

A separate disposable-database rollback exercise used a custom-format `pg_dump` at V10, applied V0011, and restored the snapshot into a clean database. Restore verification found:

- maximum restored migration version: `10`
- original relation row retained: `1`
- V0011 identity registry present in the restored database: `no`
- relation ID/endpoints/type/source/confidence/JSON evidence match the pre-migration snapshot

The test archive was temporary, isolated and not retained in the repository. No production or persistent development database was modified.

## Validation

| Gate | Result |
|---|---|
| `cargo test --offline -p domain` | PASS — 82 tests |
| PostgreSQL `knowledge_governance` integration tests | PASS — 5/5 |
| PostgreSQL `unified_knowledge_relations` integration tests | PASS — 3/3 |
| `cargo clippy --offline -p domain -p intel-db --all-targets -- -D warnings` | PASS |
| Rust formatting for touched Rust files | PASS |
| `git diff --check` | PASS |
| Disposable PostgreSQL V10 backup → V11 migration → pre-V11 restore | PASS |

These are local results. No CI run was started for this uncommitted work, so no remote CI result is claimed.

## Remaining issues and boundaries

- Cross-store IDs are registered only when an adapter or caller supplies them; this phase does not bulk-import KSE, RTIS POCs, Nuclei templates or research Evidence into the graph.
- The identity registry cannot enforce foreign keys into separate authorities. Resolution adapters must report missing, stale or version-mismatched source pointers rather than invent or delete entities.
- M6 must define trust/evidence scoring and review policy. A registered source, hash, relation or confidence value is not proof that a vulnerability is exploitable or a detection is effective.
- M2/M3/M4/M5 will supply template lifecycle, POC evidence/retest, search/identity resolution and user-facing review/trace capabilities.
- No POC, Nuclei template or real target was executed or contacted.
