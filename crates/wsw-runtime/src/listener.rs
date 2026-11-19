//! Listener socket 运行时（WSW-2-ADR W2-D1/D5/D6/D7）。
//!
//! - [`bind_guard`]：loopback 拒绑（`ConfigError::NonLoopback`，exit 级拒绝由
//!   进程调用面承载；PLAN §7.1 :512-514）；
//! - accept 源二次校验：hudsucker 0.25.0 builder 面只暴露 with_addr/with_listener
//!   （`vendor/hudsucker/src/proxy/builder.rs:86-93`），内部 accept loop 私有，
//!   无逐连接 accept 钩子——二次校验落 [`listener::handler::WswHandler`] 每个
//!   请求入口对 `ctx.client_addr` 做 `is_loopback_peer` 复核（fail-closed：
//!   非 loopback 对端任何请求都不被服务，403 + 五态日志）；
//! - [`AuditedConnector`]：查点 C 上游 connector——DNS 解析 → 全地址过
//!   `scope::audit_resolved` → Pinned-IP TCP 连接（SNI/Host 保持原域名；
//!   PLAN :525 DNS rebinding 面；解析面 [`DnsResolver`] 可注入，生产恒
//!   [`SystemDnsResolver`]）；
//! - [`WswListener`]：ListenerState 六态生命周期驱动（`ListenerState::transition`
//!   既有词表）+ 拦截队列停机收口（全 EXPIRED，PLAN §7.5 :553）。

pub mod handler;

use crate::authority::WswAuthority;
use crate::scope::{self, ScopeRules};
use crate::{ConfigError, ListenerState, StateError};
pub use handler::{
    InterceptCoordinator, InterceptResolution, ListenerShared, OutOfScopePolicy, ProxyErrorClass,
    RuntimeOptions,
};
use hudsucker::rustls::crypto::aws_lc_rs;
use hudsucker::rustls::pki_types::{pem::PemObject, CertificateDer};
use hudsucker::rustls::ClientConfig;
use hudsucker::rustls::RootCertStore;
use hudsucker::Proxy;
use std::error::Error as StdError;
use std::fmt;
use std::future::Future;
use std::io;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tower_service::Service;
use tracing::error as tracing_error;

/// Listener 运行时错误面。
#[derive(Debug)]
pub enum ListenerError {
    /// 配置面（`ConfigError::NonLoopback` 消费位，负例矩阵第 1 行断言）。
    Config(ConfigError),
    /// bind 失败。
    Bind(io::Error),
    /// CA 初始化失败（`downstream_tls` 域）。
    Ca(crate::authority::AuthorityError),
    /// 持久化面（024 store）。
    Persist(database::DbError),
    /// 状态机非法跃迁。
    State(StateError),
    /// 内部装配错误。
    Internal(String),
}

impl fmt::Display for ListenerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ListenerError::Config(e) => write!(f, "config: {e:?}"),
            ListenerError::Bind(e) => write!(f, "bind: {e}"),
            ListenerError::Ca(e) => write!(f, "ca: {e}"),
            ListenerError::Persist(e) => write!(f, "persist: {e}"),
            ListenerError::State(e) => write!(f, "state: {e:?}"),
            ListenerError::Internal(m) => write!(f, "internal: {m}"),
        }
    }
}

impl StdError for ListenerError {}

/// bind 守卫（PLAN §7.1 :512-514；W2-D5）：非 loopback 地址 exit 级拒绝，
/// 返回错误由调用面决定退出。bind 本身失败（占用等）→ `Bind`。
pub async fn bind_guard(addr: SocketAddr) -> Result<TcpListener, ListenerError> {
    if !addr.ip().is_loopback() {
        tracing_error!(
            class = "bind_guard",
            bind = %addr,
            "refuse to bind non-loopback address (proxy listens on 127.0.0.1/::1 only)"
        );
        return Err(ListenerError::Config(ConfigError::NonLoopback));
    }
    TcpListener::bind(addr).await.map_err(ListenerError::Bind)
}

/// 上游 connector 错误（携带查点 C 审计拒绝记录，供 `handle_error` 下溯链
/// downcast 转 403 + 可见记录）。
#[derive(Debug)]
pub enum ConnectorError {
    /// 目标 Uri 无 host。
    BadUri,
    /// scope IP 审计拒绝（查点 C；remote_ip 落记录）。
    ScopeRejected(handler::ScopeRejectRecord),
    /// DNS/TCP IO 失败（`upstream_connect` 五态）。
    Io(io::Error),
}

