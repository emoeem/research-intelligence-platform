//! Parsed/编辑态 → Outbound 字节重序列化（DESIGN §1.1 Outbound 行）。
//!
//! 语义（WSW-1-TASK §一.2/S4）：
//! - 重序列化产物 = [`OutboundBytes`]，发送面 WSW-3 消费；
//! - 编辑头序保持：用户头表原序原样；分界头（Content-Length /
//!   Transfer-Encoding）由本模块**体重定界**：
//!   - [`OutboundFraming::ContentLength`]：首个 CL 头**原位更新**为体长；
//!     缺失则尾部追加；多个 CL 头拒（出站面走私守卫，fail-closed）；
//!   - [`OutboundFraming::Chunked`]：TE:chunked 头缺失则尾部追加；与 CL
//!     头并存拒（RFC 7230 §3.3.3 出站面必须二义-free）；
//! - 入出守卫：方法/头名 token 合法性、头值/请求目标不含 CR/LF（出站面
//!   注入守卫，DESIGN §4.5「凭据不进日志/错误信息」同级的基本正确性面）；
//! - Outbound 只能从编辑态产生（不变式 2）：[`OutboundRequest::from_parsed`]
//!   为唯一 Parsed→编辑态入口，Display 不在产生路径上。

use crate::types::{
    BodyDelimiting, HeaderRecord, HttpVersion, MessageKind, OutboundBytes, ParsedMessage,
};
use serde::{Deserialize, Serialize};

/// 出站体重定界方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboundFraming {
    /// Content-Length 定界（默认重定界方式：编辑体长与原分帧脱钩）。
    ContentLength,
    /// chunked 定界（原消息为 chunked 且编辑不改方式时）。
    Chunked,
}

/// 出站 chunk 尺寸（确定性：小体单块，`0\r\n\r\n` 终结，无 trailer）。
pub const OUTBOUND_CHUNK_SIZE: usize = 16384;

/// 编辑态请求（Repeater/Fuzzer 面输入形状，WSW-3 消费）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboundRequest {
    /// 方法（token 合法性见 [`SerializeError::InvalidMethod`]）。
    pub method: String,
    /// 请求目标（origin-form/absolute-form）。
    pub target: String,
    /// HTTP 版本（仅 http/1.0、http/1.1 可出站）。
    pub version: HttpVersion,
    /// 用户头表（编辑头序保持；分界头按 framing 重定界）。
    pub headers: Vec<HeaderRecord>,
    /// 体（编辑后字节）。
    #[serde(with = "crate::types::serde_base64")]
    pub body: Vec<u8>,
    /// chunked trailer（只在 Chunked framing 下写出）。
    pub trailers: Vec<HeaderRecord>,
    /// 体重定界方式。
    pub framing: OutboundFraming,
}

/// 编辑态响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboundResponse {
    /// HTTP 版本（仅 http/1.0、http/1.1 可出站）。
    pub version: HttpVersion,
    /// 状态码。
    pub status: u16,
    /// reason phrase（None → 不输出 reason，RFC 7230 允许省略）。
    pub reason: Option<String>,
    /// 用户头表（编辑头序保持）。
    pub headers: Vec<HeaderRecord>,
    /// 体（编辑后字节）。
    #[serde(with = "crate::types::serde_base64")]
    pub body: Vec<u8>,
    /// chunked trailer（只在 Chunked framing 下写出）。
    pub trailers: Vec<HeaderRecord>,
    /// 体重定界方式。
    pub framing: OutboundFraming,
}

/// 序列化错误（出站面守卫全集）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SerializeError {
    /// 方法非 RFC token。
    InvalidMethod(String),
    /// 请求目标为空或含 SP/CR/LF。
    InvalidTarget(String),
    /// 版本无 HTTP/1.x wire 形态（h2/h3 只做标记不解析，WSW-1 非目标）。
    InvalidVersion(String),
    /// 状态码越界（100..=599）。
    InvalidStatus(u16),
    /// 头名非 token / 头值含 CR/LF（出站注入守卫）。
    InvalidHeaderLine(String),
    /// 多个 Content-Length 头（出站面走私守卫，fail-closed）。
    DuplicateContentLength,
    /// CL 头与 TE:chunked 并存（RFC 7230 §3.3.3 禁止形态）。
    ContentLengthWithChunked,
    /// 存在 trailer 但选择了 Content-Length framing，不能静默丢弃 trailer。
    TrailersRequireChunked,
    /// 消息种类不匹配。
    NotRequest,
    /// 消息种类不匹配。
    NotResponse,
}

