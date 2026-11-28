# Knowledge Ingestion Acceptance Report — 2026-11-28

## Executive result

The acceptance plan is **not complete**. Candidate-only vectorization and
review-only similarity persistence now pass an isolated PostgreSQL integration
smoke through the pinned embedding worker. The pinned parser image is absent, so
the real parser corpus cannot be executed. Human calibration, factual publication,
index refresh, and the Agent/Workflow return path remain unimplemented or
unverified.

No alternate image/model was used. No production database, DSN/systemd code, or
PostgreSQL disaster-recovery code was changed. No commit or push was made.
The baseline was `main` at `3055d7aa`; the worktree already had 36 modified or
untracked entries before this acceptance work. Those prior changes were
preserved.

## P0 — Environment and image identity

| Component | Pinned identity | Local result | Decision |
|---|---|---|---|
| `rip-parser` | `sha256:9a59eef52bf80aeb2562d9ca32cada455946c775fe92a0b0fb044a527dd8bf7a` | Not present in the default Podman store; `podman image inspect localhost/rip-parser:latest` reports image unknown | **Blocked**; no pull, rebuild, retag, or substitute |
| `rip-embed` | `sha256:bca7d0b4343f35afe199a781dbf3794c1d1afdf5924b12105d55dc0fb2291788` | Present; local inspect digest matches the JSON/YAML manifests | **Pass** |
| Test PostgreSQL | `localhost/rip-pg17:latest`, `sha256:229189e4c66a2eb8ddd1fcf4bc08ee4726cacd8aee7445ec3c6f1499a357ef45` | Present and used only as a temporary test container | **Pass** |

Environment observed: Podman 6.2.1, Linux amd64. The embed manifest pins
`bge-small-zh-v1.5`, ONNX Runtime 1.32.0, 512 dimensions, network mode `none`,
2 CPU, and 2 GiB memory. The host model directory contains the pinned model
artifacts, and the image was created on 2026-11-27.

The parser image was not found in the repository, `rip-data`, the checked
container cache, or the checked backup paths. This was a bounded local search,
not proof that no copy exists elsewhere. No image build was attempted because
the approved image digest could not be preserved or verified from an archive.

## P1 — Parser Golden Corpus

**Blocked; no parser-format acceptance is claimed.**

- No curated, hashed corpus of at least 40 real documents is present.
- The repository search found 385 Markdown files across the searched `docs/`,
  `crates/`, and `tests/` trees, but they have not been selected, source-reviewed,
  hashed, or converted into expected-output annotations.
- `crates/document-router/tests/fixtures/` contains parser fixtures such as
  `sample.pdf`, `sample.docx`, `sample.html`, Markdown, and ZIP security cases.
  These are existing test fixtures, not a 40-item real-world Golden Corpus.
- No local Nuclei YAML corpus or external GitHub source corpus was validated.
- The parser image absence prevents validation of structure/content completeness,
  source offsets, parser diagnostics, PostgreSQL persistence, and real-file
  idempotence.

The earlier `parser_pipeline` PostgreSQL tests use synthetic IR fixtures. Their
6/6 result is recorded as an integration result only; it is **not** parser
acceptance and does not satisfy this gate.

## P2 — Embedding runtime and semantic dedup

### Real worker runtime smoke — passed

Command shape:

```text
RP_TEST_PG_DSN=<disposable PostgreSQL> cargo test --offline -p intel-db \
  --test gov_semantic -- --test-threads=1 --nocapture
```

Result: 1/1 test passed through `EmbedWorkerProvider` → Tool Engine →
`podman_host`, using the pinned `rip-embed` image and a disposable PostgreSQL
container. The test now uses a unique system temporary directory rather than
the pre-existing `rip-data/parse/test-ops.db` and `test-ws`.

| Check | Observed |
|---|---|
| Embedded records | 3 |
| Vector dimension | 512 for all 3 records |
| Model identity | `bge-small-zh-v1.5` for all 3 records |
| Worker version | `0.2.0` |
| Paraphrase smoke cosine | 0.8358 |
| Unrelated smoke cosine | 0.4949 |
| Smoke threshold | 0.80 |

The two cosine values are a single card-only smoke pair, not a benchmark. They
do not establish candidate-level precision, recall, Duplicate/Related/Distinct
classification quality, false-merge rate, or missed-duplicate rate.

The embedding response is now checked before persistence for the expected model
and backend, 512 dimensions, exact batch/vector lengths, finite values, and
L2-normalization. Unit coverage includes mismatched model and invalid
normalization.

### Candidate semantic path — implementation smoke passed; quality gate not passed

- V0018 adds Nuclei-only candidate vectors and persisted, workspace-scoped
  review-only similarity suggestions. `rp-ingest semantic-scan` requires an
  explicit threshold and uses `EmbedWorkerProvider` → Tool Engine →
  `podman_host`; no alternate model or image was used.
- Candidate details expose the matched candidate, cosine score, threshold,
  model/worker version and review-only flag. The scanner skips rejected/deferred
  candidates and conflicting or invalid metadata, hashes its embedding input,
  batches no more than 64 inputs, and rejects workspaces with more than 5,000
  eligible Nuclei candidates.
