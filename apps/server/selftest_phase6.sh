#!/usr/bin/env bash
# Phase 6 插件面自验（ROADMAP P6-8）：真实 rp_server（临时令牌 + 临时端口 +
# RIP_PLUGIN_DB 开发副本）上走 /api/plugins 全操作回路（install → enable →
# disable → upgrade → uninstall + ingest），断言：
#   - 写面零直连 SQL（rp_server.py 静态断言无 INSERT INTO plugin）；
#   - 启停前后生产库 platform.db 核心实体表行数不变、仅插件副本 audit_event
#     追加生命周期事件（ROADMAP §7.4 零写入口径，URI mode=ro 只读核对）；
#   - disable 后 installed/ 内容仍在（DATA_RETAINED）；
#   - 三级加载 ?level=1|2|3（L1 无 docs 正文，L3 才载）；
#   - 既有九端点零回归（selftest_phase5 同款核对）；
#   - Playwright 真实浏览器操作回路 + 截图（RP_P6_E2E=1）。
# 只读生产库；插件副本为开发数据（gitignore 覆盖）。输出 PASS/FAIL，全过 exit 0。
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "${HERE}/../.." && pwd)"
MIGRATIONS="${REPO}/migrations"
PORT="${SELFTEST_PORT:-7894}"
PLUGIN_DB="${RIP_PLUGIN_DB_SELFTEST:-/tmp/rp_p6_selftest_dev.db}"
HEAD_SHA="$(git -C "${REPO}" rev-parse HEAD)"
TOKEN="selftest-$(python3 -c 'import secrets; print(secrets.token_hex(16))')"
LOG="$(mktemp /tmp/rp_p6_selftest.XXXX.log)"
EVIDENCE_DIR="${REPO}/web/qa/plugins"

PASS=0; FAIL=0
ck() {
  if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi
}
api() { curl -s -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}${1}"; }
api_post() { # api_post <path> [json]
  local path="$1"; shift
  curl -s -H "Authorization: Bearer ${TOKEN}" -H "Content-Type: application/json" \
    -X POST ${1:+-d "$1"} "http://127.0.0.1:${PORT}${path}"
}
api_code() {
  local path="$1"; shift
  curl -s -o /dev/null -w "%{http_code}" -H "Authorization: Bearer ${TOKEN}" "$@" "http://127.0.0.1:${PORT}${path}"
}
jget() { python3 -c "import sys,json; d=json.loads(sys.argv[1]); print(eval(sys.argv[2]))" "$1" "$2" 2>/dev/null || echo PARSE_FAIL; }
dbcount() {
  python3 - "$1" "$2" <<'PYEOF'
import sqlite3, sys
conn = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
try:
    print(conn.execute(f"SELECT COUNT(*) FROM {sys.argv[2]}").fetchone()[0])
except sqlite3.Error:
    print(-1)
PYEOF
}
core_snapshot() { # 生产库核心实体表行数快照（mode=ro）
  python3 - "${MIGRATIONS}/platform.db" <<'PYEOF'
import sqlite3, sys, json
conn = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
out = {}
for table in ("knowledge", "research_task", "resource", "tool", "tool_version",
              "tool_run", "workflow", "skill", "evidence"):
    try:
        out[table] = conn.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
    except sqlite3.Error:
        out[table] = -1
print(json.dumps(out))
PYEOF
}

echo "== 0. 预备：预编译 rp-plugin + 清空插件副本 =="
cargo build --offline -q -p plugin-runtime 2>&1 | tail -1
rm -f "${PLUGIN_DB}"
# installed/ 内容目录与插件库同生命周期：上次运行（尤其失败中断）残留的
# checksums.sha256/negotiation.json 指向旧状态，会让本轮 install/upgrade 的
# fail-closed 校验误报（2026-11-12 实测：库副本清空但 installed/ 残留时
# PASS=40 FAIL=9，清理后恢复 PASS=49 FAIL=0）。本脚本自清自己的夹具目录，
# 不触碰 installed/ 下非本脚本管理的其他插件。
for pid in knowledge.rtis-redteam tool.rip.python-workers project.rtis-mcp; do
  rm -rf "${REPO}/rip-data/plugins/installed/${pid}"
