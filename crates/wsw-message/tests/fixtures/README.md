# WSW-1 conformance fixtures

All fixtures in this directory are synthetic, deterministic protocol samples authored for this repository. They contain no captured user traffic, credentials, or third-party payloads.

Each `.http` or `.bin` file has a sibling `.expected.json` file containing:
- SHA-256 of the exact wire fixture bytes (`wire_sha256`)
- The complete serialized `ParseOutcome` (`outcome`), including parsed message, status, error code, fidelity, and declared/received body lengths

The integration test compares both the fixture byte hash and the complete JSON result. Updating a fixture requires an explicit review of the protocol intent and its expected JSON; do not regenerate goldens merely to silence a failing test.

## Positive corpus (10)

| Fixture | Coverage |
|---|---|
| `req_simple_get.http` | HTTP/1.1 origin-form GET, Host and zero Content-Length |
| `req_post_chunked.http` | Request chunked decoding and terminal zero chunk |
| `req_absolute_form.http` | Absolute-form target and authority host derivation |
| `req_multi_header.http` | Repeated Set-Cookie values retain order |
| `req_mixed_case_headers.http` | Header-name original casing is preserved |
| `req_ws_upgrade.http` | WebSocket Upgrade/Connection headers pass through |
| `res_cl_body.http` | Response Content-Length body delimiting |
| `res_chunked_trailer.http` | Chunked response and separate trailer collection |
| `res_http10_eof.http` | HTTP/1.0 EOF-delimited response |
| `res_multi_status_headers.http` | Status line and repeated response headers |

## Negative / edge corpus (7)

| Fixture | Expected handling |
|---|---|
| `negative/body_invalid_utf8.bin` | Invalid UTF-8 body bytes preserved; only preview is lossy |
| `negative/trunc_chunked_no_term.http` | Partial chunk body bytes retained; `truncated_chunk` |
| `negative/trunc_header_no_crlf.http` | Partial header block; no fabricated Parsed message |
| `negative/trunc_body_short_cl.http` | Partial body with declared/received lengths |
| `negative/oversize_header_64k.http` | Header cap rejected with `header_cap_exceeded` |
| `negative/oversize_body.http` | Parser preserves body; CAS adapter marks it truncated at configured cap |
| `negative/req_no_host.http` | HTTP/1.1 request without Host fails closed |

The binary body fixture is deliberately named `.bin`; its complete HTTP wire bytes include a non-UTF-8 body octet.
