//! `wsw_runtime::sender`：Repeater 发送引擎（WSW-3-ADR W3-D4；PLAN §6.4）。
//!
//! 发送路径（Rust-First，禁 reqwest）：
//! `OutboundRequest` → [`wsw_message::serialize::serialize_request`]（Outbound
//! 唯一事实，编辑头序保持）→ **原始 h1 字节写**（`tokio::net::TcpStream` /
//! `tokio_rustls::TlsConnector`）→ 响应字节缓冲（帽 max_body_bytes）→
//! `wsw_message::parse::parse_wire`。
//!
//! 否决 hyper client 写路径（W3-D4.3）：`http::HeaderMap` 迭代序 ≠ 编辑头序
//! —— hyper 会静默重排/重定界用户构造的请求，违反 Outbound 不变式 2；本模块
//! 直接写字节，序列化只经 `serialize_request` 单点。
//!
//! - 重定向策略 [`RedirectPolicy`]：none / follow（max_redirects 默认 10）；
//!   跟随 = 逐跳从 Location 重建 OutboundRequest + `serialize_request` 重序列化
//!   （每跳都是显式 Outbound）+ [`scope::check_redirect_hop`] 帽逐跳计数接线
//!   （W2-D5 查点 B 收口）+ 每跳 DNS 解析 IP 过 [`scope::audit_resolved`]（查
//!   点 C，Pinned-IP 直连）；跳数帽双源取 min：`policy.max_redirects`（--policy
//!   JSON 用户面，只可收紧）× `scope rules.max_redirects`（安全边界兜底），
//!   document_fetch `MIN(cap)` 同型（document_fetch.rs:757）；Location 相对
//!   引用（RFC 3986 §5）按 §5.2 merge + §5.2.4 remove_dot_segments 归一化，
//!   protocol-relative（`//host/path`）按 host 切换；
//! - 超时三件：connect/read/total（policy 默认 10s/30s/60s）；取消面 =
//!   `tokio::sync::watch`（rp-wsw send SIGTERM graceful → cancelled）；
//! - scope default deny：include 空 = 全拒（PLAN :527 主动面不自动扩大范围）；
//!   越界拒 → state=failed + error_code=scope_blocked。
//!
//! 落库面（W3-D4.6/D7 分治）：send 子命令自落 `wsw_repeater_send` 行
//! （pending→sending→终态）+ `wsw_exchange` 行（source=repeater，capture_
//! fidelity='full'——outbound bytes 即原始 wire）+ outbound bytes 全量 CAS
//! （request_outbound_sha256 = 逐字节重发唯一依据，W3-D6.3）。

use crate::persist::{ExchangeRow, PersistStore, SendPatch};
use crate::scope::{self, ScopeRules, ScopeTarget};
use hudsucker::hyper::Uri;
use hudsucker::rustls::crypto::aws_lc_rs;
use hudsucker::rustls::pki_types::{pem::PemObject, CertificateDer};
use hudsucker::rustls::{ClientConfig, RootCertStore};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::watch;
use tokio_rustls::TlsConnector;
use wsw_message::parse::{ParseOutcome, HEADER_CAP};
use wsw_message::serialize::{OutboundFraming, OutboundRequest};
use wsw_message::types::ParseStatus;

/// 重定向策略（W3-D4.4）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedirectPolicy {
    /// 不跟随（默认；3xx 原样返回）。
    #[default]
    None,
    /// 跟随（帽 max_redirects；逐跳 scope 闸 + IP 审计）。
    Follow,
}

/// 发送策略（policy JSON；W3-D4.5 默认 connect 10s / read 30s / total 60s）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendPolicy {
    /// TCP/TLS 连接超时 ms。
    #[serde(default = "default_connect_timeout_ms")]
    pub connect_timeout_ms: u64,
    /// 单跳响应读超时 ms。
    #[serde(default = "default_read_timeout_ms")]
    pub read_timeout_ms: u64,
    /// 全程总超时 ms。
    #[serde(default = "default_total_timeout_ms")]
    pub total_timeout_ms: u64,
    /// 重定向策略。
    #[serde(default)]
    pub redirect: RedirectPolicy,
    /// 重定向跳数帽（follow 时生效；default 10）。生效帽 = min(本值, scope
    /// rules.max_redirects)——policy 只可收紧，scope rules 帽兜底不放大。
    #[serde(default = "default_max_redirects")]
    pub max_redirects: u32,
    /// 响应体读帽字节（默认 8MiB；与 ListenerConfig.max_body_bytes 同源）。
    #[serde(default = "default_max_body_bytes")]
    pub max_body_bytes: u64,
}

fn default_connect_timeout_ms() -> u64 {
    10_000
}
fn default_read_timeout_ms() -> u64 {
    30_000
}
fn default_total_timeout_ms() -> u64 {
    60_000
}
fn default_max_redirects() -> u32 {
    10
}
fn default_max_body_bytes() -> u64 {
    8 * 1024 * 1024
}

impl Default for SendPolicy {
    fn default() -> Self {
        Self {
            connect_timeout_ms: default_connect_timeout_ms(),
            read_timeout_ms: default_read_timeout_ms(),
            total_timeout_ms: default_total_timeout_ms(),
            redirect: RedirectPolicy::None,
            max_redirects: default_max_redirects(),
            max_body_bytes: default_max_body_bytes(),
        }
    }
}

impl SendPolicy {
    /// policy JSON 解析（rp-wsw send --policy 文件内容）。
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// 逐跳重定向链记录（redirect_chain_json 列项）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedirectHop {
    /// 该跳响应状态码。
    pub status: u16,
    /// Location 头原值。
    pub location: String,
    /// 解析后的下一跳完整 URL。
    pub url: String,
    /// 该跳连接对端 IP。
    pub remote_ip: Option<String>,
}

