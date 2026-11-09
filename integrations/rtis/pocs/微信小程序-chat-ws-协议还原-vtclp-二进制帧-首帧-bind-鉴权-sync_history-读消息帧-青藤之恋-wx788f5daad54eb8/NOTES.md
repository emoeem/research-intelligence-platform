# 微信小程序-chat-ws-协议还原-vtclp-二进制帧-首帧-bind-鉴权-sync_history-读消息帧-青藤之恋-wx788f5daad54eb8

> 状态: `notes_only` — 目标/业务特定，通用脚本会失真。
> 本文件记录**真实技术细节与判定框架**，不是脚本。

## 主题

微信小程序 chat WebSocket 协议还原：VTCLP 二进制帧、首帧 bind 鉴权、sync_history 读消息


## 协议结构

青藤之恋（`wx788f5daad54eb8`）的聊天走 WebSocket，帧格式自定义，帧头有魔数
（此处记录为 `VTCLP`，实际以抓包为准）。

典型结构：

```
+--------+--------+--------+--------+--------+
| magic  | type   | seq    | len    | payload|
| 5 byte | 1 byte | 4 byte | 4 byte | len B  |
+--------+--------+--------+--------+--------+
```

- `magic`：固定魔数，用于帧同步
- `type`：帧类型（0x01=bind, 0x02=pong, 0x10=message, 0x11=sync, ...）
- `seq`：序号，服务端按序回包
- `len`：payload 长度（大端）
- `payload`：protobuf 或 JSON

## 握手顺序（顺序错就断开）

```
1. WS 建连 → wss://<host>/ws?token=xxx
2. 客户端发 bind 帧（含 uid + token + 设备信息）
3. 服务端回 bind_ack（含 session_id、心跳间隔）
4. 客户端按间隔发 ping，服务端回 pong
5. 之后才能发 sync_history / send_message
```

**最容易错的一步**：bind 帧必须在建连后 N 毫秒内发出（常见 100ms），
超时服务端直接 close。用脚本复现时要立刻发。

## 鉴权在哪一层

三个可能的位置，都要测：

| 位置 | 表现 | 绕过难度 |
|---|---|---|
| 建连 URL 的 token 参数 | token 无效时拒绝握手（HTTP 401） | 需要有效 token |
| bind 帧里的 uid | uid 与 token 不匹配时回 error 帧 | **改 uid 可能读到别人消息** |
| 每条消息帧里的 session_id | 逐帧校验 | 最难，需要伪造 session |

第二种是最常见的漏洞：bind 时只校验 token 有效，不校验 token 对应的 uid 与包里的
uid 一致 → 用 A 的 token + B 的 uid 就能收到 B 的消息。

## sync_history 帧

拉历史消息的帧通常是：

```
type=0x11, payload = protobuf{ peer_uid, last_seq, limit }
```

若 `peer_uid` 不校验归属，就是水平越权 —— 能读任意两人的聊天记录。

## 常见误报

- 只建连成功但收不到消息 → bind 没发或发错
- 收到 pong 但没有 data 帧 → 需要先发 sync 或 subscribe
- 帧解析出乱码 → 字节序错（试小端）或 payload 是 protobuf 不是 JSON

## 落地清单

- [ ] 抓一次真实握手，记录 bind 帧的完整字节
- [ ] 用 Python websocket 客户端复现握手
- [ ] 确认心跳间隔（不发心跳会被服务端踢）
- [ ] 测 bind 帧 uid 与 token 不一致
- [ ] 测 sync_history 的 peer_uid
- [ ] 报告写清"是否需要有效 token"（决定危害等级）


---

## 为什么没有可执行脚本

这类条目的价值在「**针对特定目标的具体契约**」：错误码表、字段名、加密常量、
路由命名。把这些抽象成"通用脚本"，产出的就是一个装满占位符的空壳，
反而比这份说明更容易误导人。

需要自动化时，从中抽出的通用部分已经落到这些脚本里：
- `../crypto_data-加密网关判定法-*/crypto-gateway-probe.py`（判定加密形态）
- `../webpack-spa-前端硬编码-aes-cbc-密钥的定位与复刻-chunk-map-全量下取法/aes-hunter.py`（定位并验证 AES 密钥）
- `../appid-secret-泄露面静态扫描-*/scan-appsecret.py`（批量扫凭证）
- `../webpack-chunk-map-提全量按需-chunk-含-hash-批量抓取/chunk-harvest.py`（抓全量 bundle）

## 待办

- [ ] 若后续对同类目标再遇到一次，把重复出现的部分抽成脚本
- [ ] 每个错误码表补一次实测确认（不同版本会变）
