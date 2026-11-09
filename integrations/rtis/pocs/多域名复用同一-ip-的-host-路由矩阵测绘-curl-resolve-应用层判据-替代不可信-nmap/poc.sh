#!/usr/bin/env bash
# ============================================================================
# 多 vhost / 多域名共 IP 的 Host 路由矩阵测绘
# 用法: ./poc.sh 203.0.113.10 a.com,b.com,c.a.com
#       ./poc.sh https://203.0.113.10 a.com b.com      (自动识别 scheme)
#
# 【判据】为什么不用 nmap
#   nmap 看的是「端口」，同一个 443 上挂了 20 个站点，nmap 只能告诉你「443 开」。
#   这里要的是「应用层路由」：同一个 IP，不同 Host 头 → 不同服务。
#   唯一可靠的判据是**响应体差异**（title / 长度 / 特定字符串 / 状态码组合）。
#
# 【四类判据（用于区分「站点不存在」和「站点存在但拒你」）】
#   A. 默认站点回退  → 所有未知 Host 都返回同一内容（多为 nginx 默认页/证书错误）
#   B. IP 白名单     → 403 且 body 含 "Forbidden"/"not allowed"
#   C. 域名停放/CF   → 522/523/530 或 "This site can't be reached" 类页面
#   D. TLS SNI 不匹配 → handshake 失败；用 -k + --resolve 可绕过证书校验看应用层
#
# 【常见误报】
#   1. 泛解析（*.a.com 全部解析到同一 IP）→ 任意子域都「存在」。
#      排除: 用随机子域 zzz9x8.a.com 做对照，若与目标响应一致 → 是泛解析，无判别力。
#   2. CDN 前置 → 你打的是 CDN 边缘，不是源站。要另做源站发现（见 POC 76）。
#   3. 只比状态码 → 大量 vhost 对未知 Host 也返回 200（默认站）。
#      必须比 body 指纹，不是比 code。
# ============================================================================
set -uo pipefail
source "$(dirname "$0")/../_lib/common.sh"

HOST_OR_URL="${1:-}"
shift || true
DOMAINS="${*:-}"
[ -z "$HOST_OR_URL" ] && usage_die "$0 <IP|URL> <domain[,domain2...]>"

IP=$(printf '%s' "$HOST_OR_URL" | sed -E 's#^https?://##; s#[:/].*$##')
SCHEME=http
case "$HOST_OR_URL" in https://*) SCHEME=https ;; esac
preflight || exit 1

log "目标 IP: $IP   协议: $SCHEME   域名: $DOMAINS"

# ---- 对照 1: 无 Host 头 / 用 IP 直连 ---------------------------------------
log "对照组 A: 直接打 IP（不带 Host）"
rA=$(reqc GET "$SCHEME://$IP/")
cA="${rA%%$'\t'*}"; bA="${rA#*$'\t'}"
fpA=$(printf '%s' "$bA" | fp)
dim "IP 直连 → $cA  fp=${fpA:0:70}"

# ---- 对照 2: 随机不存在的 Host --------------------------------------------
RAND_HOST="zzz$(date +%s | tail -c 5).invalid"
log "对照组 B: 随机 Host ($RAND_HOST)"
rB=$(reqc GET "$SCHEME://$IP/" -H "Host: $RAND_HOST")
cB="${rB%%$'\t'*}"; bB="${rB#*$'\t'}"
fpB=$(printf '%s' "$bB" | fp)
dim "随机 Host → $cB  fp=${fpB:0:70}"

echo
log "开始矩阵测绘"
printf '%-32s %-6s %-8s %s\n' "HOST" "CODE" "LEN" "TITLE / 指纹"
printf '%s\n' "────────────────────────────────────────────────────────────────"

