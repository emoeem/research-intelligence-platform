# M2 — Nuclei template lifecycle baseline and minimum plan

Status: **Pre-implementation audit snapshot** — see [M2 closeout](./M2-CLOSEOUT-AND-REMAINING-ISSUES.md) for implemented behavior and current acceptance.

## Scope and safety

This audit covers the existing Nuclei YAML intake, KSE persistence, source pin, search projection and tests. Template files were only parsed as data; no template, PoC or scanner was executed, and no target was contacted. The RTIS source database and platform databases were not changed.

Prior files named `M2_REPORT.md` describe earlier Rust foundation, source-monitor or UI work; they are historical input, not acceptance for this governance program.

## Current implementation

- `knowledge.nuclei-templates` is a dedicated `nuclei_template` KSE source with `approved_only`, a fixed 40-hex `source_commit`, and a separate metadata-only RTIS projection.
- `plugin-runtime` uses the maintained, vendored `serde_yaml_ng` YAML parser. It reads only official-source rows and only YAML files canonicalized beneath `/usr/share/nuclei-templates`, with a 2 MiB per-file limit.
- The parser retains raw YAML when available and extracts template ID, severity, tags, references, classification/CVE/CWE, protocol names, request-block count and matcher/extractor counts. KSE stores source URL/path/commit, content and metadata hashes, plugin version and review state. New entries start `DISCOVERED`.
- Generic KSE identity is `(plugin_id, plugin_version, entry_kind, item_key)`; Nuclei `item_key` is normally the upstream relative path. Re-ingest of a changed same-version row reports a change but intentionally does not overwrite the old record. A new plugin version retains another source snapshot.
- Missing template files produce `body_unavailable`, but fallback usage/description text may still populate the generic KSE body and its `content_sha256`. That hash therefore is not always an actual template-file hash.
- Matcher and extractor contents are not individually projected; only counts and raw YAML are available. `parse_status` describes parsing/schema conditions, but there is no explicit detection-effectiveness state.
- `upstream_missing` exists in the KSE schema but intake always inserts `0`; the existing SK2 test note explicitly defers removal reconciliation. Search currently selects approved rows without filtering that flag.

## Read-only corpus audit

The existing ignored static audit was run against the installed `/usr/share/nuclei-templates` tree. It only reads/parses YAML and exits after reporting:

| Class reported by current audit | Count |
|---|---:|
| YAML files scanned | 14,389 |
| Parsed with `info` | 14,366 |
| YAML parse failures | 1 |
| Missing `info` | 21 |
| Over 2 MiB and not parsed | 1 |

The 21 `missing_info` files are profile/configuration YAML and the Wappalyzer mapping, not template records. The oversized file is `http/technologies/wordpress/plugins/wordpress-plugin-detect.yaml` (10,079,081 bytes); the parser correctly refuses it under the configured bound. The one parse failure is `network/cves/2004/CVE-2004-2687.yaml`; parser diagnostics currently discard the underlying YAML error, so the precise reason remains unverified.

The post-implementation static audit uses more specific outcome classes (including missing template IDs and oversized files); its corrected counts and file-level findings are recorded in the M2 closeout. The table above preserves the original pre-implementation audit's classifications.

Host package metadata reports `nuclei-templates 1:v10.5.2.r748.ga346d8f-1`. The directory is not a Git checkout. Its package suffix is consistent with the leading eight characters of the manifest's pinned commit `a346d8f00c61bf95bbd30365f12cf923be51ba3d`, but this is not proof that every installed file exactly matches that full commit. Do not describe the local corpus audit as an audit of the pinned upstream tree.

## Gaps against M2

1. A distinct hash for the actual raw template is absent when the file is unavailable; the generic body hash can describe fallback text instead.
2. Matcher/extractor payloads are not queryable independently of the raw YAML.
3. There is no explicit `NOT_TESTED` detection-effectiveness state alongside YAML syntax/metadata status.
4. Upstream removals are not reconciled; stale approved rows can remain searchable because `upstream_missing` is not set or filtered.
5. Source snapshots retain plugin versions and commit values, but same-version changes are only reported and not persisted as a new version. The version/commit immutability rule needs a focused regression.
6. The full-corpus audit conflates non-template YAML with malformed templates and omits the detailed parser error.

## Minimum implementation plan

1. Keep `knowledge_source_entry` as the authority and reuse the existing vendored YAML parser, source pin, hashes, plugin-version history and `upstream_missing` field. No new parser dependency or schema migration is justified by this audit.
2. Extend Nuclei metadata with a raw-template SHA-256 (null when unavailable), structured matcher/extractor values, a template-shape projection status (not an upstream schema validator), and an explicit static detection-validation status of `NOT_TESTED`. Preserve the existing generic KSE body/hash behavior and do not infer effectiveness from parsing.
3. Treat each plugin version and full source commit as an immutable snapshot. Add tests proving new versions retain changed template hashes while old rows remain intact; do not silently overwrite same-version history.
4. Reconcile a complete Nuclei source inventory transactionally: set `upstream_missing` for absent item keys and clear it for present keys without deleting rows or changing human review decisions. Filter missing entries from current Search projections. Do not infer removals from failed/partial ingestion.
5. Improve static corpus reporting so configuration YAML, malformed template YAML and oversized files are distinguished, while retaining the no-execution boundary.
6. Verify with synthetic YAML and in-memory/disposable fixtures, then publish the M2 closeout and remaining issues. No target execution or runtime effectiveness claim is in scope.

## Regression test seams and cases

- Parser unit seam: `nuclei_template_metadata` in `plugin-runtime` — exact raw-byte hash, matcher/extractor payloads, and independent syntax/schema/detection-validation states.
- Intake seam: `ingest_knowledge_source_with_opts` through its shared implementation — fallback body does not acquire a template hash; a later complete active source snapshot marks removed paths missing without deleting rows or changing review decisions; reappearing paths clear the flag.
- Search seam: `SearchIndex::build` — an approved but upstream-missing template is absent from the rebuilt index.
- Boundary case: a truncated or incomplete snapshot must not mark any prior item missing.

## Acceptance boundary

M2 can establish syntax, metadata, source-pin, content-hash and upstream-presence facts. It cannot establish that a detector is effective against any product or target. Actual detection validation remains `NOT_TESTED` until separately authorized, isolated lab evidence exists.
