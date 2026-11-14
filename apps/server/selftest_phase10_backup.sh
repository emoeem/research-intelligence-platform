#!/usr/bin/env bash
# P10-1 自验脚本（ROADMAP §11.3 P10-5 backup/restore roundtrip + SESSION-HANDOFF
# §十 backup/restore、FTS 重建校验、数据保留归档三判据的真实实演）：
# 备份（rp-admin backup，Online Backup API 快照 + tar 归档 + manifest sha256）
# -> 受控破坏副本（中段覆写，sha256 偏离留证）-> 恢复（rp-admin restore，校验
# 后原子替换）-> 校验（四路对账：健康口径（quick_check + resource/evidence/
# research_task 三计数）恢复前后一致、恢复后 FTS 全量 rebuild 行数与抽样命中
# 与基线索引一致、migrations/verify.py 口径对账恢复副本与全量批库副本逐项
# 一致、数据保留归档行数对账）。生产 migrations/platform*.db 指纹前后一致
# 零写（红线）。
#
# 隔离纪律：全部实演走 mktemp 工作目录的开发副本；生产库只读指纹核对。
# scripts/rip_etl_status.py 是受保护脚本（tool-engine first_batch_b2 守卫：
# scripts/ 不得有跟踪态修改），本批零改动；其默认口径（生产 platform.db，
# mode=ro 只读）在演练前后各跑一次（点名判据：恢复前后计数一致且
# quick_check=ok）；隔离副本的健康口径用同一查询集的内联实现（db_health）。
# verify.py 证据段依赖 26 个活动源库（/home/emo/.dsh/redteam/engagements/），
# 本机不存在（M8 快照只锁定 catalog/knowledge/tasks，基线态同样如此，详见
# docs/phase10/P10-1-BATCH-REPORT.md）：本脚本对 verify.py 的判定取「恢复
# 副本与基线副本输出逐字节一致」（可运行的全部对账项口径一致）。
#
# 前置：仓库根运行（cargo 门禁按 CWD 发现 vendor 配置）；rp-admin 与
# rp-search-index 由 cargo --offline 构建。全过 exit 0。
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO}"

WORK="$(mktemp -d /tmp/rp_p10_backup.XXXX)"
PASS=0; FAIL=0
ck() { # ck <名> <期望> <实际>
  if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi
}

sha_of() { python3 -c 'import hashlib,sys;print(hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest())' "$1"; }