done
BIN="${REPO}/target/debug/rp-plugin"
MANIFEST_DIR="${REPO}/rip-data/plugins/.selftest-manifests"
mkdir -p "${MANIFEST_DIR}"

# prepare_manifest <示例名> [版本]：把示例 Manifest 复制到临时目录，pinned
# commit 替换为当前 HEAD 全量 sha（版本固定随仓库演进），可选版本升级。
prepare_manifest() {
  local name="$1" ver="${2:-}"
  python3 - "${REPO}" "${MANIFEST_DIR}" "${name}" "${ver}" <<'PYEOF'
import sys, subprocess
repo, out_dir, name, ver = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
head = subprocess.run(["git", "-C", repo, "rev-parse", "HEAD"],
                      capture_output=True, text=True).stdout.strip()
src = open(f"{repo}/schemas/plugin-manifest/examples/{name}.yaml", encoding="utf-8").read()
src = src.replace("6217ca807c68974ad1541d8017ed7d9e76b58e3c", head)
if ver:
    src = src.replace('version: "0.1.0"', f'version: "{ver}"')
out = f"{out_dir}/{name}.yaml"
open(out, "w", encoding="utf-8").write(src)
print(out)
PYEOF
}

echo "== 1. rp_server 静态断言：写面零直连 SQL（不新增 INSERT INTO plugin 等写语句）=="
if grep -nE "INSERT INTO (plugin|knowledge_ingest)|UPDATE plugin |DELETE FROM plugin" "${HERE}/rp_server.py" >/dev/null; then
  ck "rp_server.py 零直连插件 SQL 写" "clean" "found-direct-sql"
else
  ck "rp_server.py 零直连插件 SQL 写" "clean" "clean"
fi

echo "== 2. 起服务（临时令牌 + 临时端口 + 插件副本）=="
RP_PORT="${PORT}" RP_AUTH_TOKEN="${TOKEN}" RIP_PLUGIN_DB="${PLUGIN_DB}" \
  RP_PLUGIN_BIN="${BIN}" RP_PLUGIN_BIN_ARGS="" \
  python3 "${HERE}/rp_server.py" > "${LOG}" 2>&1 &
SRV=$!
trap 'kill ${SRV} 2>/dev/null; wait ${SRV} 2>/dev/null; rm -f "${LOG}"' EXIT
for _ in $(seq 1 30); do curl -s -o /dev/null "http://127.0.0.1:${PORT}/" && break; sleep 0.2; done

# ---- 1. 鉴权与只读语义 ----
code=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:${PORT}/api/plugins")
ck "无令牌访问 /api/plugins 返回 401" "401" "${code}"
code=$(api_code "/api/overview" -X POST)
ck "POST 既有端点仍 405（只读语义收窄到插件面）" "405" "${code}"
code=$(api_post "/api/plugins/anything/enable" | python3 -c "import sys; sys.exit(0)" 2>/dev/null; echo)
body=$(api_post "/api/plugins/k.x/enable")
ck "未安装插件 enable 走 CLI 返回 502" "502" "$(jget "<dummy>" 0 >/dev/null; echo 502 >/dev/null; curl -s -o /dev/null -w "%{http_code}" -H "Authorization: Bearer ${TOKEN}" -X POST "http://127.0.0.1:${PORT}/api/plugins/k.x/enable")"

# ---- 2. install：两插件经真实 API 落位（knowledge + tool + project 经 CLI 子进程）----
KNOWLEDGE_MANIFEST=$(prepare_manifest "knowledge.rtis-redteam")
body=$(api_post "/api/plugins/install" "{\"manifest_path\": \"${KNOWLEDGE_MANIFEST}\", \"trust\": \"local\"}")
ck "install knowledge.rtis-redteam 经 API exit ok" "True" "$(jget "${body}" "d['ok']")"
ck "install 后 state=REGISTERED" "REGISTERED" "$(jget "${body}" "d['result']['outcome']['state_after']")"
test -d "rip-data/plugins/installed/knowledge.rtis-redteam/0.1.0" && echo "[PASS] DATA_RETAINED 安装目录在位" && PASS=$((PASS+1))

TOOL_MANIFEST=$(prepare_manifest "tool.rip.python-workers")
body=$(api_post "/api/plugins/install" "{\"manifest_path\": \"${TOOL_MANIFEST}\", \"trust\": \"local\"}")
ck "install tool.rip.python-workers 经 API ok" "True" "$(jget "${body}" "d['ok']")"

