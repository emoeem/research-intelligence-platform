# M4 cross-source relevance review — batch 001

Status: **USER ACCEPTED BY SAMPLE REVIEW — per-pair grading explicitly waived; formal relevance metrics remain unavailable**
Created: 2026-11-27
Index: schema v7, server-managed derived index, 3,789 documents
Candidate pool: **140 query-document pairs** (14 queries × complete top-10)

## Annotation tool decision

Argilla 2.8.0 was evaluated first. Its SDK supports fields, label questions, advisory suggestions, record import/export, and would fit this task technically. However, the upstream repository now explicitly states that feature development has stopped and only bug fixes/security patches are expected. For a new workflow, RIP uses **Label Studio 1.23.2** instead. Label Studio's 1.23.2 stable release includes security fixes; the newer 1.24.0 entries are nightly development builds and are not used here. Argilla is not deployed and is not a runtime dependency.

- Label Studio source/release: https://github.com/HumanSignal/label-studio/releases
- Label Studio task import: https://labelstud.io/guide/tasks
- Label Studio annotation export: https://labelstud.io/guide/export
- Argilla maintenance notice: https://github.com/argilla-io/argilla

## Files

- Original review sheet: test-results/m4-cross-source-qrel-review-batch-001.csv
- AI suggestion audit: test-results/m4-cross-source-qrel-review-batch-001.ai-suggestions.json
- Label Studio tasks with advisory predictions: test-results/m4-label-studio-batch-001/tasks.json
- Label Studio labeling config: test-results/m4-label-studio-batch-001/labeling-config.xml
- Label Studio batch manifest: test-results/m4-label-studio-batch-001/manifest.json
- Builder: scripts/security-intel/build_m4_label_studio_batch.py
- Annotation importer: scripts/security-intel/import_m4_label_studio_annotations.py
- Evaluator: scripts/security-intel/evaluate_m4_qrels.py
- Offline run comparison / RRF experiment: scripts/security-intel/compare_m4_ranking_runs.py
- Toolchain pins: scripts/security-intel/requirements-m4-evaluation.txt
- Review UI pins: scripts/security-intel/requirements-m4-review-ui.txt
- Review UI launcher: scripts/security-intel/start_m4_label_studio.sh
- Adapter tests: scripts/security-intel/test_m4_qrel_tools.py

## Coverage

| Source system | Pairs |
|---|---:|
| CISA KEV | 102 |
| Nuclei templates | 29 |
| RTIS curated POC metadata | 8 |
| Platform resources | 1 |
| **Total** | **140** |

The 14 queries cover SQL injection, RCE, authentication bypass, directory traversal, SSRF, XSS, command injection, deserialization, file upload, Spring Cloud Gateway, Docker/container escape, API rate-limit bypass, Nginx path traversal and Jenkins script-console security.

## Review UI — already provisioned

Label Studio 1.23.2 is installed in an isolated Python 3.13.15 virtual environment. The Docker Hub image pull repeatedly failed with truncated EOF responses, so the UI uses the official PyPI package rather than an unverified third-party container image.

- URL: http://127.0.0.1:8062
- systemd user service: rip-m4-label-studio
- Data directory: ~/.local/share/rip-m4-label-studio/data
- Python environment: ~/.cache/rip-m4-label-studio-venv
- Binding: 127.0.0.1 only; no external interface or public port
- SSRF protection: enabled
- Backend/frontend Sentry DSNs: empty; external error telemetry disabled
- Latest-version network check: disabled to keep startup independent of PyPI availability
- Local owner login: rip-m4-reviewer@example.com
- Login password is stored in ~/.local/share/rip-m4-label-studio/credentials.env (mode 0600). The file contains only the local username and password; the temporary API token was deleted and legacy token authentication is disabled.

The project has already been created and the batch imported:

| Check | Verified value |
|---|---:|
| Project ID | 1 |
| Project title | RIP M4 Cross-source Qrels — Batch 001 |
| Tasks | 140 |
| Advisory predictions | 140 |
| Human annotations | 0 |

