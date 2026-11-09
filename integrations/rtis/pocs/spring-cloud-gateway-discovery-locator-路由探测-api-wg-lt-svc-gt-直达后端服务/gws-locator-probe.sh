#!/bin/bash
# Spring Cloud Gateway discovery-locator 路由探测
# 前提：能从 /actuator/env 看到 spring.cloud.gateway.discovery.locator.enabled 键（值可能被掩码，键名可见）
# 用法: ./gws-locator-probe.sh https://TARGET_BASE/api svc1,svc2,svc3

# ---- 参数检查 ----
if [ $# -lt 1 ]; then printf '用法: ./gws-locator-probe.sh <BASE_URL>   e.g. https://host/api\n' >&2; exit 1; fi
BASE="${1:?base url, e.g. https://host/api}"
SVCS="${2:-}"

echo "== 1) 确认 locator 配置键是否存在（键名可见即可，值会被 show-values=never 掩码）=="
curl -s "$BASE/actuator/env" | python3 -c '
import sys,json
try: d=json.load(sys.stdin)
except Exception: sys.exit(0)
for ps in d.get("propertySources",[]):
    for k in ps.get("properties",{}):
        if "locator" in k.lower(): print("  HIT", k, "@", ps["name"])
'

echo "== 2) 用 /error 判断 locator 是否真的生效 =="
for s in ${SVCS//,/ }; do
  printf '  %-14s ' "$s"
  # 不存在的模块前缀 vs locator 前缀，响应体格式不同即证明穿透
  curl -s -m 10 -o /tmp/gwl.$$ -w "http=%{http_code} sz=%{size_download} | " "$BASE/$s/error" -H "X-Login-Type: 3"
  head -c 90 /tmp/gwl.$$ | tr -d '\n'; echo
done
rm -f /tmp/gwl.$$

# 判定要点：
#   locator 生效  -> 返回服务自己的 Spring 错误体，形如 {"timestamp":"...","status":999,"error":"None"}
#                    或服务侧 sa-token 体 {"code": 401, "msg": "未能读取到有效 token", "data": null}（text/plain，注意空格）
#   locator 未生效 -> 返回网关/前置 nginx 的 SPA index.html（固定字节数，如 2144B）或网关自己的 404/403
#
# 注意 nginx 常会把 /api 前缀剥掉再转给网关；目录前缀必须与实际一致，否则永远拿不到服务响应。
#
# 3) locator 生效后，逐个服务试未授权面（这些往往是白名单漏掉的）：
#   /$SVC/actuator, /$SVC/actuator/env, /$SVC/actuator/nacosconfig, /$SVC/actuator/heapdump
#   /$SVC/v3/api-docs, /$SVC/swagger-ui/index.html, /$SVC/doc.html
#   /$SVC/druid/index.html, /$SVC/druid/sql.html   （Druid 控制台在则等同数据库权限）
#   /$SVC/error, /$SVC/health, /$SVC/favicon.ico   （通常 permitAll，用于确认链路）
