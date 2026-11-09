#!/usr/bin/env bash
# ============================================================================
# 鉴权实现四态判定探针
# 用法: ./poc.sh https://TARGET/api/user/info
#       ./poc.sh https://TARGET/api/user/info 'Authorization: Bearer TOKEN'
#
# 四态（这是本项目反复用到的分类，能直接决定下一步怎么打）：
#   1. NO-AUTH    没有任何鉴权，直接放行
#   2. EXISTENCE  只校验「token 是否存在/格式对不对」，值不校验
#                 → 任意伪造 token 即可冒充任意用户
#   3. VALUE      校验 token 值（查库/验签），最正常
#   4. FAIL-OPEN  异常路径放行：格式错误时反而」通过」，正确 token 却报错
#                 → 极其危险，通常伴随异常处理写反
#
# 【判据】用「错误类型区分」而不是「有没有 401」
#   大多数框架对以下四种 bad token 返回不同的错误：
#     不带 token      → "token 不能为空" / 401
#     空 token        → 同上
#     格式错 token    → "token 格式错误" / 解析异常
#     格式对值错 token → "token 无效" / "用户不存在" / 签名不匹配
#     过期 token      → "token 已过期"
#   如果这五种情况返回的**错误码或文案有区分**，说明校验逻辑是分层的，
#   可以据此定位到哪一层可以被绕过。
#   如果五种都返回同一个 401 → 统一鉴权，无可利用面。
#
# 【常见误报】
#   1. 网关先拦了（Nginx/Spring Gateway）→ 你测到的是网关不是应用。
#      检查 X-Powered-By / Server 头，或换一个内部路径试。
#   2. 返回 200 但 body 是 {"code":401,"msg":"未登录"} → 必须看业务码。
#   3. 该接口本来就公开 → 换一个明确需要登录的接口（/user/info, /order/list）。
# ============================================================================
set -uo pipefail
source "$(dirname "$0")/../_lib/common.sh"

URL="${1:-}"; HDR="${2:-}"
[ -z "$URL" ] && usage_die "$0 <URL> ['Header-Name: value']"
preflight || exit 1

HN="Authorization"
[ -n "$HDR" ] && HN="${HDR%%:*}"

send() {  # send <hdr-value|-> [method]
  local v="$1" m="${2:-GET}"
  if [ "$v" = "-" ]; then
    req "$m" "$URL"
  else
    req "$m" "$URL" -H "$HN: $v"
  fi
}

# 业务码提取（很多系统 HTTP 200 + body 里带 code）
bizcode() {
  printf '%s' "$1" | grep -oE '"(code|errno|status|errcode|ret)"\s*:\s*-?[0-9]+' \
    | head -1 | grep -oE '\-?[0-9]+$'
}

# 文案提取（错误 message，中英都取）
bizmsg() {
  printf '%s' "$1" | grep -oE '"(msg|message|error|errmsg|info)"\s*:\s*"[^"]*"' \
    | head -1 | sed -E 's/.*:\s*"//; s/"$//'
}

# ---- 五组输入 -------------------------------------------------------------
FAKE_JWT="eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxIiwidWlkIjoxLCJleHAiOjk5OTk5OTk5OTl9.$(head -c 32 /dev/urandom | base64 | tr -d '=+/' | head -c 43)"
EXPIRED_JWT="eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxIiwidWlkIjoxLCJleHAiOjE1MDAwMDAwMDB9.$(head -c 32 /dev/urandom | base64 | tr -d '=+/' | head -c 43)"

declare -A CASE NAME RAW CODE BIZ MSG CNT
CASE[none]="-"
declare -A LBL=( [none]="不带认证头" [empty]="空值" [short]="短随机串" \
                [jwt]="格式正确 JWT(签名错)" [exp]="过期 JWT" [basic]="Basic 任意" )
CASE[empty]=""
CASE[short]="x$(date +%s | tail -c 4)"
CASE[jwt]="Bearer $FAKE_JWT"
CASE[exp]="Bearer $EXPIRED_JWT"
CASE[basic]="Basic $(printf 'admin:admin' | base64)"

echo "════ 探测 $URL （认证头: $HN）════"
printf '%-22s %-6s %-6s %-10s %s\n' "输入" "HTTP" "业务码" "长度" "提示信息"
printf '%s\n' "────────────────────────────────────────────────────────────────────"
# 取状态码与 body（一次请求同时拿两样，避免两次请求结果不一致）
fetch() {  # fetch <hdr-value>  → 全局 F_CODE / F_BODY
  local v="$1" out
  if [ "$v" = "-" ]; then
    out=$(curl -sk --max-time "$TIMEOUT" -A "$UA" -w $'\n%{http_code}' "$URL" 2>/dev/null)
  else
    out=$(curl -sk --max-time "$TIMEOUT" -A "$UA" -H "$HN: $v" -w $'\n%{http_code}' "$URL" 2>/dev/null)
  fi
  F_CODE="${out##*$'\n'}"; F_BODY="${out%$'\n'*}"
  [ -z "$F_CODE" ] && F_CODE=000
}

