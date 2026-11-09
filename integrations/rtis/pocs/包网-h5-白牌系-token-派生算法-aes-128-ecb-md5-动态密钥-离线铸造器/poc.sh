#!/usr/bin/env bash
# 包网-h5-白牌系-token-派生算法-aes-128-ecb-md5-动态密钥-离线铸造器 — 参数守卫 wrapper
# 真正的实现在 mint.py。这个 wrapper 存在的唯一原因：
#   mint.py 用 sys.argv[n] if len>n else '<硬编码默认值>' 取参，
#   无参数时会直接对着默认目标发包。守卫必须在 shell 层。
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PY="$DIR/mint.py"

if [ $# -lt 1 ]; then
  cat >&2 <<'EOF'
用法: ./poc.sh <bare32hex> [alias]

  bare32hex  不带连字符的 32 位十六进制设备号/指纹
  alias      可选，默认取脚本内的常量

输出: 由该指纹派生的会员 token。纯离线，不发请求。
EOF
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  printf '[-] 需要 python3\n' >&2; exit 1
fi
[ -f "$PY" ] || { printf '[-] 缺少实现文件: %s\n' "$PY" >&2; exit 1; }

exec python3 "$PY" "$@"
