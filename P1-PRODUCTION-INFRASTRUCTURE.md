# P1 Production Infrastructure and Blocker Governance

> Status: P1 in progress
> Started: 2026-11-28
> Production Ready: false
> P0 status: complete as audit/freeze; production acceptance remains open

P1 is a bounded production-infrastructure stage. It starts with Batch A
(core execution capability) and Batch B (data protection), then proceeds to
Batch C and D only after their exit conditions pass.

No broad infrastructure migration, commit, push, destructive migration,
production cutover, or system-level dependency installation is authorized.

## Batch A: Restore core execution capability

### Scope

- Check host PDF, OCR, Office and image tools.
- Preserve host-first/Podman-fallback architecture.
- Recover or reproducibly rebuild `rip-parser`.
- Create a hashed, source-reviewed, representative real-document corpus.
- Run PDF, Office, image and IR consistency checks.
- Record the exact engine, parser and runtime environment for every parse.

### Current evidence

Host capabilities observed:

- `podman 6.2.1`
- `Python 3.14.10`
- `Tesseract 5.5.5`
- `Poppler pdfinfo 26.11.0`
- `LibreOffice 26.8.3.1`

Container evidence:

- Exact parser image `sha256:9a59eef52bf80aeb2562d9ca32cada455946c775fe92a0b0fb044a527dd8bf7a`
  is unknown in the local Podman store.
- Exact inspect exits 125 and reports `image not known`.
- `rip-parser` manifests and `Containerfile` remain in place.
- A `container_parse_end_to_end` test is currently not real parser proof and
  must not be promoted to format acceptance.

### Exit condition

Batch A exits only with a real approved image or an explicitly approved
reproducible rebuild, at least 40 real hashed corpus samples, format coverage,
Document IR schema/tree/source-position validation, idempotent replay, and
parser failure/timeout recovery evidence.

## Batch B: Protect production data

### Scope

- Inventory SQLite and PostgreSQL backup scope.
- Back up database and file assets.
- Restore into an independent temporary environment.
- Verify migration state and key table/index counts.
- Establish an accountable backup policy and recovery record.

### Current evidence

On 2026-11-28, `apps/server/selftest_phase10_backup.sh` passed:

- Backup and archive verification.
- 8 KiB controlled corruption.
- Atomic restore and SHA-256 match.
- `quick_check=ok`.
- Before/after counts `resources=81`, `evidence=700`, `tasks=42`.
- FTS rebuild: 226/226 documents/rows before and after.
- Sample hit set consistent.
- Retention archive: 2 rows archived/deleted, 2 rows retained.
- Production `migrations/platform*.db` fingerprints unchanged.
- Final result: `PASS=16`, `FAIL=0`.

This passes the SQLite mechanism and isolation exercise. It does not yet
establish a production backup frequency, offsite copy, credentials policy,
PostgreSQL formal recovery, RTO/RPO, or named recovery responsibility.

### Exit condition

Batch B exits only after a named owner approves backup scope/frequency/retention,
independent environment restoration, PostgreSQL dump/restore, migration
before/after reconciliation, file/artifact restore, and signed recovery evidence.

## Five-record governance format

Every P1 blocker record contains:

1. Original issue and reproduction evidence.
2. Root cause, not just the visible symptom.
3. Minimal modification scope.
4. Automated and real-environment test results.
5. Acceptance evidence and remaining risk.

## P1 blocker records

### PB-001/PB-002: parser image and real corpus

1. **Original problem/reproduction**
   - Exact parser image digest is absent.
   - No independent 40-item Golden Corpus exists.
   - `podman inspect` exits 125; `container_parse_end_to_end` alone is not format
     acceptance.
2. **Root cause**
   - The parser image is runtime state rather than a repository artifact.
   - The Containerfile/manifests exist, but the approved image digest is not
     present locally and no source corpus is frozen.
3. **Minimal modification scope**
   - Recover or rebuild only `rip-parser`.
   - Add corpus fixtures and a dedicated parser acceptance script.
   - Do not migrate the parser stack wholesale to host execution.