- Against the disposable PostgreSQL and exact pinned `rip-embed`, the
  `parser_pipeline` ingestion integration embedded two candidate vectors,
  persisted 512 dimensions/model `bge-small-zh-v1.5`/worker `0.2.0`, generated
  a review-only suggestion for the fixture pair, and passed an idempotent
  rescan. The fixture is synthetic IR and is **not** parser or quality
  acceptance. The test threshold `0.70` is a smoke assertion, not a calibrated
  value. It also injected a missing embed manifest (no partial vector writes;
  retry succeeded), then an insert constraint failure (the suggestion
  transaction rolled back and preserved the prior result; retry succeeded).
- Verification command:

  ```text
  RP_TEST_PG_DSN=<disposable PostgreSQL> cargo test --offline -p intel-db \
    --test parser_pipeline -- --test-threads=1 --nocapture
  ```

  Result: 6/6 passed; `container_parse_end_to_end` reported SKIP for the absent
  parser image. The candidate semantic branch did run with the pinned embed
  image. `cargo test --offline -p intel-db --lib` passed 41/41;
  `gov_semantic` passed 1/1; Clippy (`-D warnings`), package rustfmt check,
  `git diff --check`, and `rp-ingest --help` passed.
- No independently labeled Duplicate/Related/Distinct set exists.
- There are no independent calibration and test splits; threshold selection,
  precision, recall, false-merge rate, and missed-duplicate rate are **N/A**.
- The KnowledgeCard smoke remains separate evidence for the shared worker path;
  neither smoke proves candidate-level classification quality.
- No automatic merge or deletion was performed.

## P3 — Knowledge ingestion end-to-end

**Not passed.** Existing ingestion supports evidence-linked CVE/CWE mentions and
review-only Nuclei metadata candidates. These are not verified factual
vulnerability claims. Without real parser inputs, a defined factual candidate
type, and a guarded publication/index path, this run did not submit and publish
a real GitHub source or demonstrate the complete new/duplicate/update/conflict/
review/publish/re-index/replay lifecycle.

The `parser_pipeline` suite passed 6/6 against disposable PostgreSQL; its
`container_parse_end_to_end` gate reported SKIP because the exact parser image is
absent. The ingestion fixture verifies database/pipeline logic only, not parser
input acceptance or publication. The selected candidate-semantic integration
case also passed through the real pinned embed worker.

## P4 — Failure and recovery

| Failure case | Result |
|---|---|
| Parser process exit/retry | Not run; pinned parser image unavailable |
| Embedding unavailable/retry | Pass for missing-manifest failure; zero vector writes before retry. Actual worker process termination not tested |
| PostgreSQL interruption mid-ingestion | Not run; suggestion-table insert constraint failure did verify transaction rollback/retry |
| Candidate semantic rescan/idempotency | Pass; no failure injection |
| Duplicate/replay on real parsed source | Not run; no real parser corpus |

The disposable PostgreSQL container used for the successful integration tests
was stopped and removed. Test-owned unique temporary runtime directories were
removed by their tests. The existing `rip-data/parse/test-*` content was left
untouched.

## P5 — Gate summary and remaining work

| Gate | Status |
|---|---|
| Image identity / embed runtime | **Partial pass** — embed pinned image verified; parser pinned image unavailable |
| Parser Golden Corpus (40+) | **Blocked** |
| Real embedding vector generation/persistence | **Pass** |
| Candidate semantic path | **Implementation smoke passed** — no human labels or calibrated metrics |
| Candidate semantic dedup benchmark | **Not passed** — no independently labeled data or test split |
| Real-source ingestion/publication loop | **Not passed** |
| Failure/recovery suite | **Not run** |
| Documentation / handoff update | **Pass** |

Next acceptance work is gated on:

1. Recovering the exact approved parser image (or obtaining approval for a
   reproducible rebuild whose produced digest is explicitly accepted).
2. Curating and hashing at least 40 real, source-reviewed samples with format,
   license/source, expected structure, and position annotations.
3. Having reviewers label candidate pairs as Duplicate, Related, or Distinct,
   then freezing independent calibration and test splits before reporting
   metrics.
4. Defining evidence requirements for factual candidates before enabling
   publication and index refresh.
5. Running parser-process exit and PostgreSQL connection/transaction interruption
   tests against disposable resources; the candidate embed manifest and
   suggestion-transaction failure cases are covered, but do not represent those
   broader interruption scenarios.

The frozen DSN loader, systemd startup/recovery, and PostgreSQL backup/restore
modules remain out of scope and unchanged.

## Final regression verification

- `cargo test --offline --workspace` passed after updating the M4 search-index
  assertion to account for resource, skill, and tool projections and allowing
  the SK3 test to recognize the network guard's `private_network_blocked`
  rejection: 1,485 passed, 0 failed, 6 explicitly ignored. The ignored tests
  are two opt-in latency benchmarks, three installed-corpus audits, and one
  Chromium-sensitive browser-capture test.
- `cargo fmt -p intel-db -p tool-engine -- --check` and `git diff --check`
  passed.
- `cd web && npm run test:visual:gate` passed: production TypeScript/Vite build
  and all 85 Playwright visual/accessibility tests.
- The repository-wide Web `format:check` still reports formatting differences
  in the unchanged `pnpm-lock.yaml` and `tests/m3-components.test.tsx`; these
  files were not reformatted as unrelated to ingestion acceptance.
- These regression results do not change the blocked acceptance decisions
  above. In particular, they do not supply the missing pinned parser image,
  real Golden Corpus, human semantic labels, factual publication/index loop,
  or parser/PostgreSQL interruption drills. The project is not declared
  production-ready, and no commit or push was made.
