#!/usr/bin/env bash
# ============================================================================
# redteam POC 公共库 v1
# 用法: source "$(dirname "$0")/../_lib/common.sh"
#
# 设计原则（为什么这样写）：
#   1. 所有判定基于「基线对照」，不基于单次响应。单次 200/404 在 CDN、
#      WAF、软 404 场景下没有判别力。
#   2. 归一化（norm）先抹掉每次请求必然变化的字段（uuid、时间戳、耗时、
#      traceId），否则每个响应都「不同」，差分全部失效。
#   3. 不做破坏性动作。全部只读探测，写操作留给人判断。
#   4. 失败不静默：网络错误必须打印，否则「无输出」会被误读成「无漏洞」。
# ============================================================================

set -uo pipefail

: "${UA:=Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36}"
: "${TIMEOUT:=12}"
: "${SLEEP:=1}"

_c_red=$'\033[31m'; _c_grn=$'\033[32m'; _c_yel=$'\033[33m'
_c_cya=$'\033[36m'; _c_dim=$'\033[2m'; _c_rst=$'\033[0m'; _c_bld=$'\033[1m'

log()  { printf '%s[*]%s %s\n' "$_c_cya" "$_c_rst" "$*"; }
ok()   { printf '%s[+]%s %s\n' "$_c_grn" "$_c_rst" "$*"; }
warn() { printf '%s[!]%s %s\n' "$_c_yel" "$_c_rst" "$*"; }
err()  { printf '%s[-]%s %s\n' "$_c_red" "$_c_rst" "$*" >&2; }
dim()  { printf '%s    %s%s\n' "$_c_dim" "$*" "$_c_rst"; }

# 分节标题
hdr()  { printf '\n%s== %s ==%s\n' "$_c_bld" "$*" "$_c_rst"; }
# 分隔线
rule() { printf '%s%s%s\n' "$_c_dim" "$(printf '%.0s-' $(seq 1 "${1:-60}"))" "$_c_rst"; }

need() {
  command -v "$1" >/dev/null 2>&1 || { err "缺少依赖: $1"; return 1; }
}

# ---- HTTP ----------------------------------------------------------------
# req METHOD URL [curl-args...]  → stdout: body
req() {
  local m="$1" u="$2"; shift 2
  curl -sk --max-time "$TIMEOUT" -A "$UA" -X "$m" "$u" "$@" 2>/dev/null
}

# reqc METHOD URL [curl-args...] → stdout: "<http_code>\t<body>"
reqc() {
  local m="$1" u="$2"; shift 2
  local out
  out=$(curl -sk --max-time "$TIMEOUT" -A "$UA" -X "$m" \
        -w $'\n%{http_code}' "$u" "$@" 2>/dev/null) || { printf '000\t'; return; }
  local code="${out##*$'\n'}"
  local body="${out%$'\n'*}"
  printf '%s\t%s' "$code" "$body"
}

# code_of METHOD URL [curl-args...] → 纯状态码
code_of() {
  local m="$1" u="$2"; shift 2
  curl -sk -o /dev/null -w '%{http_code}' --max-time "$TIMEOUT" -A "$UA" \
       -X "$m" "$u" "$@" 2>/dev/null || echo 000
}

# ---- 归一化 --------------------------------------------------------------
# 抹掉每次请求必然变化的字段，只留下「结构指纹」。
# 覆盖: uuid / 24+ 位十六进制 / 13 位毫秒时间戳 / 10 位秒时间戳 /
#       长数字串 / ISO 时间 / IP / 显式 elapsed 字段
norm() {
  sed -E \
    -e 's/[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}/<UUID>/g' \
    -e 's/\b[0-9a-fA-F]{24,}\b/<HEX>/g' \
    -e 's/\b16[0-9]{11}\b/<TS13>/g' \
    -e 's/\b17[0-9]{9}\b/<TS10>/g' \
    -e 's/"?(elapsed|cost|took|duration|rt|time)"?\s*[:=]\s*"?[0-9.]+"?/"\1":<N>/g' \
    -e 's/[0-9]{4}-[0-9]{2}-[0-9]{2}[T ][0-9]{2}:[0-9]{2}:[0-9]{2}[^"]*/<ISO>/g' \
    -e 's/\b([0-9]{1,3}\.){3}[0-9]{1,3}\b/<IP>/g' \
    -e 's/\b[0-9]{6,}\b/<NUM>/g' \
    | tr -d '\r'
}

# 结构指纹: 归一化后取前 N 字节，用于快速比对
fp() { norm | head -c "${1:-400}"; }

# ---- 基线 ----------------------------------------------------------------
# 用一个「几乎不可能存在」的随机路径建立基线。
# 这是所有软 404 / 恒定响应场景的判别根。
rand_path() { printf '/zzz_%s_%s' "$(date +%s%N | tail -c 7)" "$RANDOM"; }

# baseline METHOD URL [curl-args...] → 打印 "code<TAB>fp"
baseline() {
  local m="$1" u="$2"; shift 2
  local r; r=$(reqc "$m" "${u%/}$(rand_path)" "$@")
  local c="${r%%$'\t'*}"; local b="${r#*$'\t'}"
  printf '%s\t%s' "$c" "$(printf '%s' "$b" | fp)"
}

# 判断某响应是否与基线「结构相同」（视为被统一拦截 / 软 404）
same_as() {  # same_as <body> <baseline_fp>
  local got; got=$(printf '%s' "$1" | fp)
  [ "$got" = "$2" ]
}

# ---- 三态判读 ------------------------------------------------------------
# 通用三态：给出 code 与 body，返回分类标签
#   BLOCK   = 被 WAF/网关拦（403/406/412，或含拦截关键词）
#   SOFT404 = 软 404（200 但内容等于基线）
#   MISS    = 真 404/无路由
#   HIT     = 有业务响应（值得人工看）
classify3() {  # classify3 <code> <body> <baseline_fp>
  local c="$1" b="$2" base="${3:-}"
  case "$c" in
    000) echo "NETFAIL"; return ;;
    403|406|412|418|451) echo "BLOCK";  return ;;
    429) echo "RATELIMIT"; return ;;
    404|410) echo "MISS"; return ;;
    500|502|503|504) echo "ERROR"; return ;;
  esac
  if [ -n "$base" ] && same_as "$b" "$base"; then echo "SOFT404"; return; fi
  case "$b" in
    *"Access Denied"*|*"Request Rejected"*|*"安全拦截"*|*"系统维护中"*|*"noright"*|*"拦截"*)
      echo "BLOCK"; return ;;
  esac
  echo "HIT"
}

# ---- 依赖检查 ------------------------------------------------------------
preflight() {
  local missing=0
  for t in curl jq; do
    need "$t" || missing=1
  done
  [ "$missing" = 0 ] || { err "先装: pacman -S curl jq"; return 1; }
  return 0
}

# ---- 用法防护 ------------------------------------------------------------
usage_die() {
  printf '用法: %s\n' "$*" >&2
  exit 1
}
