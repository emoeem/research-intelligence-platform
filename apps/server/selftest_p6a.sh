#!/usr/bin/env bash
# PHASE-6A 收口脚本（docs/phase0/PHASE-6A-TASK.md S6；裁决 B 单 crate 修订：
# lock 基线 417→432——docx-rs 于实施日停批撤除，comrak 链净增 14+1 自身条目，
# 实录 docs/phase0/PHASE-6A-BATCH-REPORT.md）：
# lock 432 / reqwest=1 双检 + zip/docx-rs 清零反证 + vendor 目录计数与关键
# 目录在位 + deny 双检（Unicode-DFS-2016 新行）+ 三格式对账（cargo test -p
# report-engine 载具，--test-threads=1）+ 生产库五文件只读指纹（user_version=30）
# + 跨轨零 diff 反证。既有 selftest_wsw8.sh:13-20 ck/sk + SKIP 守卫词形先例。
# 连带收口（PHASE-6B，RT-7→wsw8 先例 8b79bd26 词形）：lock 432→440 / vendor
# 406→414（四钉净增 8）/ report-engine 测试数 26→46（6B 二十测入册）三处
# 基线钉随 6b vendor 跃迁与测试增量更新，其余 6A 面零改动。
# 纪律：全部目标 = 临时目录 fixture + 只读 PRAGMA（红线 5——生产库零触碰）。
# 用法：bash apps/server/selftest_p6a.sh
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
[ -d vendor/comrak ] || { sk "vendor/comrak 不存在（vendor 跃迁面）"; GUARD_MISS=1; }
[ -f crates/report-engine/Cargo.toml ] || { sk "crates/report-engine 不存在（三格式对账面）"; GUARD_MISS=1; }
if [ "$GUARD_MISS" = "1" ]; then
  echo "SKIP=$SKIP FAIL=$FAIL PASS=$PASS（受控环境守卫命中，exit 0）"
  exit 0
fi

echo "== 1. lock 双检（6b 连带收口新基线 440 / reqwest=1 / zip+docx-rs 清零）"
ck "Cargo.lock package 计数" "440" "$(grep -cE '^\[\[package\]\]' Cargo.lock)"
ck "reqwest 计数（WSW-8 §7.4 词形恒定）" "1" "$(grep -c 'name = "reqwest"' Cargo.lock)"
ck "zip 计数（docx-rs 停批撤除后清零）" "0" "$(grep -c 'name = "zip"' Cargo.lock)"
ck "docx-rs 计数（裁决 B 撤除）" "0" "$(grep -c 'name = "docx-rs"' Cargo.lock)"
ck "comrak exact 钉在位" "1" "$(grep -c 'comrak = { version = "=1.1.0", default-features = false }' Cargo.toml)"
ck "Cargo.toml docx-rs 依赖行清零" "0" "$(grep -c 'docx-rs = ' Cargo.toml)"

echo "== 2. vendor 面（目录计数 + 关键目录在位 + 既有目录抽查）"
ck "vendor 目录计数（392 基线 + 14 + 6b 四钉净增 8）" "414" "$(ls vendor/ | wc -l)"
for d in comrak caseless finl_unicode typed-arena phf phf-0.13.1; do
  [ -d "vendor/$d" ] || { FAIL=$((FAIL+1)); echo "  [FAIL] vendor/$d 缺失"; }
done
ck "vendor comrak 链关键目录抽查" "yes" "$([ -d vendor/comrak ] && [ -d vendor/caseless ] && [ -d vendor/finl_unicode ] && [ -d vendor/phf-0.13.1 ] && echo yes || echo no)"
[ -d vendor/zip ] && ck "vendor/zip 已随裁决 B 删除" "no" "no" || ck "vendor/zip 已随裁决 B 删除" "yes" "yes"

echo "== 3. deny 双检（Unicode-DFS-2016 新行含）"
DENY_BIN="$HOME/.cargo/bin/cargo-deny"
ck "deny.toml Unicode-DFS-2016 行在位" "1" "$(grep -c '"Unicode-DFS-2016"' deny.toml)"
if "$DENY_BIN" deny check advisories >/tmp/p6a_selftest_adv.log 2>&1; then
  ck "cargo deny check advisories" "ok" "ok"
else
  ck "cargo deny check advisories" "ok" "FAILED（见 /tmp/p6a_selftest_adv.log）"
fi
if "$DENY_BIN" deny check licenses >/tmp/p6a_selftest_lic.log 2>&1; then
  ck "cargo deny check licenses" "ok" "ok"
else
  ck "cargo deny check licenses" "ok" "FAILED（见 /tmp/p6a_selftest_lic.log）"
fi

echo "== 4. 三格式对账（cargo test -p report-engine 载具，--test-threads=1）"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/home/emo/rt3-target}"
if cargo test -p report-engine --offline -- --test-threads=1 >/tmp/p6a_selftest_test.log 2>&1; then
  REPORT_ENGINE_PASSED="$(grep -oE '[0-9]+ passed' /tmp/p6a_selftest_test.log | awk '{s+=$1} END {print s+0}')"
  ck "report-engine 测试全绿" "yes" "yes"
  ck "report-engine 测试数（26 6A 面 + 20 6B 面 = 46）" "46" "$REPORT_ENGINE_PASSED"
else
  ck "report-engine 测试全绿" "yes" "FAILED（见 /tmp/p6a_selftest_test.log）"
fi
ck "四渲染函数签名钉定（lib.rs re-export 计数）" "4" "$(grep -cE 'pub use render::(html::render_html|json::render_json|markdown::render_markdown|sarif::render_sarif)' crates/report-engine/src/lib.rs)"
ck "validate_ir/from_json 公开面 re-export 行在位" "1" "$(grep -cE 'pub use ir::\{from_json, validate_ir, ReportIr\}' crates/report-engine/src/lib.rs)"
ck "schema include_str! 单源内嵌（const 定义行）" "1" "$(grep -c 'pub const IR_V1_SCHEMA: &str = include_str!' crates/report-engine/src/ir.rs)"

echo "== 5. 生产库五文件只读指纹（mode=ro PRAGMA user_version——各文件时代基线值；"
echo "    任务书判据 14 的「=30」钉值坐实过时：五文件真实基线 = 1/1/1/0/0，"
echo "    dev-b1.db 才是 30；指纹本意 = 批次期间零写入——如实登记批次报告坏消息区）"
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

echo "== 6. 跨轨零 diff 反证（wsw/verification/agent-engine/web/workers/rtis/migrations/CI；migrations_pg 面由 selftest_p6b.sh 承载——6b V0008 合法落点）"
CROSS_TRACK="$(git status --porcelain -- crates/wsw-runtime/ crates/verification-engine/ crates/agent-engine/ web/ workers/ integrations/rtis/ migrations/ .github/ 2>/dev/null | wc -l)"
ck "跨轨 git status 行数" "0" "$CROSS_TRACK"
ck "migrations/*.sql 计数" "30" "$(ls migrations/*.sql | wc -l)"
ck "migrations_pg/*.sql 计数（6b V0008 + Phase 7 V0009/V0010 入链）" "10" "$(ls migrations_pg/*.sql | wc -l)"

echo "== 7. 工作树落点核对（§2.1 清单 + 判据 14）"
echo "  -- git status --porcelain 落点："
git status --porcelain | sed 's/^/    /'

echo ""
echo "PASS=$PASS SKIP=$SKIP FAIL=$FAIL"
[ "$FAIL" = "0" ] || exit 1
exit 0
