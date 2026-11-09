---
name: miniprogram-recon
description: 小程序资产侦察：多源枚举 appid（wx+16hex）、不经小程序从 H5 bundle 提接口字典、三条 wxapkg 包获取路径（adb/PC 缓存/抓包下载通道）
whenToUse: 目标有微信小程序、需要圈定 appid 与包体资产时
role: recon
enabled: true
---

# 小程序资产侦察

目标：把"看不见的 App"变成两份可测资产——**appid 清单**（后续 API 测试的身份锚点）和 **wxapkg 包体**（解包审计的原料）。小程序流量默认不可常规抓包，侦察阶段要尽量绕开"装微信+root 手机"这条最贵路径。

## 一、appid 识别与枚举面

appid 格式恒定：`wx` + 16 位小写 hex，可直接正则收割。

| 枚举面 | 位置 | 命令/要点 |
|---|---|---|
| 主站 JS | 页面引用的 JS 里 `wx.config({appId:"wx..."})`、H5 跳小程序的 scheme | `grep -rhoE 'wx[0-9a-f]{16}' js/ \| sort \| uniq -c` |
| 测绘引擎 | body 特征 | FOFA: `body="wx.config" && domain="target.com"`；Hunter 同理 |
| 公众号文章 | 文章内嵌小程序卡片、"阅读原文"、历史推文 | 微信搜一搜公众号历史文章逐篇过，卡片长按可看小程序名 |
| App 内嵌分享 | 同公司 App 的分享面板（打开小程序/H5） | 分享链接里 `weixin://dl/business/?t=*` 与 appid 同源 |
| 小程序聚合搜索 | 微信内"搜一搜"小程序 tab | 按公司名/品牌名/产品名逐个搜，记录 小程序名↔appid↔简介 |
| 应用市场截图 | 应用商店 APK 截图/介绍里的小程序码 | 小程序码解码可得原始 id（低频，备用） |

把每个 appid 记录为 `小程序名 + appid + 关联主体 + 来源`，后面接口测试的 `Referer` 头要用它。

## 二、旁路：不经小程序抓包，从 H5/SPA bundle 提接口字典

很多小程序业务同源一套 H5 站。**不 root、不抓包**，先从生产 bundle 明文提取接口清单，往往够覆盖 80% 接口面：

```bash
BASE=https://h5.target.com/
curl -sk "$BASE" -o index.html
grep -oE '(src|href)="[^"]+\.js[^"]*"' index.html | sed -E 's/.*="([^"]+)".*/\1/' | sort -u > js.list
# 逐个下载 js（含 sourcemap 探测：tail -c 400 | grep sourceMappingURL）
# 三条提取规则，覆盖 AddressUrl+/baseURL+/纯路径字符串 三种写法：
grep -ohE '(AddressUrl|baseURL|apiUrl|API_URL|\$http\.(get|post))\s*[+(]\s*["'"'"'][a-zA-Z0-9_./-]{3,80}' all.js
grep -ohE '["'"'"']/(api|app|v[0-9]|user|member|admin|order|pay|auth|sms|upload)/[a-zA-Z0-9_/-]{2,60}["'"'"']' all.js
# 网关注册前缀（SPA 常把前缀单独写成字符串）：
grep -oE '"/(api|h5|app|gateway|[a-z]{2,12}api)/"' all.js
```

拿到候选路径后做**无 token 复核**（GET 先行，再按前端真实方法 POST `pageNum=1&pageSize=2`，只读接口，命中 1 条即停）。

## 三、wxapkg 获取路径（按成本从低到高）

| 路径 | 位置/操作 | 备注 |
|---|---|---|
| PC 微信缓存 | 新版：`<微信数据目录>/radium/users/<uid>/applet/packages/<appid>/<version>/*.wxapkg`；旧版：`WeChat Files/Applet/<appid>/`；WeChatAppEx 进程目录 | 需先在小程序里打开过；PC 包常为**加密容器**（`V1MMWX` 头），解密需本机 key |
| Windows 自动监控 | e0e1-wx-gui：自动检测运行中的小程序并抓主包/分包、自动反编译 | 覆盖微信 11581–19459；先读其 tools.md 配代理回连 |
| macOS 缓存 | WxLocated：扫描 3.x/4.x 缓存，支持 V1MMWX 加密包解析 | Apple Silicon 原生 |
| Android root | `adb shell 'su -c "find /data/data/com.tencent.mm/MicroMsg/<hash>/appbrand/pkg -name \"*.wxapkg\""'` → `cp` 到 /sdcard → `chmod 644` → `adb pull` | 安卓包为**标准未加密格式**，首选 |
| 抓包下载通道 | 代理 + 证书，过滤 `dldir1.qq.com` 域的 `.wxapkg` 下载响应 | 小程序冷启动/更新时才下发；抓到即原包 |
| 第三方/历史 | 解包站、GitHub 泄露、旧版本存档 | 完整性未知，仅作对照 |

**判定 oracle**：
- 包头首字节 `0xBE` = 标准未加密包，直接解；`56 31 4D 4D 57 58`（"V1MMWX"）= PC 加密容器，先走本机解密；
- 是否拿全分包：解包后 `app.json` 的 `subpackages` 条数 vs 实际解包数，缺分包会漏接口（很多业务全在分包里）。

## 四、授权提示

- wxapkg 属目标知识产权：只在**授权测试环境**内解包分析，产物、密钥、截图一律脱敏，不得传播或复用代码。
- adb 取包只读目标微信数据目录，不做其它数据操作；抓包只记录目标业务流量。
- appid 枚举阶段零交互、无侵入；发现疑似第三方外包公司的关联小程序，先确认归属再测。

## 纪律

1. 仅授权范围：appid 与包体仅用于本项目授权目标的测试，不做横向收集。
2. 证据脱敏：包内真实 appid/密钥/内网域名在报告中以 `wxXXXXXXXXXXXXXXXX`、`<REDACTED>` 占位。
3. 包体与解包产物落盘在 `runs/<project>/` 下，测试结束后按授权约定处置，不入公共仓库。

## 产出约定

- appid 清单、接口字典、包路径按 `redteam_asset_add` 落库：provenance=passive，tool 标明来源（h5-bundle/adb/pc-cache）。
- 侦察阶段发现的可疑泄露（bundle 明文密钥、内网域名）按 `redteam_poc_add` 回填：category=info-leak，language=bash/python，usage 写清复现命令。
- 后续动作衔接：包体 → `miniprogram-unpack`；接口面 → `miniprogram-api-test`；同主体公众号 → `miniprogram-official-account`；云开发配置 → `miniprogram-cloud`。

<!-- 改编自: 本地实战 poc 沉淀 + eeeeeeeeee-code/e0e1-wx, leo9960/wechat-app-unpack, bx33661/miniapp-audit-skills, 2026-10-31 -->
