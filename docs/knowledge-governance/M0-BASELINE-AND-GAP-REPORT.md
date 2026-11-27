# M0 — Security Knowledge Governance Baseline and Gap Report

Status: **Complete — read-only audit**

Scope: establish the code/data baseline for the requested M0 → M1 → M6 → M2 → M3 → M4 → M5 → M7 governance program. This report does not treat prior RIP phases or reports with similarly named M numbers as acceptance of this program.

## 1. Audit boundaries

- Read source, schema, reports, API/UI code, and test fixtures; opened the two development data files with SQLite `mode=ro`.
- Did not apply a migration, change a database, edit RTIS evidence, execute a PoC/Nuclei template, or send a request to a target.
- The production/core `migrations/platform.db` and original RTIS `integrations/rtis/knowledge.db` remain outside the write scope. Future migration tests use in-memory fixtures or disposable clones with a tested recovery path.
- The pre-existing modified and untracked worktree was preserved. In particular, the API version-statistics fix remains an existing local change and is not included in this M0 report's change set.

## 2. Observed baseline

### Data and integrity

| Store | Read-only observation | Interpretation |
|---|---:|---|
| `migrations/dev-p6.db` | `PRAGMA integrity_check = ok`; `user_version = 32`; 18,102 KSE rows | Existing plugin knowledge data面 |
| KSE `catalog_entry` | 3,991 rows: 3,125 APPROVED, 806 DISCOVERED, 60 REJECTED | Review state remains per source entry/version |
| KSE `nuclei_template` | 14,111 rows: 552 APPROVED, 13,559 DISCOVERED | APPROVED records document source/classification review, not successful detection |
| Nuclei plugin snapshot | Version 0.1.1; 14,111 rows | Upstream source snapshot and plugin version are recorded |
| `migrations/platform-search.db` | `PRAGMA integrity_check = ok`; 3,708 `search_index_doc` rows and 3,708 FTS rows | This is a separate Resource/search projection, not a row-for-row copy of KSE |

The search index schema contains Resource and Nuclei/POC projection fields. Its 3,708 rows are not directly comparable to the 18,102 KSE rows: the stores have different entity scope and lifecycle. Exact cross-store reconciliation is an M4/M7 requirement, not a basis for inferring missing rows.

The current self-owned POC evidence review reports an overlay of 49 entries and 25 relations, evidence levels K0 29 / K1 9 / K2 10 / K3 1 / K4 0, and a queue of 28 high-priority records. Its evidence-recovery matrix reports 33 records / 94 evidence gaps. These are review/curation facts from existing reports; no record was promoted or modified in this audit.

### Existing implementation foundations

| Capability | Verified foundation | Current boundary |
|---|---|---|
| Knowledge source identity/review | `knowledge_source_entry` (migrations 011/032), `KnowledgeSourceEntry`, per-entry hashes, pinned source metadata, `review_status`, and audit-backed review API | RTIS POC records remain a separate authoritative path; entity types are not joined in one typed graph |
| Nuclei parsing/search | Dedicated `nuclei_template` kind; `nuclei_yaml` parser; template ID, classification, severity, tags, protocols, matcher/extractor counts and `parse_status`; exact CVE/CWE/severity/protocol facets | Snapshot/version history exists through plugin versions, but upstream-missing synchronization is explicitly deferred in an existing test note; parse status does not establish detection effectiveness |
| Knowledge-card relations | PostgreSQL `knowledge_cards` and `knowledge_relations`; 10 relation types; source, confidence and JSON evidence fields | Relations are FK-bound to KnowledgeCards and lack first-class relation review state/effective dates; they do not directly link KSE, RTIS POCs, tools or research Evidence |
| Research evidence | Append-only `research_evidence` with content hash and run/trace references; separate plan-item result states | Existing UI is plan-scoped. It does not provide a unified source-version/evidence/relationship trace across all requested entity classes |
| Deduplication | Exact/normalized-hash and relation candidates exist in the KnowledgeCard governance subsystem; source rows have stable keys and content/metadata hashes | No verified cross-source identity-resolution workflow and no shared deduplication evaluation report were found |
| Search | Search v6 FTS index, structured Nuclei fields, exact CVE/CWE filters, and multi-source query evidence in the intake report | No Precision@10, Recall@20, or duplicate-result evaluation set/report was found; query coverage does not itself prove relevance quality |
| API/UI | `/api/knowledge_sources` has source/kind/review/query filters and a bounded limit; `/api/plugins` exposes source review stats/history. Existing React Knowledge and Evidence pages provide basic search/review or plan-based evidence views, with the established three locales | Knowledge search currently has no page/offset contract; no single overview/detail/review/relations/evidence governance flow was found |

