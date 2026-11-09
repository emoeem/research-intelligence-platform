---
name: miniprogram-unpack
description: wxapkg 解包与静态分析工具链：unveilr 主链、PC 端 V1MMWX 加密包判定、主包/分包完整性校验、解包产物六类敏感信息 grep 清单
whenToUse: 拿到 wxapkg 包体后做解包审计；分包缺漏判定与硬编码提取
role: vuln-scan
enabled: true
---

# wxapkg 解包与静态分析

## 解包工具链（按场景选）

| 场景 | 工具 | 用法 |
|---|---|---|
| 标准包（头 `0xBE`，安卓/抓包所得） | unveilr（Rust，首选） | `unveilr main.wxapkg`（cargo install unveilr 或 npm 镜像 wxappUnpacker） |
| 无 unveilr 环境 | 纯 python 拆段 | 见知识库模板 `小程序-wxapkg-全自动解包…/extract.py`（按 v1 头拆原始文件段，不做 wxss/wxml 还原） |
| PC 加密容器（头 `56 31 4D 4D 57 58` = "V1MMWX"） | unveilr 支持自动解密 | 需在取包那台机器上跑（解密 key 派生自本机 salt，见下） |
| 老格式/还原要求高 | wxappUnpacker（node） | `node wuWxapkg.py main.wxapkg`，子模块分别还原 wxml/wxss/js |

**PC 加密包要点**：`V1MMWX` 容器 = AES-EBT 前段 + XOR 后段，解密 key 由 wxid 派生；在原机器上跑 unveilr 会自动读本机微信目录取参数。跨机器解密需要先把 `<微信数据目录>/radium/users/<uid>/` 相关 salt 文件一并带走。

## 主包/分包完整性校验（必做）

```bash
# 解包后第一件事：数分包
python3 - <<'PY'
import json,glob
app=json.load(open(glob.glob('**/app.json',recursive=True)[0],encoding='utf-8'))
subs=app.get('subpackages') or app.get('subPackages') or []
print('主包页面:',len(app.get('pages',[])),'分包数:',len(subs))
for s in subs: print(' 分包root:',s.get('root'),'页面数:',len(s.get('pages',[])))
PY
# 实际解出的目录数 < app.json 分包数 ⇒ 有分包没拿到，回 recon 阶段补包（很多业务全在分包里）
```

## 解包产物六类 grep 清单

```bash
D=<解包输出目录>
# 1. appid（后续 Referer 头要用）
grep -rhoE 'wx[0-9a-f]{16}' "$D" | sort | uniq -c
# 2. appsecret / 密钥形态（-secret 后跟 32hex）
grep -rniE "(app_?secret|secret_?key)[\"' :=]{1,4}[0-9a-f]{32}" "$D"
# 3. 云 AK（阿里 LTAI / 腾讯 AKID）
grep -rnoE 'LTAI[0-9A-Za-z]{12,28}|AKID[0-9A-Za-z]{13,20}' "$D"
# 4. 加密 key 上下文（衔接知识库 wxapkg-CryptoJS-AES poc 的定位法）
grep -rniE "CryptoJS|enc\.Utf8\.parse|AES\.(encrypt|decrypt)|setKey|signStr" "$D" | head -40
# 5. 内网/环境域名
grep -rnohE 'https?://[a-z0-9.-]*(dev|test|uat|internal|intra)[a-z0-9.-]*' "$D" | sort -u
grep -rnohE 'https?://(10\.[0-9.]+|172\.(1[6-9]|2[0-9]|3[01])\.[0-9.]+|192\.168\.[0-9.]+)' "$D" | sort -u
# 6. 接口全集（正则见 miniprogram-recon；解包后的 js 比线上 bundle 更全）
grep -rnohE '"/(api|app|gateway|v[0-9])/[A-Za-z0-9_/-]{2,80}"' "$D" | sort -u > apis.txt
```

第三方 SDK key 一并过：地图（腾讯/高德 key）、推送（个推/极光 appkey）、统计（友盟）、支付（mch_id + 商户 key 路径）。

## WMPF 运行时补充（静态盲区）

静态看不到的路由/运行时行为，走 WMPF CDP 桥接 + Frida 注入采集（知识库 poc `wmpf-小程序-cdp-桥接-frida-注入…`）：`--remote-debugging-port` 挂上后 DOM/网络面板直接看，弥补"分包加密、接口拼装在运行时"的盲区。

## 纪律

- 解包产物含真实用户数据样例时（ wedding 类小程序常见缓存 PII），只在本地分析，报告与回填一律脱敏
- 知识库回填：`redteam_poc_add` category=info-leak，content 放命中清单节选（key 脱敏为 `***`）

## 产出约定

解包+grep 产出清单后：可执行的判定方法（如密钥解密链、签名复现）按实战 poc 格式回填；纯配置类发现记 engagement 的 assets.db vuln 表。

<!-- 改编自: 本地实战 poc（wxapkg-CryptoJS-AES、wmpf-CDP）+ leo9960/wechat-app-unpack、threecha/wxappUnpacker、wux1an/unveilr 文档, 2026-10-31 -->
