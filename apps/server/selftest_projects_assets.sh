#!/usr/bin/env bash
# Independent Projects / Research Assets HTTP integration test.
# All writes target a temporary copy of migrations/platform.db; the primary DB is fingerprinted.
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO}"
PORT="${SELFTEST_PROJECTS_PORT:-7825}"
TOKEN="rip-projects-assets-selftest-$(python3 -c 'import secrets; print(secrets.token_hex(12))')"
DB="$(mktemp /tmp/rip-projects-assets-XXXXXX.db)"
LOG="$(mktemp /tmp/rip-projects-assets-server.XXXXXX.log)"
PRIMARY_VERSION="$(sqlite3 migrations/platform.db 'PRAGMA user_version;')"
cleanup() {
  [[ -n "${SERVER_PID:-}" ]] && kill "${SERVER_PID}" 2>/dev/null || true
  [[ -n "${SERVER_PID:-}" ]] && wait "${SERVER_PID}" 2>/dev/null || true
  rm -f "${DB}" "${DB}-wal" "${DB}-shm" "${LOG}"
}
trap cleanup EXIT

cp migrations/platform.db "${DB}"
sqlite3 "${DB}" "INSERT INTO research_task(task_id,objective,state,status_history,source_file,source_system,created_at,updated_at) VALUES('rtis-project-assets-test','Test RTIS task','CREATED','[]','selftest.json','rtis-tasks','now','now');"
cargo build --offline -q -p verification-engine --bin rp-verify
RP_PORT="${PORT}" RP_AUTH_TOKEN="${TOKEN}" RP_DB_PATH="${DB}" \
  RP_VERIFY_BIN="${REPO}/target/debug/rp-verify" \
  python3 apps/server/rp_server.py >"${LOG}" 2>&1 &
SERVER_PID=$!
for _ in $(seq 1 40); do
  if curl -fsS -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}/api/projects?workspace_id=selftest" >/dev/null 2>&1; then break; fi
  sleep 0.25
done

RIP_TEST_BASE="http://127.0.0.1:${PORT}" RIP_TEST_TOKEN="${TOKEN}" python3 - <<'PY'
import json, os, urllib.request, urllib.error
base=os.environ["RIP_TEST_BASE"]; token=os.environ["RIP_TEST_TOKEN"]
def req(method,path,body=None,auth=True,origin=base):
    data=None if body is None else json.dumps(body).encode()
    headers={"Content-Type":"application/json"}
    if auth: headers["Authorization"]="Bearer "+token
    if origin: headers["Origin"]=origin
    request=urllib.request.Request(base+path,data=data,headers=headers,method=method)
    try:
        with urllib.request.urlopen(request,timeout=20) as response:
            return response.status,json.loads(response.read())
    except urllib.error.HTTPError as error:
        return error.code,json.loads(error.read())

assert req("GET","/api/projects?workspace_id=selftest")[0]==200
assert req("GET","/api/research/assets?workspace_id=selftest")[0]==200
status,project=req("POST","/api/projects",{
    "workspace_id":"selftest","name":"Integration project","slug":"integration-project",
    "objective":"Exercise real Projects APIs","priority":"high"
})
assert status==201,(status,project)
pid=project["project_id"]
assert req("POST",f"/api/projects/{pid}/tasks",{"task_id":"rtis-project-assets-test","role":"research"})[0]==201
status,updated=req("PATCH",f"/api/projects/{pid}?workspace_id=selftest",{"status":"active","summary":"Updated"})
assert status==200 and updated["status"]=="active",(status,updated)
status,detail=req("GET",f"/api/projects/{pid}?workspace_id=selftest")
assert status==200 and len(detail["tasks"])==1,(status,detail)
assert req("GET",f"/api/projects/{pid}?workspace_id=other")[0]==404

status,asset=req("POST","/api/research/assets",{
    "workspace_id":"selftest","asset_type":"domain","canonical_value":"asset-selftest.example",
    "display_name":"Asset integration fixture"
})
assert status==201,(status,asset)
aid=asset["asset_id"]
status,endpoint=req("POST",f"/api/research/assets/{aid}/endpoints",{
    "scheme":"https","host":"asset-selftest.example","port":443,"protocol":"https","path":"/"
})
assert status==201,(status,endpoint)
status,observation=req("POST",f"/api/research/assets/{aid}/observations",{
    "observation_type":"http_response","source_type":"manual","source_ref":"selftest",
    "evidence_ref":"evidence:selftest-1","endpoint_id":endpoint["endpoint_id"],
    "payload":{"status":200}
})
assert status==201,(status,observation)
status,technology=req("POST",f"/api/research/assets/{aid}/technology",{
    "observation_id":observation["observation_id"],"technology":"FixtureServer",
    "version":"1.0","confidence":0.9,"evidence_ref":"evidence:selftest-1"
})
assert status==201,(status,technology)
assert req("POST",f"/api/projects/{pid}/assets",{"asset_id":aid,"role":"in_scope"})[0]==201
status,detail=req("GET",f"/api/research/assets/{aid}?workspace_id=selftest")
assert status==200 and len(detail["observations"])==2 and len(detail["endpoints"])==1 and len(detail["technologies"])==1,(status,detail)
assert req("GET",f"/api/research/assets/{aid}?workspace_id=other")[0]==404
status,_=req("POST","/api/projects",{"name":"Cross origin","slug":"cross-origin"},origin="https://attacker.invalid")
assert status==403,status
status,_=req("POST","/api/projects",{"name":"No auth","slug":"no-auth"},auth=False)
assert status==401,status
print("PASS: Projects CRUD, workspace isolation, Project-Task/Asset relations, Asset/Endpoint/Observation/Technology APIs, Origin and Bearer guards")
PY

AFTER_VERSION="$(sqlite3 migrations/platform.db 'PRAGMA user_version;')"
[[ "${AFTER_VERSION}" == "${PRIMARY_VERSION}" ]]
echo "PASS: primary platform.db untouched (user_version=${AFTER_VERSION})"