for k in none empty short jwt exp basic; do
  fetch "${CASE[$k]}"
  CODE[$k]="$F_CODE"
  body="$F_BODY"
  BIZ[$k]=$(bizcode "$body")
  MSG[$k]=$(bizmsg "$body")
  CNT[$k]=${#body}
  printf '%-22s %-6s %-6s %-10s %s\n' "${LBL[$k]}" "${CODE[$k]}" \
    "${BIZ[$k]:--}" "${CNT[$k]}" "${MSG[$k]:0:40}"
  sleep 0.4
done

# ---- 判定 ----------------------------------------------------------------
echo
log "判定"

# 1) NO-AUTH: 不带认证头就通
if [ "${CODE[none]}" = "200" ] && { [ -z "${BIZ[none]}" ] || [ "${BIZ[none]}" = "0" ] || [ "${BIZ[none]}" = "200" ]; }; then
  if [ "${CNT[none]}" -gt 50 ] && ! printf '%s' "${MSG[none]}" | grep -qiE 'login|未登录|请先登录|unauthor'; then
    ok "★ NO-AUTH：接口无鉴权（不带任何凭据即返回业务数据）"
  fi
fi

# 2) FAIL-OPEN: 正确格式反而失败，错误格式反而通过
if [ "${CODE[jwt]}" != "200" ] && [ "${CODE[short]}" = "200" ]; then
  ok "★★ FAIL-OPEN：格式错的反倒通过，格式对的被拒 → 异常处理写反"
fi
if [ "${CODE[empty]}" = "200" ] && [ "${CODE[none]}" != "200" ]; then
  ok "★★ FAIL-OPEN：空值通过而缺头被拒 → 空字符串绕过了校验"
fi

# 3) EXISTENCE: 不同错误值返回完全相同的响应 → 只判存在性
#    注意: 若所有输入（含「不带认证头」）返回同一响应，那是统一拦截，
#    不是 EXISTENCE，两种情况必须分开，否则会误导下一步。
same=1
for k in short jwt exp; do
  [ "${CODE[$k]}:${BIZ[$k]}:${MSG[$k]}" != "${CODE[short]}:${BIZ[short]}:${MSG[short]}" ] && same=0
done
if [ "$same" = 1 ] && [ "${CODE[short]}" != "200" ]; then
  if [ "${CODE[none]}" = "${CODE[short]}" ] && [ "${MSG[none]}" = "${MSG[short]}" ]; then
    dim "三种非法 token 与「不带头」响应完全一致 → 统一拦截，非 EXISTENCE"
  else
    warn "EXISTENCE 嫌疑：三种非法 token 响应完全一致，但与不带头不同 → 可能只校验存在性/格式"
    dim "    验证: 用两个不同的合法用户 token 交叉测试，看是否都拿到同一份数据"
  fi
fi

# 4) VALUE: 五种输入有区分错误 → 正常分层校验
distinct=$(for k in none empty short jwt exp; do echo "${CODE[$k]}|${BIZ[$k]}|${MSG[$k]}"; done | sort -u | wc -l)
if [ "$distinct" -ge 3 ]; then
  ok "VALUE：校验分层清晰（$distinct 种不同错误）→ 鉴权实现正常"
  echo "  各层错误:"
  for k in none empty short jwt exp; do
    printf '    %-20s %s %s\n' "${LBL[$k]}" "${CODE[$k]}" "${MSG[$k]:0:40}"
  done
elif [ "$distinct" = 1 ]; then
  warn "所有输入返回同一错误 → 统一拦截（可能是网关层，未到应用鉴权）"
  dim "    检查 Server / X-Powered-By 头确认是不是网关"
fi

# ---- 补充: 方法绕过 -------------------------------------------------------
echo
log "方法绕过（部分网关只对 GET 做鉴权）"
for m in GET POST PUT DELETE PATCH OPTIONS HEAD; do
  c=$(code_of "$m" "$URL")
  b=$(req "$m" "$URL")
  bz=$(bizcode "$b")
  [ "$m" = "GET" ] && continue
  if [ "$c" = "200" ] && { [ -z "$bz" ] || [ "$bz" = "0" ] || [ "$bz" = "200" ]; } && [ "${#b}" -gt 50 ]; then
    warn "  $m → 200 且返回业务数据（GET 需要鉴权，$m 不需要？）"
  else
    dim "  $m → $c"
  fi
  sleep 0.3
done

cat <<'EOF'

════ 四态速查 ════
  NO-AUTH     → 直接拿数据，找是否有 PII / 业务敏感字段
  EXISTENCE   → 伪造任意 token 试越权；重点看 token 结构（uuid? 自增 id? 用户名?）
  FAIL-OPEN   → 找触发异常的输入（超长、特殊字符、畸形 base64）
  VALUE       → 正常，转向: 弱密钥(JWT 爆破) / 算法混淆 / kid 注入 / 越权(IDOR)

════ 方法绕过补充 ════
  上面的「方法绕过」只测了无凭据情况。
  如果只有 POST 需要鉴权，还可以试:
    - 加 X-Original-URL / X-Rewrite-URL 头
    - 大小写路径 /api/User/Info
    - 结尾加 / 或 /. 或 ;
    - 参数污染 ?id=1&id=2
EOF
