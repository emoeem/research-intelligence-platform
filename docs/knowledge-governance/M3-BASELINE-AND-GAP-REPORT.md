# M3 — POC evidence, validity, remediation and retest baseline

Status: **Read-only audit complete; minimum implementation plan recorded**

## Scope and safety

This is the M3 baseline for the security-knowledge governance program, not any historical RIP document also named “M3”. The RTIS knowledge database and curation overlay were read only. The overlay validator passed. Aggregate schema and coverage checks did not inspect POC source bodies, `verified_note` contents, evidence payloads or session material. No PoC or template was executed and no target was contacted.

The existing worktree contains unrelated and earlier-phase modifications. This baseline does not claim them as M3 changes and they must be preserved.

## Existing authorities and reusable paths

- RTIS `poc` remains authoritative for POC identity and original source fields. Its SQLite schema includes `source`, `source_url`, `versions`, `path`, and the legacy `verified` / `verified_note` fields; it has no license, remediation, retest, or last-verified columns. `k_meta` separately holds evidence level, lifecycle, verification basis, version and review timestamps. `k_event` is the event store. No source rows were modified.
- The RIP curation overlay is a separate metadata-only layer. Its current 49 entries have `evidence_refs`, explicit `missing_evidence`, source paths/commits, review gates, source-integrity status and a coarse `verification_result`. It contains no explicit evidence-completeness state, POC-artifact validity state, remediation status, retest outcome, expected/actual result, license field, or last verification timestamp.
- SQLite migration 020 already provides `research_plan_item` linked by `rtis_poc_id`, with context-specific five-state results (`CONFIRMED`, `NOT_REPRODUCED`, `NOT_TESTED`, `INCONCLUSIVE`, `FALSE_POSITIVE`), expected observation, result reason, run/trace IDs and error category. `research_plan_event` records result transitions. Migration 021 provides append-only `research_evidence` with a SHA-256 of the normalized evidence JSON payload and run/trace references. That hash is not a hash of an external evidence file.
- The existing result guard rejects unsupported states, requires reasons for `CONFIRMED` / `FALSE_POSITIVE`, and rejects `NOT_REPRODUCED` when execution failed. This is useful for retest history, but a run outcome is not the same thing as current POC validity or remediation state.
- M1/M6 provide cross-authority entity identities, source snapshots, relation evidence and reviewed relation/source assessments. They can support references to POC, vulnerability, Evidence and `FIXED_BY`/`VALIDATES` assertions without copying source bodies.
- The existing curated-POC API and Web view expose evidence gaps and references, source integrity, coarse verification conclusion, review gate and related entities. They do not expose the separate M3 states listed above.

## Read-only data baseline

The RTIS source contains 14,721 POC rows, 28 `k_meta` rows and 659 `k_event` rows. The legacy `verified` flag is `1` for 134 rows; it is not treated as proof of reproduction or current validity. Aggregate source coverage is 14,572 non-empty source URLs, 39 non-empty `versions` values and 14,254 non-empty paths. These counts do not establish completeness or license rights.

The current curation overlay validates as 49 entries, 2 external entities and 25 relations:

| Measure | Current value |
|---|---:|
| Evidence refs present / total refs | 33 / 70 |
| Explicit evidence-gap records / missing items | 33 / 94 |
| Local source paths | 42 / 49 |
| Source commits | 39 / 49 |
| Upstream source URLs | 9 / 49 |
| Evidence completeness, validity, remediation, retest, expected/actual, license, last-verified fields | 0 / 49 |

The absence of these structured fields means **not assessed**, not “valid”, “fixed”, “failed retest”, or “complete”. `verification_result=historical_evidence_supported` does not mean the POC artifact is valid or the affected product remains vulnerable. `last_reviewed` / `updated_at` are metadata timestamps, not test timestamps. Existing prose and evidence files were not read to infer missing values.

## Gaps and minimum implementation plan

1. Keep RTIS immutable and preserve existing POC IDs, source data, review decisions and evidence references. Do not rewrite `verified` or backfill facts from descriptive text.
2. Extend the additive Overlay metadata with explicit unassessed defaults for evidence completeness, POC artifact validity, remediation and license, plus an optional per-POC assessment. A positive/terminal claim must cite reviewable evidence, product/version context and observation time; otherwise remain `NOT_ASSESSED` / `UNKNOWN`. Do not rewrite the legacy RTIS fields.
3. Use the existing research-plan/item/event/evidence ledger for individual retest attempts instead of adding a second execution history. Add a read-only query keyed by RTIS POC ID so attempts, result transitions and evidence hashes can be traced without returning evidence payloads. Preserve all five outcomes and their run/trace/time metadata. Never infer `NOT_REPRODUCED` from a failed, timed-out or unavailable run.
4. Link evidence and remediation claims to M1 typed entities and evidence-backed relations where applicable. Keep source commit, POC artifact validity, target/version state, remediation status and run result as separate facts.
5. Extend the read-only Overlay validator to detect invalid states, unsupported positive claims, missing evidence references, malformed hashes and missing review metadata. Add focused tests for unassessed defaults and rejected success-shaped metadata before populating any new claims.
6. Preserve compatibility with current overlay consumers and existing SQLite/PostgreSQL authorities. First determine whether optional overlay metadata plus the existing append-only research ledger fully covers the workflow; only add a database migration if an observable gap cannot be closed through existing stores. If a migration is needed, use disposable databases, a backup and verified restore; never migrate `migrations/dev-p6.db`.
7. Keep this phase metadata- and evidence-governance-only. No PoC execution or target traffic is in scope. Existing dispatch/execution paths are not certified by this audit as an authorization or isolation boundary; require a separately evidenced, explicitly authorized isolated-lab scope before any future execution.

## Data-quality boundaries

- Current evidence references are URI/path strings; the overlay does not independently bind every file to a byte hash. The existing research evidence hash protects normalized JSON payloads only.
- Current `source_commit` values may identify a source-tree migration commit rather than the commit that produced a test result. Source provenance and execution provenance must remain distinct.
- The `versions` field is sparse and cannot be presumed to represent affected product versions without per-record review.
- Existing POC descriptions, usage and evidence can contain sensitive material. This audit intentionally reports structural coverage only; later evidence ingestion must follow access-control and redaction policy.
- All 14,672 RTIS POCs outside the 49-entry curated overlay remain unassessed by this overlay. Absence from the curated set is not rejection or approval.

## M3 acceptance boundary

M3 may establish evidence completeness, artifact validity, remediation and retest claims only when their evidence, scope, version and review status are traceable. Metadata presence, a successful process exit, a historical `verified` bit, or source-code existence alone is not validation. No target-specific or current remediation conclusion is asserted by this baseline.
