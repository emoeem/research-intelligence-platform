#!/usr/bin/env bash
# 前端硬编码 AES 密钥定位器 — wrapper
# 真正的逻辑在 aes-hunter.py（需要 cryptography 库，bash 做不了分组密码）。
# 这里只做参数守卫 + 依赖检查，让「无参数」有一个能看懂的用法输出。
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [ $# -lt 1 ]; then
  cat >&2 <<'EOF'
用法:
  ./poc.sh --url https://TARGET/                  # 抓首页引用的 js
  ./poc.sh --dir ./chunks                         # 本地已下载 chunk 目录
  ./poc.sh --file app.js --sample <base64密文>     # 手动指定

  --sample B64       可重复；不给则从 js 自动挑长 base64（误报高，命中标 [AUTO]）
  --max-candidates N 默认 400

判据: 解密后必须 PKCS7 padding 合法 + 明文可打印 ≥92% + 命中 JSON/查询串/中文，
      三者同时满足才算命中。只看「不抛异常」会把随机数据当明文。
EOF
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  printf '[-] 需要 python3\n' >&2; exit 1
fi
if ! python3 -c 'import cryptography' 2>/dev/null; then
  printf '[-] 缺少 cryptography: pip install --user cryptography\n' >&2; exit 1
fi

exec python3 "$DIR/aes-hunter.py" "$@"