/// 发送终态产出（六态词表的机器面：succeeded/failed/timed_out/cancelled）。
#[derive(Debug, Clone)]
pub struct SendOutcome {
    /// 'succeeded'/'failed'/'timed_out'/'cancelled'（pending/sending 为行级
    /// 中间态，不出现在引擎产出）。
    pub state: &'static str,
    /// 机器可读错误码（scope_blocked/redirect_limit/connect_failed/
    /// tls_failed/read_failed/serialize_failed/cap_exceeded/…）。
    pub error_code: Option<String>,
    /// 人类可读原因（error_code 辅助；凭据不落）。
    pub reason: Option<String>,
    /// 最终响应状态码（跟随重定向 = 末跳）。
    pub response_status: Option<u16>,
    /// 最终响应原始字节（帽内；parse_wire 输入）。
    pub response_bytes: Vec<u8>,
    /// 解析产物。
    pub parsed: Option<ParseOutcome>,
    /// 重定向链（none 策略恒空）。
    pub redirect_chain: Vec<RedirectHop>,
    /// 末跳 outbound bytes sha256（CAS 落库后回填，逐字节重发依据）。
    pub outbound_sha256: Option<String>,
    /// 末跳 outbound bytes（CAS 写面在调用方；引擎返回字节本身）。
    pub outbound_bytes: Vec<u8>,
    /// 末跳对端 IP。
    pub remote_ip: Option<IpAddr>,
    /// 全程耗时 ms。
    pub duration_ms: i64,
    /// 首跳 TTFB ms（首字节响应头到达；失败面 None 语义由调用方落 NULL）。
    pub ttfb_ms: Option<i64>,
}

impl SendOutcome {
    /// 引擎产出 → send 行终态 patch（DB 写面收敛单点）。
    pub fn to_patch(&self, exchange_id: Option<String>) -> SendPatch {
        SendPatch {
            state: self.state,
            exchange_id,
            request_outbound_sha256: self.outbound_sha256.clone(),
            redirect_chain_json: Some(
                serde_json::to_string(&self.redirect_chain).unwrap_or_else(|_| "[]".into()),
            ),
            response_status: self.response_status.map(|s| s as i64),
            response_body_sha256: (!self.response_bytes.is_empty())
                .then(|| wsw_message::cas::sha256_hex(&self.response_bytes)),
            response_body_len: Some(self.response_bytes.len() as i64),
            response_mime: self.parsed.as_ref().and_then(|p| {
                p.message.as_ref().and_then(|m| {
                    m.headers
                        .iter()
                        .find(|h| h.name.eq_ignore_ascii_case("content-type"))
                        .map(|h| h.value.clone())
                })
            }),
            remote_ip: self.remote_ip.map(|ip| ip.to_string()),
            started_at: None,
            completed_at: Some(crate::persist::now_iso()),
            duration_ms: Some(self.duration_ms),
            ttfb_ms: self.ttfb_ms,
            error_code: self.error_code.clone(),
        }
    }
}

/// 发送错误（引擎内聚；不侵入 ProxyErrorClass——那是 MITM 面词表）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendError {
    /// serialize_request 拒（InvalidMethod/InvalidTarget/头注入守卫…）。
    Serialize(String),
    /// scope 闸拒（查点 A/B 词表 reason.code()）。
    Scope(String),
    /// 连接失败（含超时）。
    Connect(String),
    /// TLS 握手失败。
    Tls(String),
    /// 读响应失败（非超时）。
    Read(String),
    /// 超时（connect/read/total；六态 timed_out 语义面）。
    Timeout(String),
    /// 响应体超帽。
    CapExceeded(u64),
    /// 取消（SIGTERM 面）。
    Cancelled,
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SendError::Serialize(m) => write!(f, "serialize: {m}"),
            SendError::Scope(m) => write!(f, "scope_blocked: {m}"),
            SendError::Connect(m) => write!(f, "connect: {m}"),
            SendError::Tls(m) => write!(f, "tls: {m}"),
            SendError::Read(m) => write!(f, "read: {m}"),
            SendError::Timeout(m) => write!(f, "timeout: {m}"),
            SendError::CapExceeded(cap) => write!(f, "response cap exceeded: {cap}"),
            SendError::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::error::Error for SendError {}

/// 请求目标解析产物（scheme/host/port/origin-form path+query）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct RequestTarget {
    scheme: String,
    host: String,
    port: u16,
    path: String,
}

/// OutboundRequest → 发送目标（absolute-form 直取 URI；origin-form 回落
/// Host 头——fail-closed：两者皆缺 → Serialize 错误）。
fn resolve_target(req: &OutboundRequest) -> Result<RequestTarget, SendError> {
    let default_port_for = |scheme: &str| if scheme == "https" { 443 } else { 80 };
    if req.target.contains("://") {
        let uri: Uri = req
            .target
            .parse()
            .map_err(|e| SendError::Serialize(format!("absolute target parse: {e}")))?;
        let scheme = uri.scheme_str().unwrap_or("http").to_string();
        let host = uri
            .host()
            .ok_or_else(|| SendError::Serialize("target has no host".into()))?
            .trim_matches(['[', ']'])
            .to_string();
        let port = uri
            .port_u16()
            .unwrap_or_else(|| default_port_for(&scheme));
        let path = if uri.path().is_empty() {
            "/".to_string()
        } else {
            format!("{}{}", uri.path(), uri.query().map(|q| format!("?{q}")).unwrap_or_default())
        };
        return Ok(RequestTarget { scheme, host, port, path });
    }
    let host_header = req
        .headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case("host"))
        .map(|h| h.value.clone())
        .ok_or_else(|| SendError::Serialize("origin-form target requires Host header".into()))?;
    let (host, port) = split_authority(&host_header, default_port_for("http"));
    Ok(RequestTarget {
        scheme: "http".to_string(),
        host,
        port,
        path: if req.target.is_empty() {
            "/".to_string()
        } else {
            req.target.clone()
        },
    })
}

