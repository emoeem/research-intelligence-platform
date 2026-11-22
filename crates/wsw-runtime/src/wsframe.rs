//! `wsw_runtime::wsframe`：WS 帧类型面（WSW-5；PLAN §4.5 :274-283 语义面）。
//!
//! - [`WsOpcode`]：opcode 词表 enum（PLAN :276/:678 text/binary/ping/pong/
//!   close 五词形，030 CHECK 同源）；
//! - tungstenite [`Message`](hudsucker::tokio_tungstenite::tungstenite::Message)
//!   ↔ 词表映射纯函数（opcode 判定 / payload 提取 / 掩码方向词形承载）；
//! - 帧行组装 = [`crate::persist::WsFrameRow`]（030 表；CAS BodyRef 四派生
//!   列由捕获侧经 `wsw_message::cas::store_body` 落账）。
//!
//! 边界（D3）：零第二 WS 栈——消费面全走 hudsucker 再导出
//! `hudsucker::tokio_tungstenite`（vendor lib.rs:51）；本模块零 socket 零
//! IO（帧捕获挂在 `impl WebSocketHandler for WswHandler` :2098 单点）。

use hudsucker::tokio_tungstenite::tungstenite::Message;

/// opcode 词表（PLAN :276/:678；030 CHECK 同源——[`crate::persist::
/// WS_FRAME_OPCODES`] 逐字一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsOpcode {
    /// 文本帧。
    Text,
    /// 二进制帧。
    Binary,
    /// Ping 控制帧。
    Ping,
    /// Pong 控制帧。
    Pong,
    /// Close 控制帧。
    Close,
}

impl WsOpcode {
    /// 词表串（030 opcode CHECK 词形）。
    pub fn as_str(self) -> &'static str {
        match self {
            WsOpcode::Text => "text",
            WsOpcode::Binary => "binary",
            WsOpcode::Ping => "ping",
            WsOpcode::Pong => "pong",
            WsOpcode::Close => "close",
        }
    }

    /// 词表串解析（非法即 None——不静默映射）。
    pub fn from_word(word: &str) -> Option<Self> {
        match word {
            "text" => Some(WsOpcode::Text),
            "binary" => Some(WsOpcode::Binary),
            "ping" => Some(WsOpcode::Ping),
            "pong" => Some(WsOpcode::Pong),
            "close" => Some(WsOpcode::Close),
            _ => None,
        }
    }
}

/// tungstenite Message → opcode 词表（`Frame` 变体不透出——读取面永不产出，
/// vendor protocol/message.rs「you're not going to get this value while
/// reading」；防御面归 Close 词表外的 None）。
pub fn opcode_of(msg: &Message) -> Option<WsOpcode> {
    match msg {
        Message::Text(_) => Some(WsOpcode::Text),
        Message::Binary(_) => Some(WsOpcode::Binary),
        Message::Ping(_) => Some(WsOpcode::Ping),
        Message::Pong(_) => Some(WsOpcode::Pong),
        Message::Close(_) => Some(WsOpcode::Close),
        Message::Frame(_) => None,
    }
}

/// tungstenite Message → opcode 词表串（[`opcode_of`] 词形投影）。
pub fn opcode_word(msg: &Message) -> Option<&'static str> {
    opcode_of(msg).map(WsOpcode::as_str)
}

/// tungstenite Message → payload 原始字节（Close = 空字节；CloseFrame 的
/// code/reason 不入 payload 列——帧行的 close 观察面 = 行本身，PLAN :278）。
pub fn payload_of(msg: &Message) -> Vec<u8> {
    match msg {
        Message::Text(t) => t.as_bytes().to_vec(),
        Message::Binary(b) => b.to_vec(),
        Message::Ping(b) => b.to_vec(),
        Message::Pong(b) => b.to_vec(),
        Message::Close(_) => Vec::new(),
        Message::Frame(_) => Vec::new(),
    }
}

/// 方向 → 线缆掩码面（RFC6455 §5.1：client→server 帧恒掩码、server→client
/// 帧恒不掩码。tungstenite 解码后掩码位不透出，帧行的 masked 列由方向词形
/// 承载线缆层事实——如实词形，非观测伪造）。
pub fn masked_by_direction(direction: &str) -> bool {
    direction == "client_to_server"
}

/// 帧 payload 的 CAS mime 词形（D3：text 帧落 'text/plain'、binary 帧落
/// 'application/octet-stream'；ping/pong/close 控制帧 payload 极短（≤125B，
/// RFC6455 §5.5）同样按字节面落 'application/octet-stream'）。
pub fn payload_mime(opcode: WsOpcode) -> &'static str {
    match opcode {
        WsOpcode::Text => "text/plain",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opcode_word_table_roundtrip() {
        for (msg, want) in [
            (Message::text("hi"), "text"),
            (Message::binary(vec![1u8, 2]), "binary"),
            (Message::Ping(vec![9].into()), "ping"),
            (Message::Pong(vec![9].into()), "pong"),
            (Message::Close(None), "close"),
        ] {
            assert_eq!(opcode_word(&msg), Some(want));
            assert_eq!(
                WsOpcode::from_word(want),
                opcode_of(&msg),
                "词表串解析与 opcode 判定互逆"
            );
        }
        let _ = Message::Close(None); // Frame 变体读取面不透出——无构造面即无测试构造（词表面 None 分支防御性存在）。
        assert_eq!(WsOpcode::from_word("gzip"), None, "词表外拒绝");
    }

    #[test]
    fn payload_extraction_matches_wire_bytes() {
        assert_eq!(payload_of(&Message::text("héllo")), "héllo".as_bytes());
        assert_eq!(
            payload_of(&Message::Binary(vec![0u8, 1, 254].into())),
            vec![0, 1, 254]
        );
        assert_eq!(
            payload_of(&Message::Ping(b"pp".as_slice().into())),
            b"pp".to_vec()
        );
        assert_eq!(
            payload_of(&Message::Pong(b"qq".as_slice().into())),
            b"qq".to_vec()
        );
        assert!(payload_of(&Message::Close(None)).is_empty());
    }

    #[test]
    fn masked_follows_rfc6455_direction_semantics() {
        assert!(masked_by_direction("client_to_server"));
        assert!(!masked_by_direction("server_to_client"));
        assert!(!masked_by_direction("anything_else"));
    }

    #[test]
    fn payload_mime_word_table() {
        assert_eq!(payload_mime(WsOpcode::Text), "text/plain");
        assert_eq!(payload_mime(WsOpcode::Binary), "application/octet-stream");
        assert_eq!(payload_mime(WsOpcode::Close), "application/octet-stream");
    }
}
