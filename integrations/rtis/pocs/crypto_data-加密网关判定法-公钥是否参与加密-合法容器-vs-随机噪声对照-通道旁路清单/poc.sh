#!/usr/bin/env bash
# 加密网关形态判定器 — wrapper
# 逻辑在 crypto-gateway-probe.py。默认零请求，只有显式给 --config 才发一次 GET。
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [ $# -lt 1 ]; then
  cat >&2 <<'EOF'
用法:
  ./poc.sh --samples samples.txt                  # 判定密文形态（纯离线）
  ./poc.sh --samples s.txt --plain p.txt          # 有明文对，判定更准
  ./poc.sh --pubkey 'MIIBIjANBgkq...'             # 判下发的是公钥还是私钥
  ./poc.sh --config 'https://TARGET/config?appid=X'  # 抓配置里的密钥字段

samples.txt 每行一个密文，支持 base64 / hex / 双层 urlencode 混排（自动识别）。
判定: ECB-OR-FIXED-IV / DETERMINISTIC / RSA-HYBRID / CBC-PKCS7-RANDOM-IV /
      STREAM-OR-ENCODING / AES-BLOCK-UNKNOWN
EOF
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  printf '[-] 需要 python3\n' >&2; exit 1
fi

exec python3 "$DIR/crypto-gateway-probe.py" "$@"