/// host:port 拆分（IPv6 括号形态兼容；缺省端口 80——https 走 absolute-form 面）。
fn split_authority(authority: &str, default_port: u16) -> (String, u16) {
    let host_port = match authority.rsplit_once('@') {
        Some((_, h)) => h,
        None => authority,
    };
    if let Some(stripped) = host_port.strip_prefix('[') {
        if let Some((inner, rest)) = stripped.split_once(']') {
            let port = rest
                .strip_prefix(':')
                .and_then(|p| p.parse::<u16>().ok())
                .unwrap_or(default_port);
            return (inner.to_string(), port);
        }
    }
    match host_port.rsplit_once(':') {
        Some((h, p)) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) => {
            (h.to_string(), p.parse::<u16>().unwrap_or(default_port))
        }
        _ => (host_port.to_string(), default_port),
    }
}

/// DNS 解析（IP 字面量直取；域名系统解析全地址去重）。
fn resolve_ips(host: &str, port: u16) -> Result<Vec<IpAddr>, SendError> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(vec![ip]);
    }
    let addrs: Vec<SocketAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|e| SendError::Connect(format!("dns resolve {host}: {e}")))?
        .collect();
    if addrs.is_empty() {
        return Err(SendError::Connect(format!("dns resolved no addresses for {host}")));
    }
    let mut ips = Vec::new();
    for a in addrs {
        if !ips.contains(&a.ip()) {
            ips.push(a.ip());
        }
    }
    Ok(ips)
}

/// 上游根库（webpki 公共根 + 追加信任 CA；listener.rs build_upstream_roots
/// 同型面——sender 独立组装避免依赖私有函数）。
fn build_upstream_roots(extra: &[PathBuf]) -> Result<RootCertStore, SendError> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    for path in extra {
        for cert in CertificateDer::pem_file_iter(path)
            .map_err(|e| SendError::Tls(format!("read upstream ca {}: {e}", path.display())))?
        {
            roots
                .add(cert.map_err(|e| SendError::Tls(format!("parse upstream ca: {e}")))?)
                .map_err(|e| SendError::Tls(format!("add upstream ca: {e}")))?;
        }
    }
    Ok(roots)
}

/// TLS connector 组装（每次 send 构造一次；extra_ca 为空走 webpki 公共根）。
fn tls_connector(extra_ca: &[PathBuf]) -> Result<TlsConnector, SendError> {
    let roots = build_upstream_roots(extra_ca)?;
    let config = ClientConfig::builder_with_provider(Arc::new(aws_lc_rs::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| SendError::Tls(format!("tls protocol versions: {e}")))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(TlsConnector::from(Arc::new(config)))
}

/// 双协议流（TcpStream / TlsStream 统一读写面；trait-object 不适用——
/// AsyncReadExt/AsyncWriteExt 泛型方法非 dyn-compatible）。
enum AnyStream {
    Plain(TcpStream),
    Tls(Box<tokio_rustls::client::TlsStream<TcpStream>>),
}

impl AnyStream {
    async fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            AnyStream::Plain(s) => AsyncReadExt::read(s, buf).await,
            AnyStream::Tls(s) => AsyncReadExt::read(s, buf).await,
        }
    }

    async fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        match self {
            AnyStream::Plain(s) => AsyncWriteExt::write_all(s, buf).await,
            AnyStream::Tls(s) => AsyncWriteExt::write_all(s, buf).await,
        }
    }

    async fn flush(&mut self) -> std::io::Result<()> {
        match self {
            AnyStream::Plain(s) => AsyncWriteExt::flush(s).await,
            AnyStream::Tls(s) => AsyncWriteExt::flush(s).await,
        }
    }
}

/// 建连（Pinned-IP；明文/HTTPS 分叉；connect 超时 + 取消选择面）。
async fn connect_stream(
    target: &RequestTarget,
    ip: IpAddr,
    tls: Option<&TlsConnector>,
    connect_timeout: Duration,
    cancel: &watch::Receiver<bool>,
) -> Result<AnyStream, SendError> {
    let addr = SocketAddr::new(ip, target.port);
    let connect = async {
        let tcp = TcpStream::connect(addr)
            .await
            .map_err(|e| SendError::Connect(format!("{addr}: {e}")))?;
        match tls {
            Some(connector) => {
                let server_name =
                    hudsucker::rustls::pki_types::ServerName::try_from(target.host.clone())
                        .map_err(|e| SendError::Tls(format!("server name {}: {e}", target.host)))?;
                let stream = connector
                    .connect(server_name, tcp)
                    .await
                    .map_err(|e| SendError::Tls(format!("handshake {}: {e}", target.host)))?;
                Ok(AnyStream::Tls(Box::new(stream)))
            }
            None => Ok(AnyStream::Plain(tcp)),
        }
    };
    tokio::select! {
        res = connect => res,
        _ = tokio::time::sleep(connect_timeout) => {
            Err(SendError::Timeout(format!("connect timeout after {connect_timeout:?}")))
        }
        _ = cancelled(cancel.clone()) => Err(SendError::Cancelled),
    }
}

