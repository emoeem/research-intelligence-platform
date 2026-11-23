# M0 Project Baseline

## Repository

- Repository root: `/media/emo/code/pensting/research-platform`
- HEAD and worktree were captured before changes.
- No uncommitted work was present at audit start.
- Phase 7/8 implementation remains in place: Rust core, Python workers, React/Vite web UI, PostgreSQL core, and SQLite operations.

## CI

The existing `.github/workflows/ci.yml` has Rust, protocol parity, Python, and Web jobs.

Confirmed gaps:

- No PostgreSQL service or `RP_TEST_PG_DSN` job in CI.
- The Rust job therefore does not execute database integration suites that depend on PostgreSQL.
- Browser tests are self-contained Playwright coverage and are not in normal CI.
- The performance benchmarks marked `#[ignore]` are not in CI.

## Runtime and data files

- `.sock.lock` is tracked.
- PostgreSQL is not represented by repository database snapshots; SQLite artifacts are tracked.
- `migrations/platform-dryrun.db`, `migrations/platform-governance.db`, and `migrations/platform-search.db` are tracked SQLite artifacts.
- `migrations/sources-snapshot/` contains source and audit snapshots plus SQLite WAL/SHM sidecars.
- Production `platform*.db` and RTIS `knowledge.db` must remain excluded and unmodified.