## 3. Gap register and minimum implementation plan

### M0 — Baseline and audit

Read-only baseline and related-code inspection are complete. Before any future persistent development-data change, record a fresh backup, source/data hashes, integrity checks, expected row deltas, and recovery verification.

### M1 — Unified entity and relation model

The repository has multiple useful authorities: plugin KSE for ingested source entries, RTIS for POC/research facts, KnowledgeCards/Relations for governed knowledge, and research Evidence for execution artifacts. A new copy of these facts would create conflicting truth. The M1 ADR must define stable typed references from those authorities and a single canonical relation/evidence model without duplicating entity bodies. Required relations are DETECTS, EXPLOITS, AFFECTS, FIXED_BY, ANALYZES, MITIGATED_BY, USES_TOOL, DERIVED_FROM, and VALIDATES; each needs source evidence, confidence, review state, and effective-time semantics.

### M6 — Source trust and evidence governance

Reuse existing source pins, licensing/risk metadata, review status, audit log and Evidence references. Define a coherent trust/evidence rubric and relation review transitions. DISCOVERED/unreviewed material and speculative relations must remain non-authoritative and excluded from trusted research context.

### M2 — Nuclei lifecycle

Keep the existing YAML parser and structured search path. Add only verified lifecycle gaps: pinned commit/hash/version snapshots, change/removal state and lineage, and explicit distinctions between parser validity, static metadata review, lab-tested template behavior, and target-specific results. No template execution in intake.

### M3 — POC evidence and retest

Keep RTIS as the authority for POC facts. Expose separate fields/states for source integrity, evidence completeness, remediation/advisory status, POC validity and retest outcome. Preserve unknown/missing evidence as unknown. Do not execute POCs absent a specifically authorized isolated environment.

### M4 — Search and deduplication

Extend the existing Search path rather than add another index service. Add exact CVE/GHSA/template-ID lookups, reviewed relationship expansion and source/result aggregation. Hash matches may be exact duplicates; same identifiers require identity review; similarity creates candidates only. Establish a labeled evaluation set and report Precision@10, Recall@20 and duplicate-result rate.

### M5 — Knowledge governance UI

Extend the current design system, API hooks, routes and i18n. Deliver overview/browse, detail, source review, interactive relation management and evidence trace. Add server-backed pagination as needed and preserve filter/search, loading, empty, error and action-feedback states in zh-CN, en-US and ja-JP.

### M7 — Final quality gates and handoff

Run scoped format/static/unit/integration tests; data-consistency, migration-forward and restore/rollback tests on disposable databases; API and web integration/build/interaction tests; and related regression gates. Record actual remote CI outcomes separately from local checks. Publish architecture/schema, source/dedup, Nuclei, POC evidence/retest, relation quality, search relevance, UI acceptance, tests/CI and remaining-issues reports.

## 4. Validation performed for M0

- Read-only `migrations/dev-p6.db` and `migrations/platform-search.db` integrity checks: **PASS**.
- `PYTHONDONTWRITEBYTECODE=1 python3 apps/server/test_knowledge_source_stats.py`: **PASS**.
- `apps/server/selftest_knowledge_review.sh`: **PASS**, including metadata-only API response, explicit audited review, invalid/empty batch rejection, and Bearer authentication. The script used and removed its own `/tmp` fixture.
- `git diff --check`: **PASS** for the pre-existing diff.
- These local checks do not claim remote CI coverage for the uncommitted API change. No CI result was re-labeled or inferred from an earlier commit.

## 5. Remaining questions for implementation stages

1. M1 must decide whether to extend an existing authoritative relation store or add a minimal cross-authority reference layer, after documenting ownership, query direction, transaction boundaries and migration compatibility.
2. Relation confidence is not verification. The model/UI must present these separately and require evidence and explicit review before any relation is treated as validated.
3. Development KSE and Resource/Search projections are distinct. M4 must define deterministic projection membership and reconciliation rather than silently equating their row totals.
4. No authorized lab was designated. PoC and template runtime verification remains out of scope; the platform must represent “not tested” distinctly.
