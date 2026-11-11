# SK-3 拒绝矩阵 fixture（private-url 类）

链接目录形态，混入私网/本地/云 metadata 端点 URL 的反例。
catalog.extract 只按 scheme 过滤（本组 URL scheme 合法，http/https，会进
entries，这是 T0 纯解析的如实语义）；私网拒绝发生在 document.fetch 的
策略闸（private_network_blocked 错误码）与本审查工具（AUD-NET-01/02）。

- [Localhost](http://127.0.0.1:8080/admin): loopback 反例
- [Metadata](http://169.254.169.254/latest/meta-data/): cloud metadata endpoint 反例
- [LAN](https://192.168.1.1/router): RFC1918 反例
- [Loopback alt](http://localhost:9000/): hostname 形反例
- [Public](https://example.com/ok): 公网对照条目