/// 取消面（watch bool=true = 收到 SIGTERM/graceful cancel）。
async fn cancelled(cancel: watch::Receiver<bool>) {
    let mut cancel = cancel;
    while !*cancel.borrow_and_update() {
        if cancel.changed().await.is_err() {
            // 发送端已撤（进程退出路径）→ 永不触发取消。
            std::future::pending::<()>().await;
        }
    }
}

/// 双协议流抽象（TcpStream / TlsStream 统一读写面）。
/// 响应读取（帽 + 定界）：读头块（CRLFCRLF）→ CL/chunked/EOF 定界补体；
/// 超帽即断（CapExceeded）。单跳 read 超时 + 取消全程选择面。
async fn read_response(
    stream: &mut AnyStream,
    cap: u64,
    read_timeout: Duration,
    deadline: Instant,
    cancel: &watch::Receiver<bool>,
) -> Result<Vec<u8>, SendError> {
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut chunk = [0u8; 8192];
    // 1) 头块（≤HEADER_CAP）。
    let head_end = loop {
        if let Some(pos) = find_head_end(&buf) {
            break pos;
        }
        if buf.len() > HEADER_CAP {
            return Err(SendError::Read("header block exceeds 64KiB cap".into()));
        }
        if Instant::now() >= deadline {
            return Err(SendError::Timeout("total timeout during response head".into()));
        }
        tokio::select! {
            n = stream.read(&mut chunk) => {
                match n {
                    Ok(0) => break buf.len(), // EOF：无体响应（如 204）
                    Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    Err(e) => return Err(SendError::Read(format!("read head: {e}"))),
                }
            }
            _ = tokio::time::sleep(read_timeout) => {
                return Err(SendError::Timeout(format!("read timeout after {read_timeout:?}")));
            }
            _ = cancelled(cancel.clone()) => return Err(SendError::Cancelled),
        }
    };
    // 2) 体定界（Content-Length / chunked 终结 / EOF）。
    let framing = detect_framing(&buf[..head_end]);
    let target_len: Option<usize> = match framing {
        Framing::ContentLength(cl) => Some(head_end + cl),
        Framing::ChunkedTerminator => None, // 读到 0\r\n\r\n
        Framing::Eof => None,
    };
    loop {
        if let Some(t) = target_len {
            if buf.len() >= t {
                buf.truncate(t);
                break;
            }
        } else if framing == Framing::ChunkedTerminator && find_chunked_end(&buf).is_some() {
            break;
        }
        if buf.len() as u64 > cap {
            return Err(SendError::CapExceeded(cap));
        }
        if Instant::now() >= deadline {
            return Err(SendError::Timeout("total timeout during response body".into()));
        }
        tokio::select! {
            n = stream.read(&mut chunk) => {
                match n {
                    Ok(0) => break, // EOF 定界
                    Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    Err(e) => return Err(SendError::Read(format!("read body: {e}"))),
                }
            }
            _ = tokio::time::sleep(read_timeout) => {
                return Err(SendError::Timeout(format!("read timeout after {read_timeout:?}")));
            }
            _ = cancelled(cancel.clone()) => return Err(SendError::Cancelled),
        }
    }
    if buf.len() as u64 > cap {
        return Err(SendError::CapExceeded(cap));
    }
    Ok(buf)
}

enum Framing {
    ContentLength(usize),
    ChunkedTerminator,
    Eof,
}

impl PartialEq for Framing {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Framing::ContentLength(_), Framing::ContentLength(_))
                | (Framing::ChunkedTerminator, Framing::ChunkedTerminator)
                | (Framing::Eof, Framing::Eof)
        )
    }
}

fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
}

fn detect_framing(head: &[u8]) -> Framing {
    let head_str = String::from_utf8_lossy(head);
    let mut cl: Option<usize> = None;
    let mut chunked = false;
    for line in head_str.split("\r\n") {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        if name == "content-length" {
            cl = value.parse::<usize>().ok();
        } else if name == "transfer-encoding" && value.to_ascii_lowercase().contains("chunked") {
            chunked = true;
        }
    }
    if chunked {
        Framing::ChunkedTerminator
    } else if let Some(cl) = cl {
        Framing::ContentLength(cl)
    } else {
        Framing::Eof
    }
}

fn find_chunked_end(buf: &[u8]) -> Option<usize> {
    buf.windows(5)
        .rposition(|w| w == b"0\r\n\r\n")
        .map(|p| p + 5)
}