impl std::fmt::Display for SerializeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMethod(m) => write!(f, "invalid method: {m:?}"),
            Self::InvalidTarget(t) => write!(f, "invalid request target: {t:?}"),
            Self::InvalidVersion(v) => write!(f, "version has no http/1.x wire form: {v}"),
            Self::InvalidStatus(s) => write!(f, "invalid status: {s}"),
            Self::InvalidHeaderLine(h) => write!(f, "invalid header line: {h:?}"),
            Self::DuplicateContentLength => write!(f, "duplicate Content-Length headers"),
            Self::ContentLengthWithChunked => {
                write!(
                    f,
                    "Content-Length and Transfer-Encoding: chunked both present"
                )
            }
            Self::TrailersRequireChunked => {
                write!(f, "chunked trailers require chunked outbound framing")
            }
            Self::NotRequest => write!(f, "message is not a request"),
            Self::NotResponse => write!(f, "message is not a response"),
        }
    }
}

impl std::error::Error for SerializeError {}

const fn is_token_char(b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || matches!(
            b,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

fn is_token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_token_char)
}

fn validate_headers(headers: &[HeaderRecord]) -> Result<(), SerializeError> {
    for h in headers {
        if !is_token(&h.name) {
            return Err(SerializeError::InvalidHeaderLine(format!(
                "header name {:?} is not a token",
                h.name
            )));
        }
        if h.value
            .bytes()
            .any(|b| (b < 0x20 && b != b'\t') || b == 0x7f)
        {
            return Err(SerializeError::InvalidHeaderLine(format!(
                "header {} value contains a prohibited control byte",
                h.name
            )));
        }
    }
    Ok(())
}

fn find_header(headers: &[HeaderRecord], name: &str) -> Vec<usize> {
    headers
        .iter()
        .enumerate()
        .filter(|(_, h)| h.name.eq_ignore_ascii_case(name))
        .map(|(i, _)| i)
        .collect()
}

fn has_te_chunked(headers: &[HeaderRecord]) -> Result<bool, SerializeError> {
    let values = headers
        .iter()
        .filter(|h| h.name.eq_ignore_ascii_case("transfer-encoding"))
        .flat_map(|h| h.value.split(','))
        .map(str::trim)
        .collect::<Vec<_>>();
    if values.is_empty() {
        return Ok(false);
    }
    if values.len() == 1 && values[0].eq_ignore_ascii_case("chunked") {
        Ok(true)
    } else {
        Err(SerializeError::InvalidHeaderLine(
            "unsupported or ambiguous Transfer-Encoding".into(),
        ))
    }
}

fn validate_version(version: HttpVersion) -> Result<&'static str, SerializeError> {
    version
        .wire_str()
        .ok_or_else(|| SerializeError::InvalidVersion(version.as_str().to_string()))
}

fn encode_body_chunked(body: &[u8], trailers: &[HeaderRecord], out: &mut Vec<u8>) {
    for chunk in body.chunks(OUTBOUND_CHUNK_SIZE) {
        out.extend_from_slice(format!("{:x}\r\n", chunk.len()).as_bytes());
        out.extend_from_slice(chunk);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b"0\r\n");
    for trailer in trailers {
        out.extend_from_slice(format!("{}: {}\r\n", trailer.name, trailer.value).as_bytes());
    }
    out.extend_from_slice(b"\r\n");
}

