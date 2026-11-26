#!/usr/bin/env bash
# Curated POC search end-to-end contract: Bearer auth, lazy derived-index build,
# structured filters, metadata-only projection, and immutable source DBs.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
WORK="$(mktemp -d /tmp/rip-poc-search.XXXXXX)"
PORT="$(python3 - <<'PY'
import socket
s=socket.socket(); s.bind(('127.0.0.1',0)); print(s.getsockname()[1]); s.close()
PY
)"
TOKEN="poc-search-test-$(python3 -c 'import secrets; print(secrets.token_hex(16))')"
SERVER_PID=""
cleanup() {
  if [[ -n "$SERVER_PID" ]]; then kill "$SERVER_PID" 2>/dev/null || true; wait "$SERVER_PID" 2>/dev/null || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT
python3 integrations/rtis/curation/validate_overlay.py >/dev/null
sha256sum migrations/platform.db migrations/sources-snapshot/catalog.db integrations/rtis/knowledge.db integrations/rtis/curation/k_meta_overlay.json > "$WORK/sources.before"
RP_PORT="$PORT" RP_AUTH_TOKEN="$TOKEN" \
RP_CURATED_SEARCH_INDEX_DB="$WORK/curated-poc-search.db" \
RP_SEARCH_INDEX_BIN="$ROOT/target/debug/rp-search-index" \
python3 apps/server/rp_server.py >"$WORK/server.log" 2>&1 &
SERVER_PID=$!
for _ in $(seq 1 100); do
  if curl -fsS "http://127.0.0.1:${PORT}/api/stats" -H "Authorization: Bearer ${TOKEN}" >/dev/null 2>&1; then break; fi
  sleep 0.1
done
python3 - "$PORT" "$TOKEN" <<'PY'
import json, sys, urllib.error, urllib.parse, urllib.request
port, token = sys.argv[1:]
base = f"http://127.0.0.1:{port}/api/curated-pocs/search"
def get(params=None, auth=True, endpoint=None):
    url = (endpoint or base) + ("?" + urllib.parse.urlencode(params) if params else "")
    headers = {"Authorization": f"Bearer {token}"} if auth else {}
    req = urllib.request.Request(url, headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=20) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError as exc:
        return exc.code, json.load(exc)

status, all_hits = get({"q":"zeai", "limit":"20"})
assert status == 200 and all_hits["total"] == 3, (status, all_hits)
assert all(h["resource_type"] == "rtis_poc" for h in all_hits["hits"])
assert all(h.get("description") is None for h in all_hits["hits"])
assert all("verified_note" not in h for h in all_hits["hits"])
assert all(h.get("source_path") and h.get("source_commit") for h in all_hits["hits"])
assert all("poc_missing_evidence" in h and "poc_evidence_refs" in h for h in all_hits["hits"])
print("PASS: keyword search returns 3 curated zeai POCs with provenance and evidence metadata only")

status, k2 = get({"q":"zeai", "evidence_level":"K2"})
assert status == 200 and k2["total"] == 2, (status, k2)
assert all(h["evidence_level"] == "K2" for h in k2["hits"])
print("PASS: K2 facet remains independent from lifecycle")

status, k0 = get({"q":"zeai", "evidence_level":"K0", "knowledge_lifecycle":"Candidate"})
assert status == 200 and k0["total"] == 1, (status, k0)
assert k0["hits"][0]["evidence_level"] == "K0"
assert k0["hits"][0]["knowledge_lifecycle"] == "Candidate"
print("PASS: K0 + Candidate filter")

status, category = get({"q":"zeai", "poc_category":"auth-bypass"})
assert status == 200 and category["total"] == 1, (status, category)
assert category["hits"][0]["poc_category"] == "auth-bypass"
print("PASS: POC category facet")

status, severity = get({"q":"zeai", "poc_severity":"HIGH"})
assert status == 200 and severity["total"] == 3, (status, severity)
assert all(h["poc_severity"] == "high" for h in severity["hits"])
print("PASS: severity facet is case-normalized")

status, queue = get({"poc_missing_evidence":"true", "poc_source_integrity":"missing",
                     "poc_verification_result":"candidate_not_verified", "limit":"100"})
assert status == 200 and queue["total"] == 22, (status, queue)
assert all(h["poc_missing_evidence"] and h["poc_review_gate"] for h in queue["hits"])
assert all(h["poc_source_integrity"] == "missing" for h in queue["hits"])
print("PASS: evidence queue filters return all 22 source-integrity-missing candidates with review gates")

status, related = get({"poc_dedup_status":"related-family", "limit":"100"})
assert status == 200 and related["total"] == 5, (status, related)
print("PASS: dedup relationship facet")

status, tools = get({"poc_verification_result":"not_applicable", "limit":"100"})
assert status == 200 and tools["total"] == 8, (status, tools)
print("PASS: non-target tool/method records are explicitly not_applicable")

status, seclists = get({"q":"SecLists", "limit":"10"})
assert status == 200 and seclists["total"] >= 1, (status, seclists)
assert any("https://github.com/danielmiessler/SecLists" in h["poc_source_urls"] for h in seclists["hits"])
print("PASS: upstream source URLs are searchable and returned as provenance")

status, case_links = get({"q":"case-xingqyh-icu-2026-11", "limit":"10"})
assert status == 200 and case_links["total"] == 3, (status, case_links)
assert all(h["related_entities"] == ["has_case:case-xingqyh-icu-2026-11"] for h in case_links["hits"])
print("PASS: external case-group links are indexed and returned")

status, summary = get(endpoint=f"http://127.0.0.1:{port}/api/curated-pocs/summary")
assert status == 200 and summary["total_curated"] == 49, (status, summary)
assert summary["missing_evidence_entries"] == 33 and summary["review_queue_count"] == 33
assert summary["metadata_incomplete_entries"] == 0
assert summary["missing_evidence_items"] == 95
assert summary["entries_with_source_path"] == 41 and summary["entries_without_source_path"] == 8
assert summary["entries_with_source_commit"] == 38 and summary["entries_without_source_commit"] == 11
assert summary["entries_with_source_urls"] == 9 and summary["entries_without_source_urls"] == 40
assert summary["relation_count"] == 25 and summary["external_entity_count"] == 2
print("PASS: governance summary is consistent with 49 overlay records and 33 evidence-gap entries")

status, invalid = get({"q":"zeai", "evidence_level":"K9"})
assert status == 400 and invalid["error"]["code"] == "bad_poc_filter", (status, invalid)
print("PASS: invalid facet rejected with HTTP 400")

status, invalid_cve = get({"q":"zeai", "poc_cve_id":"not-a-cve"})
assert status == 400 and invalid_cve["error"]["code"] == "bad_poc_filter", (status, invalid_cve)
print("PASS: malformed CVE filter rejected")

status, empty = get({})
assert status == 400 and empty["error"]["code"] == "poc_search_query_required", (status, empty)
print("PASS: empty search rejected")

status, unauthorized = get({"q":"zeai"}, auth=False)
assert status == 401, status
status, unauthorized_summary = get(auth=False, endpoint=f"http://127.0.0.1:{port}/api/curated-pocs/summary")
assert status == 401, status
print("PASS: Bearer auth enforced for search and summary")
PY
sha256sum migrations/platform.db migrations/sources-snapshot/catalog.db integrations/rtis/knowledge.db integrations/rtis/curation/k_meta_overlay.json > "$WORK/sources.after"
cmp -s "$WORK/sources.before" "$WORK/sources.after"
echo "PASS: all source DBs and curation Overlay unchanged"
test -s "$WORK/curated-poc-search.db"
echo "PASS: lazy index build wrote only to disposable test index"
echo "SELFTEST POC SEARCH: PASS"