/// 重建下一跳 OutboundRequest（Location → origin-form target + Host 头重写；
/// 303 → GET 空体；其余方法保持——RFC 9110 §10.4 保守面，307/308 保方法，
/// 301/302 亦保方法（历史惯例重写 POST→GET 属 UA 语义，显式编辑面自理））。
/// Location 形态全谱（RFC 3986 §5）：绝对 URL / protocol-relative（`//host`
/// → host 切换，继承当前 scheme）/ absolute-path（`/path`）/ relative-path
/// （`bar`、`../up` → §5.2 merge + §5.2.4 remove_dot_segments 归一化）。
fn build_redirect_request(
    prev: &OutboundRequest,
    prev_target: &RequestTarget,
    location: &str,
) -> Result<(OutboundRequest, RequestTarget, String), SendError> {
    // Location 解析：绝对 URL / protocol-relative / 相对引用（RFC 3986 §5）。
    let new_url = if location.contains("://") {
        location.to_string()
    } else if let Some(rest) = location.strip_prefix("//") {
        // protocol-relative（network-path reference）：host 切换，继承当前
        // scheme（https→同 scheme 不降级），port/path 由 URI 解析接管。
        format!("{}://{}", prev_target.scheme, rest)
    } else {
        // 相对引用：并入当前 path（merge + dot-segment 归一化，无双斜杠）。
        let joined = join_reference(&prev_target.path, location);
        format!(
            "{}://{}:{}{}",
            prev_target.scheme, prev_target.host, prev_target.port, joined
        )
    };
    let mut target: RequestTarget = {
        let uri: Uri = new_url
            .parse()
            .map_err(|e| SendError::Serialize(format!("location parse: {e}")))?;
        let scheme = uri.scheme_str().unwrap_or(&prev_target.scheme).to_string();
        let default_port = if scheme == "https" { 443 } else { 80 };
        let host = uri
            .host()
            .ok_or_else(|| SendError::Serialize("location has no host".into()))?
            .trim_matches(['[', ']'])
            .to_string();
        let port = uri.port_u16().unwrap_or(default_port);
        let path = if uri.path().is_empty() {
            "/".to_string()
        } else {
            format!("{}{}", uri.path(), uri.query().map(|q| format!("?{q}")).unwrap_or_default())
        };
        RequestTarget { scheme, host, port, path }
    };
    // 头表重建：Host 换新 authority，去 Content-Length（serialize 重定界），
    // 其余编辑头序保持。
    let authority = format!("{}:{}", target.host, target.port);
    let mut headers: Vec<wsw_message::types::HeaderRecord> = Vec::new();
    for h in &prev.headers {
        let name_l = h.name.to_ascii_lowercase();
        if name_l == "host" || name_l == "content-length" {
            continue;
        }
        headers.push(h.clone());
    }
    headers.insert(
        0,
        wsw_message::types::HeaderRecord::new("Host", authority.clone()),
    );
    let origin_form = target.path.clone();
    let next = OutboundRequest {
        method: prev.method.clone(),
        target: origin_form.clone(),
        version: prev.version,
        headers,
        body: prev.body.clone(),
        trailers: Vec::new(),
        framing: OutboundFraming::ContentLength,
    };
    // absolute-form URL（redirect_chain url 列）。
    let url = format!("{}://{}:{}{}", target.scheme, target.host, target.port, target.path);
    if next.target.contains("://") {
        // absolute-form 目标走 URI authority。
        target = resolve_target(&next)?;
    }
    Ok((next, target, url))
}

/// RFC 3986 §5.2.3 merge + §5.2.4 remove_dot_segments：relative-path 引用
/// 并入 base path 目录段（目录段含末尾 '/'——join 点不产双斜杠），`.`/`..`
/// 段逐段归一化（尾斜杠语义保留）；absolute-path 引用直取后归一化；仅
/// query 的引用（"?q"）path 取 base 原样。base_path 为 origin-form path
/// （可能带 query——merge 只取 path 段）；query 只取 reference 自身。
fn join_reference(base_path: &str, reference: &str) -> String {
    let (ref_path, ref_query) = match reference.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (reference, None),
    };
    let (base_only, _) = base_path.split_once('?').unwrap_or((base_path, ""));
    let merged: String = if ref_path.starts_with('/') {
        ref_path.to_string()
    } else if ref_path.is_empty() {
        base_only.to_string()
    } else {
        let dir = match base_only.rfind('/') {
            // 目录段含末尾 '/'（"/a/b" → "/a/"；根 "/" → "/"）。
            Some(pos) => &base_only[..=pos],
            None => "/",
        };
        format!("{dir}{ref_path}")
    };
    remove_dot_segments(&merged, ref_query)
}

/// RFC 3986 §5.2.4 remove_dot_segments（两缓冲原算法）：`.`/`..` 段剥离 +
/// 末段回退；`..` 在根之上不越出根（`/../x` → `/x`）；尾斜杠保留；
/// query 透传不参与归一化。
fn remove_dot_segments(path: &str, query: Option<&str>) -> String {
    let pop_last = |out: &mut String| {
        if let Some(pos) = out.rfind('/') {
            out.truncate(pos);
        }
    };
    let mut out = String::new();
    let mut input = path.to_string();
    while !input.is_empty() {
        if let Some(rest) = input.strip_prefix("../").or_else(|| input.strip_prefix("./")) {
            input = rest.to_string();
        } else if let Some(rest) = input.strip_prefix("/./") {
            input = format!("/{rest}");
        } else if input == "/." {
            input = "/".to_string();
        } else if let Some(rest) = input.strip_prefix("/../") {
            input = format!("/{rest}");
            pop_last(&mut out);
        } else if input == "/.." {
            input = "/".to_string();
            pop_last(&mut out);
        } else if input == "." || input == ".." {
            input = String::new();
        } else {
            // 移出一段（含前导 '/'，至下一 '/' 止——不含下一 '/'）。
            let end = match input[1..].find('/') {
                Some(pos) => 1 + pos,
                None => input.len(),
            };
            out.push_str(&input[..end]);
            input = input[end..].to_string();
        }
    }
    if let Some(q) = query {
        out.push('?');
        out.push_str(q);
    }
    out
}

