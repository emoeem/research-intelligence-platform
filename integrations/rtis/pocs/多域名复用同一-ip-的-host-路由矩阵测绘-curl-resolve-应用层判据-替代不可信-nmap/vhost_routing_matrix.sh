#!/usr/bin/env bash
# vhost_routing_matrix.sh — 多域名复用 IP 的 Host 路由矩阵
# 用法: ./vhost_routing_matrix.sh <IP> "<域名1 域名2 ...>"
# 判据: 只用应用层响应；curl 000 = 真实不可达（不要用 nmap）
set -u
# ---- 参数检查 ----
if [ $# -lt 2 ]; then printf '用法: ./vhost_routing_matrix.sh <IP> <domain[,domain2]>\n' >&2; exit 1; fi
IP="${1:?usage: $0 <IP> \"dom1 dom2 ...\"}"
DOMS="${2:?need domain list}"
SLEEP="${SLEEP:-1.5}"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

probe443() {
  local d=$1
  local stat
  stat=$(curl -sk -m 20 --resolve "$d:443:$IP" -D "$TMP/h" "https://$d/" -o "$TMP/b" \
         -w '%{http_code}|%{size_download}|%{time_total}' 2>/dev/null)
  local gw
  gw=$(grep -iE '^(server|via|x-kong-[a-z-]*|x-envoy[a-z-]*):' "$TMP/h" 2>/dev/null \
       | tr -d '\r' | paste -sd';' -)
  local title
  title=$(grep -oP '(?<=<title>)[^<]{0,60}' "$TMP/b" 2>/dev/null | head -1)
  printf '%-34s %-12s %-8s %-16s %s\n' "$d" "${stat%%|*}" \
         "$(echo "$stat" | cut -d'|' -f2)" "${gw:--}" "${title:--}"
}

printf '%-34s %-12s %-8s %-16s %s\n' DOMAIN CODE BYTES 'SERVER/GATEWAY' TITLE
for d in $DOMS; do probe443 "$d"; sleep "$SLEEP"; done

# ---- 网关 admin 面（按需取消注释）----
# Kong Admin API（默认 8001 明文 / 8444 TLS）
# for d in $DOMS; do
#   for p in 8001 8444; do
#     s=https; [ "$p" = 8001 ] && s=http
#     curl -sk -m 8 --resolve "$d:$p:$IP" -o /dev/null -w "$d:$p -> %{http_code}\n" "$s://$d:$p/"
#     sleep "$SLEEP"
#   done
# done
# Envoy / istio admin（15000 / 15001 / 9901）
# for d in $DOMS; do
#   for p in 15000 15001 9901; do
#     curl -sk -m 8 --resolve "$d:$p:$IP" -o /dev/null -w "$d:$p /server_info -> %{http_code}\n" "http://$d:$p/server_info"
#     sleep "$SLEEP"
#   done
# done
# 443 路径面（网关若把 admin 路由到 80/443）
# for d in $DOMS; do
#   for path in /admin /metrics /status /stats /config_dump /server_info /clusters; do
#     curl -sk -m 10 --resolve "$d:443:$IP" -o /dev/null -w "$d$path -> %{http_code}|%{size_download}\n" "https://$d$path"
#     sleep "$SLEEP"
#   done
# done

# 聚类提示：
#  · 同一 (状态码 + 字节数 + Server) 三元的域名 = 同一后端，结论可复用；
#  · 长度与首页完全一致的 200 = SPA catch-all 回落，不代表该路径存在；
#  · 长度极短（<100B）的 JSON 403 = 网关插件拦截（Kong ip-restriction / key-auth 等）；
#  · 多个端口 curl 000 = 真实不可达，直接划掉，不要交给 nmap 判定。
