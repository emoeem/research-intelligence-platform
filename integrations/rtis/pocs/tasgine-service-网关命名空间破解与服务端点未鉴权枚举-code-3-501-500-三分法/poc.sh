#!/usr/bin/env bash
# 参数守卫 wrapper。实现见 tasgine_enum.py。
# 为什么需要: tasgine_enum.py 用 sys.argv[n] 直接取参，无参数时抛 IndexError
# （用户看到的是 Traceback，不是用法），或对着硬编码默认目标发包。
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PY="$DIR/tasgine_enum.py"

if [ $# -lt 1 ]; then
  cat >&2 <<'EOF'
用法: ./poc.sh <base_url> [routes_file] [delay_sec]

  base_url     https://TARGET
  routes_file  每行一个路由；不给则用内置清单
  delay_sec    默认 3

判据: code 3 / 501 / 500 三分法区分「不存在 / 需鉴权 / 未鉴权可达」。
EOF
  exit 1
fi

command -v python3 >/dev/null 2>&1 || { printf '[-] 需要 python3\n' >&2; exit 1; }
[ -f "$PY" ] || { printf '[-] 缺少实现文件: %s\n' "$PY" >&2; exit 1; }

exec python3 "$PY" "$@"