/// 发送主入口（W3-D4 全路径）：Outbound → serialize → h1 字节写 → parse_wire；
/// 重定向逐跳 scope 闸 + IP 审计 + 帽；超时/取消；outbound bytes 原样返回
/// （CAS 写面在调用方）。
pub async fn send_request(
    rules: &ScopeRules,
    request: &OutboundRequest,
    policy: &SendPolicy,
    extra_ca: &[PathBuf],
    cancel: &watch::Receiver<bool>,
) -> Result<SendOutcome, SendError> {
    let started = Instant::now();
    let deadline = started + Duration::from_millis(policy.total_timeout_ms);
    let tls = tls_connector(extra_ca)?;
    let mut current = request.clone();
    let mut redirect_chain: Vec<RedirectHop> = Vec::new();
    let mut ttfb: Option<i64> = None;
    let mut last_remote_ip: Option<IpAddr>;
    let mut hop: u32 = 0;
    // 重定向帽双源收口：生效帽 = min(policy.max_redirects, rules.max_redirects)
    // ——policy（--policy JSON 用户面）只可收紧，scope rules 帽兜底不放大
    // （document_fetch.rs:757 MIN(cap) 同型；policy 帽曾为定义未消费的死配置，
    // 本行接线为唯一生效点）。
    let redirect_cap = policy.max_redirects.min(rules.max_redirects);

    loop {
        // 1) 目标解析 + scope 闸（查点 A：hop 0 = check；重定向跳 = 帽逐跳
        // check_redirect_hop 接线，W2-D5 查点 B 收口；hop 超生效帽 →
        // RedirectLimit 拒，cap 报生效帽）。
        let target = resolve_target(&current)?;
        let scope_target = ScopeTarget {
            scheme: &target.scheme,
            host: &target.host,
            port: target.port,
            path: &target.path,
        };
        let decision = if hop == 0 {
            scope::check(rules, &scope_target)
        } else if hop > redirect_cap {
            scope::ScopeDecision::Blocked {
                reason: scope::ScopeRejection::RedirectLimit { hop, cap: redirect_cap },
            }
        } else {
            scope::check_redirect_hop(rules, &scope_target, hop)
        };
        if let scope::ScopeDecision::Blocked { reason } = decision {
            return Err(SendError::Scope(reason.code().to_string()));
        }

        // 2) serialize（Outbound 唯一事实；失败即 Serialize 错）。
        let outbound = wsw_message::serialize::serialize_request(&current)
            .map_err(|e| SendError::Serialize(e.to_string()))?;

        // 3) 查点 C：DNS 解析全地址过闸（Pinned-IP 直连）。
        let ips = resolve_ips(&target.host, target.port)?;
        let _traces = scope::audit_resolved(rules, &ips).map_err(|rejections| {
            SendError::Scope(format!(
                "ip_audit:{}:{}",
                rejections[0].reason, rejections[0].ip
            ))
        })?;
        let ip = ips[0];
        last_remote_ip = Some(ip);

        // 4) 建连 + 写 + 读。
        let tls_for_scheme: Option<&TlsConnector> = if target.scheme == "https" { Some(&tls) } else { None };
        let mut stream = connect_stream(
            &target,
            ip,
            tls_for_scheme,
            Duration::from_millis(policy.connect_timeout_ms),
            cancel,
        )
        .await?;
        let write_started = Instant::now();
        stream
            .write_all(outbound.as_bytes())
            .await
            .map_err(|e| SendError::Connect(format!("write: {e}")))?;
        stream.flush().await.map_err(|e| SendError::Connect(format!("flush: {e}")))?;
        let remaining_total = if deadline > Instant::now() { deadline - Instant::now() } else { Duration::from_millis(0) };
        let read_budget = remaining_total.min(Duration::from_millis(policy.read_timeout_ms));
        let response_bytes = read_response(
            &mut stream,
            policy.max_body_bytes,
            read_budget.max(Duration::from_millis(1)),
            deadline,
            cancel,
        )
        .await?;
        if ttfb.is_none() {
            ttfb = Some(
                write_started
                    .elapsed()
                    .as_millis()
                    .max(1) as i64,
            );
        }
        drop(stream);

        // 5) 解析。
        let parsed = wsw_message::parse::parse_wire(&response_bytes);
        let status = parsed
            .message
            .as_ref()
            .and_then(|m| m.status)
            .ok_or_else(|| SendError::Read("response parse produced no status".into()))?;

        // 6) 重定向判定。
        let is_redirect = (300..400).contains(&status);
        let location = parsed.message.as_ref().and_then(|m| {
            m.headers
                .iter()
                .find(|h| h.name.eq_ignore_ascii_case("location"))
                .map(|h| h.value.clone())
        });
        if policy.redirect == RedirectPolicy::Follow && is_redirect {
            if let Some(loc) = location {
                if !loc.is_empty() {
                    let (next_req, _next_target, next_url) =
                        build_redirect_request(&current, &target, &loc)?;
                    redirect_chain.push(RedirectHop {
                        status,
                        location: loc,
                        url: next_url,
                        remote_ip: Some(ip.to_string()),
                    });
                    current = next_req;
                    hop += 1;
                    // 帽检查提前到下一跳 scope 闸（check_redirect_hop(hop)）；
                    // 此处不再重复（避免双重语义）。
                    continue;
                }
            }
        }
        // 终态：succeeded（3xx 不跟随 = 原样成功面——响应已到即事实）。
        return Ok(SendOutcome {
            state: "succeeded",
            error_code: None,
            reason: None,
            response_status: Some(status),
            response_bytes,
            parsed: Some(parsed),
            redirect_chain,
            outbound_sha256: None,
            outbound_bytes: outbound.into_inner(),
            remote_ip: last_remote_ip,
            duration_ms: started.elapsed().as_millis() as i64,
            ttfb_ms: ttfb,
        });
    }
}

