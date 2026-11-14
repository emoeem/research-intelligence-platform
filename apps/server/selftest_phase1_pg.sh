#!/usr/bin/env bash
# RIP3 P1-1 自验脚本（PG 基座 + SQLite WAL 落地：ADR-0001 / 12-STORAGE 纪律）：
#   1. 隔离容器（localhost/rip-pg17 digest 钉死，独立端口/数据卷/对象目录，零碰生产）；
#   2. 三扩展在位（vector/pg_search/pg_jieba）+ jiebacfg 中文分词实测；
#   3. refinery 迁移应用 + 幂等二跑零变化；
#   4. rp-doc 全链：import（对象新建）→ 同内容二导入（复用，不重复存储）→
#      add-source（引用计数）→ new-revision（SUPERSEDED 历史可访问）→
#      delete-source（引用计数 -1）→ health（版本/扩展/行数）；
#   5. SQLite WAL：可写运行库经 rp-tool 打开后 journal_mode=wal；
#      生产 migrations/platform*.db 指纹前后一致且 journal_mode 保持非 wal（红线）；
#   6. 清理容器（用完即删纪律）。
# 全过 exit 0；任何 FAIL exit 1。前置：仓库根运行 + podman 可用（V-9 口径跳过）。
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO}"

PASS=0; FAIL=0
ck() { if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi; }

BIN="target/debug/rp-doc"
TOOL_BIN="target/debug/rp-tool"
IMAGE="localhost/rip-pg17:latest@sha256:229189e4c66a2eb8ddd1fcf4bc08ee4726cacd8aee7445ec3c6f1499a357ef45"

if ! command -v podman >/dev/null 2>&1; then
  echo "SKIP: podman unavailable (V-9，容器端到端保持本地执行)"; exit 0
fi
if [[ ! -x "$BIN" ]]; then
  echo "[FAIL] rp-doc 不在位（先 cargo build --offline -p intel-db --bin rp-doc）"; exit 1
fi

WORK="$(mktemp -d /tmp/rp_p1_pg.XXXX)"
trap 'podman rm -f rip-p17-selftest >/dev/null 2>&1; chmod -R u+rwX "${WORK}" 2>/dev/null; rm -rf "${WORK}" 2>/dev/null' EXIT

# 生产库指纹（前）+ journal_mode（前）
FINGERPRINT_BEFORE=$(for f in migrations/platform*.db; do [[ -e "$f" ]] && sha256sum "$f" | awk '{print substr($1,1,16), "'"$f"'"}'; done | sort)

echo "== 1. 隔离容器（digest 钉死：${IMAGE: -12}）"
PORT=$(( (RANDOM % 2000) + 15000 ))
podman rm -f rip-p17-selftest >/dev/null 2>&1
mkdir -p "${WORK}/pgdata"
podman run -d --name rip-p17-selftest -e POSTGRES_PASSWORD=pgselftest \
  -p "127.0.0.1:${PORT}:5432" -v "${WORK}/pgdata:/var/lib/postgresql/data" "$IMAGE" >/dev/null \
  && ck "容器启动" "0" "0" || { echo "[FAIL] 容器启动"; exit 1; }

READY=0
for i in $(seq 1 60); do
  podman exec rip-p17-selftest pg_isready -U postgres >/dev/null 2>&1 \
    && podman logs rip-p17-selftest 2>&1 | grep -q "PostgreSQL init process complete" && { READY=1; break; }
  sleep 2
done
ck "就绪（init complete）" "1" "$READY"

export RP_PG_DSN="postgres://postgres:pgselftest@127.0.0.1:${PORT}/postgres"
export RIP_OBJECT_STORE_DIR="${WORK}/objects"

echo "== 2. 三扩展 + jiebacfg 中文分词"
EXTS=$("$BIN" health 2>/dev/null | python3 -c "import json,sys; print(','.join(json.load(sys.stdin)['extensions']))")
ck "三扩展在位" "pg_jieba,pg_search,vector" "$EXTS"
JIEBA=$("$BIN" health >/dev/null 2>&1; podman exec rip-p17-selftest psql -U postgres -tAc \
  "SELECT to_tsvector('jiebacfg', '研究一个开源项目的架构与质量')::text" 2>/dev/null)
echo "$JIEBA" | grep -q "'架构'" && ck "jiebacfg 分词（架构/研究命中）" "0" "0" || ck "jiebacfg 分词（架构/研究命中）" "0" "1"

echo "== 3. 迁移应用 + 幂等"
MIG1=$("$BIN" health 2>/dev/null | python3 -c "import json,sys; print(json.load(sys.stdin)['migration_version'])")
ck "迁移链版本 = 2" "2" "$MIG1"
MIG2=$("$BIN" health 2>/dev/null | python3 -c "import json,sys; print(json.load(sys.stdin)['migration_version'])")
ck "迁移幂等（二跑同版本）" "$MIG1" "$MIG2"

