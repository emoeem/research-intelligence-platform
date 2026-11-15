#!/usr/bin/env bash
# RIP3 P2-1 自验脚本（解析管线：rip-parser 容器族 + Parser Registry + IR 落库
# + 九态 workflow 定义，ADR-0003 裁决面）：
#   1. 容器族在位（digest 钉死引用 + manifest 三处同步一致性）；
#   2. detect/parse/ocr 三 op 容器冒烟（network=none + read-only rootfs）；
#   3. ZIP 安全判定负例（路径穿越 + 炸弹压缩比拒）；
#   4. 隔离 PG（rip-pg17 digest 钉死，独立端口/数据卷/对象目录，零碰生产）：
#      import → parse（驱动全链：detect 路由 → 容器 parse → IR 校验 →
#      blocks 落库 → parse_state 回填）→ 幂等跳过 → --force 重解析 →
#      parse-status → 注入样本 REVIEW_PENDING → approve；
#   5. 九态 workflow 定义：rp-tool register ×3 + rp-workflow put + run
#      （真容器两分支：干净样本 approve 直通；注入样本 quality_gate →
#      human 停等 → resume → quarantine end）；
#   6. parser_pipeline 集成测试（PG + 镜像在场实跑）；
#   7. 生产 migrations/platform*.db 指纹前后一致（红线）；
#   8. 清理容器（用完即删纪律）。
# 全过 exit 0；任何 FAIL exit 1。前置：仓库根运行 + podman 可用（V-9 口径跳过）。
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO}"

PASS=0; FAIL=0
ck() { if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi; }

BIN="target/debug/rp-doc"
TOOL_BIN="target/debug/rp-tool"
WF_BIN="target/debug/rp-workflow"
PARSER_IMAGE="localhost/rip-parser:latest@sha256:$(python3 -c "import json; print(json.load(open('containerfiles/rip-parser/rip.parser.docling.json'))['provenance']['image_digest'][7:])")"

if ! command -v podman >/dev/null 2>&1; then
  echo "SKIP: podman unavailable (V-9，容器端到端保持本地执行)"; exit 0
fi
for b in "$BIN" "$TOOL_BIN" "$WF_BIN"; do
  if [[ ! -x "$b" ]]; then
    echo "[FAIL] $b 不在位（先 cargo build --offline -p intel-db -p tool-registry -p workflow-engine）"; exit 1
  fi
done

WORK="$(mktemp -d /tmp/rp_p2_parser.XXXX)"
trap 'podman rm -f rip-p17-p2-selftest >/dev/null 2>&1; chmod -R u+rwX "${WORK}" 2>/dev/null; rm -rf "${WORK}" 2>/dev/null' EXIT

# 生产库指纹（前）
FINGERPRINT_BEFORE=$(for f in migrations/platform*.db; do [[ -e "$f" ]] && sha256sum "$f" | awk '{print substr($1,1,16), "'"$f"'"}'; done | sort)

echo "== 1. 容器族在位 + manifest 三处同步"
podman image exists "$PARSER_IMAGE" && ck "镜像 digest 钉死引用在位" "0" "0" || { echo "[FAIL] rip-parser 镜像缺席（${PARSER_IMAGE: -16}）"; exit 1; }
SYNC=$(python3 - <<'PYEOF'
import json, yaml, sys
digests = set()
for tid in ["rip.parser.detect", "rip.parser.docling", "rip.parser.ocr"]:
    j = json.load(open(f"containerfiles/rip-parser/{tid}.json"))
    y = yaml.safe_load(open(f"containerfiles/rip-parser/{tid}.yaml"))
    if j["runtime"]["image"] != y["runtime"]["image"] or j["id"] != y["id"]:
        print("DIVERGED"); sys.exit(0)
    digests.add(j["provenance"]["image_digest"])
readme = open("containerfiles/README.md").read()
if len(digests) == 1 and next(iter(digests))[8:] in readme:
    print("SYNCED")
else:
    print("DIVERGED")
PYEOF
)
ck "manifest yaml/json + README 台账一致" "SYNCED" "$SYNC"

echo "== 2. detect/parse/ocr 三 op 容器冒烟（network=none）"
SAMPLE_DIR="rip-data/parse-smoke/in"
if [[ ! -f "${SAMPLE_DIR}/sample.md" ]]; then
  echo "[FAIL] 冒烟样本缺席（${SAMPLE_DIR}，运行时数据受控环境准备）"; exit 1
