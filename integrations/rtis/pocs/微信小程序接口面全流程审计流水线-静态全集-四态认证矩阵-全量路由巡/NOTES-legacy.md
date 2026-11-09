# 微信小程序接口面全流程审计流水线
# 来源：https://github.com/xegeng/wx-miniapp-skills (miniapp-audit-pipeline)
# 脚本原地址：skills/miniapp-audit-pipeline/scripts/

## ===== 关键判据速查 =====
# A 无凭据      -> 401/403 = 有门 ; 业务码 = 可能无门
# B 真id+空token -> 进业务 = token 未校验
# C 真id+假token -> 同B = token 形同虚设
# D 随机id+同token -> 401 = 存在性预言机（可枚举 id 是否注册）
#
# 铁律：业务码 != HTTP 成功 != 登录成功
#       {"code":1007,"msg":"用户手机号为空"} = 已穿过鉴权层，只是数据缺失

## ===== 零请求先做：静态搜免登录标记（最高价值） =====
grep -rn "isNotCheckAccessToken\|noAuth\|noLogin\|whiteList\|白名单" <unpacked_dir> \
  | head -50

## ===== 静态接口提取（两级输出） =====
# api_fragments  裸片段 unicom/app/user/auth  <- 置信度高，矩阵首选
# apis           完整 URL
# relative_candidates /xxx/yyy 未确认
# noauth_hints   免登录标记 + 上下文  <- 最高价值
python3 extract_apis.py --root <unpacked> --out apis.json

## ===== 全量解包（主包 + 分包，缺一漏接口） =====
# 包路径：<微信数据目录>/radium/users/<uid>/applet/packages/<appid>/<version>/*.wxapkg
python3 unpack.py --appid wxXXXXXXXX --version <ver> --out <dir>

## ===== 四态矩阵 =====
# matrix.json 填 base_url / 真实id / 随机id / 空token / 假token
python3 auth_matrix.py --config matrix.json --apis apis.json --out matrix_result.json --delay 3

## ===== 全量路由巡（拿动态实证） =====
python3 nav_sweep.py --routes routes.json --out nav.json --wait 2200 --resume

## ===== 关键坑（会直接导致结论错误） =====
# 1. CDP Network 是进程级的，同机多小程序会混 -> 按 request.id 的 "pid." 前缀分离
# 2. wx.request hook 在 4.x 常挂不上（appservice 独立线程）-> 改用 CDP Network 采集，别在 hook 上耗
# 3. 高频 reLaunch 会压崩渲染进程（80页崩1-2次）-> 每 call 包 asyncio.wait_for(nav 6s / recent 20s)
# 4. PEM 密钥高频假阳性：-----BEGIN PUBLIC KEY----- 出现在 jsencrypt/crypto-js 的 PEM 拼装函数里 = 库模板，不是漏洞
# 5. 空 200 不等于成功（istio 等网关对无效 POST 返 200+空体）-> 必须看响应体
# 6. 渲染进程崩溃后 pid 变化 -> 重起桥 -> UI 重开小程序 -> 重跑 Phase0 -> nav_sweep --resume

## ===== 实测校准基线（脚本退化时先怀疑自己） =====
# extract_apis 裸片段召回 79/79 (100%)，零噪音，402 文件 / 215 JS
# nav_sweep 覆盖 80/80 路由，859 请求，中途崩 2 次靠 --resume 跑完
# auth_matrix 分类 4/4 正确（guarded / NO_CRED_REQUIRED / TOKEN_NOT_CHECKED+ORACLE / not_found）

## ===== 安全边界 =====
# 短信/验证码类接口一次调用就真下发 -> 只做单次存在性判定，绝不枚举
# 手机号/身份证类免登录接口 -> 只用格式合法但必然无效的测试值
# 登录绕过类（自己凭证+他人手机号）-> 需授权 + 专用测试账号