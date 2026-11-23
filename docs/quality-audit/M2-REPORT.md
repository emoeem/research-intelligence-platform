# M2 Report

Status: **DONE**

## Implemented

- Removed `requests` re-resolution from Source Monitor.
- DNS resolution is validated before request.
- Actual HTTP/TCP connection uses the resolved validated IP.
- HTTPS keeps the original hostname as TLS SNI and verifies it.
- HTTP Host headers retain the original hostname.
- Redirect targets are resolved and checked again.
- Response bodies are capped at 256 KiB.
- Private-address unit test and redirect-to-private test pass.
- Feedback candidate show/reject/promote operations now require matching `workspace_id`.
- KnowledgeCard detail CLI now requires matching `workspace_id`.
- Browser /agents nested-interactive accessibility defect fixed and axe-verified.

## Remaining

- Full REST/MCP runtime credentials are not yet represented by a two-workspace HTTP integration fixture.
- Live recursive DNS takeover testing is an external adversarial-test boundary; the implementation and unit smoke cover resolver and redirect pinning.
