#!/bin/bash
# ThinkPHP 5.1/6 + url_route_must=true 的「路由存在性 oracle」
# 用法: tp_route_oracle.sh https://HOST /api/v1/xxx [gap]
# 判据:
#   content-type 含 json 或 body 以 { 开头        -> 路由存在（通常 405 Not Found Params）
#   html 且 body 含「当前访问路由未定义或不匹配」 -> 路由不存在
#   其他（403 WAF / 302）                         -> 无效样本，重试或换线路

# ---- 参数检查 ----
if [ $# -lt 1 ]; then printf '用法: ./tp_route_oracle.sh <host>\n' >&2; exit 1; fi
H="${1:?host}"; P="${2:?path}"; GAP="${3:-4.5}"
U="$H/zzz?s=$P"          # /zzz 可替换为任意非 /index.php 的路径（try_files 会带 s 重写进框架）
code=$(curl -sk --max-time 15 -D /tmp/rh -o /tmp/rb -w '%{http_code}' "$U")
size=$(wc -c </tmp/rb)
ct=$(grep -i '^content-type' /tmp/rh | head -1 | tr -d '\r')
if grep -q '当前访问路由未定义或不匹配' /tmp/rb; then
  echo "ABSENT   $code $size  $P"
elif echo "$ct" | grep -qi json; then
  echo "EXISTS   $code $size  $P  $(head -c 120 /tmp/rb)"
else
  echo "UNKNOWN  $code $size  $P  $ct"
fi
sleep "$GAP"

# 校验 oracle 自身是否有效：先用一条已知存在的路由做对照
#   tp_route_oracle.sh https://HOST /api/v1/common/banner/list   -> 必须 EXISTS
#
# 关键前提与坑:
# 1) 入口路径不能写 /index.php（源站 nginx 常 location = /index.php { internal; } 会 403/146B）
# 2) 必须先用「确定不存在的应用名」验证 think-multi-app 是否启用：
#      s=/zzqqzz/index/index -> 出现 "app not exists:zzqqzz" = 多应用开启（s= 可直调 app/<name>/controller）
#                              仍是「路由未定义」          = 多应用关闭（s= 只是路由路径，别浪费时间猜控制器）
# 3) 同一路由常只接受特定 method；405 JSON 也算存在
# 4) EdgeOne/SafeLine 对 s= 中的反斜杠、and/sleep 会直接 RST 或 403，本 oracle 只用纯路径，不触发