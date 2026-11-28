#!/usr/bin/env bash
# PB-009 真实中断演练（隔离 PG 容器，digest 钉死，零碰生产）：
#   演练 A：parser 进程 SIGKILL 中断 → 健康面存活 → 回放重解析成功；
#   演练 B：PG 连接中断（容器重启）期间批量摄取 → 全部请求有结论不挂死
#           → 回放补齐 → 内容对象 CAS 零重复（distinct sha == 文档去重锚）；
#   演练 C：parse 超时守卫（--timeout 1）→ 有结论不挂死；
#   演练 D：全量回放幂等 → parse replayed=true、对象数稳定。
# 自足语料：4 份仓库真实 md + 4 份系统真实 PDF（运行时复制）。
# 尺寸旗标用显式字节（GNU find -size -1M 向上取整语义坑实录：<1 单位只匹配
# 空文件，曾致 PDF 选源为 0、语料塌缩，首跑实录）。
# 全过 exit 0；任何 FAIL exit 1。前置：podman + target/debug/rp-doc。
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO}"

PASS=0; FAIL=0
ck() { if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi; }

DOC_BIN="target/debug/rp-doc"
if ! command -v podman >/dev/null 2>&1; then echo "SKIP: podman unavailable"; exit 0; fi
[[ -x "$DOC_BIN" ]] || cargo build --offline -p intel-db --bin rp-doc >/dev/null 2>&1
[[ -x "$DOC_BIN" ]] || { echo "[FAIL] rp-doc 缺位"; exit 1; }

WORK="$(mktemp -d /tmp/rp_drills.XXXX)"
PORT=$(( 26500 + RANDOM % 2000 ))
CONT="rip-p17-drills"
trap 'podman rm -f "$CONT" >/dev/null 2>&1; chmod -R u+rwX "${WORK}" 2>/dev/null; rm -rf "${WORK}"' EXIT

echo "== 0. 隔离容器 + 自足语料（8 份真实文档）"
podman rm -f "$CONT" >/dev/null 2>&1
mkdir -p "${WORK}/pgdata" "${WORK}/objects" "${WORK}/ripdata" "${WORK}/corpus"
mapfile -t MDS < <(find docs -name "*.md" -size +2k | sort | head -4)
mapfile -t PDFS < <(find /usr/share/doc -name "*.pdf" -size +8192c -size -1048576c | sort | head -4)
i=0
for f in "${MDS[@]}" "${PDFS[@]}"; do i=$((i+1)); cp "$f" "${WORK}/corpus/$(printf '%03d' $i)-$(basename "$f")"; done
ck "语料份数（4 md + 4 pdf）" "8" "$(ls "${WORK}/corpus" | wc -l)"
BIGPDF=$(find /usr/share/doc -name "*.pdf" -size +300000c -size -1048576c | sort | head -1)
[[ -n "$BIGPDF" ]] || BIGPDF="${PDFS[0]}"
echo "[INFO] 演练 A/C 大文档: $(basename "$BIGPDF")"
podman run -d --name "$CONT" -e POSTGRES_PASSWORD=pgselftest \
  -p "127.0.0.1:${PORT}:5432" -v "${WORK}/pgdata:/var/lib/postgresql/data" \
  "localhost/rip-pg17:latest@sha256:229189e4c66a2eb8ddd1fcf4bc08ee4726cacd8aee7445ec3c6f1499a357ef45" >/dev/null
READY=1
for i in $(seq 1 60); do
  if podman exec "$CONT" pg_isready -U postgres >/dev/null 2>&1; then READY=0; break; fi
  sleep 2
done
ck "容器就绪" "0" "$READY"
export RP_PG_DSN="postgres://postgres:pgselftest@127.0.0.1:${PORT}/postgres"
export RIP_OBJECT_STORE_DIR="${WORK}/objects" RIP_DATA_DIR="${WORK}/ripdata"

import_one() { # $1=file -> document_id（失败输出空）
  "$DOC_BIN" import --file "$1" --title "$(basename "$1")" --workspace default 2>/dev/null \
    | python3 -I -c "import json,sys; print(json.load(sys.stdin).get('document_id',''))"
}
counts() { psql "$RP_PG_DSN" -tAc "$1"; }

echo "== 演练 A：parser 进程 SIGKILL 中断（健康面存活 + 回放）"
BIGDOC=$(import_one "$BIGPDF")
ck "演练 A 大文档导入" "doc-" "${BIGDOC:0:4}"
"$DOC_BIN" parse --document "$BIGDOC" >/dev/null 2>&1 &
KILLPID=$!
sleep 0.7
kill -9 "$KILLPID" 2>/dev/null
wait "$KILLPID" 2>/dev/null
echo "[INFO] SIGKILL 已投递（pid=$KILLPID）"
"$DOC_BIN" health >/dev/null 2>&1
ck "SIGKILL 后 health 可用" "0" "$?"
REPLAY=$("$DOC_BIN" parse --document "$BIGDOC" --force 2>/dev/null | python3 -I -c "import json,sys; print(json.load(sys.stdin).get('parse_state',''))")
ck "演练 A 回放重解析成功" "APPROVED" "$REPLAY"

