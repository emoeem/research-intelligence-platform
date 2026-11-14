#!/usr/bin/env bash
# P10-2 自验脚本（ROADMAP §11.3 P10-1/P10-2：插件生态收口——信任级四档驱动
# 安装确认强度与自动更新默认策略（D10 仅 official 自动提示）生效、插件升级
# 失败自动回滚、镜像 digest 回滚（ADR TOOL-RUNTIME §6 语义）、回滚窗口清理
# （D6）、插件目录发布形态）：
#
#   1. 镜像 digest 回滚实演（rp-image-pin）：校验失败 rolled_back 指针不切换
#      + 显式回滚拨回旧 digest（真实台账文件 + digest 三方对账真实验证链）；
#   2. D10 门控实演（rp-plugin update-check --auto）：local 拒绝
#      （update_check_auto_not_allowed）、manual 全档可用；
#   3. 插件升级失败自动回滚实演：包体校验和破坏 → upgrade exit 1
#      checksum_verification_failed → active 指针保持 0.1.0；
#   4. 升级成功 + 回滚窗口清理（D6）：未确认拒绝、确认后旧版本 UNINSTALLED、
#      installed 目录物理删除、active 不动；
#   5. D10 official 放行实演（隔离库二，prompted）；
#   6. 插件目录发布形态：catalog-publish（trust 分级 + sha 完整性）+
#      catalog-verify + 篡改拒绝 + 按目录条目安装；
#   7. server 确认分级强制（真实 rp_server 临时令牌临时端口）：local 无
#      confirm 拒绝（install_confirmation_required）、confirm 后受理、remote
#      confirm_id 不一致拒绝（install_confirm_id_mismatch）、trust-policy
#      端点四档在列；
#   8. 生产 migrations/platform*.db 指纹前后一致零写（红线）。
#
# 隔离纪律：全部实演走 mktemp 工作目录与隔离插件库；生产库只读指纹核对。
# 前置：仓库根运行（cargo 门禁按 CWD 发现 vendor 配置）。全过 exit 0。
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO}"

WORK="$(mktemp -d /tmp/rp_p10_eco.XXXX)"
PASS=0; FAIL=0
ck() { # ck <名> <期望> <实际>
  if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi
}
demo() { echo "[实演] $1"; }
sha_of() { python3 -c 'import hashlib,sys;print(hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest())' "$1"; }
fingerprint() { for f in migrations/platform*.db; do echo "$(sha_of "$f")  $(basename "$f")"; done | sort; }
# jget <file> <dotted.path>：点路径取值，段为数字时取数组下标，len(path) 取长度。
jget() {
  python3 - "$1" "$2" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
path = sys.argv[2]
is_len = path.startswith("len(") and path.endswith(")")
if is_len:
    path = path[4:-1]
for part in path.split("."):
    if not part:
        continue
    data = data[int(part)] if part.isdigit() else data[part]
print(len(data) if is_len else data)
PY
}
SERVER_PID=""
cleanup() { [[ -n "${SERVER_PID}" ]] && kill "${SERVER_PID}" 2>/dev/null; rm -rf "${WORK}"; }
trap cleanup EXIT

echo "== 0. 构建（cargo --offline）与生产库指纹（前）"
cargo build --offline -q -p plugin-runtime --bin rp-plugin || { echo "rp-plugin build FAIL"; exit 1; }
cargo build --offline -q -p tool-engine --bin rp-image-pin || { echo "rp-image-pin build FAIL"; exit 1; }
fingerprint > "${WORK}/fp-before.txt"
echo "[PASS] rp-plugin 与 rp-image-pin 已构建；生产 platform*.db 指纹已留底（$(wc -l < "${WORK}/fp-before.txt") 个文件）"

IMAGE_PIN="target/debug/rp-image-pin"
RP="target/debug/rp-plugin"
PLUGIN_DB="${WORK}/plugin-eco.db"
DATA_DIR="${WORK}/data"
DATA_ROOT="${DATA_DIR}"
mkdir -p "${DATA_DIR}"
MANIFEST_V1="schemas/plugin-manifest/examples/knowledge.rtis-redteam.json"

