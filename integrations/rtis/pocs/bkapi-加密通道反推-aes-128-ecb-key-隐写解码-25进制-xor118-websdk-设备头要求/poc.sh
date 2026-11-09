#!/usr/bin/env bash
# 参数守卫 wrapper。实现见 bkapi_aes_client.py。
# 为什么需要: bkapi_aes_client.py 用 sys.argv[n] 直接取参，无参数时抛 IndexError
# （用户看到的是 Traceback，不是用法），或对着硬编码默认目标发包。
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PY="$DIR/bkapi_aes_client.py"

if [ $# -lt 1 ]; then
  cat >&2 <<'EOF'
用法: ./poc.sh <HOST_URL> [--proxy http://127.0.0.1:PORT]

  HOST_URL  https://TARGET_HOST

注意: 该接口要求 WebSDK 生成的 4 个设备头（device_fingerprint /
      fp_version / sdk_version / sid），缺则统一返回 安全性错误DD! (2001)。
EOF
  exit 1
fi

command -v python3 >/dev/null 2>&1 || { printf '[-] 需要 python3\n' >&2; exit 1; }
[ -f "$PY" ] || { printf '[-] 缺少实现文件: %s\n' "$PY" >&2; exit 1; }

exec python3 "$PY" "$@"
