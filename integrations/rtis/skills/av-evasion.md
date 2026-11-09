---
name: av-evasion
description: Windows 免杀与防御规避。AMSI 绕过、ETW patching、shellcode 加载器、Defender 排除项、EDR hook 对抗、检测面分析。授权红队与 AV 产品评估使用。
tags: [windows, evasion, amsi, etw, edr, shellcode, redteam]
---

# AV / EDR 免杀与防御规避

**判定纪律**：免杀结论必须给「哪款引擎、哪个版本、什么时候测的、用什么指令复测」。说「过了 Defender」而不给 `Get-MpThreatDetection` 输出，等于没说。

## 一、先搞清楚在跟谁打

| 对手 | 检测面 | 绕过优先级 |
|---|---|---|
| Windows Defender | 静态特征 + AMSI + 云端 ML + 行为监控 | 先过 AMSI |
| 商业 EDR（CrowdStrike/S1/Defender for Endpoint） | 用户态 hook + ETW + 内核回调 + 内存扫描 | 内存加密 + 间接 syscall |
| 国产杀软（360/火绒/腾讯） | 驱动 + 云查 + 主动防御弹窗 | 白利用 + 签名 |
| 仅静态扫描（上传到 VT 判） | PE 特征、字符串、导入表 | 加壳/分离加载 |

**先确认对手是谁**。对着 VT 全绿而落地被拦，说明打的是行为层；反过来一样。

## 二、AMSI 绕过

AMSI 是 PowerShell / VBScript / JScript / .NET 的脚本内容扫描接口。

### 2.1 内存 patch（进程内）

```powershell
# 定位 amsi.dll 中 AmsiScanBuffer 的地址，改写开头字节为立即返回
$a = [Ref].Assembly.GetTypes() | ? {$_.Name -like "*iUtils"} | select -First 1 -ExpandProperty FullName
$b = [Ref].Assembly.GetTypes() | ? {$_.Name -like "*Context"} | select -First 1 -ExpandProperty FullName
```

实际 patch 用 C# 或 C 实现：`VirtualProtect(PAGE_EXECUTE_READWRITE)` → 写入 `0xB8,0x57,0x00,0x07,0x80,0xC3`（`mov eax, 0x80070057; ret`，返回 E_INVALIDARG）。

**注意**：Defender 的 AMSI provider 会校验自身代码完整性（`amsi.dll` 的 .text 段 hash）。部分版本会检测 patch。

### 2.2 注册表禁用 provider

```powershell
# 需要管理员。把 AmsiEnable 置 0 或删除 provider CLSID
reg add "HKLM\SOFTWARE\Microsoft\Windows Script\Settings" /v AmsiEnable /t REG_DWORD /d 0 /f
```

### 2.3 DLL 劫持 / CLR hook

在进程加载 `amsi.dll` 之前放同名的伪造 DLL（需正确导出），或 hook `AmsiScanBuffer` 的导入表。

### 2.4 反射加载（不进磁盘）

```powershell
# 用 Assembly.Load 从 byte[] 加载，不落盘，绕开文件扫描
[System.Reflection.Assembly]::Load([Convert]::FromBase64String($b64))
```

**绕过不完整的信号**：脚本执行报 `此脚本包含恶意内容` 但无进程告警 → AMSI 仍在工作；报 `AMSI 初始化失败` → 可能被 patch 但被别的机制兜住了。

## 三、ETW patching

ETW 是 EDR 的主要遥测来源（`Microsoft-Windows-Threat-Intelligence`、`.NET Runtime`）。

```
目标 provider：
  1c95126e-7eea-49a9-a3fe-a378b03ddb4d  (.NET Runtime)
  Microsoft-Windows-DotNETRuntime
  Microsoft-Windows-Threat-Intelligence  (内核侧，用户态改不了)
```

用户态做法：定位 `ntdll!EtwEventWrite`，patch 首字节为 `xor eax,eax; ret`（返回 0 表示成功写入）。

**局限**：只能屏蔽用户态 ETW。内核 provider（如 `Threat-Intelligence`）和内联 hook 不受影响。

