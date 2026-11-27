# M4 Baseline — Unified Search Relevance and Cross-Source Deduplication

Date: 2026-11-26
Status: **M4 implementation accepted with documented scope limits — M4-A/B verified; M4-C audit-only; M4-D sampled user acceptance with per-pair grading waived; numerical relevance metrics unavailable**
Scope: Search relevance, unified search API readiness, canonical identity and duplicate-candidate governance. No source database writes.

## 1. Executive findings

1. The current SQLite FTS index is healthy as a rebuildable metadata projection (schema v6, 3,708 documents and 3,708 FTS rows), but it has no general cross-source identity or duplicate-group projection.
2. The existing Rust search engine supports FTS5/BM25, CJK 2-gram matching, quality and keyword-coverage signals, filters, source annotations and explainable reasons. It does not currently collapse or annotate cross-source duplicate candidates.
3. The cross-store `rp-search-unified` crate merges PostgreSQL hybrid-search hits and SQLite FTS hits using RRF, but its identity key is `(store, id)`; equivalent entities in different stores remain separate. It is currently a CLI/library path, not a registered general HTTP search endpoint.
4. The ⌘K palette loads up to 200 tools, 200 knowledge items and 200 tasks, then performs client-side token matching. It does not call the unified search engine. The Knowledge page's `/api/curated-pocs/search` is a separate, POC-only FTS endpoint.
5. Existing duplicate workflows are useful but scoped: exact-content and MinHash candidate edges operate on PostgreSQL KnowledgeCards within a workspace. M1 provides typed cross-authority entity identities and evidence-backed candidate relations, but current search documents are not yet joined to those identities.
6. Naive URL/title deduplication is unsafe. Current index URLs often identify a whole plugin corpus, and repeated Nuclei titles can refer to different CVEs/templates.

## 2. Read-only index baseline

Inspected `migrations/platform-search.db` in SQLite read-only mode.

| Measure | Observed |
|---|---:|
| Index schema version | 6 |
| Search documents | 3,708 |
| FTS rows | 3,708 |
| `catalog_entry` | 3,107 |
| `nuclei_template` | 552 |
| `rtis_poc` | 49 |
| Governance state | 3,708 ACTIVE |

The index is a derived snapshot built from Platform/KSE/catalog sources and explicitly curated RTIS POC metadata. It is not the authority for source content or review decisions.

### Identity and duplicate caveats

- `resource_id` is unique within the index. KSE IDs include plugin ID, plugin version, entry kind and item key; POC IDs use `rtis:poc:<id>`.
- `canonical_url` is not a safe global dedup key. The current 3,708 rows have 1,862 distinct canonical URL strings, but the apparent excess is dominated by two corpus-level URLs: 1,296 PoC Index rows share `https://github.com/0xMarcio/pocindex`, and 552 Nuclei rows share `plugin://knowledge.nuclei-templates`. These are source/corpus locators, not duplicate-resource evidence.
- There are 15 repeated normalized-name groups. They are Nuclei template titles, including four Oracle WebLogic RCE entries and three Drupal RCE entries with different CVE/template paths. Same title is therefore not sufficient evidence of duplication.
- The 49 curated POCs currently have 44 `independent` and 5 `related-family` dedup states; no current `duplicate_candidate` or `duplicate` POC rows were found in this snapshot.
- The current search document schema does not expose a generic per-source content SHA-256, source snapshot reference, canonical entity reference or duplicate-group ID. POC-specific source commit/hash/evidence metadata is not a substitute for a generic identity model.
- KSE source records do carry source metadata such as item key, source location and content/metadata hashes in the authoritative KSE store; the current search projection does not preserve the complete identity/provenance tuple.

## 3. Search implementation baseline

### Rust FTS engine

`crates/search-engine/src/query.rs` implements:

