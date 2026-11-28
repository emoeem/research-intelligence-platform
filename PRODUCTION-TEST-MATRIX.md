# RIP Production Test Matrix

> Status: P0 baseline
> Unit success does not satisfy real-environment or production acceptance.

| Track | Implemented | Unit/local | Real environment | Production | P0 next gate |
|---|---:|---:|---:|---:|---|
| Rust workspace | Yes | 1,485 pass / 0 fail / 6 ignored | No formal production run | Not accepted | Repeat at P1/P7 exit |
| Python workers | Yes | Worker suites have been observed passing | Embedding worker passed with pinned image | Not accepted | Real parser and interruption |
| SQLite migrations | Yes | Migration smoke evidence | Local independent backup/restore evidence | Not accepted | Fresh DB, upgrade, repeat, failure recovery |
| PostgreSQL migrations | Yes | Disposable PG integration 4/4 in prior evidence | Temporary dump/restore 2/2 | Formal policy open | Repeat in isolated environment; document RTO/RPO |
| Web visual/API | Yes | Build + 85 Playwright pass; API selftests | Browser evidence exists | Not accepted | Real create/update/delete/status/error operations |
| Parser PDF/OCR/Office | Partial | Synthetic IR/integration fixture pass | Blocked: parser image absent | Not accepted | 40+ real Golden Corpus |
| Document IR | Partial | Schema and synthetic fixtures | Not verified | Not accepted | Real structure/evidence/offset validation |
| Ingestion idempotency | Partial | Candidate and replay smoke pass | No real parser source replay | Not accepted | Real duplicate/retry/cancel/restart |
| Knowledge candidate/review | Implemented | Batch review and candidate semantic tests | No human/calibrated pass | Not accepted | Independent labels and factual publication |
| Retrieval | Implemented | Local evaluation tooling exists | No human relevance metrics | Not accepted | Recall@K/MRR/NDCG on frozen labeled set |
| AI Provider E2E | Partial | Mock/local selftests | No real Provider | Not accepted | Live Provider research chain |
| Tool/MCP security | Implemented | Protocol/parity/unit tests | Limited real container evidence | Not accepted | Permission, timeout, cancel, audit, hostile parameters |
| Workflow recovery | Implemented | Local resume/pause/retry tests | Interrupted real service not verified | Not accepted | Restart/recovery/cancel propagation |
| Backup/restore | Implemented for SQLite | Local roundtrip evidence | PG temporary restore pass | Formal backup policy open | Independent environment and data reconciliation |
| POC/Nuclei | Governance/partial | Static and guard tests | Not run on authorized targets | Not accepted | Authorized isolated execution |

## Required P7 real scenarios

1. PDF, DOCX, PPTX, XLSX, HTML, Markdown, TXT and image ingestion.
2. OCR for digital and scanned PDFs.
3. AI extract, human review, publish, evidence trace and re-index.
4. Keyword, semantic and hybrid retrieval with evidence citations.
5. Real AI Provider research and controlled tool use.
6. Mid-workflow interruption and post-service-restart recovery.
7. Independent database/file restore with reconciled counts and samples.
