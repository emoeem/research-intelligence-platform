# M1 ADR — Unified knowledge identity and relationship model

Status: **Implemented and locally verified**

## Context

RIP currently has multiple authorities with non-overlapping responsibilities:

- Plugin KSE owns ingested source-entry versions and their content/provenance metadata.
- RTIS remains authoritative for POC/research facts.
- `intel-db` owns governed KnowledgeCards and the existing KnowledgeCard-only `knowledge_relations`.
- Research Evidence and Context Citation Evidence retain immutable evidence snapshots in their existing domains.

Copying those records into another general-purpose knowledge store would create conflicting facts, double review state, and fragile synchronization. The existing PostgreSQL relation table is also insufficient by itself: its endpoints are KnowledgeCard foreign keys, its types omit several requested relations, and it has no relation-level review lifecycle or effective dates.

## Decision

Extend the existing `intel-db` relation authority into a cross-authority identity-reference and relationship layer. Store **identity pointers and relationship assertions only**; keep entity facts and evidence bodies in their existing authoritative stores.

1. Add a `knowledge_entity_refs` registry for stable typed identity keys, and `knowledge_entity_sources` for one-to-many versioned source pointers (source system, external ID, version, URI/commit/hash and observation time). These tables do not duplicate titles, descriptions, POC content, template YAML, or evidence payloads.
2. Retarget the existing `knowledge_relations.src_id/dst_id` foreign keys from KnowledgeCards to the identity registry. Backfill all existing KnowledgeCards under their current IDs before changing constraints; keep those IDs and relation rows unchanged.
3. Keep the existing relation table as the single graph edge store. Add the requested relation types, relation review status, effective-from/to timestamps, and an append-only relation review log. Historical edges become `CANDIDATE`, not verified.
4. Add normalized relation-to-Evidence identity links. An edge may be proposed without evidence, but it cannot be reviewed as `APPROVED` unless at least one typed Evidence reference with a usable provenance/hash is linked. Confidence is a separate estimate, never a substitute for verification.
5. Add Rust domain types and `intel-db` APIs for stable typed references and relation review. Existing KnowledgeCard creation registers its own identity reference transactionally; existing relation CLI behavior remains valid and produces a candidate edge.
6. Resolution adapters may verify pointers against their authoritative source read-only. They must preserve unresolved/stale references explicitly rather than fabricate an entity or silently delete a relation.

Storage fields are versioned source pointers and identity keys, not a second copy of the knowledge graph's source facts. A relation's endpoint type is resolved through its registered identity reference.

## Required relation vocabulary

Retain all existing types for backward compatibility. Add:

- `EXPLOITS`
- `AFFECTS`
- `FIXED_BY`
- `ANALYZES`
- `MITIGATED_BY`
- `USES_TOOL`
- `VALIDATES`

`FIXES` remains distinct from `FIXED_BY`; relation direction is explicit and must not be silently inverted. `DETECTS` and `DERIVED_FROM` already exist.

## Entity identity types

The registry supports `knowledge_card` plus the six requested classes: `vulnerability`, `detection_template`, `poc`, `security_tool`, `technical_document`, and `evidence`. Canonical keys are domain-specific (for example a normalized CVE/GHSA identifier, stable template ID, RTIS POC ID, tool registry ID, canonical document source key, or evidence source namespace + ID). Version-specific source pointers remain separate from stable logical identity.

## Candidate comparison and mature-library review

| Candidate | Assessment | Decision |
|---|---|---|
| Duplicate graph/KG database or an external graph library | Would introduce a second authority and a synchronization boundary; does not encode RIP's source/evidence/review/effective-time rules for us | Reject for this requirement |
| Replace KnowledgeCards or RTIS facts with a new entity store | Violates existing authority boundaries and risks losing historical review/evidence semantics | Reject |
| Extend current PostgreSQL `knowledge_relations` with an identity-reference registry | Reuses the established `intel-db`, PostgreSQL transactions/foreign keys/indexes, existing Rust relation type and `refinery` migrations; supports cross-source pointers without copying facts | Adopt |