/// 请求重序列化（编辑头序保持 + 体重定界）。
pub fn serialize_request(req: &OutboundRequest) -> Result<OutboundBytes, SerializeError> {
    let version = validate_version(req.version)?;
    if !is_token(&req.method) {
        return Err(SerializeError::InvalidMethod(req.method.clone()));
    }
    if req.target.is_empty() || req.target.bytes().any(|b| b <= 0x20 || b == 0x7f) {
        return Err(SerializeError::InvalidTarget(req.target.clone()));
    }
    validate_headers(&req.headers)?;
    validate_headers(&req.trailers)?;
    let cl_positions = find_header(&req.headers, "content-length");
    let te_chunked = has_te_chunked(&req.headers)?;

    let mut headers = req.headers.clone();
    let mut out = Vec::new();
    out.extend_from_slice(format!("{} {} {}\r\n", req.method, req.target, version).as_bytes());
    match req.framing {
        OutboundFraming::Chunked => {
            if !cl_positions.is_empty() {
                return Err(SerializeError::ContentLengthWithChunked);
            }
            if !te_chunked {
                headers.push(HeaderRecord::new("Transfer-Encoding", "chunked"));
            }
            for h in &headers {
                out.extend_from_slice(format!("{}: {}\r\n", h.name, h.value).as_bytes());
            }
            out.extend_from_slice(b"\r\n");
            encode_body_chunked(&req.body, &req.trailers, &mut out);
        }
        OutboundFraming::ContentLength => {
            if !req.trailers.is_empty() {
                return Err(SerializeError::TrailersRequireChunked);
            }
            if te_chunked {
                return Err(SerializeError::ContentLengthWithChunked);
            }
            match cl_positions.as_slice() {
                [] => headers.push(HeaderRecord::new(
                    "Content-Length",
                    req.body.len().to_string(),
                )),
                [i] => headers[*i].value = req.body.len().to_string(),
                _ => return Err(SerializeError::DuplicateContentLength),
            }
            for h in &headers {
                out.extend_from_slice(format!("{}: {}\r\n", h.name, h.value).as_bytes());
            }
            out.extend_from_slice(b"\r\n");
            out.extend_from_slice(&req.body);
        }
    }
    Ok(OutboundBytes::new(out))
}

/// 响应重序列化（同 [`serialize_request`] 守卫与定界语义）。
pub fn serialize_response(resp: &OutboundResponse) -> Result<OutboundBytes, SerializeError> {
    let version = validate_version(resp.version)?;
    if !(100..=599).contains(&resp.status) {
        return Err(SerializeError::InvalidStatus(resp.status));
    }
    if let Some(reason) = &resp.reason {
        if reason
            .bytes()
            .any(|b| (b < 0x20 && b != b'\t') || b == 0x7f)
        {
            return Err(SerializeError::InvalidHeaderLine(format!(
                "reason phrase {reason:?} contains a prohibited control byte"
            )));
        }
    }
    validate_headers(&resp.headers)?;
    validate_headers(&resp.trailers)?;
    let cl_positions = find_header(&resp.headers, "content-length");
    let te_chunked = has_te_chunked(&resp.headers)?;

    let mut headers = resp.headers.clone();
    let mut out = Vec::new();
    match &resp.reason {
        Some(r) => {
            out.extend_from_slice(format!("{} {} {}\r\n", version, resp.status, r).as_bytes())
        }
        None => out.extend_from_slice(format!("{} {}\r\n", version, resp.status).as_bytes()),
    }
    match resp.framing {
        OutboundFraming::Chunked => {
            if !cl_positions.is_empty() {
                return Err(SerializeError::ContentLengthWithChunked);
            }
            if !te_chunked {
                headers.push(HeaderRecord::new("Transfer-Encoding", "chunked"));
            }
            for h in &headers {
                out.extend_from_slice(format!("{}: {}\r\n", h.name, h.value).as_bytes());
            }
            out.extend_from_slice(b"\r\n");
            encode_body_chunked(&resp.body, &resp.trailers, &mut out);
        }
        OutboundFraming::ContentLength => {
            if !resp.trailers.is_empty() {
                return Err(SerializeError::TrailersRequireChunked);
            }
            if te_chunked {
                return Err(SerializeError::ContentLengthWithChunked);
            }
            match cl_positions.as_slice() {
                [] => headers.push(HeaderRecord::new(
                    "Content-Length",
                    resp.body.len().to_string(),
                )),
                [i] => headers[*i].value = resp.body.len().to_string(),
                _ => return Err(SerializeError::DuplicateContentLength),
            }
            for h in &headers {
                out.extend_from_slice(format!("{}: {}\r\n", h.name, h.value).as_bytes());
            }
            out.extend_from_slice(b"\r\n");
            out.extend_from_slice(&resp.body);
        }
    }
    Ok(OutboundBytes::new(out))
}

