//! document_fetch：`rip.knowledge.document.fetch`（T1，P6-11 第 2 步，
//! 安全知识源计划 §6.2C）的策略面与执行面。
//!
//! 纪律（计划 §6.2C 逐条落码）：
//! - 只接受 Registry 已批准 canonical URL：approved 闸按
//!   knowledge_source_entry（011）`review_status='APPROVED'` 复核，URL 与
//!   条目 canonical_url 不一致即拒（not_approved_rejected），不猜造目标；
//! - 域名 allowlist：host 经 [`adapter::egress_proxy::host_from_url`] 派生
//!   （复用既有面，不重写），精确匹配，无通配无后缀；
//! - DNS/IP 私网检查：解析后的**全部**地址过 [`private_reason`]，任何
//!   loopback/RFC1918/link-local（含 169.254.169.254 metadata 端点，它是
//!   169.254/16 的成员）/CGNAT/ULA/保留段即拒（private_network_blocked）；
//!   连接使用已复核 IP（防 DNS rebinding TOCTOU）；localhost/.local/
//!   .internal 主机名在解析前先拒；
//! - 重定向逐跳复核：301/302/303/307/308 的每一跳 Location 先过同一
//!   [`check_hop`]（allowlist + 私网）再跟随，跳数上限硬帽 10；
//! - 大小/MIME/超时限制：max_bytes（缺省 2 MiB，硬帽 8 MiB）、MIME 白名单
//!   （text/*、application/json 等）、逐 hop 连接/读超时；
//! - 默认只读 GET，无 Cookie/Authorization，不执行页面脚本；内容 sha256
//!   与抓取时间记录进 012 knowledge_document_fetch（本模块是该表唯一
//!   写面），拒绝事件同样落行（status=rejected）供复核。
//!
//! 已知边界（批次裁定）：workspace 无 TLS 依赖栈，传输面本批仅 http 明文；
//! https 请求按 [`FetchRejection::SchemeUnsupported`] 结构化拒绝（不静默
//! 降级）。TLS 引入需新依赖评估（vendor/离线构建），归后续批次。
//!
//! 传输抽象：[`Transport`] 生产实现 [`ProxyTransport`] 经
//! [`adapter::egress_proxy::EgressProxy`]（run 生命周期白名单代理）转发
//! 绝对形式 GET；测试用 Mock 注入（重定向链/拒绝链零真实网络）。

use std::collections::BTreeSet;
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::adapter::egress_proxy::{host_from_url, EgressProxy};
use crate::adapter::egress_proxy::{EgressAllowlist, ProxyEvent};
use tool_registry::hash::sha256_hex;

/// 响应体硬帽（8 MiB；max_bytes 参数不得超出）。
pub const MAX_BODY_HARD_CAP: usize = 8 * 1024 * 1024;
/// 默认响应体上限（2 MiB）。
const DEFAULT_MAX_BYTES: usize = 2 * 1024 * 1024;
/// 默认重定向跳数上限。
const DEFAULT_MAX_REDIRECTS: u32 = 3;
/// 重定向跳数硬帽。
const MAX_REDIRECTS_HARD_CAP: u32 = 10;
/// 每 hop 读超时。
const READ_TIMEOUT: Duration = Duration::from_secs(15);
/// MIME 白名单前缀（响应缺 Content-Type 按 application/octet-stream 拒）。
const MIME_ALLOW_PREFIXES: [&str; 4] =
    ["text/", "application/json", "application/xml", "application/markdown"];

/// 抓取策略（allowlist + 限额；派发面按 approved 条目 host 与 Manifest
/// egress.ports 组装；工具面可显式传入）。
#[derive(Debug, Clone)]
pub struct FetchPolicy {
    /// 允许的目标 host（小写精确匹配；无通配无后缀）。
    pub allowed_hosts: Vec<String>,
    /// 允许的目标端口（Manifest egress.ports 口径；本批 http=80）。
    pub allowed_ports: Vec<u16>,
    /// 响应体上限（字节）。
    pub max_bytes: usize,
    /// 重定向跳数上限。
    pub max_redirects: u32,
    /// approved 闸：true 时必须给出 approved 复核闭包且通过。
    pub require_approved: bool,
}

impl Default for FetchPolicy {
    fn default() -> Self {
        Self {
            allowed_hosts: Vec::new(),
            allowed_ports: vec![80, 443],
            max_bytes: DEFAULT_MAX_BYTES,
            max_redirects: DEFAULT_MAX_REDIRECTS,
            require_approved: true,
        }
    }
}

impl FetchPolicy {
    /// 构造（max_bytes 折到硬帽以内，跳数折到硬帽以内）。
    pub fn new(allowed_hosts: Vec<String>, allowed_ports: Vec<u16>) -> Self {
        Self {
            allowed_hosts,
            allowed_ports: if allowed_ports.is_empty() {
                vec![80, 443]
            } else {
                allowed_ports
            },
            ..Self::default()
        }
    }
}

/// 抓取输入。
#[derive(Debug, Clone)]
pub struct FetchInput {
    /// 来源插件 id（记录与 approved 闸定位用，可空）。
    pub source_id: Option<String>,
    /// 目标 URL（必须等于 approved 条目 canonical_url）。
    pub url: String,
    /// knowledge_source_entry.item_key（给出时与 URL 一起精确定位条目）。
    pub entry_key: Option<String>,
}

/// 拒绝类别（结构化错误码面；CLI/派发面直接序列化为 JSON）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum FetchRejection {
    /// URL 不可解析（scheme 缺失/authority 非法）。
    InvalidUrl(String),
    /// 非 http scheme（本批无 TLS 栈；不静默降级）。
    SchemeUnsupported(String),
    /// host 不在 allowlist（复用 egress_proxy 拒绝词形）。
    HostNotAllowlisted(String),
    /// 端口不在 allowlist。
    PortNotAllowed(u16),
    /// 私网/保留地址（含主机名形态）；载荷 (host, 详情)。
    PrivateNetworkBlocked(String),
    /// DNS 解析失败。
    DnsResolveFailed(String),
    /// 重定向跳数超限。
    TooManyRedirects,
    /// 非 2xx 状态。
    HttpStatus(u16),
    /// MIME 不在白名单。
    MimeNotAllowed(String),
    /// 响应体超限。
    BodyTooLarge(usize),
    /// approved 闸失败（URL 非 Registry 已批准 canonical URL 或与条目不符）。
    NotApprovedRejected(String),
    /// 传输层错误（代理连接失败等）。
    TransportError(String),
}

