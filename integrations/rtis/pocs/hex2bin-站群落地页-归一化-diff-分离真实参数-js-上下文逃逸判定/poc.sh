#!/usr/bin/env bash
# hex2bin-站群落地页-归一化-diff-分离真实参数-js-上下文逃逸判定 — 参数守卫 wrapper
# 真正的实现在 hex2bin_param_diff.py。这个 wrapper 存在的唯一原因：
#   hex2bin_param_diff.py 用 sys.argv[n] if len>n else '<硬编码默认值>' 取参，
#   无参数时会直接对着默认目标发包。守卫必须在 shell 层。
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PY="$DIR/hex2bin_param_diff.py"

if [ $# -lt 1 ]; then
  cat >&2 <<'EOF'
用法: ./poc.sh <URL>

  URL  返回 document.write(hex2bin('<hex>')) 的落地页

注意: 脚本默认走本地代理 127.0.0.1:7892，没有代理时请改 PROXY 变量
      或先确认目标可直连。判定只看 Location 与 JS 数据流，不做浏览器执行。
EOF
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  printf '[-] 需要 python3\n' >&2; exit 1
fi
[ -f "$PY" ] || { printf '[-] 缺少实现文件: %s\n' "$PY" >&2; exit 1; }

exec python3 "$PY" "$@"
