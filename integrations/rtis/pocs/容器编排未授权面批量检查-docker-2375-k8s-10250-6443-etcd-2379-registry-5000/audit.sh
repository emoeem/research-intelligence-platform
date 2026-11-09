#!/usr/bin/env bash
# 容器编排未授权面批量检查（模板，未针对任何真实目标验证）。
#
# 判据（全部为只读 GET/版本探测，不做任何写操作）：
#   Docker API   2375  GET /version          → JSON 带 ApiVersion 且无认证 = 未授权 API（可 POST /containers/create 挂 / 拿宿主 root，验证需另行授权）
#   Docker TLS   2376  GET /version（证书缺失时报 TLS 错误 = 端口在，配置自判）
#   Kubelet 只读 10255 GET /pods            → 返回 pod JSON = 只读口未关
#   Kubelet API  10250 GET /pods            → 401/403=正常；200=匿名可读；/runningpods 同判
#   K8s API      6443 GET /api              → 403 且 kind:"Status" 带 system:anonymous = 匿名可达（RBAC 在挡，面在）；200 = 匿名可列
#   etcd         2379 GET /version          → {"etcdserver":"x"} = 端口暴露；再 GET /v2/keys?recursive=true 出键值 = 无认证 dump
#   Registry     5000 GET /v2/_catalog      → {"repositories":[...]} = 匿名可列仓库
#   Dashboard    30000+随机 或 kubectl proxy 场景：GET /api/v1/namespaces/kubernetes-dashboard/services 判（需先有 6443 权）
#
# 用法：
#   ./audit.sh <target_host[:port]> [more_targets...]        # 单个/多个目标
#   cat targets.txt | ./audit.sh -                            # 从 stdin 批量（每行一个 host[:port]）
#   端口省略时对每个 host 扫默认 7 个端口（2375/2376/10250/10255/6443/2379/5000）
#
# 依赖：curl、timeout（coreutils）
#
# 纪律：
#   - 仅授权范围内使用；本脚本全部只读，验证"可挂载宿主根"需单独授权且不落持久化容器
#   - 结果按 PASS(正常)/OPEN(面在)/AUTHBYPASS(未授权可读) 记录，回填 knowledge.db：category=unauthorized
set -u
PORTS_DEFAULT="2375 2376 10250 10255 6443 2379 5000"

probe() { # $1=url  $2=特征判定关键字（grep -E）
  local url="$1" expect="$2" body code
  body=$(timeout 6 curl -sk -m 6 -w '\n%{http_code}' "$url" 2>/dev/null) || { echo "CLOSED"; return; }
  code="${body##*$'\n'}"; body="${body%$'\n'*}"
  if echo "$body" | grep -qE "$expect"; then echo "AUTHBYPASS|$code|$(echo "$body" | head -c 120 | tr '\n' ' ')";
  elif [[ "$code" == "000" ]]; then echo "CLOSED";
  else echo "OPEN|$code|$(echo "$body" | head -c 120 | tr '\n' ' ')"; fi
}

one_target_port() { # $1=host $2=port
  local host="$1" port="$2" v
  case "$port" in
    2375) v=$(probe "http://$host:$port/version" '"ApiVersion"|"Version"') ;;
    2376) v=$(probe "https://$host:$port/version" '"ApiVersion"') ;;
    10250) v=$(probe "https://$host:$port/pods" '"kind"[: ]*"PodList"') ;;
    10255) v=$(probe "http://$host:$port/pods" '"kind"[: ]*"PodList"') ;;
    6443)  v=$(probe "https://$host:$port/api" '"Status"|"APIVersions"') ;;
    2379)  v=$(probe "http://$host:$port/version" 'etcdserver') ;;
    5000)  v=$(probe "http://$host:$port/v2/_catalog" '"repositories"') ;;
    *) return ;;
  esac
  printf '%-4s %-28s %s\n' "$port" "$host" "$v"
}

if [[ "${1:-}" == "-" ]]; then
  mapfile -t HOSTS
else
  HOSTS=("$@")
fi
[[ ${#HOSTS[@]} -eq 0 ]] && { echo "用法: $0 <host[:port]>... 或 - <targets.txt" >&2; exit 1; }

for h in "${HOSTS[@]}"; do
  if [[ "$h" == *:* ]]; then one_target_port "${h%%:*}" "${h##*:}"; else
    for p in $PORTS_DEFAULT; do one_target_port "$h" "$p"; done
  fi
done | tee "container-audit-$(date +%Y%m%d-%H%M%S).log" | grep -E "AUTHBYPASS" && echo "↑ 存在未授权可读面，人工复核后回填" || echo "无 AUTHBYPASS 命中"
