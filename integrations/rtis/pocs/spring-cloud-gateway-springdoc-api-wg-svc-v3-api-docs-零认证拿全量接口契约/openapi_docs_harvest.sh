#!/bin/bash
# Spring Cloud Gateway discovery-locator + springdoc 未关闭 → 零认证拿全量接口契约
# 用法: ./openapi_docs_harvest.sh <DOMAIN> <PLATFORM_ID> [OUTDIR]
#   DOMAIN      必须能解析到目标的网关（带正确 Host 语义）
#   PLATFORM_ID 目标网关的 siteId/platformId（从 /api/site/bootstrap 或前端 X-Login-Type 取值）
set -u
# ---- 参数检查 ----
if [ $# -lt 1 ]; then printf '用法: ./openapi_docs_harvest.sh <DOMAIN>\n' >&2; exit 1; fi
DOMAIN="${1:?DOMAIN}"; PID="${2:?PLATFORM_ID}"; OUT="${3:-./openapi_out}"
mkdir -p "$OUT"

# ① 从网关 actuator 的 routes/服务注册里拿 serviceId 列表；拿不到就用常见业务名扫
SERVICES="${SERVICES:-member risk discount bigdata firm system operate share gateway}"

# ② 逐个服务拉 springdoc v3 文档
for svc in $SERVICES; do
  for p in v3/api-docs v2/api-docs swagger-resources swagger-ui.html doc.html; do
    code=$(curl -sS -o "$OUT/${svc}_$(echo "$p"|tr '/' '_').json" -w '%{http_code}' --max-time 30 \
      -H "Host: $DOMAIN" -H "X-Login-Type: $PID" \
      "https://$DOMAIN/api/wg-$svc/$p" 2>/dev/null)
    sz=$(stat -c%s "$OUT/${svc}_$(echo "$p"|tr '/' '_').json" 2>/dev/null || echo 0)
    echo "$svc/$p -> $code ${sz}B"
    sleep 0.6
  done
done

# ③ 判定：真文档 > 100KB 且含 "openapi"；65B/122B 的分别是 sa-token 体与空 spec
echo "--- 真文档 ---"
for f in "$OUT"/*_v3_api-docs.json; do
  sz=$(stat -c%s "$f" 2>/dev/null || echo 0)
  [ "$sz" -gt 1000 ] && grep -q '"openapi"' "$f" && echo "HIT $f ${sz}B"
done

# ④ 提取全部 operation 与 /anon/ 清单
python3 - "$OUT" <<'PY'
import json,glob,os,sys
out=sys.argv[1]; rows=[]
for f in sorted(glob.glob(os.path.join(out,'*_v3_api-docs.json'))):
    svc=os.path.basename(f).split('_v3_')[0]
    try: d=json.load(open(f))
    except Exception: continue
    for p,ops in (d.get('paths') or {}).items():
        for m,op in ops.items():
            if m.lower() in ('get','post','put','delete','patch'):
                rows.append((svc,p,m.upper(),op.get('summary','') or ''))
anon=[r for r in rows if '/anon/' in r[1]]
for r in anon: print('ANON\t%s\t%s\t%s\t%s'%r)
print(f'\n# total operations={len(rows)}  anon={len(anon)}', file=sys.stderr)
open(os.path.join(out,'anon_endpoints.tsv'),'w').write('\n'.join('\t'.join(r) for r in anon))
PY
