---
name: miniprogram-api-test
description: 小程序 API 测试方法论：合法请求头构造（Referer/UA）、四态认证矩阵、加密参数降级与签名复刻、分包接口与隐藏面巡检
whenToUse: 圈定 appid 与接口清单后做后端接口授权测试；加密/签名通道的接口复现
role: vuln-scan
enabled: true
---

# 小程序 API 测试

## 一、合法请求头（后端校验才放行的场景）

```http
POST /api/xxx HTTP/1.1
Host: api.target.com
Referer: https://servicewechat.com/{appid}/{version}/page-frame.html
User-Agent: Mozilla/5.0 (Linux; Android 12) AppleWebKit/537.36 MicroMessenger/8.0.49.2600(0x28003135) XWEB/1160045
Content-Type: application/json
```

- `appid` 用侦察阶段圈定的真实值；`version` 随便填编译版本号（多数后端不校验）
- 网关侧常见校验组合：只查 Referer 前缀 / 只查 UA 含 MicroMessenger / 都不查——**三者各测一遍**，宽松的那个就是测试通道

## 二、四态认证矩阵（本地实战 poc 收口的核心打法）

对接口清单逐条打四态，直接判定授权缺口：

| 态 | 请求形态 | 判定 |
|---|---|---|
| A 无凭证 | 不带任何 token/uid | 200 带业务数据 = 未授权 |
| B 伪造凭证 | `Authorization: Bearer xxx` / `token=test` 随机串 | 不校验有效性 = 未授权 |
| C 他人凭证 | 测试账号 A 的 token + 改包体里的 userId/openid/orderId | 返回 B 的数据 = 水平越权 |
| D 篡改参数 | 只改数字 id±1、手机号、金额字段 | 数据串号 = 越权/业务逻辑 |

执行纪律：A/B 态全量巡（低成本高命中），C/D 态只在自建双账号间做；命中 1 条即停手取证，不全量拖数据。

## 三、加密/签名通道复刻（本库最强经验的接口化）

静态解包拿到算法后，接口测试的三条复刻路径（细节见知识库对应 poc）：

1. **加密降级**：请求/响应加密版本字段（如 `WxEncryptVersion`）改成明文版本号原样重放——服务端兼容旧版本时直接拿明文
2. **签名离线复现**：`sign=base64(HmacSHA1(signStr, secret))` 类，secret 来自解包 `config` 或 `get_sign` 的 data 字段——写个 15 行 python 按字典序拼参数即可批量造合法签名
3. **硬编码 AES**：CryptoJS 硬编码 key 定位后，字段级解密请求体→改参→重加密回传（复现判据见 poc：能解就能重加密，重加密成功 = 通道完全可控）

## 四、分包接口与隐藏面

- 分包里的接口主包 JS 不含——必须解包全部分包再提接口全集（完整性校验见 miniprogram-unpack）
- `app.json` 未声明但 JS 里出现的路径（灰度/内部接口）：单独列一张表测 A 态
- websocket 通道（`wss://`）：首帧协议还原后测鉴权帧伪造（参考 poc `微信小程序-chat-ws-协议还原…` 的 VTCLP 帧 bind 鉴权判定）

## 纪律

- 全程自建测试账号；不碰真实用户数据，越权取证只拉自己两个账号的数据对
- 加密通道复现脚本放 engagement 的 attack-files/，回填 `redteam_poc_add`：category=unauthorized（认证缺口）或 auth-bypass（签名/加密绕过），usage 写清请求头模板与复现命令

## 产出约定

四态矩阵结果按接口回填 vuln 表；整条流水线（静态全集→四态→路由巡）已有模板 poc（id=85），新目标直接复用并 `hit_count` 递增。

<!-- 改编自: 本地实战 poc（接口面全流程审计流水线、WxEncryptVersion 降级、签名链离线复现、chat-ws 协议还原）, 2026-10-31 -->