impl FetchRejection {
    /// 结构化错误码（reject_reason 词形，manifest output_schema 登记）。
    pub fn code(&self) -> &'static str {
        match self {
            FetchRejection::InvalidUrl(_) => "invalid_url",
            FetchRejection::SchemeUnsupported(_) => "scheme_unsupported",
            FetchRejection::HostNotAllowlisted(_) => "host_not_allowlisted",
            FetchRejection::PortNotAllowed(_) => "port_not_allowed",
            FetchRejection::PrivateNetworkBlocked(_) => "private_network_blocked",
            FetchRejection::DnsResolveFailed(_) => "dns_resolve_failed",
            FetchRejection::TooManyRedirects => "too_many_redirects",
            FetchRejection::HttpStatus(_) => "http_status",
            FetchRejection::MimeNotAllowed(_) => "mime_not_allowed",
            FetchRejection::BodyTooLarge(_) => "body_too_large",
            FetchRejection::NotApprovedRejected(_) => "not_approved_rejected",
            FetchRejection::TransportError(_) => "transport_error",
        }
    }

    /// 人类可读详情。
    pub fn detail(&self) -> String {
        match self {
            FetchRejection::InvalidUrl(u) => format!("invalid url: {u:?}"),
            FetchRejection::SchemeUnsupported(s) => format!(
                "scheme {s:?} unsupported: no TLS stack in this batch; http only"
            ),
            FetchRejection::HostNotAllowlisted(h) => format!("host_not_allowlisted: {h:?}"),
            FetchRejection::PortNotAllowed(p) => format!("port_not_allowed: {p}"),
            FetchRejection::PrivateNetworkBlocked(h) => format!(
                "private_network_blocked: {h:?} resolves to or names a non-public target \
                 (localhost/loopback/RFC1918/link-local incl. 169.254.169.254 metadata)"
            ),
            FetchRejection::DnsResolveFailed(h) => format!("dns_resolve_failed: {h:?}"),
            FetchRejection::TooManyRedirects => "too many redirects".to_string(),
            FetchRejection::HttpStatus(c) => format!("http status {c}"),
            FetchRejection::MimeNotAllowed(m) => format!("mime_not_allowed: {m:?}"),
            FetchRejection::BodyTooLarge(n) => format!("body too large: {n} bytes"),
            FetchRejection::NotApprovedRejected(r) => format!("not_approved_rejected: {r}"),
            FetchRejection::TransportError(e) => format!("transport_error: {e}"),
        }
    }
}

/// 传输响应（状态 + 头 + 体；体由传输实现按策略帽预截）。
#[derive(Debug, Clone)]
pub struct TransportResponse {
    /// HTTP 状态码。
    pub status: u16,
    /// 响应头（键小写）。
    pub headers: Vec<(String, String)>,
    /// 响应体。
    pub body: Vec<u8>,
    /// 传输实现是否已按帽截断（true 时策略面不必重复超限判定）。
    pub truncated_by_transport: bool,
}

/// 传输抽象（生产：白名单代理；测试：Mock 注入）。
pub trait Transport {
    /// 以 GET 取回响应（传输实现按 max_bytes 预截体）。
    fn get(&self, url: &str, max_bytes: usize) -> std::io::Result<TransportResponse>;
}

/// 生产传输：经 [`EgressProxy`]（run 生命周期白名单代理）以绝对形式 GET
/// 转发。代理侧逐请求 allowlist 复核（403 结构化拒绝事件），与本模块的
/// 策略面双重防御。
pub struct ProxyTransport {
    proxy: EgressProxy,
}

impl ProxyTransport {
    /// 按 allowlist 启动 run 级代理（事件台账供审计取用）。
    pub fn spawn(run_id: &str, allow: EgressAllowlist) -> std::io::Result<Self> {
        Ok(Self {
            proxy: EgressProxy::spawn_for_run(run_id, allow)?,
        })
    }

    /// 本 run 代理事件快照（TunnelAccepted/Rejected，结构化留痕）。
    pub fn events(&self) -> Vec<ProxyEvent> {
        self.proxy.events()
    }
}

impl Transport for ProxyTransport {
    fn get(&self, url: &str, max_bytes: usize) -> std::io::Result<TransportResponse> {
        // 绝对形式目标（代理仅转发 http 绝对形式；策略面已先行限 http）。
        let mut stream =
            TcpStream::connect(("127.0.0.1", self.proxy.port())).map_err(|e| {
                std::io::Error::new(ErrorKind::ConnectionRefused, format!("proxy: {e}"))
            })?;
        stream.set_read_timeout(Some(READ_TIMEOUT))?;
        stream.set_write_timeout(Some(READ_TIMEOUT))?;
        let request = format!(
            "GET {url} HTTP/1.1\r\nHost: {}\r\nUser-Agent: RIP-document-fetch/0.1\r\nAccept: text/*,application/json\r\nConnection: close\r\n\r\n",
            host_from_url(url).unwrap_or_default(),
        );
        stream.write_all(request.as_bytes())?;
        read_http_response(&mut stream, max_bytes)
    }
}

