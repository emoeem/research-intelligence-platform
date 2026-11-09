#!/bin/bash
# K3Cloud ServiceFacade 服务存在性差分探测
# 用途：不登录、不发 payload，仅用两种错误格式判断某个 ServicesStub 服务是否存在
# 实测于 asintec.ik3cloud.com (K3Cloud 8.2.886.8)，2026-11-03

H="$1"   # https://<tenant>.ik3cloud.com
[ -z "$H" ] && { echo "用法: $0 https://asintec.ik3cloud.com <service1> <service2> ..."; exit 1; }
shift

BASE="/K3Cloud/Kingdee.BOS.ServiceFacade.ServicesStub"
for svc in "$@"; do
  body=$(curl -sk "$H$BASE.$svc.common.kdsvc" --max-time 8)
  case "$body" in
    *"N001: Unexpectable request"*)
      echo "[EXISTS?]  $svc  -> N001 (请求校验器拦截，服务名可能有效)" ;;
    *"no found"*)
      echo "[ABSENT]   $svc  -> no found (服务不存在)" ;;
    "")
      echo "[EMPTY]    $svc  -> 空响应" ;;
    *)
      echo "[OTHER]    $svc  -> $(echo "$body" | head -c 80)" ;;
  esac
  sleep 1
done

# 判定矩阵（实测）：
#   Account.AuthService.validateUser          -> N001
#   DevReportService.GetBusinessObjectData    -> N001
#   Nonexist.FakeService.FakeMethod           -> N001
#   User.UserService.GetUserInfo              -> no found
# 注意：N001 同时出现在「确定存在」和「确定不存在」的服务上，且对 benign body
#       `{"ap0":"test","format":"3"}` 与真实反序列化 payload 响应一致 ——
#       说明 N001 是 body 解析之前的统一请求校验器返回，**不能作为服务存在性判据**。
#       本脚本的差分只能证明「同一部署内有两条错误码路径」，不能证明服务存在。
#       真正的存在性判据：需要正确请求格式，或用 /K3Cloud/ 之外的部署形态。
