#!/bin/bash
# 多租户 SaaS 鉴权前缀差分法：用"同 host 三条前缀对照"定位未授权接口
# 适用：网关按 Host 白名单 + 框架级拦截器（如 Spring HandlerInterceptor）只覆盖部分路径前缀
# 用法：./saas-prefix-diff.sh <HOST> <KNOWN_PROTECTED_PATH> <CANDIDATE_PATH>

# ---- 参数检查 ----
if [ $# -lt 3 ]; then printf '用法: ./saas-prefix-diff.sh <HOST> <KNOWN_PROTECTED_PATH> <CANDIDATE_PATH>\n' >&2; exit 1; fi
H="$1"; PROT="$2"; CAND="$3"
UA='Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/125.0 Safari/537.36'

probe(){ # $1=label $2=path
  local code body
  body=$(curl -sS --max-time 20 -A "$UA" -H 'Accept: application/json' -w '\n%{http_code}' "https://$H$2")
  code=$(printf '%s' "$body" | tail -1)
  body=$(printf '%s' "$body" | sed '$d' | tr -d '\n\t' | cut -c1-160)
  printf '[%s] %s -> %s %s\n' "$1" "$2" "$code" "$body"
}

# 1) 已知受保护前缀（应 401/403）——作为"鉴权确实存在"的基线
probe PROTECTED "$PROT"
# 2) 候选前缀（若 200 且带数据 = 拦截器未覆盖）
probe CANDIDATE "$CAND"
# 3) 路由不存在（看框架的 404 形态，用来区分"路由不存在"和"处理器报错"）
probe NO-ROUTE /$(basename "$CAND").zzz-not-exist

# 判读规则（本次实战得出）：
#   401/403 空体            → 鉴权拦截器生效
#   200 + {"code":200,...}  → 拦截器未覆盖，直接拿数据（漏洞）
#   200 + {"code":9999,"msg":"业务原因"} → 业务异常码，处理器已执行（不是鉴权，别当成"挡住了"）
#   200 + {"code":9999,"msg":null}       → 处理器抛未捕获异常（常因缺登录上下文/缺参），与鉴权无关
#   404 + {"path":"/api/..."}            → 前缀被 nginx 剥离，可用于推断后端真实 context-path

# 典型实战（163K）：
#   ./saas-prefix-diff.sh hnzx.163kyun.cn \
#       /love/api/member/loveContact/getLoveCustomerContact?customerId=1 \
#       /love/api/loveMemberCustomer/getLoveCustomerInfo?id=1
#   → PROTECTED 401，CANDIDATE 200 + 107 字段档案