- FTS5 MATCH with OR semantics for expanded query terms.
- CJK 2-gram phrase matching and ASCII term matching.
- BM25 normalization relative to the current matched result set.
- A mixed score with nominal weights FTS/BM25 50%, quality 30%, keyword coverage 20%; missing axes are excluded and remaining weights normalized.
- Structured exact-match filters for resource type, quality, license, maintenance, Nuclei severity/protocol/CVE/CWE, POC evidence level/lifecycle/severity/category/CVE and POC governance facets.
- Result explanations, keyword matches, source annotations and stable resource-ID tie-breaks inside this engine.

Known limits:

- No dedicated exact identifier/CVE/template-ID ranking branch or field-specific boost contract is visible in the current query path.
- No canonical-entity grouping, cross-source duplicate suppression or candidate badges.
- No 100+ human-judged cross-source benchmark exists yet. An earlier 10-query qrel for the 141-card `ws-bench` workspace is generated from category/prefix rules, not independent human judgements; its four-mode PG-only baseline is documented in `docs/rip3/batches/P4-4-BENCHMARK.md`. It is useful regression material but cannot validate cross-source search.
- The current score is relative to each result set; it should not be interpreted as a calibrated probability or confidence value.

### Existing benchmark harness and exact-ID baseline

- `crates/intel-db/src/bench.rs` and `rp-kc bench --qrel-file` already provide Recall@K, Precision@K, MRR, NDCG@K and latency across BM25/vector/hybrid/rerank for PostgreSQL KnowledgeCards. The existing `rip-data/bench/qrel.json` contains 10 generated queries over a 141-card isolated workspace; relevance was derived from category/prefix rules. Historical metrics are BM25 Recall@10 0.5415 / P@10 0.3600 / MRR 0.6500 / NDCG@10 0.6613; vector 0.5692 / 0.4500 / 0.8667 / 0.7771; hybrid 0.5812 / 0.4700 / 0.9167 / 0.8212; rerank 0.5812 / 0.4700 / 0.9500 / 0.8102. This does not cover the SQLite corpus or cross-store identity.
- Added a reproducible structured exact-CVE self-consistency check: `scripts/security-intel/evaluate_exact_cve_search.py`; output: `test-results/m4-exact-cve-search-baseline-2026-11-26.json`. It queries the first 100 sorted CVEs from the 505 unique CVEs in the current Nuclei index, with gold resource IDs derived from `nuclei_cve_ids_json` in the same index snapshot. Result: 100/100 queries completed; Recall@10 1.0000; MRR 1.0000; first relevant result rank 1 for 100/100; all structured relevant template IDs found within top 100 for 100/100; mean precision over returned hits 0.9900; first 10 query order deterministic on repeat. Two queries also returned one additional same-product template with a different CVE; this is a potential related-family result requiring human judgement, not an automatically confirmed false positive.
- This exact-CVE test is a structured index self-consistency regression, **not** an independent human-judged relevance benchmark. It must not be used to claim semantic search Precision@10/Recall@20 or cross-source dedup quality. M4 should extend the existing benchmark harness/metrics and produce a reviewed 100+ query qrel across both stores.

### Cross-store unified search

`crates/unified-search/src/lib.rs` invokes PostgreSQL hybrid search and SQLite FTS, then performs RRF (k=60). The merge key is `(store, id)`, so same-entity hits from separate stores cannot currently be unified. Equal RRF scores do not have an explicit deterministic secondary tie-break. SQLite errors degrade to an empty channel with an error field; PostgreSQL errors propagate.

No general `/api/search` handler or route was found in `apps/server/rp_server.py`. The unified search crate is not currently wired into the server API.

### Existing UI/API paths

- `GET /api/curated-pocs/search`: POC-only endpoint. It validates filters, invokes `rp-search-index` as a subprocess, and returns curated POC metadata without POC bodies. Its separate derived index is rebuilt using source-file modification times.
- Knowledge page: searches only the curated POC overlay tab through the endpoint above.
- ⌘K global palette: loads `/api/tools?limit=200`, `/api/knowledge?limit=200`, and `/api/tasks?limit=200`; matching is client-side, all query tokens must match, prefix of the first token ranks ahead of contains, and source-list order is the tie-break. It is a navigation palette, not full corpus search.

## 4. Existing dedup and M1 reuse

