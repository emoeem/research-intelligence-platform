#!/usr/bin/env bash
# 参数守卫 wrapper。实现见 openredir_landing_survey.py。
# 为什么需要: openredir_landing_survey.py 用 sys.argv[n] 直接取参，无参数时抛 IndexError
# （用户看到的是 Traceback，不是用法），或对着硬编码默认目标发包。
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PY="$DIR/openredir_landing_survey.py"

if [ $# -lt 1 ]; then
  cat >&2 <<'EOF'
用法: ./poc.sh <URL>

  URL  待普查的落地页

判据: 唯一标记法 —— 注入无点域名，只认 Location 头与 JS 数据流，
      不认页面里出现的字符串（假阳性极高）。
EOF
  exit 1
fi

command -v python3 >/dev/null 2>&1 || { printf '[-] 需要 python3\n' >&2; exit 1; }
[ -f "$PY" ] || { printf '[-] 缺少实现文件: %s\n' "$PY" >&2; exit 1; }

exec python3 "$PY" "$@"
