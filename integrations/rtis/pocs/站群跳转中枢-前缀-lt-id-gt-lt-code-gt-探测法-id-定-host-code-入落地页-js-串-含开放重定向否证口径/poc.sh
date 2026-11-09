#!/usr/bin/env bash
# 站群跳转中枢探测 —— id 决定 host、code 进落地页 JS 字符串
# 用法: ./poc.sh <HUB_URL> [PREFIX] [CODE] [ID列表，逗号分隔]
#   例: ./poc.sh https://hub.example /mk abc123 1001,1002,999999
set -u
. "$(dirname "$0")/../_lib/common.sh"
preflight || exit 1

HUB="${1:-}"; [ -n "$HUB" ] || usage_die "$0 <HUB_URL> [PREFIX] [CODE] [ID列表]"
HUB="${HUB%/}"
PREFIX="${2:-/mk}"; PREFIX="/${PREFIX#/}"
CODE="${3:-testcode}"
IDS="${4:-1,2,100000,999999}"

TIMEOUT="${TIMEOUT:-10}"

# 取 Location；CloudFront 偶发 200 空 body，必须重发一次
loc_of() {
  local url="$1" out loc code
  for attempt in 1 2; do
    out=$(curl -sk --max-time "$TIMEOUT" -A "$UA" -o /dev/null \
          -D - "$url" 2>/dev/null)
    code=$(printf '%s' "$out" | awk 'NR==1{print $2}')
    loc=$(printf '%s' "$out" | tr -d '\r' | awk 'tolower($1)=="location:"{print $2; exit}')
    if [ -n "$loc" ]; then printf '%s\t%s' "$code" "$loc"; return; fi
    [ "$code" = "200" ] && dim "  第 ${attempt} 次 200 无 Location，重发（CloudFront 已知抖动）"
    sleep 0.6
  done
  printf '%s\t' "$code"
}

hdr "站群跳转中枢探测"
dim "中枢: $HUB$PREFIX/<id>/$CODE"
echo

# ---------- 1. id 是否活维度 ----------
log "步骤1 · id 维度活性（换 id → Location 是否跟着变）"
declare -A LOC=()
prev=""; changed=0; alive=0
IFS=',' read -ra IDARR <<< "$IDS"
for id in "${IDARR[@]}"; do
  id=$(printf '%s' "$id" | tr -d ' ')
  r=$(loc_of "$HUB$PREFIX/$id/$CODE")
  c="${r%%$'\t'*}"; l="${r#*$'\t'}"
  if [ -n "$l" ]; then
    alive=$((alive+1))
    printf '  id=%-10s %s  → %s\n' "$id" "$c" "$l"
    LOC["$id"]="$l"
  else
    printf '  id=%-10s %s  → (无 Location)\n' "$id" "$c"
  fi
done

# 比较不同 id 的 Location 主机是否不同
hosts=$(for k in "${!LOC[@]}"; do
          printf '%s' "${LOC[$k]}" | sed -E 's|^[a-z]+://([^/]+).*|\1|'
        done | sort -u)
nhost=$(printf '%s\n' "$hosts" | grep -c . || true)
echo
if [ "$alive" -eq 0 ]; then
  err "所有 id 均无 Location —— 中枢路径模板不对，或已被下线"
  dim "试试换前缀：/mk /hy /go /jump /r /apk /dy 语义等价"
  exit 2
fi
if [ "$nhost" -gt 1 ]; then
  ok "id 是 DB 活维度：${nhost} 个不同落地 host"
  printf '%s\n' "$hosts" | sed 's/^/    → /'
else
  warn "所有 id 落到同一 host（${hosts}）—— id 可能只决定路径，不决定域"
fi
echo

# ---------- 2. 前缀语义 ----------
log "步骤2 · 前缀是否无实义"
for p in /mk /hy /go /jump /r; do
  r=$(loc_of "$HUB$p/${IDARR[0]}/$CODE")
  l="${r#*$'\t'}"
  printf '  %-6s → %s\n' "$p" "${l:-(无)}"
done
echo

# ---------- 3. 开放重定向判定 ----------
hdr "步骤3 · 开放重定向判定（看 code 在模板里的落点）"
sample=""
for k in "${!LOC[@]}"; do sample="${LOC[$k]}"; break; done
dim "样本: $sample"
echo
verdict=""
case "$sample" in
  *"://$CODE"*|*/"$CODE"/*)
    ok "code 占据 authority/path 起始段 → 可能成立开放重定向，需实测"
    verdict="maybe" ;;
  *"-$CODE"*|*"_$CODE"*|*"/$CODE"*)
    err "code 落在固定前缀之后（前缀切断了 // 的 authority 语义）→ 不成立"
    dim "例：/hjsqnew-<code> 中 //evil.com 会变成 /hjsqnew-//evil.com，仍是同 host 路径"
    verdict="no" ;;
  *"?"*"$CODE"*|*"&"*"$CODE"*)
    err "code 只进查询值，host 固定 → 不成立"
    verdict="no" ;;
  *)
    dim "模板形态未匹配已知模式，人工看：$sample"
    verdict="?" ;;
esac
echo

# ---------- 4. code 落点（落地页）----------
if [ "$verdict" != "no" ] && [ -n "$sample" ]; then
  log "步骤4 · code 在落地页的落点"
  body=$(req GET "$sample")
  if printf '%s' "$body" | grep -q "hex2bin("; then
    ok "落地页使用 document.write(hex2bin(...))，需解码后审计"
    dim "下一步: curl '$sample' | python3 解 hex2bin 后 grep channel:/copyToClip"
  else
    dim "未发现 hex2bin 包装（可能是明文或其它模板族）"
  fi
  echo
  warn "注意：页面里 <!-- uuid_ts_ip --> 的注释 IP 是访客出口 IP，不是源站泄露"
fi

hdr "结论"
case "$verdict" in
  maybe) ok  "需实测：把 $CODE 换成 //evil.example.com 再取一次 Location"; exit 0 ;;
  no)    err "开放重定向不成立（code 落点安全）"; exit 2 ;;
  *)     warn "无法自动定案，见上方样本"; exit 3 ;;
esac