This is domain-specific modeling, not a new general-purpose graph engine. Existing maintained PostgreSQL, `postgres` crate and `refinery` migration system are sufficient; no new dependency is proposed. Before any future dependency change, re-evaluate maintenance, license, dependency tree/MSRV/features, advisories and vendored offline build impact.

## Migration, backup and rollback

- Add one additive PostgreSQL migration after V0010; run it only against in-memory/isolated test databases during development.
- Migration order: create the identity registry/source tables; backfill current KnowledgeCards and their source pointers; create relation-evidence and append-only review-log tables; add review/effective-time columns; replace relation-type constraint; retarget the two endpoint foreign keys; add indexes and consistency checks.
- Verify row counts, IDs, JSON evidence, source fields and existing relation semantics before and after. The initial relation review state is `CANDIDATE`; do not auto-approve pre-existing edges.
- For any later persistent development deployment: produce a verified `pg_dump --format=custom` backup and a disposable restore; test migration on that restore; capture pre/post schema and row-count manifests; rollback by restoring the verified pre-migration backup into a clean database. Do not promise down-migration data preservation.
- Do not alter SQLite migration files or the protected production databases for this graph model.

## Acceptance tests

1. Migration backfills KnowledgeCard identity refs and preserves all old card/relation IDs, relation JSON, confidence/source values and review data.
2. A `vulnerability → detection_template` or `poc → security_tool` relation can be represented without copying either entity body.
3. Unknown entity/relation types, missing endpoint refs, out-of-range confidence, invalid time ranges, and invalid evidence refs fail explicitly.
4. New and historical relations remain non-approved until reviewed; approval without evidence fails; valid approval writes an append-only reviewer/reason/timestamp event.
5. Validity dates and entity/source version pointers survive round-trip queries.
6. Existing KnowledgeCard-only callers and contradictory-relation safeguards continue to pass.
7. Upgrade and backup-restore rollback are exercised on a disposable PostgreSQL database; no production DB is touched.

## Risks and open follow-up

- The registry cannot provide cross-store referential integrity by SQL foreign key. Registration/resolution adapters must validate source identity and report stale/missing pointers.
- Some legacy relations may have empty evidence. They remain candidates and need future human evidence review; do not infer support from their existence or confidence.
- M6 will define source trust/evidence policies and approval thresholds using this structure. M5 will expose typed endpoints, relation review history, evidence pointers and validity state.

## Implementation outcome

- Applied `V0011__unified_knowledge_entities_and_relations.sql` only to disposable PostgreSQL databases/schemas. The migration backfills KnowledgeCard identities before retargeting foreign keys, preserves endpoint and relation IDs/fields, and initializes every historical edge as `CANDIDATE`.
- Added Rust entity/relation vocabularies and `intel-db::knowledge_graph` write/review APIs. Existing KnowledgeCard creation and relation calls use the same registry/edge table transactionally; detail reads expose endpoint types, review state, validity and linked source snapshots.
- On a fresh database, the migration backfill test preserved the relation ID, endpoints, relation type, JSON evidence, relation source and confidence; it created two identity/source registrations, kept the edge `CANDIDATE`, and created no review event.
- Backup/restore rollback was exercised with a disposable database: `pg_dump --format=custom` at V10, apply V0011, verify the migrated edge and identity registrations, then restore the V10 snapshot to a clean database. The restored database reported migration version 10, retained the relation row and had no V0011 identity registry. The backup was a temporary test artifact and is not retained as repository data.
- Validation: 8/8 PostgreSQL governance integration tests passed serially on a fresh disposable database; 82 domain tests passed; scoped Clippy with `-D warnings` passed. No PoC or Nuclei template was executed and no target was contacted.

Remaining implementation is intentionally deferred to later ordered phases: source trust/evidence policy (M6), Nuclei lifecycle integration (M2), POC/retest integration (M3), search/identity resolution (M4), and UI/API presentation (M5).
