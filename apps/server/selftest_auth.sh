#!/usr/bin/env bash
# M7 鉴权自验：起服务（临时令牌 + 临时端口），断言鉴权/只读/数据对账/静态安全。
# 输出每项 PASS/FAIL，全过 exit 0。只读操作，不触碰源库。
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "${HERE}/../.." && pwd)"
PORT="${SELFTEST_PORT:-7891}"
TOKEN="selftest-$(python3 -c 'import secrets; print(secrets.token_hex(16))')"
LOG="$(mktemp /tmp/rp_selftest.XXXX.log)"

PASS=0; FAIL=0
ck() { # ck <名> <期望> <实际>
  if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi
}

RP_PORT="${PORT}" RP_AUTH_TOKEN="${TOKEN}" python3 "${HERE}/rp_server.py" > "${LOG}" 2>&1 &
SRV=$!
trap 'kill ${SRV} 2>/dev/null; wait ${SRV} 2>/dev/null; rm -f "${LOG}"' EXIT
for _ in $(seq 1 30); do curl -sf "http://127.0.0.1:${PORT}/api/healthz" >/dev/null 2>&1 && break; curl -s -o /dev/null "http://127.0.0.1:${PORT}/" && break; sleep 0.2; done

# 1. 无令牌 → 401 + WWW-Authenticate
code=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:${PORT}/api/stats")
ck "无令牌访问 API 返回 401" "401" "${code}"
www=$(curl -si "http://127.0.0.1:${PORT}/api/stats" | grep -i "^WWW-Authenticate:" | tr -d "\r")
ck "401 携带 WWW-Authenticate: Bearer" "WWW-Authenticate: Bearer" "${www}"

# 2. 错误令牌 → 401
code=$(curl -s -o /dev/null -w "%{http_code}" -H "Authorization: Bearer wrong-token" "http://127.0.0.1:${PORT}/api/stats")
ck "错误令牌返回 401" "401" "${code}"

# 3. 正确令牌 → 200，且数字与库直查一致
body=$(curl -s -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}/api/stats")
api_ev=$(echo "${body}" | python3 -c "import sys,json; print(json.load(sys.stdin).get('evidence',{}).get('total','MISSING'))" 2>/dev/null || echo PARSE_FAIL)
db_ev=$(python3 -c "import sqlite3; print(sqlite3.connect('file:${REPO}/migrations/platform.db?mode=ro',uri=True).execute('SELECT COUNT(*) FROM evidence').fetchone()[0])")
ck "evidence 总数 API=库直查（fullbatch=700）" "${db_ev}" "${api_ev}"
code=$(curl -s -o /dev/null -w "%{http_code}" -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}/api/tasks")
ck "正确令牌访问 tasks 返回 200" "200" "${code}"

# 4. 静态页无需令牌（登录流程可用）
code=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:${PORT}/login.html")
ck "登录页无令牌可访问" "200" "${code}"
code=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:${PORT}/")
ck "首页静态资源无令牌可访问" "200" "${code}"