impl OutboundRequest {
    /// Parsed → 编辑态（不变式 2 的唯一入口；重定界默认 CL）。
    ///
    /// chunked 原消息编辑后默认改 CL 重定界（编辑体长与原分帧脱钩）；
    /// 保留 chunked 出站需显式 `framing = Chunked`。
    pub fn from_parsed(msg: &ParsedMessage) -> Result<Self, SerializeError> {
        if msg.kind != MessageKind::Request {
            return Err(SerializeError::NotRequest);
        }
        let preserve_trailers = !msg.trailers.is_empty();
        let mut headers = msg.headers.clone();
        if preserve_trailers {
            headers.retain(|h| !h.name.eq_ignore_ascii_case("content-length"));
            if !headers
                .iter()
                .any(|h| h.name.eq_ignore_ascii_case("transfer-encoding"))
            {
                headers.push(HeaderRecord::new("Transfer-Encoding", "chunked"));
            }
        } else {
            headers.retain(|h| {
                !h.name.eq_ignore_ascii_case("transfer-encoding")
                    && !(msg.delimiting == BodyDelimiting::Chunked
                        && h.name.eq_ignore_ascii_case("content-length"))
            });
        }
        Ok(Self {
            method: msg.method.clone().ok_or(SerializeError::NotRequest)?,
            target: msg.target.clone().ok_or(SerializeError::NotRequest)?,
            version: msg.version,
            headers,
            body: msg.body.clone(),
            trailers: msg.trailers.clone(),
            framing: if preserve_trailers {
                OutboundFraming::Chunked
            } else {
                OutboundFraming::ContentLength
            },
        })
    }
}