declare -A SEEN
report() {
  local d="$1" c="$2" b="$3" tag="$4"
  local len=${#b}
  local title; title=$(printf '%s' "$b" | grep -oiE '<title>[^<]*' | head -1 | sed 's/<title>//i')
  [ -z "$title" ] && title=$(printf '%s' "$b" | head -c 60 | tr -d '\n\r')
  local f; f=$(printf '%s' "$b" | fp)
  local verdict="OK"

  if [ "$f" = "$fpB" ] || [ "$f" = "$fpA" ]; then
    verdict="DEFAULT"   # 与默认站一致 → 该 Host 未被单独配置
  fi
  case "$c" in
    000) verdict="TLSFAIL" ;;
    403) verdict="IP-WHITELIST" ;;
    421) verdict="MISDIRECTED" ;;
    522|523|530) verdict="CF-DOWN" ;;
  esac
  if printf '%s' "$b" | grep -qiE 'not allowed|Access Denied|no site configured|default (server|page)'; then
    verdict="DEFAULT"
  fi

  # 同一指纹已经出现过 → 标 DUP
  if [ -n "${SEEN[$f]:-}" ] && [ "$verdict" = "OK" ]; then
    verdict="SAME-AS:${SEEN[$f]}"
  fi
  [ "$verdict" = "OK" ] && SEEN[$f]="$d"

  printf '%-32s %-6s %-8s %s  %s%s%s\n' "$d" "$c" "$len" "${title:0:52}" \
    "$([ "$verdict" = "OK" ] && echo "$_c_grn" || echo "$_c_dim")" \
    "[$verdict]" "$_c_rst"
}

for d in ${DOMAINS//,/ }; do
  [ -z "$d" ] && continue
  # 用 --resolve 强制把域名钉到该 IP，绕过 DNS 与证书校验
  port=80; [ "$SCHEME" = "https" ] && port=443
  r=$(curl -sk --max-time "$TIMEOUT" -A "$UA" \
        --resolve "$d:$port:$IP" \
        -w $'\n%{http_code}' "$SCHEME://$d/" 2>/dev/null)
  c="${r##*$'\n'}"; b="${r%$'\n'*}"
  [ -z "$c" ] && c=000
  report "$d" "$c" "$b"

  # 子域快速扩展：常见前缀（只在给了主域时做）
  if printf '%s' "$d" | grep -qE '^[a-z0-9-]+\.[a-z0-9-]+\.[a-z]{2,}$'; then
    parts=$(printf '%s' "$d" | awk -F. '{print $(NF-1)"."$NF}')
    for pre in www admin api test dev uat pre prod m mobile static img cdn file upload pay order user member agent h5 wap app gateway; do
      sub="$pre.$parts"
      [ "$sub" = "$d" ] && continue
      c2=$(code_of GET "$SCHEME://$IP/" -H "Host: $sub" --resolve "$sub:$port:$IP")
      # 只报「不是默认站」的子域，否则噪音太大
      if [ "$c2" != "000" ] && [ "$c2" != "404" ]; then
        b2=$(req GET "$SCHEME://$sub/" --resolve "$sub:$port:$IP")
        f2=$(printf '%s' "$b2" | fp)
        if [ "$f2" != "$fpB" ] && [ "$f2" != "$fpA" ]; then
          report "$sub" "$c2" "$b2"
        fi
      fi
      sleep 0.2
    done
  fi
  sleep "$SLEEP"
done

echo
cat <<'EOF'
════ 判读规则 ════
  OK            → 该 Host 有独立配置（真实站点）
  DEFAULT       → 与默认站/随机 Host 一致（未配置，或泛解析）
  SAME-AS:x     → 与 x 完全同指纹（同一套代码，可能是多域名复用）
  IP-WHITELIST  → 403，存在但只允许特定来源
  TLSFAIL       → SNI 不匹配，可能是证书绑定死（也用 --resolve 再试一次）
  CF-DOWN       → CDN 后面源站挂了，站点可能存在

════ 泛解析判定 ════
  如果随机 Host 和你的目标 Host 指纹一致 → 该域是泛解析，本脚本对它无判别力。
  此时改用 DNS 历史 / CT 日志 / 证书 SAN 去找真实子域，而不是爆破。
EOF
