#!/usr/bin/env bash
# Vue CLI / webpack SPA 生产 sourcemap 泄露检测 + 源码还原
# 用法: ./spa-sourcemap-check.sh https://target
set -u
# ---- 参数检查 ----
if [ $# -lt 1 ]; then printf '用法: ./spa-sourcemap-check.sh <https://TARGET>\n' >&2; exit 1; fi
BASE="${1:?usage: $0 https://target}"
UA="Mozilla/5.0"
IDX=$(curl -sk --max-time 20 -A "$UA" "$BASE/")
APP=$(printf '%s' "$IDX" | grep -oE '/static/js/app\.[a-f0-9]+\.js' | head -1)
[ -z "$APP" ] && APP=$(printf '%s' "$IDX" | grep -oE '/[a-zA-Z0-9/_.-]+\.js' | head -1)
echo "[*] bundle: $APP"
[ -z "$APP" ] && { echo "[-] 未找到 bundle，可能不是 Vue CLI 产物"; exit 1; }

# 0) 关键前置：先确认这个站点“缺失文件长什么样”。
#    很多 SPA 的 nginx 配了 try_files … /index.html，任何不存在的路径都回 200 + HTML。
#    此时只看 HTTP 200 会把 index.html 当成 .map 命中（本项目踩过）。
is_map() { # $1 = file
  head -c 200 "$1" | tr -d ' \n\r\t' | grep -q '^{"' \
    && head -c 200 "$1" | grep -q '"version"' \
    && head -c 2000 "$1" | grep -q '"sources"'
}

# 1) 直取 .map
code=$(curl -sk -o /tmp/app.js.map -w '%{http_code}' --max-time 30 -A "$UA" "$BASE$APP.map")
echo "[*] $APP.map -> HTTP $code  $(wc -c < /tmp/app.js.map) bytes"
if [ "$code" != "200" ] || ! is_map /tmp/app.js.map; then
  # 2) 退一步：bundle 末尾的 sourceMappingURL 注释
  curl -sk --max-time 30 -A "$UA" "$BASE$APP" -o /tmp/app.js
  if tail -c 200 /tmp/app.js | grep -qoE 'sourceMappingURL=[^ */]+'; then
    echo "[+] bundle 尾部带 sourceMappingURL，按注释里的名字再取一次"
  fi
  echo "[-] 无 sourcemap（返回的可能是 SPA fallback 的 index.html）"; exit 1
fi

# 3) 还原源码（sourcesContent 全量落盘，含中文注释/接口常量）
python3 - "$@" <<'PY'
import json,re,os,sys
m=json.load(open('/tmp/app.js.map'))
srcs=m.get('sources',[]); cont=m.get('sourcesContent',[])
have=sum(1 for c in cont if c)
print(f"[+] sources={len(srcs)} sourcesContent={have} chars={sum(len(c) for c in cont if c)}")
os.makedirs('srcmap-out',exist_ok=True)
for s,c in zip(srcs,cont):
    if not c: continue
    name=re.sub(r'[^A-Za-z0-9_.-]','_',s.replace('webpack:///',''))[:120]
    open('srcmap-out/'+name,'w').write(c)
print("[+] 已还原到 ./srcmap-out/")
PY
echo "[*] 接着做三件事：grep 硬编码密钥(accessKey/secret/appid/password)、grep 内网 IP、读 src/api/url.* 拿全量接口清单"

# 附：批量模式（从 webpack runtime 的 chunk map 推全量 chunk，逐个试 .map）
#   grep -oE '[0-9]+===e?"[^"]+\.js"' runtime.*.js   # 拿 100+ 条 chunk 路径
#   对每条试  <path>.map  与  <path 去掉 .js>.map，两个命名都要试；
#   命中判定必须用 is_map()，不要用状态码。