impl OutboundResponse {
    /// Parsed → 编辑态（同 [`OutboundRequest::from_parsed`]）。
    pub fn from_parsed(msg: &ParsedMessage) -> Result<Self, SerializeError> {
        if msg.kind != MessageKind::Response {
            return Err(SerializeError::NotResponse);
        }
        let preserve_trailers = !msg.trailers.is_empty();
        let mut headers = msg.headers.clone();
        if preserve_trailers {
            headers.retain(|h| !h.name.eq_ignore_ascii_case("content-length"));
            if !headers
                .iter()
                .any(|h| h.name.eq_ignore_ascii_case("transfer-encoding"))
            {
                headers.push(HeaderRecord::new("Transfer-Encoding", "chunked"));
            }
        } else {
            headers.retain(|h| {
                !h.name.eq_ignore_ascii_case("transfer-encoding")
                    && !(msg.delimiting == BodyDelimiting::Chunked
                        && h.name.eq_ignore_ascii_case("content-length"))
            });
        }
        Ok(Self {
            version: msg.version,
            status: msg.status.ok_or(SerializeError::NotResponse)?,
            reason: msg.reason.clone(),
            headers,
            body: msg.body.clone(),
            trailers: msg.trailers.clone(),
            framing: if preserve_trailers {
                OutboundFraming::Chunked
            } else {
                OutboundFraming::ContentLength
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_wire;
    use crate::types::BodyDelimiting;

    fn request(
        method: &str,
        target: &str,
        headers: Vec<HeaderRecord>,
        body: &[u8],
    ) -> OutboundRequest {
        OutboundRequest {
            method: method.into(),
            target: target.into(),
            version: HttpVersion::Http11,
            headers,
            body: body.to_vec(),
            trailers: Vec::new(),
            framing: OutboundFraming::ContentLength,
        }
    }

    #[test]
    fn roundtrip_semantic_equivalence_cl() {
        let wire = b"GET /p?q=1 HTTP/1.1\r\nHost: example.com\r\nUser-Agent: wsw\r\nContent-Length: 0\r\n\r\n";
        let parsed = parse_wire(wire).message.unwrap();
        let outbound = OutboundRequest::from_parsed(&parsed).unwrap();
        let bytes = serialize_request(&outbound).unwrap();
        let reparsed = parse_wire(bytes.as_bytes()).message.unwrap();
        assert_eq!(reparsed.method, parsed.method);
        assert_eq!(reparsed.target, parsed.target);
        assert_eq!(reparsed.version, parsed.version);
        assert_eq!(
            reparsed.headers, parsed.headers,
            "CL 原位更新为同值，头序逐位相等"
        );
        assert_eq!(reparsed.body, parsed.body);
        // 字节级：wire 与 outbound 全等（同 framing 同值）
        assert_eq!(bytes.as_bytes(), &wire[..]);
    }

    #[test]
    fn edited_header_keeps_order_and_bytes() {
        let wire = b"GET / HTTP/1.1\r\nHost: example.com\r\nUser-Agent: old\r\nX-Trace: t1\r\nContent-Length: 0\r\n\r\n";
        let parsed = parse_wire(wire).message.unwrap();
        let mut outbound = OutboundRequest::from_parsed(&parsed).unwrap();
        // 编辑 User-Agent（中部头，前序后序均不动）
        outbound.headers[1].value = "new".into();
        let bytes = serialize_request(&outbound).unwrap();
        let expected: &[u8] =
            b"GET / HTTP/1.1\r\nHost: example.com\r\nUser-Agent: new\r\nX-Trace: t1\r\nContent-Length: 0\r\n\r\n";
        assert_eq!(bytes.as_bytes(), expected, "编辑头序保持，字节级 diff 断言");
        // 编辑体长 → CL 原位更新
        outbound.body = b"payload".to_vec();
        let bytes = serialize_request(&outbound).unwrap();
        let expected: &[u8] =
            b"GET / HTTP/1.1\r\nHost: example.com\r\nUser-Agent: new\r\nX-Trace: t1\r\nContent-Length: 7\r\n\r\npayload";
        assert_eq!(bytes.as_bytes(), expected);
    }

    #[test]
    fn missing_cl_appended_at_tail() {
        let req = request(
            "GET",
            "/x",
            vec![HeaderRecord::new("Host", "example.com")],
            b"",
        );
        let bytes = serialize_request(&req).unwrap();
        assert_eq!(
            bytes.as_bytes(),
            b"GET /x HTTP/1.1\r\nHost: example.com\r\nContent-Length: 0\r\n\r\n"
        );
    }

    #[test]
    fn chunked_outbound_framing_exact_bytes() {
        let req = request(
            "POST",
            "/up",
            vec![HeaderRecord::new("Host", "example.com")],
            b"abcdefg",
        );
        let mut req = req;
        req.framing = OutboundFraming::Chunked;
        let bytes = serialize_request(&req).unwrap();
        assert_eq!(
            bytes.as_bytes(),
            b"POST /up HTTP/1.1\r\nHost: example.com\r\nTransfer-Encoding: chunked\r\n\r\n7\r\nabcdefg\r\n0\r\n\r\n"
        );
        // TE 头已存在则不重复追加
        let mut req2 = req.clone();
        req2.headers
            .push(HeaderRecord::new("Transfer-Encoding", "chunked"));
        let bytes2 = serialize_request(&req2).unwrap();
        assert_eq!(bytes2.as_bytes(), bytes.as_bytes());
        // 回读语义等价
        let reparsed = parse_wire(bytes2.as_bytes()).message.unwrap();
        assert_eq!(reparsed.body, b"abcdefg".to_vec());
        assert_eq!(reparsed.delimiting, BodyDelimiting::Chunked);
    }

    #[test]
    fn outbound_guards_fail_closed() {
        let req = request(
            "GET",
            "/x",
            vec![
                HeaderRecord::new("Host", "example.com"),
                HeaderRecord::new("Content-Length", "0"),
                HeaderRecord::new("content-length", "5"),
            ],
            b"",
        );
        assert_eq!(
            serialize_request(&req).unwrap_err(),
            SerializeError::DuplicateContentLength
        );
        let mut bad = request("GET", "/x", vec![HeaderRecord::new("Host", "h")], b"");
        bad.headers
            .push(HeaderRecord::new("X-Inject", "a\r\nX-Evil: 1"));
        assert!(matches!(
            serialize_request(&bad).unwrap_err(),
            SerializeError::InvalidHeaderLine(_)
        ));
        bad.headers.clear();
        bad.headers.push(HeaderRecord::new("Bad Name", "v"));
        assert!(matches!(
            serialize_request(&bad).unwrap_err(),
            SerializeError::InvalidHeaderLine(_)
        ));
        bad.headers.clear();
        bad.method = "GE T".into();
        assert!(matches!(
            serialize_request(&bad).unwrap_err(),
            SerializeError::InvalidMethod(_)
        ));
        bad.method = "GET".into();
        bad.target = "/a b".into();
        assert!(matches!(
            serialize_request(&bad).unwrap_err(),
            SerializeError::InvalidTarget(_)
        ));
        bad.target = "/a".into();
        bad.version = HttpVersion::H2;
        assert!(matches!(
            serialize_request(&bad).unwrap_err(),
            SerializeError::InvalidVersion(_)
        ));
    }

    #[test]
    fn cl_and_te_both_present_rejected_outbound() {
        let mut req = request(
            "POST",
            "/x",
            vec![
                HeaderRecord::new("Host", "h"),
                HeaderRecord::new("Transfer-Encoding", "chunked"),
            ],
            b"",
        );
        req.framing = OutboundFraming::ContentLength;
        assert_eq!(
            serialize_request(&req).unwrap_err(),
            SerializeError::ContentLengthWithChunked
        );
        req.framing = OutboundFraming::Chunked;
        req.headers.push(HeaderRecord::new("Content-Length", "0"));
        assert_eq!(
            serialize_request(&req).unwrap_err(),
            SerializeError::ContentLengthWithChunked
        );
    }

    #[test]
    fn response_roundtrip_and_reason() {
        let wire = b"HTTP/1.1 200 OK\r\nServer: t\r\nContent-Length: 5\r\n\r\nhello";
        let parsed = parse_wire(wire).message.unwrap();
        let outbound = OutboundResponse::from_parsed(&parsed).unwrap();
        let bytes = serialize_response(&outbound).unwrap();
        assert_eq!(bytes.as_bytes(), &wire[..], "响应全等往返");
        let reparsed = parse_wire(bytes.as_bytes()).message.unwrap();
        assert_eq!(reparsed.status, Some(200));
        assert_eq!(reparsed.reason.as_deref(), Some("OK"));
        assert_eq!(reparsed.body, b"hello".to_vec());

        // reason 缺省省略（RFC 7230）
        let mut resp = OutboundResponse::from_parsed(&parsed).unwrap();
        resp.reason = None;
        let bytes = serialize_response(&resp).unwrap();
        assert_eq!(
            bytes.as_bytes(),
            b"HTTP/1.1 200\r\nServer: t\r\nContent-Length: 5\r\n\r\nhello"
        );
    }

    #[test]
    fn parsed_chunked_without_trailers_reframes_to_content_length() {
        let wire = b"POST /upload HTTP/1.1\r\nHost: example.com\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\n\r\n";
        let parsed = parse_wire(wire).message.unwrap();
        let outbound = OutboundRequest::from_parsed(&parsed).unwrap();
        assert_eq!(outbound.framing, OutboundFraming::ContentLength);
        assert!(outbound
            .headers
            .iter()
            .all(|header| !header.name.eq_ignore_ascii_case("transfer-encoding")));
        let bytes = serialize_request(&outbound).unwrap();
        let reparsed = parse_wire(bytes.as_bytes()).message.unwrap();
        assert_eq!(reparsed.body, b"abc");
        assert_eq!(reparsed.delimiting, BodyDelimiting::ContentLength);
    }

    #[test]
    fn parsed_chunked_trailers_survive_outbound_roundtrip() {
        let wire = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\nX-Checksum: yes\r\n\r\n";
        let parsed = parse_wire(wire).message.unwrap();
        let outbound = OutboundResponse::from_parsed(&parsed).unwrap();
        assert_eq!(outbound.framing, OutboundFraming::Chunked);
        assert_eq!(
            outbound.trailers,
            vec![HeaderRecord::new("X-Checksum", "yes")]
        );
        let bytes = serialize_response(&outbound).unwrap();
        let reparsed = parse_wire(bytes.as_bytes()).message.unwrap();
        assert_eq!(reparsed.body, b"abc");
        assert_eq!(reparsed.trailers, parsed.trailers);
        assert_eq!(reparsed.delimiting, BodyDelimiting::Chunked);
    }

    #[test]
    fn content_length_framing_refuses_to_drop_trailers() {
        let mut req = request(
            "POST",
            "/x",
            vec![HeaderRecord::new("Host", "example.com")],
            b"x",
        );
        req.trailers.push(HeaderRecord::new("X-Trace", "kept"));
        assert_eq!(
            serialize_request(&req).unwrap_err(),
            SerializeError::TrailersRequireChunked
        );
    }

    #[test]
    fn invalid_status_rejected() {
        let wire = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
        let parsed = parse_wire(wire).message.unwrap();
        let mut resp = OutboundResponse::from_parsed(&parsed).unwrap();
        resp.status = 42;
        assert_eq!(
            serialize_response(&resp).unwrap_err(),
            SerializeError::InvalidStatus(42)
        );
    }
}
