//! 四表示类型与词表枚举（DESIGN §1.1；PLAN §6.3 :477-482 逐条落地）。
//!
//! 四表示（DESIGN §1.1 不变式三条由本模块与 lib.rs 头注机制化）：
//! - [`WireBytes`]：捕获时的原始传输字节，唯一原始事实（不变式 1）。
//! - [`ParsedMessage`]：结构化表示（httparse 驱动的派生投影 +
//!   `http::Request`/`http::Response` 类型视图）。头序保持为有序
//!   `Vec<HeaderRecord>` 作为正典投影——`http::HeaderMap` 迭代序规范未定义，
//!   不作头序正典（conformance 断言逐位落在 Vec 上）。
//! - [`DisplayPayload`]：展示投影，永不回写（不变式 3：生成过程不得改变
//!   Wire/Parsed 存储；[`crate::redact`] 为纯函数挂点）。
//! - [`OutboundBytes`]：实际发送到目标的重序列化字节（不变式 2：只能从
//!   编辑态产生，见 [`crate::serialize`]）。
//!
//! 字节字段 JSON 往返统一 base64（`serde_base64` helper），fixtures 期望
//! JSON 同口径。

use bytes::Bytes;
use serde::{Deserialize, Serialize};

/// Wire：捕获时的原始传输字节（新类型包装，DESIGN §1.1 Wire 行）。
///
/// CONNECT 建链后 TLS 明文的首个原始 HTTP 报文字节流（请求与响应各一份），
/// 保留原样头序、原始空格/制表、原始 chunked 分帧字节。超帽截断由采集面
/// （WSW-2 wsw-proxy）标注 [`CaptureFidelity::Truncated`] + 原因，本类型只
/// 承载字节，不携带解析结论。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WireBytes(#[serde(with = "serde_base64")] Vec<u8>);

impl WireBytes {
    /// 从原始字节构造。
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }

    /// 内部字节视图。
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// 字节长度。
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// 消费自身，返回内部字节。
    pub fn into_inner(self) -> Vec<u8> {
        self.0
    }
}

impl From<Vec<u8>> for WireBytes {
    fn from(v: Vec<u8>) -> Self {
        Self(v)
    }
}

/// Outbound：实际发送到目标的重序列化字节（新类型包装，DESIGN §1.1 Outbound 行）。
///
/// 只能由 [`crate::serialize`] 从用户编辑态产生；发送前落 CAS（编辑产物可
/// 溯源），Outbound ≠ Wire 时 `edited=true`（WSW-3 消费）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OutboundBytes(#[serde(with = "serde_base64")] Vec<u8>);

impl OutboundBytes {
    /// 从重序列化字节构造。
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }

    /// 内部字节视图。
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// 字节长度。
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// 消费自身，返回内部字节。
    pub fn into_inner(self) -> Vec<u8> {
        self.0
    }
}

impl From<Vec<u8>> for OutboundBytes {
    fn from(v: Vec<u8>) -> Self {
        Self(v)
    }
}

/// 头记录（头序保持数组单元，DESIGN §1.2 request_headers/response_headers 行）。
///
/// `name` 原样大小写保留（Parsed 层不规范化，Display 层才归一，§四.1
/// req_mixed_case_headers 断言要点）；重复头多值保留为多条记录（保序）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeaderRecord {
    /// 头名（原样大小写）。
    pub name: String,
    /// 头值（原样字节 lossy UTF-8；HTTP/1.1 头值名义 ASCII，非 UTF-8 字节
    /// 以 U+FFFD 展示，体字节不受影响）。
    pub value: String,
}

impl HeaderRecord {
    /// 构造。
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

/// HTTP 版本标记（DESIGN §1.1 Parsed 行：lock 已有 http 1.5.0 类型面）。
///
/// `http::Version` 无 serde 派生，本枚举承载 JSON 词表（小写字符串，
/// DESIGN §1.2 protocol/http_version 行「小写字符串」口径）并双向映射
/// `http::Version`；`H2`/`H3` 只做标记不解析（WSW-1 非目标原文）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HttpVersion {
    /// HTTP/0.9（历史形态，解析面可见）。
    Http09,
    /// HTTP/1.0（EOF 定界常见形态，§四.1 res_http10_eof）。
    Http10,
    /// HTTP/1.1（主面）。
    Http11,
    /// HTTP/2（只做标记不解析，WSW-1 非目标）。
    H2,
    /// HTTP/3（只做标记不解析）。
    H3,
}

