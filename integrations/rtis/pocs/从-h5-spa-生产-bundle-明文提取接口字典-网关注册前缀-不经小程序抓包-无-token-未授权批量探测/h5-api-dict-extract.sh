#!/usr/bin/env bash
# h5-api-dict-extract.sh —— 不抓包、不 root、不用小程序：从 Web H5/SPA bundle 明文提取接口字典
# 用法: ./h5-api-dict-extract.sh https://www.example.com/
set -u
# ---- 参数检查 ----
if [ $# -lt 1 ]; then printf '用法: ./h5-api-dict-extract.sh <https://TARGET>\n' >&2; exit 1; fi
BASE="${1:?usage: $0 https://site/}"
UA='Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/126.0 Safari/537.36'
OUT="h5dict_$(echo "$BASE" | sed 's#https\?://##; s#[/:]#_#g')"; mkdir -p "$OUT"

curl -sk -m 20 -A "$UA" "$BASE" -o "$OUT/index.html"
# 1) 列出所有 js
grep -oE '(src|href)="[^"]+\.js[^"]*"' "$OUT/index.html" | sed -E 's/.*="([^"]+)".*/\1/' | sort -u > "$OUT/js.list"
# 2) 下载 + 找 sourcemap（没有 map 也能继续，明文常量同样够用）
while read -r js; do
  case "$js" in //*) js="https:$js";; ./*) js="${BASE%/}/${js#./}";; /*) js="$(echo "$BASE" | cut -d/ -f1-3)$js";; esac
  f="$OUT/$(basename "${js%%\?*}")"
  curl -sk -m 30 -A "$UA" "$js" -o "$f" 2>/dev/null
  sm=$(tail -c 400 "$f" | grep -oE 'sourceMappingURL=[^ */]+' | head -1 | cut -d= -f2)
  [ -n "${sm:-}" ] && echo "MAP $js -> $sm"
done < "$OUT/js.list"

# 3) 三条通用提取规则（覆盖 AddressUrl+/baseURL+/纯字符串 三种写法）
cat "$OUT"/*.js 2>/dev/null > "$OUT/all.js"
grep -oE '(AddressUrl|baseURL|apiUrl|API_URL|\$http\.(get|post))\s*[+(]\s*["'"'"'][a-zA-Z0-9_./-]{3,80}' "$OUT/all.js" \
  | grep -oE '["'"'"'][a-zA-Z0-9_./-]{3,80}$' | tr -d '"'"'" | sort -u > "$OUT/api.txt"
grep -oE '["'"'"']/(api|app|v[0-9]|user|member|admin|manage|order|pay|auth|sms|upload|file|log)/[a-zA-Z0-9_/-]{2,60}["'"'"']' "$OUT/all.js" \
  | tr -d '"'"'" | sort -u >> "$OUT/api.txt"
sort -u "$OUT/api.txt" -o "$OUT/api.txt"
echo "[+] 接口候选 $(wc -l < "$OUT/api.txt") 条 → $OUT/api.txt"

# 4) 找网关注册前缀（SPA 常把前缀单独写成字符串）
grep -oE '"/(api|h5|app|gateway|[a-z]{2,12}api|[a-z]{4,12}[0-9]{2})/"' "$OUT/all.js" | tr -d '"' | sort -u

# 5) 无 token 复核：先 GET，再按前端真实方法 POST 表单（只读接口，1 条即停）
while read -r p; do
  printf '%-46s ' "$p"
  curl -sk -m 12 -A "$UA" -o /tmp/p1 -w 'GET %{http_code}/%{size_download} ' "$BASE$p"
  curl -sk -m 12 -A "$UA" -X POST -H 'Content-Type: application/x-www-form-urlencoded' --data 'pageNum=1&pageSize=2' \
       -o /tmp/p2 -w '| POST %{http_code}/%{size_download} ' "$BASE$p"
  echo "| $(head -c 90 /tmp/p2 | tr -d '\n')"
  sleep 1
done < "$OUT/api.txt"
