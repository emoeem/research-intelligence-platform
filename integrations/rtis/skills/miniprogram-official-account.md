---
name: miniprogram-official-account
description: 公众号攻击面：appid/appsecret 泄露→access_token 链路、IP 白名单判定、可调用接口矩阵（素材/客服/用户/群发）、oauth redirect_uri 滥用、JS-SDK 签名泄露钓鱼
whenToUse: 目标运营微信公众号；发现 appid/secret 泄露线索或网页授权面时
role: vuln-scan
enabled: true
---

# 公众号攻击面

## 一、appsecret 泄露面（入口）

| 泄露源 | 判定 |
|---|---|
| 解包产物/前端 JS | miniprogram-unpack 的 grep 清单直接命中 |
| GitHub/gitee 搜索 | `appsecret` + 目标域名、`"wx"+16hex` + `secret`（授权范围内搜，配 GitHub API） |
| 教程/文档/PPT | 技术分享截图里的配置面板、运维手册 |
| 历史接口回显 | 老版本接口把配置项原样返回（衔接 hunt-source-leak 的经验） |

## 二、access_token 链路（危害证明路径）

```
appid + appsecret → GET https://api.weixin.qq.com/cgi-bin/token?grant_type=client_credential&appid=X&secret=Y
  → {"access_token":"...","expires_in":7200}   # 成功 = 泄露坐实
  → errcode 40164 "invalid ip, not in whitelist"  # IP 白名单在挡
```

- **errcode 40164 判定价值**：报错本身就泄露了"调用方 IP 不在白名单"，说明 secret 正确、只差 IP——配合目标侧 SSRF（经其服务器代调，出网 IP 在白名单内）或云函数中转即可闭环，链路价值升级为高危
- 白名单为空/未配置的老号：直接全接口可调

## 三、token 可调接口矩阵（按危害排序，全部只读优先）

| 接口族 | 动作 | 危害 |
|---|---|---|
| 用户管理 | `GET /cgi-bin/user/get?access_token=X&next_openid=` | 全量关注者 openid 清单（PII 面） |
| 用户信息 | `POST /cgi-bin/user/info/batchget` | 昵称/头像/地区/性别 |
| 素材管理 | `GET /cgi-bin/material/batchget_material` | 素材库（含历史推文、图片） |
| 客服接口 | `POST /cgi-bin/message/custom/send` | 冒充客服发消息（钓鱼面，需授权后演示） |
| 群发接口 | `POST /cgi-bin/message/mass/sendall` | **全量粉丝群发**（严重级；验证只对测试号演示能力，不真发） |
| 模板消息 | `GET /cgi-bin/template/get_all_private_template` | 模板清单→可定向触达 |

证明纪律：群发能力只证明"接口返回 0/参数校验通过到发送前一步"，**绝不真发**；用户信息只拉自己控制的测试号。

## 四、网页授权（oauth）面

- `redirect_uri` 参数化：授权回调链接 `https://open.weixin.qq.com/connect/oauth2/authorize?appid=X&redirect_uri=<URL>` 中 redirect_uri 若不校验域名白名单（依赖后端），可构造任意回跳 → 授权 code 劫持
- scope 提权：`snsapi_base`（仅 openid） vs `snsapi_userinfo`（资料）——用 userinfo 链接诱导授权属于社工面，仅授权演练
- 回调域 SSRF：公众号服务器配置 URL 校验（接口推送 token 验证）时，若目标平台把验证 URL 用户可控 → 服务端代请求（衔接 hunt-ssrf）

## 五、JS-SDK 签名泄露

- 前端接口下发 `signature/jsapi_ticket/timestamp` → 用官方校验工具验签通过 = 可伪造任意页面的 wx.config → 自定义分享标题/描述/图（钓鱼）。签名接口本身若不需要登录即可取（常见于"分享即领"类 H5）→ 未授权伪造分享
- 判定：拿到 signature 后 `curl` 微信 js 签名校验接口或本地 sha1(jsticket+noncestr+timestamp+url) 复算比对

## 纪律

- 拿到 token 后**只调只读接口**（user/get、material/batchget 计数级证明即停）
- 群发/客服类只做"参数构造到位"的能力证明；真实触达用户 = 红线
- 回填：category=info-leak（secret 泄露）或 unauthorized（token 可调接口），component=微信公众号，severity 按 token 能力矩阵定级

<!-- 改编自: 公开微信开放平台文档 + 本地小程序实战经验外推, 2026-10-31 -->
