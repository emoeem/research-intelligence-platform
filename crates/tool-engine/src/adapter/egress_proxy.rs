//! egress_proxy：podman_host 过渡期出站白名单代理（P3-2 批次，
//! PHASE3-DESIGN §5.3 的网络面最小实现）。
//!
//! 职责（Tool Policy Resolver 的派发面最小化，TOOL-CONTRACT §9.1 note）：
//! - run 生命周期内的本机 CONNECT 白名单代理：监听 `127.0.0.1:<ephemeral>`；
//!   容器以 `--network pasta:-T,<port>` 端口转发，容器内 `127.0.0.1:<port>`
//!   即宿主 loopback 同端口（本机 podman 6.1.4 + pasta 实测冻结；宿主接口
//!   IP 直连变体被 pasta 拒绝，Connection refused，见 tests/podman_host_egress.rs）；
//! - allowlist：目标 host 由 Adapter 从 input.url 派生（运行前冻结，精确
//!   匹配，无通配无后缀），端口限 Manifest egress.ports（P3-2 口径 80/443）；
//! - 拒绝一律回 403 并落结构化事件（[`ProxyEvent::Rejected`]），不静默换
//!   通道：工具侧表现为代理 CONNECT 失败的结构化错误帧；
//! - 仅 std::net，零新依赖；run 结束 Drop 停 accept 线程。
//!
//! 边界声明（ADR §8 已知代价的宿主面版本，tests 同文标注）：代理层策略执行
//! 只对遵守 http(s)_proxy env 的客户端（自有可信工具面）有效；pasta 默认
//! 出站未在网络层硬隔离，不受信工具与网络层硬隔离属 podman_vm 目标拓扑，
//! 解锁前 podman_host 不登记不受信工具（ADR §3.2 过渡期纪律）。

use std::collections::{BTreeMap, HashSet};
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// 请求头与首行的读取上限（代理面防御上限，非业务约束）。
const MAX_HEAD_BYTES: usize = 32 * 1024;
/// 上游连接超时。
const UPSTREAM_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// 泵空闲轮询间隔（同时是 stop 传播粒度）。
const PUMP_POLL: Duration = Duration::from_millis(250);
/// 端口探活轮询节奏（就绪/关闭等待共用；P5 验收 a 项口径 ~10 次 x 100ms）。
const PORT_PROBE_INTERVAL: Duration = Duration::from_millis(100);

/// 端口就绪探活：有界轮询 TCP 连接，连上即视为就绪（true），超时仍连不上
/// 返回 false（失败语义由调用方定，分派面按 INFRA 准备失败落账）。
///
/// 探活连接立即双向关闭：代理侧该连接落到 read_head 的 EOF/400 路径，
/// 不产生 [`ProxyEvent`]（事件台账零污染）。仅 std::net，零新依赖。
pub fn wait_port_ready(port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(stream) => {
                let _ = stream.shutdown(Shutdown::Both);
                return true;
            }
            Err(_) if Instant::now() < deadline => std::thread::sleep(PORT_PROBE_INTERVAL),
            Err(_) => return false,
        }
    }
}

/// 端口关闭等待：有界轮询 TCP 连接直到被拒（true = 端口已关闭）或超时
/// （false = 超时内端口仍可达）。替代「shutdown 后定长 sleep 即断言」的
/// 竞态写法（P5 验收 b 项）：shutdown 语义虽同步 join accept 线程，内核侧
/// listen fd 收尾与新连接被拒的呈现存在毫秒级漂移，高负载下定长窗口会撞上。
pub fn wait_port_closed(port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(stream) => {
                let _ = stream.shutdown(Shutdown::Both);
                if Instant::now() >= deadline {
                    return false;
                }
                std::thread::sleep(PORT_PROBE_INTERVAL);
            }
            Err(_) => return true,
        }
    }
}

/// run 生命周期内的出站白名单（host 精确匹配 + 端口枚举）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressAllowlist {
    /// 允许的目标 host（小写精确匹配；无通配、无后缀匹配）。
    pub hosts: Vec<String>,
    /// 允许的目标端口（Manifest egress.ports，P3-2 口径 80/443）。
    pub ports: Vec<u16>,
}

