#!/usr/bin/env bash
# 参数守卫 wrapper。实现见 miwen_secret.py。
# 为什么需要: miwen_secret.py 用 sys.argv[n] 直接取参，无参数时抛 IndexError
# （用户看到的是 Traceback，不是用法），或对着硬编码默认目标发包。
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PY="$DIR/miwen_secret.py"

if [ $# -lt 1 ]; then
  cat >&2 <<'EOF'
用法: ./poc.sh <file> [--miwen <密文>]

  file      含 miwen 字段的 JS / 配置文件
  --miwen   直接给密文，跳过从文件提取

纯离线派生 secret。
EOF
  exit 1
fi

command -v python3 >/dev/null 2>&1 || { printf '[-] 需要 python3\n' >&2; exit 1; }
[ -f "$PY" ] || { printf '[-] 缺少实现文件: %s\n' "$PY" >&2; exit 1; }

exec python3 "$PY" "$@"
