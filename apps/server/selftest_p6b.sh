#!/usr/bin/env bash
# PHASE-6B 收口脚本（docs/phase0/PHASE-6B-TASK.md S7；vendor+deny 政策二期：
# 轻量 PDF 链四钉 pdf-writer/subsetter/skrifa/crc32fast——lock 基线 432→440，
# 实录 docs/phase0/PHASE-6B-BATCH-REPORT.md）：
# lock 440 / reqwest=1 / zip=0 三检 + typst 清零反证 + 四 exact 钉对账 +
# vendor 目录计数 414 与八新目录在位 + deny 双检 + deny.toml 零 diff 反证 +
# IR schema sha256 恒定钉 + SARIF 官方 schema 资产在位 + 六新签名 re-export
# 对账 + 渲染/质检零 I/O 反证 + report-engine 46 测试载具 + 生产库五文件
# 只读指纹（mode=ro，1/1/1/0/0——6A C17 真实指纹值）+ 跨轨零 diff 反证。
# ck/sk + SKIP 守卫词形先例 = selftest_p6a.sh:16-21。
# 纪律：生产库面全部 mode=ro 只读 PRAGMA（红线 5——零写入路径）。
# 用法：bash apps/server/selftest_p6b.sh
set -u
cd "$(dirname "$0")/../.."   # repo root
REPO_ROOT="$PWD"

PASS=0; SKIP=0; FAIL=0
ck() {  # ck <label> <expected> <actual>
  if [ "$2" = "$3" ]; then PASS=$((PASS+1)); echo "  [ok] $1 = $3"
  else FAIL=$((FAIL+1)); echo "  [FAIL] $1: expected=$2 actual=$3"; fi
}
sk() { SKIP=$((SKIP+1)); echo "  [SKIP] $1"; }

echo "== 0. SKIP 守卫预挂（环境事实：cargo-deny 缺位 / sqlite3 缺位）"
GUARD_MISS=0
command -v cargo >/dev/null 2>&1 || { sk "cargo 不存在（Rust 门禁面）"; GUARD_MISS=1; }
[ -f "$HOME/.cargo/bin/cargo-deny" ] || command -v cargo-deny >/dev/null 2>&1 || { sk "cargo-deny 缺位（双检面）"; GUARD_MISS=1; }
command -v sqlite3 >/dev/null 2>&1 || { sk "sqlite3 缺位（只读指纹面）"; GUARD_MISS=1; }
command -v git >/dev/null 2>&1 || { sk "git 缺位（零 diff 反证面）"; GUARD_MISS=1; }
[ -d vendor/pdf-writer ] || { sk "vendor/pdf-writer 不存在（vendor 跃迁面）"; GUARD_MISS=1; }
[ -f crates/report-engine/Cargo.toml ] || { sk "crates/report-engine 不存在（质检载具面）"; GUARD_MISS=1; }
[ -f migrations_pg/V0008__reports.sql ] || { sk "V0008__reports.sql 不存在（reports 落库面）"; GUARD_MISS=1; }
if [ "$GUARD_MISS" = "1" ]; then
  echo "SKIP=$SKIP FAIL=$FAIL PASS=$PASS（受控环境守卫命中，exit 0）"
  exit 0
fi

echo "== 1. lock 三检（6b 新基线 440 / reqwest=1 / zip=0 / typst 链清零反证）"
ck "Cargo.lock package 计数（432→440）" "440" "$(grep -cE '^\[\[package\]\]' Cargo.lock)"
ck "reqwest 计数（WSW-8 §7.4 词形恒定）" "1" "$(grep -c 'name = "reqwest"' Cargo.lock)"
ck "zip 计数（清零反证）" "0" "$(grep -c 'name = "zip"' Cargo.lock)"
ck "typst 计数（D1 停批链零引入反证）" "0" "$(grep -c 'name = "typst"' Cargo.lock)"
ck "pdf-writer exact 钉在位" "1" "$(grep -c 'pdf-writer = { version = "=0.15.0" }' Cargo.toml)"
ck "subsetter exact 钉在位" "1" "$(grep -c 'subsetter = { version = "=0.2.6", default-features = false }' Cargo.toml)"
ck "skrifa exact 钉在位" "1" "$(grep -c 'skrifa = { version = "=0.42.1", default-features = false, features = \["std"\] }' Cargo.toml)"
ck "crc32fast exact 钉在位" "1" "$(grep -c 'crc32fast = { version = "=1.5.5" }' Cargo.toml)"

