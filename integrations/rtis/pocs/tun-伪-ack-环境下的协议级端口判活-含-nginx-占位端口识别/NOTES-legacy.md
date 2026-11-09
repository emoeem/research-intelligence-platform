#!/usr/bin/env python3
# 在 sing-box TUN 伪 ACK 环境下做端口判活：绝不信 connect()，只认应用层回包。
# 完整实现见 attack-files/91.110.233.0_24/dbprobe2.py

核心要点
1) 伪 ACK 陷阱
   exec 3<>/dev/tcp/192.0.2.1/443  → "成功"
   nmap -Pn / nc -z 对任意目标全 open，全是假阳性。
   => 一律改用「发协议载荷 + 读回包」判定。

2) 关闭端口在伪 ACK 环境下 0.4s 内 EOF，不会干等超时
   => 2505 次探测 14 分钟跑完，无需靠缩短超时提速。

3) 每端口必须带协议特征，不能只看"有回包"
| 端口 | 载荷 | 通过判据 |
|---|---|---|
| 3306/3307/13306/33060 | 不发送，等服务端 greeting | pkt[4]==0x0a 且含版本串 |
| 6379/6380 | b'PING\r\n' | +PONG / -NOAUTH / -ERR |
| 5432 | struct.pack('!ii',8,80877103) | 回 'S' 或 'N' |
| 1433 | TDS PreLogin 19B | 回 0x04 |
| 11211 | b'stats\r\n' | 含 b'STAT ' |
| 9042 | b'\x04\x00\x00\x00\x09\x00\x00\x00\x00' (CQL v4 OPTIONS) | 回 0x84 |
| 8086 | GET /ping | 204 / X-Influxdb-Version |
| 27017/9200 | GET / (HTTP) | cluster_name / tagline / "MongoDB over HTTP" |
| 2375 | GET /version | JSON ApiVersion |
| 5672 | AMQP\x00\x00\x09\x01 | 回 AMQP |
| 2181 | ruok | imok |
| 9092 | ApiVersions v0 (api_key=18) | 回 api_key=18 |
| 2379/8500/8848/9000/9090/3000/15672 | HTTP + 各服务 marker | etcdcluster / MinIO 200 / nacos JSON |

4) 必做双向对照，否则"全 closed"可能是探针坏了
   阳性：取一个已知开放的 HTTP 应用端口（5001 之类）→ 必须命中
   阴性：取一个已知关闭的 DB 端口 → 必须判闭

5) 关键陷阱：nginx 占位端口会被误判成"服务开放"
   很多集群把面板端口集统一绑给 nginx/WAF 做拦截，回 400/302：
     Server: nginx + Content-Length: 2 + 400 Bad Request     (Host 不匹配)
     Server: nginx + 302 → https://<ip>/                       (IP 直访)
   这不是 Elasticsearch/Mongo/MinIO，**恰恰是它们不在的证据**（监听权已被 nginx 拿走）。
   判据：回包里有 Server: nginx / 边缘特征头（x-ws-request-id 等）=> 记为"占位"，不算服务命中。

6) 歧义消除：DB 口是不是被 nginx 吃掉、真 DB 躲在后面？
   对 3306/6379/5432/1433/11211 直接发 HTTP：
     回 HTTP => nginx 占口，不是 DB
     回 000  => 边缘 DROP，物理上不可能有 DB
   实测（5 台）全 000 => 边缘是白名单式端口映射，DB 口不在映射表内。

7) 限速与红线
   同 IP 内串行、间隔 0.5-0.6s、单 IP ≤15 端口/波次；跨主机可并行 6 路。
   只读：INFO/PING/DBSIZE/_cat/indices 可以；不 CONFIG SET、不 SLAVEOF、不写 key、不 exec 容器。
