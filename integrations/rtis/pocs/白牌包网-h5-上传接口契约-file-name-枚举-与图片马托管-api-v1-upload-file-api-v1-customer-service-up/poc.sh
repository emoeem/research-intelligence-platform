#!/usr/bin/env bash
# 白牌包网-h5-上传接口契约-file-name-枚举-与图片马托管-api-v1-upload-f — 参数守卫 wrapper
# 真正的实现在 poc.py。这个 wrapper 存在的唯一原因：
#   poc.py 用 sys.argv[n] if len>n else '<硬编码默认值>' 取参，
#   无参数时会直接对着默认目标发包。守卫必须在 shell 层。
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PY="$DIR/poc.py"

if [ $# -lt 4 ]; then
  cat >&2 <<'EOF'
用法: ./poc.sh <base_url> <token> <timestamp> <customerUID> [filename]

  base_url      https://TARGET
  token         自有账号 token
  timestamp     13 或 16 位时间戳
  customerUID   自己的 uid
  filename      可选，默认用内置候选

注意: 会真实上传文件到目标。只在授权环境跑。
EOF
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  printf '[-] 需要 python3\n' >&2; exit 1
fi
[ -f "$PY" ] || { printf '[-] 缺少实现文件: %s\n' "$PY" >&2; exit 1; }

exec python3 "$PY" "$@"