impl HttpVersion {
    /// JSON/wire 词表小写字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Http09 => "http/0.9",
            Self::Http10 => "http/1.0",
            Self::Http11 => "http/1.1",
            Self::H2 => "h2",
            Self::H3 => "h3",
        }
    }

    /// wire 状态/请求行字符串（大写，HTTP/1.x 专属；H2/H3 无 HTTP/1.1
    /// wire 形态，出站序列化面拒绝，见 `crate::serialize`）。
    pub fn wire_str(&self) -> Option<&'static str> {
        match self {
            Self::Http09 => Some("HTTP/0.9"),
            Self::Http10 => Some("HTTP/1.0"),
            Self::Http11 => Some("HTTP/1.1"),
            Self::H2 | Self::H3 => None,
        }
    }

    /// 从词表字符串解析（serde 反序列化与 Exchange 字段消费共用）。
    pub fn from_str_vocabulary(s: &str) -> Option<Self> {
        match s {
            "http/0.9" => Some(Self::Http09),
            "http/1.0" => Some(Self::Http10),
            "http/1.1" => Some(Self::Http11),
            "h2" => Some(Self::H2),
            "h3" => Some(Self::H3),
            _ => None,
        }
    }
}

impl From<http::Version> for HttpVersion {
    fn from(v: http::Version) -> Self {
        match v {
            http::Version::HTTP_09 => Self::Http09,
            http::Version::HTTP_10 => Self::Http10,
            http::Version::HTTP_11 => Self::Http11,
            http::Version::HTTP_2 => Self::H2,
            http::Version::HTTP_3 => Self::H3,
            _ => Self::Http11,
        }
    }
}

impl From<HttpVersion> for http::Version {
    fn from(v: HttpVersion) -> Self {
        match v {
            HttpVersion::Http09 => http::Version::HTTP_09,
            HttpVersion::Http10 => http::Version::HTTP_10,
            HttpVersion::Http11 => http::Version::HTTP_11,
            HttpVersion::H2 => http::Version::HTTP_2,
            HttpVersion::H3 => http::Version::HTTP_3,
        }
    }
}

impl Serialize for HttpVersion {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for HttpVersion {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::from_str_vocabulary(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown http version: {s}")))
    }
}

/// 消息种类（请求面/响应面，Exchange direction 字段的消息对语义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    /// 请求（client_to_server）。
    Request,
    /// 响应（server_to_client）。
    Response,
}

/// 体定界结果（DESIGN §1.1 Parsed 行「体定界结果」；§四.1/§四.2 三体定界）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyDelimiting {
    /// Content-Length 精确取体（document_fetch.rs:247- 三体定界先例）。
    ContentLength,
    /// Transfer-Encoding: chunked 去帧（含 trailer 区分，trailer 不混入头表）。
    Chunked,
    /// EOF 定界（HTTP/1.0 Connection: close / CONNECT 后明文残余）。
    Eof,
    /// 无体（无 CL/TE 且无残余字节）。
    None,
}

/// 捕获保真词表（DESIGN §1.2 capture_fidelity 行，PLAN :493）。
///
/// `DegradedNoWire`/`Import` 为 Exchange 级语义（采集面未留 wire / 导入面），
/// 解析级产物只产生 `Full`/`Truncated`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureFidelity {
    /// 全量捕获。
    Full,
    /// 超帽截断（PLAN :482「记录明确的 Capture Fidelity 状态和原因」）。
    Truncated,
    /// 未留 wire（采集面未落原始字节）。
    DegradedNoWire,
    /// 导入面（PLAN §8.3 :594「不能伪装成真实捕获」）。
    Import,
}

/// 解析状态词表（DESIGN §1.2 parse_status 行）。
///
/// 不变式 1：解析失败不伪造 Parsed（`Failed` + [`ErrorCode`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParseStatus {
    /// 解析完成。
    Ok,
    /// 部分解析（截断负例「如实标注」面，§四.2）。
    Partial,
    /// 解析失败（不产出 [`ParsedMessage`]）。
    Failed,
}

