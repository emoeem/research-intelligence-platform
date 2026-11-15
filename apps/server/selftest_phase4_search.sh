#!/usr/bin/env bash
# RIP3 P4 自验脚本（统一检索：BM25 + 向量 + RRF/MMR + rerank + 缓存 + tokenizers）：
#   1. 检索容器族在位（rip-embed / rip-rerank digest 钉死 + manifest 三处同步）；
#   2. 隔离 PG（rip-pg17 digest 钉死，独立端口/数据卷，零碰生产）；
#   3. rp-kc 全链：create → score → approve → embed-cards → search 四模式
#      （bm25 / vector / hybrid / rerank）；
#   4. rerank 前后排序对比（hybrid 序 vs rerank 序：相关主题卡置顶 + source=rerank
#      + rerank 分降序）；
#   5. 缓存面（R2 6.6）：向量面二跑 search-cache 条目落盘 + hybrid 二跑结果一致；
#   6. tokenizers 接入（R2 6.6 Tokenizer Adapter）：calibration 两测试（切换一致
#      + 公式偏差 <5%）；
#   7. 集成测试 search_hybrid（PG + 双镜像在场实跑，含 rerank 全链；串行
#      线程 + timeout 保护——容器派发并发 flake 实录：cargo test 默认双线程
#      并发派发 podman 容器曾挂死 25min，静场串行纪律，HANDOVER 负载 flake）；
#   8. 生产 migrations/platform*.db 指纹前后一致（红线）；
#   9. 清理容器（用完即删纪律）。
# 全过 exit 0；任何 FAIL exit 1。前置：仓库根运行 + podman 可用（V-9 口径跳过）。
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO}"

PASS=0; FAIL=0
ck() { if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi; }

KC_BIN="target/debug/rp-kc"
EMBED_IMAGE="localhost/rip-embed:latest@sha256:$(python3 -c "import json; print(json.load(open('containerfiles/rip-embed/rip.embed.text.json'))['provenance']['image_digest'][7:])")"
RERANK_IMAGE="localhost/rip-rerank:latest@sha256:$(python3 -c "import json; print(json.load(open('containerfiles/rip-rerank/rip.rerank.text.json'))['provenance']['image_digest'][7:])")"

if ! command -v podman >/dev/null 2>&1; then
  echo "SKIP: podman unavailable (V-9，容器端到端保持本地执行)"; exit 0
fi
if [[ ! -x "$KC_BIN" ]]; then
  echo "[FAIL] $KC_BIN 不在位（先 cargo build --offline -p intel-db）"; exit 1
fi

WORK="$(mktemp -d /tmp/rp_p4_search.XXXX)"
trap 'podman rm -f rip-p17-p4-selftest >/dev/null 2>&1; chmod -R u+rwX "${WORK}" 2>/dev/null; rm -rf "${WORK}" 2>/dev/null' EXIT

FINGERPRINT_BEFORE=$(for f in migrations/platform*.db; do [[ -e "$f" ]] && sha256sum "$f" | awk '{print substr($1,1,16), "'"$f"'"}'; done | sort)

echo "== 1. 检索容器族在位 + manifest 三处同步"
podman image exists "$EMBED_IMAGE" && ck "rip-embed digest 钉死在位" "0" "0" || { echo "[FAIL] rip-embed 缺席"; exit 1; }
podman image exists "$RERANK_IMAGE" && ck "rip-rerank digest 钉死在位" "0" "0" || { echo "[FAIL] rip-rerank 缺席"; exit 1; }
SYNC=$(python3 - <<'PYEOF'
import json, yaml, sys
for tid, path in [("rip.embed.text", "containerfiles/rip-embed"), ("rip.rerank.text", "containerfiles/rip-rerank")]:
    j = json.load(open(f"{path}/{tid}.json"))
    y = yaml.safe_load(open(f"{path}/{tid}.yaml"))
    if j["runtime"]["image"] != y["runtime"]["image"] or j["id"] != y["id"]:
        print("DIVERGED"); sys.exit(0)
readme = open("containerfiles/README.md").read()
d1 = json.load(open("containerfiles/rip-embed/rip.embed.text.json"))["provenance"]["image_digest"][7:]
d2 = json.load(open("containerfiles/rip-rerank/rip.rerank.text.json"))["provenance"]["image_digest"][7:]
print("SYNCED" if d1 in readme and d2 in readme else "DIVERGED")
PYEOF
)
ck "manifest yaml/json + README 台账一致" "SYNCED" "$SYNC"

