#!/usr/bin/env bash
# php-api-控制器-方法三态枚举-oracle-516-517-9-筛匿名接口 — 参数守卫 wrapper
# 真正的实现在 enum_api_oracle.py。这个 wrapper 存在的唯一原因：
#   enum_api_oracle.py 用 sys.argv[n] if len>n else '<硬编码默认值>' 取参，
#   无参数时会直接对着默认目标发包。守卫必须在 shell 层。
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PY="$DIR/enum_api_oracle.py"

if [ $# -lt 1 ]; then
  cat >&2 <<'EOF'
用法: ./poc.sh <BASE_URL> [更多参数见 NOTES.md]

  BASE_URL  目标 base（脚本内不再使用任何硬编码域名）

判据: 控制器/方法组合的 516 / 517 / 9 三态差分，筛出可匿名访问的接口
EOF
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  printf '[-] 需要 python3\n' >&2; exit 1
fi
[ -f "$PY" ] || { printf '[-] 缺少实现文件: %s\n' "$PY" >&2; exit 1; }

exec python3 "$PY" "$@"
