# Visual Acceptance

## Required viewports

| Name | Viewport |
|---|---|
| Desktop XL | 1920×1080 |
| Desktop | 1440×900 |
| Compact Desktop | 1280×800 |
| Tablet | 1024×768 |
| Mobile | 390×844 |

For every production route capture Dark and Light themes, normal state, empty state, loading/error/unauthorized/read-only/partial/disconnected states where applicable, plus the primary interaction. Workbench routes additionally capture selected request/session and panel-resize states.

## Baseline discipline

- Keep browser, fonts, viewport, fixture and theme deterministic.
- Never update snapshots solely to make CI green.
- Inspect actual and diff images before accepting baseline changes.
- Record screenshot paths and reviewer decision per batch.
- Current Playwright gate: 71/71 passed on 2026-11-24. Forty-two baseline PNGs were changed by a pre-existing snapshot update and still require human review; see `M0-CLOSEOUT.md`.

## Layout checklist

- No uncontrolled horizontal overflow.
- Main workspace uses available width appropriate to its layout mode.
- Inspector and Activity do not permanently steal space when closed.
- Splitters respect minimum sizes and support keyboard adjustment.
- Scroll containers have clear ownership.
- Mobile workbench may switch to sequential panes, but every primary operation remains reachable.
