#!/usr/bin/env bash
# PB-010/PB-012 知识发布闭环自验（受守卫发布回放）：
#   MCP 供体提交（真实 rp-mcp 二进制 stdio JSON-RPC，严格 envelope）→
#   只落 CANDIDATE（收据留痕）→ MCP approve 词表拒绝（"MCP never approves"）→
#   发布前缺省检索不可复得 → 人工/服务面 rp-kc 计分 + approve（双闸）→
#   发布后 MCP 检索复得（bm25 索引随行更新，无独立重建步）→
#   审批闸负例：未计分 approve 阻断 / 硬标记 approve 阻断 / 拒绝内容不可发布。
# 纪律：隔离 PG 容器（digest 钉死，独立端口/数据卷，零碰生产）；
#   一次性 caller 主库（rp-admin caller-add/key-gen，明文 key 仅内存）；
#   生产 migrations/platform*.db 指纹前后一致（红线）；容器用完即删。
# 全过 exit 0；任何 FAIL exit 1。前置：仓库根运行 + podman 可用（V-9 口径跳过）。
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO}"

PASS=0; FAIL=0
ck() { if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi; }
ck_contains() { if [[ "$3" == *"$2"* ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (未含: $2)"; fi; }

KC_BIN="target/debug/rp-kc"
MCP_BIN="target/debug/rp-mcp"
ADMIN_BIN="target/debug/rp-admin"

if ! command -v podman >/dev/null 2>&1; then
  echo "SKIP: podman unavailable (V-9，容器端到端保持本地执行)"; exit 0
fi
if [[ ! -x "$KC_BIN" || ! -x "$MCP_BIN" || ! -x "$ADMIN_BIN" ]]; then
  echo "构建 rp-kc / rp-mcp / rp-admin（--offline）..."
  cargo build --offline -p intel-db --bin rp-kc -p mcp-server --bin rp-mcp -p database --bin rp-admin >/dev/null 2>&1
fi
[[ -x "$KC_BIN" && -x "$MCP_BIN" && -x "$ADMIN_BIN" ]] || { echo "[FAIL] 二进制缺位"; exit 1; }

WORK="$(mktemp -d /tmp/rp_pub_replay.XXXX)"
PORT=$(( 20000 + RANDOM % 20000 ))
trap 'podman rm -f rip-p17-pub-replay >/dev/null 2>&1; if [[ -z "${RP_KEEP_WORK:-}" ]]; then chmod -R u+rwX "${WORK}" 2>/dev/null; rm -rf "${WORK}" 2>/dev/null; else echo "[keep] WORK=${WORK}"; fi' EXIT

FINGERPRINT_BEFORE=$(for f in migrations/platform*.db; do [[ -e "$f" ]] && sha256sum "$f" | awk '{print substr($1,1,16), "'"$f"'"}'; done | sort)

echo "== 1. 隔离 PG 容器（digest 钉死）"
podman rm -f rip-p17-pub-replay >/dev/null 2>&1
mkdir -p "${WORK}/pgdata"
podman run -d --name rip-p17-pub-replay -e POSTGRES_PASSWORD=pgselftest \
  -p "127.0.0.1:${PORT}:5432" -v "${WORK}/pgdata:/var/lib/postgresql/data" \
  "localhost/rip-pg17:latest@sha256:229189e4c66a2eb8ddd1fcf4bc08ee4726cacd8aee7445ec3c6f1499a357ef45" >/dev/null \
  && ck "隔离容器启动" "0" "0" || { echo "[FAIL] 隔离容器启动"; exit 1; }
READY=0
for i in $(seq 1 60); do
  podman exec rip-p17-pub-replay pg_isready -U postgres >/dev/null 2>&1 \
    && podman logs rip-p17-pub-replay 2>&1 | grep -q "PostgreSQL init process complete" && { READY=1; break; }
  sleep 2
done
ck "就绪（init complete）" "1" "$READY"
export RP_PG_DSN="postgres://postgres:pgselftest@127.0.0.1:${PORT}/postgres"

echo "== 2. 一次性 caller 主库（rp-admin caller-add / key-gen）"
MAIN="${WORK}/main.db"; AUX="${WORK}/aux.db"
CALLER_JSON=$("$ADMIN_BIN" caller-add --db "$MAIN" --name "pb012-replay-caller" --level T1)
CALLER_ID=$(printf '%s' "$CALLER_JSON" | python3 -c "import json,sys; print(json.load(sys.stdin)['caller_id'])")
KEY_JSON=$("$ADMIN_BIN" key-gen --db "$MAIN" --caller-id "$CALLER_ID" --label "pb012-selftest" 2>/dev/null)
MCP_TOKEN=$(printf '%s' "$KEY_JSON" | python3 -c "import json,sys; print(json.load(sys.stdin)['key'])")
ck "caller 登记 + key 签发（caller- 前缀）" "caller-" "${CALLER_ID:0:7}"
# aux 目录库（只读合并不触本用例主库）：最小 tool 表。
python3 - "$AUX" <<'PYEOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
c.execute("CREATE TABLE IF NOT EXISTS tool (tool_id TEXT PRIMARY KEY, name TEXT NOT NULL, family TEXT, lifecycle TEXT NOT NULL DEFAULT 'REGISTERED', governance TEXT NOT NULL DEFAULT 'ACTIVE', updated_at TEXT, input_schema TEXT, output_schema TEXT)")
c.execute("INSERT OR IGNORE INTO tool (tool_id, name, family, lifecycle, governance, updated_at) VALUES ('rip.kb.aux-selftest','Aux Selftest','cli','REGISTERED','ACTIVE','2026-01-01T00:00:00.000Z')")
c.commit(); c.close()
PYEOF

echo "== 3. MCP 供体提交（真实 rp-mcp，严格 envelope）"
TAG="pb012$(date +%s)"
TOKEN_OK="pubok${TAG}"
TOKEN_HARD="pubhard${TAG}"
SRC_TEXT="PB-012 发布回放来源正文 ${TOKEN_OK}（静态夹具，无网络请求）"
SRC_SHA=$(printf '%s' "$SRC_TEXT" | sha256sum | awk '{print $1}')
python3 - "${WORK}/mcp_in1.jsonl" "$MCP_TOKEN" "$TOKEN_OK" "$TOKEN_HARD" "$SRC_SHA" <<'PYEOF'
import json, sys, hashlib
out_path, token, tok_ok, tok_hard, src_sha = sys.argv[1:6]
def envelope(idem, title, content):
    return {
        "contract_version": "1.0",
        "idempotency_key": idem,
        "knowledge_type": "detection_method",
        "title": title,
        "summary": "PB-012 发布回放（自测夹具，静态构造）",
        "content": content,
        "confidence": 0.6,
        "source": {
            "type": "document",
            "title": "selftest source fixture",
            "uri": "file:///selftest/publication-replay-fixture.txt",
            "content_sha256": src_sha,
            "license": "unknown",
            "accessed_at": "2026-11-28T00:00:00Z",
        },
        "evidence": [{"quote_sha256": hashlib.sha256(content.encode()).hexdigest(),
                      "char_start": 0, "char_end": len(content)}],
        "risk_flags": [],
    }
lines = [
    {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"pb012-selftest","version":"0"}}},
    {"jsonrpc":"2.0","method":"notifications/initialized"},
    {"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"rip.submit_knowledge_candidate","arguments":envelope(f"pb012-{tok_ok}", f"PB012 回放 批准路径 {tok_ok}", f"知识正文 {tok_ok} 检索复得断言锚")}},
    {"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"rip.submit_knowledge_candidate","arguments":dict(envelope(f"pb012-{tok_hard}", f"PB012 回放 硬标记对照 {tok_hard}", f"知识正文 {tok_hard} 硬标记锚"), risk_flags=["POSSIBLE_PROMPT_INJECTION"])}},
    {"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"rip.review_knowledge_candidate","arguments":{"knowledge_id":"kc-pb012-fake","decision":"approve","reason":"MCP 面越权批准断言"}}},
    {"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"rip.search_knowledge","arguments":{"q":tok_ok,"limit":10}}},
]
with open(out_path, "w") as f:
    for l in lines:
        f.write(json.dumps(l, ensure_ascii=False) + "\n")
PYEOF
MCP_ENV=(env RP_MCP_TOKEN="$MCP_TOKEN" RP_DB_PATH="$MAIN" RP_AUX_DB="$AUX" RP_RUN_DB="$WORK/runs.db" RP_PLUGIN_DB=none RP_KC_BIN="$PWD/$KC_BIN")
MCP_ENV+=("RP_PG_DSN=$RP_PG_DSN")
"${MCP_ENV[@]}" "$MCP_BIN" < "${WORK}/mcp_in1.jsonl" > "${WORK}/mcp_out1.jsonl" 2>/dev/null
python3 - "${WORK}/mcp_out1.jsonl" "${WORK}/mcp1.json" <<'PYEOF'
import json, sys
out = {}
for line in open(sys.argv[1]):
    line = line.strip()
    if not line:
        continue
    try:
        m = json.loads(line)
    except Exception:
        continue
    if isinstance(m.get("id"), int):
        out[m["id"]] = m
json.dump(out, open(sys.argv[2], "w"))
PYEOF
KID=$(python3 -c "import json; d=json.load(open('${WORK}/mcp1.json')); print(json.loads(d['2']['result']['content'][0]['text'])['report']['knowledge_id'])")
KSTATUS=$(python3 -c "import json; d=json.load(open('${WORK}/mcp1.json')); print(json.loads(d['2']['result']['content'][0]['text'])['report']['review_status'])")
KID_HARD=$(python3 -c "import json; d=json.load(open('${WORK}/mcp1.json')); print(json.loads(d['3']['result']['content'][0]['text'])['report']['knowledge_id'])")
ck "MCP 供体提交回执（CANDIDATE）" "CANDIDATE" "$KSTATUS"
ck "MCP 提交落 kc- 前缀 id" "kc-" "${KID:0:3}"
MCP_APPROVE_TEXT=$(python3 -c "import json; d=json.load(open('${WORK}/mcp1.json')); print(d['4']['result']['content'][0]['text'])")
MCP_APPROVE_ISERR=$(python3 -c "import json; d=json.load(open('${WORK}/mcp1.json')); print(d['4']['result'].get('isError'))")
ck "MCP approve isError=true" "True" "$MCP_APPROVE_ISERR"
ck_contains "MCP approve 拒绝文案点名禁区" "MCP never approves" "$MCP_APPROVE_TEXT"
PRE_ABSENT=$(python3 -c "
import json
d=json.load(open('${WORK}/mcp1.json'))
t=json.loads(d['5']['result']['content'][0]['text'])
body=t if isinstance(t,list) else t.get('report',[])
print('ABSENT' if not any(h.get('knowledge_id')=='$KID' for h in body if isinstance(h,dict)) else 'HAS')
")
ck "发布前 MCP 缺省检索不可复得候选" "ABSENT" "$PRE_ABSENT"

echo "== 4. 审批闸负例（人工/服务面 rp-kc）"
# 4a. 未计分 approve 阻断。
"$KC_BIN" review --card "$KID" --decision approve --reason "未计分阻断断言" --reviewer human-pb012 >"${WORK}/g1.out" 2>/dev/null
RC1=$?
ck "未计分 approve exit 1" "1" "$RC1"
ck_contains "未计分 approve 文案点名质量分缺失" "not computed" "$(cat "${WORK}/g1.out")"
# 4b. 硬标记候选计分后仍阻断。
HARD_SCORE=$("$KC_BIN" score --card "$KID_HARD" | python3 -c "import json,sys; print(json.load(sys.stdin)['quality_score'])")
"$KC_BIN" review --card "$KID_HARD" --decision approve --reason "硬标记阻断断言" --reviewer human-pb012 >"${WORK}/g2.out" 2>/dev/null
RC2=$?
ck "硬标记 approve exit 1" "1" "$RC2"
ck_contains "硬标记 approve 文案点名 hard risk flag" "hard risk flag" "$(cat "${WORK}/g2.out")"

echo "== 5. 人工/服务批准（发布）+ 发布后 MCP 复得"
SCORE=$("$KC_BIN" score --card "$KID" | python3 -c "import json,sys; print(round(json.load(sys.stdin)['quality_score'],4))")
ck "供体卡计分过地板(>=0.35)" "True" "$(python3 -c "print('True' if $SCORE >= 0.35 else 'False')")"
"$KC_BIN" review --card "$KID" --decision approve --reason "PB-012 发布回放 人工批准" --reviewer human-pb012 >/dev/null
RC3=$?
ck "人工 approve exit 0" "0" "$RC3"
STATE=$("$KC_BIN" show --card "$KID" | python3 -c "import json,sys; print(json.load(sys.stdin)['review_status'])")
ck "发布态 APPROVED" "APPROVED" "$STATE"
# MCP 进程 2：发布后缺省检索复得。
python3 - "${WORK}/mcp_in2.jsonl" "$MCP_TOKEN" "$TOKEN_OK" <<'PYEOF'
import json, sys
out_path, token, tok_ok = sys.argv[1:4]
lines = [
    {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"pb012-selftest","version":"0"}}},
    {"jsonrpc":"2.0","method":"notifications/initialized"},
    {"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"rip.search_knowledge","arguments":{"q":tok_ok,"limit":10}}},
]
with open(out_path, "w") as f:
    for l in lines:
        f.write(json.dumps(l, ensure_ascii=False) + "\n")
PYEOF
"${MCP_ENV[@]}" "$MCP_BIN" < "${WORK}/mcp_in2.jsonl" > "${WORK}/mcp_out2.jsonl" 2>/dev/null
POST_ABSENT=$(python3 -c "
import json
out={}
for line in open('${WORK}/mcp_out2.jsonl'):
    line=line.strip()
    if not line: continue
    try: m=json.loads(line)
    except Exception: continue
    if isinstance(m.get('id'), int): out[m['id']]=m
t=json.loads(out[2]['result']['content'][0]['text'])
body=t if isinstance(t,list) else t.get('report',[])
ids=[h.get('knowledge_id') for h in body if isinstance(h,dict)]
print('PRESENT' if '$KID' in ids else 'ABSENT')
")
ck "发布后 MCP 缺省检索复得（无独立重建步）" "PRESENT" "$POST_ABSENT"
# 审计链：donor 写入即候选 + 人工批准两行留痕。
LOG=$("$KC_BIN" show --card "$KID" | python3 -c "
import json,sys
d=json.load(sys.stdin)
rows=d.get('review_log') or []
marks=[]
for r in rows:
    if r.get('to_status')=='CANDIDATE' and r.get('reviewer')=='$CALLER_ID': marks.append('DONOR')
    if r.get('to_status')=='APPROVED' and r.get('reviewer')=='human-pb012': marks.append('HUMAN')
print('+'.join(marks) or 'MISSING')
")
ck "审计链 donor+human 双留痕" "DONOR+HUMAN" "$LOG"

echo "== 6. 生产库红线指纹"
FINGERPRINT_AFTER=$(for f in migrations/platform*.db; do [[ -e "$f" ]] && sha256sum "$f" | awk '{print substr($1,1,16), "'"$f"'"}'; done | sort)
ck "migrations/platform*.db 指纹零写一致" "$FINGERPRINT_BEFORE" "$FINGERPRINT_AFTER"

echo "== 自验汇总: PASS=$PASS FAIL=$FAIL"
[[ "$FAIL" -eq 0 ]] || exit 1