impl fmt::Display for ConnectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectorError::BadUri => write!(f, "connector: destination has no host"),
            ConnectorError::ScopeRejected(r) => write!(f, "connector scope rejected: {r}"),
            ConnectorError::Io(e) => write!(f, "connector io: {e}"),
        }
    }
}

impl StdError for ConnectorError {
    /// 变体内部记录挂入 source 链（hyper-util `e!(Connect, src)` 只把
    /// ConnectorError 本体挂为 legacy::Error 的 source——record 藏在变体内，
    /// 不链入则 handle_error 侧 `downcast_ref::<ScopeRejectRecord>()` 永远
    /// 匹配不到，查点 C 拒绝会落成 502 而非 403；scope_gate.rs 集成级负例
    /// 实测钉住此链形）。
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            ConnectorError::ScopeRejected(r) => Some(r),
            _ => None,
        }
    }
}

/// DNS 解析面（查点 C 可注入；默认系统解析。集成测试喂解析桩免改
/// /etc/hosts——WSW-2-TASK §四.2 DNS rebinding 行集成级口径的承接位）。
pub trait DnsResolver: Send + Sync {
    /// host → 去重 IP 列表（IP 字面量直取；域名走系统 DNS 全地址）。
    fn resolve(&self, host: &str, port: u16) -> Result<Vec<IpAddr>, ConnectorError>;
}

/// 系统解析默认实现（生产路径恒此面）。
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemDnsResolver;

impl DnsResolver for SystemDnsResolver {
    fn resolve(&self, host: &str, port: u16) -> Result<Vec<IpAddr>, ConnectorError> {
        resolve_ips_system(host, port)
    }
}

/// 系统 DNS 解析（IP 字面量直取；域名 `(host, port).to_socket_addrs()` 全地址）。
fn resolve_ips_system(host: &str, port: u16) -> Result<Vec<IpAddr>, ConnectorError> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(vec![ip]);
    }
    let addrs: Vec<SocketAddr> = (host, port)
        .to_socket_addrs()
        .map_err(ConnectorError::Io)?
        .collect();
    if addrs.is_empty() {
        return Err(ConnectorError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            format!("dns resolved no addresses for {host}"),
        )));
    }
    let mut ips = Vec::new();
    for a in addrs {
        if !ips.contains(&a.ip()) {
            ips.push(a.ip());
        }
    }
    Ok(ips)
}

/// 查点 C 审计 connector：DNS 解析完成、TCP 连接前，实际 IP 全地址过闸
/// （`document_fetch.rs:460-514` 语义；Pinned-IP：按审计通过 IP 直连，
/// Host/SNI 保持原域名——域名串过闸 ≠ IP 过闸，PLAN :525）。
#[derive(Clone)]
pub struct AuditedConnector {
    rules: Arc<ScopeRules>,
    shared: Arc<ListenerShared>,
    resolver: Arc<dyn DnsResolver>,
}

impl AuditedConnector {
    /// 构造（规则与计数面挂共享运行面；解析面 = 系统解析）。
    pub fn new(shared: &Arc<ListenerShared>) -> Self {
        Self::with_resolver(shared, Arc::new(SystemDnsResolver))
    }

    /// 注入解析面（集成测试桩；生产恒 [`SystemDnsResolver`]）。
    pub fn with_resolver(shared: &Arc<ListenerShared>, resolver: Arc<dyn DnsResolver>) -> Self {
        Self {
            rules: Arc::new(shared.rules.clone()),
            shared: Arc::clone(shared),
            resolver,
        }
    }
}