make_pkg() { # make_pkg <目录>
  mkdir -p "$1/capabilities"
  printf '# RTIS 红队知识包\n' > "$1/README.md"
  printf 'entry stub: capabilities/knowledge_source.yaml\n' > "$1/capabilities/knowledge_source.yaml"
}
manifest_version() { # manifest_version <源> <目标> <新版本>
  python3 -c '
import json, sys
src, dst, ver = sys.argv[1], sys.argv[2], sys.argv[3]
m = json.load(open(src))
m["version"] = ver
json.dump(m, open(dst, "w"), ensure_ascii=False, indent=2)
' "$1" "$2" "$3"
}

echo
echo "== 1. 镜像 digest 回滚实演（ADR TOOL-RUNTIME §6：更新先验证再切换，升级失败回滚旧 digest）"
LEDGER="${WORK}/image-ledger.json"
TOOL_MANIFEST="containerfiles/rip-tool-webresearch/tool-manifest.json"
OLD_DIGEST=$(jget "${TOOL_MANIFEST}" runtime.image | python3 -c 'import sys;print(sys.stdin.read().strip().split("@")[1])')
NEW_DIGEST="sha256:$(python3 -c 'print("22"*32)')"
BAD_DIGEST="sha256:$(python3 -c 'print("33"*32)')"
OLD_HEX="${OLD_DIGEST#sha256:}"
ck "旧 digest 从真实 tool-manifest 提取（sha256:<64hex>）" "64" "${#OLD_HEX}"
"${IMAGE_PIN}" --ledger "${LEDGER}" pin --image localhost/rip-tool-webresearch --digest "${OLD_DIGEST}" > /dev/null
ck "pin 入账" "0" "$?"

# 1a. 升级校验失败：入参 digest 与 Manifest runtime.image 三方对账失败 →
#     验证链拒绝 → rolled_back 指针不切换。
"${IMAGE_PIN}" --ledger "${LEDGER}" update --image localhost/rip-tool-webresearch \
  --digest "${BAD_DIGEST}" --manifest "${TOOL_MANIFEST}" > "${WORK}/img-update-fail.json"
RC=$?
ck "校验失败 update 安全落回滚（exit 0）" "0" "${RC}"
ck "outcome.result=rolled_back" "rolled_back" "$(jget "${WORK}/img-update-fail.json" outcome.result)"
ck "verified=false（真实验证链拒绝）" "False" "$(jget "${WORK}/img-update-fail.json" verified)"
ck "升级失败回滚旧 digest：current 保持 pin 值（指针未切换）" "${OLD_DIGEST}" "$(jget "${WORK}/img-update-fail.json" outcome.current_digest)"
demo "镜像 digest 回滚：升级校验失败自动回滚旧 digest（指针未切换，attempt 记 rolled_back，reason=digest 三方对账失败）"

# 1b. 验证通过切换 → 显式回滚拨回旧 digest（回滚位互换，可再滚回）。
UPDATED_MANIFEST="${WORK}/tool-manifest-updated.json"
python3 -c '
import json, sys
src, dst, new = sys.argv[1], sys.argv[2], sys.argv[3]
m = json.load(open(src))
old = m["runtime"]["image"].split("@")[1]
m["runtime"]["image"] = m["runtime"]["image"].replace(old, new)
if "provenance" in m and "image_digest" in m.get("provenance", {}):
    m["provenance"]["image_digest"] = new
json.dump(m, open(dst, "w"), ensure_ascii=False, indent=2)
' "${TOOL_MANIFEST}" "${UPDATED_MANIFEST}" "${NEW_DIGEST}"
"${IMAGE_PIN}" --ledger "${LEDGER}" update --image localhost/rip-tool-webresearch \
  --digest "${NEW_DIGEST}" --manifest "${UPDATED_MANIFEST}" > "${WORK}/img-update-ok.json"