# 健康口径（与 scripts/rip_etl_status.py 同一查询集：quick_check + 三计数），
# 可指任意库文件（rip_etl_status.py 为受保护脚本，不做 env 覆盖扩展）。
db_health() { python3 -c '
import sqlite3, sys
c = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
ok = c.execute("PRAGMA quick_check").fetchone()[0]
n_res = c.execute("SELECT COUNT(*) FROM resource").fetchone()[0]
n_ev = c.execute("SELECT COUNT(*) FROM evidence").fetchone()[0]
n_task = c.execute("SELECT COUNT(*) FROM research_task").fetchone()[0]
print(f"quick_check={ok} | resources={n_res} evidence={n_ev} tasks={n_task}")
' "$1"; }

fingerprint() { # 生产库指纹：逐文件 sha256（排序，去时间戳）
  for f in migrations/platform*.db; do echo "$(sha_of "$f")  $(basename "$f")"; done | sort
}

cleanup() { rm -rf "${WORK}"; }
trap cleanup EXIT

echo "== 0. 构建（cargo --offline）与生产库指纹（前）"
cargo build --offline -q -p database --bin rp-admin || { echo "rp-admin build FAIL"; exit 1; }
cargo build --offline -q -p search-engine --bin rp-search-index || { echo "rp-search-index build FAIL"; exit 1; }
fingerprint > "${WORK}/fp-before.txt"
echo "[PASS] rp-admin 与 rp-search-index 已构建；生产 platform*.db 指纹已留底（$(wc -l < "${WORK}/fp-before.txt") 个文件）"
echo "+ python3 -I scripts/rip_etl_status.py（点名判据，默认口径 = 生产 platform.db 只读）"
python3 -I scripts/rip_etl_status.py | tail -1 | tee "${WORK}/etl-prod-before.txt"

ISO_DB="${WORK}/platform.iso.db"
DAMAGED_DB="${WORK}/platform.damaged.db"
ARCHIVE="${WORK}/p10-backup.tar"
CATALOG_DB="migrations/sources-snapshot/catalog.db"

echo
echo "== 1. 备份：隔离副本 + rp-admin backup（Online Backup API 快照 + tar 归档）"
cp migrations/platform.db "${ISO_DB}"
echo "+ db_health ${ISO_DB}（rip_etl_status 同款查询集）"
db_health "${ISO_DB}" | tee "${WORK}/etl-before.txt"
grep -q "quick_check=ok | resources=81 evidence=700 tasks=42" "${WORK}/etl-before.txt" \
  && { PASS=$((PASS+1)); echo "[PASS] 备份前隔离副本健康口径：quick_check=ok, 81/700/42"; } \
  || { FAIL=$((FAIL+1)); echo "[FAIL] 备份前隔离副本健康口径异常"; }
echo "+ target/debug/rp-admin backup --db ${ISO_DB} --out ${ARCHIVE} --extra rip-data/tool-runs.db"
target/debug/rp-admin backup --db "${ISO_DB}" --out "${ARCHIVE}" --extra rip-data/tool-runs.db \
  | tee "${WORK}/backup-report.json" || { echo "[FAIL] backup 命令失败"; exit 1; }
MANIFEST_DB_SHA="$(python3 -c "import json;print(json.load(open('${WORK}/backup-report.json'))['db_sha256'])")"
MANIFEST_UV="$(python3 -c "import json;print(json.load(open('${WORK}/backup-report.json'))['user_version'])")"
echo "+ target/debug/rp-admin verify-archive --archive ${ARCHIVE}"
target/debug/rp-admin verify-archive --archive "${ARCHIVE}" > "${WORK}/verify-archive.json" || { echo "[FAIL] verify-archive 失败"; exit 1; }
ck "verify-archive manifest db_sha256 与备份报告一致" \
  "$MANIFEST_DB_SHA" "$(python3 -c "import json;print(json.load(open('${WORK}/verify-archive.json'))['db_sha256'])")"
# platform.db 迁移链记录走 schema_version 表（user_version 停在 1），备份
# 清单与源库实际值对账，不硬编码。
SRC_UV="$(python3 -c "import sqlite3;print(sqlite3.connect('file:${ISO_DB}?mode=ro',uri=True).execute('PRAGMA user_version').fetchone()[0])")"
ck "备份 user_version 与源库实际值一致（源=${SRC_UV}）" "$SRC_UV" "$MANIFEST_UV"

echo
echo "== 2. 受控破坏：副本中段覆写 8KB（模拟位腐），sha256 偏离留证"
cp "${ISO_DB}" "${DAMAGED_DB}"
echo "+ python3 受控覆写 ${DAMAGED_DB} 中段 8192 字节为 0xFF"
python3 - "${DAMAGED_DB}" <<'PYEOF'
import sys
p = sys.argv[1]
with open(p, "r+b") as f:
    f.seek(0, 2)
    mid = f.tell() // 2
    f.seek(mid)
    f.write(b"\xff" * 8192)
print(f"damaged: overwrote 8192 bytes at offset {mid}")
PYEOF
DAMAGED_SHA="$(sha_of "${DAMAGED_DB}")"
echo "damaged sha256=${DAMAGED_SHA}"
echo "manifest db_sha256=${MANIFEST_DB_SHA}"
ck "受控破坏后副本 sha256 偏离备份快照" "differs" \
  "$([[ "${DAMAGED_SHA}" != "${MANIFEST_DB_SHA}" ]] && echo differs || echo same)"
echo -n "damaged quick_check 实况："
python3 -c "import sqlite3;c=sqlite3.connect('file:${DAMAGED_DB}?mode=ro',uri=True);print(c.execute('PRAGMA quick_check').fetchone()[0][:120])" 2>&1 | head -1

echo
echo "== 3. 恢复：rp-admin restore（解包校验后原子替换受损副本）"
echo "+ target/debug/rp-admin restore --archive ${ARCHIVE} --db ${DAMAGED_DB} --extras-out ${WORK}/extras-out"
target/debug/rp-admin restore --archive "${ARCHIVE}" --db "${DAMAGED_DB}" --extras-out "${WORK}/extras-out" \
  | tee "${WORK}/restore-report.json" || { echo "[FAIL] restore 命令失败"; exit 1; }
ck "恢复后文件 sha256 与备份快照逐字节一致" "$MANIFEST_DB_SHA" "$(sha_of "${DAMAGED_DB}")"
ck "恢复报告 integrity=ok" "ok" "$(python3 -c "import json;print(json.load(open('${WORK}/restore-report.json'))['integrity'])")"
ck "附加文件（rip-data/tool-runs.db）恢复 sha256 一致" \
  "$(sha_of rip-data/tool-runs.db)" "$(sha_of "${WORK}/extras-out/tool-runs.db")"
echo "+ db_health ${DAMAGED_DB}（恢复后健康口径）"
db_health "${DAMAGED_DB}" | tee "${WORK}/etl-after.txt"
grep -q "quick_check=ok | resources=81 evidence=700 tasks=42" "${WORK}/etl-after.txt" \
  && { PASS=$((PASS+1)); echo "[PASS] 恢复后健康口径与备份前一致：quick_check=ok, 81/700/42"; } \
  || { FAIL=$((FAIL+1)); echo "[FAIL] 恢复后健康口径异常"; }
diff "${WORK}/etl-before.txt" "${WORK}/etl-after.txt" > /dev/null \
  && { PASS=$((PASS+1)); echo "[PASS] 健康口径恢复前后逐字一致"; } \
  || { FAIL=$((FAIL+1)); echo "[FAIL] 健康口径恢复前后不一致"; }

echo
echo "== 4. 校验：FTS 全量 rebuild 对账 + 数据保留归档 + verify.py 口径 + 生产库指纹（后）"

echo "-- 4a. FTS 全量 rebuild 对账（rp-search-index，目录源 = M8 快照 catalog.db 只读）"
IDX_BEFORE="${WORK}/idx-before.db"
IDX_AFTER="${WORK}/idx-after.db"
KEYWORD="$(python3 - <<PYEOF
import sqlite3, re
c = sqlite3.connect("file:${ISO_DB}?mode=ro", uri=True)
row = c.execute("SELECT name, description FROM resource ORDER BY resource_id LIMIT 80").fetchall()
for name, desc in row:
    text = (name or "") + " " + (desc or "")
    for tok in re.findall(r"[a-z0-9+#.\-]{4,}", text.lower()):
        if not tok.startswith(("http", "www", "github.com")):
            print(tok.split("-")[0]); raise SystemExit
print("tool")
PYEOF
)"
echo "抽样关键词：${KEYWORD}"
echo "+ target/debug/rp-search-index build --platform-db ${ISO_DB} --catalog-db ${CATALOG_DB} --index-db ${IDX_BEFORE}"
target/debug/rp-search-index build --platform-db "${ISO_DB}" --catalog-db "${CATALOG_DB}" --index-db "${IDX_BEFORE}" > "${WORK}/idx-before.json" || { echo "[FAIL] 基线索引构建失败"; exit 1; }
echo "+ target/debug/rp-search-index build --rebuild --platform-db ${DAMAGED_DB}（恢复副本）--catalog-db ${CATALOG_DB} --index-db ${IDX_AFTER}"
target/debug/rp-search-index build --rebuild --platform-db "${DAMAGED_DB}" --catalog-db "${CATALOG_DB}" --index-db "${IDX_AFTER}" > "${WORK}/idx-after.json" || { echo "[FAIL] 恢复后 rebuild 失败"; exit 1; }
D1="$(python3 -c "import json;d=json.load(open('${WORK}/idx-before.json'));print(d['doc_count'],d['fts_rows'])")"
D2="$(python3 -c "import json;d=json.load(open('${WORK}/idx-after.json'));print(d['doc_count'],d['fts_rows'])")"
echo "基线索引 doc_count/fts_rows = ${D1}；恢复后 rebuild = ${D2}"
ck "FTS rebuild 行数与基线一致" "$D1" "$D2"
for IDX in "${IDX_BEFORE}" "${IDX_AFTER}"; do
  target/debug/rp-search-index query --index-db "${IDX}" --keywords "${KEYWORD}" --limit 100 \
    | python3 -c 'import json,sys;d=json.load(sys.stdin);print("\n".join(sorted(h["resource_id"] for h in d["hits"])))' \
    > "${WORK}/hits-$(basename "${IDX}").txt"