impl Service<hudsucker::hyper::Uri> for AuditedConnector {
    type Response = hudsucker::hyper_util::rt::TokioIo<TcpStream>;
    type Error = ConnectorError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, dst: hudsucker::hyper::Uri) -> Self::Future {
        let rules = Arc::clone(&self.rules);
        let shared = Arc::clone(&self.shared);
        let resolver = Arc::clone(&self.resolver);
        Box::pin(async move {
            let host = dst.host().ok_or(ConnectorError::BadUri)?.to_string();
            let port = dst.port_u16().unwrap_or(match dst.scheme_str() {
                Some("https") => 443,
                _ => 80,
            });
            let ips = resolver.resolve(&host, port)?;
            // 查点 C：全地址过闸；任一私网/元数据命中即拒（audit_resolved
            // 全有或全无语义），拒绝记录可见（remote_ip 落行 + 计数）。
            let traces = match scope::audit_resolved(&rules, &ips) {
                Ok(traces) => {
                    for t in &traces {
                        tracing::info!(
                            class = "scope_trace",
                            kind = t.kind,
                            detail = %t.detail,
                            host = %host,
                            "upstream connect audit trace (explicit allow_private)"
                        );
                    }
                    traces
                }
                Err(rejections) => {
                    shared
                        .counters
                        .scope_blocked_total
                        .fetch_add(1, Ordering::Relaxed);
                    let record = handler::ScopeRejectRecord {
                        host: host.clone(),
                        ip: rejections[0].ip,
                        reason: rejections[0].reason.clone(),
                    };
                    tracing_error!(
                        class = "scope_blocked",
                        point = "connect_audit",
                        host = %host,
                        remote_ip = %rejections[0].ip,
                        reason = %rejections[0].reason,
                        "upstream DNS resolved IP rejected by scope audit (DNS rebinding guard)"
                    );
                    return Err(ConnectorError::ScopeRejected(record));
                }
            };
            // Pinned-IP 连接：取首个通过审计的解析 IP（Host/SNI 由上层保持）。
            let _ = traces;
            let addr = SocketAddr::new(ips[0], port);
            let tcp = TcpStream::connect(addr).await.map_err(ConnectorError::Io)?;
            Ok(hudsucker::hyper_util::rt::TokioIo::new(tcp))
        })
    }
}

/// 上游 TLS 根库：webpki 公共根 + `upstream_ca_paths` 追加信任面（本地回环
/// TLS 夹具自签证书；POC `--upstream-ca` 先例 main.rs:146-166）。
fn build_upstream_roots(extra: &[PathBuf]) -> Result<RootCertStore, ListenerError> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let mut n = 0;
    for path in extra {
        for cert in CertificateDer::pem_file_iter(path).map_err(|e| {
            ListenerError::Internal(format!("read upstream ca {}: {e}", path.display()))
        })? {
            roots
                .add(cert.map_err(|e| ListenerError::Internal(format!("parse upstream ca: {e}")))?)
                .map_err(|e| ListenerError::Internal(format!("add upstream ca: {e}")))?;
            n += 1;
        }
    }
    tracing::info!(
        webpki_roots = roots.len() - n,
        extra_cas = n,
        "upstream root store assembled (webpki public roots + explicit extra CA files)"
    );
    Ok(roots)
}

/// Listener 六态运行时句柄。
pub struct WswListener {
    /// 共享运行面（handler/连接器/状态页共用）。
    pub shared: Arc<ListenerShared>,
    shutdown_tx: watch::Sender<bool>,
    /// proxy 任务句柄（stop 时 take 并 await 收口）。
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

/// 状态页/对账面快照（W2-D8 代理状态页数据源；rp_server JSON 投影同形）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ListenerStatus {
    /// listener ID（wsw-lis- 前缀）。
    pub listener_id: String,
    /// 名称。
    pub name: String,
    /// 绑定地址（loopback）。
    pub bind: String,
    /// 六态。
    pub state: String,
    /// 拦截开关。
    pub intercept_enabled: bool,
    /// 累计请求数。
    pub requests_total: u64,
    /// 累计响应数。
    pub responses_total: u64,
    /// scope 拒绝数（可见记录面）。
    pub scope_blocked_total: u64,
    /// out-of-scope passive 放行数。
    pub out_of_scope_passive_total: u64,
    /// 队列 pending 数。
    pub queue_pending: usize,
    /// CAS 捕获登记数。
    pub captures_total: u64,
    /// 错误五态计数。
    pub error_counts: std::collections::BTreeMap<String, u64>,
    /// 启动时刻（ISO8601 UTC）。
    pub started_at: String,
}