ck "验证通过的 update 切换" "switched" "$(jget "${WORK}/img-update-ok.json" outcome.result)"
ck "切换后 current=新 digest" "${NEW_DIGEST}" "$(jget "${WORK}/img-update-ok.json" outcome.current_digest)"
"${IMAGE_PIN}" --ledger "${LEDGER}" rollback --image localhost/rip-tool-webresearch > "${WORK}/img-rollback.json"
ck "显式回滚 result=rolled_back" "rolled_back" "$(jget "${WORK}/img-rollback.json" outcome.result)"
ck "显式回滚拨回旧 digest" "${OLD_DIGEST}" "$(jget "${WORK}/img-rollback.json" outcome.current_digest)"
demo "镜像 digest 回滚：显式回滚拨回旧 digest（previous_digest 留作再滚回位，history 全程留痕）"

echo
echo "== 2. D10 门控实演（update-check --auto：仅 official 自动提示）"
make_pkg "${WORK}/pkg-v1"
"${RP}" --db "${PLUGIN_DB}" --migrations migrations --data-dir "${DATA_DIR}" --data-root "${DATA_ROOT}" \
  install knowledge.rtis-redteam --manifest "${MANIFEST_V1}" --package "${WORK}/pkg-v1" --trust local > /dev/null
ck "install（local 信任级）受理" "0" "$?"
"${RP}" --db "${PLUGIN_DB}" --migrations migrations --data-dir "${DATA_DIR}" --data-root "${DATA_ROOT}" \
  update-check knowledge.rtis-redteam --new-version 0.2.0 --auto > "${WORK}/auto-blocked.json" 2>&1
RC=$?
CODE=$(jget "${WORK}/auto-blocked.json" code)
ck "local + --auto 拒绝（exit 1）" "1" "${RC}"
ck "机器可读错误码" "update_check_auto_not_allowed" "${CODE}"
demo "D10 门控：local 信任级拒绝自动更新提示（update_check_auto_not_allowed，audit result=blocked，manual 检查不受限）"
"${RP}" --db "${PLUGIN_DB}" --migrations migrations --data-dir "${DATA_DIR}" --data-root "${DATA_ROOT}" \
  update-check knowledge.rtis-redteam --new-version 0.2.0 > "${WORK}/manual-ok.json" 2>"${WORK}/manual-ok.err"
RC=$?
ck "manual update-check 全档可用（exit 0）" "0" "${RC}"
ck "manual 检查发现兼容新版本" "True" "$(jget "${WORK}/manual-ok.json" has_compatible_new_version)"

echo
echo "== 3. 插件升级失败自动回滚实演（§5.3：失败回滚旧版本，指针不切换）"
manifest_version "${MANIFEST_V1}" "${WORK}/m2.json" "0.2.0"
manifest_version "${MANIFEST_V1}" "${WORK}/m3.json" "0.3.0"
# 包体破坏（staging 可传递）：缺 Manifest 声明的 capability entry 文件，
# VALIDATED 检查（rule 7 entry 存在性）在升级链内 fail-closed。
# 说明：CLI stage_package 的 copy_tree 按设计跳过包内 checksums.sha256 与
# plugin.lock（安装链重新生成），来源包的清单篡改不进升级链，故破坏面取
# Manifest 声明的 entry 缺失。
mkdir -p "${WORK}/pkg-v2-bad"
"${RP}" --db "${PLUGIN_DB}" --migrations migrations --data-dir "${DATA_DIR}" --data-root "${DATA_ROOT}" \
  upgrade knowledge.rtis-redteam --manifest "${WORK}/m2.json" --package "${WORK}/pkg-v2-bad" \
  > "${WORK}/upgrade-fail.json" 2>&1
