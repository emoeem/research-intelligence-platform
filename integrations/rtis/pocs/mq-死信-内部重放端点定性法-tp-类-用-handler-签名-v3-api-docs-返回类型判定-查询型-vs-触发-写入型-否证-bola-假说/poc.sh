#!/usr/bin/env bash
# 参数守卫 wrapper。实现见 inner_endpoint_triage.py。
# 为什么需要: inner_endpoint_triage.py 用 sys.argv[n] 直接取参，无参数时抛 IndexError
# （用户看到的是 Traceback，不是用法），或对着硬编码默认目标发包。
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PY="$DIR/inner_endpoint_triage.py"

if [ $# -lt 3 ]; then
  cat >&2 <<'EOF'
用法: ./poc.sh <map.json> <docs.json> <want_keyword>

  map.json   路由→handler 映射（从 bundle 或 v3/api-docs 导出）
  docs.json  api-docs 原始 JSON
  want       要定性的事件名/路径关键词

只做静态判定（查询型 vs 触发写入型），不发请求。
EOF
  exit 1
fi

command -v python3 >/dev/null 2>&1 || { printf '[-] 需要 python3\n' >&2; exit 1; }
[ -f "$PY" ] || { printf '[-] 缺少实现文件: %s\n' "$PY" >&2; exit 1; }

exec python3 "$PY" "$@"