- `intel-db::card_repo::dedup_scan_same_content` creates idempotent `SAME_CONTENT` candidate relations for same-content KnowledgeCards within a workspace; it does not auto-merge cards.
- The governance worker's MinHash scan creates `NEAR_DUPLICATE` candidate relations with estimated Jaccard evidence and confidence. This is a candidate-generation path, not a deletion/merge operation.
- M1 `knowledge_entity_refs` enforces unique `(entity_type, canonical_key)`; `knowledge_entity_sources` preserves source system/entity/version, URI, commit and optional SHA-256; `knowledge_relations` supports `SAME_CONTENT`, `NEAR_DUPLICATE`, `RELATED_TO` and domain relations with candidate/review status; evidence-backed review is separate.
- M4 should reuse these identities and candidate relations. It must not introduce a second competing source of truth or silently approve/merge relations.

## 5. M4 minimum implementation plan

### M4-A — Canonical identity and provenance projection

- Define a typed canonical identity contract for vulnerability, detection template, POC, security tool, technical document, evidence and knowledge card.
- Map each source record to one entity reference plus one or more immutable source references. Preserve source system, source record ID, version/commit, canonical URI, content hash (when actually available), observed time and source state.
- Do not infer identity from corpus URLs or title equality. Use source-native stable identifiers first; use exact content hash for same-content candidates; use product/CVE/version/context combinations only as typed candidate signals.

### M4-B — Search projection and relevance

- Extend the rebuildable search projection with generic entity/source references, aliases, exact identifiers, source freshness/provenance, evidence and governance facets, and duplicate-candidate group references.
- Keep source authorities immutable; the index remains disposable and rebuildable.
- Add exact identifier/CVE/template ID branches and explainable field-level ranking boosts while preserving existing FTS/BM25 behavior as a baseline.
- Add deterministic tie-breaking and preserve source/channel failure reporting.
- Apply workspace/authorization and governance visibility filters before returning results.

### M4-C — Cross-source duplicate candidates

- Candidate sources: exact normalized content hash; same source-native identifier across source versions; normalized URL only where URL semantics identify an individual record; conservative title/body similarity; typed CVE/product/version relationships.
- Keep `SAME_CONTENT`, `NEAR_DUPLICATE`, and `RELATED_TO` distinct. Same vulnerability/CVE or same product does not by itself prove duplicate POCs/templates.
- Every candidate must preserve both source records, evidence, source version and rationale. Candidate generation must be idempotent and auditable.
- No automatic deletion, merge, approval or source-state mutation. Human review and M1 evidence-backed relation approval remain mandatory.

### M4-D — Search API and evaluation contract

- Expose the unified search engine through an authenticated, workspace-scoped read-only API. Keep the POC-only endpoint compatible during migration.
- Return canonical entity identity, per-source provenance, channel/rank explanation, confidence/evidence signals, relation candidates and duplicate rationale without leaking source bodies or secrets.
- Build a judged evaluation set of at least 100 queries across exact CVE/template/POC IDs, product/version, vulnerability concepts, Chinese/English mixed terms, tools, advisories, remediation and relationship traversal.
- Measure Precision@10, Recall@20, nDCG@10, exact-ID recall, duplicate-result rate and source diversity. Do not claim quality targets until the gold set is reviewed and metrics are actually computed.

## 6. M4 acceptance gates

- Search result IDs map to stable typed entities and retain all source references.
- No result is hidden solely because titles or corpus URLs match.
- Exact content duplicates produce idempotent candidate edges; near-duplicate candidates include a reproducible similarity rationale.
- Candidate edges remain unapproved until evidence-backed review.
- Query ranking is deterministic and returns explainable score components.
- Search API applies authorization/workspace/governance filters before result projection.
- At least 100 judged queries are available; metrics and regression fixtures are committed with the implementation (not merely a synthetic unjudged query list).
- Existing FTS, POC filters, Nuclei filters and source annotations retain regression coverage.
- Full database writes/migrations, if any, are tested on disposable copies with backup/restore. Search indexes remain rebuildable.
- No auto-merge or destructive dedup action.

## 7. Safety and Git boundaries