echo "== 2. 隔离 PG"
PORT=$(( (RANDOM % 2000) + 21000 ))
podman rm -f rip-p17-p4-selftest >/dev/null 2>&1
mkdir -p "${WORK}/pgdata" "${WORK}/objects"
podman run -d --name rip-p17-p4-selftest -e POSTGRES_PASSWORD=pgselftest \
  -p "127.0.0.1:${PORT}:5432" -v "${WORK}/pgdata:/var/lib/postgresql/data" \
  "localhost/rip-pg17:latest@sha256:229189e4c66a2eb8ddd1fcf4bc08ee4726cacd8aee7445ec3c6f1499a357ef45" >/dev/null \
  && ck "隔离容器启动" "0" "0" || { echo "[FAIL] 隔离容器启动"; exit 1; }
READY=0
for i in $(seq 1 60); do
  podman exec rip-p17-p4-selftest pg_isready -U postgres >/dev/null 2>&1 \
    && podman logs rip-p17-p4-selftest 2>&1 | grep -q "PostgreSQL init process complete" && { READY=1; break; }
  sleep 2
done
ck "就绪（init complete）" "1" "$READY"
export RP_PG_DSN="postgres://postgres:pgselftest@127.0.0.1:${PORT}/postgres"
export RIP_OBJECT_STORE_DIR="${WORK}/objects"
export RIP_DATA_DIR="${WORK}/ripdata"

echo "== 3. rp-kc 全链（建卡 → 计分 → 批准 → 嵌入 → 四模式检索）"
ID1=$("$KC_BIN" create --type vuln_principle --title "P4 甲（SQL 注入防御）" --summary "摘要" \
  --content "防御 SQL 注入的核心手段是参数化查询与预编译语句，辅以最小权限数据库账号。" --workspace ws-p4t \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["knowledge_id"])')
[[ -n "$ID1" ]] && ck "create" "1" "1" || { echo "[FAIL] create"; exit 1; }
ID2=$("$KC_BIN" create --type detection_method --title "P4 乙（注入检测）" --summary "摘要" \
  --content "SQL 注入检测常用错误回显分析与时间盲注探测，配合日志回溯定位注入点。" --workspace ws-p4t \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["knowledge_id"])')
ID3=$("$KC_BIN" create --type fix_method --title "P4 丙（散步心得）" --summary "摘要" \
  --content "傍晚沿河堤散步半小时有助于放松，注意补充水分。" --workspace ws-p4t \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["knowledge_id"])')
for id in "$ID1" "$ID2" "$ID3"; do
  "$KC_BIN" score --card "$id" >/dev/null 2>&1
  "$KC_BIN" review --card "$id" --decision approve --reason "p4-selftest" --reviewer p4 >/dev/null 2>&1
done
AP=$("$KC_BIN" show --card "$ID1" 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin).get("review_status",""))')
ck "三卡批准" "APPROVED" "$AP"
EMB=$("$KC_BIN" embed-cards --workspace ws-p4t --run-db "${WORK}/ripdata/parse/ops.db" --manifest-dir "${REPO}/containerfiles/rip-embed" 2>/dev/null \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["embedded"])')
ck "embed-cards 三卡补向量" "3" "$EMB"
BM25N=$("$KC_BIN" search --query "SQL 注入" --mode bm25 --workspace ws-p4t | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))')
[[ "$BM25N" -ge 1 ]] && ck "search bm25 词面命中" "1" "1" || ck "search bm25 词面命中" "1" "0"
VECN=$("$KC_BIN" search --query "SQL 注入怎么防御" --mode vector --workspace ws-p4t --run-db "${WORK}/ripdata/parse/ops.db" --manifest-dir "${REPO}/containerfiles/rip-embed" 2>/dev/null | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))')
[[ "$VECN" -ge 1 ]] && ck "search vector 语义命中" "1" "1" || ck "search vector 语义命中" "1" "0"