/// Manifest network.mode 的执行面分派（P3-2）。
///
/// - [`NetworkPlan::None`]：维持 P2-5 现状，容器 `--network none`；
/// - [`NetworkPlan::EgressAllowlist`]：容器走 pasta:-T 端口转发 + 代理
///   env，allowlist 为「input.url 派生 host + egress.ports」。ports 来自
///   Manifest（派发调用方从 registry 读出后传入）；host 派生在 Adapter。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum NetworkPlan {
    /// 无网络（默认建议，计划 §5.4）。
    #[default]
    None,
    /// 白名单出网（schema v1.json network.mode=egress_allowlist）。
    EgressAllowlist {
        /// Manifest egress.ports（schema 264 到 265 行：egress_allowlist 必填）。
        ports: Vec<u16>,
    },
}

/// 代理事件（本批结构化留痕面：测试断言与派发审计共用形状）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxyEvent {
    /// CONNECT 隧道建立（host/port 已过白名单）。
    TunnelAccepted {
        /// 白名单命中的目标 host。
        host: String,
        /// 白名单命中的目标端口。
        port: u16,
    },
    /// 绝对形式 HTTP 请求转发（host/port 已过白名单）。
    ForwardAccepted {
        /// 白名单命中的目标 host。
        host: String,
        /// 白名单命中的目标端口。
        port: u16,
        /// 转发的请求方法。
        method: String,
    },
    /// 白名单拒绝（不静默：403 回显 + 事件落账）。
    Rejected {
        /// 被拒目标 host（解析失败时为原始 target）。
        host: String,
        /// 被拒目标端口（解析失败时为 0）。
        port: u16,
        /// 拒绝原因（host_not_allowlisted / port_not_allowed / 解析失败说明）。
        reason: String,
    },
}

/// run 级事件台账（进程内；测试断言面，不落盘不入库）。
static RUN_EVENTS: Mutex<BTreeMap<String, Vec<ProxyEvent>>> = Mutex::new(BTreeMap::new());

/// 读取某 run 的代理事件快照（无记录返回空表）。
pub fn proxy_events(run_id: &str) -> Vec<ProxyEvent> {
    RUN_EVENTS
        .lock()
        .expect("egress events mutex")
        .get(run_id)
        .cloned()
        .unwrap_or_default()
}

/// 从 input.url 派生 allowlist host（小写、去 userinfo、去端口）。
///
/// 仅接受 http/https scheme；host 精确形（无通配符、无路径残留）；IPv6
/// 字面量剥方括号返回。解析失败返回 Err（Adapter 落 POLICY/VALIDATION
/// 结构化错误，不猜测目标）。
pub fn host_from_url(url: &str) -> Result<String, String> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| format!("url missing scheme: {url:?}"))?;
    if scheme != "http" && scheme != "https" {
        return Err(format!("scheme not allowed: {scheme:?} (http/https only)"));
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("").to_string();
    let authority = match authority.rsplit_once('@') {
        Some((_userinfo, host)) => host.to_string(),
        None => authority,
    };
    let host = if let Some(rest) = authority.strip_prefix('[') {
        // IPv6 字面量：[::1]:443 形态，剥方括号与端口；冒号合法。
        let end = rest
            .find(']')
            .ok_or_else(|| format!("unterminated ipv6 literal: {url:?}"))?;
        let host = rest[..end].to_string();
        if host.is_empty() {
            return Err(format!("url host invalid: {url:?}"));
        }
        return Ok(host.to_lowercase());
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) => {
                if port.is_empty() || port.bytes().all(|b| b.is_ascii_digit()) {
                    host.to_string()
                } else {
                    return Err(format!("url port invalid: {url:?}"));
                }
            }
            None => authority,
        }
    };
    if host.is_empty() || host.contains(['*', '/', '@', ':']) {
        return Err(format!("url host invalid: {url:?}"));
    }
    Ok(host.to_lowercase())
}

/// 白名单代理句柄：Drop 停 accept 线程（run 生命周期锚点）。
pub struct EgressProxy {
    port: u16,
    stop: Arc<AtomicBool>,
    events: Arc<Mutex<Vec<ProxyEvent>>>,
    accept_thread: Option<JoinHandle<()>>,
}

impl Drop for EgressProxy {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.accept_thread.take() {
            let _ = handle.join();
        }
    }
}

impl EgressProxy {
    /// 启动代理（监听 127.0.0.1 ephemeral 端口，accept 循环 20ms 轮询 stop）。
    pub fn spawn(allow: EgressAllowlist) -> std::io::Result<Self> {
        Self::spawn_inner(allow, None)
    }