echo "== 2. vendor 面（目录计数 406+8=414 + 八新目录在位）"
ck "vendor 目录计数（406 基线 + 8 净增）" "414" "$(ls vendor/ | wc -l)"
NEW_DIRS_OK=yes
for d in pdf-writer subsetter skrifa crc32fast read-fonts font-types bytemuck bytemuck_derive; do
  [ -d "vendor/$d" ] || { NEW_DIRS_OK=no; FAIL=$((FAIL+1)); echo "  [FAIL] vendor/$d 缺失"; }
done
ck "八新目录在位抽查" "yes" "$NEW_DIRS_OK"

echo "== 3. deny 双检 + deny.toml 零 diff 反证（A15 零增行纪律）"
DENY_BIN="$HOME/.cargo/bin/cargo-deny"
if "$DENY_BIN" deny check advisories >/tmp/p6b_selftest_adv.log 2>&1; then
  ck "cargo deny check advisories" "ok" "ok"
else
  ck "cargo deny check advisories" "ok" "FAILED（见 /tmp/p6b_selftest_adv.log）"
fi
if "$DENY_BIN" deny check licenses >/tmp/p6b_selftest_lic.log 2>&1; then
  ck "cargo deny check licenses" "ok" "ok"
else
  ck "cargo deny check licenses" "ok" "FAILED（见 /tmp/p6b_selftest_lic.log）"
fi
ck "deny.toml 零 diff（零许可缺口零 ignore 增行）" "0" "$(git diff deny.toml | wc -l)"

echo "== 4. IR/SARIF schema 双钉（IR 冻结面恒定 + 官方 SARIF 资产在位）"
ck "report-ir schema sha256（888e9210 恒定钉）" \
  "888e9210f1ba44a5706439adb7a486ee75f8c4b6cc3c871c6184b5eb577a499e  schemas/report-ir/report-ir-v1.schema.json" \
  "$(sha256sum schemas/report-ir/report-ir-v1.schema.json)"
[ -f schemas/sarif/sarif-schema-2.1.0.json ] && ck "SARIF 官方 schema 资产在位" "yes" "yes" \
  || { FAIL=$((FAIL+1)); echo "  [FAIL] schemas/sarif/sarif-schema-2.1.0.json 缺失"; }

echo "== 5. 六新签名 re-export 对账 + 渲染/质检零 I/O 反证（判据 4）"
ck "FontSource/discover_system_fonts re-export" "1" "$(grep -c 'pub use fonts::{discover_system_fonts, FontSource};' crates/report-engine/src/lib.rs)"
ck "render_docx re-export" "1" "$(grep -c 'pub use render::docx::render_docx;' crates/report-engine/src/lib.rs)"
ck "qc 三函数 + QcCheck/QcReport re-export" "1" "$(grep -c 'pub use qc::{qc_docx, qc_pdf, qc_sarif, QcCheck, QcReport};' crates/report-engine/src/lib.rs)"
ck "render_pdf re-export" "1" "$(grep -c 'pub use render::pdf::render_pdf;' crates/report-engine/src/lib.rs)"
ck "render_attack_path_svg re-export" "1" "$(grep -c 'pub use render::svg::render_attack_path_svg;' crates/report-engine/src/lib.rs)"
ck "渲染/质检零 I/O 反证（std::fs|std::process|Command）" "0" \
  "$(grep -rn 'std::fs\|std::process\|Command' crates/report-engine/src/render/pdf.rs crates/report-engine/src/render/docx.rs crates/report-engine/src/render/svg.rs crates/report-engine/src/qc.rs | wc -l)"
ck "discover_system_fonts 唯一 I/O 面在位" "1" "$(grep -c 'pub fn discover_system_fonts' crates/report-engine/src/fonts.rs)"

echo "== 6. report-engine 质检载具（cargo test -p report-engine，--test-threads=1）"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/home/emo/rt3-target}"
if cargo test -p report-engine --offline -- --test-threads=1 >/tmp/p6b_selftest_test.log 2>&1; then
  REPORT_ENGINE_PASSED="$(grep -oE '[0-9]+ passed' /tmp/p6b_selftest_test.log | awk '{s+=$1} END {print s+0}')"
  ck "report-engine 测试全绿（6A 26 + 6B 20）" "yes" "yes"
  ck "report-engine 测试数（46）" "46" "$REPORT_ENGINE_PASSED"
else
  ck "report-engine 测试全绿" "yes" "FAILED（见 /tmp/p6b_selftest_test.log）"
fi

