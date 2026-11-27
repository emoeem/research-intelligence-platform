# M2 — Nuclei template lifecycle closeout

Status: **Complete for static ingestion, provenance and upstream-presence lifecycle**

## Delivered

- Preserved Nuclei templates in the existing KSE authority and reused the vendored YAML parser; no M2 database migration or new dependency was needed.
- Added a distinct SHA-256 for the original YAML bytes. It is `null` when the raw template is unavailable; the existing generic KSE body and `content_sha256` behavior remains unchanged.
- Projected matcher and extractor structures, along with existing template ID, severity, tags, CVE/CWE, protocol, references and request metadata.
- Kept three claims separate: YAML syntax status, metadata/template-shape projection status, and detection effectiveness. `metadata_projected` does not mean official Nuclei schema validation; `detection_validation_status` remains `NOT_TESTED`.
- Required a complete 40-character hexadecimal `source_commit`. The metadata-package builder verifies the full commit only for a clean Git checkout whose HEAD exactly matches the pin. A non-Git template directory is explicitly reported as `unverified_non_git_tree`.
- Added transactional removal/reappearance reconciliation for a complete inventory from the current active Nuclei source snapshot. Missing entries are retained and marked `upstream_missing`; reappearing entries are cleared. Review decisions are unchanged, and incomplete/empty inventories do not imply deletion.
- Excluded `upstream_missing` Nuclei entries from rebuilt Search indexes.
- Updated the Nuclei source-manifest commit examples and historical provenance descriptions so a short package suffix is not presented as full-tree verification.

## Static data findings

The local template directory is not a Git checkout. Package metadata reports `nuclei-templates 1:v10.5.2.r748.ga346d8f-1`; its suffix agrees with only the first eight characters of the pinned commit `a346d8f00c61bf95bbd30365f12cf923be51ba3d`. The installed tree therefore remains unverified against the full pin.

The post-implementation static audit read 14,389 YAML files:

| Result | Count |
|---|---:|
| Parsed template metadata | 14,365 |
| Non-template/configuration YAML | 21 |
| Missing template ID | 1 |
| Invalid YAML | 1 |
| Over 2 MiB | 1 |
| Unreadable | 0 |

The missing-ID file is `helpers/payloads/swagger.yml`; the invalid YAML file is `network/cves/2004/CVE-2004-2687.yaml`; and the oversized file is `http/technologies/wordpress/plugins/wordpress-plugin-detect.yaml` (10,079,081 bytes). Static parse outcomes are not detection results.

The RTIS metadata projection contained 14,111 rows: 14,109 parsed, one invalid YAML and one `body_unavailable`. The bounded full projection was also written to a disposable in-memory platform database, including 15,084 legacy rows, 49 overlay entries, 25 source relations and 28 projected relation rows. No production or persistent development database was modified.

## Verification

| Gate | Result |
|---|---|
| `cargo test --offline -p plugin-runtime -p search-engine -p plugin-registry` | PASS; three explicit static-audit tests are ignored by default |
| Three ignored static-audit tests, run individually | PASS; read-only YAML corpus results above |
| `cargo clippy --offline -p plugin-runtime -p search-engine -p plugin-registry --all-targets -- -D warnings` | PASS |
| Nuclei package builder Python tests | PASS — 4/4 |
| Focused Web knowledge smoke test | PASS — 8/8 |
| `git diff --check` | PASS |
| Disposable in-memory full KSE projection | PASS |

The package-builder tests cover clean exact-commit checkout acceptance, commit mismatch rejection, dirty-tree rejection without replacing existing output, invalid pin rejection, and read-only source handling. No CI result is claimed. No template or PoC was executed, and no target was contacted.

## Remaining issues and boundaries

- The local package's shortened revision is not sufficient to prove the complete installed tree matches the manifest pin. A clean checkout at the pinned full commit or an independently verified package artifact is still needed for that provenance claim.
- The oversized template's raw YAML remains unavailable to the bounded parser; its generic fallback body is not a substitute for the raw-template hash.
- Detection effectiveness remains `NOT_TESTED`. Syntax, metadata, source approval and Search visibility do not establish that a template detects a vulnerability.
- Search lifecycle reconciliation is only as complete as the authoritative source inventory supplied by a successful active-source ingest. Failed or partial ingestion must not be used to infer upstream removals.
- Full workspace quality gates, cross-source search evaluation and final project acceptance remain in M7.

## Migration and data-safety note

M2 introduced no database migration. Migration 032 remains the historical M2 entry-kind migration; M6 later added SQLite 033 for review history. M6's disposable forward migration and restore to V032 are documented in [M6 closeout](./M6-CLOSEOUT-AND-REMAINING-ISSUES.md). No persistent database migration was run for M2.
