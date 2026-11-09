VTCLP 帧（websocket/binary.js）
==============================
布局: [0x01][varint(type)][0x00][varint(len)][utf8 json]
type: 1=bind 2=heartbeat 3=data     (MessageType.Bind=1,Heartbeat=2,Data=3)
varint: 每字节低 7 位是小端数据，最高位=1 表示还有下一字节

帧体 JSON: {"param":{...},"packet_id":<自增 int>,"cmd":"<指令>"}
响应:      {"code":0,"packet_id":N,"msg":{...}}
推送:      {"push":"new_msg","info":{...},"push_id":"..."}
判据: 按 packet_id 匹配响应；带 push 字段的是异步推送。
客户端按 uid 灰度可能发明文 JSON 而不是二进制；decodeWsMessage 两者都吃，明文帧同样合法。

握手
====
wx.connectSocket({url:"wss://chat-love.qingteng-inc.com/ws"})   —— 无 query、无 header、无签名
备份/备用: wss://chat-love-bak.qingteng-inc.com/ws ; wss://jqjrpwr.afunapp.com/ws (g12) ;
           wss://ubjcnqa.afunapp.com/ws (g14) ; 另有 jymqwni/dtosxwt/nglhotr.afunapp.com
建议头: Origin: https://servicewechat.com
        Referer: https://servicewechat.com/wx788f5daad54eb854/922/page-frame.html

鉴权 = 首帧 bind（不是握手、不是订阅帧）
================================
>>> {"param":{"uid":<uid>,"token":"<loginInfo.token>","platform":1,"version":"5.62.1","device_id":""},
     "packet_id":1,"cmd":"bind"}
<<< {"code":0,"packet_id":1,"msg":{}}
失败: {"code":1,"msg":"身份校验不通过"}

**token 必须是登录 JWT**（== HTTP authorization 头那个 eyJ...）。
auth_token（AES-CBC 密文，形如 SBAk0TDvZi9I…）在这里 100% 不认。

未 bind 直接发数据帧 ⇒ sync_session_list code 100 / sync_contacts code 10 / sync_history code 10
⇒ 鉴权真实生效，无免登录面。

bind 是弱校验：param.uid 与 token 内的 Uid 不一致也返回 code 0，
真实身份取自 token。判断归属要看 sync_max_seq / sync_session_list，不能看 bind 的 code。

读消息帧（sender.js）
=====================
sync_session_list  {}                                     → msg.session_list[]
sync_contacts      {}                                     → msg.list[{uid,auth_token}]  ← 下发联系人 auth_token
sync_max_seq       {}                                     → msg.max_seq
sync_history       {"friend":<uid>,"msg_seq":<seq>,"limit":20}
update_session     {"friend":<uid>,"msg_seq":<seq>,"limit":20}  → msg.msg_list
sync_read          {"target_uid":<uid>} / {"session_id":<sid>,"seq":<n>}
sync_msg           {"offset":<n>}
（他人 uid 作 friend → {"code":100,"msg":""}，有会话关系校验）

不要发的写帧: send_msg / ack_read / ack_delay / delete_session / sync_read(会改已读位)

复现脚本
========
见 runs/qingteng/r22/ws22.py，模式:
  ws22.py sess <uid> <JWT> [urlidx]         bind + session_list + max_seq
  ws22.py hist <uid> <token> <friend> [n]   bind + sync_history(friend)
  ws22.py combo                            单连接内多身份对照（判定归属用）