# 5. 只读语义保持
code=$(curl -s -o /dev/null -w "%{http_code}" -X POST -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${PORT}/api/stats")
ck "POST 仍为 405" "405" "${code}"

# 6. 路径穿越防护保持
code=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:${PORT}/../../etc/passwd")
ck "路径穿越仍被拒" "404" "${code}"

# 7. 鉴权关闭模式（开发态）：清空令牌来源重启
# 2026-11-10 修正：仓库根 .rp-workbench-token 存在时，原「只清环境变量」的
# 重启仍会从默认令牌文件加载令牌（401），永远进不了无鉴权分支；显式把
# RP_AUTH_TOKEN_FILE 指到空文件才真正清空来源。测试架修复，服务端行为不变。
kill "${SRV}" 2>/dev/null; wait "${SRV}" 2>/dev/null
EMPTY_TOKEN_FILE="$(mktemp /tmp/rp_empty_token.XXXX)"
trap 'kill ${SRV} 2>/dev/null; wait ${SRV} 2>/dev/null; rm -f "${LOG}" "${EMPTY_TOKEN_FILE}"' EXIT
RP_PORT="${PORT}" RP_DB_PATH="${REPO}/migrations/platform.db" \
  RP_AUTH_TOKEN="" RP_AUTH_TOKEN_FILE="${EMPTY_TOKEN_FILE}" \
  python3 "${HERE}/rp_server.py" > "${LOG}" 2>&1 &
SRV=$!
sleep 0.6
code=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:${PORT}/api/stats")
ck "无令牌配置时为开发模式（API 可用）" "200" "${code}"
grep -q "鉴权未启用" "${LOG}" && ck "开发模式启动输出显著告警" "yes" "yes" || ck "开发模式启动输出显著告警" "yes" "no"

# ============================================================================
# 8. MA-1 多 key 用例块（2026-11-22 追加；上方既有用例零改动——判据 2 载体）
#    临时库经 rp-admin 自动迁移至 026 + rp-admin 预置双 caller 双 key +
#    独立端口起服（审计 JSONL 开启、限频关闭），断言：
#    - legacy 单令牌兼容并存（审计 caller=default，D3）
#    - 双 caller 双 key 互不串扰（审计 caller 各归各，判据 3）
#    - 未知 key 401 形状不变（判据 4）
#    - key-revoke 后下一请求 401（无缓存直查，判据 5）+ 重复 revoke 幂等（判据 6）
#    - caller 停用（join 判定）后其 key 401（判据 4；停用由测试面直接对临时库
#      UPDATE——红线 3 只约束 server 校验路径，测试 fixture 写面沿 parity.rs 惯例）
#    - 审计 JSONL caller 字段三态 default/caller_id/"-"（D5）
# ============================================================================
RP_ADMIN="${RP_ADMIN_BIN:-}"
if [[ -z "${RP_ADMIN}" ]]; then
  for cand in "${CARGO_TARGET_DIR:-/home/emo/rt3-target}/debug/rp-admin" "${REPO}/target/debug/rp-admin"; do
    [[ -x "${cand}" ]] && RP_ADMIN="${cand}" && break
  done
fi
if [[ -z "${RP_ADMIN}" ]]; then
  (cd "${REPO}" && CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/home/emo/rt3-target}" \
    cargo build --offline -q -p database --bin rp-admin) \
    && RP_ADMIN="${CARGO_TARGET_DIR:-/home/emo/rt3-target}/debug/rp-admin"
fi
if [[ -n "${RP_ADMIN}" && -x "${RP_ADMIN}" ]]; then
  MK_PORT="${SELFTEST_PORT_MK:-$((PORT + 1))}"
  MK_DB="$(mktemp /tmp/rp_selftest_mk.XXXXXX.db)"
  MK_LOG="$(mktemp /tmp/rp_selftest_mk.XXXX.log)"
  MK_AUDIT="$(mktemp /tmp/rp_selftest_mk_audit.XXXX.jsonl)"
  MK_DIR="$(mktemp -d /tmp/rp_selftest_mk.XXXX.d)"

  # 预置：caller-add 自动迁移空库至 026（rp-admin 五子命令 = 026 唯一写入口）。
  "${RP_ADMIN}" caller-add --db "${MK_DB}" --name "selftest-caller-a" --level T1 \
    --caller-id caller-selftest-a > "${MK_DIR}/caller-a.json" 2>&1 \
    && ck "rp-admin caller-add 成功（自动迁移 026）" "0" "0" \
    || ck "rp-admin caller-add 成功（自动迁移 026）" "0" "$("${RP_ADMIN}" caller-add --db "${MK_DB}" --name "selftest-caller-a" --level T1 --caller-id caller-selftest-a > /dev/null 2>&1; echo $?)"
  "${RP_ADMIN}" caller-add --db "${MK_DB}" --name "selftest-caller-b" --level T0 \
    --caller-id caller-selftest-b > /dev/null 2>&1
  KEY_A_JSON=$("${RP_ADMIN}" key-gen --db "${MK_DB}" --caller-id caller-selftest-a \
    --label selftest-a 2> "${MK_DIR}/keygen-a.err")
  KEY_B_JSON=$("${RP_ADMIN}" key-gen --db "${MK_DB}" --caller-id caller-selftest-b \
    --label selftest-b 2> "${MK_DIR}/keygen-b.err")
  KEY_A=$(printf '%s' "${KEY_A_JSON}" | python3 -c "import sys,json; print(json.load(sys.stdin)['key'])")
  KEY_B=$(printf '%s' "${KEY_B_JSON}" | python3 -c "import sys,json; print(json.load(sys.stdin)['key'])")
  HASH_A=$(printf '%s' "${KEY_A_JSON}" | python3 -c "import sys,json; print(json.load(sys.stdin)['key_hash'])")
  HASH_B=$(printf '%s' "${KEY_B_JSON}" | python3 -c "import sys,json; print(json.load(sys.stdin)['key_hash'])")
  ck "key-gen JSON 可解析出明文" "2" "$( [[ -n "${KEY_A}" && -n "${KEY_B}" ]] && echo 2 || echo 0)"
  [[ "${#KEY_A}" -eq 47 && "${KEY_A}" == rp1_* && "${#KEY_B}" -eq 47 && "${KEY_B}" == rp1_* ]] \
    && ck "key 明文形状 rp1_+43 位 base64url（47 字符）" "yes" "yes" \
    || ck "key 明文形状 rp1_+43 位 base64url（47 字符）" "yes" "no"
  grep -q "仅此一次展示" "${MK_DIR}/keygen-a.err" \
    && ck "key-gen stderr 一次性警示" "yes" "yes" \
    || ck "key-gen stderr 一次性警示" "yes" "no"
  ck "key_hash=sha256(明文全文) 交叉验证" \
    "$(python3 -c "import hashlib; print(hashlib.sha256('${KEY_A}'.encode()).hexdigest())")" "${HASH_A}"
  KL_OUT=$("${RP_ADMIN}" key-list --db "${MK_DB}")
  if printf '%s' "${KL_OUT}" | grep -q "${KEY_A}"; then
    ck "key-list 不回明文" "no" "yes"
  else
    ck "key-list 不回明文" "no" "no"
  fi

  # 起第二实例（多 key 面）：审计 JSONL + 限频关闭。
  RP_PORT="${MK_PORT}" RP_AUTH_TOKEN="${TOKEN}" RP_DB_PATH="${MK_DB}" \
    RP_API_AUDIT_LOG="${MK_AUDIT}" RP_RATE_LIMIT_RPM=0 \
    python3 "${HERE}/rp_server.py" > "${MK_LOG}" 2>&1 &
  SRV_MK=$!
  trap 'kill ${SRV} ${SRV_MK} 2>/dev/null; wait ${SRV} ${SRV_MK} 2>/dev/null; rm -f "${LOG}" "${EMPTY_TOKEN_FILE}" "${MK_DB}" "${MK_LOG}" "${MK_AUDIT}"; rm -rf "${MK_DIR}"' EXIT
  for _ in $(seq 1 30); do
    curl -sf -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${MK_PORT}/api/healthz" >/dev/null 2>&1 && break
    sleep 0.2
  done

  # legacy 单令牌兼容并存（D3）。
  code=$(curl -s -o /dev/null -w "%{http_code}" -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${MK_PORT}/api/stats")
  ck "legacy 单令牌与多 key 并存（default）" "200" "${code}"
  # 双 key 各自 200。
  code=$(curl -s -o /dev/null -w "%{http_code}" -H "Authorization: Bearer ${KEY_A}" "http://127.0.0.1:${MK_PORT}/api/stats")
  ck "caller-a key 200" "200" "${code}"
  code=$(curl -s -o /dev/null -w "%{http_code}" -H "Authorization: Bearer ${KEY_B}" "http://127.0.0.1:${MK_PORT}/api/stats")
  ck "caller-b key 200" "200" "${code}"
  # 未知 key 401 形状不变（判据 4）。
  code=$(curl -s -o /dev/null -w "%{http_code}" -H "Authorization: Bearer rp1_zzz-unknown-key" "http://127.0.0.1:${MK_PORT}/api/stats")
  ck "未知 key 401" "401" "${code}"
  www=$(curl -si -H "Authorization: Bearer rp1_zzz-unknown-key" "http://127.0.0.1:${MK_PORT}/api/stats" | grep -i "^WWW-Authenticate:" | tr -d "\r")
  ck "未知 key 401 携带 WWW-Authenticate: Bearer" "WWW-Authenticate: Bearer" "${www}"
  # key-revoke 即时生效（判据 5）：A 吊销 → 401；B 不受影响（互不串扰）。
  "${RP_ADMIN}" key-revoke --db "${MK_DB}" --key-hash "${HASH_A}" > /dev/null 2>&1
  code=$(curl -s -o /dev/null -w "%{http_code}" -H "Authorization: Bearer ${KEY_A}" "http://127.0.0.1:${MK_PORT}/api/stats")
  ck "key-revoke 后下一请求 401（无缓存直查）" "401" "${code}"
  code=$(curl -s -o /dev/null -w "%{http_code}" -H "Authorization: Bearer ${KEY_B}" "http://127.0.0.1:${MK_PORT}/api/stats")
  ck "revoke 只影响 A（B 仍 200，互不串扰）" "200" "${code}"
  # 重复 revoke 幂等（判据 6）。
  REV2=$("${RP_ADMIN}" key-revoke --db "${MK_DB}" --key-hash "${HASH_A}" 2>&1); REV2_RC=$?
  ck "重复 revoke 幂等（exit 0 + already_revoked）" "0:true" \
    "${REV2_RC}:$(printf '%s' "${REV2}" | python3 -c "import sys,json; print(json.dumps(json.load(sys.stdin)['already_revoked']))" 2>/dev/null || echo parsefail)"
  # caller 停用（join 判定）：B caller 停用 → 其 key 下一请求 401。
  # （测试夹具直写临时库——红线 3 只约束 server 校验路径；单连接执行+commit。）
  python3 -c "import sqlite3; c=sqlite3.connect('${MK_DB}'); c.execute(\"UPDATE caller SET revoked_at='2026-11-22T00:00:00.000Z' WHERE caller_id='caller-selftest-b'\"); c.commit()"
  code=$(curl -s -o /dev/null -w "%{http_code}" -H "Authorization: Bearer ${KEY_B}" "http://127.0.0.1:${MK_PORT}/api/stats")
  ck "caller 停用后其 key 401（join 判定）" "401" "${code}"
  code=$(curl -s -o /dev/null -w "%{http_code}" -H "Authorization: Bearer ${TOKEN}" "http://127.0.0.1:${MK_PORT}/api/stats")
  ck "caller 停用不影响 legacy default" "200" "${code}"

  # 审计 JSONL caller 三态（D5/判据 2/3）：caller_id/default 只出现在过鉴权
  # 的 200 行；吊销 key / 停用 caller / 未知 key 的 401 = 匿名未过鉴权 → "-"
  #（401 行不允许携带任何身份）。
  AUDIT_CALLER=$(python3 - "${MK_AUDIT}" <<'PYEOF'
import json, sys
rows = [json.loads(l) for l in open(sys.argv[1], encoding="utf-8") if l.strip()]
def has(caller, status):
    return any(r.get("caller") == caller and r.get("status") == status for r in rows)
anon401 = any(r.get("caller") == "-" and r.get("status") == 401 for r in rows)
noauth401 = not any(r.get("status") == 401 and r.get("caller") not in ("-",) for r in rows)
print(",".join([
    "a200" if has("caller-selftest-a", 200) else "-",
    "b200" if has("caller-selftest-b", 200) else "-",
    "def200" if has("default", 200) else "-",
    "anon401" if anon401 else "-",
    "noauth401" if noauth401 else "-",
]))
PYEOF
)
  ck "审计 caller 三态（200 行归身份/401 行全匿名）" \
    "a200,b200,def200,anon401,noauth401" "${AUDIT_CALLER}"
else
  ck "rp-admin 可用（多 key 用例块前置）" "yes" "no"
fi

echo "----"
echo "结果：PASS=${PASS} FAIL=${FAIL}"
[[ ${FAIL} -eq 0 ]]