4. **Tests**
   - Automated: IR/schema/tree/commit tests and parser pipeline suite.
   - Real: host tool versions, exact container identity, representative PDF,
     DOCX, PPTX, XLSX, HTML, Markdown, TXT and images.
   - OCR: digital PDF and scanned PDF; compare text, structure and evidence offsets.
5. **Acceptance/remain risk**
   - Not accepted until approved digest and 40+ hashed real samples pass.
   - Remaining risk: image reproducibility and real-world malformed-document coverage.

### PB-005: production backup policy

1. **Original problem/reproduction**
   - SQLite roundtrip evidence exists, but no signed PostgreSQL/production policy,
     retention, offsite copy or recovery responsibility exists.
2. **Root cause**
   - Backup code and a test roundtrip have been implemented; operational ownership,
     schedule and independent PostgreSQL recovery are separate incomplete gates.
3. **Minimal modification scope**
   - Add operational policy and evidence only.
   - Reuse `rp-admin backup/restore` and existing migration/health code.
   - Do not redesign backup format or database role.
4. **Tests**
   - Automated: existing 16-check SQLite roundtrip.
   - Real: PostgreSQL dump/restore, independent service restart, file/artifact
     restore, migration state and row reconciliation.
5. **Acceptance/remain risk**
   - Requires named owner, RTO/RPO, frequency, retention, offsite path and
     successful independent restore.
   - Remaining risk: formal PostgreSQL production backup and cross-database
     consistency at the exact transaction boundary.

### PB-003/PB-004: human relevance and semantic labels

1. **Original problem/reproduction**
   - M4 has 140 retrieval candidates and zero human grades.
   - Semantic smoke pairs do not calibrate Duplicate/Related/Distinct quality.
2. **Root cause**
   - Synthetic and implementation smoke tests do not replace independent human
     relevance judgments.
3. **Minimal modification scope**
   - Add frozen label fixtures/splits and evaluation tooling only.
4. **Tests**
   - Independent retrieval metrics: Recall@K, MRR and NDCG.
   - Independent semantic metrics: precision, recall, false merge and missed duplicate.
5. **Acceptance/remain risk**
   - No completion claim until independent split and calibrated threshold exist.

### PB-006: real AI Provider E2E

1. **Original problem/reproduction**
   - No live Provider credentials or external-provider run is recorded.
2. **Root cause**
   - Local/mock and live-provider behavior are separate acceptance categories.
3. **Minimal modification scope**
   - Add one operator-run live Provider scenario and evidence record.
4. **Tests**
   - Search, retrieve evidence, use controlled tools, submit candidate, review and
     publish, then re-search.
5. **Acceptance/remain risk**
   - Credentials must not enter git or docs; Provider output and citations must be
     reproducible enough for audit.

### PB-007: production entry and TLS

1. **Original problem/reproduction**
   - 7800 is an existing entry; 7801 is not listening and no formal production
     reverse proxy/TLS cutover is accepted.
2. **Root cause**
   - Development/user-service evidence exists, but production topology and
     rollback are not signed.
3. **Minimal modification scope**
   - Configure existing server/systemd/reverse proxy only.
   - Do not redesign API or frontend.
4. **Tests**
   - Certificate, real Bearer, health, read/write, error/permission, old-entry
     rollback and observation.
5. **Acceptance/remain risk**
   - Not complete until external entry and rollback are demonstrated.

### PB-008: POC/Nuclei authorized validation

1. **Original problem/reproduction**
   - Nuclei and POC execution have governance evidence but no authorized target
     execution evidence.
2. **Root cause**
   - Static validation proves metadata and guards; it does not prove safe runtime
     behavior or technical result.
3. **Minimal modification scope**
   - Use existing isolated/container execution surfaces and an authorized target.
   - Do not broaden target scope or enable automatic execution.
4. **Tests**
   - Positive, negative, timeout/cancel, permission and cleanup evidence.
5. **Acceptance/remain risk**
   - Remains blocked outside an authorized environment.

## Batch C and D are gated

Batch C (AI and complete business closure) starts only after Batch A and Batch B
exit. Batch D (production deployment/POC/TLS/final acceptance) starts only after
Batch C exit and all P0/P1 security blockers are resolved.