This audit read the derived SQLite search index and source code only. No persistent database was modified; no source records were deleted or merged; no POC or template was executed. The current worktree contains pre-existing and phase-related modifications. Preserve all modified and untracked files; do not reset, clean, stash, checkout, rebase, commit or push without explicit authorization.


## 8. M4-A implementation — typed identity and exact identifier projection

Status: **M4-A COMPLETE; M4 overall remains IN PROGRESS.**

### Implemented

- `SearchIdentity` is now a typed projection in `crates/search-engine/src/model.rs`: entity type, canonical key, source system, source entity ID, source version, source content SHA-256, metadata SHA-256 and referenced vulnerability IDs.
- Search index schema upgraded from v6 to v7 with `identity_json`. This remains a rebuildable SQLite projection; no source database or PostgreSQL migration was made.
- Platform resources receive source identity `platform/<resource_id>`.
- KSE plugin rows preserve plugin ID, item key, plugin version, source Commit, source path, content hash and metadata hash.
- Nuclei templates map to `detection_template` identities using their source-native `template_id` (when present), and expose their CVE IDs as vulnerability references.
- PoC Index risk-enrichment rows map to `vulnerability` identities using `cve:<CVE-ID>`. They are not classified as exploit POCs.
- Curated RTIS rows map to `poc` identities using `rtis:poc:<id>`; their CVE references are normalized to uppercase. No POC body is read and no content hash is invented.
- Exact `canonical_key` and cross-resource `vulnerability_id` filters are available through `rp-search-index` and `rp-search-unified --sqlite-filters`.

### Real-source temporary rebuild

Built a disposable v7 index from read-only `migrations/dev-p6.db` and `integrations/rtis/knowledge.db` plus the current curation overlay. Result: 3,708 documents / 3,708 FTS rows; rebuild completed in under one second.

Coverage from the temporary v7 index:

| Measure | Result |
|---|---:|
| Source identity present | 3,708 / 3,708 |
| Typed canonical entity key | 1,897 |
| Referenced vulnerability IDs | 1,849 |
| Source content SHA-256 | 3,659 |
| Source metadata SHA-256 | 3,659 |
| Exact content-hash duplicate groups | 0 |
| Exact metadata-hash duplicate groups | 0 |
| Duplicate typed canonical-key groups | 0 |
| Normalized title-collision groups | 15 |
| Corpus-level URL-collision groups | 2 |
| Automatic merge/delete operations | 0 |

The 49 curated RTIS POC rows intentionally have no source-content hash because the search projection does not read POC bodies. The 15 repeated title groups are Nuclei template title collisions with distinct template paths/CVEs; they remain separate. The two URL collisions are source/corpus locators, not record identities.

### Reproducible checks

- `scripts/security-intel/evaluate_exact_cve_search.py` now requires a v7 index and runs 100 structured exact-CVE self-consistency queries. Report: `test-results/m4-exact-cve-search-baseline-2026-11-26.json`.
- v7 exact-CVE result: 100/100 queries completed; Recall@10 1.0000; MRR 1.0000; first relevant rank 1 in 100/100; all structured relevant templates in top 100; mean precision among returned hits 0.9900; first 10 queries deterministic on repeat. This remains same-index structured self-consistency, not a human-judged semantic benchmark.
- `scripts/security-intel/audit_search_dedup.py` reports exact hash/canonical-key candidates and title/URL collisions without mutating data. Report: `test-results/m4-dedup-audit-2026-11-26.json`.
- Rust tests: search-engine 24 unit + 2 rebuild integration; unified-search 2 library + 1 CLI parser = 29 passed, 0 failed.
- `cargo check --offline -p search-engine -p unified-search`: PASS.
- `cargo clippy --offline -p search-engine -p unified-search --all-targets -- -D warnings`: PASS.
- Rustfmt check on all touched Rust files and `git diff --check`: PASS.

### Version transition boundary

The tracked `migrations/platform-search.db` remains schema v6 and was not modified. The v7 engine correctly rejects it with an explicit “rebuild required” error. M4-B now wires the unified API to a server-managed v7 derived index and its rebuild lifecycle; live dual-store execution remains blocked by local PostgreSQL availability. Do not replace the tracked v6 file as an incidental side effect.

