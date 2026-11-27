# RTIS knowledge curation overlay

This directory contains RIP-owned curation metadata for RTIS POC records. It is deliberately separate from `integrations/rtis/knowledge.db` because RTIS remains the vulnerability-research fact source and its production SQLite database is read-only to RIP ingestion.

`k_meta_overlay.json` is loaded by `plugin-runtime` when ingesting the RTIS POC corpus and can also be supplied to `rp-search-index` as an optional read-only search source. Existing `k_meta` rows in RTIS remain authoritative; overlay records only fill missing metadata on the RIP ingestion projection and define which POC metadata enters the curated search surface. Neither path updates `poc`, `k_meta`, `k_relation`, `k_event`, FTS, or hit counters in the source database.

## Evidence policy

`evidence_level` and `lifecycle` are separate axes, following `security-research-upgrade/rtis/design/Knowledge_Lifecycle_Design.md` §3.1–3.2:

- K0 Captured: observation only, no reliable source.
- K1 Researched: reliable official/source/standard reference (`source_urls`).
- K2 Evidence-backed: raw request/response or command echo, with a concrete `evidence_refs` entry (paths are relative to the `pensting` workspace root; entries outside this repository use `../runs/...`).
- K3 Experimented: rerunnable local experiment, with experiment/evidence and rerun references.
- K4 Reusable: at least two distinct environments with outcome records. Never infer K4 from a single target or local lab.

Lifecycle is independently gated: Researched requires a document/external source; Lab-validated requires a reproducible local experiment; Field-validated requires a real target and structured `outcome.helped=true`; Verified requires Field-validated in at least two distinct environments plus human review. AI curation must never promote an item to Verified by itself.

The current batch intentionally keeps records 900053–900060 at K1 because the old `/tmp/tool-lab` test logs referenced by their notes are absent from this workspace. Their upstream source references are retained, but local test claims are not promoted to K2/K3. Records 900061–900063 are K2 with their original report/log/screenshot evidence and explicit scope limitations. Record 900064 is K3/Lab-validated with the P5-9c local-range evidence chain; its legacy `poc.verified=1` does not make it K4 or Verified.

## Updating the overlay

1. Read the upstream lifecycle design before changing levels.
2. Check each `knowledge_id` and `code` against the read-only RTIS database.
3. Add exact `source_urls` / `evidence_refs`; do not invent links or reconstruct missing logs.
4. Keep claims, limitations, and `verified_note` intact. Do not turn one-account evidence into arbitrary-user proof.
5. Validate the JSON and source IDs before staging a new plugin version. Ingest with `--dry-run` first, then use a disposable validation database. Never validate by writing to the production RTIS database.

## Nuclei template body recovery

The RTIS `poc` rows for the 14,111 Nuclei templates keep `content` empty but retain `source_url` and an absolute `path`. During RIP ingestion, the runtime now attempts to read the corresponding YAML only when `source=nuclei`, `category=nuclei`, and the official ProjectDiscovery template source URL all match. It accepts only `.yaml`/`.yml` files beneath `/usr/share/nuclei-templates`, canonicalizes the path to reject traversal/symlink escapes, and caps each file at 2 MiB (the single 10 MiB WordPress plugin detector remains on the compact usage/description fallback). The YAML body is then indexed as the POC body with `body_source=nuclei_template_file`. If the local template tree is unavailable (for example on Windows/macOS), ingestion falls back to the existing `usage` / `description` summary and remains functional. The RTIS database is opened read-only throughout.

## Sensitive verification-note projection

A small number of legacy verified_note fields contain credential material or personal identifiers. For those records, the overlay may set redact_verified_note: true and provide a non-empty redacted_verified_note. The plugin runtime substitutes that safe summary only in the RIP ingestion projection; it never edits the read-only RTIS source database. The validator rejects a redaction flag without a replacement. Do not put secret values in the overlay itself, and keep raw evidence in an appropriately restricted location.

The 2026-11-26 self-POC review adds conservative metadata for high-value candidates. Records without recoverable raw evidence remain K0/Candidate even if the legacy verified boolean or prose note claims a successful test. Record 900043 is K2/Researched because its report and raw response files exist, but a related 2026-11-11 follow-up documents server-side session remediation; current exploitability is not asserted. Record 900047 remains a separate K0 rule candidate because the inspected evidence does not establish the specific front-end credential-construction mechanism described by that rule.


## Standard FTS search integration

The standard search index can include the 49 POC records explicitly listed in this Overlay. It reads only RTIS metadata columns (`id`, `code`, `title`, `kind`, `category`, `cve`, `component`, `severity`, `tags`) and Overlay metadata. It does not select POC `description`, `usage`, `content`, or `verified_note`.

Build a disposable/rebuildable index with both optional inputs:

