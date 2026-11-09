#!/usr/bin/env bash
# 四态鉴权探针：判定一个接口的 token 校验是「缺失」「存在性」「值校验」「fail-open（写反）」中的哪一种
# 用法：./auth4state.sh 'https://host/prefix' '/api/post/getPostReplyList' 'prid=1&pageNum=1'
# 前提：一个有效 token（<TOKEN>），以及一个应当是"他人"的 uid（可选）
set -uo pipefail
# ---- 参数检查 ----
if [ $# -lt 1 ]; then printf '用法: ./auth4state.sh <URL>\n' >&2; exit 1; fi
BASE="$1"; PATH_="$2"; BODY="$3"; TOK="${4:-}"; SLEEP="${5:-2}"

probe () { # $1=标签 $2=token头(可空)
  local ts http bytes code body
  ts=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  local args=(-sS -o /tmp/a4s.body -w '%{http_code} %{size_download}' -X POST "$BASE$PATH_"
              -H 'Content-Type: application/x-www-form-urlencoded'
              -H 'User-Agent: Mozilla/5.0 (Linux; Android 13) AppleWebKit/537.36 Chrome/126.0 Mobile Safari/537.36')
  [ -n "${2:-}" ] && args+=(-H "token: $2")
  read -r http bytes < <("${args[@]}" --data "$BODY" 2>/dev/null)
  body=$(sed '1s/^\xEF\xBB\xBF//' /tmp/a4s.body 2>/dev/null)   # 剥 BOM，jq 才不会挂
  code=$(printf '%s' "$body" | grep -oE '"code":[0-9]+' | head -1 | cut -d: -f2)
  printf '%-22s %s | HTTP %s | %s B | code=%s | %s\n' "$1" "$ts" "${http:-000}" "${bytes:-0}" "${code:-?}" \
         "$(printf '%s' "$body" | head -c 120 | tr -d '\n')"
  sleep "$SLEEP"
}

echo "=== 基线：不带 token 头 ===";            probe "no-header"      ""
echo "=== 反 1：垃圾 token ===";               probe "garbage"        "garbage_token_123"
echo "=== 反 2：同形态随机 44 字符 base64 ==="; probe "random-44-b64"  "$(head -c48 /dev/urandom | base64 | head -c44)"
[ -n "$TOK" ] && { echo "=== 正：有效 token ==="; probe "valid" "$TOK"; }

cat <<'EOF'

判读表（code 取 body 里的业务码，不看 HTTP 码）：
  no-header 放行 + garbage 拒绝          → fail-open：条件写反（if (h!=null && !valid) reject;）
  no-header 拒绝 + garbage 拒绝 + valid 放行 → 校验正常
  no-header 拒绝 + garbage 放行 + valid 放行 → 存在性校验（只看头在不在）
  no-header 拒绝 + random-44 放行         → 只校验形态/长度，不是查表
  三者全放行                              → 该路径根本没挂鉴权
再加一步「业务参数是否生效」的对照：把 uid/ruid 换成不存在的值，
  若响应与正常值逐字节相同（比对 sha256）⇒ 参数已被忽略（不要误判成"IDOR"）。
EOF