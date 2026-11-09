#!/usr/bin/env bash
# ============================================================================
# 微信小程序接口面：静态全集 + 四态认证矩阵
#
# 小程序和 Web 的差别在于：接口全集是**能从包里静态挖全**的。
# wxapkg 解开后，app-service.js 里的 request({url: ...}) 调用就是全量 API 面，
# 不需要爬、不需要猜。
#
# 四态认证矩阵（每个接口都要过一遍，少一态结论就不可靠）：
#   S1 无任何凭证        → 200 说明无鉴权；401 说明有拦截
#   S2 过期/伪造 token   → 200 说明只存在性校验（更危险）；401 说明校验签名/有效期
#   S3 低权 token        → 能读到别人数据 = 水平越权
#   S4 无 token + 伪造 Referer/X-Client   → 说明鉴权依赖可伪造的头
#
# 判据：任一态与「随机路径基线」相同 → 该接口不存在（小程序路由兜底很常见）
#
# 用法: ./poc.sh <BASE_URL> <接口清单> [低权token] [--referer 域名]
# ============================================================================
set -uo pipefail
source "$(dirname "$0")/../_lib/common.sh"

if [ $# -lt 2 ]; then
  cat >&2 <<'U'
用法: ./poc.sh <BASE_URL> <接口文件> [低权token] [--referer <小程序域名>]
  BASE_URL   小程序 API 根，如 https://api.example.com
  接口文件   每行一个路径（从 app-service.js 挖出来的）
  低权token  可选，S3 用
  --referer  可选，S4 用；默认 servicewechat.com
示例: ./poc.sh https://api.x.com apis.txt "eyJhbGc..." --referer servicewechat.com
判据: 四态逐一比对，任一态拿到数据即记录；与随机基线相同则判接口不存在
U
  exit 1
fi

BASE="${1%/}"; IFILE="$2"; shift 2
TOK=""; REF="servicewechat.com"
while [ $# -gt 0 ]; do
  case "$1" in
    --referer) REF="${2:-servicewechat.com}"; shift 2 ;;
    *) TOK="$1"; shift ;;
  esac
done
[ -f "$IFILE" ] || { warn "接口文件不存在: $IFILE"; exit 1; }
need curl || exit 1
mapfile -t EPS < <(grep -v '^\s*$' "$IFILE" | sed 's/^\///' | sort -u)
[ ${#EPS[@]} -eq 0 ] && { warn "接口清单为空"; exit 1; }
dim "接口 ${#EPS[@]} 条"

hdr "0. 基线"
RND=$(rand_path)
R=$(reqc GET "$BASE$RND"); RND_C="${R%%$'\t'*}"; RND_FP=$(printf '%s' "${R#*$'\t'}" | norm)
dim "随机路径 → $RND_C"

# 四态定义
declare -A ST_DESC=(
  [S1]="无凭证"
  [S2]="伪造token"
  [S3]="低权token"
  [S4]="伪造Referer"
)
FAKE_TOK="eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJ1aWQiOjEsImV4cCI6OTk5OTk5OTk5OX0.$(printf 'A%.0s' $(seq 1 43))"

call() {  # call <state> <path> → "code\tbcode\tlen"
  local st="$1" p="$2"
  local args=()
  case "$st" in
    S1) ;;
    S2) args=(-H "Authorization: Bearer $FAKE_TOK" -H "token: $FAKE_TOK") ;;
    S3) [ -n "$TOK" ] && args=(-H "Authorization: Bearer $TOK" -H "token: $TOK") || { echo "-1	-	0"; return; } ;;
    S4) args=(-H "Referer: https://$REF/" -H "X-Requested-With: XMLHttpRequest") ;;
  esac
  local r c b bc
  r=$(reqc GET "$BASE$p" "${args[@]}")
  c="${r%%$'\t'*}"; b="${r#*$'\t'}"
  bc=$(printf '%s' "$b" | grep -oE '"code"\s*:\s*-?[0-9]+' | head -1 | grep -oE '\-?[0-9]+')
  printf '%s\t%s\t%s' "$c" "${bc:--}" "${#b}"
}

hdr "1. 四态矩阵"
: > /tmp/rt_mp_summary.txt
for ep in "${EPS[@]}"; do
  line="$ep"
  any=0
  for st in S1 S2 S3 S4; do
    [ "$st" = "S3" ] && [ -z "$TOK" ] && { line="$line	-"; continue; }
    out=$(call "$st" "$ep")
    c=$(printf '%s' "$out" | cut -f1)
    bc=$(printf '%s' "$out" | cut -f2)
    ln=$(printf '%s' "$out" | cut -f3)
    # 与随机基线同状态码 + 同长度 视为不存在（小程序路由兜底多为固定 200+固定 body）
    if [ "$c" = "$RND_C" ] && [ "$ln" -lt 400 ] 2>/dev/null; then
      line="$line	DED"; continue
    fi
    if [ "$c" = "200" ] && [ "$bc" != "-" ] && [ "$bc" != "401" ] && [ "$bc" != "403" ]; then
      line="$line	OPEN"; any=1
    elif [ "$c" = "200" ] && [ "$bc" = "-" ]; then
      line="$line	200?"; any=1
    else
      line="$line	$c"
    fi
    sleep 0.3
  done
  printf '%s\n' "$line" >> /tmp/rt_mp_summary.txt
  [ "$any" -eq 1 ] && warn "$line"
done

hdr "2. 汇总"
column -t -s $'\t' /tmp/rt_mp_summary.txt 2>/dev/null | head -40 || head -40 /tmp/rt_mp_summary.txt
dim "完整矩阵: /tmp/rt_mp_summary.txt"

hdr "3. 判读"
cat >&2 <<'U'
列定义: 接口 | S1无凭证 | S2伪造token | S3低权token | S4伪造Referer
  DED  = 与随机基线相同（接口不存在 / 路由兜底）
  OPEN = 200 且业务码不是 401/403 → 数据面可访问
  200? = 200 但无 code 字段（老式返回，需人工看 body）

高价值组合（按严重度）：
  S1=OPEN                     → 完全无鉴权，最高
  S2=OPEN 而 S1=401           → 只校验 token 存在性，不验签名。伪造即可用
  S3=OPEN 且返回数据含他人 ID → 水平越权
  S4=OPEN 而 S1=401           → 鉴权依赖 Referer（可伪造），且不验签名

必须补的一步：S2 的伪造 token 一定会被真服务端拒（JWT 有签名）。
若 S2 返回 200，先确认是不是「服务端忽略 token 解析失败、当匿名处理」——
用 S1 的响应比对：S1 和 S2 完全一致 = 确实是匿名放行，不是伪造成功。
U

verdict "miniprogram-api-matrix" "DONE" "${#EPS[@]} 接口 × 4 态"