## 9. Remaining M4 work

- M4-B implementation is in place for authenticated default-workspace API, v7 index lifecycle, typed identity merge, source retention and facets. General workspace ACL mapping, live PG/SQLite parity and ranking explanation/boost benchmark remain open.
- M4-C read-only duplicate audit found no exact content/metadata/canonical-key duplicate groups. Live M1 candidate relation write/read/review requires PostgreSQL and remains open; no automatic merge/delete.
- M4-D still needs a 100+ human-reviewed cross-source qrel before reporting Precision@10, Recall@20, nDCG@10, exact-ID recall, duplicate-result rate and source diversity.
- M5 implementation is delivered; see `M5-POC-GOVERNANCE-UI-CLOSEOUT.md`. M7 local gates pass; remote GitHub Actions remains untriggered because changes are uncommitted.

## 10. M4-B implementation checkpoint (2026-11-26)

Status: **API and identity merge implemented; runtime integration remains partially blocked.**

Implemented since M4-A:
- `rp-search-unified` now merges only hits with the same typed `(entity_type, canonical_key)` identity across stores. Same title without matching typed identity remains separate.
- Merged hits retain a list of source references; SQLite and PostgreSQL projections attach source identity metadata. RRF ordering has deterministic secondary keys.
- Server exposes authenticated `GET /api/search`, validates query/limit/facets, rebuilds the derived schema-v7 index when stale or schema-v6, and returns channel counts plus source-bearing hits. Existing POC-only search remains compatible.
- API contract tests cover route registration, safe CLI arguments, filter validation, query limits, workspace fail-closed behavior, and schema-v7 rebuild/reuse.
- Rust unified-search tests cover same-identity collapse, different-identity preservation, source retention, and stable repeated ordering.

Verification observed:
- `apps/server/test_unified_search_api.py`: 6 passed.
- `cargo test --offline -p unified-search`: 4 passed (3 library + 1 CLI parser).
- `cargo build --offline -p unified-search --bin rp-search-unified`: PASS.
- `target/debug/rp-search-unified --help`: PASS.
- The current local PostgreSQL service is not listening (`pg_isready` reports no response). Therefore a live dual-store query and channel parity cannot be certified in this environment.
- Current REST auth resolves a caller identity but does not expose a caller-to-workspace ACL mapping to this handler. The API intentionally accepts only `default` and rejects other workspaces with 403. This is fail-closed, but it is not yet general multi-workspace authorization.

## 11. M4-C dedup audit checkpoint

- Existing read-only dedup audit reports zero exact content-hash duplicate groups, zero metadata-hash duplicate groups, and zero duplicate typed canonical-key groups in the audited schema-v7 snapshot.
- Repeated normalized titles and corpus-level URLs remain collisions, not duplicate proof. No records were deleted, merged, or auto-approved.
- Existing M1 relation model and PG APIs are available in code; however, PostgreSQL is unavailable locally, so a live idempotent cross-source candidate write/read/review cycle was not executed.
- No synthetic duplicate candidate was inserted merely to make the queue non-empty.

## 12. M4-D benchmark boundary

- The 100-query exact-CVE test remains a same-index structured self-consistency test, not a human-judged relevance benchmark.
- A reviewed 100+ cross-source qrel and live PostgreSQL search results are still required before reporting Precision@10, Recall@20, nDCG@10, duplicate-result rate or source diversity as production-quality metrics.
- Do not reinterpret synthetic/source-derived labels as human judgement.

M4 overall is **accepted with documented scope limits** for the selected personal-workspace model. PostgreSQL isolated runtime and cross-store source-reference merge have since been verified below. M4-C remains audit-only because no defensible duplicate groups were found. The user explicitly accepted the 140-pair batch by sample review and waived individual grades; this does not produce numerical relevance metrics.


## 11. M4 acceptance checkpoint (2026-11-27)

Status: **M4-A/B implementation and live dual-store smoke verified; M4-C audit-only by evidence; M4-D accepted by sampled user review with per-pair grading waived.**

