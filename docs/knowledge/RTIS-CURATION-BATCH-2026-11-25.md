# RTIS knowledge curation batch — 2026-11-25

## Scope

This batch adds a RIP-owned metadata overlay for 12 existing RTIS POC records (IDs 900053–900064) and 10 relationships. It does not rewrite or enrich the RTIS source SQLite database. The overlay is packaged with the RTIS knowledge plugin and applied only to the RIP `knowledge_ingest` projection.

## Evidence and lifecycle decisions

Evidence level and lifecycle are independent axes. The upstream lifecycle design is the authority; a legacy `poc.verified=true` flag is not enough to promote a record to K4 or `Verified`.

| POC IDs | Count | Evidence | Lifecycle | Rationale |
|---|---:|---|---|---|
| 900053–900060 | 8 | K1 | Researched | Upstream sources are identified. The historical `/tmp/tool-lab` logs cited by the old notes are absent from this workspace, so claimed local tests are not promoted to K2/K3. |
| 900061, 900063 | 2 | K2 | Researched | Original report/findings/log/screenshot support concrete observations. The inspected event store does not provide a structured `outcome.helped=true`, so Field-validated is not assigned. |
| 900062 | 1 | K1 | Researched | Own-account guest order creation is documented, but arbitrary `userId` and third-party account access were not tested. The legacy verified flag is retained with its caveat; no K2/K3 or impact escalation. |
| 900064 | 1 | K3 | Lab-validated | P5-9c local-range experiment has an evidence reference and rerunnable local template. It remains lab-scoped; this does not imply a real-world vulnerability or K4/Verified. |

The 10 relationships connect the local tool validation records to their method/source records and group the xingqyh observations under a common case. They preserve scope notes and do not imply that a method record proves a target finding.

## Ingestion implementation

- Added `integrations/rtis/curation/k_meta_overlay.json` and its maintenance/evidence policy in `integrations/rtis/curation/README.md`.
- Added `validate_overlay.py`, which checks IDs/codes against RTIS in read-only mode, rejects shadowing existing authoritative `k_meta`, validates evidence/lifecycle gates, and checks local evidence references.
- `plugin-runtime` loads the overlay from the plugin package. Existing RTIS `k_meta` rows remain authoritative; overlay entries only enrich the RIP projection. The source database is opened read-only.
- POC `verified_note` is preserved separately from the boolean `verified` field. Curated relationships are attached as metadata to related POC records.
- Existing generated method-case noise filtering remains in place (`__pycache__`, generator scripts, hidden generated indexes, empty files, stub notes).

## Isolated validation

Package version `knowledge.rtis-redteam 0.2.5` was staged in a disposable validation environment, based on a fresh copy of `migrations/dev-p6.db`. Production RTIS data and the default installed plugin were not upgraded.

Dry-run and real ingestion both completed successfully:

| Content kind | Items |
|---|---:|
| POC | 14,721 |
| Method cases | 238 |
| Skill Markdown | 125 |
| Total | 15,084 |

Total source body size reported by the ingest pipeline: 28,574,657 bytes. No rows were skipped in this clean 0.2.5 validation run. All 12 curated IDs were present with the expected conservative levels/lifecycles; relationship metadata and the 900062 arbitrary-user caveat were retained. Generated-artifact exclusion checks passed.

Targeted automated checks:

- `python3 integrations/rtis/curation/validate_overlay.py` — PASS (12 entries, 10 relations; source DB read-only).
- `cargo fmt --all -- --check` — PASS.
- `cargo test -p plugin-runtime --test knowledge_ingest_provenance -- --nocapture` — 5/5 PASS.
- `cargo test -p plugin-runtime --test sk3_source_sync -- --nocapture` — 6/6 PASS.
- `rp-plugin ingest ... --dry-run` — PASS.
- `rp-plugin ingest ...` against disposable DB — PASS.

The source DB SHA-256 at final inspection was `7b6083f8e1e84c9aff60a3d4f429f29cfc436ef1aafa7221d4d3cd945df641a7`. No write was issued against `integrations/rtis/knowledge.db`.

## Remaining work

1. Continue reviewing the remaining corpus in batches, prioritizing missing `k_meta`, low-evidence/high-impact records, duplicate clusters, and records with missing source/evidence references.
2. Reconcile overlay relations against the complete case/service graph before adding more edges; do not infer service entities from names alone.
3. Obtain the missing historical local tool-lab logs before promoting 900054–900060 beyond K1.
4. Keep 900061–900063 below Field-validated until a structured outcome record is available; preserve the arbitrary-user limitation on 900062.
5. Promote K3/Lab-validated to K4/Verified only after at least two distinct environments and human review are recorded.
6. Only after broader review and acceptance should the overlay be rolled into the normal plugin release path. This batch did not upgrade the production/default plugin installation.

## Worktree safety

The repository already contained extensive unrelated modified files. This batch did not reset, clean, stash, commit, or push them. The disposable validation package/database and existing unrelated worktree changes are retained for review.
