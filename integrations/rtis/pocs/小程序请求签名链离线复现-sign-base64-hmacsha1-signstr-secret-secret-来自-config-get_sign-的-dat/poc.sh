#!/usr/bin/env bash
# 小程序请求签名链离线复现-sign-base64-hmacsha1-signstr-secret-s — 参数守卫 wrapper
# 真正的实现在 poc.py。这个 wrapper 存在的唯一原因：
#   poc.py 用 sys.argv[n] if len>n else '<硬编码默认值>' 取参，
#   无参数时会直接对着默认目标发包。守卫必须在 shell 层。
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PY="$DIR/poc.py"

if [ $# -lt 1 ]; then
  cat >&2 <<'EOF'
用法: ./poc.sh sign   <secret> <params...>
       ./poc.sh verify <secret> <sign> <params...>

  params 为 k=v 序列，顺序参与签名

纯离线复刻，不发请求。
EOF
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  printf '[-] 需要 python3\n' >&2; exit 1
fi
[ -f "$PY" ] || { printf '[-] 缺少实现文件: %s\n' "$PY" >&2; exit 1; }

exec python3 "$PY" "$@"