### Personal workspace and PostgreSQL acceptance environment

- Workspace policy selected: `personal`. The unified search API accepts only the `default` workspace and rejects arbitrary workspace IDs before dispatch. No multi-tenant membership/RBAC model is claimed or introduced.
- PostgreSQL runs in a dedicated Podman container `rip-m4-personal-pg`, bound only to `127.0.0.1:55432`, database `rip_m4_acceptance`, backed by the named volume `rip-m4-personal-pgdata`. The system PostgreSQL service and source databases were not changed.
- PostgreSQL migrations reached V0012. Required extensions `vector`, `pg_search` and `pg_jieba` are installed. The `intel-db` unit and PostgreSQL integration suites passed after explicit migration bootstrap; the document-domain test helper now applies migrations before each test so test ordering cannot make fresh databases flaky.
- Local digest-pinned Podman images are resolved by verifying the tag's actual manifest digest and then executing the immutable image ID. The local `rip-embed` image was rebuilt from its Containerfile; JSON/YAML manifest pins and `containerfiles/README.md` were updated to the observed digest. No image was pushed to a registry.

### Unified index and server rebuild lifecycle

- `rp-search-index build` now accepts `--plugin-db` separately from `--platform-db`. This is required because the live platform resource database and the plugin/KSE knowledge database are separate files.
- `rp_server.py::_ensure_curated_search_index` now includes `PLUGIN_DB_PATH` in the source freshness check and passes it to the builder. The prior server-managed rebuild path could omit the plugin corpus when the platform database did not contain `knowledge_source_entry` rows.
- The rebuilt v7 derived index contains 3,789 documents and 3,789 FTS rows, assembled from the platform DB, plugin DB, catalog metadata and curated RTIS Overlay. The tracked schema-v6 `migrations/platform-search.db` was not overwritten.
- Server API contract tests cover plugin DB source freshness and `--plugin-db` argument propagation. Live `rp-search-unified` smoke queries for SQL injection and CVE-2021-44228 returned both channels (`pg_intel` and `sqlite_knowledge`) with no SQLite channel error. The PostgreSQL side used isolated acceptance-only fixture cards, not production knowledge.
- Search results include ranking explanation fields (score, FTS score, keyword coverage, matched keywords and source annotations). Typed identity merge behavior and deterministic ordering remain covered by Rust tests; the live smoke proves dual-store retrieval, not that the temporary PG fixtures represent the same entity as a SQLite source record.

### M4-D review batch

- Generated 140 query-document pairs: 14 queries × the complete top-10 result list, preserving the actual ranked positions.
- Source distribution: 102 CISA KEV, 29 Nuclei templates, 8 RTIS curated POC metadata rows and 1 Platform resource.
- AI suggestions are populated in `ai_suggested_grade` with rationale/confidence, but all `human_grade` fields remain blank. The evaluator currently reports `pending_human_judgment` (0/140 judged) and refuses to calculate metrics from AI labels.
- Review instructions and grade definitions: `docs/knowledge-governance/M4-QREL-REVIEW-BATCH-001.md`. After human review, run `python3 scripts/security-intel/evaluate_m4_qrels.py` to calculate NDCG@10, MRR@10, Precision@5/10 and MAP@10.

### Remaining acceptance boundary

- M4-C remains read-only audit-only. No exact content/metadata/canonical-key duplicate groups justify automatic candidate creation; repeated titles and corpus URLs remain separate unless evidence supports a typed relation. No auto-merge/delete was performed.
- M4-D is not accepted until all 140 `human_grade` values are confirmed. No human-labelled Precision/Recall/NDCG claim is made yet.
- Remote GitHub Actions has not been triggered because the worktree remains uncommitted. No commit or push was performed.


## 12. Reusable qrel review and evaluation tooling (2026-11-27)

