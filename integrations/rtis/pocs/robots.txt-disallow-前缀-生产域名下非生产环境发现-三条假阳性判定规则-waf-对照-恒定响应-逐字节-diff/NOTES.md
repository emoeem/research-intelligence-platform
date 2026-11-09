# robots.txt-disallow-前缀-生产域名下非生产环境发现-三条假阳性判定规则-waf-对照-恒定响应-逐字节-diff

> 状态: `script_ok` — 配套脚本 `poc.sh`
> 本文件说明**为什么这么写**，不是使用手册。用法见脚本头 docstring。

## 为什么需要这个脚本

运维常把 /test /staging /admin 写进 robots.txt（本意是"别爬"），但生产域名的
robots 往往是从非生产环境复制来的。这些前缀在生产域名上如果是另一套后端在服务，
就会出现 Server / X-Powered-By / 错误页模板的差异 —— 这才是判据，不是"能访问"。

## 判据设计

三条假阳性规则，缺一条就会误报：
  R1 WAF 对照   先建"确定不存在路径"的基线，与之逐字节相同的响应对应兜底页
  R2 恒定响应   同一前缀两次请求 body 完全相同且含"建设中/维护"→ 占位页
  R3 逐字节diff 前缀响应与根响应归一化后确实不同，且差异落在结构字段上

## 常见误报（先排除这些，再下结论）

① 前端 router 的 history 模式会让所有路径返回同一份 index.html → 必须与"根"比对
② CDN 可能按路径缓存，第二次请求拿到缓存结果掩盖真实差异
③ 只有长度不同不算差异（随机广告位、时间戳）
④ robots.txt 本身可能是 SPA 兜底页（返回 HTML 而不是纯文本）

## 验证与收口

最强证据不是响应体，是 TLS 证书：
  openssl s_client -connect host:443 -servername host | openssl x509 -noout -subject -ext subjectAltName
  证书 CN/SAN 变了 = 按路径分流到了另一台机器
其次是完整响应头集合（X-Env / X-Backend / Set-Cookie Domain），
再次是 /favicon.ico 的 md5（同套前端复用，但后端不同）。

## 待办

- [ ] 对真实目标跑通一次，把结果写进 `INDEX.json` 的 `battle_verified`
- [ ] 构造阴性对照：一个必然不命中的输入，确认判据不会永远返回命中
