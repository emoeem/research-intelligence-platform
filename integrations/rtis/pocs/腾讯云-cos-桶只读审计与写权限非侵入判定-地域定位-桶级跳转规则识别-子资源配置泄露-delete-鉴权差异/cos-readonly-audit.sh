#!/bin/bash
# 腾讯云 COS 桶只读审计（不写一个字节）
# 用法: ./cos-audit.sh <bucket-appid> [probe-key]
# ---- 参数检查 ----
if [ $# -lt 1 ]; then printf '用法: ./cos-readonly-audit.sh <bucket-appid>\n' >&2; exit 1; fi
B="${1:?bucket-appid e.g. mybucket-1250000000}"
K="${2:-index.html}"
UA='Mozilla/5.0 (Linux; Android 13; SM-G991B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36'

echo "===== 1) 地域定位（accelerate/未知地域时必做）====="
for r in ap-guangzhou ap-shanghai ap-beijing ap-chengdu ap-nanjing ap-chongqing ap-hongkong \
         ap-seoul ap-singapore ap-tokyo ap-bangkok ap-jakarta na-ashburn na-siliconvalley \
         eu-frankfurt sa-saopaulo; do
  c=$(curl -sS -m 10 -o /dev/null -w '%{http_code}' -H "User-Agent: $UA" "https://$B.cos.$r.myqcloud.com/$K")
  [ "$c" != "404" ] && echo "  >>> $r => $c (命中，其余地域返回 404 NoSuchBucket)"
done
echo "  对照（排除全局劫持）:"
curl -sS -m 10 -H "User-Agent: $UA" "https://zzznotexist-1234567890.cos.<REGION>.myqcloud.com/x" | grep -o '<Code>[^<]*</Code>'

echo "===== 2) 任意 key 判定：对象重定向 vs 桶级跳转规则 ====="
# 三个不同随机 key 若返回同一个 Location => 桶级规则（与 key 无关）
for i in 1 2 3; do
  nk="__nx_$(head -c8 /dev/urandom | od -An -tx1 | tr -d ' \n')"
  echo -n "  GET /$nk -> "
  curl -sS -m 10 -o /dev/null -D - -H "User-Agent: $UA" "https://$B.cos.<REGION>.myqcloud.com/$nk" \
    | grep -Ei '^(HTTP/|Location:)' | tr -d '\r' | tr '\n' ' '; echo
done
echo "  HEAD 同对象（有 ETag=对象存在；404 无 ETag=对象不存在、规则只对 GET 生效）:"
curl -sS -m 10 -I -H "User-Agent: $UA" "https://$B.cos.<REGION>.myqcloud.com/$K" | grep -Ei 'HTTP/|etag|last-modified|content-length'

echo "===== 3) 桶级 14 个子资源（200 = 配置泄露）====="
for s in acl policy cors website tagging logging versioning replication lifecycle \
         intelligenttiering object-lock inventory accelerate encryption; do
  printf '  ?%-20s ' "$s"
  curl -sS -m 10 -o /dev/null -w '%{http_code}\n' -H "User-Agent: $UA" "https://$B.cos.<REGION>.myqcloud.com/?$s"
done

echo "===== 4) 写权限非侵入判定（每项仅 1 次，空 body，随机 key，绝不发合法 body）====="
RK="__no_such_key_$(head -c8 /dev/urandom | od -An -tx1 | tr -d ' \n')"
echo "  POST /?delete  ->"
curl -sS -m 12 -X POST -H "User-Agent: $UA" -H 'Content-Length: 0' \
  "https://$B.cos.<REGION>.myqcloud.com/?delete" | grep -o '<Code>[^<]*</Code>\|<Message>[^<]*</Message>'
echo "  POST /$RK?uploads ->"
curl -sS -m 12 -X POST -H "User-Agent: $UA" -H 'Content-Length: 0' \
  "https://$B.cos.<REGION>.myqcloud.com/$RK?uploads" | grep -o '<Code>[^<]*</Code>'
# 判读：
#   403 AccessDenied            => ACL 前置拒绝（安全）
#   400 InvalidRequest/MalformedXML => 进入参数解析层，未被 ACL 拦住（写能力"可能"存在，需授权后用合法 body 验证）
#   !! 3500/400 之后立即停手，不要发合法 body !!

echo "===== 5) 跳转目标轮换检测（判断是静态指针还是自动化改写）====="
for i in $(seq 1 10); do
  curl -sS -m 10 -o /dev/null -D - -H "User-Agent: $UA" "https://$B.cos.<REGION>.myqcloud.com/$K" \
    | grep -i '^location' | tr -d '\r'
  sleep 6
done
# 值恒定 => 静态配置，改一次长期生效；周期性变化 => 平台侧自动化在改写，换包属持续对抗
