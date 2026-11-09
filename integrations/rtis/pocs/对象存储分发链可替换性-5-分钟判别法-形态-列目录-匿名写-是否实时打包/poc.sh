#!/usr/bin/env bash
# 对象存储/S3-CDN 分发链「可替换性」5 分钟判别法
# 输入：一个已知存在的对象 URL（apk/ipa/zip 均可）
# 输出：桶形态 / 可列目录 / 可匿名写 / 是否实时打包 —— 四项判定
#
# 纪律：探针用不可能存在的名字，命中立刻 DELETE；只 HEAD 不 GET 全量对象。

P='-x http://127.0.0.1:7892 --http1.1'   # 这批 CF 边缘对 h2 ALPN 会断，必须 --http1.1
OBJ="$1"                                  # 例 https://tx.r4t8y2u9io.com/apk/0ea114/ooxxoo8.apk
BASE=$(echo "$OBJ" | awk -F/ '{print $1"//"$3}')
DIR=$(echo "$OBJ" | sed 's#/[^/]*$##')
R=$RANDOM$RANDOM

echo "=== 1. 形态指纹（看响应头认厂商）==="
curl $P -sk -I -D- "$OBJ" | grep -iE '^(server|x-oss-request-id|x-amz-request-id|x-amz-bucket-region|x-obs-request-id|x-cos-request-id|x-tos-|etag|last-modified|content-length|x-cache|x-errno|x-reqid|accept-ranges)' 
# Server: AmazonS3       → S3/COS/MinIO 兼容
# x-oss-request-id       → 阿里 OSS
# x-obs-request-id       → 华为 OBS
# x-cos-request-id       → 腾讯 COS
# X-Errno + X-Reqid      → 网宿 WCS
# X-Cache: from cloudfront → AWS CloudFront 前置
# ETag 带 -N 后缀        → 分片上传，覆盖必须走分片 API（要凭据）

echo "=== 2. 列目录（四种语法）==="
for q in '/?list-type=2' '/?prefix=' '/?prefix=&delimiter=/' "/${DIR#*//*}/" ; do :; done
curl $P -sk -o /dev/null -w 'root            %{http_code}\n' "$BASE/"
curl $P -sk -o /dev/null -w 'list-type=2     %{http_code}\n' "$BASE/?list-type=2"
curl $P -sk -o /dev/null -w 'prefix=         %{http_code}\n' "$BASE/?prefix=&delimiter=/"
curl $P -sk -o /dev/null -w 'dir-trailing    %{http_code}\n' "${DIR%/}/"
# 200 且 body 含 <ListBucketResult>/<Contents>/<Key> = 可列（严重）
# 400/403 = 不可列（正常）

echo "=== 3. 匿名写探针（唯一写入动作）==="
curl $P -sk -X PUT -H 'Content-Type: text/plain' --data-binary 'probe' \
     -o /dev/null -w 'PUT  %{http_code}\n' "${DIR%/}/__probe_${R}.txt"
curl $P -sk -X POST --data-binary 'probe' \
     -o /dev/null -w 'POST %{http_code}\n' "${BASE}/__probe_${R}.txt"
# 200/201 = 可匿名写（critical，立刻 DELETE 并取证）
# 403      = AccessDenied（桶策略拒绝）
# 405      = 前置 nginx/CDN 直接拒方法（连签名校验都到不了）
# 501      = 不支持

echo "=== 4. 清理核验（必做）==="
curl $P -sk -o /dev/null -w 'probe GET %{http_code}\n' "${DIR%/}/__probe_${R}.txt"
# 404 = 确认未落地（正确）  200 = 真写进去了，立刻 DELETE
curl $P -sk -X DELETE -o /dev/null -w 'DELETE %{http_code}\n' "${DIR%/}/__probe_${R}.txt"

echo "=== 5. 区分「实时打包」还是「预置静态对象」==="
# 5a 不存在的路径：404 = 按 key 查（静态）；200 新内容 = 实时生成（危险面）
curl $P -sk -m 75 -o /dev/null -w 'bogus-key %{http_code} %{size_download}\n' \
     "$(dirname "$OBJ")/zzz999-999999.$(echo "$OBJ" | awk -F. '{print $NF}')"
# CODE=000 + 75s 0 字节 = 源站读超时（对象不存在但源站不返回 404）→ 判定为静态
# 5b 文件名里的时间戳 vs Last-Modified（注意时区差）
#     对应 ⇒ 上传时刻 ⇒ 预置；不对应 ⇒ 可能是渲染时生成
# 5c 参数影响：七个参数打同一 URL，比 Content-Length + ETag
for q in "" "?channel=t1" "?c=t1" "?pkg=com.evil" "?name=e" "?app=x" "?id=1"; do
  curl $P -sk -I -D- "$OBJ$q" | grep -iE 'content-length|etag' | tr '\n' ' '; echo " <- $q"
done
# 逐字节相同 ⇒ 参数不参与出包，不可参数化
