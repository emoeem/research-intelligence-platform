#!/usr/bin/env bash
# 参数守卫 wrapper。实现见 poc.py。
# 为什么需要: poc.py 用 sys.argv[n] 直接取参，无参数时抛 IndexError
# （用户看到的是 Traceback，不是用法），或对着硬编码默认目标发包。
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PY="$DIR/poc.py"

if [ $# -lt 1 ]; then
  cat >&2 <<'EOF'
用法: ./poc.sh <host-ip.tsv>

  host-ip.tsv  两列，每行 `host<TAB>ip`

判据: 每台 host 在 :443(SNI) 与 :80(Host) 各一次，只记应用层事实。
      TUN 环境下绝不信端口扫描，只看 body/状态码/异常类型。
EOF
  exit 1
fi

command -v python3 >/dev/null 2>&1 || { printf '[-] 需要 python3\n' >&2; exit 1; }
[ -f "$PY" ] || { printf '[-] 缺少实现文件: %s\n' "$PY" >&2; exit 1; }

exec python3 "$PY" "$@"
