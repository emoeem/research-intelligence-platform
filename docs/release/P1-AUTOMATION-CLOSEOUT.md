# P1 Automation Closeout

> Status: automation complete; deployment and external-secret gates remain
> Date: 2026-11-28
> Production Ready: false

## Implemented

### Host parser

- `rp-doc parse` defaults to `HostParserProvider`.
- PDF/Office/text/OCR host routing reuses Document Router and Tool Engine.
- Tesseract 5.5.5 SHA-256 is pinned and its real smoke passes.
- `--container` remains an explicit isolation fallback.

### Real corpus automation

- `scripts/rip_golden_corpus_manifest.py` inventories supported files and hashes.
- `scripts/rip_golden_corpus_ingest.py` imports and parses files through existing
  `rp-doc` only with `--execute`.
- The script fail-closes for fewer than 40 files, missing PG DSN or missing
  `rp-doc`.
- Parsing/candidate ingestion does not automatically publish factual knowledge.

### AI-assisted retrieval

- `apps/server/rip_retrieval_evaluator.py` implements two modes:
  - `ai-assisted-characterization`
  - `human-ground-truth`
- AI-only labels never set `human_ground_truth_claim=true`.
- Existing 301 AI-assist rows produce characterization metrics explicitly.

### Recovery

- Workflow host recovery drill passes and reopens a successful run in a fresh
  CLI process.
- Workflow resume/retry/checkpoint tests pass.
- SQLite 16/16 recovery gate passes.
- Independent PostgreSQL template0 restore passes 2/2 reconciliation.

### Deployment gates

- `scripts/rip_llm_readiness.sh` checks Provider variables without printing secrets.
- `scripts/rip_production_probe.sh` validates an explicitly supplied HTTPS URL.
- `docs/release/P1-RECOVERY-SIGNOFF-TEMPLATE.md` records backup sign-off fields.

## Optional deployment additions

The following are useful later additions, not initial ingestion gates:

1. Configure `RIP_LLM_*` and a Provider secret only if external AI is enabled.
2. Add public HTTPS/DNS/certificate/reverse proxy only for external access.
3. Add scheduled PostgreSQL dump/offsite retention when data volume or recovery
   requirements justify it.
4. Add a larger real-corpus acceptance set as source data grows.

## Verification

- Full Rust workspace test: pass.
- Rust workspace Clippy: pass.
- Python worker suite: 237 pass, 11 skipped.
- Web visual/accessibility gate: 85 pass.
- Workflow recovery drill: pass.
- Independent PostgreSQL restore: pass.
- Protected `scripts/` tracked modifications: none.

## Not claimed

No claim is made for public HTTPS cutover or live external AI Provider E2E.
Formal offsite backup sign-off and a large real-corpus suite are deferred
deployment/scale additions, not startup gates. Complete human Ground Truth
is feedback calibration, not required for automated candidate operation.