/// 解析错误码词表（DESIGN §1.2 error_code 行「小写词表」；§四.2 负例全集）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// chunked 分帧缺终结（§四.2 trunc_chunked_no_term）。
    TruncatedChunk,
    /// 头块缺 CRLFCRLF 终结（§四.2 trunc_header_no_crlf）。
    TruncatedHeader,
    /// CL 声明 > 实收（§四.2 trunc_body_short_cl；差值记
    /// `ParseOutcome.body_declared/body_received`）。
    TruncatedBody,
    /// 头帽拒：64KiB 字节帽（document_fetch.rs:259 先例，00 §2.2）+
    /// httparse 头数槽位帽，同一 error_code（§四.2 oversize_header_64k）。
    HeaderCapExceeded,
    /// HTTP/1.1 请求无 Host（fail-closed，不猜 authority，§四.2 req_no_host）。
    MissingHost,
    /// chunk-size 行非法。
    InvalidChunk,
    /// 请求行非法（httparse Error）。
    InvalidRequestLine,
    /// 状态行非法（httparse Error）。
    InvalidStatusLine,
    /// 头行非法（httparse Error）。
    InvalidHeaderLine,
}

/// Parsed：结构化表示（DESIGN §1.1 Parsed 行）。
///
/// 从 [`WireBytes`] 派生（不变式 1），头序/大小写/重复头全部原样保留为
/// [`HeaderRecord`] 有序数组；体为原始字节（保留非 UTF-8，预览走 lossy，
/// POC 先例 `crates/wsw-poc/src/main.rs:45-47`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedMessage {
    /// 消息种类。
    pub kind: MessageKind,
    /// 请求方法（响应为 None）。
    pub method: Option<String>,
    /// 请求目标（原样 request-target：origin-form/absolute-form）。
    pub target: Option<String>,
    /// 响应状态码（请求为 None）。
    pub status: Option<u16>,
    /// 响应 reason phrase（原样，可为空串；请求为 None）。
    pub reason: Option<String>,
    /// HTTP 版本标记（h2 只标记不解析）。
    pub version: HttpVersion,
    /// 头序保持数组（重复头多值保留、大小写原样）。
    pub headers: Vec<HeaderRecord>,
    /// chunked trailer 区（不混入头表，§四.1 res_chunked_trailer）。
    pub trailers: Vec<HeaderRecord>,
    /// 体原始字节（base64 JSON 往返）。
    #[serde(with = "serde_base64")]
    pub body: Vec<u8>,
    /// 体定界结果。
    pub delimiting: BodyDelimiting,
}

impl ParsedMessage {
    /// 体预览（lossy，帽内截断；POC 先例 `crates/wsw-poc/src/main.rs:45-47`）。
    pub fn body_preview_lossy(&self, cap: usize) -> String {
        String::from_utf8_lossy(&self.body[..self.body.len().min(cap)]).into_owned()
    }

    /// host 派生（§一.2「egress 语义先例 egress_proxy.rs:151 host 派生一致」；
    /// 单一实现直接调用 `tool_engine::adapter::host_from_url`，不复制实现）。
    ///
    /// absolute-form 请求目标按 URL 语义派生；origin-form 回落 Host 头
    /// （合成 `http://<host>/` 走同一函数，小写/去 userinfo/去端口/IPv6 剥
    /// 方括号口径一致）。派生失败返回 None（不猜 authority，fail-closed）。
    pub fn derived_host(&self) -> Option<String> {
        if self.kind == MessageKind::Request {
            if let Some(target) = &self.target {
                if target.contains("://") {
                    if let Ok(host) = tool_engine::adapter::host_from_url(target) {
                        return Some(host);
                    }
                }
            }
        }
        let host = self
            .headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case("host"))?
            .value
            .trim()
            .to_string();
        if host.is_empty() {
            return None;
        }
        tool_engine::adapter::host_from_url(&format!("http://{host}/")).ok()
    }

    /// `http::Request` 类型视图（http-body-util `Full` 体；DESIGN §1.1
    /// Parsed 行「http::Request 类型」）。注意：`http::HeaderMap` 迭代序
    /// 规范未定义，头序正典始终是 [`ParsedMessage::headers`] 有序数组。
    pub fn to_http_request(
        &self,
    ) -> Result<http::Request<http_body_util::Full<Bytes>>, HttpViewError> {
        if self.kind != MessageKind::Request {
            return Err(HttpViewError::NotRequest);
        }
        let method = self
            .method
            .as_deref()
            .ok_or(HttpViewError::NotRequest)?
            .to_owned();
        let target = self
            .target
            .as_deref()
            .ok_or(HttpViewError::NotRequest)?
            .to_owned();
        let mut builder = http::Request::builder()
            .method(
                http::Method::from_bytes(method.as_bytes())
                    .map_err(|e| HttpViewError::Invalid(format!("method: {e}")))?,
            )
            .uri(
                http::Uri::try_from(target.as_str())
                    .map_err(|e| HttpViewError::Invalid(format!("uri: {e}")))?,
            )
            .version(http::Version::from(self.version));
        for h in &self.headers {
            builder = builder.header(h.name.as_str(), h.value.as_str());
        }
        builder
            .body(http_body_util::Full::new(Bytes::from(self.body.clone())))
            .map_err(|e| HttpViewError::Invalid(e.to_string()))
    }

    /// `http::Response` 类型视图（同 [`ParsedMessage::to_http_request`] 口径）。
    pub fn to_http_response(
        &self,
    ) -> Result<http::Response<http_body_util::Full<Bytes>>, HttpViewError> {
        if self.kind != MessageKind::Response {
            return Err(HttpViewError::NotResponse);
        }
        let status = self.status.ok_or(HttpViewError::NotResponse)?;
        let status = http::StatusCode::from_u16(status)
            .map_err(|e| HttpViewError::Invalid(format!("status: {e}")))?;
        let mut builder = http::Response::builder()
            .status(status)
            .version(http::Version::from(self.version));
        for h in &self.headers {
            builder = builder.header(h.name.as_str(), h.value.as_str());
        }
        builder
            .body(http_body_util::Full::new(Bytes::from(self.body.clone())))
            .map_err(|e| HttpViewError::Invalid(e.to_string()))
    }

    /// 体尺寸提示（`http_body::SizeHint`；内存体恒 exact）。
    pub fn body_size_hint(&self) -> http_body::SizeHint {
        use http_body::Body as _;
        http_body_util::Full::new(Bytes::from(self.body.clone())).size_hint()
    }
}

