# M1 Report

Status: **DONE**

## Implemented

- KnowledgeCard promotion now uses transaction-bound repository helpers.
- Card creation, review decision/log, and candidate state update share one PostgreSQL transaction.
- CI has a PostgreSQL 17.11 service and fail-fast `RP_TEST_PG_DSN`.
- Phase 7 tests fail in CI when DSN is missing or unreachable.
- Local PostgreSQL tests explicitly print `SKIP` instead of silently returning.
- Added duplicate/replay promotion coverage asserting one card and one review log.
- Real PostgreSQL 17.11 execution: 7 Phase 7 tests passed.
- Concurrent promotion test passed; only one card/review log remained.
- Injected candidate-state update trigger failure passed; card and review log rolled back.
- PostgreSQL backup/recovery gate passed.