/// 落库面（W3-D4.6）：outbound CAS + exchange(source=repeater) 行组装 +
/// send 行推进。send 行由调用方 pending 起步；本函数收尾终态。
/// 返回 exchange_id（send 行 exchange_id 列引用）。
#[allow(clippy::too_many_arguments)]
pub fn persist_send_result(
    store: &PersistStore,
    artifacts_root: &Path,
    send_id: &str,
    session_id: &str,
    listener_id: &str,
    tab_id: &str,
    version_no: i64,
    request: &OutboundRequest,
    outcome: &SendOutcome,
) -> Result<Option<String>, database::DbError> {
    // 1) outbound bytes CAS（逐字节重发依据，W3-D6.3）；请求体 CAS（/body
    //    按需端点读回面；非空才落——tab version JSON 已内联一份编辑态副本，
    //    CAS 面服务 History 行读回）。
    let request_ref = (!request.body.is_empty())
        .then(|| {
            wsw_message::cas::store_body(
                artifacts_root,
                &request.body,
                request.body.len() as u64,
            )
            .map_err(|e| database::DbError::Validation(format!("request CAS store: {e}")))
        })
        .transpose()?;
    let outbound_ref = wsw_message::cas::store_body(
        artifacts_root,
        &outcome.outbound_bytes,
        outcome.outbound_bytes.len() as u64,
    )
    .map_err(|e| database::DbError::Validation(format!("outbound CAS store: {e}")))?;
    tracing::info!(
        class = "capture",
        send_id = %send_id,
        sha256 = %outbound_ref.sha256,
        size = outbound_ref.size,
        uri = %outbound_ref.uri,
        "outbound bytes stored to CAS (byte-exact resend basis, W3-D6.3)"
    );

    // 1.5) 响应体 CAS（/body 按需端点读回面；非空才落）。
    let response_ref = (!outcome.response_bytes.is_empty())
        .then(|| {
            wsw_message::cas::store_body(
                artifacts_root,
                &outcome.response_bytes,
                outcome.response_bytes.len() as u64,
            )
            .map_err(|e| database::DbError::Validation(format!("response CAS store: {e}")))
        })
        .transpose()?;

    // 2) exchange 行（source=repeater；capture_fidelity=full——outbound bytes
    //    即原始 wire；响应解析面如实落 parse_status）。
    let exchange_id = crate::listener::handler::new_exchange_id();
    let seq = store.next_sequence_no(listener_id)?;
    let (req_json, res_json) = (request, outcome.parsed.as_ref().and_then(|p| p.message.as_ref()));
    let req_headers_json = serde_json::to_string(
        &req_json
            .headers
            .iter()
            .map(|h| serde_json::json!({"name": h.name, "value": h.value}))
            .collect::<Vec<_>>(),
    )
    .unwrap_or_else(|_| "[]".into());
    let res_headers_json = res_json
        .map(|m| {
            serde_json::to_string(
                &m.headers
                    .iter()
                    .map(|h| serde_json::json!({"name": h.name, "value": h.value}))
                    .collect::<Vec<_>>(),
            )
            .unwrap_or_else(|_| "[]".into())
        })
        .unwrap_or_else(|| "[]".into());
    let target = resolve_target(request).ok();
    let started_at = crate::persist::now_iso();
    let completed_at = crate::persist::now_iso();
    let search = crate::listener::handler::build_search_text(
        target.as_ref().map(|t| format!("{}:{}", t.host, t.port)).as_deref().unwrap_or(""),
        target.as_ref().map(|t| t.path.as_str()).unwrap_or(""),
        &req_json
            .headers
            .iter()
            .map(|h| (h.name.as_str(), h.value.as_str()))
            .collect::<Vec<_>>(),
        &[crate::listener::handler::body_snippet(&req_json.body)],
    );
    let response_mime = outcome.to_patch(None).response_mime;
    let parse_status = match outcome.parsed.as_ref().map(|p| p.parse_status) {
        Some(ParseStatus::Ok) => "ok",
        Some(ParseStatus::Partial) => "partial",
        _ => "failed",
    };
    let row = ExchangeRow {
        exchange_id: exchange_id.clone(),
        session_id: session_id.to_string(),
        listener_id: listener_id.to_string(),
        sequence_no: seq,
        direction: "client_to_server",
        source: "repeater",
        protocol: "http",
        http_version: Some(req_json.version.as_str().to_string()),
        tls_version: None,
        alpn: None,
        method: Some(req_json.method.clone()),
        scheme: target.as_ref().map(|t| t.scheme.clone()),
        authority: target
            .as_ref()
            .map(|t| format!("{}:{}", t.host, t.port)),
        path: target.as_ref().map(|t| t.path.clone()),
        response_status: outcome.response_status.map(|s| s as i64),
        request_headers_json: Some(req_headers_json),
        response_headers_json: Some(res_headers_json),
        request_body_sha256: (!req_json.body.is_empty()).then(|| wsw_message::cas::sha256_hex(&req_json.body)),
        request_body_uri: request_ref.as_ref().map(|r| r.uri.clone()),
        request_body_len: Some(req_json.body.len() as i64),
        request_mime: req_json
            .headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case("content-type"))
            .map(|h| h.value.clone()),
        request_body_fidelity: Some("full"),
        response_body_sha256: (!outcome.response_bytes.is_empty())
            .then(|| wsw_message::cas::sha256_hex(&outcome.response_bytes)),
        response_body_uri: response_ref.as_ref().map(|r| r.uri.clone()),
        response_body_len: Some(outcome.response_bytes.len() as i64),
        response_mime,
        response_body_fidelity: Some("full"),
        started_at: Some(started_at.clone()),
        completed_at: Some(completed_at),
        duration_ms: Some(outcome.duration_ms),
        ttfb_ms: outcome.ttfb_ms,
        remote_ip: outcome.remote_ip.map(|ip| ip.to_string()),
        scope_decision: Some("in_scope"),
        edited: false,
        tags_json: "[]".to_string(),
        notes: None,
        capture_fidelity: "full",
        parse_status,
        error_code: outcome.error_code.clone(),
        search_text: Some(search),
        ..Default::default()
    };
    let _ = &res_json;
    store.insert_exchanges(std::slice::from_ref(&row))?;

    // 3) send 行推进终态（pending/sending → succeeded/failed/…）；outbound
    //    CAS sha 回填（逐字节重发唯一依据列，W3-D6.3）。
    let mut patch = outcome.to_patch(Some(exchange_id.clone()));
    patch.request_outbound_sha256 = Some(outbound_ref.sha256);
    store.advance_send(send_id, &patch)?;
    let _ = (tab_id, version_no);
    Ok(Some(exchange_id))
}