impl WswListener {
    /// 启动 Listener（Stopped→Starting→Running；失败 → Failed）。
    ///
    /// 步骤：配置校验（既有 `ListenerConfig::validate` 词表）→ bind 守卫 →
    /// CA 打开/复用 → 024 store 打开 + 迁移 + 重启对账（回载/过期）→
    /// Proxy 组装（AuditedConnector + WswHandler）→ spawn。
    pub async fn start(
        options: RuntimeOptions,
        migrations_dir: &Path,
    ) -> Result<Arc<Self>, ListenerError> {
        let mut state_now = ListenerState::Stopped;
        let fail = |e: ListenerError, state: &mut ListenerState| -> ListenerError {
            let _ = state.transition(ListenerState::Failed);
            tracing_error!(class = "listener", error = %e, "listener start failed");
            e
        };
        options
            .config
            .validate()
            .map_err(|e| fail(ListenerError::Config(e), &mut state_now))?;
        state_now = state_now
            .transition(ListenerState::Starting)
            .map_err(ListenerError::State)?;

        // 1) bind 守卫（loopback 拒绑）
        let listener = bind_guard(options.config.bind).await.map_err(|e| {
            let _ = state_now.transition(ListenerState::Failed);
            tracing_error!(class = "listener", bind = %options.config.bind, error = %e, "bind failed");
            e
        })?;

        // 2) CA 打开/复用（0600/0700；已存在则加载）
        let ca_dir = options
            .ca_dir
            .clone()
            .unwrap_or_else(crate::authority::resolve_ca_dir);
        let authority =
            WswAuthority::open(&ca_dir, aws_lc_rs::default_provider()).map_err(|e| {
                let _ = state_now.transition(ListenerState::Failed);
                ListenerError::Ca(e)
            })?;

        // 3) 024 store：打开 + 迁移 + 重启对账（W2-D6）
        let persist = match &options.db_path {
            Some(db) => {
                let store =
                    crate::persist::PersistStore::open_file(db).map_err(ListenerError::Persist)?;
                store
                    .run_migrations(migrations_dir)
                    .map_err(ListenerError::Persist)?;
                let outcome = store
                    .reconcile_on_start(&crate::persist::now_iso(), "reconcile_on_start")
                    .map_err(ListenerError::Persist)?;
                tracing::info!(
                    reloaded = outcome.reloaded.len(),
                    expired = outcome.expired_count,
                    "intercept queue reconciled on start"
                );
                Some((store, outcome.reloaded))
            }
            None => None,
        };

        let reloaded_rows = match &persist {
            Some((_, reloaded)) => reloaded.clone(),
            None => Vec::new(),
        };
        let shared = Arc::new(ListenerShared::new(options, persist.map(|(s, _)| s)));
        shared.coordinator.recover(reloaded_rows);

        // 4) 上游 connector：审计 connector 包入 HttpsConnector（ALPN h2 面）
        let mut roots = build_upstream_roots(&shared.options.upstream_ca_paths)?;
        let _ = &mut roots;
        let client_config =
            ClientConfig::builder_with_provider(Arc::new(aws_lc_rs::default_provider()))
                .with_safe_default_protocol_versions()
                .map_err(|e| ListenerError::Internal(format!("tls protocol versions: {e}")))?
                .with_root_certificates(roots)
                .with_no_client_auth();
        let connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_tls_config(client_config)
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .wrap_connector(AuditedConnector::new(&shared));

        // 5) 处理器 + Proxy 组装
        let handler = handler::WswHandler::new(Arc::clone(&shared));
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
        let graceful = async move {
            loop {
                if *shutdown_rx.borrow() {
                    break;
                }
                if shutdown_rx.changed().await.is_err() {
                    break;
                }
            }
        };
        let proxy = Proxy::builder()
            .with_listener(listener)
            .with_ca(authority)
            .with_http_connector(connector)
            .with_http_handler(handler.clone())
            .with_websocket_handler(handler)
            .with_graceful_shutdown(graceful)
            .build()
            .map_err(|e| ListenerError::Internal(format!("proxy build: {e}")))?;

        // 6) 生命周期驱动
        shared
            .set_state(ListenerState::Starting)
            .map_err(ListenerError::State)?;
        let shared_for_task = Arc::clone(&shared);
        let task = tokio::spawn(async move {
            if let Err(e) = proxy.start().await {
                let _ = shared_for_task.set_state(ListenerState::Failed);
                tracing_error!(class = "listener", error = %e, "proxy task terminated");
            }
        });
        shared
            .set_state(ListenerState::Running)
            .map_err(ListenerError::State)?;

        // 登记 listener 行（024；无持久面时跳过）。
        if let Some(store) = shared.persist() {
            let row = crate::persist::ListenerRow {
                listener_id: shared.options.listener_id.clone(),
                name: shared.options.name.clone(),
                bind: shared.options.config.bind.to_string(),
                state: ListenerState::Running,
                config_json: serde_json::to_string(&shared.options)
                    .unwrap_or_else(|_| "{}".to_string()),
                created_at: shared.started_at.clone(),
                updated_at: crate::persist::now_iso(),
            };
            if let Err(e) = store.register_listener(&row) {
                tracing_error!(class = "listener", error = %e, "listener row register failed");
            }
        }

        tracing::info!(
            listener_id = %shared.options.listener_id,
            bind = %shared.options.config.bind.to_string(),
            intercept = shared.options.intercept_enabled,
            "wsw listener running (loopback only; scope gate backend-enforced)"
        );
        Ok(Arc::new(Self {
            shared,
            shutdown_tx,
            task: Mutex::new(Some(task)),
        }))
    }