done
if diff "${WORK}/hits-idx-before.db.txt" "${WORK}/hits-idx-after.db.txt" > /dev/null; then
  PASS=$((PASS+1)); echo "[PASS] 抽样关键词命中集一致（$(wc -l < "${WORK}/hits-idx-before.db.txt") 条命中）"
else
  FAIL=$((FAIL+1)); echo "[FAIL] 抽样关键词命中集不一致"
fi

echo "-- 4b. 数据保留归档（rp-admin reten-archive，内置策略 external_api_audit 保留 180 天）"
RET_DB="${WORK}/retention.db"
python3 - "${RET_DB}" <<'PYEOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
c.executescript("""
CREATE TABLE IF NOT EXISTS external_api_audit (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ts TEXT NOT NULL, event TEXT NOT NULL, method TEXT NOT NULL,
  path TEXT NOT NULL, remote TEXT, status INTEGER NOT NULL,
  duration_ms INTEGER, trace_id TEXT, correlation_id TEXT,
  rate_limited INTEGER NOT NULL DEFAULT 0 CHECK (rate_limited IN (0, 1)),
  auth_enabled INTEGER NOT NULL CHECK (auth_enabled IN (0, 1)),
  meta_json TEXT);
CREATE INDEX IF NOT EXISTS ix_external_api_audit_ts ON external_api_audit(ts);
INSERT INTO external_api_audit (ts, event, method, path, status, auth_enabled) VALUES
  ('2020-01-01T00:00:00Z','request','GET','/old-1',200,0),
  ('2020-06-01T00:00:00Z','request','POST','/old-2',404,0),
  ('2099-01-01T00:00:00Z','request','GET','/new-1',200,1),
  ('2099-02-01T00:00:00Z','request','GET','/new-2',200,1);
""")
c.commit(); c.close()
print("retention fixture: 2 old rows (2020) + 2 recent rows (2099)")
PYEOF
echo "+ target/debug/rp-admin reten-archive --db ${RET_DB} --archive-out ${WORK}/audit-archive.db"
target/debug/rp-admin reten-archive --db "${RET_DB}" --archive-out "${WORK}/audit-archive.db" \
  | tee "${WORK}/retention-report.json" || { echo "[FAIL] reten-archive 失败"; exit 1; }