- Label Studio 1.23.2 selected for the human annotation UI after evaluating Argilla. Argilla's own repository now says feature development has stopped and only bug fixes/security patches are expected; it is not deployed.
- Label Studio tasks and XML config generated under test-results/m4-label-studio-batch-001/. AI suggestions are imported as predictions, not annotations.
- Added a strict JSON annotation importer. It updates only human judgment columns, rejects unknown candidate IDs and conflicting annotator labels, and never reads Label Studio predictions as qrels.
- Replaced hand-written ranking metrics with ir-measures 0.4.3. It exports standard TREC qrels/run and calculates NDCG@10, RR(rel=2)@10, P(rel=2)@5/10, AP(rel=2)@10 and Judged@10. It blocks incomplete human judgments.
- Added ranx 0.3.21 offline comparison/RRF experiment utility. It refuses to compare runs whose top-10 contains unjudged documents. It does not replace the Rust production RRF implementation.
- Four adapter tests pass: Label Studio task generation, annotation import preserving AI suggestions, conflict rejection without mutation, and evaluator refusal of AI-only grades.
- The per-pair qrel grading gate is waived by the user's sampled acceptance decision; numerical relevance metrics remain unavailable until a future judged sample exists.


## 13. Label Studio provisioning and import verification (2026-11-27)

- Official Label Studio 1.23.2 was installed with uv/Python 3.13.15 in a dedicated virtual environment. The Docker Hub image repeatedly failed during blob downloads with EOF; no third-party mirror image was substituted.
- Pinned sentry-sdk below 3 because Label Studio 1.23.2 passes an option removed in sentry-sdk 3.0.1.
- Added a repository launcher and systemd user service. It binds only to 127.0.0.1:8062, persists data under ~/.local/share/rip-m4-label-studio/data, enables SSRF protection, disables the network version check, and clears backend/frontend Sentry DSNs to avoid external telemetry.
- Created the local reviewer account. A temporary API token was used to provision the project and import tasks, then deleted; legacy token authentication is disabled again. The password remains in a mode-0600 local credentials file and is not tracked.
- Label Studio project ID 1 was created. API verified 140 tasks, 140 advisory predictions and 0 annotations; first task readback confirmed the pre-annotation value and no human annotation.
- Human qrel review remains pending; the UI is provisioned but no user judgment has been fabricated.


## 14. Typed cross-store source-reference merge smoke (2026-11-27)

- Added explicit external source-reference identity to PostgreSQL hit metadata. The merge key is source_system + source_entity_id + source_version, and only CURRENT + APPROVED external source refs can connect a PG card to a SQLite projection. Titles are not used for cross-store identity.
- Unit tests verify exact approved source tuple merge, version mismatch isolation, deprecated/non-current isolation, unreviewed source isolation, and preservation of both source records.
- Live isolated PostgreSQL acceptance fixture linked one PG knowledge card to the exact CISA KEV source tuple in the v7 SQLite index. The unified query returned one merged result preserving both sources: pg-intel/kc-18ebc7c04260de0d0000 and sqlite-knowledge/kse:knowledge.cisa-kev@0.1.2:catalog_entry:CVE-2020-29574. Both search channels were available and SQLite reported no error.
- This was an acceptance-only fixture in the isolated Podman database, not a production knowledge record.


## 15. User acceptance and remaining metric boundary (2026-11-27)

- The user explicitly confirmed that they inspected a sample of the 140 candidate results, found the content acceptable, and approved the batch without individually submitting all 140 labels.
- The batch is therefore accepted for this project milestone under a documented sampled-review waiver. This is a product/operational acceptance decision, not a claim of exhaustive relevance annotation.
- Label Studio persistence remains 0 annotations / 0 drafts; the CSV remains 0/140 human grades. These facts are retained, not overwritten.
- The ir-measures evaluator correctly remains pending_human_judgment with metrics null. No ranking relevance metric is inferred from AI suggestions or the user's sample-level acceptance.
- For future quantitative ranking evaluation, create a smaller agreed judged sample or a new qrel batch; do not reopen this accepted batch solely to force 140 clicks.
- PostgreSQL acceptance container rip-m4-personal-pg is running under Podman and accepts connections on 127.0.0.1:55432. The system PostgreSQL service on port 5432 is separate and is not running; this is expected for the selected Podman environment.
