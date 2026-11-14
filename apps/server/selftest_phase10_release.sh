#!/usr/bin/env bash
# P10-4 自验脚本（发布与合规收口：ROADMAP P10-6 四文档 + SESSION-HANDOFF §十
# 许可证清单 / axe 对比度 / 视觉基线三项的机器可核对面）：
#   1. 关键 bins 离线构建（vendor 口径，零网络）；
#   2. 许可证合规三面：cargo-deny licenses（工具在位时）+ vendor 实装值
#      fail-closed 复核（强著作权拒绝）+ node_modules 实装值 fail-closed 复核；
#   3. 发布文档在位：四 guides + 两 DEPENDENCIES + deny.toml + 视觉基线 PNG
#      计数与阻断断言形状（color-contrast 豁免分支已移除）；
#   4. 冒烟三连（resolve/search/一次工具 run）走 mktemp 隔离副本（fresh clone
#      兼容口径，不依赖维护机生产库）；
#   5. 新文档 em/en dash 零命中；
#   6. 宿主机 command -v 快照 diff（脚本自身零安装核对）；
#   7. 生产 migrations/platform*.db 指纹前后一致（零写红线）。
# 全过 exit 0；任何 FAIL exit 1。前置：仓库根运行。
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO}"

PASS=0; FAIL=0
ck() { # ck <名> <期望> <实际>
  if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi
}
sha_of() { python3 -c 'import hashlib,sys;print(hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest())' "$1"; }
fingerprint() { for f in migrations/platform*.db; do [[ -e "$f" ]] && echo "$(sha_of "$f")  $(basename "$f")"; done | sort; }
host_snapshot() { for c in podman virsh virt-install qemu-img docker quickemu node python3 cargo npx chromium chromium-browser google-chrome-stable; do printf "%s=" "$c"; command -v "$c" || echo MISSING; done; }

WORK="$(mktemp -d /tmp/rp_p10_release.XXXX)"
trap 'rm -rf "${WORK}"' EXIT

echo "== 0. 生产库指纹（前）+ 宿主机快照（前）"
fingerprint > "${WORK}/fp-before.txt"
host_snapshot > "${WORK}/host-before.txt"

echo
echo "== 1. 关键 bins 离线构建（vendor 口径）"
cargo build --offline -q -p database --bin rp-admin -p search-engine --bin rp-search-index \
  -p resource-engine --bin rp-resolve -p tool-registry --bin rp-tool -p mcp-server \
  -p quality-engine --bin rp-review -p research-engine --bin rp-research 2>&1 | tail -2
ck "关键 bins 离线构建" "0" "$?"

echo
echo "== 2. 许可证合规（cargo-deny + 实装值 fail-closed 复核）"
if command -v cargo-deny >/dev/null 2>&1 || [[ -x "${HOME}/.cargo/bin/cargo-deny" ]]; then
  DENY_BIN="$(command -v cargo-deny || echo "${HOME}/.cargo/bin/cargo-deny")"
  "${DENY_BIN}" check licenses > "${WORK}/deny.out" 2>&1
  ck "cargo deny check licenses" "0" "$?"
  grep -q "licenses ok" "${WORK}/deny.out" && { PASS=$((PASS+1)); echo "[PASS] cargo-deny 判定 licenses ok"; } \
    || { FAIL=$((FAIL+1)); echo "[FAIL] cargo-deny 判定异常"; tail -3 "${WORK}/deny.out"; }
else
  echo "[INFO] cargo-deny 不在位（安装口径见 docs/phase10/DEPENDENCIES-RUST.md 第一节），走实装值复核"
