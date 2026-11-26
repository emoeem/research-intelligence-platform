# RIP high-value self-owned POC review and Nuclei ingestion closure

Date: 2026-11-26
Project: Research Intelligence Platform
Scope: Nuclei structured knowledge ingestion/search, followed by an evidence-gated review of high-value self-owned RTIS POCs.

## Executive result

This batch completed the Nuclei knowledge path and added a conservative curation batch for nine high-value self-owned POC records. It does not bulk-label the full RTIS corpus and does not modify the RTIS source database.

The RTIS source database remains read-only. The batch does not approve any new record, execute any POC against a target, or claim that a historical finding remains exploitable.

## Nuclei structured ingestion

The dedicated Nuclei content kind and YAML parser feed the reviewable knowledge_source_entry path, separate from the legacy RTIS POC write path.

- 14,111 RTIS records matched the official Nuclei source tuple.
- 14,109 YAML bodies parsed successfully.
- 1 invalid YAML file was retained with invalid_yaml status.
- 1 body above the 2 MiB limit was retained with body_unavailable status and summary fallback.
- All 14,111 entries were written to a disposable in-memory validation database as DISCOVERED.
- The full disposable ingestion also reconciled 15,084 legacy RTIS rows.
- Search index schema v3 exposes severity, protocol, CVE, CWE and tags as structured fields and exact filters.
- Context Broker exposes the new Nuclei content only after explicit APPROVED review.
- Source provenance pins the Nuclei snapshot to ga346d8f. License evidence is recorded from the installed package metadata and upstream license path; do not inherit RTIS kit licensing or verification.

Migration 032 adds only the nuclei_template entry kind to knowledge_source_entry. The migration regression test verifies that existing rows, IDs, hashes and review states survive unchanged and unknown kinds remain rejected.

## High-value self-owned POC review

Nine records received explicit RIP-owned curation metadata. Existing RTIS k_meta remains authoritative and is never shadowed.

| ID | Subject | Curation decision | Reason |
|---:|---|---|---|
| 4 | List pagination / authorization boundary | K0 / Candidate | Source script exists; no raw request/response or independent evidence bundle was found |
| 12 | Multi-tenant auth-prefix differential | K0 / Candidate | Script exists; cross-tenant claim lacks recoverable raw captures |
| 16 | Actuator / Nacos credential exposure | K0 / Candidate | POC path is unresolved; note contains credential material; downstream Nacos access was not established |
| 18 | Springdoc OpenAPI allowlist exposure | K0 / Candidate | Script exists; downloaded OpenAPI JSON and paired response captures were not located |
| 20 | Spring Boot anonymous-handler oracle | K0 / Candidate | Script exists; calibration responses and dictionary run log were not located |
| 29 | Four-state authentication probe | K0 / Candidate | Script exists; dated response matrix and target/path attribution need recovery |
| 72 | Mobile request-signature key bootstrap | K0 / Candidate | Script exists; credential-bearing note is redacted; raw response evidence was not located |
| 900043 | Unsigned identity-cookie case | K2 / Researched | Original report and raw response artifacts exist; a related follow-up documents server-side session remediation, so current exploitability is not asserted |
| 900047 | Frontend-generated session credential rule | K0 / Candidate | The inspected AoZoneyun report demonstrates unsigned plaintext identity cookies, not the specific frontend crypto construction claimed by this separate rule |

The K0 decisions are intentional. A legacy verified boolean or prose verification note is not a substitute for raw request/response evidence. No K0 item is automatically approved or exposed as trusted context.

Record 79 was not duplicated or shadowed: it already has an authoritative RTIS k_meta row at K3 / Field-validated. It remains a separate high-value reuse candidate.

## Sensitive-note redaction

Three legacy verification notes (IDs 16, 72 and 900043) contained credential material or personal identifiers. The curation overlay now marks these notes for redaction in the RIP projection.

- The runtime substitutes a safe note before serializing searchable metadata.
- The overlay validator requires a non-empty replacement when redaction is enabled.
- A regression test proves that the source note's secret marker does not appear in the projected JSON.
- The RTIS source database is not edited; its original notes remain unchanged.
- The raw evidence remains referenced separately and must be stored with appropriate access controls.

This is projection-level redaction, not a claim that every historical artifact in the workspace has been sanitized.

## Self-owned POC corpus coverage

