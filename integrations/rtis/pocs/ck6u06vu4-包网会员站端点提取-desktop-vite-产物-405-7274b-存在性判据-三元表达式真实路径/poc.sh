#!/usr/bin/env bash
# CK6u06Vu4 / BBIN 系白牌包网会员站 —— 端点提取 + 契约建立
# 用法: ./ck6-extract.sh <IP> <HOST:9900>
set -u
# ---- 参数检查 ----
if [ $# -lt 2 ]; then printf '用法: ./poc.sh <IP> <HOST:9900>\n' >&2; exit 1; fi
IP="${1:?ip}"; HOST="${2:?host:9900}"
UA='Mozilla/5.0 (Linux; Android 13) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Mobile Safari/537.36'
J=/tmp/ck6jar.$$; : > "$J"

warm() { curl -sk -4 --max-time 25 -c "$J" -b "$J" -o /dev/null "https://$IP:9900/" -H "Host: $HOST" -A "$UA"; }

# ---------- 1. 上传 / 登录 / 注册 端点提取 ----------
extract() {
  warm
  for page in /desktop/showcase /desktop/ /infe/user/join/ /web/mobile/ /mobile/; do
    html=$(curl -sk -4 --max-time 25 -c "$J" -b "$J" "https://$IP:9900$page" -H "Host: $HOST" -A "$UA")
    echo "=== $page (${#html}B)"
    # 页面直接引用 + CDN 上的 bundle
    js=$(printf '%s' "$html" | grep -oE '(src|href)="[^"]+\.js' | sed 's/.*="//')
    for u in $js; do
      case "$u" in
        http*) full="$u" ;;
        /*)    full="https://$IP:9900$u" ;;
        *)     continue ;;
      esac
      f=/tmp/ck6-$(basename "$u")
      curl -sk -4 --max-time 40 -o "$f" "$full" -H "Host: $HOST" -A "$UA" 2>/dev/null
      [ -s "$f" ] || continue
      grep -ohE '[a-zA-Z0-9_/.:-]*(imgupdate|upload|Upload|avatar|idcard|photo|attach)[a-zA-Z0-9_/.:-]*' "$f" | sort -u | sed "s|^|  UPLOAD> |"
      grep -ohE '"/[a-zA-Z0-9_/.-]*(login|register|upload)[a-zA-Z0-9_/.-]*"' "$f" | sort -u | sed "s|^|  API>    |"
    done
  done
}

# ---------- 2. 存在性判据 ----------
# 不存在 → 405 + 7274B (SPA 兜底)   存在但需登录 → 200 + 小体积业务 JSON
probe_oracle() {
  warm
  for p in /infe/rest/register/zzqqxx99notexist.json /entrance/showcase/images/imgupdate.json "$@"; do
    out=$(curl -sk -4 --max-time 25 -c "$J" -b "$J" -w '|%{http_code}|%{size_download}' "https://$IP:9900$p" -H "Host: $HOST" -A "$UA")
    code=$(printf '%s' "$out" | awk -F'|' '{print $(NF-1)" "$NF}')
    body=$(printf '%s' "$out" | head -c 120 | tr -d '\n')
    printf '%-52s [%s] %s\n' "$p" "$code" "$body"
    sleep 0.6
  done
}

# ---------- 3. 登录契约 ----------
login_contract() {
  warm
  fp=$(openssl rand -hex 16 2>/dev/null || head -c16 /dev/urandom | xxd -p)
  for d in '' 'account=x&password=y' 'username=x&password=y'; do
    for q in '' "?k=$fp"; do
      printf 'k=%-34s body=%-24s → ' "${q:-<none>}" "${d:-<empty>}"
      curl -sk -4 --max-time 25 -c "$J" -b "$J" --data "$d" \
        "https://$IP:9900/entrance/login.json$q" \
        -H "Host: $HOST" -A "$UA" -H 'Content-Type: application/x-www-form-urlencoded; charset=UTF-8' \
        -H 'X-Requested-With: XMLHttpRequest' | head -c 160; echo
      sleep 1
    done
  done
}

"$@"
