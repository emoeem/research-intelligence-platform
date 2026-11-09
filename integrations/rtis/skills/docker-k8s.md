---
name: docker-k8s
description: 容器编排未授权面：Docker 2375/2376 API、registry 5000、K8s 6443/10250/10255/etcd 2379、SA token 泄露→RBAC 判定、dashboard 匿名；全 curl 化判定
whenToUse: 摸到容器化资产（测绘 service:docker/k8s、运维段端口、内网 2xxxx 端口）时
role: vuln-scan
enabled: true
---

# Docker / Kubernetes 未授权面

## 一、Docker

```bash
# 2375 未授权 API（无 TLS）——宿主 root 等价面
curl -s http://TARGET:2375/version | jq -r .ApiVersion     # 有版本 JSON = 未授权确认
curl -s http://TARGET:2375/containers/json | jq 'length'   # 容器清单（只读）
# 利用概念（需单独授权）：POST /containers/create 挂载 / 后 exec —— 等价宿主 root
# 2376：TLS 端口，无证书连接报 TLS 错误 = 端口在，找证书泄露面（/etc/docker/ 目录类）
# 5000 registry 匿名：
curl -s http://TARGET:5000/v2/_catalog | jq .               # 仓库清单
curl -s http://TARGET:5000/v2/<repo>/tags/list              # tag → manifests → 层拉取（镜像里常有源码/凭据）
```

批量脚本：知识库模板 poc `容器编排未授权面批量检查…/audit.sh`（七端口三态矩阵）。

## 二、Kubernetes

```bash
# 6443 API server 匿名
curl -sk https://TARGET:6443/api | jq -r .kind              # APIVersions = 匿名可列（高危）
curl -sk https://TARGET:6443/api/v1/namespaces              # 403 带 system:anonymous = 面在 RBAC 挡
# kubelet 10250 / 只读口 10255
curl -sk https://TARGET:10250/pods | jq -r .kind            # PodList = kubelet 匿名读
curl -sk https://TARGET:10250/runningpods/                  # 运行中 pod（exec 面）
curl -s  http://TARGET:10255/pods                            # 只读口未关
# etcd 2379 无认证
curl -s http://TARGET:2379/version                          # etcdserver 版本 = 暴露
curl -s http://TARGET:2379/v2/keys?recursive=true | head -c 500   # 键值 dump = 全集群 secrets
# dashboard（常在 30000+ NodePort 或集群内）
curl -sk https://TARGET:<port>/api/v1/namespaces/kubernetes-dashboard/... # 匿名可进 = 高危
```

## 三、拿到 Pod 内权限后的判定链（SA token）

```bash
# pod 内：/var/run/secrets/kubernetes.io/serviceaccount/ 下 token + ca.crt + namespace
TOKEN=$(cat /var/run/secrets/kubernetes.io/serviceaccount/token)
curl -sk -H "Authorization: Bearer $TOKEN" https://$KUBERNETES_SERVICE_HOST:443/version
# RBAC 边界判定（只查不写）：
kubectl auth can-i --list                                   # 当前 SA 能做什么
kubectl auth can-i create pods                              # create pods = 可接管（判定即停）
kubectl auth can-i list secrets --all-namespaces            # secrets 可列 = 全集群凭据面
```

- `list secrets` 可列 → 读 secrets 拿数据库串/下一层 AK（只读证明 1-2 个即停）
- `create pods` → 挂载主机路径的 pod = 节点 root（概念证明，需授权）

## 纪律

- 全流程只读：GET 类 curl；任何 POST/exec/apply 属利用升级，需单独授权并记录
- etcd dump 只 head 取样证明可读，不落全量（全集群 secret，落盘即风险）
- 回填：category=unauthorized，component=Docker/Kubernetes，tags 带端口号方便复用

<!-- 改编自: carlospolop/hacktricks(2375/5000 pentesting docker) + K8s 官方文档 RBAC 章节, 2026-10-31 -->
