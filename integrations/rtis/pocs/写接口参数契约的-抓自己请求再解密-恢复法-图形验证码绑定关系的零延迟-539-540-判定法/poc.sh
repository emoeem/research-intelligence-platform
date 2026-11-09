#!/usr/bin/env bash
# 写接口参数契约恢复 —— 单变量替换定位「哪些字段参与服务端签名」
# 用法: ./poc.sh <URL> <METHOD> (--json '<body>' | --form 'a=1&b=2') [-H 'k: v']...
#
# 原理：同一请求重放两次确认响应稳定 → 逐字段替换为标记值 →
#       只有「参与签名/校验」的字段被改后才会产生新响应。
set -u
. "$(dirname "$0")/../_lib/common.sh"
preflight || exit 1

URL="${1:-}"; METHOD="${2:-POST}"
[ -n "$URL" ] || usage_die "用法: $0 <URL> <METHOD> (--json '<body>'|--form 'k=v&k2=v2') [-H 'k: v']..."
shift 2 2>/dev/null || true

KIND=""; BODY=""; HS=()
while [ $# -gt 0 ]; do
  case "$1" in
    --json) KIND=json; BODY="${2:-}"; shift 2 ;;
    --form) KIND=form; BODY="${2:-}"; shift 2 ;;
    -H)     HS+=(-H "${2:-}"); shift 2 ;;
    *)      shift ;;
  esac
done
[ -n "$KIND" ] || usage_die "必须给 --json 或 --form"

TIMEOUT="${TIMEOUT:-15}"
CT="application/json"
[ "$KIND" = "form" ] && CT="application/x-www-form-urlencoded"
HS+=(-H "Content-Type: $CT")

send() {  # send <body> → "code<TAB>fp"
  local b="$1" r
  r=$(reqc "$METHOD" "$URL" "${HS[@]}" --data-raw "$b")
  local c="${r%%$'\t'*}"; local bb="${r#*$'\t'}"
  printf '%s\t%s' "$c" "$(printf '%s' "$bb" | fp)"
}

hdr "写接口参数契约探测"
dim "$METHOD $URL"
dim "载体: $KIND"
echo

# ---------- 0. 稳定性对照 ----------
log "步骤0 · 重放稳定性（两次同请求必须一致，否则差分无效）"
r1=$(send "$BODY"); r2=$(send "$BODY")
c1="${r1%%$'\t'*}"; f1="${r1#*$'\t'}"
c2="${r2%%$'\t'*}"; f2="${r2#*$'\t'}"
dim "第一次: $c1"
dim "第二次: $c2"
if [ "$f1" != "$f2" ]; then
  warn "两次响应结构不同 —— 存在时间戳/随机数/nonce，差分前必须先归一化"
  dim "建议：把每次变化的字段加进 _lib/common.sh 的 norm()"
else
  ok "响应稳定，可以开始单变量差分"
fi
BASE_FP="$f1"; BASE_CODE="$c1"
echo

# ---------- 1. 提取字段 ----------
if [ "$KIND" = "json" ]; then
  need jq || exit 1
  mapfile -t FIELDS < <(printf '%s' "$BODY" | jq -r 'paths(scalars) | join(".")' 2>/dev/null)
else
  mapfile -t FIELDS < <(printf '%s' "$BODY" | tr '&' '\n' | awk -F= 'NF{print $1}')
fi
if [ "${#FIELDS[@]}" -eq 0 ]; then
  err "没解析出任何字段，检查 body 格式"
  exit 2
fi
log "发现 ${#FIELDS[@]} 个字段: ${FIELDS[*]}"
echo

# ---------- 2. 单变量替换 ----------
hdr "步骤1 · 单变量替换（每次只改一个字段）"
printf '  %-28s %-6s %-8s %s\n' "字段" "CODE" "变化?" "判定"
rule 72
declare -a SIGNED=()
for f in "${FIELDS[@]}"; do
  if [ "$KIND" = "json" ]; then
    nb=$(printf '%s' "$BODY" | jq -c --arg k "$f" '
      (setpath(($k|split(".")|map(tonumber? // .));
        (getpath($k|split(".")|map(tonumber? // .)) as $v
         | if ($v|type)=="number" then 987654321 else "zzz_PROBE_" + ($v|type) end)))' 2>/dev/null)
  else
    nb=$(printf '%s' "$BODY" | tr '&' '\n' | awk -F= -v k="$f" 'BEGIN{OFS="="}
      $1==k{print $1,"zzz_PROBE"; next} {print}' | paste -sd'&' -)
  fi
  [ -z "$nb" ] && { dim "$f → 构造失败，跳过"; continue; }

  r=$(send "$nb"); c="${r%%$'\t'*}"; fpx="${r#*$'\t'}"
  if [ "$c" = "$BASE_CODE" ] && [ "$fpx" = "$BASE_FP" ]; then
    printf '  %-28s %-6s %-8s %s\n' "$f" "$c" "-" "不参与校验（可随意改）"
  elif [ "$c" = "$BASE_CODE" ]; then
    printf '  %-28s %-6s %-8s %s\n' "$f" "$c" "Δbody" "存在但内容相关（需人工看）"
    SIGNED+=("$f")
  else
    printf '  %-28s %-6s %-8s %s\n' "$f" "$c" "Δcode" "★ 参与校验/签名"
    SIGNED+=("$f")
  fi
done
echo

# ---------- 3. 结论 ----------
hdr "结论"
if [ "${#SIGNED[@]}" -eq 0 ]; then
  warn "没有字段影响响应 —— 该接口可能完全不校验（或全部走异步）"
  dim "下一步：换一个必然非法的值（负数/超长/不存在 id）再试"
  exit 3
fi
ok "参与校验的字段（${#SIGNED[@]} 个）: ${SIGNED[*]}"
dim "签名链恢复方向：只改这些字段中的一个 + 保持其余原样，观察错误码语义"
dim "业务码 oracle：把 signed 字段改成「格式合法但值不存在」，错误码会从 签名错误 变成 主体不存在"
echo
warn "本脚本只做单变量替换，不构造有效绕过；同一 captchaId 的复用判定需人工按 NOTES.md 步骤做"
