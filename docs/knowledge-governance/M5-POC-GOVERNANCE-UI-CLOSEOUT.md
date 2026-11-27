# M5 — Knowledge Governance UI Closeout

Date: 2026-11-26
Status: **UI/API implementation and local frontend acceptance complete; full product acceptance depends on M4/M7 gates.**

## Delivered

- Added authenticated read-only `GET /api/curated-pocs/governance`.
- List projection reads the curation overlay only and returns POC metadata plus explicit fail-closed M3 defaults. It does not return POC bodies, descriptions, `verified_note`, or evidence payloads.
- Detail query `?poc_id=<positive-id>` calls `rp-verify poc-history` with the configured `RP_DB_PATH` and returns the existing append-only retest history projection (result transitions and evidence hashes/metadata only).
- Added `POC 治理` view to Knowledge workspace: 49 curated POC records, separate evidence/artifact/remediation/license/review states, evidence gaps, source provenance, and retest history.
- Added search by POC ID/title/assessment state, loading/error/empty states, and read-only detail panel.
- Added zh-CN/en-US/ja-JP UI translations.
- Added backend contract tests and frontend smoke coverage.
- Hardened the existing knowledge-source review write contract to pass the exact plugin version and mandatory auditable reason required by `rp-plugin review`; UI now blocks approval/rejection until a reason is entered and prevents mixed plugin-version batches.

## Verification

- `apps/server/test_poc_governance_api.py`: 3 passed.
- `apps/server/test_plugin_review_api.py`: 3 passed.
- `apps/server/selftest_knowledge_review.sh`: PASS end-to-end (metadata read, versioned review write, audit history, invalid status/empty batch rejection, Bearer auth).
- Live local API-handler invocation: 49 rows; 0 assessed; 49 not assessed; POC 4 detail returned 0 attempts and no payload.
- `web/tests/knowledge-smoke.test.tsx`: 9 passed, including governance detail and retest metadata.
- `npm --prefix web run typecheck`: PASS.
- `npm --prefix web run build`: PASS.
- Full web suite: 22 files / 158 tests passed.

## Governance boundaries

- No assessment was fabricated or inferred from historical verified flags.
- No retest was triggered by opening the UI.
- The UI is read-only; it does not approve evidence or mark a POC valid/fixed. Those actions require separately evidenced review workflows.

## Relation/evidence detail completion

- The governance detail projection now includes the 25 overlay relation records as incoming/outgoing, typed edges with confidence, note and resolved external-entity title where available.
- UI renders source evidence references and the selected POC's relation neighborhood; it remains read-only and does not mutate relation edges.
- The review write flow now requires a valid plugin semver and a non-empty reason, carries both through the API to `rp-plugin review`, and prevents mixed-version batch selection. This fixes the stale API/CLI contract rather than weakening CLI validation.
- Final focused Knowledge smoke: 9/9 passed. Full web suite after relation detail: 22 files / 158 tests passed; typecheck and production build passed.
