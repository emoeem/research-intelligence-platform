# Research Intelligence Platform — project closeout

Date: 2026-11-27
Branch at validation: main
Validated HEAD before delivery commit: 0d83a685

## Final acceptance

The implementation and governance work covered by the current project plan is accepted within the user's explicit scope decisions:

- Workspace mode: personal (single default workspace; no multi-tenant RBAC claim).
- PostgreSQL acceptance environment: Podman container, loopback only.
- Search relevance batch: 140 candidates accepted by the user's sample inspection; individual grade submissions waived.
- UI: M7 reference-based visual standard accepted at product level; existing Figma file retained and not modified.
- SPA migration: M8 remains staged; default root is intentionally not switched.

## Milestone status

| Milestone | Status |
|---|---|
| M0 baseline audit | Complete |
| M1 unified entity/relation model | Complete |
| M2 Nuclei ingestion/provenance lifecycle | Complete |
| M3 POC evidence/remediation/retest governance | Complete |
| M4 unified search and cross-source identity | Accepted with documented limits |
| M5 POC governance UI/API | Complete |
| M6 source/evidence governance | Complete |
| M7 visual/i18n/responsive acceptance | Closed for reference-based scope |
| M8 SPA entry migration | Staged by explicit user decision; not promoted |

## M4 relevance acceptance boundary

The user inspected a sample of the 140 candidate pairs, confirmed the content was acceptable, and explicitly waived submitting 140 separate Label Studio annotations. The batch is accepted operationally under this sampled-review waiver.

The persisted facts remain unchanged: 140 tasks, 140 advisory predictions, zero saved annotations/drafts, zero human_grade values. The evaluator remains pending and numerical relevance metrics remain null. AI suggestions are not human judgments. This closeout does not claim NDCG/MRR/MAP/precision/recall from the batch. A future quantitative benchmark should use a smaller agreed judged sample or a separate qrel batch.

M4-C remains audit-only: the reviewed snapshot found no exact content, metadata or typed-canonical-key duplicate groups that justify auto-merge. No record was automatically merged or deleted.

## Validation evidence

### Rust

- cargo fmt --all -- --check: PASS.
- cargo test --offline --workspace: PASS; 1,442 passed, 0 failed, 6 ignored across 198 test binaries.
- cargo clippy --offline --workspace --all-targets -- -D warnings: PASS.
- cargo build --offline --workspace: PASS.
- Host-specific OCR success-path tests now use the existing SHA-256 pin guard and explicitly skip when the installed Tesseract binary does not match the pinned catalog identity. Production identity verification remains fail-closed; the pin was not weakened.

### PostgreSQL

- Podman PostgreSQL 17 acceptance container running at 127.0.0.1:55432.
- On a newly created disposable database and temporary role: Phase 7 integration 7/7 passed twice, source-monitor PostgreSQL smoke passed, backup/recovery gate passed.
- Temporary database and role were removed by a shell cleanup trap. The persistent personal acceptance database and source databases were not changed.

### Python / server

- Python workers: 237 tests passed, 11 skipped because the ONNX/tokenizers runtime is not installed in the system Python 3.14 environment.
- Server regressions: 12/12 passed.
- Security metadata package builder tests: 7/7 passed.
- POC assessment tests: 11/11 passed.
- M4 qrel adapter tests: 5/5 passed.
- Knowledge review selftest, POC search selftest and overlay validator: PASS.

### Web

- TypeScript and production build: PASS.
- Vitest: 22 files / 158 tests passed.
- Prettier check: PASS.
- Playwright visual/accessibility: 85/85 passed.
- Responsive matrix: 12 routes × 3 locales × 2 themes × 5 viewport widths = 360/360 passed.

### Security and performance

- Repository security scan: PASS.
- cargo-deny advisories check: PASS.
- Full cargo-deny check: advisories, bans, licenses and sources all PASS; duplicate dependency-version warnings remain.
- SQLite performance smoke: 1,000 documents / 10,000 chunks; retrieval p50 0.138 ms, p95 0.232 ms; backup/restore verified.
- cargo-audit binary is not installed locally; cargo-deny provided the local advisory and dependency-policy checks. Remote CI remains the authoritative cargo-audit execution.

## Explicit non-claims / follow-up boundaries

- No exhaustive human relevance grades or numerical qrel metrics are claimed for M4 batch 001.
- No pixel-perfect Figma parity is claimed; the user's decision was reference-only and the existing Figma file was not modified.
- No Lighthouse/Core Web Vitals or cold-network performance claim is made.
- Six Rust tests remain ignored for documented manual/environment requirements; 11 ONNX-specific Python tests are skipped because the runtime dependency is absent.
- M8 is not promoted to the default entry. Deployment reverse-proxy/TLS and production rollback validation remain outside the selected staged scope.

## Delivery hygiene

- Preserve the untracked test-results directory, security-intel raw data directory and download_trickest_cve_pages.sh as local artifacts; do not add them to the delivery commit.
- No database backups, production databases, RTIS source database or user data were deleted or rewritten.
- No destructive Git operation was performed.
