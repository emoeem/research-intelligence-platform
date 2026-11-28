# P1 Production Infrastructure Status

> Status: in progress
> Date: 2026-11-28
> Production Ready: false

## Batch A: core execution capability

### Host capability observation

The host provides:

- Podman 6.2.1
- Python 3.14.10
- Tesseract 5.5.5
- Poppler 26.11.0
- LibreOffice 26.8.3.1
- Pandoc

These are observations, not parser acceptance. Host tools must report actual
engine versions and remain subject to file safety boundaries.

### Exact parser image

The repository pins
`rip-parser:latest@sha256:9a59eef52bf80aeb2562d9ca32cada455946c775fe92a0b0fb044a527dd8bf7a`.

The exact digest is absent. An attempted reproducible-container build using the
existing `containerfiles/rip-parser/Containerfile` stopped at the apt phase
because the build container could not resolve `deb.debian.org`. The work did
not create or register an alternate parser identity.

The P1 probe is `scripts/rip_p1_acceptance.sh`.

### Real corpus

The existing Office/PDF/image files under `crates/document-router/tests/fixtures`
are test fixtures. They are not a source-reviewed 40-item Golden Corpus and
must not be used to claim real-world format acceptance.

Required corpus manifest fields:

- stable sample ID
- source and license
- original SHA-256
- format and size
- expected text/title/structure
- page/block/location annotations
- scanned/digital/OCR category
- malformed/negative case type
- human review status

## Batch B: data protection

`apps/server/selftest_phase10_backup.sh` passed `PASS=16 FAIL=0` on 2026-11-28.

Passed mechanism evidence:

- backup archive and manifest
- controlled damage
- atomic restore
- SHA-256 exact match
- SQLite integrity
- resource/evidence/task reconciliation
- FTS rebuild
- retention archive
- production database fingerprint unchanged

This is not a production policy sign-off. A formal owner must sign:

- backup scope
- SQLite/PostgreSQL frequency
- retention and offsite location
- encryption/key ownership
- restore owner and RTO/RPO
- independent environment restoration
- migration verification
- file and artifact restoration
- annual or post-change recovery drill

## Batch A/B remaining blockers

- Real corpus automation and source/quality metadata pipeline remain to be completed.
- Human labels are feedback-only and do not gate ingestion or candidate creation.
- Formal PostgreSQL backup and offsite policy unsigned.
- DeepSeek API environment variables are not configured; no real Provider request
  was sent and no secret was persisted or logged.
- Production domain/TLS entry is not provided.

## 2026-11-28 host package verification

The following packages were initially verified and then removed as optional
PDF additions: MuPDF Tools, OCRmyPDF, antiword and unRTF. RIP's host path does
not require these packages. Poppler remains a system dependency and is present.

- `poppler 26.11.0-1.1`
- `python-jsonschema 4.26.0-1`
- `python-magic 1:0.4.27-6`
- `clamav 1.5.4-2`
- `yara 4.5.8-2.1`

Observed host smoke results:

- `pdfinfo`, `pdftotext`, `pdfimages` and Tesseract passed against existing PDF
  and PNG fixtures.
- Python `jsonschema` and `magic` imported successfully.
- A minimal YARA match rule passed.
- ClamAV reported a configuration gap: `/var/lib/clamav` has no supported
  virus database, so `clamscan` could not open a database and returned exit 2.

## 2026-11-28 host parser implementation

`rp-doc parse` now defaults to `HostParserProvider` and reuses the existing
Document Router, Tool Engine and Python parsing worker. `--container` explicitly
selects the Podman parser fallback.

Verified:

- Host format detection and SHA-256 pass unit tests.
- Host Markdown/TXT parsing produces schema-valid Document IR.
- Real PDF, DOCX, PPTX and XLSX fixtures pass host PDF/LibreOffice routing.
- OCR is fail-closed when the installed Tesseract binary does not match the
  repository capability pin.

Remaining:

- Tesseract package identity must be aligned or re-pinned before host OCR is
  accepted.
- A source-reviewed 40+ item real corpus is still absent.

The Tesseract identity mismatch was resolved on 2026-11-28 by probing the
installed `tesseract 5.5.5-1.1` binary and synchronizing the full SHA-256 in
the host capability seed and `rip.doc.image.ocr` manifest. The real host PDF,
Office, text and OCR fixture smoke now passes.

## 2026-11-28 authorized Nuclei pass

An authorized low-risk, rate-limited Nuclei pass was run against
`https://002237.xyz` with `-tags tech -severity info -disable-update-check`,
after the user confirmed the target scope. Results:

- Cloudflare DNS/WAF detected.
- Astro technology detected.
- No destructive POC or exploit was executed.
- The command was stopped after the bounded identification pass produced
  evidence; no production data or repository secret was written.

## 2026-11-28 authorized real target review

Test target: `www.002237.xyz` / `002237.xyz`

Observed:

- Both hostnames resolved through Cloudflare and served HTTPS.
- The TLS certificate was a valid Let's Encrypt certificate for
  `*.002237.xyz` / `002237.xyz`, valid through 2027-02-13.
- The root returned HTTP 200.
- Security headers included HSTS, CSP, `X-Content-Type-Options`,
  `Permissions-Policy`, `Referrer-Policy`, and frame/object restrictions.
- `robots.txt` returned 200 and explicitly disallowed common AI training
  crawlers.
- `/security.txt`, `/sitemap.xml`, and `/.well-known/security.txt` returned 404.
- A bounded low-risk `misconfig/exposure` Nuclei pass returned zero findings
  before it was stopped at the tool timeout.
- No destructive POC, authentication bypass, data modification or exploit was
  executed.

Residual target risk:

- The missing security/contact artifacts are operational gaps, not proven
  vulnerabilities.
- The Nuclei pass did not complete and is not a clean negative security
  certificate.

## Exit status

Batch B mechanism acceptance: pass.
Batch B formal production sign-off: open.
Batch A: blocked by exact parser image and real corpus.
MCP knowledge-ingestion compile defect: fixed and verified.
P1 does not exit while production gates remain open.
