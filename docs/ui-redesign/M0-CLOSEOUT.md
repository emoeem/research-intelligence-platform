# M0 Closeout — Baseline and Architecture Audit

Date: 2026-11-24\
Repository: `/media/emo/code/pensting/research-platform`\
Status: **M0 technical gate passed; snapshot provenance remains documented**

## 1. Baseline authority

Use `M0-RECHECK.md` as the current field audit. Earlier M0 snapshots predate the current dirty working tree and must not be read as the current Git baseline.

- Branch: `main`
- HEAD at audit: `0f95c926`
- No Git reset/clean/stash/checkout/rebase/commit/push was performed.
- No database, migration, API contract, Rust Core or Python Worker change was made in this M0 closeout.
- The repository already contained broad UI implementation changes before this closeout. They were preserved rather than overwritten.

## 2. Gate rerun

| Gate | Result | Evidence |
|---|---|---|
| TypeScript typecheck | PASS | `npm run typecheck`, exit 0 |
| Unit tests | PASS | 20 files, 135 tests |
| Prettier | PASS | `npm run format:check`, exit 0 after formatting the 8 specifically reported files |
| Production build | PASS | Vite 8.3.7, 2423 modules transformed |
| Playwright visual gate | PASS | 71/71 tests, exit 0 |
| axe serious/critical | PASS | 24 prior `.rail-profile` violations no longer reproduce; all covered page/theme scans passed |
| Git whitespace | PASS | `git diff --check`, exit 0 |

Vitest still prints jsdom Canvas/ResizeObserver and React `act(...)` warnings. They do not fail the current 135 tests; they remain cleanup candidates and are not represented as functional browser failures.

## 3. Snapshot provenance caveat

42 Playwright baseline PNG files were modified by a snapshot-update process already running in the worktree before this closeout. The subsequent 71-test run passed against those current baselines. This proves deterministic test agreement, **not** independent human approval of every changed baseline image.

Do not bulk-revert or blindly accept these files. Keep the snapshot provenance note and require visual review when reviewing the final screenshot set.

## 4. Resolved and outstanding M0 findings

- M0-003 ARIA prohibited attribute: resolved in current source by giving `.rail-profile` a valid `role="group"`; axe rerun passed.
- M0-004 Agents screenshot mismatch: automated visual run now passes against the present baseline; human review of the updated baseline remains open.
- M0-012 Prettier failures: resolved; targeted formatting completed and format check passes.
- Dual browser entry, M7 static page retention, route mapping, TanStack Table actual usage, and History virtualization remain implementation/migration items for M2–M8, not reasons to rewrite backend APIs during M0.

## 5. M0 decision

The code/route/dependency/service audit is complete and the repeatable frontend quality gates pass. M0 is closed as an audit phase. The updated snapshot set remains explicitly unapproved until visual review; this limitation must carry into M7 final visual acceptance.

## 6. Next phase

Proceed to M1 documentation freeze and design artifact completion. M1 is based on the user's approved Dashboard reference direction, existing RIP tokens and the isolated prototype. Figma pixel-level comparison is tracked separately and must not be falsely reported as complete.