/// http 类型视图错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpViewError {
    /// 消息不是请求面。
    NotRequest,
    /// 消息不是响应面。
    NotResponse,
    /// 字段构造失败（method/uri/header 值等越过 http 类型校验）。
    Invalid(String),
}

impl std::fmt::Display for HttpViewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotRequest => write!(f, "message is not a request"),
            Self::NotResponse => write!(f, "message is not a response"),
            Self::Invalid(m) => write!(f, "invalid http view: {m}"),
        }
    }
}

impl std::error::Error for HttpViewError {}

/// Display：展示投影（DESIGN §1.1 Display 行「永不回写」；脱敏唯一挂点
/// [`crate::redact`]，WSW-6 起 UI 消费）。
///
/// 本批只定义 conversion 接口位：头展示序（脱敏后）+ 体 lossy 预览；
/// 解压/字符集/Pretty JSON/Hex/Rendered 属 WSW-6 Decoder 批次。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayPayload {
    /// 头展示序（匹配脱敏词表的头值替换为掩码，名与位置原样）。
    pub headers: Vec<HeaderRecord>,
    /// 体预览（lossy 文本；Hex/Rendered 属 WSW-6）。
    pub body_preview: String,
    /// 预览是否经历 UTF-8 lossy 替换。
    pub body_preview_lossy: bool,
    /// 预览是否因帽截断。
    pub body_preview_truncated: bool,
    /// 被脱敏的头名（原样大小写，保序去重）。
    pub redacted_header_names: Vec<String>,
}

/// 字节字段 JSON base64 编解码 helper（Wire/Outbound/体共用）。
pub(crate) mod serde_base64 {
    use base64::Engine as _;
    use serde::{Deserialize, Deserializer, Serializer};

    const ENGINE: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

    pub(crate) fn serialize<S: Serializer>(v: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&ENGINE.encode(v))
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(deserializer)?;
        ENGINE
            .decode(s.as_bytes())
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_bytes_serde_roundtrip_base64() {
        let raw: &[u8] = b"GET / HTTP/1.1\r\n\r\n";
        let wire = WireBytes::new(raw);
        let json = serde_json::to_string(&wire).unwrap();
        assert_eq!(json, "\"R0VUIC8gSFRUUC8xLjENCg0K\"");
        let back: WireBytes = serde_json::from_str(&json).unwrap();
        assert_eq!(back, wire);
    }

