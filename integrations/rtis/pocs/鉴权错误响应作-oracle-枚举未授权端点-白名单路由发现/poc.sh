#!/usr/bin/env bash
# 用法: ./authz_oracle_enum.sh https://api.example.com 60
# 原理: 若「不带鉴权头」时的响应内容在所有路径上恒定（同一错误码/文案），
#       而鉴权中间件前置于路由，则该恒定响应可作为「非白名单路径」的基线；
#       任何偏离基线的响应 = 该路径绕过了鉴权中间件（白名单端点）。

# ---- 参数检查 ----
if [ $# -lt 1 ]; then printf '用法: ./poc.sh <https://api.example.com> [SLEEP]\n' >&2; exit 1; fi
API="$1"; SLEEP="${2:-3}"

# 1) 建立基线：用一个几乎不可能存在的路径
BASE_PATH="/zzz_definitely_fake_$(date +%s)"
BASE=$(curl -sk --max-time 12 "$API$BASE_PATH")
echo "[baseline] $BASE_PATH -> $BASE"

# 2) 候选路径：按框架常见前缀 × 常见免鉴权业务名
NAMES="login visitor_login wx_login silent_login auto_login auth/login oauth/login \
wx/login login/code login/visitor get_token token/get register callback \
config/get_sign get_sign config/index config/init version/app online_date \
banner/list notice/list agreement privacy"
PREFIXES="/ /api/ /api/v1/ /v1/ /config/ /auth/ /open/ /public/"

for pre in $PREFIXES; do
  for n in $NAMES; do
    body=$(curl -sk --max-time 10 -X POST "$API$pre$n" \
           -H 'Content-Type: application/json' -d '{}' 2>/dev/null)
    [ -z "$body" ] && { echo "EMPTY  $pre$n"; continue; }
    # 与基线完全一致 -> 被中间件拦截，跳过
    [ "$body" = "$BASE" ] && continue
    # 骨架相同（仅 rid/时间戳等变化）也视为拦截
    if [ "$(echo "$body" | sed -E 's/[0-9a-f]{8}-[0-9a-f-]{20,}//g; s/[0-9]{6,}//g')" = \
         "$(echo "$BASE" | sed -E 's/[0-9a-f]{8}-[0-9a-f-]{20,}//g; s/[0-9]{6,}//g')" ]; then
      continue
    fi
    printf 'OPEN   %-40s %s\n' "$pre$n" "$(echo "$body" | head -c 200)"
    sleep "$SLEEP"
  done
done

# 3) 每个 OPEN 路径再做方法爆破（GET/POST/PUT）—— 返回 404 page not found
#    说明「路由存在但方法不对」，与鉴权错误不同，同样是白名单证据。
#
# 关键判据小结：
#   - 与基线完全一致            -> 被鉴权中间件拦截（不是白名单）
#   - "404 page not found"      -> 路由未注册但未被鉴权拦（前缀在鉴权组外）
#   - 任何业务 JSON             -> ★ 白名单端点，进入下一步测参数
#
# 注意：若基线本身随路径变化（有的框架会先路由再鉴权），此法失效，
#       改用「带假 token 与不带 token 的响应差异」作 oracle。