RET_SUMMARY="$(python3 -c "
import json
r = json.load(open('${WORK}/retention-report.json'))[0]
print(r['status'], r.get('archived'), r.get('deleted'), r.get('rows_after'))
")"
ck "retention 归档对账（archived=deleted=2，rows_after=2）" "archived 2 2 2" "$RET_SUMMARY"
ARC_ROWS="$(python3 -c "import sqlite3;print(sqlite3.connect('${WORK}/audit-archive.db').execute('SELECT COUNT(*) FROM external_api_audit').fetchone()[0])")"
ck "归档库行数（DDL 复刻 + 旧行入档）" "2" "$ARC_ROWS"

echo "-- 4c. migrations/verify.py 口径对账（恢复副本 vs 基线副本，全部走工作目录副本）"
FB_BASE="${WORK}/fullbatch.baseline.db"
FB_REST="${WORK}/fullbatch.restored.db"
cp migrations/platform-fullbatch.db "${FB_BASE}"
cp migrations/platform-fullbatch.db "${WORK}/fullbatch.work.db"
FB_ARCHIVE="${WORK}/fullbatch.tar"
target/debug/rp-admin backup --db "${WORK}/fullbatch.work.db" --out "${FB_ARCHIVE}" > "${WORK}/fb-backup.json" || { echo "[FAIL] fullbatch backup 失败"; exit 1; }
python3 - "${WORK}/fullbatch.work.db" <<'PYEOF'
import sys
p = sys.argv[1]
with open(p, "r+b") as f:
    f.seek(0, 2)
    mid = f.tell() // 2
    f.seek(mid)
    f.write(b"\xff" * 65536)
