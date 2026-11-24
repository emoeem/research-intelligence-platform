# Functional Acceptance

- All registered workspace and deep-link routes remain reachable.
- Global navigation, workspace navigation and session tabs have separate semantics.
- Search, filters, table sorting, selection, pagination and column controls perform real actions.
- Split panels resize, collapse and restore as specified.
- Empty, loading, error, unauthorized, read-only, partial and disconnected states are explicit.
- Every production action maps to an existing API capability and displays server-confirmed results.
- Repeater sends only through the existing authorized backend path; no fake response is presented as a production result.
- Proxy, Fuzzer and plugin operations preserve scope, trust and permission confirmations.
- Browser refresh and route transitions preserve only the state documented by each workspace.
- A control without a backend capability is disabled or absent with an accurate explanation, never a fake success.
