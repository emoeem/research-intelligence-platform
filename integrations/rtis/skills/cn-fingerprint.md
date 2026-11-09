---
name: cn-fingerprint
description: 国产 OA/中间件/CMS 指纹识别与高危默认路径：致远/通达/万户/泛微/用友/金蝶/蓝凌、Druid/Nacos/XXL-JOB/Apollo/Skywalking/RuoYi/JeecgBoot、富文本编辑器四件套、信息泄露路径（.git/备份/日志/探针）、27,732 SQLi 案例高频参数
whenToUse: 目标是国内资产（中文页面、国产 OA 登录框、.do/.jsp 后缀、政府/国企/教育/医疗背景）时，先跑本字典定位产品与暴露面，再进对应 hunt-* 技能
role: recon
enabled: true
---

# 国产组件指纹 + 高危路径 + 默认口令

> 英文 SRC 字典（SecLists / H1）对国产 OA 覆盖面接近 0。本技能补 CN 战场缺口：怎么认、认出来往哪打、默认口令是什么。

## 1. 国产 OA 指纹表

| 产品 | 根路径 / 登录 | 指纹特征 | 已知 RCE/上传端点 |
|---|---|---|---|
| **致远 OA**（Seeyon） | `/seeyon/`、`/seeyon/main.do` | `Server: SEEYON-OA`、`X-Powered-By: SEEYON`、`<title>致远协同管理软件</title>`、`COMMON.NEED_LOGIN` | `/seeyon/htmlofficeservlet`（A8 RCE）、`/seeyon/thirdpartyController.do`（SSRF） |
| **通达 OA**（Tongda） | `/general/login.php`、`/ispirit/`、`/Pda/` | `<title>通达OA</title>`、`PHPSESSID` | `/ispirit/interface/gateway.php`、`/mobile/auth_mobi.php`（任意用户登录） |
| **万户 ezOffice** | `/defaultroot/login.jsp` | `万户`、`defaultroot` | `/defaultroot/dragpage/upload.jsp`（任意上传+截断） |
| **泛微 e-cology** | `/login/Login.jsp`、`/weaver/` | `e-cology`、`ecology_JSessionId` cookie、`/mobile/` | `/weaver/bsh.servlet.BshServlet`（BeanShell RCE） |
| **用友 NC** | `/nc/`、`/nc/servlet/`、`/portal/` | `<title>用友NC</title>` | — |
| **用友 协作 OA** | `/oaerp/` | `oaerp` | `/oaerp/ui/sync/excelUpload.jsp`（任意上传） |
| **金蝶 GSiS / EAS** | `/kdgs/`、`/eas/` | `KingdeeApp`、`kdgs` | `/kdgs/core/upload/upload.jsp` |
| **蓝凌 LandrayOA** | `/sys/login/login.do`、`/sys/web/index.jsp` | `蓝凌` | — |

## 2. 国产中间件指纹（弱口令/未授权高发）

| 产品 | 路径 | 指纹 |
|---|---|---|
| **Druid**（阿里） | `/druid/index.html`、`/druid/sql.html`、`/druid/weburi.html` | `<title>Druid</title>`；直接访问 `sql.html` 无跳登录 = **未授权** |
| **Nacos** | `/nacos/`、`/nacos/v1/auth/users` | `<title>Nacos</title>` |
| **XXL-JOB** | `/xxl-job-admin/` | `<title>任务调度中心</title>` |
| **Apollo** | `/portal/`、`/eureka/apps` | `apollo`；`/eureka/apps` 返回服务列表 = 未授权 |
| **Skywalking** | `/graphql` | `<title>SkyWalking</title>`；graphql 未授权可读全链路 |
| **Sentinel** | `/resource/machineResource.json` | `sentinel` |
| **Dubbo Admin** | `/dubbo-admin/` | `dubbo` |
| **RuoYi** | `/login`、`/system/user`、`/tool/swagger` | `ruoyi`；**几乎裸奔，必爆破** |
| **JeecgBoot** | `/jeecg-boot/`、`/sys/user` | `jeecg-boot` |
| **DolphinScheduler** | `/dolphinscheduler/ui/` | `dolphinscheduler` |

## 3. 富文本编辑器四件套（占国产上传案例 42%）

| 编辑器 | 占比 | 检测路径 | 上传后落点 |
|---|---|---|---|
| **FCKeditor** | 48% | `/FCKeditor/editor/filemanager/browser/default/connectors/test.html`、`.../jsp/connector` | `/FCKeditor/UserFiles/` |
| **eWebEditor** | 28% | `/ewebeditor/admin/default.jsp`、`/ewebeditor/admin_login.asp`、`/ewebeditor/php/upload.php` | `/ewebeditor/uploadfile/` |
| **UEditor** | 12% | `/ueditor/controller.jsp?action=config`、`/ueditor/jsp/controller.jsp`、`/ueditor/net/controller.ashx` | `/ueditor/php/upload/` |
| **KindEditor** | 8% | `/kindeditor/php/upload_json.php`、`/kindeditor/jsp/upload_json.jsp` | `/kindeditor/attached/` |

