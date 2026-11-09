# flux-wms-识别与未授权面探测-actuator-saml-文件读取路径-登录裸rsa

> 状态: `script_ok` — 配套脚本 `poc.sh`
> 本文件说明**为什么这么写**，不是使用手册。用法见脚本头 docstring。

## 为什么需要这个脚本

WMS 系统有三个共同特征：固定前端模板、Spring Boot 后端、登录接口下发 RSA 公钥。
这三条决定了它的探测面是固定的，不需要通用扫描器。

## 判据设计

五面，每面独立判据：
  A 指纹    favicon.ico 的 md5 是跨站比对最稳的指纹（同套代码换域名，ico 不变）
  B actuator 判据是 JSON 结构（"_links" / "propertySources"），不是 200
  C SAML    必须是 EntityDescriptor + X509Certificate 同时出现
  D 配置     body 必须匹配 `^\s*(spring|server|datasource)\s*[:.]` 才算 yml
  E 裸 RSA   下发公钥本身不是漏洞，但它说明密码是 RSA 加密提交

## 常见误报（先排除这些，再下结论）

① 任意路径都返回 200 的站（SPA 兜底）会让所有 200 判据失效 → 必须先建随机路径基线
② actuator 的 404 可能被网关改写成 200 + 错误页
③ SAML metadata 在部分实现里对匿名开放是设计如此（SPNEGO 协商需要），不算漏洞
④ RSA 公钥固定不变才是问题（可离线构造密文撞库），每次都变则正常

## 验证与收口

同一性确认（避免把别的 Spring 站当 FluxWMS）：
  1) /actuator/mappings 里搜 flux/wms 的 Controller 包名
  2) 登录页 JS 里搜版本号
  3) favicon hash 拿去 FOFA：icon_hash="<md5>"，同 hash 即同套代码

## 待办

- [ ] 对真实目标跑通一次，把结果写进 `INDEX.json` 的 `battle_verified`
- [ ] 构造阴性对照：一个必然不命中的输入，确认判据不会永远返回命中
