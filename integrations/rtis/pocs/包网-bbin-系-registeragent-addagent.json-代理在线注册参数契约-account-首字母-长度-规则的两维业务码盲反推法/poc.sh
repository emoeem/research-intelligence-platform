#!/usr/bin/env bash
# 参数守卫 wrapper。实现见 bbin_registeragent_account_rule.py。
# 为什么需要: bbin_registeragent_account_rule.py 用 sys.argv[n] 直接取参，无参数时抛 IndexError
# （用户看到的是 Traceback，不是用法），或对着硬编码默认目标发包。
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PY="$DIR/bbin_registeragent_account_rule.py"

if [ $# -lt 1 ]; then
  cat >&2 <<'EOF'
用法: ./poc.sh <IP:PORT> <HOST:PORT> <account> [password]

  IP:PORT    直连地址
  HOST:PORT  Host 头（vhost 路由）
  account    待测代理账号

注意: 这个接口会**真的注册代理账号**。只在授权环境跑，且用一次性测试账号。
业务码语义: 111032531=账号已存在 / 111032519=进入密码层（都是格式判定通过的正信号）。
EOF
  exit 1
fi

command -v python3 >/dev/null 2>&1 || { printf '[-] 需要 python3\n' >&2; exit 1; }
[ -f "$PY" ] || { printf '[-] 缺少实现文件: %s\n' "$PY" >&2; exit 1; }

exec python3 "$PY" "$@"
