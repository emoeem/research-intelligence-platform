# 反知识：端口扫描"全端口 open"不是资产面

## 现象
```
nmap -Pn --top-ports 1000 --open 43.200.81.252
PORT      STATE SERVICE
1/tcp open tcpmux
3/tcp open compressnet
7/tcp open echo
...
52869/tcp open unknown
```
两台完全无关的主机（AWS 首尔 / A2 Hosting 美国）返回**逐条相同**的全端口 open 列表。

## 验证（必做三步）
1. 连随机高位端口：`/dev/tcp/HOST/12345`、`/dev/tcp/HOST/54321` → 全部 connect-ok
2. 取 banner：`exec 3<>/dev/tcp/HOST/80; printf 'GET / HTTP/1.0\r\nHost: x\r\n\r\n' >&3; head -c 200 <&3` → 空
3. 对比 80/443 的真实响应 → 只有真实 Web 端口有响应

三条同时命中 = 前置设备对所有 TCP SYN 回 SYN-ACK（SYN proxy / 全端口接受 / 云厂商的按需端口转发）。

## 判定
- **端口状态无参考价值**，不能写"开放 N 个端口"，也不能据此评估攻击面
- 在报告"手段用尽门禁"里应记为 **不可判定（undeterminable）**，不是"已完成"
- 唯一可用的结论是：目标 80/443 有真实服务，其余端口既不能证有也不能证无

## 通用化
任何 nmap 结果里出现「端口号 1/3/7/9/13 等极低位端口 open」或「两台不同主机端口列表完全一致」，
先做上面三步验证再下结论。云厂商 LB（AWS NLB 按需后端）也会呈现同样特征。
