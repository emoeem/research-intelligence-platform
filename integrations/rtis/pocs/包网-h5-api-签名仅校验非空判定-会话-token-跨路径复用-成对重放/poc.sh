#!/bin/bash
# api.sh — 弱构建包网 API 会话重放器
# 用法: ./api.sh "/api/v1/member/order/index?page=1" [-X POST -d '{}' ...]
# 依赖 tokens.tsv:  path<TAB>method<TAB>token<TAB>customerUID<TAB>timestamp<TAB>url
set -u
# ---- 参数检查 ----
if [ $# -lt 1 ]; then printf '用法: ./poc.sh "<path?query>"   （依赖同目录 tokens.tsv）\n' >&2; exit 1; fi
P="$1"; shift
H=${API_HOST:-t1xlws1.lqecey.com}
LINE=$(awk -F'\t' -v p="${P%%\?*}" '$1==p{print; exit}' tokens.tsv)
if [ -z "$LINE" ]; then
  LINE=$(head -1 tokens.tsv)   # 会话 token 不绑 path，任一请求的 token 都可用
fi
TOK=$(echo "$LINE" | cut -f3); CUID=$(echo "$LINE" | cut -f4); TS=$(echo "$LINE" | cut -f5)
curl -sk -m 20 -w '\n[HTTP %{http_code} %{time_total}s]\n' \
  -H "token: $TOK" -H "timestamp: $TS" -H "device: h5" -H "customerUID: ${CUID:-0}" \
  -H "Accept: application/json, text/plain, */*" \
  -H "Origin: https://${H5_HOST:-example}" -H "Referer: https://${H5_HOST:-example}/" \
  "$@" "https://$H$P"

# ---- 从 agent-browser 导出 tokens.tsv ----
# agent-browser network requests --filter api --json > reqs.json
# python3 - <<'PY'
# import json
# from urllib.parse import urlparse
# d=json.load(open('reqs.json'))
# out=[]
# for r in d['data']['requests']:
#     h=r.get('headers',{}); tok=h.get('token')
#     if not tok: continue
#     u=urlparse(r['url'])
#     out.append("\t".join([u.path, r['method'], tok, h.get('customerUID',''), h.get('timestamp',''), r['url']]))
# open('tokens.tsv','w').write("\n".join(out)+"\n")
# PY