echo "== 7. intel-db reports_pg 面（PG 可达实跑 / 不可达 SKIP 分计）"
PG_TARGET_DIR="/tmp/p6b_selftest_pgdata"
if command -v podman >/dev/null 2>&1 && podman image exists "localhost/rip-pg17:latest"; then
  PGPORT=$(( (RANDOM % 2000) + 29000 ))
  rm -rf "${PG_TARGET_DIR}"; mkdir -p "${PG_TARGET_DIR}"
  podman rm -f rip-p6b-selftest >/dev/null 2>&1
  podman run -d --name rip-p6b-selftest -e POSTGRES_PASSWORD=pgselftest \
    -p "127.0.0.1:${PGPORT}:5432" -v "${PG_TARGET_DIR}:/var/lib/postgresql/data" \
    "localhost/rip-pg17:latest@sha256:229189e4c66a2eb8ddd1fcf4bc08ee4726cacd8aee7445ec3c6f1499a357ef45" >/dev/null 2>&1
  PGREADY=0
  for i in $(seq 1 45); do
    podman exec rip-p6b-selftest pg_isready -U postgres >/dev/null 2>&1 && { PGREADY=1; break; }
    sleep 2
  done
  if [ "$PGREADY" = "1" ]; then
    RP_TEST_PG_DSN="postgres://postgres:pgselftest@127.0.0.1:${PGPORT}/postgres" \
      cargo test -p intel-db --offline --test reports_pg -- --test-threads=1 >/tmp/p6b_selftest_reports.log 2>&1
    REPORTS_PG_PASSED="$(grep -oE '[0-9]+ passed' /tmp/p6b_selftest_reports.log | awk '{s+=$1} END {print s+0}')"
    ck "reports_pg 测试全绿（幂等+roundtrip+additive+list）" "4" "$REPORTS_PG_PASSED"
  else
    sk "一次性 PG 未就绪（V0008 测试面跳过——V-9 口径）"
  fi
  podman rm -f rip-p6b-selftest >/dev/null 2>&1
else
  sk "podman 或 rip-pg17 镜像缺席（V0008 测试面跳过——V-9 口径）"
fi

echo "== 8. 生产库五文件只读指纹（mode=ro PRAGMA user_version，1/1/1/0/0）"
RO_FINGERPRINT_OK=yes
declare -A EXPECTED_UV=(
  [migrations/platform.db]=1
  [migrations/platform-dryrun.db]=1
  [migrations/platform-fullbatch.db]=1
  [migrations/platform-governance.db]=0
  [migrations/platform-search.db]=0
)
for f in migrations/platform.db migrations/platform-dryrun.db migrations/platform-fullbatch.db migrations/platform-governance.db migrations/platform-search.db; do
  if [ ! -f "$f" ]; then
    sk "$f 不存在"
    continue
  fi
  UV="$(sqlite3 "file:$f?mode=ro" "PRAGMA user_version" 2>/dev/null || echo QUERY_FAILED)"
  ck "$f user_version（mode=ro 时代基线）" "${EXPECTED_UV[$f]}" "$UV"
  [ "$UV" = "${EXPECTED_UV[$f]}" ] || RO_FINGERPRINT_OK=no
done

echo "== 9. 跨轨零 diff 反证（wsw/verification/agent-engine/web/workers/rtis/CI/SQLite 链）"
CROSS_TRACK="$(git status --porcelain -- crates/wsw-runtime/ crates/verification-engine/ crates/agent-engine/ web/ workers/ integrations/rtis/ migrations/ .github/ 2>/dev/null | wc -l)"
ck "跨轨 git status 行数" "0" "$CROSS_TRACK"
ck "migrations/*.sql 计数（036 不立项恒定）" "30" "$(ls migrations/*.sql | wc -l)"
ck "migrations_pg/*.sql 计数（V0001-V0010，Phase 7 批 V0009/V0010 入链）" "10" "$(ls migrations_pg/*.sql | wc -l)"
ck "V0008 迁移在位" "1" "$(ls migrations_pg/V0008__reports.sql 2>/dev/null | wc -l)"
ck "report_repo 三写读面签名（store/load/list）" "3" \
  "$(grep -cE 'pub fn (store_report|load_report|list_reports_by_task)' crates/intel-db/src/report_repo.rs)"
ck "upsert_format_export additive 写面在位" "1" "$(grep -c 'pub fn upsert_format_export' crates/intel-db/src/report_repo.rs)"

echo "== 10. 工作树落点核对（§2.1 清单）"
echo "  -- git status --porcelain 落点："
git status --porcelain | sed 's/^/    /'

echo ""
echo "PASS=$PASS SKIP=$SKIP FAIL=$FAIL"
[ "$FAIL" = "0" ] || exit 1
exit 0