fi
SMOKE="${WORK}/smoke"; mkdir -p "$SMOKE/out"
REQ='{"run_id":"rn-selftest-detect","input":{"path":"/task/in/sample.md"}}'
DET=$(podman run --rm -i --network none --read-only --userns=keep-id --cap-drop ALL \
  -v "$REPO/${SAMPLE_DIR}:/task/in:ro" -v "$SMOKE/out:/task/out:rw" \
  "$PARSER_IMAGE" python3 /app/parser_tools.py detect <<< "$REQ" 2>/dev/null)
ck "detect op（魔法数判定 markdown）" "markdown" \
  "$(echo "$DET" | python3 -c 'import json,sys; print(json.load(sys.stdin)["data"]["format"])' 2>/dev/null)"
REQ='{"run_id":"rn-selftest-parse","input":{"path":"/task/in/sample.md"}}'
PRS=$(podman run --rm -i --network none --read-only --userns=keep-id --cap-drop ALL \
  -v "$REPO/${SAMPLE_DIR}:/task/in:ro" -v "$SMOKE/out:/task/out:rw" \
  "$PARSER_IMAGE" python3 /app/parser_tools.py parse <<< "$REQ" 2>/dev/null)
ck "parse op（IR 信封 + 注入标记）" "POSSIBLE_PROMPT_INJECTION" \
  "$(echo "$PRS" | python3 -c 'import json,sys; print(json.load(sys.stdin)["data"]["flags"][0])' 2>/dev/null)"
ck "parse op（native_markdown 引擎如实记录）" "native_markdown" \
  "$(echo "$PRS" | python3 -c 'import json,sys; print(json.load(sys.stdin)["data"]["parser"])' 2>/dev/null)"
REQ='{"run_id":"rn-selftest-zip","input":{"path":"/task/in/evil.zip"}}'
EVIL=$(podman run --rm -i --network none --read-only --userns=keep-id --cap-drop ALL \
  -v "$REPO/${SAMPLE_DIR}:/task/in:ro" -v "$SMOKE/out:/task/out:rw" \
  "$PARSER_IMAGE" python3 /app/parser_tools.py detect <<< "$REQ" 2>/dev/null)
ck "ZIP 负例（穿越 + 炸弹拒）" "False" \
  "$(echo "$EVIL" | python3 -c 'import json,sys; print(json.load(sys.stdin)["data"]["zip_verdict"]["safe"])' 2>/dev/null)"

echo "== 3. 隔离 PG + rp-doc parse 全链"
PORT=$(( (RANDOM % 2000) + 18000 ))
podman rm -f rip-p17-p2-selftest >/dev/null 2>&1
mkdir -p "${WORK}/pgdata" "${WORK}/objects"
podman run -d --name rip-p17-p2-selftest -e POSTGRES_PASSWORD=pgselftest \
  -p "127.0.0.1:${PORT}:5432" -v "${WORK}/pgdata:/var/lib/postgresql/data" \
  "localhost/rip-pg17:latest@sha256:229189e4c66a2eb8ddd1fcf4bc08ee4726cacd8aee7445ec3c6f1499a357ef45" >/dev/null \
  && ck "隔离容器启动" "0" "0" || { echo "[FAIL] 隔离容器启动"; exit 1; }
READY=0
for i in $(seq 1 60); do
  podman exec rip-p17-p2-selftest pg_isready -U postgres >/dev/null 2>&1 \
    && podman logs rip-p17-p2-selftest 2>&1 | grep -q "PostgreSQL init process complete" && { READY=1; break; }
  sleep 2
done
ck "就绪（init complete）" "1" "$READY"
export RP_PG_DSN="postgres://postgres:pgselftest@127.0.0.1:${PORT}/postgres"
export RIP_OBJECT_STORE_DIR="${WORK}/objects"
export RIP_DATA_DIR="${WORK}/ripdata"

IMP=$("$BIN" import --file "${SAMPLE_DIR}/sample.md" --title "P2 selftest 注入样本" 2>/dev/null)
DOC=$(echo "$IMP" | python3 -c 'import json,sys; print(json.load(sys.stdin)["document_id"])' 2>/dev/null)
[[ -n "$DOC" ]] && ck "import（对象新建）" "1" "1" || { echo "[FAIL] import"; exit 1; }

