# Performance Acceptance

- Keep route-level code splitting.
- Do not initialize CodeMirror/large editors before their panel is opened.
- Large HTTP bodies load on demand and remain subject to existing size limits and redaction.
- Large lists use bounded server pagination and virtualization where the API contract supports it.
- Avoid loading all records or all message bodies into the browser.
- Keep ECharts modular registration and avoid importing unused chart modules.
- Record production build chunk sizes before and after dependency changes.
- Test table scrolling, panel resizing, editor initialization and route transitions with representative large datasets.
- Any material regression must be fixed before the corresponding phase is accepted.
