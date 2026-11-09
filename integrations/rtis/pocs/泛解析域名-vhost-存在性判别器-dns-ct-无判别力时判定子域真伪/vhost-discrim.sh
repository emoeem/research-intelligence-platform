#!/usr/bin/env bash
# 泛解析域名 vhost 存在性判别器
# 场景：*.domain 全部解析到同一 IP（CNAME 兜底），DNS 与证书层零判别力，需要判定某个子域是不是真实服务
# 关键点：不要用 / 做判据；要用"不存在的非根路径"，且必须做双向对照 + 顺序对照

set -u
# ---- 参数检查 ----
if [ $# -lt 1 ]; then printf '用法: ./vhost-discrim.sh <domain>\n' >&2; exit 1; fi
DOMAIN="${1:?domain}"; IP="${2:?ip}"; shift 2
UA='Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36'
PATH_PROBE="/zz9k2xnope$$"
SLEEP="${SLEEP:-4}"

probe() {
  local h="$1" tag="$2" try out code md5 loc
  for try in 1 2 3 4; do
    out=$(curl -sk -4 --max-time 25 -A "$UA" --resolve "$h:443:$IP" -D /tmp/vd_h.$$ -o /tmp/vd_b.$$ \
          -w '%{http_code}|%{size_download}' "https://$h$PATH_PROBE")
    code=${out%%|*}
    [ "$code" = "429" ] && { sleep 6; continue; }
    md5=$(md5sum /tmp/vd_b.$$ | cut -d' ' -f1)
    loc=$(grep -i '^location:' /tmp/vd_h.$$ | tr -d '\r' | sed 's/[Ll]ocation: *//')
    printf '%-28s %-6s %-12s %-34s %s\n' "$tag:$h" "$code" "${out##*|}" "$md5" "${loc:0:60}"
    LAST_MD5="$md5"; LAST_CODE="$code"
    return 0
  done
  printf '%-28s %s\n' "$tag:$h" "FAILED(429)"; LAST_MD5=""
}

# 1) 随机基线 x2（确认基线稳定）
probe "zz$(head -c4 /dev/urandom | od -An -tx1 | tr -d ' \n').$DOMAIN" BASE; sleep "$SLEEP"
probe "zz$(head -c4 /dev/urandom | od -An -tx1 | tr -d ' \n').$DOMAIN" BASE
BASE_MD5="$LAST_MD5"; BASE_CODE="$LAST_CODE"; sleep "$SLEEP"

# 2) 候选子域
declare -A RES
for s in "$@"; do
  probe "$s.$DOMAIN" TARGET
  if [ "$LAST_MD5" = "$BASE_MD5" ] && [ -n "$BASE_MD5" ]; then
    RES["$s.$DOMAIN"]="泛解析噪音（与随机基线逐字节相同）"
  else
    RES["$s.$DOMAIN"]="★ 疑似真实服务（$LAST_CODE/${LAST_MD5:0:8}）"
  fi
  sleep "$SLEEP"
done

# 3) 已知真域对照，必须放在目标之后（排除频控假阴性）
probe "www.$DOMAIN" CONTROL
[ "$LAST_MD5" = "$BASE_MD5" ] && echo "!! 控制组 == 基线 ⇒ 该面不可判别，结论无效（换路径/换端口/上浏览器）"

for k in "${!RES[@]}"; do printf '%-30s %s\n' "$k" "${RES[$k]}"; done
