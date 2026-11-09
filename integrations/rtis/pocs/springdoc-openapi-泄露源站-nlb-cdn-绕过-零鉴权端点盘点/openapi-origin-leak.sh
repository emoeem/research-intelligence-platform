#!/usr/bin/env bash
# 用法: bash openapi-origin-leak.sh <https://target> [/prefix]
set -u
T="${1:?target url}"; P="${2:-/dcserv}"
P="${P%/}"
UA='Mozilla/5.0 (Windows NT 10.0; Win64; x64)'

echo "== 1. 前缀探测 =="
for pre in "" /dcserv /adserv /report /api /track /collect /gateway /admin; do
  code=$(curl -sS -m 10 -A "$UA" -o /tmp/_p -w '%{http_code}' "$T${pre}/error" 2>/dev/null)
  body=$(head -c 120 /tmp/_p | tr -d '\n')
  echo "  ${pre:-/}/error -> $code | $body"
done

echo "== 2. OpenAPI 文档 =="
for u in "$T$P/v3/api-docs" "$T$P/api-docs" "$T$P/swagger-resources"; do
  code=$(curl -sS -m 12 -A "$UA" -o /tmp/_api -w '%{http_code}|%{size_download}' "$u")
  echo "  $u -> $code"
  if [ "${code%%|*}" = "200" ]; then
    cp /tmp/_api ./openapi.json
    echo "  [saved] ./openapi.json"
    break
  fi
done

echo "== 3. 鉴权面判定（无 securitySchemes = 零鉴权）=="
python3 - <<'PY'
import json,sys
try: d=json.load(open('./openapi.json'))
except Exception as e: print('  parse fail:',e); sys.exit()
ss=d.get('components',{}).get('securitySchemes',{})
print('  securitySchemes:', list(ss.keys()) or 'NONE -> 所有端点无鉴权')
paths=d.get('paths',{})
print('  端点总数:', sum(len([m for m in v if m in ('get','post','put','delete','patch')]) for v in paths.values()))
for p,v in paths.items():
    for m in ('get','post','put','delete','patch'):
        if m in v and not v[m].get('security'): print(f'    {m.upper():6} {p}')
print('  servers:')
for s in d.get('servers',[]): print('   ',s.get('url'))
PY

echo "== 4. 源站解析 + 直连（CDN 绕过）=="
python3 -c "
import json
d=json.load(open('./openapi.json'))
for s in d.get('servers',[]):
    u=s.get('url','')
    if u.startswith('http'): print('ORIGIN_URL='+u)
" > /tmp/_servers
cat /tmp/_servers
while IFS='=' read -r _ u; do
  host=$(echo "$u" | sed -E 's#https?://##; s#/.*##')
  echo "  host=$host"
  dig +short "$host" | head -3 | sed 's/^/    ip= /'
  for path in /api/health /health /actuator/health; do
    echo -n "    $u$path -> "
    curl -sS -m 10 -A "$UA" -o /tmp/_h -w '%{http_code}|%{size_download}\n' "${u%/}$path"
  done
done < /tmp/_servers

echo "== 5. 提醒 =="
echo "  只做 GET 读验证；写接口(update/delete)与本机无关的写操作必须先获授权。"
