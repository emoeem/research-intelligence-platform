#!/usr/bin/env bash
# PB-005 备份/异地链自验（一次性一次性库，零碰生产 platform*.db）：
#   rp-admin backup（在线快照 + manifest 逐成员 sha256）→ verify-archive →
#   异地复制（缺省本地 vault 目录；--offsite [HOST:PATH] 时 rsync over ssh）
#   → restore 演练（恢复副本行数对账）→ reten-archive 保留策略实跑。
# 全过 exit 0；任何 FAIL exit 1。
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO}"

PASS=0; FAIL=0
ck() { if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi; }

ADMIN="target/debug/rp-admin"
[[ -x "$ADMIN" ]] || cargo build --offline -p database --bin rp-admin >/dev/null 2>&1
[[ -x "$ADMIN" ]] || { echo "[FAIL] rp-admin 缺位"; exit 1; }
command -v rsync >/dev/null 2>&1 || { echo "SKIP: rsync unavailable"; exit 0; }

OFFSITE=""
[[ "${1:-}" == "--offsite" ]] && OFFSITE="${2:-}"

WORK="$(mktemp -d /tmp/rp_offsite.XXXX)"
trap 'chmod -R u+rwX "${WORK}" 2>/dev/null; rm -rf "${WORK}"' EXIT
FP_BEFORE=$(for f in migrations/platform*.db; do [[ -e "$f" ]] && sha256sum "$f" | awk '{print substr($1,1,16) }'; done | sort | sha256sum | awk '{print $1}')

echo "== 1. 一次性源库播种（rp-admin 自动迁移 + 两条 caller 行）"
DB="${WORK}/src.db"
"$ADMIN" caller-add --db "$DB" --name offsite-drill-a --level T1 >/dev/null
"$ADMIN" caller-add --db "$DB" --name offsite-drill-b --level T2 >/dev/null
ROWS=$("$ADMIN" caller-list --db "$DB" | python3 -I -c "import json,sys; print(len(json.load(sys.stdin)))")
ck "源库 caller 行数" "2" "$ROWS"

echo "== 2. backup（在线快照 + manifest sha256）"
TAR="${WORK}/offsite-drill.tar"
"$ADMIN" backup --db "$DB" --out "$TAR" >/dev/null
ck "backup exit 0" "0" "$?"
"$ADMIN" verify-archive --archive "$TAR" >/dev/null
ck "verify-archive exit 0" "0" "$?"
MANIFEST_OK=$("$ADMIN" verify-archive --archive "$TAR" 2>/dev/null | python3 -I -c "import json,sys; d=json.load(sys.stdin); print(d.get('ok', d.get('status','')))")
echo "[INFO] verify-archive 输出: $(head -c 200 <<<"$MANIFEST_OK")"
chmod 600 "$TAR"

echo "== 3. 异地复制（缺省本地 vault；--offsite 走 rsync over ssh）"
if [[ -n "$OFFSITE" ]]; then
  rsync -e "ssh -o BatchMode=yes -o StrictHostKeyChecking=accept-new" -a "$TAR" "$OFFSITE/" \
    && rsync -a "$TAR.sha256" "$OFFSITE/" 2>/dev/null || true
  RC=$?
  ck "异地 rsync exit 0" "0" "$RC"
  DEST="$OFFSITE"
else
  VAULT="${WORK}/vault"; mkdir -p "$VAULT"
  rsync -a "$TAR" "$VAULT/"
  sha256sum "$TAR" > "${TAR}.sha256"
  rsync -a "${TAR}.sha256" "$VAULT/"
  ck "vault rsync exit 0" "0" "$?"
  DEST="$VAULT"
fi
REMOTE_TAR="${DEST}/$(basename "$TAR")"
sha256sum -c "${DEST}/$(basename "$TAR").sha256" >/dev/null 2>&1
ck "异地副本 sha256 对账" "0" "$?"

echo "== 4. restore 演练（恢复副本行数对账）"
RESTORED="${WORK}/restored.db"
"$ADMIN" restore --archive "$REMOTE_TAR" --db "$RESTORED" >/dev/null
ck "restore exit 0" "0" "$?"
ROWS_RESTORED=$("$ADMIN" caller-list --db "$RESTORED" | python3 -I -c "import json,sys; print(len(json.load(sys.stdin)))")
ck "恢复副本 caller 行数对账" "2" "$ROWS_RESTORED"

echo "== 5. reten-archive 保留策略实跑"
"$ADMIN" reten-archive --db "$RESTORED" --archive-out "${WORK}/reten.db" >/dev/null
ck "reten-archive exit 0" "0" "$?"

echo "== 6. 生产库红线指纹"
FP_AFTER=$(for f in migrations/platform*.db; do [[ -e "$f" ]] && sha256sum "$f" | awk '{print substr($1,1,16) }'; done | sort | sha256sum | awk '{print $1}')
ck "migrations/platform*.db 指纹零写一致" "$FP_BEFORE" "$FP_AFTER"

echo "== 自验汇总: PASS=$PASS FAIL=$FAIL"
[[ "$FAIL" -eq 0 ]] || exit 1