    #[test]
    fn outbound_bytes_serde_roundtrip() {
        let ob = OutboundBytes::new(vec![0x00, 0xff, 0x7f]);
        let json = serde_json::to_string(&ob).unwrap();
        let back: OutboundBytes = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ob);
    }

    #[test]
    fn http_version_vocabulary_and_map() {
        assert_eq!(HttpVersion::as_str(&HttpVersion::Http11), "http/1.1");
        assert_eq!(HttpVersion::as_str(&HttpVersion::H2), "h2");
        assert_eq!(
            HttpVersion::from(http::Version::HTTP_2),
            HttpVersion::H2,
            "h2 只做标记不解析（DESIGN §1.1）"
        );
        assert_eq!(
            http::Version::from(HttpVersion::Http10),
            http::Version::HTTP_10
        );
        assert_eq!(HttpVersion::wire_str(&HttpVersion::H2), None);
        let json = serde_json::to_string(&HttpVersion::Http10).unwrap();
        assert_eq!(json, "\"http/1.0\"");
        let back: HttpVersion = serde_json::from_str(&json).unwrap();
        assert_eq!(back, HttpVersion::Http10);
        assert!(serde_json::from_str::<HttpVersion>("\"http/9.9\"").is_err());
    }

    #[test]
    fn vocabulary_enums_lowercase() {
        assert_eq!(
            serde_json::to_string(&CaptureFidelity::DegradedNoWire).unwrap(),
            "\"degraded_no_wire\""
        );
        assert_eq!(
            serde_json::to_string(&ParseStatus::Partial).unwrap(),
            "\"partial\""
        );
        assert_eq!(
            serde_json::to_string(&ErrorCode::HeaderCapExceeded).unwrap(),
            "\"header_cap_exceeded\""
        );
        assert_eq!(
            serde_json::to_string(&BodyDelimiting::ContentLength).unwrap(),
            "\"content_length\""
        );
        assert_eq!(
            serde_json::to_string(&MessageKind::Request).unwrap(),
            "\"request\""
        );
    }

    #[test]
    fn parsed_message_serde_roundtrip_with_binary_body() {
        let msg = ParsedMessage {
            kind: MessageKind::Request,
            method: Some("POST".into()),
            target: Some("/up".into()),
            status: None,
            reason: None,
            version: HttpVersion::Http11,
            headers: vec![
                HeaderRecord::new("Host", "example.com"),
                HeaderRecord::new("Content-Length", "3"),
            ],
            trailers: vec![],
            body: vec![0x61, 0x62, 0xff],
            delimiting: BodyDelimiting::ContentLength,
        };
        let json = serde_json::to_string_pretty(&msg).unwrap();
        let back: ParsedMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back, msg);
        // 体字节 base64 编码进 JSON（fixtures 期望 JSON 同口径）
        assert!(json.contains("YWL/")); // [0x61,0x62,0xff] -> "YWL/"
    }

    #[test]
    fn http_type_views_and_size_hint() {
        let req = ParsedMessage {
            kind: MessageKind::Request,
            method: Some("GET".into()),
            target: Some("http://example.com/p".into()),
            status: None,
            reason: None,
            version: HttpVersion::Http11,
            headers: vec![HeaderRecord::new("Host", "example.com")],
            trailers: vec![],
            body: vec![],
            delimiting: BodyDelimiting::None,
        };
        let view = req.to_http_request().unwrap();
        assert_eq!(view.method(), http::Method::GET);
        assert_eq!(view.version(), http::Version::HTTP_11);
        assert_eq!(view.uri(), "http://example.com/p");
        assert!(req.to_http_response().is_err());

        let hint = req.body_size_hint();
        assert_eq!(hint.exact(), Some(0), "内存体 SizeHint 恒 exact");

        let resp = ParsedMessage {
            kind: MessageKind::Response,
            method: None,
            target: None,
            status: Some(200),
            reason: Some("OK".into()),
            version: HttpVersion::Http11,
            headers: vec![],
            trailers: vec![],
            body: b"hi".to_vec(),
            delimiting: BodyDelimiting::ContentLength,
        };
        let view = resp.to_http_response().unwrap();
        assert_eq!(view.status(), http::StatusCode::OK);
        assert!(resp.to_http_request().is_err());
    }

    #[test]
    fn display_payload_never_writeback_shape() {
        let d = DisplayPayload {
            headers: vec![HeaderRecord::new("Cookie", "[REDACTED]")],
            body_preview: "abc".into(),
            body_preview_lossy: false,
            body_preview_truncated: false,
            redacted_header_names: vec!["Cookie".into()],
        };
        let back: DisplayPayload =
            serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        assert_eq!(back, d);
    }
}