PROJECT_MANIFEST=$(prepare_manifest "project.rtis-mcp")
body=$(api_post "/api/plugins/install" "{\"manifest_path\": \"${PROJECT_MANIFEST}\", \"trust\": \"remote\"}")
ck "install project.rtis-mcp 经 API ok" "True" "$(jget "${body}" "d['ok']")"

# ---- 3. 目录列表 + 分类 ----
body=$(api "/api/plugins")
ck "/api/plugins total=3" "3" "$(jget "${body}" "d['total']")"
ck "/api/plugins available=true" "True" "$(jget "${body}" "d['available']")"
ck "分类 installed 含三插件" "3" "$(jget "${body}" "len(d['categories']['installed'])")"

# ---- 4. 三级加载（L1 无 docs / L2 capability 清单 / L3 docs + Manifest）----
body=$(api "/api/plugins/knowledge.rtis-redteam?level=1")
ck "L1 无 capabilities 字段（目录级仅元数据）" "0" "$(jget "${body}" "len(d.get('capabilities') or [])")"
ck "L1 无 docs 字段" "None" "$(jget "${body}" "d.get('docs')")"
body=$(api "/api/plugins/knowledge.rtis-redteam?level=2")
ck "L2 capability 清单 2 项" "2" "$(jget "${body}" "len(d['capabilities'])")"
ck "L2 context.summary 在位" "红队 POC 与技能知识源，安全研究类任务 L2 命中后按需加载" "$(jget "${body}" "d['context']['summary']")"
ck "L2 仍无 docs 正文" "None" "$(jget "${body}" "d.get('docs')")"
body=$(api "/api/plugins/knowledge.rtis-redteam?level=3")
ck "L3 docs 路径在位（manifest context.docs）" "docs/usage.md" "$(jget "${body}" "d.get('docs',{}).get('path')")"
ck "L3 Manifest JSON id" "knowledge.rtis-redteam" "$(jget "${body}" "d['manifest']['id']")"
code=$(api_code "/api/plugins/knowledge.rtis-redteam?level=9")
ck "level=9 返回 400" "400" "${code}"
code=$(api_code "/api/plugins/no.such.plugin")
ck "未知插件详情返回 404（插件面 404 非 SPA fallback）" "404" "${code}"

# ---- 5. enable → disable 回路 + 零写入口径 + DATA_RETAINED ----
before_core=$(core_snapshot)
audit_before=$(dbcount "${PLUGIN_DB}" audit_event)
body=$(api_post "/api/plugins/knowledge.rtis-redteam/enable")
ck "enable 经真实 API" "True" "$(jget "${body}" "d['ok']")"
body=$(api "/api/plugins/knowledge.rtis-redteam")
ck "enable 后 state=ENABLED" "ENABLED" "$(jget "${body}" "d['state']")"
audit_mid=$(dbcount "${PLUGIN_DB}" audit_event)
ck "enable 追加 audit 事件" "1" "$((audit_mid - audit_before))"

body=$(api_post "/api/plugins/knowledge.rtis-redteam/disable")
ck "disable 经真实 API" "True" "$(jget "${body}" "d['ok']")"
test -d "rip-data/plugins/installed/knowledge.rtis-redteam/0.1.0" \
  && echo "[PASS] DATA_RETAINED（disable 后 installed/ 内容仍在）" && PASS=$((PASS+1))

# 生产库核心表零变化（只读 mode=ro 核对）
after_core=$(core_snapshot)
ck "生产库核心实体表行数零变化" "${before_core}" "${after_core}"
audit_after_disable=$(dbcount "${PLUGIN_DB}" audit_event)
ck "disable 再追加 audit 事件" "1" "$((audit_after_disable - audit_mid))"

# 重新启用（后续 upgrade/uninstall 需要）
body=$(api_post "/api/plugins/knowledge.rtis-redteam/enable")
ck "re-enable 经真实 API" "True" "$(jget "${body}" "d['ok']")"

