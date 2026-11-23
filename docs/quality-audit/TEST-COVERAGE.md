# M0 Test Coverage Audit

## Rust

- Workspace tests exist and run in CI.
- PostgreSQL suites under `crates/intel-db/tests/` use `RP_TEST_PG_DSN`.
- Missing DSN or unreachable DSN is commonly converted into an early return or `None`, not a hard CI failure.
- Phase 7 has report, replay, rejection, migration, and workspace immutability coverage.
- Promotion concurrency and mid-transaction connection failure coverage is absent.

## Python

- Python worker discovery runs in CI.
- Source Monitor unit tests cover basic public/private URL behavior and HTTP fallback.
- DNS rebinding, proxy bypass, IPv6 private ranges, pinned TCP target, and large-response limits need stronger coverage.

## Web

- Web build and Vitest run in CI.
- Playwright suites exist under `web/tests/`.
- Some suites are intentionally split between mock and real-backend modes with `test.skip`.

## Missing gates

- PostgreSQL service and migration job.
- PostgreSQL fail-fast DSN job.
- Browser smoke job.
- PR-scale database/performance smoke.
- Scheduled 100K/1M benchmark.
- PostgreSQL and SQLite backup/recovery recovery drill.
- Worker crash, timeout, retry, restart, and idempotency recovery drill.