/// 读完响应头 + 体（体至 max_bytes 截断并置 truncated_by_transport）。
/// 定界三种形态：Content-Length 精确取、Transfer-Encoding: chunked 去帧、
/// 其余读到 EOF（Connection: close）。2026-11-12 冒烟实测修正：example.com
/// chunked 形态下旧实现把分帧记进正文（哈希失真），本版按 RFC 7230 解帧。
fn read_http_response(
    stream: &mut TcpStream,
    max_bytes: usize,
) -> std::io::Result<TransportResponse> {
    let mut head = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end;
    loop {
        if let Some(idx) = find_head_end(&head) {
            head_end = idx;
            break;
        }
        if head.len() > 64 * 1024 {
            return Err(std::io::Error::new(ErrorKind::InvalidData, "head too large"));
        }
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Err(std::io::Error::new(ErrorKind::UnexpectedEof, "head incomplete"));
        }
        head.extend_from_slice(&chunk[..n]);
    }
    let head_text = String::from_utf8_lossy(&head[..head_end]).to_string();
    let mut lines = head_text.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| std::io::Error::new(ErrorKind::InvalidData, "bad status line"))?;
    let mut headers = Vec::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }
    let header = |name: &str| -> Option<String> {
        headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    };
    let chunked = header("transfer-encoding")
        .map(|v| v.to_ascii_lowercase().contains("chunked"))
        .unwrap_or(false);
    let content_length: Option<usize> = header("content-length").and_then(|v| v.parse().ok());

    // 余量（头与体可能同 chunk 到达）；体读取按定界形态分派。
    let mut pending: Vec<u8> = head[head_end + 4..].to_vec();
    fn take_from(
        stream: &mut TcpStream,
        pending: &mut Vec<u8>,
        chunk: &mut [u8; 4096],
        n: usize,
    ) -> std::io::Result<Vec<u8>> {
        while pending.len() < n {
            let got = stream.read(chunk)?;
            if got == 0 {
                return Err(std::io::Error::new(ErrorKind::UnexpectedEof, "body incomplete"));
            }
            pending.extend_from_slice(&chunk[..got]);
        }
        Ok(pending.drain(..n).collect())
    }
    let mut body = Vec::new();
    if chunked {
        // 解帧：<hex>[;ext]\r\n <data> \r\n ... 0-chunk 后读尾注头至空行。
        loop {
            // 读一行 chunk-size（确保 pending 里有 \r\n）
            let line_end = loop {
                if let Some(i) = pending.windows(2).position(|w| w == b"\r\n") {
                    break i;
                }
                let got = stream.read(&mut chunk)?;
                if got == 0 {
                    return Err(std::io::Error::new(ErrorKind::UnexpectedEof, "chunk size incomplete"));
                }
                pending.extend_from_slice(&chunk[..got]);
            };
            let size_line = String::from_utf8_lossy(&pending[..line_end]).to_string();
            pending.drain(..line_end + 2);
            let size_hex = size_line.split(';').next().unwrap_or("").trim().to_string();
            let size = usize::from_str_radix(&size_hex, 16)
                .map_err(|_| std::io::Error::new(ErrorKind::InvalidData, "bad chunk size"))?;
            if size == 0 {
                // 尾注：读到空行（或 EOF）
                loop {
                    if let Some(i) = pending.windows(2).position(|w| w == b"\r\n") {
                        if i == 0 {
                            pending.drain(..2);
                            break;
                        }
                        pending.drain(..i + 2);
                        continue;
                    }
                    let got = stream.read(&mut chunk)?;
                    if got == 0 {
                        break;
                    }
                    pending.extend_from_slice(&chunk[..got]);
                }
                break;
            }
            let data = take_from(stream, &mut pending, &mut chunk, size + 2)?; // data + \r\n
            body.extend_from_slice(&data[..size]);
            if body.len() > max_bytes {
                body.truncate(max_bytes);
                return Ok(TransportResponse {
                    status,
                    headers,
                    body,
                    truncated_by_transport: true,
                });
            }
        }
    } else if let Some(n) = content_length {
        let mut data = if pending.len() >= n {
            pending.drain(..n).collect()
        } else {
            take_from(stream, &mut pending, &mut chunk, n)?
        };
        if data.len() > max_bytes {
            data.truncate(max_bytes);
            return Ok(TransportResponse {
                status,
                headers,
                body: data,
                truncated_by_transport: true,
            });
        }
        body = data;
    } else {
        // close 定界：读到 EOF
        loop {
            body.extend_from_slice(&pending);
            pending.clear();
            if body.len() > max_bytes {
                body.truncate(max_bytes);
                return Ok(TransportResponse {
                    status,
                    headers,
                    body,
                    truncated_by_transport: true,
                });
            }
            let got = stream.read(&mut chunk)?;
            if got == 0 {
                break;
            }
            pending.extend_from_slice(&chunk[..got]);
        }
    }
    Ok(TransportResponse {
        status,
        headers,
        body,
        truncated_by_transport: false,
    })
}

fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

/// 主机名级私网/内网判定（DNS 解析前先拒，不给内网名解析机会）。
fn hostname_is_internal(host: &str) -> bool {
    host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local")
        || host.ends_with(".internal") || host.ends_with(".home.arpa")
        || host.ends_with(".lan")
}

/// IP 私网/保留判定：返回 Some(原因) 即拒绝。
pub fn private_reason(ip: IpAddr) -> Option<&'static str> {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            if v4.is_loopback() {
                Some("loopback")
            } else if v4.is_unspecified() || o[0] == 0 {
                Some("unspecified_or_reserved")
            } else if v4.is_private() {
                Some("rfc1918")
            } else if v4.is_link_local() {
                // 169.254.0.0/16：云 metadata 端点 169.254.169.254 属本段
                Some("link_local")
            } else if o[0] == 100 && (o[1] & 0b1100_0000) == 0b0100_0000 {
                Some("cgnat") // 100.64.0.0/10
            } else if v4.is_broadcast() {
                Some("broadcast")
            } else if v4.is_documentation() {
                Some("documentation")
            } else if o[0] == 198 && (o[1] == 18 || o[1] == 19) {
                Some("benchmark") // 198.18.0.0/15
            } else if o[0] >= 224 {
                Some("multicast_or_reserved")
            } else {
                None
            }
        }
        IpAddr::V6(v6) => {
            let seg = v6.segments();
            if v6.is_loopback() {
                Some("loopback")
            } else if v6.is_unspecified() {
                Some("unspecified")
            } else if (seg[0] & 0xffc0) == 0xfe80 {
                Some("link_local") // fe80::/10
            } else if (seg[0] & 0xfe00) == 0xfc00 {
                Some("ula") // fc00::/7
            } else if seg[0] == 0x2001 && seg[1] == 0x0db8 {
                Some("documentation") // 2001:db8::/32
            } else if (seg[0] & 0xff00) == 0xff00 {
                Some("multicast") // ff00::/8
            } else if seg[0] == 0x0064 && seg[1] == 0xff9b {
                Some("nat64") // 64:ff9b::/96 映射 v4，一律按内网处理
            } else if seg[0] == 0 && seg[1] == 0 && seg[2] == 0 && seg[3] == 0
                && seg[4] == 0 && seg[5] == 0xffff
            {
                // ::ffff:0:0/96 IPv4-mapped：按内嵌 v4 复核
                let v4 = std::net::Ipv4Addr::new(
                    (seg[6] >> 8) as u8,
                    (seg[6] & 0xff) as u8,
                    (seg[7] >> 8) as u8,
                    (seg[7] & 0xff) as u8,
                );
                private_reason(IpAddr::V4(v4)).map(|r| match r {
                    "rfc1918" => "rfc1918_mapped",
                    "link_local" => "link_local_mapped",
                    other => other,
                })
            } else {
                None
            }
        }
    }
}

