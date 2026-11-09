#!/usr/bin/env bash
# 包网-app_config.txt-离线解密-aes-256-cbc-sha256-硬编码常量-iv — 参数守卫 wrapper
# 真正的实现在 appconfig_decrypt.py。这个 wrapper 存在的唯一原因：
#   appconfig_decrypt.py 用 sys.argv[n] if len>n else '<硬编码默认值>' 取参，
#   无参数时会直接对着默认目标发包。守卫必须在 shell 层。
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PY="$DIR/appconfig_decrypt.py"

if [ $# -lt 1 ]; then
  cat >&2 <<'EOF'
用法: ./poc.sh <file|url> [--key K] [--raw]

  file   本地 app_config.txt（或给 URL，脚本会先抓下来）
  --key  覆盖默认 key
  --raw  只输出解密后原文

纯离线（给 URL 时只有一次 GET）。
EOF
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  printf '[-] 需要 python3\n' >&2; exit 1
fi
[ -f "$PY" ] || { printf '[-] 缺少实现文件: %s\n' "$PY" >&2; exit 1; }

exec python3 "$PY" "$@"
