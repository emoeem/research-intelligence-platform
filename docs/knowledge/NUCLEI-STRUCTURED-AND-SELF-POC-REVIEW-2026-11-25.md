# RIP knowledge curation — Nuclei structure and self-owned POC review

Date: 2026-11-25

## Scope and safety boundary

This batch is an additive code and audit step. The RTIS source database
`integrations/rtis/knowledge.db` was opened read-only. No production database
rows, installed plugins, or user data were changed. The current worktree already
contains extensive unrelated changes; no reset, checkout, clean, stash, commit,
or push was performed.

## Nuclei template baseline

RTIS contains 14,111 rows with `source=nuclei` and `category=nuclei`.
All 14,111 stored absolute template paths currently resolve beneath
`/usr/share/nuclei-templates`; all use the `.yaml` extension.

The installed template tree contains 14,389 YAML files. The local parser audit
reported:

| Result | Count | Treatment |
|---|---:|---|
| Parsed | 14,366 | Structured metadata can be emitted |
| Missing `info` | 21 | Not ordinary detection templates; includes profiles and Wappalyzer mapping |
| YAML parse failure | 1 | Preserve raw source; inspect parser compatibility before excluding |
| Over 2 MiB | 1 | Keep the existing summary fallback; do not ingest the 10 MB body |

The 21 files without `info` are profile/mapping files, not normal Nuclei
single-template documents. The one parse failure is
`network/cves/2004/CVE-2004-2687.yaml`. The oversized item is
`http/technologies/wordpress/plugins/wordpress-plugin-detect.yaml`
(10,079,081 bytes).

The RTIS snapshot has 278 fewer YAML files than the current installed tree.
This is a source-snapshot difference, not a missing-path failure. Do not silently
add those 278 rows: source revision and stable IDs must be reconciled first.

## Structured parser change

Files changed in this step:

- `crates/plugin-runtime/Cargo.toml`: reuse workspace `serde_yaml_ng`.
- `crates/plugin-runtime/src/ingest.rs`: derive a compact `nuclei_template`
  object in POC metadata when the trusted local YAML body was recovered.

Fields extracted:

- template ID, name, author, severity, description;
- tags and reference URLs;
- raw classification object (CVE/CWE/CVSS/CPE/EPSS fields where present);
- template metadata object;
- protocol keys, request-block count, matcher count, extractor count, flow presence.

The original YAML remains the body. Parse failures and unavailable bodies are
explicitly represented by `parse_status`; summaries are never parsed as YAML.
No Nuclei `verified` flag is translated into RIP verification.

## Important retrieval integration gap

This parser change is not yet the end-to-end searchable Nuclei implementation.

The current RTIS POC path writes to legacy `knowledge_ingest` and stores
structured fields in `meta_json`. Current `search-engine` indexing and
`ContextBroker::PluginSource` consume `knowledge_source_entry`, with the latter
requiring `review_status='APPROVED'`; they do not read legacy
`knowledge_ingest` rows. Therefore the new JSON fields are not yet first-class
search filters/facets and this code-only step must not be reported as a completed
searchability upgrade.

Next architecture step: route Nuclei YAML into the existing reviewable
`knowledge_source_entry` flow as a dedicated `nuclei_template` content kind
(or an equivalent explicit source), initially `DISCOVERED`. Preserve
plugin/version/source commit/path/hash provenance. Index title, description,
tags, classification identifiers, severity, protocol and raw body; only expose
approved entries to the model/context layer. Keep execution of templates behind
the existing tool/verification safety gates.

The legacy `knowledge_ingest` uniqueness key is based on plugin ID, plugin
version, content kind and body hash. A metadata-only change will not update an
already-ingested row at the same plugin version. The next ingestion release
must use a new plugin version or an explicit migration/rebuild path; do not
assume rerunning the same version updates `meta_json`.

## Self-owned POC baseline

Read-only query results for `source='self'`:

| Measure | Count |
|---|---:|
| POC rows | 139 |
| Legacy `verified=1` | 129 |
| Non-empty content | 135 |
| Non-empty usage | 110 |
| Non-empty description | 133 |
| Rows with a path | 116 |
| Paths that do not resolve after workspace-root mapping | 9 |
| Existing RTIS `k_meta` rows in the whole database | 28 |

The nine unresolved self-POC paths are IDs:
`10, 16, 37, 42, 46, 52, 81, 83, 87`.
Do not rewrite these paths by filename guess. Reconcile each against the actual
repository tree, then record a correction in a RIP-owned overlay or a reviewed
source update.

The ten self-sourced rows with `verified=0` are ID 47 and IDs 900052–900060.
Historical `/tmp/tool-lab` logs referenced by 900052–900060 are absent from
this workspace. In particular, 900057–900060 have no POC body. Keep their
claims at the conservative evidence level already chosen; do not promote them
based on `verified_note` text alone.

The 2026-11-25 overlay already covers IDs 900053–900064 and 10 relationships:
900053–900060 remain K1, 900061/900063 are K2, 900062 remains K1 with its
arbitrary-user limitation, and 900064 remains K3/Lab-validated. This batch
does not alter those decisions.

## First self-owned review queue (not auto-promoted)

Prioritize content/evidence review rather than tag expansion:

| ID | Candidate | Why review it |
|---|---|---|
| 4 | List pagination / server-side authorization boundary | Reusable method; existing script and field evidence |
| 12 | Multi-tenant authorization-prefix differential | Reusable cross-tenant method; existing script and field evidence |
| 18 | Springdoc allowlist exposure | High-impact API-contract exposure; script and recorded observations |
| 20 | Three-state application-response oracle | Reusable route-existence triage method |
| 29 | Four-state authentication behavior probe | Reusable authentication implementation classifier |
| 72 | Client-key bootstrap / signature-key exposure | Critical-impact case; redact secrets and review evidence scope before approval |
| 900010 | BFF read-only batch baseline | Reusable method with documented failed-bypass boundaries |
| 900043 | Unsigned identity-cookie case | Critical-impact case; isolate sensitive evidence and verify source references |
| 900047 | Client-generated session credential rule | Critical-impact reusable rule; confirm evidence chain and false-positive boundaries |

ID 79 already has a K3 Field-validated `k_meta` entry and should be treated as
a quality-review/reuse candidate, not duplicated as a new metadata record.

For every candidate, review the source file, exact evidence references, affected
scope, negative controls, stop conditions, reproducibility, and secret/PII
redaction. Do not promote a lifecycle or evidence level from the legacy boolean
alone. Keep case-specific findings distinct from reusable methods and rules.

## Validation performed

- `python3 integrations/rtis/curation/validate_overlay.py` — PASS (12 entries, 10 relations; RTIS DB read-only).
- `cargo test -p plugin-runtime` — PASS; 77 unit/integration tests passed, 1 manual corpus-audit test is ignored by default. The installed-template smoke test passed.
- `cargo test -p plugin-runtime audit_installed_nuclei_template_corpus -- --ignored --nocapture` — PASS; 14,366/14,389 YAML files parsed.
- `cargo clippy -p plugin-runtime --all-targets -- -D warnings` — PASS.
- `cargo fmt -p plugin-runtime` — applied.
- `git diff --check` — PASS for the files changed in this batch.
- RTIS source DB SHA-256 remains `7b6083f8e1e84c9aff60a3d4f429f29cfc436ef1aafa7221d4d3cd945df641a7`, matching the previous curation report; no source DB write occurred.

No full workspace tests were run because the workspace has a large unrelated
active worktree diff.
