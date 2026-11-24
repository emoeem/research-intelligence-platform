# M2 Implementation Report — AppShell and Global Navigation

Date: 2026-11-24\
Status: **IMPLEMENTATION COMPLETE — frontend acceptance gates passed**\
Scope: Unified AppShell, selected-object Inspector, Activity Dock, responsive split layout, translations and shell acceptance coverage.

## Delivered

### AppShell and navigation

1. Topbar exposes Runs and Runtime shortcuts to the existing routes.
2. Notifications remains visibly disabled with an explanation. Repository/API inspection found no notification source or read/acknowledgement contract; enabling it would imply a capability the backend does not provide.
3. Existing workspace IDs, deep links, route behavior, theme/language behavior and session tabs remain intact.
4. Mobile/tablet Inspector remains an overlay/drawer; desktop Inspector becomes a true resizable right-side pane.

### Inspector — real selected-object context

Inspector resolves and renders the current object for:
- Run: status, tool, duration, error category and task ID; related Tool/Task links.
- Tool: name, kind, lifecycle, governance, health, permission and timeout; links to recent Runs for that exact tool ID.
- Plugin: name, version, kind, state, trust level and license.
- Task: title, state, type, target and objective; links to Runs whose task ID matches the selected task.
- Knowledge: title, lifecycle, governance, component, severity, source and source URL.

No fake related entities are created. Where the API does not expose a relationship, Inspector says there are no registered related objects instead of showing a “pending” placeholder.

### Activity Dock — real backend data

| Tab | Source and behavior |
|---|---|
| Queue | Existing live queue/plan component retained |
| Tool Runs | `/api/runs?limit=12`, refresh every 5 seconds; status, tool, created time and deep link |
| Logs | Recent failed/timed-out Run events; when a Run detail is selected, shows lifecycle timestamps, error category and real `error_json` |
| Output | For the selected Run, shows registered Artifact ID, role, MIME type, size and URI link |
| Status | `/api/runtime?limit=20`, refresh every 10 seconds; node health and heartbeat age |

**Backend boundary:** no dedicated stdout/stderr or streaming log API exists in the current contract. The Logs tab therefore presents actual Run lifecycle/error events, not a fabricated terminal or invented live log stream. Output is based only on Artifacts returned by Run detail. Notifications similarly remain disabled until the backend exposes a real contract.

### Resizable panels

- Reused the installed `react-resizable-panels` dependency.
- Desktop Inspector: horizontal splitter, minimum pane sizes, pointer drag and keyboard arrows.
- Activity: vertical splitter, minimum pane sizes, pointer drag and keyboard arrows.
- Both dimensions persist in local storage under versioned keys:
  - `rip_shell_inspector_width_v1`
  - `rip_shell_activity_height_v1`
- On viewports below 1280px, Inspector uses the existing overlay pattern rather than squeezing the main work area.
- A small non-ResizeObserver fallback keeps keyboard-accessible separators available in test/limited DOM environments.

### Localization

All newly introduced labels, empty/error/loading states, Inspector fields and splitter accessible names are translated for zh-CN, en-US and ja-JP.

## Acceptance and validation

| Gate | Result |
|---|---|
| TypeScript | PASS |
| Prettier format check | PASS |
| Full unit tests | PASS — 20 files / 138 tests |
| Production build | PASS — 2427 modules transformed |
| Full Playwright visual/axe gate | PASS — 73/73 |
| M2 shell responsive matrix | PASS — 5 widths: 360, 768, 1024, 1280, 1600 |
| M2 Activity data-state matrix | PASS — 8 states: loading, empty, unavailable, request error, active Run, failed Run, healthy Runtime, degraded Runtime |
| Splitter keyboard/persistence checks | PASS |
| Splitter pointer-drag check | Included in the M2 shell browser gate |
| axe serious/critical findings in existing covered matrix | PASS |
| `git diff --check` | PASS (final workspace check recorded below) |

The five-width matrix checks the Inspector/Activity visibility model and horizontal overflow. The eight-state matrix exercises the Activity loading, empty, unavailable, error, active/failure and Runtime health states. Existing visual snapshots remain automated regression evidence; human screenshot sign-off remains part of the later M7 visual review.

## M2 closeout decision

M2 frontend implementation is complete. The backend-dependent features that do not exist (notifications and a raw stdout/stderr streaming endpoint) are not represented as working controls; the UI exposes the real capabilities available today and explains the boundary. No backend schema or execution API was changed.

Do not treat this report as permission to skip M3-M8 acceptance requirements. The next phase may begin only under the master plan's phase order and its own gates.