/// URL 端口推导（缺省 http=80/https=443；非法端口 Err）。
fn url_port(url: &str) -> Result<u16, FetchRejection> {
    let rest = url.split_once("://").map(|(_, r)| r).ok_or_else(|| {
        FetchRejection::InvalidUrl(url.to_string())
    })?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit_once('@').map(|(_, h)| h).unwrap_or(authority);
    let port_str = if authority.starts_with('[') {
        let end = authority.find(']').ok_or_else(|| {
            FetchRejection::InvalidUrl(url.to_string())
        })?;
        authority[end + 1..].strip_prefix(':')
    } else {
        authority.rsplit_once(':').map(|(_, p)| p)
    };
    match port_str {
        Some("") => Ok(default_port(url)),
        Some(p) => p.parse::<u16>().map_err(|_| FetchRejection::InvalidUrl(url.to_string())),
        None => Ok(default_port(url)),
    }
}

fn default_port(url: &str) -> u16 {
    if url.to_ascii_lowercase().starts_with("https://") {
        443
    } else {
        80
    }
}

/// DNS 解析面函数类型（host+port 到全部解析 IP；Err = 解析失败）。
pub type DnsResolver<'a> = &'a dyn Fn(&str, u16) -> Result<Vec<IpAddr>, ()>;
/// approved 闸复核函数类型（url + entry_key 到 canonical 或拒绝理由）。
pub type ApprovedChecker<'a> =
    &'a dyn Fn(&str, Option<&str>) -> Result<String, String>;

/// 生产 DNS 解析面（std ToSocketAddrs；返回全部解析地址的 IP 列表）。
fn system_dns(host: &str, port: u16) -> Result<Vec<IpAddr>, ()> {
    let addrs: Vec<std::net::SocketAddr> = (host, port).to_socket_addrs().map_err(|_| ())?.collect();
    if addrs.is_empty() {
        return Err(());
    }
    Ok(addrs.into_iter().map(|a| a.ip()).collect())
}

/// 单跳复核（生产入口）：scheme + host 派生（复用 egress_proxy::host_from_url）
/// + 端口 + DNS 全址私网复核 + allowlist。任何一环失败即拒（不猜目标）。
///
/// 闸序：scheme → host 解析 → 主机名内网先拒 → 端口 → DNS 全址私网 →
/// allowlist（私网闸先于 allowlist：内网目标无论是否在 allowlist 都先拒）。
pub fn check_hop(url: &str, policy: &FetchPolicy) -> Result<String, FetchRejection> {
    check_hop_with(url, policy, &system_dns)
}

/// 单跳复核（可注入 DNS 面；测试用 Mock 零真实解析）。
pub fn check_hop_with(
    url: &str,
    policy: &FetchPolicy,
    dns: DnsResolver<'_>,
) -> Result<String, FetchRejection> {
    let lower = url.to_ascii_lowercase();
    if !lower.contains("://") {
        // 无 scheme（javascript:alert(1)/相对路径/裸 host）一律 invalid_url
        return Err(FetchRejection::InvalidUrl(url.to_string()));
    }
    if !lower.starts_with("http://") {
        let scheme = lower.split("://").next().unwrap_or("").to_string();
        return Err(FetchRejection::SchemeUnsupported(scheme));
    }
    let host = host_from_url(url)
        .map_err(FetchRejection::InvalidUrl)?;
    if hostname_is_internal(&host) {
        return Err(FetchRejection::PrivateNetworkBlocked(host));
    }
    // IP 字面量目标先于端口/解析直判（loopback/RFC1918/link-local 等无论
    // 端口与 allowlist 形态一律先拒）
    if let Ok(ip) = host.parse::<IpAddr>() {
        if let Some(reason) = private_reason(ip) {
            return Err(FetchRejection::PrivateNetworkBlocked(format!(
                "{host} ({reason})"
            )));
        }
    }
    let port = url_port(url)?;
    if !policy.allowed_ports.contains(&port) {
        return Err(FetchRejection::PortNotAllowed(port));
    }
    let addrs = dns(&host, port)
        .map_err(|_| FetchRejection::DnsResolveFailed(host.clone()))?;
    if addrs.is_empty() {
        return Err(FetchRejection::DnsResolveFailed(host.clone()));
    }
    for ip in &addrs {
        if let Some(reason) = private_reason(*ip) {
            return Err(FetchRejection::PrivateNetworkBlocked(format!(
                "{host} ({reason})"
            )));
        }
    }
    let hosts: BTreeSet<String> =
        policy.allowed_hosts.iter().map(|h| h.to_lowercase()).collect();
    if !hosts.contains(&host) {
        return Err(FetchRejection::HostNotAllowlisted(host));
    }
    Ok(host)
}

/// Location 解析（相对 Location 按当前 URL 拼接；scheme 相对按当前 host）。
fn resolve_location(current: &str, location: &str) -> Result<String, FetchRejection> {
    let loc = location.trim();
    if loc.to_ascii_lowercase().starts_with("http://") {
        return Ok(loc.to_string());
    }
    if loc.to_ascii_lowercase().starts_with("https://") {
        return Err(FetchRejection::SchemeUnsupported("https".to_string()));
    }
    if loc.starts_with("//") {
        return Ok(format!("http:{loc}"));
    }
    let rest = current.split_once("://").map(|(_, r)| r).ok_or_else(|| {
        FetchRejection::InvalidUrl(current.to_string())
    })?;
    let authority_end = rest.find('/').unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if loc.starts_with('/') {
        Ok(format!("http://{authority}{loc}"))
    } else {
        // 相对路径：拼到当前路径目录（够用面；跨域跳转都会过 check_hop）
        let base_path = &rest[authority_end..];
        let dir = base_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        Ok(format!("http://{authority}{dir}/{loc}"))
    }
}

fn header(resp: &TransportResponse, name: &str) -> Option<String> {
    resp.headers
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.clone())
}