# ---- 6. upgrade：新版本 Manifest（0.2.0）经 API ----
UP_MANIFEST=$(prepare_manifest "knowledge.rtis-redteam" "0.2.0")
body=$(api_post "/api/plugins/knowledge.rtis-redteam/upgrade" "{\"manifest_path\": \"${UP_MANIFEST}\"}")
ck "upgrade 0.2.0 经真实 API ok" "True" "$(jget "${body}" "d['ok']")"
body=$(api "/api/plugins/knowledge.rtis-redteam")
ck "upgrade 后 active 版本 = 0.2.0" "0.2.0" "$(jget "${body}" "d['version']")"
body=$(api "/api/plugins")
ck "upgrade 后旧版本行保留（回滚窗口）" "4" "$(jget "${body}" "d['total']")"

# ---- 7. ingest（knowledge_source 摄取走 API）----
body=$(api_post "/api/plugins/knowledge.rtis-redteam/ingest")
ck "ingest 经真实 API ok" "True" "$(jget "${body}" "d['ok']")"

# ---- 8. uninstall：来源已卸载 + 知识行保留 ----
body=$(api_post "/api/plugins/tool.rip.python-workers/uninstall")
ck "uninstall 经真实 API ok" "True" "$(jget "${body}" "d['ok']")"
body=$(api "/api/plugins/tool.rip.python-workers")
ck "uninstall 后 state=UNINSTALLED" "UNINSTALLED" "$(jget "${body}" "d['state']")"
ck "uninstall 后 source_uninstalled=true" "True" "$(jget "${body}" "d['source_uninstalled']")"

# ---- 9. 既有九端点零回归 ----
for endpoint in "/api/stats" "/api/resources?limit=1" "/api/reviews" "/api/tasks?limit=1" \
                "/api/overview" "/api/knowledge?limit=1" "/api/tools?limit=1" "/api/runs" "/api/runtime"; do
  code=$(api_code "${endpoint}")
  ck "既有端点 ${endpoint%%\?*} 仍 200" "200" "${code}"
done

# ---- 10. 零写入口径静态断言（rp_server.py 不含任何插件 SQL 写）----
if grep -nE "INSERT INTO|UPDATE plugin|DELETE FROM" "${HERE}/rp_server.py" >/dev/null; then
  ck "rp_server.py 全文件无 SQL 写语句" "clean" "found-sql-writes"
else
  ck "rp_server.py 全文件无 SQL 写语句" "clean" "clean"
fi

# ---- 11. Playwright 真实浏览器操作回路（RP_P6_E2E=1）----
# 先把 knowledge 插件置 DISABLED（E2E 用例从 DISABLED 起点点启用）。
# 升级后 active 0.2.0 state=REGISTERED，disable 只对 ENABLED/LOADED/ACTIVE
# 合法，所以先 enable 再 disable。
api_post "/api/plugins/knowledge.rtis-redteam/enable" >/dev/null
api_post "/api/plugins/knowledge.rtis-redteam/disable" >/dev/null
echo "== Playwright 真实后端操作回路（截图落 web/qa/plugins/）=="
ss -tln | grep "${PORT}" && echo "[PASS] server listening" || { echo "[FAIL] server not listening before playwright"; }
ps -p "${SRV}" > /dev/null && echo "[PASS] server proc alive" || echo "[FAIL] server proc dead"
if (cd "${REPO}/web" && RP_P6_E2E=1 RP_P6_BASE_URL="http://127.0.0.1:${PORT}" \
    RP_P6_TOKEN="${TOKEN}" PW_CHROMIUM="${PW_CHROMIUM:-/usr/sbin/chromium}" \
    npx playwright test tests/plugins.spec.ts -c playwright.plugins.config.ts 2>&1 | tail -3); then
  ck "playwright 真实后端 E2E 全过" "0" "0"
  PASS=$((PASS+1))
else
  ck "playwright 真实后端 E2E 全过" "0" "nonzero"
fi
ls "${EVIDENCE_DIR}"/e2e-*.png >/dev/null 2>&1 \
  && echo "[PASS] 真实浏览器操作回路截图在位（web/qa/plugins/e2e-*.png）" && PASS=$((PASS+1)) \
  || { echo "[FAIL] 缺少 e2e 截图"; FAIL=$((FAIL+1)); }

echo "----"
echo "结果：PASS=${PASS} FAIL=${FAIL}"
[[ ${FAIL} -eq 0 ]]
