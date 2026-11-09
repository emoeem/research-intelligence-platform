---
name: hunt-file-upload
description: 文件上传挖掘：五层校验绕过矩阵（前端/MIME/魔数/后缀/内容检测）、中间件解析漏洞、.htaccess/.user.ini、OSS 云直传判定、无害证明法
whenToUse: 任意上传功能（头像/附件/导入/富文本图片/OSS 直传签名）的专项测试
role: vuln-scan
enabled: true
---

# 文件上传

## 五层校验绕过矩阵（逐层探测，记录挡在哪一层）

| 层 | 校验手段 | 绕过 |
|---|---|---|
| 1 前端 JS | 扩展名白名单 | 直接 curl/burp 重放请求，忽略前端 |
| 2 Content-Type | `image/png` 校验 | 改 MIME 保后缀（`Content-Type: image/png` + `shell.php`） |
| 3 魔数 | 文件头 `89504E47` | 图片马：`GIF89a` 头 / 真 PNG 前缀拼接 |
| 4 后缀 | 黑名单/白名单 | 黑名单：`php5/phtml/Php/PHP.`/`php .`/`%00` 截断（老版）；白名单：解析漏洞组合（下） |
| 5 内容检测 | 关键字查杀 | 一句话变形、加密 webshell（授权后）、图片二次渲染绕过（GIF 无损区） |

## 中间件解析漏洞组合（白名单下的出路）

| 中间件 | 打法 |
|---|---|
| IIS 6.0 | `shell.asp;.jpg` 分号截断；`/shell.asp/1.jpg` 目录解析 |
| IIS 7.5 / Nginx fastcgi | `upload/img.jpg/.php`（cgi.fix_pathinfo=1 时）URL 级解析 |
| Apache | `shell.php.jpg` 多后缀从右往左遇到不认识即按 php（AddHandler 配置时）；`.htaccess` 可上传时 `AddType application/x-httpd-php .jpg` |
| Nginx 别名穿越 | `location /files { alias /data/; }` 类 `/files../etc/passwd`（属路径穿越，同测） |

## .htaccess / .user.ini（持久化后门通道，授权后）

- Apache 可传 `.htaccess`：`AddType application/x-httpd-php .txt` + 一句话 txt
- PHP-CGI 可传 `.user.ini`：`auto_prepend_file=img.jpg` + 每个php请求自动包含

## OSS/云直传判定（现代应用主流）

前端拿签名直传 OSS 时：
1. **Policy 篡改**：签名 policy 若前端可改（content-length-range、key 前缀、content-type 未锁定）→ 上传任意类型/任意 key
2. **key 可控**：`key` 含用户输入路径 → 目录穿越式覆盖（`../` 上层 key）
3. **上传后访问行为**：拿到文件 URL 后 `curl -I` 看 `Content-Type` 与浏览器渲染行为——HTML 可渲染 = 存储型 XSS（衔接 hunt-xss）；text/html 被强制下载则记录为低危
4. **桶权限**：上传成功后测列举（衔接知识库 COS/OSS 三段式 poc）

## 无害证明法（SRC 纪律核心）

1. 上传 `.txt` 写入标记内容 → 证明"能传非白名单类型"（多数场景已够中危）
2. 证明"能执行"：优先找目标自身可执行通道（.htaccess/user.ini/解析漏洞组合），上传一句话后**只执行 `echo` 级命令**留证据截图
3. 验证完立即删除上传物，记录清理时间
4. 回填：category=file-upload，usage 记"校验层绕过链→证明方式→清理状态"

<!-- 改编自: PayloadsAllTheThings(Upload Insecure Files) + 本地匿名上传定性矩阵 poc 经验, 2026-10-31 -->
