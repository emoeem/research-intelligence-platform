#!/usr/bin/env bash
# 代理后台-bundle-硬编码-aes-256-gcm-密钥-匿名解密全品牌-ag-hall-远程配 — 参数守卫 wrapper
# 真正的实现在 decode_config.py。这个 wrapper 存在的唯一原因：
#   decode_config.py 用 sys.argv[n] if len>n else '<硬编码默认值>' 取参，
#   无参数时会直接对着默认目标发包。守卫必须在 shell 层。
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PY="$DIR/decode_config.py"

if [ $# -lt 3 ]; then
  cat >&2 <<'EOF'
用法: ./poc.sh <mode> <config_file> <key>

  mode         ag | hall
  config_file  已下载到本地的 config.json
  key          bundle 里抠出的 AES-256-GCM 密钥（base64）

纯离线解密，不联网。
EOF
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  printf '[-] 需要 python3\n' >&2; exit 1
fi
[ -f "$PY" ] || { printf '[-] 缺少实现文件: %s\n' "$PY" >&2; exit 1; }

exec python3 "$PY" "$@"
