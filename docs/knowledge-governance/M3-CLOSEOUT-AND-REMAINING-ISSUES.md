# M3 Closeout — POC Evidence, Validity, Remediation and Retest Governance

Date: 2026-11-26
Status: **CLOSED — governance implementation and local acceptance complete**
Scope: Metadata/evidence governance only. No PoC execution and no target traffic.

## 1. Outcome

M3 establishes a fail-closed assessment contract for curated POCs and a read-only retest-history view over the existing research ledger. It deliberately does not promote historical POCs or infer that any target is currently vulnerable or fixed.

The RTIS source database remains authoritative and unchanged. No new database migration was required: the existing research-plan, result-event and append-only evidence tables cover the retest history needed by this phase.

## 2. Assessment contract

The overlay now declares explicit defaults:

- `evidence_completeness=not_assessed`
- `artifact_validity=not_assessed`
- `remediation_status=unknown`
- `license_status=unknown`

Per-POC `poc_assessment` metadata is optional and validated when present. The validator enforces:

- Closed vocabularies and string-typed states; malformed JSON types fail as validation errors.
- Unique evidence references and artifact references; every hashed artifact must also be in `evidence_refs`.
- SHA-256 must be 64 lowercase hexadecimal characters. Local artifact bytes are hashed and compared during overlay validation.
- A remote HTTPS URL may be retained as provenance in `evidence_refs`, but cannot masquerade as a locally hashed artifact. Remote content must be materialized locally before its bytes can be hash-verified.
- `complete` evidence requires a checklist with at least one present item, no declared gaps, and hashed artifacts. Present checklist items must point to referenced, hashed artifacts.
- `incomplete` evidence requires an explicit missing item.
- Valid/invalid/superseded/disputed artifact claims require evidence references, hashed artifacts and an actual result. A `valid` claim additionally requires reproduction conditions, steps and an expected result.
- Remediation claims require an affected product, non-empty unique affected-version strings, and evidence. License claims require a license expression and evidence.
- Assessed claims require an assessment timestamp and reviewer. Both approved and rejected review decisions require reviewer, reason and review timestamp; approval cannot be granted without an assessed claim.
- Local evidence paths must resolve to files inside the workspace; remote provenance references must use HTTPS.

Implementation:
- `integrations/rtis/curation/poc_assessment.py`
- `integrations/rtis/curation/validate_overlay.py`
- `integrations/rtis/curation/k_meta_overlay.json`
- `integrations/rtis/curation/test_poc_assessment.py`

## 3. Retest history

M3 reuses the existing SQLite research ledger:

- `research_plan_item.rtis_poc_id` links an attempt to the RTIS POC ID.
- `research_plan_event` retains append-only result transitions.
- `research_evidence` retains append-only evidence snapshots and payload hashes.

Added `PlanStore::list_poc_retest_history(rtis_poc_id)` and the `rp-verify poc-history --poc-id <id>` CLI command. The query returns attempt identity, provider/version, five-state result, error category, run/trace IDs, timestamps, result transitions and evidence metadata (evidence ID/type/hash/run/trace/time). It intentionally does not return evidence payloads. Non-positive POC IDs are rejected.

No duplicate retest history table or schema migration was introduced.

Implementation:
- `crates/verification-engine/src/store.rs`
- `crates/verification-engine/src/bin/rp-verify.rs`
- `crates/verification-engine/tests/store.rs`

## 4. Existing data boundary

The M3 baseline remains the source of truth for coverage:

- RTIS source: 14,721 POC rows; 28 `k_meta` rows; 659 `k_event` rows.
- Curated overlay: 49 entries, 2 external entities and 25 resolved relations.
- Existing evidence refs: 33 / 70; explicit evidence-gap records/items: 33 / 94.
- Local source paths: 42 / 49; source commits: 39 / 49; upstream source URLs: 9 / 49.
- New per-POC M3 assessments: **0 / 49**.

The 0/49 figure is intentional, not a failed backfill: no record was upgraded from historical prose, the legacy RTIS `verified` bit, source-code presence or a source migration commit. Existing records retain their previous evidence levels, lifecycles, review gates and references. All remain unassessed under the new M3 dimensions until suitable evidence is actually reviewed.

The baseline report is `docs/knowledge-governance/M3-BASELINE-AND-GAP-REPORT.md`.

## 5. Local verification

| Gate | Result |
|---|---:|
| POC assessment Python tests | 11 passed |
| Overlay validator | PASS — 49 entries, 2 external entities, 25 resolved relations; source DB read-only |
| Rust verification-engine unit tests | 45 passed |
| Rust provider integration tests | 2 passed |
| Rust store integration tests | 10 passed |
| Rust scoped Clippy (`-D warnings`) | PASS |
| Disposable SQLite CLI smoke | PASS — plan/item linked to POC 900008, NOT_TESTED → INCONCLUSIVE transitions and run/trace metadata returned; POC ID 0 rejected |
| Python compile checks | PASS |
| `git diff --check` | PASS |

The CLI smoke test used a newly created disposable database and a `fixture://` target reference. It did not dispatch a provider, run a PoC, or send network traffic.

## 6. Explicit non-claims and remaining work

- No current vulnerability or remediation state has been asserted for any real target.
- No historical PoC was executed or revalidated.
- The legacy RTIS `verified` flag remains historical metadata, not a current validity signal.
- A successful research-plan process exit is not equivalent to a confirmed finding; the five-state result guard remains authoritative.
- Evidence payloads remain protected by the existing evidence store; the history projection exposes hashes and metadata only.
- Remote URLs are provenance references, not byte-verified evidence artifacts.
- Per-record evidence recovery and review continue only where source, scope, product/version and evidence are available. Unrecoverable records stay unassessed.
- M4 must add search and deduplication behavior; M5 must expose the assessment/retest states in governance UI; M7 must run the complete quality gates and final handoff.
- No GitHub CI result is claimed for this uncommitted worktree.

## 7. Git and data safety

No commit, push, reset, clean, stash, checkout or rebase was performed. No production or persistent development database was modified. Existing modified and untracked files were preserved.

## Final local verification — 2026-11-26

M3 implementation and local acceptance are complete:
- POC assessment validator: 11/11 tests passed.
- Overlay validator: 49 curated entries, 2 external entities, 25 resolved relations; source DB read-only.
- `verification-engine`: 45 unit, 2 provider integration, 10 store integration tests passed.
- `cargo clippy -p verification-engine --all-targets -- -D warnings`, rustfmt and `git diff --check` passed.
- Retest history endpoint/UI is read-only and returns status transitions plus evidence hashes/metadata only; it does not dispatch a new retest.

Data-state boundary remains unchanged: all 49 curated POCs are `not_assessed` for artifact validity/evidence completeness unless an explicit review record supplies the required hashed evidence and reviewer metadata. Historical `verified` flags do not override the new axes. No public targets were tested and no exploit payload was executed.