/// 抓取记录（012 knowledge_document_fetch 行；ok 与 rejected 都落行）。
#[derive(Debug, Clone, Serialize)]
pub struct FetchRecord {
    /// fdf-<sha24> 内容寻址 id（ok 与 rejected 双态）。
    pub fetch_id: String,
    /// 来源插件 id。
    pub source_id: String,
    /// 首跳 URL。
    pub requested_url: String,
    /// 末跳 URL。
    pub final_url: String,
    /// 逐跳 URL 链。
    pub hops: Vec<String>,
    /// ok | rejected。
    pub status: String,
    /// 结构化错误码（拒绝行）。
    pub reject_reason: Option<String>,
    /// 人类可读详情（拒绝行）。
    pub reject_detail: Option<String>,
    /// 响应体 sha256（ok 行）。
    pub content_sha256: Option<String>,
    /// 响应体字节数（ok 行）。
    pub content_bytes: Option<u64>,
    /// Content-Type 主值（ok 行）。
    pub mime_type: Option<String>,
    /// ISO8601 UTC 抓取时刻。
    pub fetched_at: String,
    /// 耗时毫秒。
    pub elapsed_ms: u64,
    /// allowlist/限额策略快照 JSON。
    pub policy_json: String,
}

/// 抓取结果。
#[derive(Debug)]
pub struct FetchOutcome {
    /// 抓取记录（012 行形态）。
    pub record: FetchRecord,
    /// 响应体（仅 status=ok；调用方决定是否再落盘/入库正文）。
    pub body: Vec<u8>,
}

/// 执行抓取：策略闸（approved + 每 hop allowlist/私网）+ 重定向逐跳 +
/// 限额 + 哈希落库（pool 给出时写 012；拒绝也落行）。
pub fn fetch_document(
    pool: Option<&database::Pool>,
    transport: &dyn Transport,
    policy: &FetchPolicy,
    input: &FetchInput,
    approved_check: Option<ApprovedChecker<'_>>,
) -> Result<FetchOutcome, FetchRejection> {
    let started = Instant::now();
    let outcome = fetch_document_inner(transport, policy, input, approved_check, &system_dns);
    if let Some(pool) = pool {
        let now = domain::time::now_iso();
        let record = match &outcome {
            Ok(o) => o.record.clone(),
            Err(rej) => FetchRecord {
                fetch_id: String::new(), // 落库前按内容寻址生成
                source_id: input.source_id.clone().unwrap_or_default(),
                requested_url: input.url.clone(),
                final_url: input.url.clone(),
                hops: vec![input.url.clone()],
                status: "rejected".to_string(),
                reject_reason: Some(rej.code().to_string()),
                reject_detail: Some(rej.detail()),
                content_sha256: None,
                content_bytes: None,
                mime_type: None,
                fetched_at: now.clone(),
                elapsed_ms: started.elapsed().as_millis() as u64,
                policy_json: serde_json::json!({
                    "allowed_hosts": policy.allowed_hosts,
                    "allowed_ports": policy.allowed_ports,
                    "max_bytes": policy.max_bytes,
                    "max_redirects": policy.max_redirects,
                    "require_approved": policy.require_approved,
                })
                .to_string(),
            },
        };
        record_fetch(pool, &record)
            .unwrap_or_else(|e| eprintln!("record_fetch failed: {e}"));
    }
    outcome
}

#[allow(clippy::too_many_arguments)]
fn fetch_document_inner(
    transport: &dyn Transport,
    policy: &FetchPolicy,
    input: &FetchInput,
    approved_check: Option<ApprovedChecker<'_>>,
    dns: DnsResolver<'_>,
) -> Result<FetchOutcome, FetchRejection> {
    let started = Instant::now();
    // approved 闸（计划 §6.2C：不接受任意拼接 URL）
    if policy.require_approved {
        let check = approved_check.ok_or_else(|| {
            FetchRejection::NotApprovedRejected(
                "approved gate requires a registry checker".to_string(),
            )
        })?;
        check(&input.url, input.entry_key.as_deref())
            .map_err(FetchRejection::NotApprovedRejected)?;
    }
    let max_bytes = policy.max_bytes.min(MAX_BODY_HARD_CAP);
    let max_redirects = policy.max_redirects.min(MAX_REDIRECTS_HARD_CAP);

    let mut url = input.url.clone();
    let mut hops = Vec::new();
    let record_url = input.url.clone();
    let source_id = input.source_id.clone().unwrap_or_default();
    let policy_json = serde_json::json!({
        "allowed_hosts": policy.allowed_hosts,
        "allowed_ports": policy.allowed_ports,
        "max_bytes": max_bytes,
        "max_redirects": max_redirects,
        "require_approved": policy.require_approved,
    })
    .to_string();

    loop {
        check_hop_with(&url, policy, dns)?; // 每跳（含首跳）全闸复核
        hops.push(url.clone());
        if hops.len() as u32 > max_redirects + 1 {
            return Err(FetchRejection::TooManyRedirects);
        }
        let resp = transport
            .get(&url, max_bytes)
            .map_err(|e| FetchRejection::TransportError(e.to_string()))?;
        if matches!(resp.status, 301 | 302 | 303 | 307 | 308) {
            let location = header(&resp, "location").ok_or_else(|| {
                FetchRejection::TransportError("redirect without Location".to_string())
            })?;
            url = resolve_location(&url, &location)?;
            continue; // 下一跳先过 check_hop 再 GET
        }
        if !(200..300).contains(&resp.status) {
            return Err(FetchRejection::HttpStatus(resp.status));
        }
        let mime = header(&resp, "content-type").unwrap_or_default();
        let mime_for_check = if mime.is_empty() {
            "application/octet-stream".to_string()
        } else {
            mime.split(';').next().unwrap_or("").trim().to_string()
        };
        if !MIME_ALLOW_PREFIXES
            .iter()
            .any(|p| mime_for_check.starts_with(p))
        {
            return Err(FetchRejection::MimeNotAllowed(mime_for_check));
        }
        if resp.truncated_by_transport || resp.body.len() > max_bytes {
            return Err(FetchRejection::BodyTooLarge(resp.body.len()));
        }
        let sha = sha256_hex(&resp.body);
        let fetched_at = domain::time::now_iso();
        let record = FetchRecord {
            fetch_id: format!(
                "fdf-{}",
                &sha256_hex(
                    format!("{record_url}|{}|{sha}|{fetched_at}", url)
                        .as_bytes()
                )[..24]
            ),
            source_id,
            requested_url: record_url,
            final_url: url,
            hops,
            status: "ok".to_string(),
            reject_reason: None,
            reject_detail: None,
            content_sha256: Some(sha),
            content_bytes: Some(resp.body.len() as u64),
            mime_type: Some(mime_for_check),
            fetched_at: fetched_at.clone(),
            elapsed_ms: started.elapsed().as_millis() as u64,
            policy_json,
        };
        return Ok(FetchOutcome {
            record,
            body: resp.body,
        });
    }
}