    /// 带 run 归属启动：事件镜像进进程内 run 级台账（[`proxy_events`]）。
    pub fn spawn_for_run(run_id: &str, allow: EgressAllowlist) -> std::io::Result<Self> {
        RUN_EVENTS
            .lock()
            .expect("egress events mutex")
            .insert(run_id.to_string(), Vec::new());
        Self::spawn_inner(allow, Some(run_id.to_string()))
    }

    /// spawn 共用体：run 归属在 accept 线程启动前冻结进连接线程。
    fn spawn_inner(allow: EgressAllowlist, run_id: Option<String>) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let events: Arc<Mutex<Vec<ProxyEvent>>> = Arc::new(Mutex::new(Vec::new()));
        listener.set_nonblocking(true)?;
        let allow = Arc::new(allow);
        let stop_thread = Arc::clone(&stop);
        let events_thread = Arc::clone(&events);
        let run_id_accept = run_id.clone();
        let accept_thread = std::thread::Builder::new()
            .name("rip-egress-proxy".to_string())
            .spawn(move || {
                while !stop_thread.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _peer)) => {
                            let allow = Arc::clone(&allow);
                            let events = Arc::clone(&events_thread);
                            let stop_conn = Arc::clone(&stop_thread);
                            let run_id_conn = run_id_accept.clone();
                            let _ = std::thread::Builder::new()
                                .name("rip-egress-conn".to_string())
                                .spawn(move || {
                                    handle_conn(stream, allow, events, run_id_conn, stop_conn);
                                });
                        }
                        Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(20));
                        }
                        Err(_) => std::thread::sleep(Duration::from_millis(20)),
                    }
                }
            })?;
        Ok(Self {
            port,
            stop,
            events,
            accept_thread: Some(accept_thread),
        })
    }

    /// 代理监听端口（容器 env 与 pasta:-T 参数共用）。
    pub fn port(&self) -> u16 {
        self.port
    }

    /// 本代理已记录事件快照。
    pub fn events(&self) -> Vec<ProxyEvent> {
        self.events.lock().expect("egress events mutex").clone()
    }

    /// 显式停机（Drop 语义等价；测试断言端口关闭用）。
    pub fn shutdown(self) {
        drop(self);
    }
}

/// 事件落账：本代理本地队列 + run 级全局台账（spawn_for_run 时）。
fn push_event(events: &Arc<Mutex<Vec<ProxyEvent>>>, run_id: &Option<String>, event: ProxyEvent) {
    events
        .lock()
        .expect("egress events mutex")
        .push(event.clone());
    if let Some(run_id) = run_id {
        if let Some(list) = RUN_EVENTS
            .lock()
            .expect("egress events mutex")
            .get_mut(run_id)
        {
            list.push(event);
        }
    }
}