    /// 停止（Running/Degraded→Stopping→Stopped；拦截队列全量 EXPIRED 收口）。
    pub async fn stop(&self) -> Result<(), ListenerError> {
        // Stopping：仅 Running/Degraded 可进（既有 transition 词表）。
        self.shared
            .set_state(ListenerState::Stopping)
            .map_err(ListenerError::State)?;
        // 拦截队列收口：内存 waiter 清 + 全 EXPIRED（PLAN §7.5 :553）。
        self.shared.coordinator.shutdown(&crate::persist::now_iso());
        if let Some(store) = self.shared.persist() {
            if let Err(e) = store.expire_all_queued(&crate::persist::now_iso(), "listener_stopped")
            {
                tracing_error!(class = "listener", error = %e, "persist expire_all_queued failed");
            }
            let row = crate::persist::ListenerRow {
                listener_id: self.shared.options.listener_id.clone(),
                name: self.shared.options.name.clone(),
                bind: self.shared.options.config.bind.to_string(),
                state: ListenerState::Stopped,
                config_json: serde_json::to_string(&self.shared.options)
                    .unwrap_or_else(|_| "{}".to_string()),
                created_at: self.shared.started_at.clone(),
                updated_at: crate::persist::now_iso(),
            };
            if let Err(e) = store.register_listener(&row) {
                tracing_error!(class = "listener", error = %e, "listener row stop register failed");
            }
        }
        let _ = self.shutdown_tx.send(true);
        let task = self.task.lock().expect("listener task mutex").take();
        if let Some(task) = task {
            let _ = task.await;
        }
        self.shared
            .set_state(ListenerState::Stopped)
            .map_err(ListenerError::State)?;
        tracing::info!(
            listener_id = %self.shared.options.listener_id,
            "wsw listener stopped (queue drained, all pending intercepts expired)"
        );
        Ok(())
    }

    /// 当前六态。
    pub fn state(&self) -> ListenerState {
        self.shared.state()
    }

    /// 状态快照（W2-D8 代理状态页数据源）。
    pub fn status(&self) -> ListenerStatus {
        let counters = &self.shared.counters;
        ListenerStatus {
            listener_id: self.shared.options.listener_id.clone(),
            name: self.shared.options.name.clone(),
            bind: self.shared.options.config.bind.to_string(),
            state: crate::persist::listener_state_to_db(self.shared.state()).to_string(),
            intercept_enabled: self.shared.options.intercept_enabled,
            requests_total: counters.requests_total.load(Ordering::Relaxed),
            responses_total: counters.responses_total.load(Ordering::Relaxed),
            scope_blocked_total: counters.scope_blocked_total.load(Ordering::Relaxed),
            out_of_scope_passive_total: counters.out_of_scope_passive_total.load(Ordering::Relaxed),
            queue_pending: self.shared.coordinator.pending(),
            captures_total: counters.captures_total.load(Ordering::Relaxed),
            error_counts: counters.error_counts(),
            started_at: self.shared.started_at.clone(),
        }
    }
}

use std::sync::atomic::Ordering;