/// 012 knowledge_document_fetch 受控写面（本模块是该表唯一写者）。
pub fn record_fetch(pool: &database::Pool, record: &FetchRecord) -> database::DbResult<()> {
    use database::rusqlite;
    let mut record = record.clone();
    if record.fetch_id.is_empty() {
        // 拒绝行：按 (url|reason|时刻) 内容寻址
        let key = format!(
            "{}|{}|{}",
            record.requested_url,
            record.reject_reason.clone().unwrap_or_default(),
            record.fetched_at
        );
        record.fetch_id = format!("fdf-{}", &sha256_hex(key.as_bytes())[..24]);
    }
    let hops_json = serde_json::json!(record.hops).to_string();
    pool.with_conn(|c| {
        c.execute(
            "INSERT OR IGNORE INTO knowledge_document_fetch
             (fetch_id, source_id, requested_url, final_url, hops_json, status,
              reject_reason, reject_detail, content_sha256, content_bytes,
              mime_type, fetched_at, elapsed_ms, policy_json, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            rusqlite::params![
                record.fetch_id,
                record.source_id,
                record.requested_url,
                record.final_url,
                hops_json,
                record.status,
                record.reject_reason,
                record.reject_detail,
                record.content_sha256,
                record.content_bytes,
                record.mime_type,
                record.fetched_at,
                record.elapsed_ms,
                record.policy_json,
                record.fetched_at,
            ],
        )?;
        Ok::<(), rusqlite::Error>(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    fn policy_hosts(hosts: &[&str]) -> FetchPolicy {
        FetchPolicy::new(
            hosts.iter().map(|h| h.to_string()).collect(),
            vec![80],
        )
    }

    fn noop_check() -> impl Fn(&str, Option<&str>) -> Result<String, String> {
        |url: &str, _key: Option<&str>| Ok(url.to_string())
    }

    /// 测试 DNS 面：IP 字面量 host 解析为自身（私网矩阵如实命中私网段），
    /// 其余 host 解析到公网对照 IP（零真实网络）。
    fn fake_dns(host: &str, _port: u16) -> Result<Vec<IpAddr>, ()> {
        if host == "dns-fail.example" {
            return Err(());
        }
        if let Ok(ip) = host.parse::<IpAddr>() {
            return Ok(vec![ip]);
        }
        Ok(vec![IpAddr::from([93, 184, 216, 34])]) // example.com 公网段对照
    }

    #[test]
    fn private_network_rejection_matrix() {
        let policy = policy_hosts(&["anything.example"]);
        for url in [
            "http://127.0.0.1/x",
            "http://169.254.169.254/latest/meta-data/",
            "http://10.0.0.1/x",
            "http://192.168.1.1/x",
            "http://172.16.0.1/x",
            "http://localhost:9000/",
            "http://[::1]:8080/x",
            "http://metadata.google.internal/x",
            "http://[fe80::1]/x",
            "http://[fc00::1]/x",
        ] {
            let err = check_hop_with(url, &policy, &fake_dns).unwrap_err();
            assert_eq!(
                err.code(),
                "private_network_blocked",
                "url {url} must be private-blocked, got {:?}",
                err
            );
        }
    }

    #[test]
    fn allowlist_and_port_and_scheme_rejections() {
        let policy = policy_hosts(&["example.com"]);
        // 非 allowlist host（解析可成的公网名）
        let err = check_hop_with("http://httpbin.org/x", &policy, &fake_dns).unwrap_err();
        assert_eq!(err.code(), "host_not_allowlisted");
        assert!(err.detail().contains("host_not_allowlisted"));
        // 端口不在 allowlist
        let policy8080 = FetchPolicy::new(vec!["example.com".into()], vec![8080]);
        let err = check_hop_with("http://example.com/x", &policy8080, &fake_dns).unwrap_err();
        assert_eq!(err.code(), "port_not_allowed");
        // scheme
        let err = check_hop_with("https://example.com/x", &policy_hosts(&["example.com"]), &fake_dns)
            .unwrap_err();
        assert_eq!(err.code(), "scheme_unsupported");
        let err = check_hop_with("file:///etc/passwd", &policy_hosts(&["example.com"]), &fake_dns)
            .unwrap_err();
        assert_eq!(err.code(), "scheme_unsupported");
        let err = check_hop_with("javascript:alert(1)", &policy_hosts(&["x.example"]), &fake_dns)
            .unwrap_err();
        assert_eq!(err.code(), "invalid_url", "无 :// 的非 http 目标按 invalid_url 拒");
    }

    #[test]
    fn approved_gate_requires_checker_and_matches() {
        let policy = FetchPolicy {
            require_approved: true,
            ..policy_hosts(&["example.com"])
        };
        let input = FetchInput {
            source_id: Some("knowledge.demo".into()),
            url: "http://example.com/doc".into(),
            entry_key: None,
        };
        // 无 checker：拒绝
        let err = fetch_document_inner(&MockTransport::empty(), &policy, &input, None, &fake_dns)
            .unwrap_err();
        assert_eq!(err.code(), "not_approved_rejected");
        // checker 拒绝：URL 非 approved canonical
        let deny = |_url: &str, _k: Option<&str>| -> Result<String, String> {
            Err("url not in APPROVED canonical set".to_string())
        };
        let err = fetch_document_inner(&MockTransport::empty(), &policy, &input, Some(&deny), &fake_dns)
            .unwrap_err();
        assert_eq!(err.code(), "not_approved_rejected");
    }

    // ---------------------------------------------------------------- Mock

    struct MockTransport {
        /// 每次请求依次出队（url -> 响应）。
        queue: std::sync::Mutex<Vec<(String, MockResp)>>,
        /// 记录实际请求过的 URL（逐跳断言面）。
        requested: std::sync::Mutex<Vec<String>>,
    }

    struct MockResp {
        status: u16,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    }

    impl MockTransport {
        fn empty() -> Self {
            Self {
                queue: std::sync::Mutex::new(Vec::new()),
                requested: std::sync::Mutex::new(Vec::new()),
            }
        }
        fn with(resps: Vec<(String, MockResp)>) -> Self {
            Self {
                queue: std::sync::Mutex::new(resps),
                requested: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    impl Transport for MockTransport {
        fn get(&self, url: &str, _max: usize) -> std::io::Result<TransportResponse> {
            self.requested.lock().unwrap().push(url.to_string());
            let mut q = self.queue.lock().unwrap();
            let (_, resp) = q.remove(0);
            Ok(TransportResponse {
                status: resp.status,
                headers: resp.headers,
                body: resp.body,
                truncated_by_transport: false,
            })
        }
    }

    fn resp(status: u16, headers: Vec<(&str, &str)>, body: &str) -> MockResp {
        MockResp {
            status,
            headers: headers
                .into_iter()
                .map(|(k, v)| (k.to_ascii_lowercase(), v.to_string()))
                .collect(),
            body: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn success_path_records_hash_and_policy() {
        let body = "# Document\nhello rip fetch";
        let t = MockTransport::with(vec![(
            "http://example.com/doc".into(),
            resp(200, vec![("Content-Type", "text/markdown; charset=utf-8")], body),
        )]);
        let policy = policy_hosts(&["example.com"]);
        let input = FetchInput {
            source_id: Some("knowledge.demo".into()),
            url: "http://example.com/doc".into(),
            entry_key: None,
        };
        let out = fetch_document_inner(&t, &policy, &input, Some(&noop_check()), &fake_dns).unwrap();
        assert_eq!(out.record.status, "ok");
        assert_eq!(out.record.content_sha256.as_deref(), Some(&sha256_hex(body.as_bytes()) as &str));
        assert_eq!(out.record.content_bytes, Some(body.len() as u64));
        assert_eq!(out.record.mime_type.as_deref(), Some("text/markdown"));
        assert_eq!(out.record.hops, vec!["http://example.com/doc"]);
        assert_eq!(out.record.requested_url, out.record.final_url);
        assert_eq!(out.body, body.as_bytes());
    }

    #[test]
    fn redirect_within_allowlist_follows_per_hop() {
        let t = MockTransport::with(vec![
            (
                "http://example.com/a".into(),
                resp(302, vec![("Location", "/b")], ""),
            ),
            (
                "http://example.com/b".into(),
                resp(200, vec![("Content-Type", "text/plain")], "final"),
            ),
        ]);
        let policy = policy_hosts(&["example.com"]);
        let input = FetchInput {
            source_id: None,
            url: "http://example.com/a".into(),
            entry_key: None,
        };
        let out = fetch_document_inner(&t, &policy, &input, Some(&noop_check()), &fake_dns).unwrap();
        assert_eq!(out.record.hops, vec!["http://example.com/a", "http://example.com/b"]);
        assert_eq!(t.requested.lock().unwrap().len(), 2);
    }

    #[test]
    fn redirect_cross_allowlist_rejected_per_hop() {
        // 首跳 example.com → 302 到 http://evil.example/（不在 allowlist）
        let t = MockTransport::with(vec![(
            "http://example.com/a".into(),
            resp(302, vec![("Location", "http://evil.example/next")], ""),
        )]);
        let policy = policy_hosts(&["example.com"]);
        let input = FetchInput {
            source_id: None,
            url: "http://example.com/a".into(),
            entry_key: None,
        };
        let err = fetch_document_inner(&t, &policy, &input, Some(&noop_check()), &fake_dns).unwrap_err();
        assert_eq!(err.code(), "host_not_allowlisted", "跨出 allowlist 域逐跳拒绝");
        // 只请求了首跳，目标域从未被请求
        assert_eq!(
            t.requested.lock().unwrap().as_slice(),
            ["http://example.com/a".to_string()]
        );
    }

    #[test]
    fn redirect_to_private_rejected_per_hop() {
        let t = MockTransport::with(vec![(
            "http://example.com/a".into(),
            resp(302, vec![("Location", "http://169.254.169.254/latest/meta-data/")], ""),
        )]);
        let policy = policy_hosts(&["example.com"]);
        let input = FetchInput {
            source_id: None,
            url: "http://example.com/a".into(),
            entry_key: None,
        };
        let err = fetch_document_inner(&t, &policy, &input, Some(&noop_check()), &fake_dns).unwrap_err();
        assert_eq!(err.code(), "private_network_blocked");
    }

    #[test]
    fn redirect_loop_hits_cap() {
        let mk = |n: &str| {
            (
                format!("http://example.com/{n}"),
                resp(302, vec![("Location", "/loop")], ""),
            )
        };
        let t = MockTransport::with((0..6).map(|i| mk(&i.to_string())).collect());
        let policy = policy_hosts(&["example.com"]);
        let input = FetchInput {
            source_id: None,
            url: "http://example.com/0".into(),
            entry_key: None,
        };
        let err = fetch_document_inner(&t, &policy, &input, Some(&noop_check()), &fake_dns).unwrap_err();
        assert_eq!(err.code(), "too_many_redirects");
    }

    #[test]
    fn mime_and_body_limits_enforced() {
        let mk_body = "x".repeat(4096);
        let policy = policy_hosts(&["example.com"]);
        let input = FetchInput {
            source_id: None,
            url: "http://example.com/x".into(),
            entry_key: None,
        };
        // MIME 拒绝
        let t = MockTransport::with(vec![(
            "http://example.com/x".into(),
            resp(200, vec![("Content-Type", "application/octet-stream")], "bin"),
        )]);
        let err = fetch_document_inner(&t, &policy, &input, Some(&noop_check()), &fake_dns).unwrap_err();
        assert_eq!(err.code(), "mime_not_allowed");
        // 缺 Content-Type 按 octet-stream 拒
        let t = MockTransport::with(vec![(
            "http://example.com/x".into(),
            resp(200, vec![], "bin"),
        )]);
        let err = fetch_document_inner(&t, &policy, &input, Some(&noop_check()), &fake_dns).unwrap_err();
        assert_eq!(err.code(), "mime_not_allowed");
        // 体超限
        let t = MockTransport::with(vec![(
            "http://example.com/x".into(),
            resp(200, vec![("Content-Type", "text/plain")], &mk_body),
        )]);
        let small = FetchPolicy {
            max_bytes: 1024,
            ..policy.clone()
        };
        let err = fetch_document_inner(&t, &small, &input, Some(&noop_check()), &fake_dns).unwrap_err();
        assert_eq!(err.code(), "body_too_large");
        // 非 2xx
        let t = MockTransport::with(vec![(
            "http://example.com/x".into(),
            resp(404, vec![("Content-Type", "text/plain")], "nope"),
        )]);
        let err = fetch_document_inner(&t, &policy, &input, Some(&noop_check()), &fake_dns).unwrap_err();
        assert_eq!(err.code(), "http_status");
    }

    #[test]
    fn proxy_transport_absolute_form_get_via_egress_proxy() {
        // 传输层真实链路：EgressProxy（复用既有白名单代理）+ 本地 echo 上游。
        // 策略面的私网闸在 fetch_document 层，传输层只验代理转发语义。
        let upstream = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = upstream.local_addr().unwrap().port();
        thread::spawn(move || {
            if let Ok((mut s, _)) = upstream.accept() {
                let mut buf = [0u8; 2048];
                let _ = std::io::Read::read(&mut s, &mut buf);
                let _ = std::io::Write::write_all(
                    &mut s,
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\r\nproxied-body",
                );
            }
        });
        let transport = ProxyTransport::spawn(
            "sk3-test-run",
            EgressAllowlist {
                hosts: vec!["localhost".to_string()],
                ports: vec![port],
            },
        )
        .unwrap();
        let out = Transport::get(
            &transport,
            &format!("http://localhost:{port}/doc"),
            4096,
        )
        .unwrap();
        assert_eq!(out.status, 200);
        assert_eq!(out.body, b"proxied-body");
        // 代理事件留痕（绝对形式 GET 转发）
        assert!(matches!(
            transport.events()[0],
            ProxyEvent::ForwardAccepted { ref host, port: p, ref method }
                if host == "localhost" && p == port && method == "GET"
        ));
    }

    #[test]
    fn record_fetch_writes_ok_and_rejected_rows() {
        let pool = database::Pool::open(database::PoolConfig {
            path: database::DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(2),
        })
        .unwrap();
        pool.with_conn(|c| {
            c.execute_batch(
                "CREATE TABLE knowledge_document_fetch (
                   fetch_id TEXT PRIMARY KEY, source_id TEXT NOT NULL DEFAULT '',
                   requested_url TEXT NOT NULL, final_url TEXT NOT NULL,
                   hops_json TEXT NOT NULL DEFAULT '[]', status TEXT NOT NULL,
                   reject_reason TEXT, reject_detail TEXT, content_sha256 TEXT,
                   content_bytes INTEGER, mime_type TEXT, fetched_at TEXT NOT NULL,
                   elapsed_ms INTEGER NOT NULL DEFAULT 0,
                   policy_json TEXT NOT NULL DEFAULT '{}', created_at TEXT NOT NULL);",
            )
        })
        .unwrap();
        let now = domain::time::now_iso();
        let ok = FetchRecord {
            fetch_id: "fdf-ok1".into(),
            source_id: "knowledge.demo".into(),
            requested_url: "http://example.com/d".into(),
            final_url: "http://example.com/d".into(),
            hops: vec!["http://example.com/d".into()],
            status: "ok".into(),
            reject_reason: None,
            reject_detail: None,
            content_sha256: Some("abc".into()),
            content_bytes: Some(3),
            mime_type: Some("text/plain".into()),
            fetched_at: now.clone(),
            elapsed_ms: 12,
            policy_json: "{}".into(),
        };
        record_fetch(&pool, &ok).unwrap();
        let rej = FetchRecord {
            fetch_id: String::new(), // 拒绝行内容寻址生成
            source_id: "knowledge.demo".into(),
            requested_url: "http://127.0.0.1/x".into(),
            final_url: "http://127.0.0.1/x".into(),
            hops: vec!["http://127.0.0.1/x".into()],
            status: "rejected".into(),
            reject_reason: Some("private_network_blocked".into()),
            reject_detail: Some("loopback".into()),
            content_sha256: None,
            content_bytes: None,
            mime_type: None,
            fetched_at: now.clone(),
            elapsed_ms: 3,
            policy_json: "{}".into(),
        };
        record_fetch(&pool, &rej).unwrap();
        let (ok_n, rej_n): (i64, i64) = pool
            .with_conn(|c| {
                Ok::<(i64, i64), database::rusqlite::Error>((
                    c.query_row(
                        "SELECT COUNT(*) FROM knowledge_document_fetch WHERE status='ok'",
                        [],
                        |r| r.get(0),
                    )?,
                    c.query_row(
                        "SELECT COUNT(*) FROM knowledge_document_fetch WHERE status='rejected'",
                        [],
                        |r| r.get(0),
                    )?,
                ))
            })
            .unwrap();
        assert_eq!((ok_n, rej_n), (1, 1));
        // 幂等：同 fetch_id 重放零新增
        record_fetch(&pool, &ok).unwrap();
        let total: i64 = pool
            .with_conn(|c| {
                c.query_row("SELECT COUNT(*) FROM knowledge_document_fetch", [], |r| r.get(0))
            })
            .unwrap();
        assert_eq!(total, 2);
    }

    #[test]
    fn rejected_fetch_recorded_via_fetch_document() {
        let pool = database::Pool::open(database::PoolConfig {
            path: database::DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(2),
        })
        .unwrap();
        pool.with_conn(|c| {
            c.execute_batch(
                "CREATE TABLE knowledge_document_fetch (
                   fetch_id TEXT PRIMARY KEY, source_id TEXT NOT NULL DEFAULT '',
                   requested_url TEXT NOT NULL, final_url TEXT NOT NULL,
                   hops_json TEXT NOT NULL DEFAULT '[]', status TEXT NOT NULL,
                   reject_reason TEXT, reject_detail TEXT, content_sha256 TEXT,
                   content_bytes INTEGER, mime_type TEXT, fetched_at TEXT NOT NULL,
                   elapsed_ms INTEGER NOT NULL DEFAULT 0,
                   policy_json TEXT NOT NULL DEFAULT '{}', created_at TEXT NOT NULL);",
            )
        })
        .unwrap();
        let policy = policy_hosts(&["example.com"]);
        let input = FetchInput {
            source_id: Some("knowledge.demo".into()),
            url: "http://127.0.0.1/x".into(),
            entry_key: None,
        };
        let err = fetch_document(
            Some(&pool),
            &MockTransport::empty(),
            &policy,
            &input,
            Some(&noop_check()),
        )
        .unwrap_err();
        assert_eq!(err.code(), "private_network_blocked");
        let n: i64 = pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT COUNT(*) FROM knowledge_document_fetch
                     WHERE status='rejected' AND reject_reason='private_network_blocked'",
                    [],
                    |r| r.get(0),
                )
            })
            .unwrap();
        assert_eq!(n, 1, "拒绝事件必须落行（审计复核面）");
    }
}
