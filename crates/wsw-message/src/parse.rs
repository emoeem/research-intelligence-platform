//! HTTP/1.1 wire→Parsed 解析器（httparse 1.10.1 驱动；DESIGN §1.1 Parsed 行）。
//!
//! 支持面（WSW-1-TASK §一.2）：
//! - 请求：origin-form / absolute-form / CONNECT 隧道解密后的明文 HTTP；
//! - 响应：Content-Length / chunked+trailer / HTTP/1.0 EOF；
//! - 体定界：请求按 CL/chunked 或无体语义，响应按 CL/chunked/EOF；chunked 去帧
//!   （trailer 区分，不混入头表）、EOF 定界（先例
//!   `crates/tool-engine/src/document_fetch.rs:247-` 三体定界语义，本实现为
//!   消息级（完整捕获缓冲）而非流式）；
//! - 头序保持、重复头多值保留、头名大小写原样（Parsed 层不规范化）；
//! - 头帽 64KiB（`document_fetch.rs:259` 先例，00 审计 §2.2）。
//!
//! 语义纪律：
//! - 负例「如实标注」而非「修复」（§四.2）：截断产物原样保留已收字节 +
//!   `parse_status=partial` + `error_code`；
//! - 解析失败不伪造 Parsed（DESIGN 不变式 1）：`parse_status=failed` +
//!   `error_code`，`message=None`；
//! - HTTP/1.1 请求缺 Host → fail-closed `missing_host`，不猜 authority；
//! - CL 与 TE:chunked 并存按 RFC 7230 §3.3.3 chunked 优先取体（ smuggling
//!   面，解析层不动原始头表，只决定取体方式）；
//! - 重复/逗号合并 Content-Length 只接受全部相同的十进制值；冲突或非法值 fail-closed，头表仍保留原样；
//! - 本批不做 method/状态码特例（HEAD/204/304 无体语义由调用面按需处理，
//!   conformance 语料不含该面）；
//! - 取体后多余字节（pipelined 下一消息）不并入本消息。

use crate::types::{
    BodyDelimiting, CaptureFidelity, ErrorCode, HeaderRecord, HttpVersion, MessageKind,
    ParseStatus, ParsedMessage,
};
use serde::{Deserialize, Serialize};

/// 头帽（字节；document_fetch.rs:259 先例 64KiB，00 审计 §2.2）。
pub const HEADER_CAP: usize = 64 * 1024;

/// 解析产物（解析器唯一出口；失败/部分/完整三态全部经本结构如实标注）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParseOutcome {
    /// 结构化消息（`parse_status=failed` 或截断到头块时为 None —— 不伪造
    /// Parsed）。
    pub message: Option<ParsedMessage>,
    /// 解析状态词表。
    pub parse_status: ParseStatus,
    /// 错误码词表（ok 时 None）。
    pub error_code: Option<ErrorCode>,
    /// 捕获保真（Full/Truncated；语法拒绝不自动等于截断；degraded_no_wire/import
    /// 由调用方按采集/导入语义赋予，DESIGN §1.2 capture_fidelity 行）。
    pub fidelity: CaptureFidelity,
    /// CL 声明字节数（`truncated_body` 时记录差值用，§四.2）。
    pub body_declared: Option<u64>,
    /// 实收字节数（`truncated_body` 时记录差值用）。
    pub body_received: Option<u64>,
}

impl ParseOutcome {
    fn failed(code: ErrorCode) -> Self {
        Self {
            message: None,
            parse_status: ParseStatus::Failed,
            error_code: Some(code),
            // 解析失败不等于采集截断：语法错误/缺 Host 仍可能是完整 Wire。
            fidelity: CaptureFidelity::Full,
            body_declared: None,
            body_received: None,
        }
    }
}

/// 自动判别解析（状态行 `HTTP/` 前缀 → 响应，否则请求）。
pub fn parse_wire(bytes: &[u8]) -> ParseOutcome {
    if bytes.starts_with(b"HTTP/") {
        parse_response(bytes)
    } else {
        parse_request(bytes)
    }
}