echo "== 4. rp-doc 全链（CAS / 多来源 / 版本链）"
printf "# P1-1 selftest\n\n同内容多来源与版本链样本文档。\n" > "${WORK}/doc.md"
R1=$("$BIN" import --file "${WORK}/doc.md" --title "selftest 样本" --source-type git \
  --source-url "https://repo.example/selftest" 2>/dev/null)
DOC=$(echo "$R1" | python3 -c "import json,sys; print(json.load(sys.stdin)['document_id'])")
SRC=$(echo "$R1" | python3 -c "import json,sys; print(json.load(sys.stdin)['source_id'])")
HASH=$(echo "$R1" | python3 -c "import json,sys; print(json.load(sys.stdin)['content_hash'])")
ck "import 新建对象" "True" "$(echo "$R1" | python3 -c "import json,sys; print(json.load(sys.stdin)['content_object_created'])")"
R2=$("$BIN" import --file "${WORK}/doc.md" --title "selftest 样本乙" --source-type url \
  --source-url "https://mirror.example/selftest" 2>/dev/null)
ck "同内容复用（不重复存储）" "False" "$(echo "$R2" | python3 -c "import json,sys; print(json.load(sys.stdin)['content_object_created'])")"
"$BIN" add-source --document "$DOC" --source-type url --source-url "https://third.example/selftest" >/dev/null 2>&1
REF=$(podman exec rip-p17-selftest psql -U postgres -tAc \
  "SELECT ref_count FROM content_objects WHERE content_hash = '$HASH'" 2>/dev/null | tr -d '[:space:]')
ck "引用计数 = 5（2 import×(来源+版本) + 1 add-source）" "5" "$REF"
OBJ=$(find "${WORK}/objects" -type f | wc -l)
ck "对象存储单文件（不重复存储）" "1" "$OBJ"
printf "# P1-1 selftest v2\n\n更新正文。\n" > "${WORK}/doc2.md"
"$BIN" new-revision --document "$DOC" --file "${WORK}/doc2.md" --parser-version "native-1.0" >/dev/null 2>&1
STATUSES=$(podman exec rip-p17-selftest psql -U postgres -tAc \
  "SELECT string_agg(revision_status, ',' ORDER BY revision_number) FROM document_revisions WHERE document_id = '$DOC'" 2>/dev/null)
ck "版本链（1=SUPERSEDED,2=CURRENT）" "SUPERSEDED,CURRENT" "$STATUSES"
"$BIN" delete-source --source "$SRC" >/dev/null 2>&1
REF2=$(podman exec rip-p17-selftest psql -U postgres -tAc \
  "SELECT ref_count FROM content_objects WHERE content_hash = '$HASH'" 2>/dev/null)
ck "删来源引用计数 -1" "4" "$REF2"
STILL=$(find "${WORK}/objects" -type f | wc -l)
ck "对象仍在位（账面保护，不误删）" "2" "$STILL"

echo "== 5. SQLite WAL（可写运行库）+ 生产库红线"
printf "# WAL 样本\n" > "${WORK}/wal.md"
RP_RUN_DB="${WORK}/tool-runs.db" "$TOOL_BIN" register --manifest manifests/tools/rip.doc.parse.json --db "${WORK}/tool-runs.db" >/dev/null 2>&1
WAL_MODE=$(sqlite3 "${WORK}/tool-runs.db" "PRAGMA journal_mode" 2>/dev/null || python3 -c "
import sqlite3, sys
c = sqlite3.connect('${WORK}/tool-runs.db')
print(c.execute('PRAGMA journal_mode').fetchone()[0])")
ck "运行库 journal_mode = wal" "wal" "$WAL_MODE"
PROD_MODE=$(python3 -c "
import sqlite3
c = sqlite3.connect('file:migrations/platform.db?mode=ro', uri=True)
print(c.execute('PRAGMA journal_mode').fetchone()[0])")
ck "生产库 journal_mode 保持非 wal（指纹红线）" "delete" "$PROD_MODE"
FINGERPRINT_AFTER=$(for f in migrations/platform*.db; do [[ -e "$f" ]] && sha256sum "$f" | awk '{print substr($1,1,16), "'"$f"'"}'; done | sort)
ck "生产库指纹前后一致（零写红线）" "$FINGERPRINT_BEFORE" "$FINGERPRINT_AFTER"

echo
echo "==================== P1-1 selftest 汇总：PASS=$PASS FAIL=$FAIL ===================="
[[ "$FAIL" == "0" ]] && exit 0 || exit 1
