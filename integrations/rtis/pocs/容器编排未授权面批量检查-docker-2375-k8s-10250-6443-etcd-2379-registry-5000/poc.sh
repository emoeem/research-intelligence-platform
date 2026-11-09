#!/usr/bin/env bash
# ============================================================================
# 容器编排未授权面批量检查
# 用法: ./poc.sh 10.0.0.5
#       ./poc.sh hostA,hostB,10.0.0.0/24
#
# 覆盖: Docker 2375/2376 · K8s kubelet 10250/10255 · apiserver 6443/8080
#       etcd 2379 · registry 5000 · containerd 2376 · crio 10000
#       Jenkins/K8s dashboard/Argo/Rancher
#
# 【判据】为什么不能只看端口通
#   这几个服务的未授权判定必须看到「结构化响应」：
#     Docker   → /version 返回 JSON 含 "ApiVersion"
#     kubelet  → /pods 返回 {"kind":"PodList"}（10255 只读，10250 需证书）
#     apiserver→ /version 返回 gitVersion
#     etcd     → /version 返回 etcdserver
#     registry → /v2/ 返回 200 空 body 或 401 带 Www-Authenticate
#   只通端口可能是别的东西（反代、蜜罐、错误配置的 nginx）。
# ============================================================================
set -uo pipefail
source "$(dirname "$0")/../_lib/common.sh"

TARGETS="${1:-}"
[ -z "$TARGETS" ] && usage_die "$0 <host[,host2|CIDR]>"
: "${CT:=3}"

# 探针表: name|port|path|expect_regex|danger
PROBES=(
  'docker-api|2375|/version|"ApiVersion"|CRIT 可起容器→宿主 root'
  'docker-api-tls|2376|/version|"ApiVersion"|CRIT 同上（TLS 但未鉴权）'
  'kubelet-ro|10255|/pods|"kind"|HIGH 可读全部 Pod 定义（含 env 里的密钥）'
  'kubelet-rw|10250|/pods|"kind"|CRIT 可 exec 进容器'
  'kube-apiserver|6443|/version|gitVersion|CRIT 全集群控制'
  'kube-apiserver-insec|8080|/version|gitVersion|CRIT 无鉴权 apiserver'
  'etcd|2379|/version|etcdserver|CRIT 直读全部 Secret'
  'registry|5000|/v2/_catalog|"repositories"|HIGH 拉取私有镜像（常含凭据）'
  'containerd|2376|/v1/version|"version"|CRIT'
  'crio|10000|/info|"storage"|HIGH'
  'k8s-dashboard|30000|/api/v1/namespaces|"kind"|CRIT'
  'rancher|80|/v3-public/settings|"data"|MED'
  'argo|2746|/api/v1/session|"loggedIn"|MED'
)

echo "目标: $TARGETS"
echo

check_host() {
  local h="$1"
  echo "════ $h ════"
  for row in "${PROBES[@]}"; do
    IFS='|' read -r name port path expect danger <<<"$row"
    # 端口是否开（用 bash /dev/tcp，比 nc 依赖少）
    timeout "$CT" bash -c "echo >/dev/tcp/$h/$port" 2>/dev/null || { dim "$name :$port 端口未开"; continue; }

    local scheme=http
    case "$port" in 2376|6443|10250) scheme=https ;; esac
    r=$(reqc GET "$scheme://$h:$port$path")
    local c="${r%%$'\t'*}"; local b="${r#*$'\t'}"

    if printf '%s' "$b" | grep -qE "$expect"; then
      ok "$name :$port $path → $c  ⚠ $danger"
      printf '%s' "$b" | head -c 260 | tr -d '\n'; echo
    elif [ "$c" = "401" ] || [ "$c" = "403" ]; then
      dim "$name :$port → $c 需鉴权（存在但安全）"
    elif [ "$c" = "000" ]; then
      dim "$name :$port TLS/协议不匹配"
    else
      dim "$name :$port → $c（无特征响应）"
    fi
    sleep 0.3
  done
  echo
}

for t in ${TARGETS//,/ }; do
  if printf '%s' "$t" | grep -q '/'; then
    # CIDR：只做端口发现，不做协议判定（避免扫描噪音过大）
    log "CIDR 模式: $t —— 先端口扫描，再挑出开放主机做协议判定"
    command -v masscan >/dev/null || command -v naabu >/dev/null || {
      err "需要 masscan 或 naabu"; continue; }
    tmp=$(mktemp)
    if command -v naabu >/dev/null; then
      naabu -host "$t" -p 2375,2376,6443,8080,10250,10255,2379,5000,10000,30000 -silent -o "$tmp" 2>/dev/null
    else
      masscan "$t" -p2375,2376,6443,8080,10250,10255,2379,5000 -oL "$tmp" --rate 1000 >/dev/null 2>&1
    fi
    while read -r ip _port _rest; do
      [ -n "$ip" ] && [ "$ip" != "#" ] && echo "$ip"
    done < "$tmp" | sort -u | while read -r ip; do check_host "$ip"; done
    rm -f "$tmp"
  else
    check_host "$t"
  fi
done

cat <<'EOF'
════ 判定口径 ════
  只有「端口通 + 返回体含特征字段」才算未授权，缺一不可。
  仅端口通 → 可能是反代/蜜罐/别的服务，不下结论。

════ 命中后的只读确认（不自动执行）════
  Docker   : curl -s http://H:2375/containers/json | jq length
             curl -s http://H:2375/info | jq '.DockerRootDir,.ServerVersion'
  etcd     : curl -s http://H:2379/v2/keys/?recursive=true | head -c 2000
             ETCDCTL_API=3 etcdctl --endpoints=http://H:2379 get / --prefix --keys-only
  kubelet  : curl -sk https://H:10255/pods | jq '.items[].spec.containers[].env'
  registry : curl -s http://H:5000/v2/_catalog | jq .
             curl -s http://H:5000/v2/<repo>/tags/list | jq .
  k8s api  : curl -sk https://H:6443/version -k
             kubectl --server=https://H:6443 --insecure-skip-tls-verify get pods -A

════ 注意 ════
  kubelet 10250 的 exec 接口是写操作（会在容器内执行命令），
  etcd 的 delete/put 会破坏集群。以上全部只读，先取证再决定下一步。
EOF
