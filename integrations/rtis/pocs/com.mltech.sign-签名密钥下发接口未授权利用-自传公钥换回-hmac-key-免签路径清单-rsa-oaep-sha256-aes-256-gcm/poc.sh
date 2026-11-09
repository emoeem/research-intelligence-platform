#!/usr/bin/env bash
# com.mltech.sign-签名密钥下发接口未授权利用-自传公钥换回-hmac-key-免签路径 — 参数守卫 wrapper
# 真正的实现在 yidui_signkey.py。这个 wrapper 存在的唯一原因：
#   yidui_signkey.py 用 sys.argv[n] if len>n else '<硬编码默认值>' 取参，
#   无参数时会直接对着默认目标发包。守卫必须在 shell 层。
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PY="$DIR/yidui_signkey.py"

if [ $# -lt 1 ]; then
  cat >&2 <<'EOF'
用法: ./poc.sh <TARGET_URL>

  TARGET_URL  目标 base，如 https://TARGET
  脚本会自动替换内部硬编码的默认域名（原来无参数会直接打到真实站点）

作用: 向 sign 接口自传 RSA 公钥，换取服务端下发的 HMAC key；
      再枚举免签路径清单。全部为读操作。
EOF
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  printf '[-] 需要 python3\n' >&2; exit 1
fi
[ -f "$PY" ] || { printf '[-] 缺少实现文件: %s\n' "$PY" >&2; exit 1; }

exec python3 "$PY" "$@"