PRSOUT=$("$BIN" parse --document "$DOC" 2>/dev/null)
STATE=$(echo "$PRSOUT" | python3 -c 'import json,sys; print(json.load(sys.stdin)["parse_state"])' 2>/dev/null)
ck "parse 全链（注入样本判 REVIEW_PENDING）" "REVIEW_PENDING" "$STATE"
BLOCKS=$(echo "$PRSOUT" | python3 -c 'import json,sys; print(json.load(sys.stdin)["blocks"])' 2>/dev/null)
[[ -n "$BLOCKS" && "$BLOCKS" -ge 2 ]] && ck "blocks 落库（>=2）" "1" "1" || ck "blocks 落库（>=2）" "1" "0"
REV=$(echo "$IMP" | python3 -c 'import json,sys; print(json.load(sys.stdin)["revision_id"])' 2>/dev/null)
AP=$("$BIN" approve --revision "$REV" 2>/dev/null | grep -c APPROVED)
ck "approve（REVIEW_PENDING -> APPROVED）" "1" "$AP"
SKIP=$("$BIN" parse --document "$DOC" 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin).get("replayed",""))' 2>/dev/null)
ck "幂等跳过（已提交版本）" "True" "$SKIP"
FORCE=$("$BIN" parse --document "$DOC" --force >/dev/null 2>&1; echo $?)
ck "--force 重解析" "0" "$FORCE"
STATE2=$("$BIN" parse-status --document "$DOC" 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin)["parse_state"])' 2>/dev/null)
ck "--force 重解析后质量门重判（注入样本回 REVIEW_PENDING）" "REVIEW_PENDING" "$STATE2"
"$BIN" approve --revision "$REV" >/dev/null 2>&1
ST=$("$BIN" parse-status --document "$DOC" 2>/dev/null | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d["parse_state"], d["blocks"] >= 2)' 2>/dev/null)
ck "parse-status 视图（再 approve 后）" "APPROVED True" "$ST"

echo "== 4. 九态 workflow 定义（真容器两分支）"
for id in rip.parser.detect rip.parser.docling rip.parser.ocr; do
  "$TOOL_BIN" register --manifest "containerfiles/rip-parser/${id}.json" --db "${WORK}/wf.db" >/dev/null 2>&1
done
PUT=$("$WF_BIN" put workflows/parse-pipeline-v1.json --db "${WORK}/wf.db" 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin)["result"])' 2>/dev/null)
ck "定义 put" "PUT" "$PUT"
CLEAN="${SAMPLE_DIR}/clean.md"
if [[ ! -f "$CLEAN" ]]; then
  printf '# 干净文档\n\n## 正常章节\n\n无指令式内容的普通段落。\n' > "$CLEAN"
fi
CLEAN_ABS=$(realpath "$CLEAN")
RUN1=$("$WF_BIN" run wf-parse-pipeline-v1 --input "{\"container_path\": \"/task/in/clean.md\", \"input_host_files\": {\"clean.md\": \"$CLEAN_ABS\"}}" --db "${WORK}/wf.db" 2>/dev/null \
  | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d["state"], d["nodes"][-1]["state"])' 2>/dev/null)
ck "干净样本 approve 直通分支" "succeeded succeeded" "$RUN1"
INJ="${SAMPLE_DIR}/sample.md"; INJ_ABS=$(realpath "$INJ")
RUN2OUT=$("$WF_BIN" run wf-parse-pipeline-v1 --input "{\"container_path\": \"/task/in/sample.md\", \"input_host_files\": {\"sample.md\": \"$INJ_ABS\"}}" --db "${WORK}/wf.db" 2>/dev/null)
RUN2=$(echo "$RUN2OUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d["state"])' 2>/dev/null)
ck "注入样本 quality_gate 分支（human 停等）" "paused" "$RUN2"
RUN2ID=$(echo "$RUN2OUT" | python3 -c 'import json,sys; print(json.load(sys.stdin)["run_id"])' 2>/dev/null)
RES=$("$WF_BIN" resume "$RUN2ID" --input '{"review_decision": "approved"}' --db "${WORK}/wf.db" 2>/dev/null \
  | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d["state"])' 2>/dev/null)
ck "resume 后走完 quarantine end" "succeeded" "$RES"

echo "== 5. parser_pipeline 集成测试（PG + 镜像在场实跑）"
export RP_TEST_PG_DSN="$RP_PG_DSN"
ITCNT=$(cargo test --offline -q -p intel-db --test parser_pipeline 2>&1 | grep -c "test result: ok. 5 passed")
ck "集成测试 5/5" "1" "$ITCNT"

echo "== 6. 生产库红线（指纹后）"
FINGERPRINT_AFTER=$(for f in migrations/platform*.db; do [[ -e "$f" ]] && sha256sum "$f" | awk '{print substr($1,1,16), "'"$f"'"}'; done | sort)
ck "platform*.db 指纹一致（零写）" "$FINGERPRINT_BEFORE" "$FINGERPRINT_AFTER"

echo "== 结果: PASS=$PASS FAIL=$FAIL"
if [[ "$FAIL" -gt 0 ]]; then exit 1; fi
echo "P2-1 selftest 全过"
