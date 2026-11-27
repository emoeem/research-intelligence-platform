# M7 — Local performance smoke report

Date: 2026-11-24

## Method

- Served the production Vite build from the staged SPA entry at 127.0.0.1:7801.
- Browser: system Chromium, headless, local loopback, warm static cache.
- Routes: 12 primary routes (/ , /research, /knowledge, /projects, /research/assets, /tools, /plugins, /agents, /workflows, /runs, /runtime, /settings).
- Viewports: desktop 1440×900 and mobile 390×844.
- Total: 24 navigation samples.
- Collected navigation loadEventEnd, DOM node count, HTTP response status, API error responses, and browser console errors.

## Results

| Metric | Result |
|---|---:|
| Successful navigations | 24 / 24 |
| HTTP route failures | 0 |
| API error responses | 0 |
| Browser console errors | 0 |
| Median navigation load event | 24 ms |
| p95 navigation load event | 28 ms |

## Interpretation and limitations

This is a local, warm-cache, headless-browser smoke measurement. It verifies that the staged build loads the 12 route shells without browser/API errors at one desktop and one mobile viewport. It is not a Lighthouse/Core Web Vitals report, does not model network latency or cold-cache devices, and does not replace real-device profiling or human interaction review.

This is not real-device profiling or a Lighthouse/Core Web Vitals report. Under the user's M7=reference and Figma=existing decision, pixel-level Figma parity is not a release gate. The user has accepted the current UI at product level and the automated visual gate is green; real-device profiling remains a non-blocking follow-up.