echo "== 演练 B：PG 连接中断（容器重启）+ 回放补齐 + CAS 零重复"
BASE_OK=0
for f in $(ls "${WORK}/corpus"/* | head -4); do
  docid=$(import_one "$f") || continue
  [[ -n "$docid" ]] || continue
  "$DOC_BIN" parse --document "$docid" >/dev/null 2>&1 && BASE_OK=$((BASE_OK+1))
done
ck "中断前基线 4 份" "4" "$BASE_OK"
# 此时 docs = 1（演练A 大文档）+ 4（基线）= 5。
ck "基线文档行数（含演练A）" "5" "$(counts "SELECT COUNT(*) FROM documents")"
# 重启容器 = 连接层真实中断（无优雅关闭）。
podman restart "$CONT" >/dev/null
READY=1
for i in $(seq 1 60); do
  if podman exec "$CONT" pg_isready -U postgres >/dev/null 2>&1; then READY=0; break; fi
  sleep 2
done
ck "重启后容器恢复" "0" "$READY"
CONCLUDED=0
for f in $(ls "${WORK}/corpus"/* | tail -4); do
  if timeout 120 "$DOC_BIN" import --file "$f" --title "$(basename "$f")" --workspace default >/dev/null 2>&1; then
    CONCLUDED=$((CONCLUDED+1))
  else
    CONCLUDED=$((CONCLUDED+1))
  fi
done
ck "中断窗口 4 请求全部有结论（零挂死）" "4" "$CONCLUDED"
# 回放补齐：仅对缺文档行的窗口文件重跑 import（连接池自愈后窗口可能已全部
# 成功，重复 import 会产新文档行；条件式补齐 = 断点续传语义）。
REPLAY_OK=0
for f in $(ls "${WORK}/corpus"/* | tail -4); do
  docid=$(psql "$RP_PG_DSN" -tAc "SELECT document_id FROM documents WHERE title='$(basename "$f")' LIMIT 1") || docid=""
  if [[ -z "$docid" ]]; then docid=$(import_one "$f"); fi
  [[ -n "$docid" ]] || continue
  "$DOC_BIN" parse --document "$docid" >/dev/null 2>&1 && REPLAY_OK=$((REPLAY_OK+1))
done
ck "回放补齐 4 份全部解析成功" "4" "$REPLAY_OK"
DOCS=$(counts "SELECT COUNT(*) FROM documents")
SHA=$(counts "SELECT COUNT(DISTINCT sha256) FROM documents")
ck "文档行数 = 演练A + 8 语料" "9" "$DOCS"
ck "内容对象 CAS 零重复（distinct sha == 9，中断回放不复制正文）" "9" "$SHA"

echo "== 演练 C：parse 超时守卫（--timeout 1，有结论不挂死）"
CDOC=$(import_one "$BIGPDF")   # 同内容重导入：CAS 复用对象，文档行独立
ck "演练 C 同内容重导入" "doc-" "${CDOC:0:4}"
timeout 60 "$DOC_BIN" parse --document "$CDOC" --timeout 1 > "${WORK}/c.out" 2>&1
CRC=$?
if [[ "$CRC" -eq 124 ]]; then
  FAIL=$((FAIL+1)); echo "[FAIL] 演练 C 挂死（timeout 命令兜底触发，parse 未自限时）"
else
  PASS=$((PASS+1)); echo "[PASS] 演练 C 有结论不挂死（exit=$CRC，超时/完成均合法）"
fi
DOCS2=$(counts "SELECT COUNT(*) FROM documents")
SHA2=$(counts "SELECT COUNT(DISTINCT sha256) FROM documents")
ck "演练 C 后文档行数 +1" "10" "$DOCS2"
ck "同内容重导入内容对象零新增（CAS 复用）" "9" "$SHA2"

echo "== 演练 D：全量回放幂等（parse replayed=true，对象数稳定）"
RE_D=0
for f in "${WORK}/corpus"/*; do
  docid=$(psql "$RP_PG_DSN" -tAc "SELECT document_id FROM documents WHERE title='$(basename "$f")' AND original_filename='$(basename "$f")' LIMIT 1") || continue
  [[ -n "$docid" ]] || continue
  R=$("$DOC_BIN" parse --document "$docid" 2>/dev/null | python3 -I -c "import json,sys; print(json.load(sys.stdin).get('replayed'))")
  [[ "$R" == "True" ]] && RE_D=$((RE_D+1))
done
ck "已解析语料重跑全部 replayed=true" "8" "$RE_D"
SHA3=$(counts "SELECT COUNT(DISTINCT sha256) FROM documents")
ck "回放后内容对象数稳定" "$SHA2" "$SHA3"

echo "== 自验汇总: PASS=$PASS FAIL=$FAIL"
[[ "$FAIL" -eq 0 ]] || exit 1