/// 强制请求面解析。
pub fn parse_request(bytes: &[u8]) -> ParseOutcome {
    parse_message(bytes, MessageKind::Request)
}

/// 强制响应面解析。
pub fn parse_response(bytes: &[u8]) -> ParseOutcome {
    parse_message(bytes, MessageKind::Response)
}

/// chunked 解帧结果。
enum ChunkedDecode {
    /// 分帧完整（含 0 块终结与 trailer 区）。
    Complete {
        body: Vec<u8>,
        trailers: Vec<HeaderRecord>,
    },
    /// 缺终结（chunk size 行/块数据/trailer 不完整）——如实标注 partial。
    Incomplete {
        body: Vec<u8>,
        trailers: Vec<HeaderRecord>,
    },
    /// 非法分帧（chunk-size 非法 / 块终结缺失）。
    Invalid,
}

fn find_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}

fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

/// 解帧（同步、完整缓冲面；WSW-2 采集面流式化不在本批范围）。
/// `<hex>[;ext]\r\n <data>\r\n ... 0\r\n <trailer 区>\r\n\r\n`。
fn decode_chunked(data: &[u8]) -> ChunkedDecode {
    let mut body = Vec::new();
    let mut trailers = Vec::new();
    let mut rest = data;
    loop {
        let Some(line_end) = find_crlf(rest) else {
            return ChunkedDecode::Incomplete { body, trailers };
        };
        let size_line = String::from_utf8_lossy(&rest[..line_end]).into_owned();
        rest = &rest[line_end + 2..];
        let hex_part = size_line.split(';').next().unwrap_or("").trim().to_string();
        let Ok(size) = usize::from_str_radix(&hex_part, 16) else {
            return ChunkedDecode::Invalid;
        };
        if size == 0 {
            // trailer 区：逐行读到空行终结；缺终结 → Incomplete（truncated_chunk）
            loop {
                let Some(i) = find_crlf(rest) else {
                    return ChunkedDecode::Incomplete { body, trailers };
                };
                let line = &rest[..i];
                rest = &rest[i + 2..];
                if line.is_empty() {
                    return ChunkedDecode::Complete { body, trailers };
                }
                let line_str = String::from_utf8_lossy(line).into_owned();
                let Some((name, value)) = line_str.split_once(':') else {
                    return ChunkedDecode::Invalid;
                };
                trailers.push(HeaderRecord::new(
                    name.trim().to_string(),
                    value.trim_start_matches(' ').to_string(),
                ));
            }
        }
        if rest.len() < size {
            // 捕获在 chunk 数据中途结束：保留已经收到的全部数据字节，
            // 并由调用方标记 partial/truncated，不能静默丢弃部分 chunk。
            body.extend_from_slice(rest);
            return ChunkedDecode::Incomplete { body, trailers };
        }
        body.extend_from_slice(&rest[..size]);
        rest = &rest[size..];
        if rest.starts_with(b"\r\n") {
            rest = &rest[2..];
        } else if rest.is_empty() {
            // 块数据完整但终结 CRLF 未到 → partial
            return ChunkedDecode::Incomplete { body, trailers };
        } else {
            return ChunkedDecode::Invalid;
        }
    }
}