A read-only aggregate over the 139 RTIS rows with source=self now gives a measurable baseline:

| Measure | Count |
|---|---:|
| Self-owned POC rows | 139 |
| Existing authoritative RTIS k_meta | 21 |
| RIP overlay entries for self-owned rows | 17 |
| Covered by either source | 38 |
| Still without curation metadata | 101 |
| Critical-severity rows | 4 |
| Critical rows already covered | 4 |
| High-severity rows still awaiting review | 28 |
| Rows with a path value | 116 |
| Path values unresolved after workspace-root mapping | 9 |
| Rows without a path value | 23 |

The nine unresolved path values include ID 16. They were not repaired by guessing. This baseline is a review queue, not a reason to annotate all 101 remaining records in bulk.

The next high-severity review batch is grouped by likely reuse/evidence families:

- API authorization and session behavior: IDs 19, 27, 31, 56, 60, 59655 and 900042.
- Client-side secrets and cryptographic request channels: IDs 10, 35, 36, 43, 45, 48, 66, 83 and 900009.
- Multi-tenant/SSRF and upload handling: IDs 40 and 900034.
- Browser/client injection and redirect family: IDs 59648–59651; deduplicate the family before adding relationships.
- Privacy, identity and verification flows: IDs 900031, 900033 and 900035.
- Payment iframe trust, supply-chain forensics and message-signature handling: IDs 900044, 900046 and 900048.

The exact evidence and current-state status for these records must be checked before adding K2 metadata. In particular, do not copy secrets into overlay notes and do not treat a source script as proof of a target finding.

## Evidence and remediation cautions

- IDs 4, 12, 18, 20 and 29 have runnable source files, but the original evidence needed for K2 promotion was not found in the inspected workspace.
- ID 16's source path does not resolve under the current workspace mapping. It must be located from an authoritative manifest or left unresolved; no filename-based path repair was made.
- ID 72's note contains signing-key material. Treat any previously exposed key as compromised; rotate/revoke it before any authorized retest.
- ID 900043 is a historical evidence-backed case. A related 2026-11-11 report documents server-side session binding on the AoZoneyun admin service. Recheck the exact endpoint before describing it as currently exploitable.
- ID 900047 remains distinct from 900043. Do not inherit the case evidence until the matching frontend JavaScript credential-construction function and raw request/response chain are located.
- No new Field-validated, Verified, K3 or K4 status was assigned.

## Validation

The following checks passed during this batch:

- `python3 integrations/rtis/curation/validate_overlay.py` — 21 entries and 10 relations validated; source DB opened read-only.
- Full disposable end-to-end ingestion test — 14,111 Nuclei rows, 14,109 parsed, 1 invalid_yaml, 1 body_unavailable, 15,084 legacy rows; additionally asserts redacted notes for IDs 16, 72 and 900043.
- Nuclei parser regression — official source tuple filtering, pinned commit and license provenance.
- Migration 032 regression — existing-row preservation and closed entry-kind domain.
- Structured search regression — exact severity/protocol/CVE/CWE filters and structured result fields.
- Context Broker regression — approved templates are visible; discovered templates are not.
- Metadata-only change detection regression — metadata changes are detected without silently overwriting the immutable same-version row.
- Package test/clippy/fmt checks — see terminal validation result for the final run.

## Rollout boundary

Not performed:

- Migration 032 was not applied to a persistent or production database.
- The production RTIS plugin was not upgraded or re-ingested.
- The persistent derived search index was not rebuilt.
- No Nuclei or self-owned POC entry was approved.
- No target was scanned or retested in this batch.
- No commit, push, reset, checkout, clean, stash, rebase or other history-changing Git operation was performed.

The worktree already contained extensive unrelated modifications. They were preserved.

## Next evidence-first queue

1. Recover raw captures for IDs 4, 12, 18, 20 and 29; then promote only records whose evidence meets the K2 criteria.
2. Locate ID 16's authoritative source artifact and restrict/rotate any credential material.
3. Recover ID 72's raw response bundle and verify rotation of the exposed signing key before a scoped retest.
4. Separate the exact frontend-crypto evidence for ID 900047 from the plaintext-cookie case 900043.
5. Review ID 79's existing K3 evidence chain and reuse boundaries without creating duplicate metadata.
6. After development-copy migration and index rebuild, validate the plugin upgrade path before any production rollout.