/// 发送失败面落库（state=failed/timed_out/cancelled；send 行推进 + 不产
/// exchange 行——无响应字节即无发送事实行，redirect_chain/error_code 落 send 行）。
pub fn persist_send_failure(
    store: &PersistStore,
    send_id: &str,
    outcome: &SendOutcome,
) -> Result<bool, database::DbError> {
    store.advance_send(send_id, &outcome.to_patch(None))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(target: &str, host: &str) -> OutboundRequest {
        OutboundRequest {
            method: "GET".to_string(),
            target: target.to_string(),
            version: wsw_message::types::HttpVersion::Http11,
            headers: vec![wsw_message::types::HeaderRecord::new("Host", host)],
            body: Vec::new(),
            trailers: Vec::new(),
            framing: OutboundFraming::ContentLength,
        }
    }

    #[test]
    fn resolve_target_absolute_and_origin_form() {
        let t = resolve_target(&req("http://127.0.0.1:8443/p59c-marker?a=1", "ignored")).unwrap();
        assert_eq!(t.scheme, "http");
        assert_eq!(t.host, "127.0.0.1");
        assert_eq!(t.port, 8443);
        assert_eq!(t.path, "/p59c-marker?a=1");
        let t = resolve_target(&req("/index", "local-range.test:8123")).unwrap();
        assert_eq!(t.host, "local-range.test");
        assert_eq!(t.port, 8123);
        assert_eq!(t.path, "/index");
        // 无 Host 头 origin-form → fail-closed。
        let no_host = OutboundRequest {
            method: "GET".into(),
            target: "/x".into(),
            version: wsw_message::types::HttpVersion::Http11,
            headers: vec![],
            body: vec![],
            trailers: vec![],
            framing: OutboundFraming::ContentLength,
        };
        assert!(resolve_target(&no_host).is_err());
    }

    #[test]
    fn redirect_request_rebuild_keeps_header_order_and_rewrites_host() {
        let prev = OutboundRequest {
            method: "GET".into(),
            target: "/a".into(),
            version: wsw_message::types::HttpVersion::Http11,
            headers: vec![
                wsw_message::types::HeaderRecord::new("Host", "127.0.0.1:8001"),
                wsw_message::types::HeaderRecord::new("X-Trace", "t1"),
                wsw_message::types::HeaderRecord::new("Authorization", "secret"),
            ],
            body: vec![],
            trailers: vec![],
            framing: OutboundFraming::ContentLength,
        };
        let prev_target = RequestTarget {
            scheme: "http".into(),
            host: "127.0.0.1".into(),
            port: 8001,
            path: "/a".into(),
        };
        let (next, tgt, url) =
            build_redirect_request(&prev, &prev_target, "http://127.0.0.1:8002/b").unwrap();
        assert_eq!(tgt.host, "127.0.0.1");
        assert_eq!(tgt.port, 8002);
        assert_eq!(tgt.path, "/b");
        assert_eq!(url, "http://127.0.0.1:8002/b");
        assert_eq!(next.headers[0].name, "Host");
        assert_eq!(next.headers[0].value, "127.0.0.1:8002");
        assert_eq!(next.headers[1].name, "X-Trace", "编辑头序保持");
        assert_eq!(next.headers[2].value, "secret");
        assert!(next
            .headers
            .iter()
            .all(|h| !h.name.eq_ignore_ascii_case("content-length")));
        // 相对 Location。
        let (_, tgt2, _) = build_redirect_request(&prev, &prev_target, "/rel-path").unwrap();
        assert_eq!(tgt2.path, "/rel-path");
    }

    #[test]
    fn framing_detection_shapes() {
        let head = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\n";
        assert!(matches!(detect_framing(head), Framing::ContentLength(5)));
        let head = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
        assert!(matches!(detect_framing(head), Framing::ChunkedTerminator));
        let head = b"HTTP/1.1 200 OK\r\n\r\n";
        assert!(matches!(detect_framing(head), Framing::Eof));
    }

    #[test]
    fn search_text_credential_headers_excluded() {
        let pairs = vec![
            ("Host", "127.0.0.1"),
            ("Authorization", "Bearer super-secret-token"),
            ("Cookie", "session=abc123"),
        ];
        let text = crate::listener::handler::build_search_text(
            "127.0.0.1:80",
            "/",
            &pairs,
            &[Some("body-text".into())],
        );
        assert!(text.contains("Host 127.0.0.1"));
        assert!(text.contains("Authorization"), "可搜头名");
        assert!(!text.contains("super-secret-token"), "凭据值不进 search_text");
        assert!(text.contains("Cookie"));
        assert!(!text.contains("session=abc123"));
        assert!(text.contains("body-text"));
    }

    #[tokio::test]
    async fn scope_default_deny_rejects_before_connect() {
        let rules = ScopeRules::default(); // include 空 = default deny
        let req = req("http://127.0.0.1:1/x", "127.0.0.1:1");
        let (_tx, rx) = watch::channel(false);
        let err = send_request(&rules, &req, &SendPolicy::default(), &[], &rx)
            .await
            .unwrap_err();
        assert!(matches!(err, SendError::Scope(ref code) if code == "no_rules_default_deny"));
    }
}
