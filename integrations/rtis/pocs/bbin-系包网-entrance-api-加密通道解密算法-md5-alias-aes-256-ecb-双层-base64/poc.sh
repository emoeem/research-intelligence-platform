#!/usr/bin/env bash
# 参数守卫 wrapper。实现见 poc.py。
# 为什么需要: poc.py 用 sys.argv[n] 直接取参，无参数时抛 IndexError
# （用户看到的是 Traceback，不是用法），或对着硬编码默认目标发包。
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PY="$DIR/poc.py"

if [ $# -lt 2 ]; then
  cat >&2 <<'EOF'
用法: ./poc.sh <BASE_URL> <ALIAS> [HOST_HDR]

  BASE_URL  https://IP:PORT 或 https://HOST:PORT
  ALIAS     window.alias 原文（脚本内部做 MD5 派生 AES-256 key）
  HOST_HDR  可选，形如 TARGET_HOST:PORT

算法: key=ASCII(MD5(alias))，AES-256-ECB/PKCS7，密文双层 base64。
      纯本地加解密，只有显式传 --send 才会发包。
EOF
  exit 1
fi

command -v python3 >/dev/null 2>&1 || { printf '[-] 需要 python3\n' >&2; exit 1; }
[ -f "$PY" ] || { printf '[-] 缺少实现文件: %s\n' "$PY" >&2; exit 1; }

exec python3 "$PY" "$@"