## 四、EDR hook 对抗

EDR 通常在 `ntdll` 的敏感函数头部下 `jmp` 到自己的 DLL。

| 手法 | 原理 | 检测风险 |
|---|---|---|
| 直接 syscall | 自己写 `syscall` 指令，不经过 ntdll | 中：syscall 来源不在 ntdll 会告警 |
| 间接 syscall | 从 ntdll 里找 `syscall; ret` gadget 再跳 | 低 |
| ntdll unhook | 从磁盘重新映射一份干净 ntdll 覆盖 | 高：内存段 hash 会变 |
| 硬件断点 hook | 用 DR 寄存器在目标函数下断 | 低：不修改代码段 |

**ScyllaHide / TitanHide** 属于现成实现，但特征已被主流 EDR 收录。

## 五、shellcode 加载器

标准形态：

```
1. 分配内存    VirtualAlloc(RW) → 写入 → VirtualProtect(RX) → CreateThread
2. 规避点：RWX 分配是高危信号
3. 替代：分两次分配 / 用 NtMapViewOfSection / 回调执行（EnumFonts、CreateTimerQueueTimer）
```

**回调执行**（避免 `CreateThread`）：
- `EnumChildWindows` / `EnumFontFamiliesEx` 传函数指针
- `SetTimer` / `CreateTimerQueueTimer`
- `CertEnumSystemStore`（签名枚举）
- APC 注入（`QueueUserAPC`）

**内存加密**：shellcode 在内存中保持加密，只在执行前解密一小段（Beacon Object File 风格）。对抗内存扫描有效。

## 六、Defender 专项

```powershell
# 查当前排除项（管理员）
Get-MpPreference | select ExclusionPath,ExclusionExtension,ExclusionProcess

# 查检测历史
Get-MpThreatDetection | sort InitialDetectionTime -Desc | select -First 10
```

**自我排除**（需管理员，且会被记录）：把落地目录加入 `ExclusionPath`。

**更隐蔽的路径**：
- 写入 `%LOCALAPPDATA%` 而非 `%TEMP%`（Temp 是高危目录）
- 用合法签名的宿主进程（`rundll32`、`regsvr32`、`mshta`、`installutil`）→ **白利用**
- DLL 侧加载（合法 EXE + 恶意 DLL 同名）

**云查（Cloud-delivered protection）**：新出现的样本会实时上传。测试时先断网或关云查，否则测的是「第一次运行被云判」而非本地特征。

## 七、签名

- 未签名 EXE 在现代 Windows 上触发 SmartScreen，下载场景直接拦
- 有签名 = 需要证书。盗用签名会带来吊销 + 取证溯源风险
- **合法路径**：用企业自己签发的证书签自己的工具（红队演练场景下由客户提供）

## 八、测试方法（可复现）

```powershell
# 1. 静态：先看 VT 有几家报
# 2. 本地：关云查、断网，落地运行
Set-MpPreference -DisableRealtimeMonitoring $true     # 仅测试机
# 3. 行为：跑完查检测历史
Get-MpThreatDetection | select ThreatName,Resources,InitialDetectionTime
# 4. 恢复
Set-MpPreference -DisableRealtimeMonitoring $false
```

**必须报告的**：引擎版本（`Get-MpComputerStatus | select AMEngineVersion,AntivirusSignatureVersion`）、测试时间、是否开云查。

## 九、常见误判

| 现象 | 真实原因 |
|---|---|
| 「过了 Defender」但落地就被删 | 测的是本地特征，没测按访问扫描（real-time on write） |
| 「VT 0 检出」但运行被拦 | VT 是静态扫描，行为层没覆盖 |
| 「改个字符串就过了」 | 只骗过了静态，行为监控仍在；换机器就失效 |
| 「关掉 Defender 就成功」 | 客户环境不会给你关的权限，结论无效 |

## 十、红线

- 只在授权测试机和隔离环境做
- 不投递到真实用户邮箱/不落地到生产终端
- 不盗用第三方签名证书
- 测试结束恢复 Defender 配置并写清恢复记录