Task 1 was read back through the API before API credentials were removed: candidate Q01-R01, query Q01, rank 1, one prediction with grade 3, and zero human annotations. The project API reported 140 tasks and 140 predictions. The user does not need to create a project or import the tasks manually.

### Historical review UI instructions (not required after the user acceptance waiver)

The following setup instructions are retained only for a future judged batch. No further review clicks are required for batch 001 under the user's acceptance decision. To reuse the UI for a future batch: open http://127.0.0.1:8062 on the CachyOS host.
2. Sign in as rip-m4-reviewer@example.com. To view the generated local password, run:

       cat ~/.local/share/rip-m4-label-studio/credentials.env

3. Open the project RIP M4 Cross-source Qrels — Batch 001 (project ID 1).
4. Review the 140 tasks. AI predictions are advisory pre-annotations only; do not accept a prediction without checking the result against the query.

Service controls:

       systemctl --user status rip-m4-label-studio
       systemctl --user stop rip-m4-label-studio
       systemctl --user start rip-m4-label-studio

### Human grade scale

| Grade | Meaning |
|---:|---|
| 0 | Irrelevant |
| 1 | Weakly related / incidental product or ecosystem overlap |
| 2 | Relevant supporting result, but not an exact match |
| 3 | Directly relevant to the query |

Use the optional rationale field when a result is ambiguous, especially when a shared product/ecosystem name could be mistaken for the queried vulnerability class.

### Export and import the human labels

1. In Label Studio, export the project as JSON, including all tasks and annotations.
2. Save the export as test-results/m4-label-studio-batch-001/export.json.
3. Run:

       python3 scripts/security-intel/import_m4_label_studio_annotations.py \
         --input test-results/m4-label-studio-batch-001/export.json

4. Check the printed judged_pair_count and unjudged_pair_count.
5. Once all 140 pairs have a human grade, run:

       ~/.cache/rip-m4-evaluation-venv/bin/python \
         scripts/security-intel/evaluate_m4_qrels.py

The evaluator writes TREC qrels/run files under test-results/m4-trec/ and computes NDCG@10, RR@10 (MRR), P(rel=2)@5/10, AP(rel=2)@10 and Judged@10 through ir-measures. It refuses to calculate metrics while any human grade is missing.

## User acceptance decision (2026-11-27)

The user inspected a sample of the 140 candidate results, confirmed that the batch content looked correct, and explicitly accepted the batch without submitting 140 individual annotations. Requiring 140 separate clicks was considered disproportionate for this acceptance pass.

This decision closes the operational review requirement for this batch as a **sampled user acceptance / grading waiver**. It does not create synthetic human grades and does not imply that every pair received an individual relevance judgment.

- Batch size: 140 pairs across 14 queries.
- Saved Label Studio annotations: 0.
- Human-grade CSV values: 0/140.
- AI suggestions remain advisory only and are not promoted to human labels.
- The qrel evaluator remains blocked and metrics remain null. No NDCG, MRR, MAP, precision or recall claim is made from this waived batch.
- Future ranking-quality evaluation that requires numerical relevance metrics must use a smaller, explicitly agreed judged sample or a separately completed qrel batch.

The user acceptance decision is recorded in docs/knowledge-governance/M4-USER-ACCEPTANCE-2026-11-27.md.

## AI suggestion boundary

The ai_suggested_grade, ai_suggestion_reason and ai_suggestion_confidence columns are advisory LLM triage based on query text, indexed title/description and source type. They are not human labels, are not calibrated probabilities, and are never read as qrels by the evaluator. All 140 human_grade fields remain blank in the original batch by design; this is consistent with the user's explicit grading waiver.

The Label Studio tasks place AI suggestions in the predictions field, separate from human annotations. The importer reads only completed human annotations, never prediction values. Conflicting annotator grades cause the import to fail without modifying the CSV.

## Scope and limitations

This is a pooled top-10 benchmark for the listed 14 queries, not a random sample of the full corpus. Results must not be generalized beyond the reviewed query set. RTIS POC rows expose metadata only; the search index intentionally omits POC body/usage text, so consult linked project evidence if the title alone is insufficient. No POC body was read and no exploit was executed to build this batch.