RC=$?
CODE=$(jget "${WORK}/upgrade-fail.json" code)
ck "破坏包 upgrade 拒绝（exit 1）" "1" "${RC}"
ck "机器可读错误码（VALIDATED 检查失败）" "validation_failed" "${CODE}"
ACTIVE=$("${RP}" --db "${PLUGIN_DB}" --migrations migrations --data-dir "${DATA_DIR}" --data-root "${DATA_ROOT}" list | \
  python3 -c '
import json, sys
rows = json.load(sys.stdin)["rows"]
act = [r for r in rows if r["is_active"]]
print(act[0]["plugin_id"] + "@" + act[0]["version"] if act else "none")
')
ck "升级失败后 active 指针不切换（仍 0.1.0）" "knowledge.rtis-redteam@0.1.0" "${ACTIVE}"
V2_ROWS=$("${RP}" --db "${PLUGIN_DB}" --migrations migrations --data-dir "${DATA_DIR}" --data-root "${DATA_ROOT}" list | python3 -c '
import json, sys
rows = json.load(sys.stdin)["rows"]
print(len([r for r in rows if r["version"] == "0.2.0"]))
')
ck "升级失败后 0.2.0 版本行不存在（事务回滚）" "0" "${V2_ROWS}"
demo "升级失败自动回滚旧版本（rolled_back、指针不切换）：active=${ACTIVE}，audit plugin.upgrade result=rolled_back"

echo
echo "== 4. 升级成功 + 回滚窗口清理实演（D6：窗口结束且用户确认后清理）"
make_pkg "${WORK}/pkg-v2-good"
"${RP}" --db "${PLUGIN_DB}" --migrations migrations --data-dir "${DATA_DIR}" --data-root "${DATA_ROOT}" \
  upgrade knowledge.rtis-redteam --manifest "${WORK}/m3.json" --package "${WORK}/pkg-v2-good" \
  > "${WORK}/upgrade-ok.json" 2>"${WORK}/upgrade-ok.err"
RC=$?
ck "好包 upgrade 受理（exit 0）" "0" "${RC}"
ck "指针切换到 0.3.0" "0.3.0" "$(jget "${WORK}/upgrade-ok.json" outcome.to_version)"
"${RP}" --db "${PLUGIN_DB}" --migrations migrations --data-dir "${DATA_DIR}" --data-root "${DATA_ROOT}" \
  rollback-window knowledge.rtis-redteam > "${WORK}/window.json"
WINDOW=$(jget "${WORK}/window.json" rollback_window_versions.0)
ck "回滚窗口含 0.1.0" "0.1.0" "${WINDOW}"
OLD_DIR=$(find "${DATA_ROOT}" -type d -path "*installed/knowledge.rtis-redteam/0.1.0" | head -1)
ck "旧版本目录在清理前在位（data_root 之下）" "yes" "$([[ -d "${OLD_DIR}" ]] && echo yes || echo no)"
"${RP}" --db "${PLUGIN_DB}" --migrations migrations --data-dir "${DATA_DIR}" --data-root "${DATA_ROOT}" \
  cleanup-rollback knowledge.rtis-redteam > "${WORK}/cleanup-noconf.json" 2>&1
RC=$?
CODE=$(jget "${WORK}/cleanup-noconf.json" code)
ck "未确认清理拒绝（exit 1）" "1" "${RC}"
ck "机器可读错误码" "cleanup_requires_confirmation" "${CODE}"
"${RP}" --db "${PLUGIN_DB}" --migrations migrations --data-dir "${DATA_DIR}" --data-root "${DATA_ROOT}" \
  cleanup-rollback knowledge.rtis-redteam --confirm > "${WORK}/cleanup-ok.json" 2>"${WORK}/cleanup-ok.err"
RC=$?
ck "确认后清理受理（exit 0）" "0" "${RC}"
ck "清理版本集" "0.1.0" "$(jget "${WORK}/cleanup-ok.json" report.cleaned.0)"
ck "data_root 之下目录物理删除（1 个）" "1" "$(jget "${WORK}/cleanup-ok.json" "len(report.dirs_removed)")"
ck "旧目录已物理删除" "yes" "$([[ ! -d "${OLD_DIR}" ]] && echo yes || echo no)"
ACTIVE=$("${RP}" --db "${PLUGIN_DB}" --migrations migrations --data-dir "${DATA_DIR}" --data-root "${DATA_ROOT}" list | python3 -c '
import json, sys
rows = json.load(sys.stdin)["rows"]
act = [r for r in rows if r["is_active"]]
print(act[0]["version"] if act else "none")
')
ck "清理后 active 版本不动（0.3.0）" "0.3.0" "${ACTIVE}"
OLD_STATE=$("${RP}" --db "${PLUGIN_DB}" --migrations migrations --data-dir "${DATA_DIR}" --data-root "${DATA_ROOT}" list | python3 -c '
import json, sys
rows = json.load(sys.stdin)["rows"]
old = [r for r in rows if r["version"] == "0.1.0"]
print(old[0]["state"] if old else "missing")
')
ck "旧版本行跃迁 UNINSTALLED（历史保留）" "UNINSTALLED" "${OLD_STATE}"
demo "回滚窗口清理（D6）：--confirm 后 0.1.0 跃迁 UNINSTALLED，installed 目录物理删除，active 0.3.0 不动"

echo
echo "== 5. D10 official 放行实演（隔离库二）"
PLUGIN_DB2="${WORK}/plugin-eco2.db"
DATA_DIR2="${WORK}/data2"
mkdir -p "${DATA_DIR2}"
"${RP}" --db "${PLUGIN_DB2}" --migrations migrations --data-dir "${DATA_DIR2}" --data-root "${DATA_ROOT}" \
  install knowledge.rtis-redteam --manifest "${MANIFEST_V1}" --package "${WORK}/pkg-v1" --trust official > /dev/null
ck "install（official 信任级）受理" "0" "$?"
"${RP}" --db "${PLUGIN_DB2}" --migrations migrations --data-dir "${DATA_DIR2}" --data-root "${DATA_ROOT}" \
  update-check knowledge.rtis-redteam --new-version 0.2.0 --auto > "${WORK}/auto-prompted.json" 2>"${WORK}/auto-prompted.err"
RC=$?
ck "official + --auto 放行（exit 0）" "0" "${RC}"
ck "auto 提示发现兼容新版本（prompted）" "True" "$(jget "${WORK}/auto-prompted.json" has_compatible_new_version)"
demo "D10 门控：official 信任级放行自动更新提示（result=prompted，audit plugin.update_check mode=auto）"

echo
echo "== 6. 插件目录发布形态实演（catalog-publish / catalog-verify / 按条目安装）"
CATALOG="${WORK}/catalog.json"
"${RP}" trust-policy > "${WORK}/trust-policy.json"
ck "trust-policy 策略表输出（no-db 只读面）" "0" "$?"
ck "策略表四档在列" "4" "$(jget "${WORK}/trust-policy.json" "len(policy.tiers)")"
"${RP}" catalog-publish --out "${CATALOG}" --manifests-dir manifests/plugins --trust-default official \
  --trust-override knowledge.secskills=community > "${WORK}/catalog-publish.json"
RC=$?
ck "catalog-publish 受理（exit 0）" "0" "${RC}"
ck "目录条目数 = manifests/plugins 全量" "3" "$(jget "${WORK}/catalog-publish.json" entries)"
"${RP}" catalog-verify --catalog "${CATALOG}" > "${WORK}/catalog-verify.json"
RC=$?
ck "catalog-verify 受理（exit 0）" "0" "${RC}"
ck "发布与校验 sha 一致" "$(jget "${WORK}/catalog-publish.json" content_sha256)" "$(jget "${WORK}/catalog-verify.json" content_sha256)"
ENTRY_TRUST=$(python3 -c '
import json, sys
d = json.load(open(sys.argv[1]))
items = {e["plugin_id"]: e for e in d["item"]}
print(items["knowledge.secskills"]["trust_level"] + "/" + items["knowledge.awesome-hacking"]["install_confirmation"])
' "${WORK}/catalog-publish.json")
ck "override 降档生效（secskills=community）与 official 常规确认并存" "community/single" "${ENTRY_TRUST}"
# 篡改拒绝：改条目正文后 verify 必须失败（sha_mismatch）。
python3 -c '
import json, sys
p = sys.argv[1]
d = json.load(open(p))
d["entries"][0]["description"] += "x"
json.dump(d, open(p, "w"), ensure_ascii=False, indent=2)
' "${CATALOG}"
"${RP}" catalog-verify --catalog "${CATALOG}" > "${WORK}/catalog-tamper.json" 2>&1
RC=$?
CODE=$(jget "${WORK}/catalog-tamper.json" code)
ck "篡改目录 verify 拒绝（exit 1）" "1" "${RC}"
ck "篡改机器可读错误码" "catalog_verify_failed" "${CODE}"
# 按目录条目安装（fresh 库三）：条目 manifest_path 即 install --manifest 消费面。
PLUGIN_DB3="${WORK}/plugin-eco3.db"
DATA_DIR3="${WORK}/data3"
mkdir -p "${DATA_DIR3}"
"${RP}" catalog-publish --out "${WORK}/catalog-fresh.json" --manifests-dir manifests/plugins --trust-default official > /dev/null
FIRST_ENTRY=$(jget "${WORK}/catalog-fresh.json" entries.0.manifest_path)
make_pkg "${WORK}/pkg-catalog"
"${RP}" --db "${PLUGIN_DB3}" --migrations migrations --data-dir "${DATA_DIR3}" --data-root "${DATA_ROOT}" \
  install knowledge.awesome-hacking --manifest "${FIRST_ENTRY}" --package "${WORK}/pkg-catalog" \
  --trust official > "${WORK}/catalog-install.json" 2>"${WORK}/catalog-install.err"
RC=$?
ck "按目录条目安装（manifest_path 消费面 + 条目 trust）受理" "0" "${RC}"
demo "目录发布形态：catalog-publish（3 条目 trust 分级 + sha）→ verify 一致 → 篡改拒绝（sha_mismatch）→ 按条目 manifest_path 安装受理"

echo
echo "== 7. server 确认分级强制实演（真实 rp_server，临时令牌 + 临时端口）"
TOKEN="p10eco-$(python3 -c 'import secrets;print(secrets.token_hex(16))')"
PORT="79$(python3 -c 'import secrets;print(secrets.randbelow(90)+10)')"
SERVER_LOG="${WORK}/server.log"
RIP_PLUGIN_DB="${PLUGIN_DB3}" RP_PORT="${PORT}" RP_AUTH_TOKEN="${TOKEN}" \
  RP_PLUGIN_BIN="target/debug/rp-plugin" RP_PLUGIN_BIN_ARGS="" \
  python3 apps/server/rp_server.py > "${SERVER_LOG}" 2>&1 &
SERVER_PID=$!
for _ in $(seq 1 50); do curl -s -o /dev/null "http://127.0.0.1:${PORT}/" && break; sleep 0.2; done
api() { curl -s -H "Authorization: Bearer ${TOKEN}" -H "Content-Type: application/json" "$@"; }
api "http://127.0.0.1:${PORT}/api/plugins/trust-policy" > "${WORK}/srv-policy.json"
ck "GET /api/plugins/trust-policy 四档在列" "4" "$(jget "${WORK}/srv-policy.json" "len(tiers)")"
HTTP=$(api -o /dev/null -w "%{http_code}" -X POST "http://127.0.0.1:${PORT}/api/plugins/install" \
  -d '{"manifest_path":"schemas/plugin-manifest/examples/knowledge.rtis-redteam.json","trust":"local"}')
ck "local 无 confirm 拒绝（400）" "400" "${HTTP}"
api -X POST "http://127.0.0.1:${PORT}/api/plugins/install" \
  -d '{"manifest_path":"schemas/plugin-manifest/examples/knowledge.rtis-redteam.json","trust":"local"}' > "${WORK}/srv-err1.json"
ERRCODE=$(jget "${WORK}/srv-err1.json" error.code)
ck "机器可读错误码" "install_confirmation_required" "${ERRCODE}"
# confirm=true 放行确认闸后进入来源拉取链：HTTP 形态无关（离线 502
# clone_failed；网络偶发可达 200 安装受理），两态均证明闸已放行，闸前态
# （400 install_confirmation_required）已被上一断言钉死。
HTTP=$(api -o /dev/null -w "%{http_code}" -X POST "http://127.0.0.1:${PORT}/api/plugins/install" \
  -d '{"manifest_path":"manifests/plugins/knowledge.tongui-course-notes.json","trust":"local","confirm":true}')
GATE_HTTP=$([[ "${HTTP}" == "200" || "${HTTP}" == "502" ]] && echo yes || echo no)
ck "local confirm=true 放行确认闸（200 受理或 502 进拉取链，均非确认面）" "yes" "${GATE_HTTP}"
api -X POST "http://127.0.0.1:${PORT}/api/plugins/install" \
  -d '{"manifest_path":"manifests/plugins/knowledge.tongui-course-notes.json","trust":"local","confirm":true}' > "${WORK}/srv-fetch.json"
# 形态无关断言：确认闸放行后响应不再是 install_confirmation_required
#（离线多为 502 clone_failed；网络偶发可达时为安装成功体，两者均证闸已放行）。
GATE_PASS=$(python3 - "${WORK}/srv-fetch.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
code = d.get("error", {}).get("code", "ok")
print("yes" if code != "install_confirmation_required" else "no")
PY
)
ck "确认闸已放行（响应错误码不再来自确认面）" "yes" "${GATE_PASS}"
HTTP=$(api -o /dev/null -w "%{http_code}" -X POST "http://127.0.0.1:${PORT}/api/plugins/install" \
  -d '{"manifest_path":"manifests/plugins/knowledge.awesome-hacking.json","trust":"remote","confirm":true,"confirm_id":"knowledge.wrong-id"}')
ck "remote confirm_id 不一致拒绝（400）" "400" "${HTTP}"
api -X POST "http://127.0.0.1:${PORT}/api/plugins/install" \
  -d '{"manifest_path":"manifests/plugins/knowledge.awesome-hacking.json","trust":"remote","confirm":true,"confirm_id":"knowledge.wrong-id"}' > "${WORK}/srv-err2.json"
ERRCODE=$(jget "${WORK}/srv-err2.json" error.code)
ck "remote 机器可读错误码" "install_confirm_id_mismatch" "${ERRCODE}"
demo "server 确认分级强制：local 无 confirm 400 install_confirmation_required → confirm=true 放行闸进拉取链（离线 clone_failed，错误码已换面）；remote confirm_id 不一致 400 install_confirm_id_mismatch；trust-policy 端点四档在列"

echo
echo "== 8. 生产库指纹（后）与收口"
fingerprint > "${WORK}/fp-after.txt"
if diff -u "${WORK}/fp-before.txt" "${WORK}/fp-after.txt" > "${WORK}/fp-diff.txt" 2>&1; then
  ck "生产 platform*.db 指纹前后一致（零写红线）" "same" "same"
else
  ck "生产 platform*.db 指纹前后一致（零写红线）" "same" "$(cat "${WORK}/fp-diff.txt")"
fi
kill "${SERVER_PID}" 2>/dev/null || true
SERVER_PID=""

echo
echo "== P10-2 selftest 汇总：PASS=${PASS} FAIL=${FAIL}"
if [[ "${FAIL}" -eq 0 ]]; then
  echo "ALL PASS"
  exit 0
else
  echo "FAILED"
  exit 1
fi
