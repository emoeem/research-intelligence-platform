# M8 — Staged SPA Entry

Date: 2026-11-24
Decision: staged migration. The legacy entry remains the default; the SPA is served in parallel for acceptance.

## Stage topology

- Legacy entry: existing server/root on port 7800. It is not modified by this stage.
- New SPA entry: `http://127.0.0.1:7801/`, served by `apps/server/stage_spa.sh`.
- Static root: `web/dist`, selected by `RP_STATIC_DIR`.
- SPA deep-link fallback: enabled only when `RP_SPA_FALLBACK=1`, and only for HTML navigation requests whose final path segment has no file extension.
- Database: isolated clone at `rip-data/staging/platform.db` (git-ignored). The script refuses the primary `migrations/platform.db` path.
- Authentication: existing Bearer token file or explicit `RP_AUTH_TOKEN`. The stage script refuses to start without authentication configured.

## Routing and cache behavior

- Known `/api/*` routes retain JSON behavior.
- Unknown `/api/*` paths remain JSON 404; they are never rewritten to `index.html`.
- Missing JS/CSS/image/font files remain 404, even when the request accepts HTML.
- Only browser document navigation to an extension-less route (e.g. `/projects/prj-…`) falls back to the SPA `index.html`.
- `index.html` uses `Cache-Control: no-store`; files under the built `assets/` directory use immutable one-year caching; other static files use one-hour caching.
- Static and API responses include `X-Content-Type-Options`, `X-Frame-Options`, `Referrer-Policy`, `Permissions-Policy` and `Cross-Origin-Resource-Policy`.
- Static responses additionally apply a restrictive CSP (`default-src 'self'`, no objects, no framing, same-origin scripts/connect, data/blob for media as required by the app). The only inline script allowed is the SHA-256-pinned theme/language anti-flash bootstrap in web/dist/index.html; its exact hash is included in the CSP rather than enabling unsafe-inline for scripts.

## Write request protection

- POST, PATCH and PUT paths call the common same-origin guard before route-specific write handlers.
- `Sec-Fetch-Site: cross-site` is rejected.
- If an `Origin` header is supplied, it must be an HTTP(S) origin whose authority exactly matches the request Host, with no path/query/fragment/user-info.
- Non-browser CLI clients may omit Origin; the server does not emit permissive CORS headers.
- Existing Bearer authentication and per-route write allowlists remain in force.

## Verification

- `apps/server/selftest_spa_entry.py` covers extension-less deep links, missing static assets, API 404 passthrough, mismatched Origin rejection, same-origin pass-through, CSP and cache/security headers.
- Live stage smoke checks on port 7801 verified deep-link HTTP 200, API unknown-path JSON 404, missing asset 404, cross-origin write 403, same-origin authenticated Project creation 201.
- The live test used the isolated staging DB; primary `migrations/platform.db` remains at user_version 30. The staging copy migrates independently to 31 when the Project/Asset API is first used.
- The old 7800 entry was not switched, disabled, or overwritten.

## Remaining acceptance gate

M8 is **STAGED — NOT DEFAULT**. Before promoting the SPA root, complete the final user acceptance pass, compare all required pages against the canonical Figma file, confirm reverse-proxy/deployment topology and TLS headers in the actual deployment environment, and rehearse restoring the old entry after a staged failure. This repository-local stage test does not prove production reverse-proxy behavior.
