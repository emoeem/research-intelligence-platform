#!/usr/bin/env bash
# 参数守卫 wrapper。实现见 anon_oracle_enum.py。
# 为什么需要: anon_oracle_enum.py 用 sys.argv[n] 直接取参，无参数时抛 IndexError
# （用户看到的是 Traceback，不是用法），或对着硬编码默认目标发包。
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PY="$DIR/anon_oracle_enum.py"

if [ $# -lt 4 ]; then
  cat >&2 <<'EOF'
用法: ./poc.sh <domain> <pid> <service> <dict_file> [prefix]

  domain     目标域名
  pid        产品 id（从网关路径取）
  service    服务名
  dict_file  路由字典，每行一个
  prefix     默认 anon

判据: 统一响应体三值差分（存在/不存在/需认证），不靠状态码。
EOF
  exit 1
fi

command -v python3 >/dev/null 2>&1 || { printf '[-] 需要 python3\n' >&2; exit 1; }
[ -f "$PY" ] || { printf '[-] 缺少实现文件: %s\n' "$PY" >&2; exit 1; }

exec python3 "$PY" "$@"