```bash
cargo run --offline -p search-engine --bin rp-search-index -- build \
  --platform-db rip-data/staging/platform.db \
  --catalog-db rip-data/staging/platform.db \
  --rtis-db integrations/rtis/knowledge.db \
  --curation-overlay integrations/rtis/curation/k_meta_overlay.json \
  --index-db /tmp/rip-search-index.db --rebuild
```

Search using `--resource-type rtis_poc` and the independent facets `--evidence-level`, `--knowledge-lifecycle`, `--poc-severity`, `--poc-category`, `--poc-cve-id`, `--poc-source-integrity`, `--poc-verification-result`, `--poc-dedup-status`, and `--poc-missing-evidence`. The `rp-search-unified` SQLite channel accepts the same facets via `--sqlite-filters` and returns POC curation/provenance fields in result details, including `source_urls`, `missing_evidence`, `review_gate`, and external case-group relations. The index is schema v6; rebuild v4/v5 or older indexes before querying. Only Overlay-listed records enter this curated POC search surface; an unlisted POC is not implicitly reviewed or promoted.

## Governance metadata and relationship validation (2026-11-27)

The current Overlay contains 49 curated records, 2 explicitly declared external case-group entities, and 25 resolved relations. Every entry now has the four independent governance axes `source_integrity`, `verification_result`, `dedup_status`, and `independent_knowledge_status`. These fields do not change `evidence_level` or `lifecycle` and are never inferred from the legacy `verified` boolean.

- `source_integrity`: `complete`, `partial`, or `missing`; `missing` requires an explicit `missing_evidence` list.
- `verification_result`: `candidate_not_verified`, `historical_evidence_supported`, or `not_applicable`. The last value is reserved for upstream tool/method references rather than target vulnerability findings.
- `dedup_status` and `independent_knowledge_status` are independent from evidence level and lifecycle. A related-family edge does not automatically mean duplicate or non-independent knowledge.
- Any non-empty `missing_evidence` list requires an actionable `review_gate`.
- `source_path`, when present, must resolve inside the repository. `source_commit`, when present, must resolve to an actual Git commit. `evidence_refs` may resolve within the `pensting` workspace (including `../runs/...`) and are checked after stripping Markdown anchors.
- Relationship endpoints must resolve to an Overlay entry or a declared `external_entities` node. External case groups are explicitly typed and titled; they are not silently dropped from the search projection.
- `source_urls` are indexed and returned as upstream provenance links. The search service does not fetch those URLs.

The stricter validator checks these rules in addition to source IDs, evidence levels, lifecycle constraints and read-only database access. Run `python3 integrations/rtis/curation/validate_overlay.py` before rebuilding the search index.

Current Overlay coverage: 42/49 records have a local source path; 39/49 have a resolvable source Commit; 9/49 have upstream source URLs. The 7 without a local path are upstream-only tool references. Three existing source files (IDs 900061–900063) have no Git history, so no Commit is invented. There are 33 records with explicit evidence gaps (94 missing-evidence items); all have review gates. Governance metadata completeness is 49/49. These counts do not imply that missing raw evidence has been recovered.

## POC evidence, validity, remediation and retest governance

The Overlay declares conservative defaults in `poc_assessment_defaults`: evidence completeness and artifact validity are `not_assessed`; remediation and license are `unknown`. A per-record `poc_assessment` may override these only with typed evidence references, SHA-256-bound artifacts and the required product/version, reproduction, timestamp and reviewer context. A complete evidence assessment requires a declared checklist with hashed evidence for each present requirement. These assessment states are separate from K-level, lifecycle and the legacy RTIS `verified` flag. No existing POC has been assigned a positive M3 assessment by this schema addition.

Retest attempts remain in the existing SQLite `research_plan_item`, append-only `research_plan_event` and `research_evidence` ledger. `rp-verify poc-history --poc-id <id>` provides a metadata-only query of linked attempts, five-state transitions, run/trace IDs and evidence hashes; it intentionally omits evidence payloads and does not write result/evidence rows. No PoC is executed by this command. A recorded result or process exit alone does not establish a current finding or remediation state.

## Search schema v6 and Web governance workbench

`rp-search-index` schema v6 includes source URLs, source integrity, verification result, deduplication and independent-knowledge status, missing evidence, evidence references, review gates, internal POC relations and external case-group relations. Filters include `--poc-source-integrity`, `--poc-verification-result`, `--poc-dedup-status`, and `--poc-missing-evidence true|false`, in addition to the original K-level/Lifecycle/Severity/Category/CVE facets. Rebuild any v4/v5 index before use.

The Knowledge Web workspace exposes this projection through `GET /api/curated-pocs/search` and a metadata-only `GET /api/curated-pocs/summary`. The default view is the evidence follow-up queue; it displays missing evidence, next review gate, source path/Commit/upstream URLs and related POC/case groups. Both routes use the existing Bearer auth, rate-limit and audit middleware. The endpoints never return POC body fields.
