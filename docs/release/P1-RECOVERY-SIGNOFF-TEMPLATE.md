# P1 Recovery Sign-off Template

> Status: unsigned
> Scope: SQLite and PostgreSQL production recovery
> No backup or recovery cutover is authorized until each field is signed.

## Production owner

- Deployment owner:
- Database owner:
- Restore test owner:
- Business approver:
- Backup runbook location:

## SQLite platform database

- Primary path:
- Database role:
- Backup frequency:
- Retention period:
- Offsite copy location:
- Encryption/key owner:
- Artifact/plugin extras:
- Search index rebuild:
- RTO:
- RPO:

## PostgreSQL authoritative database

- Production DSN secret name:
- Backup format: custom (`pg_dump -Fc`)
- Frequency:
- Retention period:
- Offsite copy location:
- Encryption/key owner:
- Migration before/after checks:
- Key table counts:
- Foreign key/integrity checks:
- Representative business samples:
- RTO:
- RPO:

## Recovery drill record

| Check | Result | Evidence |
|---|---|---|
| Independent PostgreSQL dump/restore | Pending | |
| Migration state verified | Pending | |
| knowledge/entity/relation/evidence counts | Pending | |
| review batch/audit records | Pending | |
| SQLite roundtrip restore | Passed locally 16/16 | |
| Search index rebuild | Passed locally | |
| File/artifact restore | Pending | |
| Service restart | Pending | |
| Production snapshot unchanged | Pending | |
| Rollback owner and step | Pending | |

## Technical gate

`rip_pg_recovery_gate.py` passed against isolated `rip-pg17` with a
template0 restore database and 2/2 row reconciliation. It is a technical
smoke gate, not evidence of production backup policy or an offsite recovery.
