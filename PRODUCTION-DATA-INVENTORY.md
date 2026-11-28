# RIP Production Data Inventory

> Status: P0 read-only inventory
> This file does not move, modify, backup, or classify data for deletion.

## 1. SQLite databases

| Path | Size | Purpose/class | Current control |
|---|---:|---|---|
| `migrations/platform.db` | 4,845,568 bytes | Main referenced platform DB | Migration/backup/restore owner and formal PG/SQLite role not signed |
| `migrations/platform-fullbatch.db` | 3,751,936 bytes | Full-batch derived snapshot | Runtime artifact; not production source |
| `migrations/platform-dryrun.db` | 1,118,208 bytes | Dry-run derived snapshot | Fresh-environment test only |
| `migrations/platform-governance.db` | 53,248 bytes | Governance/test snapshot | Test data |
| `migrations/platform-search.db` | 3,944,448 bytes | Search/test snapshot | Test data |
| `migrations/dev-*.db` | Variable | Development/test fixtures | Must not be mistaken for production data |
| `integrations/rtis/knowledge.db` | 27 MB | RTIS knowledge DB | Production/derived source discipline; do not modify in P0 |

## 2. Integrity snapshot

SHA-256 values observed 2026-11-28:

```text
3f090e6610c64531c349c6ca8bc6571757a559fd80e66ead5f1abc94e8cea911  migrations/platform.db
4b2d32090499cbb10d317a48b535fd52e00293703de896df3e0d018d64487bef  migrations/platform-dryrun.db
eb81d37b193823f38675d8422a34e0bb12642235d402c6f55e2c238f27a977a1  migrations/platform-fullbatch.db
afe41186f6f7704adf3e62e2b06932d8641cca8a023d623e43ff772964a76cef  migrations/platform-governance.db
2da952cad715e179ee2626b79b55784ab014897e270d18c65c05d1587dd43d54  migrations/platform-search.db
7b6083f8e1e84c9aff60a3d4f429f29cfc436ef1aafa7221d4d3cd945df641a7  integrations/rtis/knowledge.db
```

The SHA lines must be regenerated if data files change; values are observation
evidence, not approval to use or overwrite them.

## 3. PostgreSQL

- Test-only containers observed: `rip-pg17`, `rip-m4-personal-pg`.
- Temporary restore evidence used an isolated template0/database and 2/2
  row reconciliation.
- No formal production PG database/backup location/key owner is recorded as
  accepted by P0.

## 4. Migration and data classes

- SQLite migrations: 33 files, including 001-033, in `migrations/`.
- PostgreSQL migrations: 18 `V0001`-`V0018` files in `migrations_pg/`.
- Data classes:
  - Raw source snapshot and document/revision assets.
  - Candidate knowledge, evidence and review decisions.
  - Published knowledge/card/entity/relation data.
  - Tool/workflow/agent traces and audit data.
  - Artifact and plugin runtime files.
  - RTIS and source databases.

## 5. Protection rules

1. Never execute destructive migration without backup and restore verification.
2. Never treat dev/test DB copies as production.
3. Never include credentials or RTIS/platform DB bodies in git.
4. Every migration and backup must record before/after count and schema version.
5. Restore must be tested in an independent environment and reconciled before
   production use.