fi
python3 -I - "${WORK}" <<'PYEOF'
import re, sys, glob, os, json
work = sys.argv[1]
fails = []
# Rust 面：Cargo.lock 全量包 → vendor/<crate>/Cargo.toml license 字段，强著作权拒绝
allow = ("MIT", "Apache-2.0", "BSD", "ISC", "Unicode", "Zlib", "CC0", "CDLA", "BlueOak", "0BSD", "WTFPL", "MIT-0", "Unlicense", "BSL", "OpenSSL", "MPL", "LGPL")
lock = open("Cargo.lock").read()
n3 = 0
for block in lock.split("[[package]]")[1:]:
    name = re.search(r'^name = "([^"]+)"', block, re.M)
    ver = re.search(r'^version = "([^"]+)"', block, re.M)
    src = re.search(r'^source = "([^"]*)"', block, re.M)
    if not (name and ver):
        continue
    if src is None or src.group(1) == "":
        continue  # workspace 自有 crate（Cargo.lock 无 source 行；publish=false，private.ignore 口径）
    n3 += 1
    lic = None
    for d in (f"vendor/{name.group(1)}", f"vendor/{name.group(1)}-{ver.group(1)}"):
        p = os.path.join(d, "Cargo.toml")
        if os.path.exists(p):
            m = re.search(r'^license\s*=\s*"([^"]+)"', open(p, encoding="utf-8", errors="replace").read(), re.M)
            if m:
                lic = m.group(1)
            break
    if lic is None:
        fails.append(f"no-license:{name.group(1)}")
    elif lic.startswith("GPL") or lic.startswith("AGPL"):
        fails.append(f"strong-copyleft:{name.group(1)}={lic}")
# npm 面：node_modules 全量 package.json，许可型白名单外拒绝
perm = ("MIT", "Apache-2.0", "BSD", "ISC", "0BSD", "BlueOak", "MPL-2.0", "CC0", "Python-2.0", "Unlicense", "CC-BY")
root = "web/node_modules"
nweb = 0
for pj in glob.glob(os.path.join(root, "**", "package.json"), recursive=True):
    try:
        d = json.load(open(pj, encoding="utf-8"))
    except Exception:
        continue
    lic = d.get("license")
    if not d.get("name"):
        continue
    nweb += 1
    if lic is None or not any(p in str(lic) for p in perm):
        fails.append(f"npm:{d.get('name')}={lic}")
summary = f"rust3={n3} npm={nweb} fails={fails[:5]}"
open(os.path.join(work, "lic.json"), "w").write(json.dumps({"ok": not fails, "summary": summary}))
print(summary)
PYEOF
ck "许可证实装值复核（Rust 强著作权拒绝 + npm 许可型白名单）" "0" "$?"
python3 -c "import json,sys;d=json.load(open('${WORK}/lic.json'));sys.exit(0 if d['ok'] else 1)" && { PASS=$((PASS+1)); echo "[PASS] vendor/node_modules 实装值零违例"; } || { FAIL=$((FAIL+1)); echo "[FAIL] 实装值复核违例"; }

echo
echo "== 3. 发布文档在位与阻断断言形状"
for f in docs/guides/INSTALL.md docs/guides/BACKUP-RESTORE.md docs/guides/MIGRATION.md docs/guides/UPGRADE.md \
         docs/phase10/DEPENDENCIES-RUST.md docs/phase10/DEPENDENCIES-WEB.md deny.toml \
         apps/server/selftest_phase10_release.sh docs/phase10/P10-4-TASK.md docs/phase10/P10-4-BATCH-REPORT.md; do
  ck "在位: $f" "yes" "$([[ -s "$f" ]] && echo yes || echo no)"
