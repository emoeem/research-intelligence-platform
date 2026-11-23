# M0 Confirmed Issues

## P0

1. PostgreSQL integration tests silently return when `RP_TEST_PG_DSN` is absent or unreachable.
2. `promote_feedback_candidate()` opens separate pool connections for card creation and review decisions. It does not share the candidate lock transaction with all writes.
3. `KnowledgeCard` promotion has no explicit fault-injection coverage for partial transaction failure.

## P1

1. Source Monitor validates a resolved address but then calls `requests` without pinning the verified TCP target, creating a DNS re-resolution/DNS rebinding gap.
2. Source Monitor has an environment escape hatch `RIP3_SOURCE_MONITOR_ALLOW_PRIVATE=1`; test use must be separated from production behavior.
3. Workspace isolation requires code-wide evidence, but existing workspaces and task IDs have some immutability checks.

## P1/P2 repository

1. `.sock.lock` is tracked.
2. SQLite runtime/snapshot handling is inconsistent; sidecars are tracked alongside source snapshots.
3. Multiple PostgreSQL integration suites explicitly skip on missing or unreachable DSN.
4. Browser suites contain environment-qualified skips rather than a complete normal CI entry.

## P2/P3

1. CI does not contain a PostgreSQL migration/recovery/performance gate.
2. The existing workflow does not publish explicit integration test execution counts.
3. Full backup/recovery, migration failure, and worker interruption recovery are not represented as a complete CI gate.

## Todo/FIXME review

- `scripts/rip_new_tool.py` contains a scaffold TODO for echo tool semantics.
- Several TODO/FIXME strings occur only in fixtures or source-analysis test strings and are not production bypasses.