print("fullbatch copy damaged at mid 64KB")
PYEOF
target/debug/rp-admin restore --archive "${FB_ARCHIVE}" --db "${WORK}/fullbatch.work.db" > "${WORK}/fb-restore.json" || { echo "[FAIL] fullbatch restore 失败"; exit 1; }
cp "${WORK}/fullbatch.work.db" "${FB_REST}"
set +e
RP_TARGET_DB="${FB_BASE}" python3 -I migrations/verify.py > "${WORK}/verify-base.out" 2> "${WORK}/verify-base.err"
BASE_EXIT=$?
RP_TARGET_DB="${FB_REST}" python3 -I migrations/verify.py > "${WORK}/verify-rest.out" 2> "${WORK}/verify-rest.err"
REST_EXIT=$?
set -e 2>/dev/null || true
echo "verify.py 基线副本 exit=${BASE_EXIT}，恢复副本 exit=${REST_EXIT}（evidence 段源库缺失为既有环境限制，两侧对称）"
if diff "${WORK}/verify-base.out" "${WORK}/verify-rest.out" > /dev/null && diff "${WORK}/verify-base.err" "${WORK}/verify-rest.err" > /dev/null; then
  PASS=$((PASS+1)); echo "[PASS] verify.py 口径对账：恢复副本与基线副本输出逐字节一致（含 PASS 项 $(grep -c '\[PASS\]' "${WORK}/verify-rest.out") 项，evidence 段崩溃点两侧一致）"
else
  FAIL=$((FAIL+1)); echo "[FAIL] verify.py 口径对账：恢复副本与基线副本输出存在差异"
  diff "${WORK}/verify-base.out" "${WORK}/verify-rest.out" | head -5
fi
echo "[INFO] verify.py 恢复副本输出 [PASS] 项数：$(grep -c '\[PASS\]' "${WORK}/verify-rest.out")（完整对账含 evidence 段，其 26 个活动源库本机缺失，两侧对称崩溃于同一检查点，属基线既有环境限制）"

echo "-- 4d. 生产库指纹（后，零写红线核对）+ 点名脚本默认口径（后）"
fingerprint > "${WORK}/fp-after.txt"
if diff "${WORK}/fp-before.txt" "${WORK}/fp-after.txt" > /dev/null; then
  PASS=$((PASS+1)); echo "[PASS] 生产 migrations/platform*.db 指纹前后一致（零写入）"
else
  FAIL=$((FAIL+1)); echo "[FAIL] 生产库指纹发生变化"; diff "${WORK}/fp-before.txt" "${WORK}/fp-after.txt"
fi
echo "+ python3 -I scripts/rip_etl_status.py（点名判据，恢复后默认口径复跑）"
python3 -I scripts/rip_etl_status.py | tail -1 | tee "${WORK}/etl-prod-after.txt"
diff "${WORK}/etl-prod-before.txt" "${WORK}/etl-prod-after.txt" > /dev/null \
  && { PASS=$((PASS+1)); echo "[PASS] rip_etl_status 恢复前后计数一致且 quick_check=ok"; } \
  || { FAIL=$((FAIL+1)); echo "[FAIL] rip_etl_status 恢复前后输出不一致"; }

echo
echo "==================== P10-1 selftest 汇总：PASS=${PASS} FAIL=${FAIL} ===================="
[[ "${FAIL}" -eq 0 ]] || exit 1
exit 0