fn parse_message(bytes: &[u8], kind: MessageKind) -> ParseOutcome {
    // 1. 头块定位 + 头帽（64KiB；head_end 超帽与 buffer 超帽同词表）
    let head_end = match find_head_end(bytes) {
        Some(i) if i > HEADER_CAP => return ParseOutcome::failed(ErrorCode::HeaderCapExceeded),
        Some(i) => i,
        None => {
            if bytes.len() > HEADER_CAP {
                let mut outcome = ParseOutcome::failed(ErrorCode::HeaderCapExceeded);
                // 超帽且头块未终结：无法确认捕获是否完整，按截断保真标注。
                outcome.fidelity = CaptureFidelity::Truncated;
                return outcome;
            }
            // 头块缺 CRLFCRLF 终结：如实标注 partial，无 Parsed（不伪造头表）
            return ParseOutcome {
                message: None,
                parse_status: ParseStatus::Partial,
                error_code: Some(ErrorCode::TruncatedHeader),
                fidelity: CaptureFidelity::Truncated,
                body_declared: None,
                body_received: None,
            };
        }
    };
    // httparse 需要完整的头块终止符 CRLFCRLF；find_head_end 返回终止符
    // 的起点，因此解析切片必须包含完整四字节分隔符。
    let head = &bytes[..head_end + 4];
    let tail = &bytes[head_end + 4..];

    // 2. httparse 头块解析。槽位按 64KiB 头帽动态计算，不额外引入
    // 一个比字节帽更小的隐式头数限制。
    let header_slots = (head.len() / 4).saturating_add(1).max(16);
    let mut header_buf = vec![httparse::EMPTY_HEADER; header_slots];
    enum HeadParse {
        Request {
            method: String,
            target: String,
            version: HttpVersion,
            headers: Vec<HeaderRecord>,
        },
        Response {
            status: u16,
            reason: Option<String>,
            version: HttpVersion,
            headers: Vec<HeaderRecord>,
        },
    }
    let head_parse = match kind {
        MessageKind::Request => {
            // httparse 1.10 API：parse 原地填充字段，返回 Status<usize>（consumed）
            let mut req = httparse::Request::new(&mut header_buf);
            match req.parse(head) {
                Ok(httparse::Status::Complete(_consumed)) => {
                    let version = match req.version {
                        Some(1) => HttpVersion::Http11,
                        Some(0) => HttpVersion::Http10,
                        _ => return ParseOutcome::failed(ErrorCode::InvalidRequestLine),
                    };
                    let headers = req
                        .headers
                        .iter()
                        .map(|h| {
                            HeaderRecord::new(h.name.to_string(), String::from_utf8_lossy(h.value))
                        })
                        .collect();
                    HeadParse::Request {
                        method: req.method.unwrap_or_default().to_string(),
                        target: req.path.unwrap_or_default().to_string(),
                        version,
                        headers,
                    }
                }
                Ok(httparse::Status::Partial) => {
                    // head_end 已定位而 httparse 仍 Partial：防御面归头行非法
                    return ParseOutcome::failed(ErrorCode::InvalidRequestLine);
                }
                Err(httparse::Error::TooManyHeaders) => {
                    return ParseOutcome::failed(ErrorCode::HeaderCapExceeded)
                }
                Err(_) => return ParseOutcome::failed(ErrorCode::InvalidRequestLine),
            }
        }
        MessageKind::Response => {
            let mut resp = httparse::Response::new(&mut header_buf);
            match resp.parse(head) {
                Ok(httparse::Status::Complete(_consumed)) => {
                    let version = match resp.version {
                        Some(1) => HttpVersion::Http11,
                        Some(0) => HttpVersion::Http10,
                        _ => return ParseOutcome::failed(ErrorCode::InvalidStatusLine),
                    };
                    let headers = resp
                        .headers
                        .iter()
                        .map(|h| {
                            HeaderRecord::new(h.name.to_string(), String::from_utf8_lossy(h.value))
                        })
                        .collect();
                    let code = resp.code.unwrap_or_default();
                    let reason = resp.reason.map(|s| s.to_string()).filter(|s| !s.is_empty());
                    HeadParse::Response {
                        status: code,
                        reason,
                        version,
                        headers,
                    }
                }
                Ok(httparse::Status::Partial) => {
                    return ParseOutcome::failed(ErrorCode::InvalidStatusLine)
                }
                Err(httparse::Error::TooManyHeaders) => {
                    return ParseOutcome::failed(ErrorCode::HeaderCapExceeded)
                }
                Err(_) => return ParseOutcome::failed(ErrorCode::InvalidStatusLine),
            }
        }
    };

    let (version, headers, method, target, status, reason) = match head_parse {
        HeadParse::Request {
            method,
            target,
            version,
            headers,
        } => (version, headers, Some(method), Some(target), None, None),
        HeadParse::Response {
            status,
            reason,
            version,
            headers,
        } => (version, headers, None, None, Some(status), reason),
    };

    // 3. fail-closed 语义闸：HTTP/1.1 请求无 Host 不猜 authority（§四.2）
    if kind == MessageKind::Request && version == HttpVersion::Http11 {
        let has_host = headers
            .iter()
            .any(|h| h.name.eq_ignore_ascii_case("host") && !h.value.trim().is_empty());
        if !has_host {
            return ParseOutcome::failed(ErrorCode::MissingHost);
        }
    }

    // 4. 三体定界（chunked 优先 > CL > 响应 EOF）。Transfer-Encoding
    // 当前仅支持单一 chunked coding；不支持/含糊的 coding fail-closed，
    // 防止把未解码的传输编码字节误当成实体正文。
    let transfer_encodings = headers
        .iter()
        .filter(|h| h.name.eq_ignore_ascii_case("transfer-encoding"))
        .flat_map(|h| h.value.split(','))
        .map(str::trim)
        .collect::<Vec<_>>();
    let has_transfer_encoding = headers
        .iter()
        .any(|h| h.name.eq_ignore_ascii_case("transfer-encoding"));
    let te_chunked = if !has_transfer_encoding {
        false
    } else if transfer_encodings.len() == 1 && transfer_encodings[0].eq_ignore_ascii_case("chunked")
    {
        true
    } else {
        return ParseOutcome::failed(ErrorCode::InvalidHeaderLine);
    };

    // 重复/逗号合并的 Content-Length 只接受全数值且全部相同；冲突值
    // fail-closed，避免不同代理/上游采用不同长度导致 request smuggling。
    let mut content_lengths = Vec::new();
    for header in headers
        .iter()
        .filter(|h| h.name.eq_ignore_ascii_case("content-length"))
    {
        for value in header.value.split(',') {
            let value = value.trim();
            if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return ParseOutcome::failed(ErrorCode::InvalidHeaderLine);
            }
            let Ok(length) = value.parse::<u64>() else {
                return ParseOutcome::failed(ErrorCode::InvalidHeaderLine);
            };
            content_lengths.push(length);
        }
    }
    if content_lengths.windows(2).any(|pair| pair[0] != pair[1]) {
        return ParseOutcome::failed(ErrorCode::InvalidHeaderLine);
    }
    let declared_cl = content_lengths.first().copied();

    let mut body_declared: Option<u64> = None;
    let mut body_received: Option<u64> = None;
    let mut error_code: Option<ErrorCode> = None;
    let mut fidelity = CaptureFidelity::Full;
    let mut parse_status = ParseStatus::Ok;

    let (body, trailers, delimiting);
    if te_chunked {
        match decode_chunked(tail) {
            ChunkedDecode::Complete {
                body: b,
                trailers: t,
            } => {
                body = b;
                trailers = t;
                delimiting = BodyDelimiting::Chunked;
            }
            ChunkedDecode::Incomplete {
                body: b,
                trailers: t,
            } => {
                body = b;
                trailers = t;
                delimiting = BodyDelimiting::Chunked;
                parse_status = ParseStatus::Partial;
                error_code = Some(ErrorCode::TruncatedChunk);
                fidelity = CaptureFidelity::Truncated;
            }
            ChunkedDecode::Invalid => {
                return ParseOutcome::failed(ErrorCode::InvalidChunk);
            }
        }
    } else if let Some(declared) = declared_cl {
        if tail.len() as u64 >= declared {
            body = tail[..declared as usize].to_vec();
            trailers = Vec::new();
            delimiting = BodyDelimiting::ContentLength;
        } else {
            // CL 声明 > 实收：如实标注 partial + 差值（§四.2 trunc_body_short_cl）
            body = tail.to_vec();
            trailers = Vec::new();
            delimiting = BodyDelimiting::ContentLength;
            parse_status = ParseStatus::Partial;
            error_code = Some(ErrorCode::TruncatedBody);
            fidelity = CaptureFidelity::Truncated;
            body_declared = Some(declared);
            body_received = Some(tail.len() as u64);
        }
    } else if kind == MessageKind::Request {
        // RFC 9112：请求无 TE/CL 时没有消息体；后续字节可能是下一条
        // pipelined 请求，绝不能把它们吞入当前请求的 EOF body。
        body = Vec::new();
        trailers = Vec::new();
        delimiting = BodyDelimiting::None;
    } else if tail.is_empty() {
        body = Vec::new();
        trailers = Vec::new();
        delimiting = BodyDelimiting::None;
    } else {
        // 响应无 TE/CL 时以连接 EOF 定界。
        body = tail.to_vec();
        trailers = Vec::new();
        delimiting = BodyDelimiting::Eof;
    }

    let message = ParsedMessage {
        kind,
        method,
        target,
        status,
        reason,
        version,
        headers,
        trailers,
        body,
        delimiting,
    };
    ParseOutcome {
        message: Some(message),
        parse_status,
        error_code,
        fidelity,
        body_declared,
        body_received,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_origin_form_cl_zero() {
        let wire = b"GET /p?q=1 HTTP/1.1\r\nHost: example.com\r\nContent-Length: 0\r\n\r\n";
        let out = parse_wire(wire);
        assert_eq!(out.parse_status, ParseStatus::Ok);
        assert_eq!(out.error_code, None);
        let m = out.message.unwrap();
        assert_eq!(m.kind, MessageKind::Request);
        assert_eq!(m.method.as_deref(), Some("GET"));
        assert_eq!(m.target.as_deref(), Some("/p?q=1"));
        assert_eq!(m.version, HttpVersion::Http11);
        assert_eq!(m.delimiting, BodyDelimiting::ContentLength);
        assert!(m.body.is_empty());
        assert_eq!(m.headers.len(), 2);
        assert_eq!(m.headers[0].name, "Host");
        assert_eq!(m.derived_host().as_deref(), Some("example.com"));
    }

    #[test]
    fn chunked_decode_exact_reassembly_with_final_zero() {
        let wire = b"POST /up HTTP/1.1\r\nHost: example.com\r\nTransfer-Encoding: chunked\r\n\r\n\
             3\r\nabc\r\n4\r\ndefg\r\n0\r\n\r\n";
        let out = parse_wire(wire);
        assert_eq!(out.parse_status, ParseStatus::Ok);
        let m = out.message.unwrap();
        assert_eq!(
            m.body,
            b"abcdefg".to_vec(),
            "解帧字节精确重组（含最后 0 块）"
        );
        assert_eq!(m.delimiting, BodyDelimiting::Chunked);
        assert!(m.trailers.is_empty());
    }

    #[test]
    fn chunked_extension_stripped_and_trailers_separated() {
        let wire = b"POST /up HTTP/1.1\r\nHost: example.com\r\nTransfer-Encoding: chunked\r\n\r\n\
             3;ext=1\r\nabc\r\n0\r\nX-Trailer: t1\r\nX-Trailer-2: t2\r\n\r\n";
        let out = parse_wire(wire);
        let m = out.message.unwrap();
        assert_eq!(m.body, b"abc".to_vec(), "chunk 扩展名剥离");
        assert_eq!(m.trailers.len(), 2, "trailer 不混入头表");
        assert_eq!(m.trailers[0], HeaderRecord::new("X-Trailer", "t1"));
        assert_eq!(m.trailers[1], HeaderRecord::new("X-Trailer-2", "t2"));
        assert!(m
            .headers
            .iter()
            .all(|h| h.name != "X-Trailer" && h.name != "X-Trailer-2"));
    }

    #[test]
    fn multi_value_duplicate_headers_preserve_order() {
        let wire =
            b"GET / HTTP/1.1\r\nHost: h.example.com\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\n\r\n";
        let m = parse_wire(wire).message.unwrap();
        assert_eq!(m.headers.len(), 3);
        assert_eq!(m.headers[1], HeaderRecord::new("Set-Cookie", "a=1"));
        assert_eq!(
            m.headers[2],
            HeaderRecord::new("Set-Cookie", "b=2"),
            "重复头多值保序"
        );
    }

    #[test]
    fn mixed_case_header_names_preserved() {
        let wire = b"GET / HTTP/1.1\r\nhOsT: example.com\r\ncOnTeNt-LeNgTh: 0\r\n\r\n";
        let m = parse_wire(wire).message.unwrap();
        assert_eq!(m.headers[0].name, "hOsT", "Parsed 层不规范化");
        assert_eq!(m.headers[1].name, "cOnTeNt-LeNgTh");
        assert_eq!(m.headers[1].value, "0");
    }

    #[test]
    fn absolute_form_target_derives_host() {
        let wire = b"GET http://ProxyUser:pw@Example.COM:8080/p HTTP/1.1\r\nHost: Example.COM:8080\r\n\r\n";
        let m = parse_wire(wire).message.unwrap();
        assert_eq!(m.delimiting, BodyDelimiting::None);
        assert_eq!(
            m.derived_host().as_deref(),
            Some("example.com"),
            "host 派生：小写/去 userinfo/去端口（egress_proxy.rs:151 口径一致）"
        );
    }

    #[test]
    fn http11_request_without_host_fail_closed() {
        let wire = b"GET / HTTP/1.1\r\nUser-Agent: x\r\n\r\n";
        let out = parse_wire(wire);
        assert_eq!(out.parse_status, ParseStatus::Failed);
        assert_eq!(out.error_code, Some(ErrorCode::MissingHost));
        assert_eq!(
            out.fidelity,
            CaptureFidelity::Full,
            "语法拒绝不代表 Wire 丢失"
        );
        assert!(out.message.is_none(), "不伪造 Parsed，不猜 authority");
    }

    #[test]
    fn http10_request_without_host_is_ok() {
        let wire = b"GET / HTTP/1.0\r\n\r\n";
        let out = parse_wire(wire);
        assert_eq!(out.parse_status, ParseStatus::Ok);
        let m = out.message.unwrap();
        assert_eq!(m.version, HttpVersion::Http10);
        assert_eq!(m.delimiting, BodyDelimiting::None);
    }

    #[test]
    fn response_http10_eof_delimited() {
        let wire = b"HTTP/1.0 200 OK\r\nServer: t\r\n\r\nbody-bytes";
        let out = parse_wire(wire);
        assert_eq!(out.parse_status, ParseStatus::Ok);
        let m = out.message.unwrap();
        assert_eq!(m.delimiting, BodyDelimiting::Eof);
        assert_eq!(m.body, b"body-bytes".to_vec());
        assert_eq!(m.status, Some(200));
        assert_eq!(m.reason.as_deref(), Some("OK"));
    }

    #[test]
    fn response_http11_no_cl_te_eof_too() {
        // RFC 7230 §3.3.3 rule 7：HTTP/1.1 响应无 CL/TE 亦以 EOF 定界
        let wire = b"HTTP/1.1 404 Not Found\r\nServer: t\r\nConnection: close\r\n\r\nnope";
        let m = parse_wire(wire).message.unwrap();
        assert_eq!(m.delimiting, BodyDelimiting::Eof);
        assert_eq!(m.body, b"nope".to_vec());
    }

    #[test]
    fn cl_short_declared_gt_received_records_delta() {
        let wire = b"POST / HTTP/1.1\r\nHost: example.com\r\nContent-Length: 10\r\n\r\nabc";
        let out = parse_wire(wire);
        assert_eq!(out.parse_status, ParseStatus::Partial);
        assert_eq!(out.error_code, Some(ErrorCode::TruncatedBody));
        assert_eq!(out.fidelity, CaptureFidelity::Truncated);
        assert_eq!(out.body_declared, Some(10));
        assert_eq!(out.body_received, Some(3), "declared/received 差值记录");
        let m = out.message.unwrap();
        assert_eq!(m.body, b"abc".to_vec(), "已收字节原样保留");
    }

    #[test]
    fn truncated_chunk_partial() {
        let wire =
            b"POST / HTTP/1.1\r\nHost: example.com\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nab";
        let out = parse_wire(wire);
        assert_eq!(out.parse_status, ParseStatus::Partial);
        assert_eq!(out.error_code, Some(ErrorCode::TruncatedChunk));
        assert_eq!(out.fidelity, CaptureFidelity::Truncated);
        let m = out.message.unwrap();
        assert_eq!(m.body, b"ab".to_vec());
        assert_eq!(m.delimiting, BodyDelimiting::Chunked);
    }

    #[test]
    fn truncated_header_partial_no_message() {
        let wire = b"GET / HTTP/1.1\r\nHost: example.com\r\nX-Part: v";
        let out = parse_wire(wire);
        assert_eq!(out.parse_status, ParseStatus::Partial);
        assert_eq!(out.error_code, Some(ErrorCode::TruncatedHeader));
        assert!(out.message.is_none(), "头块未终结不伪造 Parsed");
    }

    #[test]
    fn request_without_framing_does_not_consume_pipelined_next_request() {
        let wire = b"GET /one HTTP/1.1\r\nHost: example.com\r\n\r\nGET /two HTTP/1.1\r\nHost: example.com\r\n\r\n";
        let outcome = parse_wire(wire);
        assert_eq!(outcome.parse_status, ParseStatus::Ok);
        let message = outcome.message.unwrap();
        assert_eq!(message.target.as_deref(), Some("/one"));
        assert!(message.body.is_empty());
        assert_eq!(message.delimiting, BodyDelimiting::None);
    }

    #[test]
    fn identical_content_length_values_are_accepted_but_conflicts_fail_closed() {
        let same = b"POST / HTTP/1.1\r\nHost: example.com\r\nContent-Length: 3\r\nContent-Length: 3\r\n\r\nabc";
        let parsed = parse_wire(same);
        assert_eq!(parsed.parse_status, ParseStatus::Ok);
        assert_eq!(parsed.message.unwrap().body, b"abc");

        let conflict = b"POST / HTTP/1.1\r\nHost: example.com\r\nContent-Length: 3\r\nContent-Length: 5\r\n\r\nabcde";
        let rejected = parse_wire(conflict);
        assert_eq!(rejected.parse_status, ParseStatus::Failed);
        assert_eq!(rejected.error_code, Some(ErrorCode::InvalidHeaderLine));
        assert_eq!(rejected.fidelity, CaptureFidelity::Full);
        assert!(rejected.message.is_none());
    }

    #[test]
    fn unsupported_or_ambiguous_transfer_encoding_fails_closed() {
        for value in ["gzip", "gzip, chunked", "chunked,", "chunked, chunked"] {
            let wire = format!(
                "POST / HTTP/1.1\r\nHost: example.com\r\nTransfer-Encoding: {value}\r\n\r\n0\r\n\r\n"
            );
            let outcome = parse_wire(wire.as_bytes());
            assert_eq!(outcome.parse_status, ParseStatus::Failed, "{value}");
            assert_eq!(
                outcome.error_code,
                Some(ErrorCode::InvalidHeaderLine),
                "{value}"
            );
            assert_eq!(outcome.fidelity, CaptureFidelity::Full, "{value}");
        }
    }

    #[test]
    fn header_slot_count_scales_with_the_64k_byte_cap() {
        let mut wire = String::from("GET / HTTP/1.1\r\nHost: example.com\r\n");
        for index in 0..700 {
            wire.push_str(&format!("X-H{index}: v\r\n"));
        }
        wire.push_str("\r\n");
        let outcome = parse_wire(wire.as_bytes());
        assert_eq!(outcome.parse_status, ParseStatus::Ok);
        assert_eq!(outcome.message.unwrap().headers.len(), 701);
    }

    #[test]
    fn header_cap_exceeded_byte_cap() {
        let pad = "a".repeat(HEADER_CAP + 8);
        let wire = format!("GET / HTTP/1.1\r\nHost: example.com\r\nX-Pad: {pad}\r\n\r\n");
        let out = parse_wire(wire.as_bytes());
        assert_eq!(out.parse_status, ParseStatus::Failed);
        assert_eq!(out.error_code, Some(ErrorCode::HeaderCapExceeded));
        assert!(out.message.is_none());
    }

    #[test]
    fn header_cap_exceeded_no_terminator_over_cap() {
        let pad = "a".repeat(HEADER_CAP + 8);
        let wire = format!("GET / HTTP/1.1\r\nX-Pad: {pad}");
        let out = parse_wire(wire.as_bytes());
        assert_eq!(out.error_code, Some(ErrorCode::HeaderCapExceeded));
    }

    #[test]
    fn cl_wins_priority_chunked_first_when_both_present() {
        // TE:chunked 与 CL 并存：按 RFC 7230 §3.3.3 chunked 优先取体；头表原样
        let wire = b"POST / HTTP/1.1\r\nHost: example.com\r\nContent-Length: 3\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nok\r\n0\r\n\r\n";
        let m = parse_wire(wire).message.unwrap();
        assert_eq!(m.body, b"ok".to_vec(), "chunked 优先");
        assert_eq!(m.delimiting, BodyDelimiting::Chunked);
        assert_eq!(m.headers.len(), 3, "头表原样不动（不修复）");
    }

    #[test]
    fn invalid_chunk_size_failed() {
        let wire =
            b"POST / HTTP/1.1\r\nHost: example.com\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n";
        let out = parse_wire(wire);
        assert_eq!(out.parse_status, ParseStatus::Failed);
        assert_eq!(out.error_code, Some(ErrorCode::InvalidChunk));
    }

    #[test]
    fn ws_upgrade_headers_pass_through() {
        let wire = b"GET /chat HTTP/1.1\r\nHost: example.com\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n";
        let m = parse_wire(wire).message.unwrap();
        let get = |n: &str| {
            m.headers
                .iter()
                .find(|h| h.name == n)
                .map(|h| h.value.clone())
        };
        assert_eq!(get("Upgrade").as_deref(), Some("websocket"));
        assert_eq!(get("Connection").as_deref(), Some("Upgrade"));
        assert_eq!(
            get("Sec-WebSocket-Key").as_deref(),
            Some("dGhlIHNhbXBsZSBub25jZQ=="),
            "Upgrade/Connection 头透传保真"
        );
    }

    #[test]
    fn pipelined_extra_bytes_not_merged() {
        // CL 取体后残余字节 = 下一 pipelined 消息，不并入本消息
        let wire =
            b"POST / HTTP/1.1\r\nHost: example.com\r\nContent-Length: 2\r\n\r\nabGET / HTTP/1.1";
        let m = parse_wire(wire).message.unwrap();
        assert_eq!(m.body, b"ab".to_vec());
    }

    #[test]
    fn response_multi_same_name_headers_ordered() {
        let wire = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nX-Tag: one\r\nX-Tag: two\r\nSet-Cookie: s=1\r\nSet-Cookie: s=2\r\n\r\n";
        let m = parse_wire(wire).message.unwrap();
        assert_eq!(m.headers.len(), 5);
        assert_eq!(m.headers[1].value, "one");
        assert_eq!(m.headers[2].value, "two");
        assert_eq!(m.headers[3].value, "s=1");
        assert_eq!(m.headers[4].value, "s=2");
    }

    #[test]
    fn binary_body_preserved_and_lossy_preview() {
        let wire_bytes: Vec<u8> = {
            let mut v =
                b"POST / HTTP/1.1\r\nHost: example.com\r\nContent-Length: 4\r\n\r\n".to_vec();
            v.extend_from_slice(&[0x61, 0x62, 0x63, 0xff]);
            v
        };
        let out = parse_wire(&wire_bytes);
        assert_eq!(out.parse_status, ParseStatus::Ok);
        assert_eq!(out.fidelity, CaptureFidelity::Full);
        let m = out.message.unwrap();
        assert_eq!(m.body, vec![0x61, 0x62, 0x63, 0xff], "体原始字节保留");
        assert_eq!(m.body_preview_lossy(16), "abc\u{fffd}", "preview 走 lossy");
    }
}

#[cfg(test)]
mod probe {
    #[test]
    fn probe_httparse_error() {
        let head = &b"GET /p?q=1 HTTP/1.1\r\nHost: example.com\r\nContent-Length: 0\r\n"[..];
        let mut buf = [httparse::EMPTY_HEADER; 512];
        let mut req = httparse::Request::new(&mut buf);
        let res = req.parse(head);
        eprintln!(
            "PROBE result = {:?}",
            res.as_ref().map(|s| *s).map_err(|e| format!("{e:?}"))
        );
        eprintln!(
            "PROBE version={:?} method={:?} path={:?}",
            req.version, req.method, req.path
        );
        let _ = res;
    }
}