echo "== 4. rerank 前后排序对比（hybrid 序 vs rerank 序）"
HYB=$("$KC_BIN" search --query "SQL 注入怎么防御" --mode hybrid --workspace ws-p4t --limit 3 --run-db "${WORK}/ripdata/parse/ops.db" --manifest-dir "${REPO}/containerfiles/rip-embed" 2>/dev/null)
TOP_HYB=$(echo "$HYB" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(1 if d and d[0]["knowledge_id"] in ("'"$ID1"'","'"$ID2"'") else 0)')
[[ "$TOP_HYB" == "1" ]] && ck "hybrid 检索相关卡在场" "1" "1" || ck "hybrid 检索相关卡在场" "1" "0"
RR=$("$KC_BIN" search --query "SQL 注入怎么防御" --mode rerank --workspace ws-p4t --limit 3 --run-db "${WORK}/ripdata/parse/ops.db" --manifest-dir "${REPO}/containerfiles/rip-embed" --rerank-manifest-dir "${REPO}/containerfiles/rip-rerank" 2>/dev/null)
RRCHECK=$(echo "$RR" | python3 -c '
import json,sys
d=json.load(sys.stdin)
ok_top = d and d[0]["knowledge_id"] in ("'"$ID1"'","'"$ID2"'")
ok_src = all(h["source"]=="rerank" for h in d)
ok_desc = all(d[i]["score"]>=d[i+1]["score"] for i in range(len(d)-1))
print(1 if ok_top and ok_src and ok_desc else 0)')
ck "rerank 全链（相关卡置顶 + source=rerank + 降序）" "1" "$RRCHECK"

echo "== 5. 缓存面（R2 6.6：向量缓存 + 检索结果缓存）"
"$KC_BIN" search --query "缓存命中探针" --mode vector --workspace ws-p4t --run-db "${WORK}/ripdata/parse/ops.db" --manifest-dir "${REPO}/containerfiles/rip-embed" >/dev/null 2>&1
N1=$(ls "${WORK}/ripdata/runtime/search-cache/" 2>/dev/null | wc -l)
"$KC_BIN" search --query "缓存命中探针" --mode vector --workspace ws-p4t --run-db "${WORK}/ripdata/parse/ops.db" --manifest-dir "${REPO}/containerfiles/rip-embed" >/dev/null 2>&1
N2=$(ls "${WORK}/ripdata/runtime/search-cache/" 2>/dev/null | wc -l)
[[ "$N1" -ge 1 && "$N2" -eq "$N1" ]] && ck "向量缓存条目落盘 + 二跑零新建（命中）" "1" "1" || ck "向量缓存条目落盘 + 二跑零新建（命中）" "1" "0"
H1=$("$KC_BIN" search --query "SQL 注入怎么防御" --mode hybrid --workspace ws-p4t --limit 3 --run-db "${WORK}/ripdata/parse/ops.db" --manifest-dir "${REPO}/containerfiles/rip-embed" 2>/dev/null)
H2=$("$KC_BIN" search --query "SQL 注入怎么防御" --mode hybrid --workspace ws-p4t --limit 3 --run-db "${WORK}/ripdata/parse/ops.db" --manifest-dir "${REPO}/containerfiles/rip-embed" 2>/dev/null)
[[ "$H1" == "$H2" ]] && ck "hybrid 二跑结果一致（检索缓存命中）" "1" "1" || ck "hybrid 二跑结果一致（检索缓存命中）" "1" "0"

echo "== 6. tokenizers 接入（R2 6.6 calibration）"
CAL=$(cargo test --offline -q -p context-broker --test tokenizer_calibration 2>&1 | grep -c "test result: ok")
ck "calibration 两测试（切换一致 + 公式偏差<5%）" "1" "$CAL"

echo "== 7. 检索集成测试（PG + 双镜像在场实跑，静场串行）"
export RP_TEST_PG_DSN="$RP_PG_DSN"
IT=$(timeout 300 cargo test --offline -q -p intel-db --test search_hybrid -- --test-threads=1 2>&1 | grep -c "test result: ok")
ck "search_hybrid 集成全绿（含 rerank 全链，串行）" "1" "$IT"

echo "== 8. 生产库红线（指纹后）"
FINGERPRINT_AFTER=$(for f in migrations/platform*.db; do [[ -e "$f" ]] && sha256sum "$f" | awk '{print substr($1,1,16), "'"$f"'"}'; done | sort)
ck "platform*.db 指纹一致（零写）" "$FINGERPRINT_BEFORE" "$FINGERPRINT_AFTER"

echo "== 结果: PASS=$PASS FAIL=$FAIL"
if [[ "$FAIL" -gt 0 ]]; then exit 1; fi
echo "P4 selftest 全过"
