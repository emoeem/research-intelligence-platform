# Nuclei structured knowledge ingestion and search closure

Date: 2026-11-26
Project: Research Intelligence Platform (RIP)
Scope: dedicated Nuclei template ingestion, review gate, search facets, context visibility.

## Executive result

Nuclei templates now have a dedicated knowledge type, independent of the legacy RTIS POC write path. The complete ingestion pipeline was exercised against the installed RTIS/Nuclei corpus using a disposable in-memory platform database.

| Measure | Result |
|---|---:|
| Official Nuclei rows selected from RTIS | 14,111 |
| Parsed YAML templates | 14,109 |
| Invalid YAML retained with parse status | 1 |
| Body unavailable (size cap) | 1 |
| New KSE rows in disposable run | 14,111 |
| Initial review state | 14,111 DISCOVERED |
| Existing RTIS legacy rows in the same run | 15,084 |

No production RTIS database or production platform database was modified. The RTIS source database SHA-256 remained
`7b6083f8e1e84c9aff60a3d4f429f29cfc436ef1aafa7221d4d3cd945df641a7`.

## Architecture changes

### 1. Dedicated entry kind and migration

- Added `EntryKind::NucleiTemplate` to the domain vocabulary.
- Extended the plugin manifest parser registry with `nuclei_template = nuclei_yaml`.
- Added `migrations/032_nuclei_template_kind.sql`.
- Migration 032 rebuilds only `knowledge_source_entry` to extend its closed `entry_kind` CHECK domain. It copies all existing rows and preserves entry IDs, body hashes, metadata hashes, review states, provenance and timestamps.
- The legacy RTIS types `poc`, `skill_md`, and `method_case` continue to write to `knowledge_ingest`. No broad RTIS migration was introduced.

A migration regression test creates a version-31 database, inserts an approved legacy row, applies 032, verifies the old row unchanged, accepts `nuclei_template`, and rejects an unknown kind.

### 2. Dedicated parser and provenance

The new `nuclei_yaml` parser:

- Opens the RTIS `knowledge.db` read-only.
- Selects only rows matching all three source markers:
  - `category = 'nuclei'`
  - `source = 'nuclei'`
  - `source_url = https://github.com/projectdiscovery/nuclei-templates`
- Restores YAML only from the trusted local Nuclei template root.
- Keeps the raw YAML as the body when it is available.
- Stores the relative template path, not a machine-specific absolute path.
- Never inherits the RTIS plugin's source commit or license status as if those belonged to the upstream Nuclei repository.

Structured fields include template ID, name, authors, severity, description, tags, references, classification, protocol types, request-block count, matcher count, extractor count and flow presence.

The raw Nuclei template's own `verified` field is not translated into RIP verification evidence.

### 3. Conservative exceptional-file handling

The parser uses a 2 MiB maximum YAML body size.

The full local projection found:

- 14,109 parsed templates.
- One invalid YAML file: `network/cves/2004/CVE-2004-2687.yaml`.
- One oversized file: `http/technologies/wordpress/plugins/wordpress-plugin-detect.yaml` (10,079,081 bytes).

The oversized template is not loaded as a 10 MB body; its summary fallback and `body_unavailable` status remain explicit. The invalid YAML is retained with its raw source and `invalid_yaml` status. Neither is silently dropped or falsely marked parsed.

### 4. Review and context visibility

All Nuclei records are written to `knowledge_source_entry` with initial `review_status = DISCOVERED`.

The existing Context Broker plugin source already requires an active plugin version and `review_status = APPROVED`; a regression test now proves that:

- An approved Nuclei template can be retrieved as context.
- A discovered Nuclei template remains invisible to model context.

No template is approved automatically.

### 5. Structured search facets

Search index schema version was advanced to 3. Rebuilding the derived index adds:

- `nuclei_severity`
- `nuclei_protocols_json`
- `nuclei_cve_ids_json`
- `nuclei_cwe_ids_json`
- `nuclei_tags_json`

The FTS topics projection also includes Nuclei tags, protocols, severity, CVE and CWE identifiers. Search results expose the structured fields.

Exact filters are available through the SQLite search filter model and CLI:

- `nuclei-severity`
- `nuclei-protocol`
- `cve-id`
- `cwe-id`

A regression test combines severity + protocol + CVE + CWE filters, checks the structured result fields, and verifies that a mismatched CVE returns no result.

The unified search CLI accepts the same keys in `--sqlite-filters`.

### 6. Metadata change detection

The KSE metadata hash now covers the structured `meta_json` and source provenance fields. Re-ingesting an existing key compares both content and metadata hashes. Duplicate keys encountered within the same ingestion pass count as skipped, not as cross-run changes.

Existing rows are not silently overwritten. If parser semantics or source metadata change, publish a new plugin version or use an explicit reviewed rebuild/migration.

## Manifest contract

The RTIS manifest examples now declare:

- plugin example version `0.2.0`
- content kind `nuclei_template`
- source path `knowledge.db`
- parser `nuclei_yaml`
- rights policy `approved_only`
- maximum entries `20,000`

The installed Arch/BlackArch package metadata identifies the template package as `nuclei-templates 1:v10.5.2.r748.ga346d8f-1` and declares its license as MIT. The Nuclei source snapshot is therefore pinned to the package's upstream revision identifier `ga346d8f`; the manifest's Nuclei kind config carries that `source_commit`. The parser records MIT for this source only and includes the upstream LICENSE.md evidence URL plus the local package identifier in structured metadata. It does not inherit the RTIS plugin's own license status or commit.

This is package/revision-level evidence, not a blanket claim that every third-party template author contribution has independent provenance. Before production approval, retain the exact upstream snapshot/package provenance and re-check the applicable repository license at the pinned revision.

## Validation

The following package test suite passed together:

- `cargo test -p domain`
- `cargo test -p database`
- `cargo test -p plugin-registry`
- `cargo test -p plugin-runtime`
- `cargo test -p search-engine`
- `cargo test -p context-broker`
- `cargo test -p unified-search`

The combined command was:

```bash
cargo test -p domain -p database -p plugin-registry -p plugin-runtime \
  -p search-engine -p context-broker -p unified-search
```

Clippy passed for the same package set:

```bash
cargo clippy -p domain -p database -p plugin-registry -p plugin-runtime \
  -p search-engine -p context-broker -p unified-search --all-targets -- -D warnings
```

The full local-corpus projection and end-to-end disposable ingestion tests are intentionally marked ignored by default because they require the installed RTIS/Nuclei filesystem. Both were run manually and passed.

## Not performed

- Migration 032 was not applied to any persistent or production database.
- The production plugin was not upgraded or re-ingested.
- The persistent search index was not rebuilt; its schema version change will require the normal derived-index rebuild.
- No Nuclei entry was approved.
- No self-owned POC evidence or verification level was promoted.
- No commit, push, reset, checkout, clean, stash or rebase was performed.

The worktree already contained extensive unrelated changes before this batch; all were preserved.

## Controlled rollout checklist

1. Confirm the currently active RTIS plugin version; ensure the deployment version is strictly newer than it.
2. Apply migration 032 only to a development database copy first.
3. Stage the updated RTIS plugin with the Nuclei manifest declaration and ingest into that development copy.
4. Reconcile the expected 14,111 Nuclei entries and the two exceptional parser statuses.
5. Rebuild the derived search index to schema version 3.
6. Validate exact CVE/CWE/protocol/severity queries.
7. Review source commit and license evidence.
8. Approve only entries with sufficient review evidence; keep all other entries DISCOVERED.
9. Run the existing RTIS POC and context regression suite before production rollout.
