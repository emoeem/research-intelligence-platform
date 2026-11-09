### 目的
WAF 把 OGNL payload 全拦之后，仍然要给出「这个目标到底是不是 Struts2」的结论。
下面四条 oracle 都不含 `%{`/`${`/`ognl` 关键字，不会被 OGNL 规则误拦。

### Oracle 1 · Struts2 静态资源
Struts2 若映射 `/*`，FilterDispatcher 会自己输出这两份内置静态资源：
```
GET /struts/utils.js
GET /struts/domTT.css
```
- 200 + JS/CSS 内容 → **是 Struts2**（且 filter 挂 `/*`）
- 404 / 302 到错误页 → 大概率不是（注意：若 filter 只挂 `*.do`，这里也会 404，所以单条不定案）

### Oracle 2 · .action 后缀
Struts2 默认 `struts.action.extension=action,,`，即 `.action` 与无后缀都应可路由：
```
GET /index.action          ← 与已知活的 .do 同 action 名对比
```
- `.action` 与 `.do` 行为一致 → 是 Struts2
- `.action` 404 而 `.do` 正常 → **不是 Struts2**（或 extension 被改成 do，仍需 oracle 3 佐证）

### Oracle 3 · DMI `method:` 前缀（决定性，且不含关键字）
Struts2 的 `DefaultActionMapper` 会解析 `?method:xxx` 前缀（S2-032 的前提）：
```
GET /<活的action>.do                      ← 基线，记下 status/长度/Location
GET /<活的action>.do?method:__nope__      ← 测试
GET /<活的action>.do?method:execute       ← 再测一个真实方法名
```
判读：
- 三次响应**逐字节相同** → 无 DMI 语义 → **不是 Struts2**（或 DMI 全局关闭，但连 method 参数都不产生任何差异仍是很强的反证）
- `method:__nope__` 触发 500 / "There is no …" / 参数注入痕迹 → 是 Struts2

### Oracle 4 · 错误信封比对（判断"是哪一套应用"）
同一单位不同子域若共用一套 JSON 错误信封，说明它们是同一个框架/同一套代码：
```
GET /<随机路径>           → 404 体；对比另一子域的同名 404 体
POST <GET-only handler>   → 405 体
```
- 两处信封字段与文案完全一致（如 `{"success":false,"message":"No handler found for GET <path>","errorCode":10404}`）→ **同一 Spring 应用**，`*.do` 只是遗留路由层，不是独立业务系统
- 信封不同 → 是两套不同的遗留应用，需分别判

### 附带：三种"看起来像漏洞、实际不是"的排除
1. `?action=XXX`（DispatchAction 风格）
   ```
   GET /ent/index.do
   GET /ent/index.do?action=Zzz123     ← 逐字节比对
   ```
   完全相同 → 该参数**不参与路由**，Wayback 里的 `?action=AddBbs` 已失效，不要据此认为存在 DMI。
2. 敏感路径返回 **429**
   ```
   GET /manager/html     → 429
   GET /manager/htmlx    → 429      ← 关键：变体也 429
   GET /manager/zzz      → 404      ← 前缀本身无规则
   GET /<随机路径>       → 404      ← 同批请求，排除 IP 封禁
   ```
   ⇒ 是「含 html 的扫描器特征路径」反扫描规则，**不是 Tomcat Manager 存在证据**。
3. `?refer=` 之类的错误页参数
   ```
   GET /error/404.html?refer=ZZMARKER123      → 页面内 grep 计数应为 0
   ```
   计数 0 → 纯静态页，参数只用于 302 Location 拼接，**无反射 XSS**。