/// 单连接处理：CONNECT 隧道或绝对形式 HTTP 转发，白名单外一律 403。
fn handle_conn(
    stream: TcpStream,
    allow: Arc<EgressAllowlist>,
    events: Arc<Mutex<Vec<ProxyEvent>>>,
    run_id: Option<String>,
    stop: Arc<AtomicBool>,
) {
    let mut client = stream;
    let _ = client.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = client.set_write_timeout(Some(Duration::from_secs(10)));
    let head = match read_head(&mut client) {
        Ok(head) => head,
        Err(_) => {
            let _ = client.write_all(
                b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
            return;
        }
    };
    let text = String::from_utf8_lossy(&head);
    let first_line = text.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_ascii_uppercase();
    let target = parts.next().unwrap_or("").to_string();
    if method == "CONNECT" {
        match parse_authority(&target) {
            Ok((host, port)) => {
                match check_allowlist(&host, port, &allow) {
                    Ok(()) => {
                        push_event(
                            &events,
                            &run_id,
                            ProxyEvent::TunnelAccepted {
                                host: host.clone(),
                                port,
                            },
                        );
                        tunnel(client, &host, port, stop);
                    }
                    Err(reason) => {
                        // 事件先落账再回 403：客户端可见响应是「事件已可读」的
                        // 同步锚点（实测反向排序在抢占下让测试读到空台账）。
                        push_event(
                            &events,
                            &run_id,
                            ProxyEvent::Rejected { host, port, reason },
                        );
                        let _ = client.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    }
                }
            }
            Err(reason) => {
                // 同上：事件先于响应落账（响应可见即事件可读）。
                push_event(
                    &events,
                    &run_id,
                    ProxyEvent::Rejected {
                        host: target,
                        port: 0,
                        reason,
                    },
                );
                let _ = client.write_all(
                    b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
        }
    } else {
        match parse_absolute_http(&target) {
            Ok((host, port, path)) => {
                match check_allowlist(&host, port, &allow) {
                    Ok(()) => {
                        push_event(
                            &events,
                            &run_id,
                            ProxyEvent::ForwardAccepted {
                                host: host.clone(),
                                port,
                                method: method.clone(),
                            },
                        );
                        forward_http(client, &head, &method, &path, &host, port, stop);
                    }
                    Err(reason) => {
                        // 事件先落账再回 403：客户端可见响应是「事件已可读」的
                        // 同步锚点（实测反向排序在抢占下让测试读到空台账）。
                        push_event(
                            &events,
                            &run_id,
                            ProxyEvent::Rejected { host, port, reason },
                        );
                        let _ = client.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    }
                }
            }
            Err(reason) => {
                // 同上：事件先于响应落账（响应可见即事件可读）。
                push_event(
                    &events,
                    &run_id,
                    ProxyEvent::Rejected {
                        host: target,
                        port: 0,
                        reason,
                    },
                );
                let _ = client.write_all(
                    b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
        }
    }
}

/// 白名单校验：host 精确匹配 + 端口枚举；任一不符返回拒绝原因。
fn check_allowlist(host: &str, port: u16, allow: &EgressAllowlist) -> Result<(), String> {
    let hosts: HashSet<&str> = allow.hosts.iter().map(|h| h.as_str()).collect();
    let ports: HashSet<u16> = allow.ports.iter().copied().collect();
    if !hosts.contains(host) {
        return Err(format!("host_not_allowlisted: {host:?}"));
    }
    if !ports.contains(&port) {
        return Err(format!("port_not_allowed: {port}"));
    }
    Ok(())
}

/// CONNECT 目标 `host:port` 解析（host 小写化）。
fn parse_authority(target: &str) -> Result<(String, u16), String> {
    let (host, port) = target
        .rsplit_once(':')
        .ok_or_else(|| format!("CONNECT target missing port: {target:?}"))?;
    let port: u16 = port
        .parse()
        .map_err(|_| format!("CONNECT port invalid: {target:?}"))?;
    if host.is_empty() {
        return Err(format!("CONNECT host empty: {target:?}"));
    }
    Ok((host.to_lowercase(), port))
}

/// 绝对形式目标 `http://host[:port]/path` 解析（仅 http scheme）。
fn parse_absolute_http(target: &str) -> Result<(String, u16, String), String> {
    let rest = target
        .strip_prefix("http://")
        .ok_or_else(|| format!("absolute-form target must be http://: {target:?}"))?;
    let (authority, path) = match rest.find(['/', '?', '#']) {
        Some(idx) => (&rest[..idx], rest[idx..].to_string()),
        None => (rest, "/".to_string()),
    };
    let authority = match authority.rsplit_once('@') {
        Some((_userinfo, host)) => host.to_string(),
        None => authority.to_string(),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (
            host.to_string(),
            port.parse::<u16>()
                .map_err(|_| format!("port invalid: {authority:?}"))?,
        ),
        None => (authority, 80),
    };
    if host.is_empty() {
        return Err(format!("target host empty: {target:?}"));
    }
    Ok((host.to_lowercase(), port, path))
}

/// 读完请求头（至 \r\n\r\n），带 32 KiB 上限。
fn read_head(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 4096];
    loop {
        if let Some(idx) = find_head_end(&buf) {
            buf.truncate(idx + 4);
            return Ok(buf);
        }
        if buf.len() > MAX_HEAD_BYTES {
            return Err(std::io::Error::new(
                ErrorKind::InvalidData,
                "head too large",
            ));
        }
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Err(std::io::Error::new(
                ErrorKind::UnexpectedEof,
                "head incomplete",
            ));
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

/// 定位 \r\n\r\n（返回首字节下标）。
fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

/// 上游连接（resolve 后逐地址 connect_timeout，首个成功为准）。
fn connect_upstream(host: &str, port: u16) -> std::io::Result<TcpStream> {
    let addrs: Vec<_> = (host, port).to_socket_addrs()?.collect();
    let mut last_err = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, UPSTREAM_CONNECT_TIMEOUT) {
            Ok(s) => return Ok(s),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        std::io::Error::new(ErrorKind::AddrNotAvailable, "no upstream address resolved")
    }))
}

/// CONNECT 隧道：200 握手后双向泵，随 stop 或对端关闭终止。
fn tunnel(client: TcpStream, host: &str, port: u16, stop: Arc<AtomicBool>) {
    let upstream = match connect_upstream(host, port) {
        Ok(s) => s,
        Err(_e) => {
            let mut client = client;
            let _ = client.write_all(
                b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
            return;
        }
    };
    let mut client = client;
    if client
        .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
        .is_err()
    {
        return;
    }
    pump_both(client, upstream, stop);
}

/// 绝对形式 HTTP 转发：重写为源形式 + Connection: close，双向泵。
fn forward_http(
    mut client: TcpStream,
    head: &[u8],
    method: &str,
    path: &str,
    host: &str,
    port: u16,
    stop: Arc<AtomicBool>,
) {
    let text = String::from_utf8_lossy(head);
    let mut rebuilt = format!("{method} {path} HTTP/1.1\r\n");
    for line in text.lines().skip(1) {
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("proxy-connection:")
            || lower.starts_with("proxy-authorization:")
            || lower.starts_with("connection:")
            || lower.starts_with("keep-alive:")
        {
            continue;
        }
        rebuilt.push_str(line);
        rebuilt.push_str("\r\n");
    }
    rebuilt.push_str("Connection: close\r\n\r\n");
    let mut upstream = match connect_upstream(host, port) {
        Ok(s) => s,
        Err(_e) => {
            let _ = client.write_all(
                b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
            return;
        }
    };
    if upstream.write_all(rebuilt.as_bytes()).is_err() {
        return;
    }
    pump_both(client, upstream, stop);
}

/// 双向泵组装：client 到 upstream 一条线程，upstream 到 client 主线程，
/// 任一方向终止即收束两流（stop 时硬关闭）。
fn pump_both(client: TcpStream, upstream: TcpStream, stop: Arc<AtomicBool>) {
    let upstream_write = match upstream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let client_write = match client.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let upstream_kill = match upstream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let to_upstream_stop = Arc::clone(&stop);
    let client_to_upstream = std::thread::spawn(move || {
        pump(client_write, upstream_write, to_upstream_stop);
    });
    pump(upstream, client, stop);
    let _ = client_to_upstream.join();
    let _ = upstream_kill.shutdown(Shutdown::Both);
}

/// 单向泵：from 到 to，250ms 读超时轮询 stop；EOF 半关闭传播。
fn pump(mut from: TcpStream, mut to: TcpStream, stop: Arc<AtomicBool>) {
    let _ = from.set_read_timeout(Some(PUMP_POLL));
    let mut buf = [0u8; 8192];
    loop {
        if stop.load(Ordering::Relaxed) {
            let _ = to.shutdown(Shutdown::Both);
            let _ = from.shutdown(Shutdown::Both);
            return;
        }
        match from.read(&mut buf) {
            Ok(0) => {
                let _ = to.shutdown(Shutdown::Write);
                return;
            }
            Ok(n) => {
                if to.write_all(&buf[..n]).is_err() {
                    let _ = to.shutdown(Shutdown::Both);
                    let _ = from.shutdown(Shutdown::Both);
                    return;
                }
            }
            Err(ref e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(_) => {
                let _ = to.shutdown(Shutdown::Both);
                let _ = from.shutdown(Shutdown::Both);
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn host_from_url_covers_authority_forms() {
        assert_eq!(
            host_from_url("https://example.com/"),
            Ok("example.com".to_string())
        );
        assert_eq!(
            host_from_url("http://Example.COM:8080/x"),
            Ok("example.com".to_string())
        );
        assert_eq!(
            host_from_url("https://user:pass@example.com/path"),
            Ok("example.com".to_string())
        );
        assert_eq!(host_from_url("http://[::1]:8080/"), Ok("::1".to_string()));
        assert!(host_from_url("ftp://example.com/").is_err());
        assert!(host_from_url("example.com").is_err());
        assert!(host_from_url("https:///path").is_err());
    }

    #[test]
    fn proxy_tunnels_allowlisted_and_rejects_others() {
        // 本地 echo 上游：允许 host=localhost，端口=echo 监听口。
        let echo = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let echo_port = echo.local_addr().unwrap().port();
        thread::spawn(move || {
            if let Ok((mut s, _)) = echo.accept() {
                let mut buf = [0u8; 64];
                if let Ok(n) = s.read(&mut buf) {
                    let _ = s.write_all(b"echo:");
                    let _ = s.write_all(&buf[..n]);
                }
            }
        });
        let proxy = EgressProxy::spawn(EgressAllowlist {
            hosts: vec!["localhost".to_string()],
            ports: vec![echo_port],
        })
        .unwrap();

        // 允许的 CONNECT：200 + 隧道内字节往返。
        let mut c = TcpStream::connect(("127.0.0.1", proxy.port())).unwrap();
        c.write_all(format!("CONNECT localhost:{echo_port} HTTP/1.1\r\n\r\n").as_bytes())
            .unwrap();
        let mut reply = [0u8; 128];
        let n = c.read(&mut reply).unwrap();
        assert!(String::from_utf8_lossy(&reply[..n]).contains("200 Connection established"));
        c.write_all(b"ping").unwrap();
        let mut echoed = Vec::new();
        let mut chunk = [0u8; 64];
        while !echoed.ends_with(b"echo:ping") {
            let m = c.read(&mut chunk).unwrap();
            assert!(m > 0, "tunnel closed before echo completed");
            echoed.extend_from_slice(&chunk[..m]);
        }
        drop(c);
        assert_eq!(
            proxy.events(),
            vec![ProxyEvent::TunnelAccepted {
                host: "localhost".to_string(),
                port: echo_port
            }]
        );

        // 非 allowlist host：403 + 结构化拒绝事件，不静默换通道。
        let mut c = TcpStream::connect(("127.0.0.1", proxy.port())).unwrap();
        c.write_all(b"CONNECT evil.example.com:443 HTTP/1.1\r\n\r\n")
            .unwrap();
        let mut reply = [0u8; 128];
        let n = c.read(&mut reply).unwrap();
        assert!(String::from_utf8_lossy(&reply[..n]).contains("403 Forbidden"));
        assert_eq!(proxy.events().len(), 2);
        assert!(matches!(
            proxy.events()[1],
            ProxyEvent::Rejected { ref host, ref reason, .. }
                if host == "evil.example.com" && reason.contains("host_not_allowlisted")
        ));

        // 端口不在 allowlist：403 + port_not_allowed。
        let mut c = TcpStream::connect(("127.0.0.1", proxy.port())).unwrap();
        c.write_all(format!("CONNECT localhost:{} HTTP/1.1\r\n\r\n", echo_port + 1).as_bytes())
            .unwrap();
        let mut reply = [0u8; 128];
        let n = c.read(&mut reply).unwrap();
        assert!(String::from_utf8_lossy(&reply[..n]).contains("403 Forbidden"));
        assert!(matches!(
            proxy.events()[2],
            ProxyEvent::Rejected { ref reason, .. } if reason.contains("port_not_allowed")
        ));

        // shutdown 后端口关闭（run 生命周期锚点）：有界轮询替代定长 sleep
        // （P5 验收 b 项——固定 100ms 在高负载下会撞上内核收尾的竞态）。
        let port = proxy.port();
        proxy.shutdown();
        assert!(
            wait_port_closed(port, Duration::from_secs(2)),
            "port must close within 2s of shutdown"
        );
    }

    #[test]
    fn proxy_forwards_absolute_form_http() {
        // 本地"HTTP"上游：写一行响应即关（Connection: close 语义）。
        let upstream = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let upstream_port = upstream.local_addr().unwrap().port();
        thread::spawn(move || {
            if let Ok((mut s, _)) = upstream.accept() {
                let mut buf = [0u8; 512];
                let _ = s.read(&mut buf);
                let _ = s.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nrip",
                );
            }
        });
        let proxy = EgressProxy::spawn(EgressAllowlist {
            hosts: vec!["localhost".to_string()],
            ports: vec![upstream_port],
        })
        .unwrap();
        let mut c = TcpStream::connect(("127.0.0.1", proxy.port())).unwrap();
        c.write_all(
            format!(
                "GET http://localhost:{upstream_port}/x?q=1 HTTP/1.1\r\nHost: localhost\r\n\r\n"
            )
            .as_bytes(),
        )
        .unwrap();
        let mut reply = String::new();
        let _ = c.read_to_string(&mut reply);
        assert!(reply.contains("200 OK"));
        assert!(reply.ends_with("rip"));
        assert!(matches!(
            proxy.events()[0],
            ProxyEvent::ForwardAccepted { ref host, port, ref method }
                if host == "localhost" && port == upstream_port && method == "GET"
        ));
        proxy.shutdown();
    }
}