> 先 GET `connector` / `controller` 看是否返回配置 JSON —— 返回即未授权可用。

## 4. 信息泄露路径（按 WooYun 命中例数）

```
版本控制（560 例）  /.git/config  /.git/HEAD  /.git/index  /.svn/entries  /.svn/wc.db
备份压缩（530 例）  /wwwroot.rar  /wwwroot.zip  /web.rar  /backup.zip  /site.tar.gz
                   /{域名}.zip  /{域名}.rar  /db.sql.gz  /backup.sql.gz
SQL 备份（136 例）  /backup.sql  /database.sql  /db.sql  /dump.sql  /data.sql
配置备份（101 例）  /config.php.bak  /web.config.bak  /.env.bak
                   /config_global.php.bak  /uc_server/data/config.inc.php.bak
PHP 探针（47 例）   /phpinfo.php  /info.php  /test.php  /1.php  /t.php  /debug.php
日志（23 例）       /ctp.log  /seeyon/logs/ctp.log  /debug.log  /error.log
                   /access.log  /runtime/logs/  /storage/logs/
.NET（36 例）       /web.config  /App_Data/  /connectionStrings.config
```

## 5. 默认口令速查（只查不爆破，命中即停）

| 产品 | 路径 | 凭据 |
|---|---|---|
| 致远 OA | `/seeyon/` | `system/system` |
| 致远 管理台 | `/seeyon/management/index.jsp` | `WLCCYBD@SEEYON` |
| 通达 OA | `/general/login.php` | `admin/admin`、`hr/123456`、`jhadmin/123456` |
| 万户 | `/defaultroot/login.jsp` | `admin/123456` |
| 泛微 | `/login/Login.jsp` | `sysadmin/1`、`admin/123456` |
| 用友 NC | `/nc/` | `system/1` |
| 用友 协作 | `/oaerp/` | `admin/123456` |
| 金蝶 GSiS | `/kdgs/` | `admin/888888` |
| 金蝶 Apusic | `/admin/login` | `apusic/apusic` |
| 蓝凌 | `/sys/login/login.do` | `sysadmin/landray`、`admin/admin` |
| Druid | `/druid/login.html` | `admin/admin` |
| Nacos | `/nacos/` | `nacos/nacos` |
| XXL-JOB | `/xxl-job-admin/` | `admin/123456` |
| RuoYi | `/login` | `admin/admin123` |
| JeecgBoot | `/login` | `jeecg/jeecg`、`admin/123456` |
| DolphinScheduler | `/dolphinscheduler/ui/` | `admin/dolphinscheduler123` |

## 6. 一把梭探测脚本

```bash
# 指纹 + 泄露 + 编辑器 三组并扫（速率 ≤5 rps）
for path in /seeyon/ /general/login.php /defaultroot/login.jsp /login/Login.jsp \
            /oaerp/ /kdgs/ /sys/login/login.do /druid/index.html /nacos/ \
            /xxl-job-admin/ /jeecg-boot/ /admin/ ; do
  curl -s -o /dev/null -w "%{http_code} $path\n" "http://TARGET$path"
done
```

```bash
for path in /.git/config /.svn/entries /wwwroot.rar /backup.sql \
            /config.php.bak /phpinfo.php /web.config.bak /ctp.log ; do
  curl -s -o /dev/null -w "%{http_code} $path\n" "http://TARGET$path"
done
```

## 7. 参数名字典（27,732 SQLi + 业务案例提炼）

```
通用     file path name url src dir folder id uid cid pid tid
下载     download down attachment attach doc
读取     read load get fetch open input
文件     filename filepath fname fn resource
模板     template tpl page include temp
复合     ?path=&name=  ?filePath=&fileName=  ?FileUrl=&FileName=  ?file=&showname=
业务     orderId orderNo accountId userId roleId orgId deptId code token sign
```

## 8. 判定纪律

- **指纹 ≠ 漏洞**。识别出致远 OA 不等于能打 RCE，必须走 `hunt-rce` / `hunt-file-upload` 走完证据链。
- 未授权判定看**是否跳到登录页**，不看 HTTP 码（200 返回登录 HTML 也是未授权）。
- 默认口令**只试表中列出的组合，每组 1 次**，不做字典爆破。
- 落库：`fingerprint` 表写 vendor/product/version/evidence；泄露类进 `redteam_vuln_add`。

<!-- 改编自: src-hunter references/dictionaries/chinese-srcfingerprints.md (515 行) + default-credentials-cn.md (274 行), 2026-11-01 -->
