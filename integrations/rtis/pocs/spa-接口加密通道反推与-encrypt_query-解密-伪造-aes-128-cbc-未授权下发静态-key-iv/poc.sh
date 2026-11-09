#!/usr/bin/env bash
# spa-接口加密通道反推与-encrypt_query-解密-伪造-aes-128-cbc-未授权下 — 参数守卫 wrapper
# 真正的实现在 encrypt_query_forge.py。这个 wrapper 存在的唯一原因：
#   encrypt_query_forge.py 用 sys.argv[n] if len>n else '<硬编码默认值>' 取参，
#   无参数时会直接对着默认目标发包。守卫必须在 shell 层。
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PY="$DIR/encrypt_query_forge.py"

if [ $# -lt 1 ]; then
  cat >&2 <<'EOF'
用法: ./poc.sh [明文JSON]        默认 {"projectId":89919}
       ./poc.sh -d <密文>         反向解密

纯离线：只做 AES-128-CBC 的加/解密复刻，不发任何网络请求。
EOF
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  printf '[-] 需要 python3\n' >&2; exit 1
fi
[ -f "$PY" ] || { printf '[-] 缺少实现文件: %s\n' "$PY" >&2; exit 1; }

exec python3 "$PY" "$@"
