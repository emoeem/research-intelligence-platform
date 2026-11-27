#!/usr/bin/env bash
# Knowledge-source review API contract: Nuclei metadata, explicit review writes,
# and audit-history round-trip on a disposable SQLite database.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
WORK="$(mktemp -d /tmp/rip-knowledge-review.XXXXXX)"
PORT="$(python3 - <<'PY'
import socket
s=socket.socket(); s.bind(('127.0.0.1',0)); print(s.getsockname()[1]); s.close()
PY
)"
TOKEN="knowledge-review-test-$(python3 -c 'import secrets; print(secrets.token_hex(16))')"
DB="$WORK/review-fixture.db"
PLUGIN_BIN="$ROOT/target/debug/rp-plugin"
PLUGIN_ARGS="--migrations $ROOT/migrations --data-dir $WORK/data --plugins-dir $WORK/plugins --data-root $WORK/data-root"
SERVER_PID=""
cleanup() {
  if [[ -n "$SERVER_PID" ]]; then kill "$SERVER_PID" 2>/dev/null || true; wait "$SERVER_PID" 2>/dev/null || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

cargo build --offline -q -p plugin-runtime --bin rp-plugin
# Run the read-only list command to initialize the disposable DB migration set.
"$PLUGIN_BIN" --migrations "$ROOT/migrations" --data-dir "$WORK/data" \
  --plugins-dir "$WORK/plugins" --data-root "$WORK/data-root" --db "$DB" \
  list >/dev/null
python3 - "$DB" <<'PY'
import hashlib, json, sqlite3, sys
path = sys.argv[1]
meta = {
    "nuclei_template": {
        "template_id": "review-fixture",
        "severity": "info",
        "parse_status": "parsed",
        "protocols": ["http"],
        "tags": ["synthetic", "review-test"],
        "classification": {"cve-id": ["CVE-2099-12345"]},
    },
    "pocindex_record": {
        "cve_id": "CVE-2099-12345",
        "cisa_date_added": "2099-01-01",
        "pocindex_kev_present": True,
        "epss_score": 0.9,
        "epss_percentile": 0.99,
        "cvss_assessments": [{"version": "3.1", "score": 9.8, "severity": "CRITICAL", "vector": "CVSS:3.1/AV:N", "source": "nvd@nist.gov", "assessment_type": "Primary"}],
        "advisories": [{"url": "https://example.com/advisory", "tags": ["Vendor Advisory"]}],
        "nuclei_metadata": {"severity": "critical", "cvss": 9.8, "cwe": "CWE-416"},
        "snapshot": {"repository_commit": "e9076fea96095e014fc107c02dd23ee8dc21476d", "retrieved_at": "2099-01-01T00:00:00Z"},
    },
    "cve_record": {
        "schema": "cisa-kev-v1",
        "cve_id": "CVE-2099-12345",
        "source_authority": "CISA",
        "known_exploited": True,
        "vendor_project": "Synthetic Vendor",
        "product": "Synthetic Product",
        "date_added": "2099-01-01",
        "due_date": "2099-02-01",
        "known_ransomware_campaign_use": "Unknown",
        "forensic_triage": "Yes",
        "cwes": ["CWE-416"],
        "cvss_score": None,
        "epss_score": None,
        "poc_status": "not_assessed",
        "current_target_status": "not_tested",
    },
}
meta_json = json.dumps(meta, separators=(",", ":"))
body = ""
conn = sqlite3.connect(path)
conn.execute(
    """INSERT INTO knowledge_source_entry (
        plugin_id, plugin_version, corpus_id, entry_kind, item_key, title,
        description, body, body_bytes, content_sha256, metadata_sha256,
        category_path_json, canonical_url, source_type, source_url, source_path,
        source_commit, license_status, rights_policy, review_status, risk_flags_json,
        source_location_json, meta_json, filtered_note, first_seen_at, last_seen_at,
        upstream_missing
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)""",
    (
        "test-plugin", "0.0.1", "synthetic-review", "nuclei_template",
        "review-fixture/test.yaml", "Synthetic Nuclei review fixture",
        "Metadata-only test fixture", body, 0, hashlib.sha256(body.encode()).hexdigest(),
        hashlib.sha256(meta_json.encode()).hexdigest(), "[]", None, "local_dir",
        "https://example.invalid/nuclei-templates", "fixtures/review-fixture/test.yaml",
        "synthetic-fixture", "MIT", "approved_only", "DISCOVERED", "[]", "{}",
        meta_json, None, "2026-11-26T00:00:00Z", "2026-11-26T00:00:00Z", 0,
    ),
)
conn.commit()
conn.close()
PY

RP_PORT="$PORT" RP_HOST=127.0.0.1 RP_AUTH_TOKEN="$TOKEN" \
RP_DB_PATH="$DB" RIP_PLUGIN_DB="$DB" \
RP_PLUGIN_BIN=cargo RP_PLUGIN_BIN_ARGS="run --offline -q -p plugin-runtime --bin rp-plugin -- $PLUGIN_ARGS" \
python3 apps/server/rp_server.py >"$WORK/server.log" 2>&1 &
SERVER_PID=$!
for _ in $(seq 1 100); do
  if curl -fsS "http://127.0.0.1:${PORT}/api/stats" -H "Authorization: Bearer ${TOKEN}" >/dev/null 2>&1; then break; fi
  sleep 0.1
done
python3 - "$PORT" "$TOKEN" <<'PY'
import json, sys, urllib.error, urllib.parse, urllib.request
port, token = sys.argv[1:]
base = f"http://127.0.0.1:{port}"
plugin_id = "test-plugin"
item_key = "review-fixture/test.yaml"
def request(path, method="GET", body=None, auth=True):
    headers = {"Origin": base}
    if auth:
        headers["Authorization"] = f"Bearer {token}"
    data = None if body is None else json.dumps(body).encode()
    if data is not None:
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(base + path, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=20) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError as exc:
        return exc.code, json.load(exc)

path = "/api/knowledge_sources?" + urllib.parse.urlencode({
    "plugin_id": plugin_id, "entry_kind": "nuclei_template", "q": "review-fixture",
})
status, payload = request(path)
assert status == 200 and payload["total"] == 1, (status, payload)
item = payload["items"][0]
assert item["review_status"] == "DISCOVERED", item
assert item["nuclei_template"]["template_id"] == "review-fixture", item
assert item["nuclei_template"]["severity"] == "info", item
assert item["cve_record"]["known_exploited"] is True, item
assert item["cve_record"]["cvss_score"] is None, item
assert item["cve_record"]["poc_status"] == "not_assessed", item
assert item["cve_record"]["forensic_triage"] == "Yes", item
assert item["pocindex_record"]["epss_score"] == 0.9, item
assert item["pocindex_record"]["cvss_assessments"][0]["source"] == "nvd@nist.gov", item
print("PASS: Nuclei, CISA and PoC Index metadata are visible without claiming PoC/target verification")

status, payload = request(
    f"/api/plugins/{plugin_id}/review", "POST",
    {"item_keys": [item_key], "status": "APPROVED", "version": "0.0.1", "reason": "Synthetic fixture metadata and provenance are complete"},
)
assert status == 200 and payload["ok"], (status, payload)
status, payload = request(path)
assert status == 200 and payload["items"][0]["review_status"] == "APPROVED", payload
assert payload["review_history"], payload
history = payload["review_history"][0]
assert history["status"] == "APPROVED" and history["item_keys"] == [item_key], history
print("PASS: explicit review write updates state and records audit history")

status, payload = request(f"/api/plugins/{plugin_id}/review", "POST",
                          {"item_keys": [], "status": "APPROVED"})
assert status == 400 and payload["error"]["code"] == "review_keys_invalid", payload
print("PASS: empty review batch is rejected")

status, payload = request(f"/api/plugins/{plugin_id}/review", "POST",
                          {"item_keys": [item_key], "status": "INVALID", "version": "0.0.1", "reason": "Synthetic fixture negative test"})
assert status == 400 and payload["error"]["code"] == "review_status_invalid", payload
print("PASS: unsupported review status is rejected")

status, payload = request(f"/api/plugins/{plugin_id}/review", "POST",
                          {"item_keys": [item_key], "status": "REJECTED", "version": "0.0.1", "reason": "Synthetic fixture auth negative test"}, auth=False)
assert status == 401, (status, payload)
print("PASS: review writes require Bearer authentication")
PY

echo "SELFTEST KNOWLEDGE REVIEW: PASS"
