#!/usr/bin/env bash
# PB-004 qrels 纪律自验：AI-only 行不得进 human Ground Truth 轨（双轨强制）。
#   1. AI 标注夹具 + 缺省 ai-assisted-characterization → 通过，human_ground_truth=false；
#   2. AI 标注夹具 + --label-mode human-ground-truth → exit 3 拒绝；
#   3. 人工标注夹具 + human-ground-truth → 通过，human_ground_truth=true；
#   4. 无标注行 + human-ground-truth → exit 3（unlabeled 不算人工）；
#   5. 真实 v3 qrels（301 行 ai-assist）缺省轨实跑 → 通过并产出标注构成。
# 全过 exit 0；任何 FAIL exit 1。
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO}"
EVAL="scripts/security-intel/evaluate_retrieval.py"

PASS=0; FAIL=0
ck() { if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi; }

WORK="$(mktemp -d /tmp/rp_qrel_gate.XXXX)"
trap 'rm -rf "${WORK}"' EXIT

mk_rows() { # $1=file $2=annotator ("" = omit)
  python3 - "$1" "$2" <<'PYEOF'
import json, sys
path, ann = sys.argv[1], sys.argv[2]
rows = []
for rank in (1, 2, 3):
    row = {"query_id": "q1", "category": "cve_exact", "rank": rank,
           "relevance": 3 if rank == 1 else 1, "doc_id": f"d{rank}",
           "title": f"doc {rank}", "source_system": "s1",
           "governance_status": "ACTIVE", "include_candidates": False}
    if ann:
        row["annotator"] = ann
    rows.append(row)
open(path, "w").write("\n".join(json.dumps(r, ensure_ascii=False) for r in rows) + "\n")
PYEOF
}

echo "== 1. AI 标注 + 缺省 ai-assisted 轨"
mk_rows "${WORK}/ai.jsonl" "ai-assist"
python3 -I "$EVAL" --labeled "${WORK}/ai.jsonl" > "${WORK}/ai-ai.out.json" 2>&1
RC=$?
ck "缺省轨 exit 0" "0" "$RC"
HGT=$(python3 -I -c "import json; print(json.load(open('${WORK}/ai-ai.out.json'))['label_track']['human_ground_truth'])")
ck "human_ground_truth=false" "False" "$HGT"

echo "== 2. AI 标注 + human-ground-truth 轨（必须拒绝）"
python3 -I "$EVAL" --labeled "${WORK}/ai.jsonl" --label-mode human-ground-truth > "${WORK}/ai-human.out" 2>&1
RC=$?
ck "AI 行进人工轨 exit 3" "3" "$RC"
grep -q "cannot be promoted to human Ground Truth" "${WORK}/ai-human.out" && ck "拒绝文案点名提升禁区" "0" "0" || ck "拒绝文案点名提升禁区" "0" "1"

echo "== 3. 人工标注 + human-ground-truth 轨"
mk_rows "${WORK}/human.jsonl" "human-review"
python3 -I "$EVAL" --labeled "${WORK}/human.jsonl" --label-mode human-ground-truth > "${WORK}/human-human.out.json" 2>&1
RC=$?
ck "人工轨 exit 0" "0" "$RC"
HGT=$(python3 -I -c "import json; print(json.load(open('${WORK}/human-human.out.json'))['label_track']['human_ground_truth'])")
ck "human_ground_truth=true" "True" "$HGT"

echo "== 4. 无标注行 + human-ground-truth 轨（必须拒绝）"
mk_rows "${WORK}/none.jsonl" ""
python3 -I "$EVAL" --labeled "${WORK}/none.jsonl" --label-mode human-ground-truth > "${WORK}/none-human.out" 2>&1
ck "unlabeled 进人工轨 exit 3" "3" "$?"

echo "== 5. 真实 v3 qrels（301 行 ai-assist）缺省轨实跑"
V3="docs/knowledge-governance/retrieval-qrel-labeled-v3-2026-11-28.jsonl"
python3 -I "$EVAL" --labeled "$V3" > "${WORK}/v3.out.json" 2>&1
RC=$?
ck "v3 缺省轨 exit 0" "0" "$RC"
AIROWS=$(python3 -I -c "import json; print(json.load(open('${WORK}/v3.out.json'))['label_track']['annotator_counts'].get('ai-assist', 0))")
ck "v3 ai-assist 行构成登记" "301" "$AIROWS"
MRR=$(python3 -I -c "import json; print(json.load(open('${WORK}/v3.out.json'))['mrr'])")
echo "[INFO] v3 ai-assisted 轨 mrr=$MRR（AI 辅助轨指标，不得表述为人工 GT）"
cp "${WORK}/v3.out.json" docs/knowledge-governance/retrieval-eval-v3-ai-assisted-2026-11-28.json

echo "== 自验汇总: PASS=$PASS FAIL=$FAIL"
[[ "$FAIL" -eq 0 ]] || exit 1
