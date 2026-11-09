---
name: miniprogram-cloud
description: 微信云开发/云托管攻击面：TCB env id 泄露定位、匿名登录开启判定、云数据库/云存储匿名读、云函数名枚举与越权调用、云托管公网面
whenToUse: 解包产物发现 cloudbase/tcb 痕迹、目标使用微信云开发时
role: vuln-scan
enabled: true
---

# 微信云开发（TCB）攻击面

## 一、env id 泄露定位（入口）

解包产物里搜（miniprogram-unpack 的 grep 流程加这几条）：

```bash
grep -rniE "cloudbase|tcb\.|wx\.cloud\.init|env[\"']?\s*[:=]\s*[\"'][a-z0-9-]{8,}" <解包目录> | head -30
# 典型形态：wx.cloud.init({ env: "env-xxx-1234567890" 或自建 envId "prod-xxx" })
```

- 小程序侧调用痕迹：`wx.cloud.callFunction` / `wx.cloud.database()` / `wx.cloud.downloadFile`
- H5 侧（Web SDK）痕迹：`@cloudbase/js-sdk`、`init({ env: '...' })`——H5 通道通常配匿名登录，是最常见入口

## 二、匿名登录判定（核心 oracle）

```bash
# 用 @cloudbase/js-sdk 直连（node 环境）
node -e "
const tcb = require('@cloudbase/js-sdk');
const app = tcb.init({ env: '<envId>' });
const auth = app.auth({ persistence: 'local' });
auth.signInAnonymously().then(()=>console.log('匿名登录=开启')).catch(e=>console.log('判定:', e.code||e.message));
"
```

| 返回 | 判定 |
|---|---|
| 登录成功 | 匿名登录开启 → 直接测数据库/存储匿名读 |
| `INVALID_PARAMS` / 关闭提示 | 匿名关闭 → 需要 secret（转 cloud-ak-sk 的 secretKey 泄露面） |

## 三、匿名登录后的资源面

| 面 | 判定命令要点 | 危害 |
|---|---|---|
| 云数据库 | `app.database().collection('<名>').limit(3).get()`——集合名从解包 JS 的调用处提取 | 匿名读业务数据（PII）= 高危 |
| 云存储 | `app.getFileInfo` / downloadFile 配合解包 JS 里的 cloud:// 路径 | 文件桶匿名读 |
| 云函数 | 函数名从 `callFunction({ name: '<fn>' })` 提取，逐个匿名 invoke | 越权调用内部函数（改参直连数据库） |

云函数 invoke 越权是价值最高点：函数内部往往用管理员身份直连数据库，前端传参即 SQL/查询条件 → 改参翻别人的数据（衔接四态矩阵 C/D 态打法）。

## 四、云托管（CloudBase Run）公网面

- `@cloudbase/js-sdk` 的 `callContainer` 痕迹 → 云托管服务有公网默认域名 `<svc>-<envId>.tcloudbase.com`（或绑定域），对域名打常规 Web 未授权面（衔接 hunt-api-misconfig / api-docs 判定法）
- 内部服务误暴露公网：云托管默认内网访问的服务若被开了 PUBLIC → 直接打

## 五、衔接 COS/OSS 泄露面

云开发底层桶与独立 COS 同判（知识库已有 3 条 COS poc：零凭据枚举三段式、CNAME 反查、只读审计）：`cloud://` 转换的 `https://<envId>.tcloudbaseapp.com/<path>` 与 `<bucket>.cos.<region>.myqcloud.com` 两条通道都过一遍匿名读。

## 纪律

- 匿名读只 `limit(3)` 取样证明可读即停，不翻全量
- 云函数 invoke 只读函数（list/get 类），写操作函数证明到参数校验层即停
- 回填：category=unauthorized，component=TCB/微信云托管；env id 脱敏为 `env-****`

<!-- 改编自: 微信云开发公开文档 + @cloudbase/js-sdk 用法 + 本地 COS poc 经验外推, 2026-10-31 -->