done
NB=$(ls web/qa/visual/baseline/pages.spec.ts/*.png 2>/dev/null | wc -l)
ck "视觉基线 pages PNG = 44" "44" "$NB"
ck "视觉基线 workflows PNG = 3" "3" "$(ls web/qa/visual/baseline/workflows.spec.ts/*.png 2>/dev/null | wc -l)"
# 阻断化形状：color-contrast 豁免分支（continue）已移除，serious 断言含全量
grep -q 'violation.id === "color-contrast"' web/qa/visual/pages.spec.ts web/qa/visual/workflows.spec.ts \
  && { FAIL=$((FAIL+1)); echo "[FAIL] color-contrast 豁免分支仍在（应已阻断化移除）"; } \
  || { PASS=$((PASS+1)); echo "[PASS] color-contrast 豁免分支已移除（与其他违规同口径阻断）"; }
grep -q "toHaveScreenshot" web/qa/visual/pages.spec.ts web/qa/visual/workflows.spec.ts \
  && { PASS=$((PASS+1)); echo "[PASS] toHaveScreenshot 基线断言在位（pages + workflows）"; } \
  || { FAIL=$((FAIL+1)); echo "[FAIL] 视觉基线断言缺失"; }
grep -q 'snapshotPathTemplate' web/playwright.config.ts \
  && { PASS=$((PASS+1)); echo "[PASS] 基线目录模板指向 qa/visual/baseline/"; } \
  || { FAIL=$((FAIL+1)); echo "[FAIL] snapshotPathTemplate 缺失"; }

echo
echo "== 4. 冒烟三连（mktemp 隔离副本，fresh clone 口径）"
cp migrations/platform-dryrun.db "${WORK}/smoke.db"
target/debug/rp-resolve --db "${WORK}/smoke.db" --keywords fuzzing --limit 5 > "${WORK}/resolve.json" 2>/dev/null
ck "冒烟 resolve（dryrun 副本）" "0" "$?"
python3 -c "import json;d=json.load(open('${WORK}/resolve.json'));assert d['search_status']=='ok'" \
  && { PASS=$((PASS+1)); echo "[PASS] resolve search_status=ok"; } || { FAIL=$((FAIL+1)); echo "[FAIL] resolve 输出异常"; }
target/debug/rp-search-index build --platform-db "${WORK}/smoke.db" --catalog-db migrations/sources-snapshot/catalog.db --index-db "${WORK}/idx.db" > "${WORK}/idx.json" 2>/dev/null
ck "冒烟 search index build" "0" "$?"
target/debug/rp-search-index query --index-db "${WORK}/idx.db" --keywords fuzzing --limit 10 > "${WORK}/query.json" 2>/dev/null
ck "冒烟 search query" "0" "$?"
mkdir -p "${WORK}/rip-data"
target/debug/rp-tool register --manifest manifests/tools/rip.kb.keyword_search.json --db "${WORK}/rip-data/tool-runs.db" > /dev/null 2>&1
ck "冒烟 tool register" "0" "$?"
target/debug/rp-tool run --db "${WORK}/rip-data/tool-runs.db" --tool rip.kb.keyword_search --input '{"keyword":"smoke"}' > "${WORK}/run.json" 2>&1
RUN_STATUS="$(python3 -c "import json;print(json.load(open('${WORK}/run.json')).get('status'))" 2>/dev/null || echo PARSE_FAIL)"
ck "冒烟 tool run SUCCEEDED" "SUCCEEDED" "$RUN_STATUS"

echo
echo "== 5. 新文档 em/en dash 零命中"
DASH_HITS="$(grep -rlP '\x{2014}|\x{2013}' docs/guides/INSTALL.md docs/guides/BACKUP-RESTORE.md docs/guides/MIGRATION.md docs/guides/UPGRADE.md docs/phase10/DEPENDENCIES-RUST.md docs/phase10/DEPENDENCIES-WEB.md docs/phase10/P10-4-TASK.md docs/phase10/P10-4-BATCH-REPORT.md 2>/dev/null | wc -l)"
ck "em/en dash 零命中" "0" "$DASH_HITS"

echo
echo "== 6. 宿主机快照（后）+ 生产库指纹（后）"
host_snapshot > "${WORK}/host-after.txt"
diff "${WORK}/host-before.txt" "${WORK}/host-after.txt" > /dev/null \
  && { PASS=$((PASS+1)); echo "[PASS] 宿主机 command -v 快照前后一致（脚本零安装）"; } \
  || { FAIL=$((FAIL+1)); echo "[FAIL] 宿主机快照出现新增条目"; diff "${WORK}/host-before.txt" "${WORK}/host-after.txt"; }
fingerprint > "${WORK}/fp-after.txt"
diff "${WORK}/fp-before.txt" "${WORK}/fp-after.txt" > /dev/null \
  && { PASS=$((PASS+1)); echo "[PASS] 生产 migrations/platform*.db 指纹前后一致（零写红线）"; } \
  || { FAIL=$((FAIL+1)); echo "[FAIL] 生产库指纹发生变化"; diff "${WORK}/fp-before.txt" "${WORK}/fp-after.txt"; }

echo
echo "==================== P10-4 selftest 汇总：PASS=${PASS} FAIL=${FAIL} ===================="
[[ "${FAIL}" -eq 0 ]] || exit 1
exit 0
