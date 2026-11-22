//! `listener::handler`：HTTP/WS 处理器 + 拦截队列桥 + 错误五态
//! （WSW-2-ADR W2-D1/D5/D6/D7）。
//!
//! - 流式读 + 帽值：透传路径 = tee 流式捕获（≤`max_body_bytes` 缓冲，超帽
//!   `truncated` 不落 CAS）；拦截路径 = 聚合读（CL 前检 + 聚合帽，超帽 →
//!   `parse_framing` 400）。
//! - scope 双查点：查点 A（本文件 `handle_request`，连接前闸）；查点 C
//!   （`listener::AuditedConnector` 解析 IP 审计）；查点 B 逐跳 = 每个
//!   经代理请求（重定向第二跳即新请求）复走查点 A。
//! - 拦截队列接入：内存 `PendingQueue`（既有原语）+ 024 persist 双面；
//!   Forward / Drop / Modified-Forward 三动作；TTL/队满/停机全部 EXPIRED
//!   终态 + 可见记录。
//! - 错误五态 `ProxyErrorClass`：scope_blocked(403)/upstream_connect(502)/
//!   upstream_tls(502)/downstream_tls(连接关)/parse_framing(400/502)。
//! - 脱敏日志投影：日志只落 method/uri/host/status/长度/五态计数 +
//!   `wsw_message::redact` 命中的头名列表（**任何头值/体值不落日志**，
//!   DESIGN §4.5 / PLAN §7.5 :555）。

use crate::persist::{self, ExchangeRow, InterceptDirection, InterceptRow, PersistStore};
use crate::scope::{self, ScopeRules, ScopeTarget};
use crate::{
    is_loopback_peer, InterceptState, ListenerConfig, ListenerState, PendingQueue, QueueError,
};
use http_body_util::BodyExt;
use hudsucker::hyper::body::{Body as HttpBody, Bytes};
use hudsucker::hyper::header::{CONTENT_LENGTH, CONTENT_TYPE};
use hudsucker::hyper::{Request, Response, StatusCode};
use hudsucker::tokio_tungstenite::tungstenite::Message;
use hudsucker::{
    Body, HttpContext, HttpHandler, RequestOrResponse, WebSocketContext, WebSocketHandler,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::fmt;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::oneshot;
use tracing::error as tracing_error;

/// 错误五态词表（W2-D7；PLAN §7.5 :555 可诊断错误 + DESIGN §4.5 凭据不进日志）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyErrorClass {
    /// 闸拒绝（查点 A/B/C）→ 403。
    ScopeBlocked,
    /// 上游 TCP/DNS 连接失败/超时 → 502。
    UpstreamConnect,
    /// 上游 TLS 握手失败/证书固定检出 → 502。
    UpstreamTls,
    /// CA 签发失败/缓存异常/下游握手失败 → 连接关（RUNNING→DEGRADED 评估）。
    DownstreamTls,
    /// 解析失败/头帽/体帽/chunked 非法 → 400/502 按 cause。
    ParseFraming,
}

impl ProxyErrorClass {
    /// 词表串（024 reason 列 + 日志 class 字段口径）。
    pub fn as_str(self) -> &'static str {
        match self {
            ProxyErrorClass::ScopeBlocked => "scope_blocked",
            ProxyErrorClass::UpstreamConnect => "upstream_connect",
            ProxyErrorClass::UpstreamTls => "upstream_tls",
            ProxyErrorClass::DownstreamTls => "downstream_tls",
            ProxyErrorClass::ParseFraming => "parse_framing",
        }
    }
}

/// `cause_chain_summary`：`Error::source()` 链式拼接错误串（POC
/// `wsw-poc/src/main.rs:110-127` 范式收敛；帽 8 层；禁整头/整体落日志）。
pub fn cause_chain_summary(err: &dyn StdError) -> String {
    let mut chain = vec![err.to_string()];
    let mut src = err.source();
    while let Some(e) = src {
        if chain.len() >= 8 {
            chain.push("...".to_string());
            break;
        }
        chain.push(e.to_string());
        src = e.source();
    }
    chain.join(" <- ")
}

/// 查点 C 拒绝记录（connector → `handle_error` 传递载体；remote_ip 落行）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeRejectRecord {
    /// 目标 host。
    pub host: String,
    /// 被拒 IP（mapped 归一后）。
    pub ip: IpAddr,
    /// private_reason/词表原因串。
    pub reason: String,
}

impl fmt::Display for ScopeRejectRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "host={} remote_ip={} reason={}",
            self.host, self.ip, self.reason
        )
    }
}

impl StdError for ScopeRejectRecord {}

/// connector 错误链 → 查点 C scope 拒绝响应（downcast 找 [`ScopeRejectRecord`]
/// → 403 + `x-wsw-scope-decision`/`x-wsw-reason` 头 + 计数与日志；返回 None =
/// 非 scope 面，交调用方继续分类）。
///
/// `handle_error`（hyper-util legacy::Error 无公共构造器）与集成测试
/// `tests/scope_gate.rs`（包装错误链直喂）走同一实现——查点 C
/// `AuditedConnector::call` 拒 → 连接面错误 → 本函数 403 的 glue 全链路
/// 由测试真实执行（WSW-2-TASK §四.2 DNS rebinding 行集成级口径）。
pub fn respond_to_scope_reject(
    shared: &Arc<ListenerShared>,
    err: &(dyn StdError + 'static),
) -> Option<Response<Body>> {
    let mut scope_record: Option<ScopeRejectRecord> = None;
    let mut src: Option<&(dyn StdError + 'static)> = Some(err);
    while let Some(e) = src {
        if let Some(r) = e.downcast_ref::<ScopeRejectRecord>() {
            scope_record = Some(r.clone());
        }
        src = e.source();
    }
    let record = scope_record?;
    shared
        .counters
        .scope_blocked_total
        .fetch_add(1, Ordering::Relaxed);
    shared.counters.bump_error(ProxyErrorClass::ScopeBlocked);
    tracing_error!(
        class = "scope_blocked",
        point = "connect_audit",
        scope_decision = "out_of_scope_blocked",
        host = %record.host,
        remote_ip = %record.ip,
        reason = %record.reason,
        cause_chain = %cause_chain_summary(err),
        "upstream connect rejected by scope IP audit (403)"
    );
    Some(reject_response(
        StatusCode::FORBIDDEN,
        "out_of_scope_blocked",
        &record.reason,
    ))
}

/// 运行配置（组合复用既有 `ListenerConfig` 根原语，不触碰其定义）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeOptions {
    /// `wsw-lis-` 前缀 ID。
    pub listener_id: String,
    /// 名称。
    pub name: String,
    /// `wsw-ses-` 前缀 ID（规则源引用）。
    pub session_id: String,
    /// out-of-scope 命中策略：blocked（默认，403）/ passive_forward（放行留痕）。
    pub out_of_scope_policy: String,
    /// 既有 ListenerConfig（validate 面复用）。
    pub config: ListenerConfig,
    /// scope 规则（`wsw_session.scope_rules_json` 解析产物）。
    pub scope_rules: ScopeRules,
    /// 拦截开关（默认关，PLAN :133）。
    pub intercept_enabled: bool,
    /// 拦截命中匹配器（本批最小 host/method/path 匹配）。
    pub intercept_matcher: InterceptMatcher,
    /// CA 目录（None → `resolve_ca_dir()`）。
    pub ca_dir: Option<std::path::PathBuf>,
    /// 上游追加信任 CA 文件（本地 TLS 夹具面）。
    pub upstream_ca_paths: Vec<std::path::PathBuf>,
    /// 024 库路径（None → 不落库，纯内存面）。
    pub db_path: Option<std::path::PathBuf>,
    /// CAS 正文根（缺省 `rip-data/artifacts`）。
    pub artifacts_root: std::path::PathBuf,
    /// Wire→CAS 捕获开关（D9）。
    pub capture_cas: bool,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            listener_id: "wsw-lis-default".to_string(),
            name: "wsw-2 listener".to_string(),
            session_id: "wsw-ses-default".to_string(),
            out_of_scope_policy: "blocked".to_string(),
            config: ListenerConfig {
                bind: std::net::SocketAddr::from(([127, 0, 0, 1], 3128)),
                queue_capacity: 64,
                max_header_bytes: 64 * 1024,
                max_body_bytes: 8 * 1024 * 1024,
                queue_ttl_seconds: 120,
            },
            scope_rules: ScopeRules::default(),
            intercept_enabled: false,
            intercept_matcher: InterceptMatcher::default(),
            ca_dir: None,
            upstream_ca_paths: Vec::new(),
            db_path: None,
            artifacts_root: wsw_message::cas::resolve_artifacts_dir(),
            capture_cas: true,
        }
    }
}

/// 拦截命中匹配器（W2-D6 最小实现：host/method/path 前缀匹配；结构化规则面
/// WSW-3 扩全）。空列表 = 该维全匹配。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterceptMatcher {
    /// host 精确/前缀匹配（小写）。
    pub hosts: Vec<String>,
    /// method 匹配（大写）。
    pub methods: Vec<String>,
    /// path 前缀匹配。
    pub path_prefixes: Vec<String>,
}

impl InterceptMatcher {
    /// 命中判定。
    pub fn matches(&self, host: &str, method: &str, path: &str) -> bool {
        let host_l = host.to_ascii_lowercase();
        let method_u = method.to_ascii_uppercase();
        let host_hit = self.hosts.is_empty()
            || self.hosts.iter().any(|p| {
                let p = p.to_ascii_lowercase();
                host_l == p || host_l.starts_with(&p)
            });
        let method_hit = self.methods.is_empty()
            || self
                .methods
                .iter()
                .any(|p| method_u == p.to_ascii_uppercase());
        let path_hit =
            self.path_prefixes.is_empty() || self.path_prefixes.iter().any(|p| path.starts_with(p));
        host_hit && method_hit && path_hit
    }
}

/// out-of-scope 命中策略（wsw_session 行级配置；W2-D5 查点 A 分支）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutOfScopePolicy {
    /// 拒（默认）：403 + `scope_decision=out_of_scope_blocked`。
    Blocked,
    /// 放行不拦截改写 + `out_of_scope_passive` 记录。
    PassiveForward,
}

impl OutOfScopePolicy {
    /// 词表串解析（非法回落 blocked——安全默认）。
    pub fn parse(s: &str) -> Self {
        if s == "passive_forward" {
            OutOfScopePolicy::PassiveForward
        } else {
            OutOfScopePolicy::Blocked
        }
    }
}

/// 计数面（状态页 + 可见记录）。
#[derive(Debug, Default)]
pub struct Counters {
    /// 累计请求。
    pub requests_total: AtomicU64,
    /// 累计响应。
    pub responses_total: AtomicU64,
    /// scope 拒绝。
    pub scope_blocked_total: AtomicU64,
    /// out-of-scope passive 放行。
    pub out_of_scope_passive_total: AtomicU64,
    /// 拦截入队。
    pub intercept_total: AtomicU64,
    /// CAS 捕获登记。
    pub captures_total: AtomicU64,
    /// exchange 落库满载丢弃（W3-D2.2：mpsc try_send Err(Full) → 丢行计数，
    /// 转发热路径零阻塞零等待）。
    pub persist_dropped_total: AtomicU64,
    /// exchange 行已入 channel 计数（worker 消费面；吞吐实测分母）。
    pub persist_enqueued_total: AtomicU64,
    /// WS 连接握手行计数（WSW-5：protocol='ws' 行 = 连接面事实，D3）。
    pub ws_connections_total: AtomicU64,
    /// WS 帧行已入 channel 计数（WSW-5 帧捕获对账分母，D3）。
    pub ws_frames_captured_total: AtomicU64,
    /// WS 帧行满载丢弃（WSW-5 帧通道有界丢弃计数可见，D3 同 W3-D2.2 口径）。
    pub ws_frames_dropped_total: AtomicU64,
    /// 五态错误计数（index = ProxyErrorClass as_str）。
    pub errors: Mutex<BTreeMap<&'static str, u64>>,
}

impl Counters {
    /// 五态错误 +1。
    pub fn bump_error(&self, class: ProxyErrorClass) {
        let mut m = self.errors.lock().expect("counters mutex");
        *m.entry(class.as_str()).or_insert(0) += 1;
    }

    /// 五态计数快照（状态页）。
    pub fn error_counts(&self) -> BTreeMap<String, u64> {
        self.errors
            .lock()
            .expect("counters mutex")
            .iter()
            .map(|(k, v)| (k.to_string(), *v))
            .collect()
    }
}

/// 共享运行面（handler/connector/生命周期/状态页共用）。
pub struct ListenerShared {
    /// 运行配置。
    pub options: RuntimeOptions,
    /// scope 规则。
    pub rules: ScopeRules,
    /// out-of-scope 策略。
    pub policy: OutOfScopePolicy,
    /// 计数面。
    pub counters: Counters,
    /// 六态（`ListenerState::transition` 词表强制）。
    state: Mutex<ListenerState>,
    /// 拦截队列桥。
    pub coordinator: InterceptCoordinator,
    /// 024 store（None = 纯内存）。
    persist: Option<PersistStore>,
    /// exchange 落库通道发送端（persist 在场才挂；W3-D2.2 有界 mpsc(2048)）。
    exchange_tx: Option<tokio::sync::mpsc::Sender<ExchangeRow>>,
    /// exchange 落库通道接收端（listener start 取走 spawn worker；取一次）。
    persist_rx: Mutex<Option<tokio::sync::mpsc::Receiver<ExchangeRow>>>,
    /// exchange 行 listener 内单调序号（UNIQUE(listener_id, sequence_no)；
    /// 启动时由 worker 侧 MAX+1 恢复）。
    persist_sequence: AtomicU64,
    /// WS 帧落库通道发送端（persist 在场才挂；WSW-5 D3 有界 mpsc(2048) 同型）。
    wsframe_tx: Option<tokio::sync::mpsc::Sender<persist::WsFrameRow>>,
    /// WS 帧落库通道接收端（listener start 取走 spawn worker；取一次）。
    wsframe_rx: Mutex<Option<tokio::sync::mpsc::Receiver<persist::WsFrameRow>>>,
    /// 在途 capture 组装器登记（worker 清扫面：超龄未推行的 partial 行兜底
    /// 落库——连接中断且双侧流末都未触发时的可见性保证）。
    pub pending_captures: Mutex<BTreeMap<String, Arc<ExchangeCapture>>>,
    /// downstream_tls 连续错误（RUNNING→DEGRADED 评估，DESIGN §2.1）。
    pub consecutive_downstream_tls: AtomicU64,
    /// 启动时刻。
    pub started_at: String,
}

impl ListenerShared {
    /// 构造（listener start 面消费）。persist 在场时同时挂 exchange 落库
    /// 有界通道（cap 2048；W3-D2.2）——接收端由 [`Self::take_persist_rx`]
    /// 取走交给单 worker。
    pub fn new(options: RuntimeOptions, persist: Option<PersistStore>) -> Self {
        let coordinator = InterceptCoordinator::new(
            options.config.queue_capacity,
            options.config.queue_ttl_seconds * 1000,
            persist.clone(),
        );
        let (exchange_tx, persist_rx) = if persist.is_some() {
            let (tx, rx) = tokio::sync::mpsc::channel(2048);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };
        let (wsframe_tx, wsframe_rx) = if persist.is_some() {
            let (tx, rx) = tokio::sync::mpsc::channel(2048);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };
        Self {
            policy: OutOfScopePolicy::parse(&options.out_of_scope_policy),
            rules: options.scope_rules.clone(),
            options,
            counters: Counters::default(),
            state: Mutex::new(ListenerState::Stopped),
            coordinator,
            persist,
            exchange_tx,
            persist_rx: Mutex::new(persist_rx),
            wsframe_tx,
            wsframe_rx: Mutex::new(wsframe_rx),
            persist_sequence: AtomicU64::new(0),
            pending_captures: Mutex::new(BTreeMap::new()),
            consecutive_downstream_tls: AtomicU64::new(0),
            started_at: persist::now_iso(),
        }
    }

    /// sequence 基线恢复（worker 启动面：MAX(sequence_no)+1；此后单调递增）。
    pub fn set_persist_sequence_base(&self, base: u64) {
        self.persist_sequence.store(base, Ordering::Relaxed);
    }

    /// 取走 exchange 落库通道接收端（worker spawn 用；二次调用 None）。
    pub fn take_persist_rx(&self) -> Option<tokio::sync::mpsc::Receiver<ExchangeRow>> {
        self.persist_rx.lock().expect("persist rx mutex").take()
    }

    /// 取走 WS 帧落库通道接收端（worker spawn 用；二次调用 None）。
    pub fn take_wsframe_rx(&self) -> Option<tokio::sync::mpsc::Receiver<persist::WsFrameRow>> {
        self.wsframe_rx.lock().expect("wsframe rx mutex").take()
    }

    /// WS 帧行 try_send（满载 → 丢行 + 计数 + 结构化日志，不等待不重试；
    /// WSW-5 D3 同 W3-D2.2 口径：帧透传热路径零阻塞零等待）。返回 false =
    /// 无落库通道或满载丢弃。
    pub fn push_ws_frame_row(&self, row: persist::WsFrameRow) -> bool {
        let Some(tx) = &self.wsframe_tx else {
            return false; // 纯内存面：无落库通道
        };
        match tx.try_send(row) {
            Ok(()) => {
                self.counters
                    .ws_frames_captured_total
                    .fetch_add(1, Ordering::Relaxed);
                true
            }
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                self.counters
                    .ws_frames_dropped_total
                    .fetch_add(1, Ordering::Relaxed);
                tracing_error!(
                    class = "parse_framing",
                    error_code = "ws_frame_channel_full_drop",
                    "ws frame row dropped: bounded wsframe channel full (2048); forward path unblocked"
                );
                false
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => false,
        }
    }

    /// exchange 行 try_send（满载 → 丢行 + 计数 + 结构化日志，不等待不重试；
    /// W3-D2.2）。返回 false = 满载丢弃。persist_enqueued_total 只计成功入
    /// 通道行（worker 吞吐分母语义）。
    pub fn push_exchange_row(&self, row: ExchangeRow) -> bool {
        let Some(tx) = &self.exchange_tx else {
            return false; // 纯内存面：无落库通道，行即弃（capture_cas/024 行为不变）
        };
        match tx.try_send(row) {
            Ok(()) => {
                self.counters
                    .persist_enqueued_total
                    .fetch_add(1, Ordering::Relaxed);
                true
            }
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                self.counters
                    .persist_dropped_total
                    .fetch_add(1, Ordering::Relaxed);
                tracing_error!(
                    class = "parse_framing",
                    error_code = "persist_channel_full_drop",
                    "exchange row dropped: bounded persist channel full (2048); forward path unblocked"
                );
                false
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => false,
        }
    }

    /// 当前六态。
    pub fn state(&self) -> ListenerState {
        *self.state.lock().expect("listener state mutex")
    }

    /// 六态跃迁（非法跃迁 Err，既有 transition 词表）。
    pub fn set_state(&self, next: ListenerState) -> Result<ListenerState, crate::StateError> {
        let mut guard = self.state.lock().expect("listener state mutex");
        *guard = guard.transition(next)?;
        Ok(*guard)
    }

    /// 024 store 视图。
    pub fn persist(&self) -> Option<&PersistStore> {
        self.persist.as_ref()
    }

    /// downstream_tls 错误登记；连续 ≥5 → 触发 RUNNING→DEGRADED（返回是否触发）。
    pub fn note_downstream_tls_error(&self) -> bool {
        let n = self
            .consecutive_downstream_tls
            .fetch_add(1, Ordering::Relaxed)
            + 1;
        if n >= 5 && self.state() == ListenerState::Running {
            let _ = self.set_state(ListenerState::Degraded);
            return true;
        }
        false
    }

    /// 成功登记（清零连续错误；DEGRADED→RUNNING 恢复）。
    pub fn note_success(&self) {
        self.consecutive_downstream_tls.store(0, Ordering::Relaxed);
        if self.state() == ListenerState::Degraded {
            let _ = self.set_state(ListenerState::Running);
        }
    }
}

/// now → epoch ms。
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// now → epoch ms（listener 清扫面消费；pub(crate) 转发）。
pub(crate) fn now_ms_pub() -> u64 {
    now_ms()
}

/// 生成 exchange 形状 ID（`wsw-<micros>-<seq>-<rand4>`；ids.rs:9 范式）。
pub fn new_exchange_id() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros();
    let ts64 = ts as u64;
    let rand4 = (std::process::id() as u64 ^ ts64 ^ (seq.wrapping_mul(0x9e37_79b9))) & 0xffff;
    format!("wsw-{ts64}-{seq}-{rand4:04x}")
}

// ===========================================================================
// WSW-3 S3：capture→exchange 落库异步面（W3-D2）
// ===========================================================================

/// 体捕获产出（CAS BodyRef 派生列 + 搜索投影片段；行内只落派生列不落字节）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyOutcome {
    /// CAS sha256（未落 CAS = None：空体/超帽/中断）。
    pub sha256: Option<String>,
    /// CAS uri（同上）。
    pub uri: Option<String>,
    /// 完整体长度（截断/中断未知 = None）。
    pub len: Option<i64>,
    /// MIME（Content-Type 投影）。
    pub mime: String,
    /// 'full'/'truncated'。
    pub fidelity: &'static str,
    /// 搜索投影体前 4KiB 文本化（二进制体 = None；DESIGN §4.5）。
    pub snippet: Option<String>,
}

/// 体前 4KiB UTF-8 文本化（非法 UTF-8 = 二进制体 → None 空投影）。
pub fn body_snippet(bytes: &[u8]) -> Option<String> {
    let cap = bytes.len().min(4096);
    std::str::from_utf8(&bytes[..cap])
        .ok()
        .map(|s| s.to_string())
}

/// 搜索文本投影组装（D7：authority+path+头名/头值+体前 4KiB；凭据头值排除
/// ——Authorization/Cookie/Set-Cookie 只落头名不落值；总帽 8KiB）。
pub fn build_search_text(
    authority: &str,
    path: &str,
    header_pairs: &[(&str, &str)],
    snippets: &[Option<String>],
) -> String {
    const CREDENTIAL_HEADERS: [&str; 3] = ["authorization", "cookie", "set-cookie"];
    let mut parts: Vec<String> = Vec::new();
    if !authority.is_empty() {
        parts.push(authority.to_string());
    }
    if !path.is_empty() {
        parts.push(path.to_string());
    }
    for (name, value) in header_pairs {
        if CREDENTIAL_HEADERS.contains(&name.to_ascii_lowercase().as_str()) {
            parts.push(name.to_string()); // 可搜头名不搜凭据值
        } else {
            parts.push(format!("{name} {value}"));
        }
    }
    for s in snippets.iter().flatten() {
        parts.push(s.clone());
    }
    let mut joined = parts.join("\n");
    joined.truncate_bytes(8192);
    joined
}

trait TruncateBytes {
    fn truncate_bytes(&mut self, cap: usize);
}

impl TruncateBytes for String {
    fn truncate_bytes(&mut self, cap: usize) {
        if self.len() <= cap {
            return;
        }
        // char boundary 回退（多字节字符不劈半）。
        let mut cut = cap;
        while cut > 0 && !self.is_char_boundary(cut) {
            cut -= 1;
        }
        self.truncate(cut);
    }
}

/// exchange 捕获组装器：请求入口 snapshot + 响应流末 join 成单行（W3-D2.1
/// 单行纪律）；双侧任一完成面推行（try_send 满载丢弃，W3-D2.2）；worker
/// 清扫面兜底超龄 partial 行。
pub struct ExchangeCapture {
    inner: Mutex<ExchangeCaptureInner>,
    shared: Arc<ListenerShared>,
}

struct ExchangeCaptureInner {
    exchange_id: String,
    session_id: String,
    listener_id: String,
    sequence_no: i64,
    started_at: String,
    started_ms: u64,
    scope_decision: Option<&'static str>,
    method: String,
    scheme: String,
    authority: String,
    path: String,
    http_version: String,
    request_headers_json: Option<String>,
    request_headers_plain: Vec<(String, String)>,
    request_body: Option<BodyOutcome>,
    response_status: Option<i64>,
    response_headers_json: Option<String>,
    response_headers_plain: Vec<(String, String)>,
    response_body: Option<BodyOutcome>,
    response_head_ms: Option<u64>,
    /// 无服务端响应面（拦截 Drop / Modified-Forward scope 拒）：立即 partial 行。
    no_response: bool,
    pushed: bool,
}

impl ExchangeCapture {
    /// 测试面：直构组装器（#[doc(hidden)]；exchange_persist.rs 集成测试
    /// 消费——正常路径组装器只在 handle_request 入口创建）。
    #[doc(hidden)]
    pub fn __test_new(
        shared: &Arc<ListenerShared>,
        exchange_id: &str,
        sequence_no: i64,
        started_ms: u64,
    ) -> Arc<Self> {
        Self::new(
            shared,
            exchange_id.to_string(),
            sequence_no,
            "GET".to_string(),
            "http".to_string(),
            "127.0.0.1:8443".to_string(),
            "/p59c-marker".to_string(),
            "http/1.1".to_string(),
            Some("in_scope"),
            started_ms,
        )
    }

    /// 测试面：起始时刻改写（清扫判龄断言用）。
    #[doc(hidden)]
    pub fn __test_set_started_ms(&self, ms: u64) {
        self.inner
            .lock()
            .expect("exchange capture mutex")
            .started_ms = ms;
    }

    /// 请求入口 snapshot（W3-D2.1：exchange_id/method/scheme/authority/path/
    /// sequence_no/started_at/scope_decision 一次定型）。
    #[allow(clippy::too_many_arguments)]
    fn new(
        shared: &Arc<ListenerShared>,
        exchange_id: String,
        sequence_no: i64,
        method: String,
        scheme: String,
        authority: String,
        path: String,
        http_version: String,
        scope_decision: Option<&'static str>,
        started_ms: u64,
    ) -> Arc<Self> {
        let arc = Arc::new(Self {
            inner: Mutex::new(ExchangeCaptureInner {
                exchange_id: exchange_id.clone(),
                session_id: shared.options.session_id.clone(),
                listener_id: shared.options.listener_id.clone(),
                sequence_no,
                started_at: persist::epoch_ms_to_iso(started_ms as i64),
                started_ms,
                scope_decision,
                method,
                scheme,
                authority,
                path,
                http_version,
                request_headers_json: None,
                request_headers_plain: Vec::new(),
                request_body: None,
                response_status: None,
                response_headers_json: None,
                response_headers_plain: Vec::new(),
                response_body: None,
                response_head_ms: None,
                no_response: false,
                pushed: false,
            }),
            shared: Arc::clone(shared),
        });
        shared
            .pending_captures
            .lock()
            .expect("pending captures mutex")
            .insert(exchange_id, Arc::clone(&arc));
        arc
    }

    /// 请求头投影登记（HeaderMap 迭代序投影——头区非原始 wire，capture 行
    /// capture_fidelity=degraded_no_wire 如实落库，W2-D9/W3-D2.5）。
    pub fn register_request_headers(&self, headers: &hudsucker::hyper::HeaderMap) {
        let pairs: Vec<(String, String)> = headers
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_string(),
                    v.to_str().unwrap_or("<binary>").to_string(),
                )
            })
            .collect();
        let json = serde_json::to_string(
            &pairs
                .iter()
                .map(|(n, v)| serde_json::json!({"name": n, "value": v}))
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|_| "[]".into());
        let mut inner = self.inner.lock().expect("exchange capture mutex");
        if inner.request_headers_json.is_none() {
            inner.request_headers_json = Some(json);
            inner.request_headers_plain = pairs;
        }
    }

    /// 请求体产出登记（流末 CAS 完成面 / 拦截聚合面 / 中断面）。
    pub fn register_request_body(&self, outcome: BodyOutcome) {
        let mut inner = self.inner.lock().expect("exchange capture mutex");
        if inner.request_body.is_none() {
            inner.request_body = Some(outcome);
        }
        self.try_push(&mut inner);
    }

    /// 响应头登记（handle_response 入口；status + 头投影 + TTFB 时刻）。
    pub fn register_response_head(&self, status: u16, headers: &hudsucker::hyper::HeaderMap) {
        let pairs: Vec<(String, String)> = headers
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_string(),
                    v.to_str().unwrap_or("<binary>").to_string(),
                )
            })
            .collect();
        let json = serde_json::to_string(
            &pairs
                .iter()
                .map(|(n, v)| serde_json::json!({"name": n, "value": v}))
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|_| "[]".into());
        let mut inner = self.inner.lock().expect("exchange capture mutex");
        if inner.response_status.is_none() {
            inner.response_status = Some(status as i64);
            inner.response_headers_json = Some(json);
            inner.response_headers_plain = pairs;
            inner.response_head_ms = Some(now_ms());
        }
    }

    /// 响应体产出登记（流末 / 中断面；本侧登记即触发推行——响应侧是
    /// 单行推行的默认触发点，W3-D2.1）。
    pub fn register_response_body(&self, outcome: BodyOutcome) {
        let mut inner = self.inner.lock().expect("exchange capture mutex");
        if inner.response_body.is_none() {
            inner.response_body = Some(outcome);
        }
        self.try_push(&mut inner);
    }

    /// 无服务端响应面（拦截 Drop / Modified-Forward scope 拒 / TTL 自动放行
    /// 前已回本地响应——本地 503/403 不经 handle_response）：立即 partial 行。
    pub fn mark_no_response(&self) {
        let mut inner = self.inner.lock().expect("exchange capture mutex");
        inner.no_response = true;
        self.try_push(&mut inner);
    }

    /// 推行判定：响应体已登记 或 无响应标记 → 组行 try_send（一次性）。
    fn try_push(&self, inner: &mut ExchangeCaptureInner) {
        if inner.pushed || !(inner.response_body.is_some() || inner.no_response) {
            return;
        }
        inner.pushed = true;
        self.shared
            .pending_captures
            .lock()
            .expect("pending captures mutex")
            .remove(&inner.exchange_id);
        let completed_ms = now_ms();
        let row = self.build_row(inner, completed_ms);
        self.shared.push_exchange_row(row);
    }

    /// 清扫兜底推行（worker 周期面；双侧流末都未触发的静默死亡连接 →
    /// 超龄 partial 行强制落库，行不丢）。返回本次推行条数。
    /// （集成测试面消费：#[doc(hidden)] 清扫兜底断言。）
    #[doc(hidden)]
    pub fn sweep_push_stale(&self, now_ms_v: u64) -> usize {
        let mut inner = self.inner.lock().expect("exchange capture mutex");
        if inner.pushed {
            return 0;
        }
        if now_ms_v.saturating_sub(inner.started_ms) < 10_000 {
            return 0;
        }
        inner.pushed = true;
        self.shared
            .pending_captures
            .lock()
            .expect("pending captures mutex")
            .remove(&inner.exchange_id);
        let row = self.build_row(&inner, now_ms_v);
        self.shared.push_exchange_row(row);
        1
    }

    /// 起始时刻 ms（清扫面判龄）。
    #[doc(hidden)]
    pub fn started_ms(&self) -> u64 {
        self.inner
            .lock()
            .expect("exchange capture mutex")
            .started_ms
    }

    /// 行组装（列映射 = W3-D1 表形状；capture_fidelity=degraded_no_wire——
    /// 头区来自 hudsucker http 视图非原始 wire，W2-D9 实态如实落库）。
    /// completed_at/duration_ms = 行定型时刻（响应侧流末 push 或 worker 清扫
    /// 时刻；partial 行同面落定型时刻，不伪造响应完成态——ADR W3-D2.1 修订
    /// 批口径，partial 识别面 = parse_status='partial' + 响应列缺口）。
    fn build_row(&self, inner: &ExchangeCaptureInner, completed_ms: u64) -> ExchangeRow {
        let request_truncated = inner
            .request_body
            .as_ref()
            .map(|b| b.fidelity == "truncated")
            .unwrap_or(false);
        let response_truncated = inner
            .response_body
            .as_ref()
            .map(|b| b.fidelity == "truncated")
            .unwrap_or(false);
        let complete = inner.request_headers_json.is_some()
            && inner.request_body.is_some()
            && inner.response_status.is_some()
            && inner.response_body.is_some()
            && !request_truncated
            && !response_truncated;
        let search = build_search_text(
            &inner.authority,
            &inner.path,
            &inner
                .request_headers_plain
                .iter()
                .map(|(n, v)| (n.as_str(), v.as_str()))
                .chain(
                    inner
                        .response_headers_plain
                        .iter()
                        .map(|(n, v)| (n.as_str(), v.as_str())),
                )
                .collect::<Vec<_>>(),
            &[
                inner.request_body.as_ref().and_then(|b| b.snippet.clone()),
                inner.response_body.as_ref().and_then(|b| b.snippet.clone()),
            ],
        );
        ExchangeRow {
            exchange_id: inner.exchange_id.clone(),
            session_id: inner.session_id.clone(),
            listener_id: inner.listener_id.clone(),
            sequence_no: inner.sequence_no,
            direction: "client_to_server",
            source: "proxy_capture",
            protocol: "http",
            http_version: Some(inner.http_version.clone()),
            tls_version: None,
            alpn: None,
            method: Some(inner.method.clone()),
            scheme: Some(inner.scheme.clone()),
            authority: Some(inner.authority.clone()),
            path: Some(inner.path.clone()),
            response_status: inner.response_status,
            request_headers_json: inner.request_headers_json.clone(),
            response_headers_json: inner.response_headers_json.clone(),
            request_body_sha256: inner.request_body.as_ref().and_then(|b| b.sha256.clone()),
            request_body_uri: inner.request_body.as_ref().and_then(|b| b.uri.clone()),
            request_body_len: inner.request_body.as_ref().and_then(|b| b.len),
            request_mime: inner.request_body.as_ref().map(|b| b.mime.clone()),
            request_body_fidelity: inner.request_body.as_ref().map(|b| b.fidelity),
            response_body_sha256: inner.response_body.as_ref().and_then(|b| b.sha256.clone()),
            response_body_uri: inner.response_body.as_ref().and_then(|b| b.uri.clone()),
            response_body_len: inner.response_body.as_ref().and_then(|b| b.len),
            response_mime: inner.response_body.as_ref().map(|b| b.mime.clone()),
            response_body_fidelity: inner.response_body.as_ref().map(|b| b.fidelity),
            started_at: Some(inner.started_at.clone()),
            completed_at: Some(persist::epoch_ms_to_iso(completed_ms as i64)),
            duration_ms: Some(completed_ms.saturating_sub(inner.started_ms) as i64),
            ttfb_ms: inner
                .response_head_ms
                .map(|h| h.saturating_sub(inner.started_ms) as i64),
            remote_ip: None, // 上游对端 IP 查点 C 面另行核销；capture 行如实缺测
            scope_decision: inner.scope_decision,
            edited: false,
            tags_json: "[]".into(),
            notes: None,
            trace_id: None,
            task_id: None,
            tool_run_id: None,
            artifact_id: None,
            evidence_id: None,
            capture_fidelity: "degraded_no_wire",
            parse_status: if complete { "ok" } else { "partial" },
            error_code: None,
            search_text: Some(search),
        }
    }
}

/// 拦截裁决三动作（W2-D6）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterceptResolution {
    /// 原样放行。
    Forward,
    /// 终态丢弃（客户端 503 + reason 头）。
    Drop,
    /// 编辑态重序列化放行（bytes 替换体；查点 A 重跑 + 终态
    /// modified_forwarded）。
    ModifiedForward(Vec<u8>),
}

impl InterceptResolution {
    /// 五态映射。
    fn state(&self) -> InterceptState {
        match self {
            InterceptResolution::Forward => InterceptState::Forwarded,
            InterceptResolution::Drop => InterceptState::Dropped,
            InterceptResolution::ModifiedForward(_) => InterceptState::ModifiedForwarded,
        }
    }

    /// reason 落账串。
    fn reason(&self) -> &'static str {
        match self {
            InterceptResolution::Forward => "forwarded",
            InterceptResolution::Drop => "dropped",
            InterceptResolution::ModifiedForward(_) => "modified_forwarded",
        }
    }
}

/// 拦截队列桥：内存 `PendingQueue`（既有原语）+ 024 persist 双面 + TTL/停机
/// 全 EXPIRED（W2-D6；W2-D4 行式状态机）。
pub struct InterceptCoordinator {
    queue: Mutex<PendingQueue>,
    waiters: Mutex<BTreeMap<String, oneshot::Sender<InterceptResolution>>>,
    /// 行到期时刻（id → epoch ms；重启回载行沿用原 expires_at）。
    deadlines: Mutex<BTreeMap<String, u64>>,
    persist: Option<PersistStore>,
    ttl_ms: u64,
}

impl InterceptCoordinator {
    /// 构造（capacity/ttl 来自 ListenerConfig 既有词表 1..3600）。
    pub fn new(capacity: usize, ttl_ms: u64, persist: Option<PersistStore>) -> Self {
        Self {
            queue: Mutex::new(
                PendingQueue::new(capacity.max(1), ttl_ms.max(1)).expect("queue params"),
            ),
            waiters: Mutex::new(BTreeMap::new()),
            deadlines: Mutex::new(BTreeMap::new()),
            persist,
            ttl_ms: ttl_ms.max(1),
        }
    }

    /// TTL 毫秒。
    pub fn ttl_ms(&self) -> u64 {
        self.ttl_ms
    }

    /// 入队（内存 + persist 双面；返回拦截行 ID + 裁决等待端）。
    pub fn enqueue(
        &self,
        host: &str,
        method: &str,
        path: &str,
        exchange_id: &str,
        listener_id: &str,
        session_id: &str,
    ) -> Result<(String, oneshot::Receiver<InterceptResolution>), QueueError> {
        let ms = now_ms();
        let id = persist::generate_intercept_id();
        {
            let mut queue = self.queue.lock().expect("queue mutex");
            queue.expire(ms);
            queue.enqueue(&id, ms)?;
        }
        let (tx, rx) = oneshot::channel();
        self.waiters
            .lock()
            .expect("waiters mutex")
            .insert(id.clone(), tx);
        self.deadlines
            .lock()
            .expect("deadlines mutex")
            .insert(id.clone(), ms.saturating_add(self.ttl_ms));
        if let Some(store) = &self.persist {
            let row = InterceptRow {
                intercept_id: id.clone(),
                event_id: format!("{exchange_id}-req"),
                listener_id: listener_id.to_string(),
                session_id: session_id.to_string(),
                exchange_id: Some(exchange_id.to_string()),
                direction: InterceptDirection::Request,
                state: InterceptState::Queued,
                host: Some(host.to_string()),
                method: Some(method.to_string()),
                path: Some(path.to_string()),
                queued_at: persist::epoch_ms_to_iso(ms as i64),
                expires_at: persist::epoch_ms_to_iso((ms.saturating_add(self.ttl_ms)) as i64),
                resolved_at: None,
                reason: None,
            };
            if let Err(e) = store.enqueue_intercept(&row) {
                // persist 失败可见但放行入队（内存面继续承载；错误计数 + 日志）。
                tracing_error!(
                    class = "parse_framing",
                    error_code = "persist_enqueue_failed",
                    cause = ?e,
                    intercept_id = %id,
                    "intercept row persist failed (in-memory queue continues)"
                );
            }
        }
        Ok((id, rx))
    }

    /// 裁决（内存五态 + persist WHERE state='queued' + waiter 发送）。
    pub fn resolve(
        &self,
        id: &str,
        action: InterceptResolution,
        now_iso_str: &str,
    ) -> Result<(), QueueError> {
        let ms = now_ms();
        let state = action.state();
        {
            let mut queue = self.queue.lock().expect("queue mutex");
            queue.expire(ms);
            queue.resolve(id, state, ms)?;
        }
        self.deadlines.lock().expect("deadlines mutex").remove(id);
        if let Some(tx) = self.waiters.lock().expect("waiters mutex").remove(id) {
            let _ = tx.send(action.clone());
        }
        if let Some(store) = &self.persist {
            if let Err(e) = store.resolve_intercept(id, state, now_iso_str, Some(action.reason())) {
                tracing_error!(
                    class = "parse_framing",
                    error_code = "persist_resolve_failed",
                    cause = %e,
                    intercept_id = %id,
                    "intercept row persist resolve failed"
                );
            }
        }
        Ok(())
    }

    /// TTL 到期（handler 等待超时面调用）：EXPIRED 终态 + 可见记录；
    /// 请求自动放行（拦截面非安全闸——安全闸是 scope；fail-open 到透传）。
    pub fn on_timeout(&self, id: &str, now_iso_str: &str) {
        let ms = now_ms();
        {
            let mut queue = self.queue.lock().expect("queue mutex");
            queue.expire(ms);
            let _ = queue.resolve(id, InterceptState::Expired, ms);
        }
        self.deadlines.lock().expect("deadlines mutex").remove(id);
        self.waiters.lock().expect("waiters mutex").remove(id);
        if let Some(store) = &self.persist {
            let _ = store.resolve_intercept(
                id,
                InterceptState::Expired,
                now_iso_str,
                Some("ttl_expired"),
            );
        }
        tracing::warn!(
            class = "scope_trace",
            kind = "intercept_expired_autoforward",
            intercept_id = %id,
            "intercept TTL expired; request auto-forwarded with visible record"
        );
    }

    /// 停机面：全部 queued 强制 EXPIRED（无视 TTL 剩余）+ waiter 清空
    /// （PLAN §7.5 :553；deadlines 登记面即全量 queued id 集）。
    pub fn shutdown(&self, now_iso_str: &str) {
        let ms = now_ms();
        let ids: Vec<String> = {
            let mut queue = self.queue.lock().expect("queue mutex");
            queue.expire(ms);
            self.deadlines
                .lock()
                .expect("deadlines mutex")
                .keys()
                .cloned()
                .collect()
        };
        for id in ids {
            let mut queue = self.queue.lock().expect("queue mutex");
            let _ = queue.resolve(&id, InterceptState::Expired, ms);
        }
        self.waiters.lock().expect("waiters mutex").clear();
        self.deadlines.lock().expect("deadlines mutex").clear();
        if let Some(store) = &self.persist {
            if let Err(e) = store.expire_all_queued(now_iso_str, "listener_stopped") {
                tracing_error!(class = "parse_framing", cause = %e, "shutdown expire failed");
            }
        }
    }

    /// 重启对账回载（reconcile_on_start 产出未过期行；沿用原 expires_at）。
    pub fn recover(&self, rows: Vec<InterceptRow>) {
        let ms = now_ms();
        for row in rows {
            let deadline = persist::iso_to_epoch_ms(&row.expires_at).unwrap_or(0);
            let deadline_u = deadline as u64;
            if deadline_u <= ms {
                continue;
            }
            let enqueue_ok = {
                let mut queue = self.queue.lock().expect("queue mutex");
                queue.enqueue(&row.intercept_id, ms).map(|_| ()).is_ok()
            };
            if enqueue_ok {
                self.deadlines
                    .lock()
                    .expect("deadlines mutex")
                    .insert(row.intercept_id.clone(), deadline_u);
            }
        }
    }

    /// 队列 pending 数（状态页）。
    pub fn pending(&self) -> usize {
        self.queue.lock().expect("queue mutex").pending()
    }
}

/// 捕获元数据（tee 完成回调面；pub 但仅经 doc(hidden) 测试助手暴露）。
///
/// WSW-3 扩展：`accum` = exchange 组装器（请求/响应双侧流末 join 落行；
/// W3-D2.1）。method/scheme/authority/path/sequence_no/started_at/
/// scope_decision 定型在 [`ExchangeCapture::new`] snapshot。
#[derive(Clone)]
#[doc(hidden)]
#[allow(missing_debug_implementations)]
pub struct CaptureMeta {
    exchange_id: String,
    direction: &'static str,
    content_type: String,
    /// exchange 组装器（None = 无落库面/测试直调）。
    accum: Option<Arc<ExchangeCapture>>,
}

/// 流式 tee 捕获体（透传路径）：≤cap 缓冲；流末完成 CAS + wsw_body_blob
/// 登记（capture_cas 开启时）；超帽 truncated 不落 CAS（避免半真数据入
/// CAS，仅日志记录——PLAN :482 fidelity 可见性）。WSW-3：流末同时向
/// exchange 组装器登记体产出（join 单行）；Drop 未完成 = 连接中断 →
/// partial 体产出如实登记（W3-D2.1 响应中断落行）。
struct TeeBody {
    inner: Body,
    shared: Arc<ListenerShared>,
    meta: CaptureMeta,
    buf: Vec<u8>,
    cap: usize,
    truncated: bool,
    /// 流末已走 clean-end 面（Drop 兜底不再登记）。
    finished: bool,
    /// 流曾被 poll（空体请求 hyper 可能不 poll 直接弃流——Drop 面区分
    /// 「零字节完整」与「中断截断」的判据）。
    polled: bool,
}

impl futures::Stream for TeeBody {
    type Item = Result<Bytes, hudsucker::Error>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        this.polled = true;
        match Pin::new(&mut this.inner).poll_frame(cx) {
            std::task::Poll::Ready(None) => {
                let meta = this.meta.clone();
                let shared = Arc::clone(&this.shared);
                let buf = std::mem::take(&mut this.buf);
                let truncated = this.truncated;
                let cap = this.cap;
                // 先置 finished 再 spawn：Drop 兜底面不重复登记。
                this.finished = true;
                // 流末注册：小任务做 CAS 写 + 024 blob 行 + exchange 体产出
                // 登记（异步面；W3-D2.1 join 点 = capture_finish 同点）。
                tokio::spawn(async move {
                    let outcome = capture_body_outcome(&shared, &meta, &buf, truncated, cap as u64);
                    if let Some(acc) = &meta.accum {
                        if meta.direction == "request" {
                            acc.register_request_body(outcome);
                        } else {
                            acc.register_response_body(outcome);
                        }
                    }
                });
                std::task::Poll::Ready(None)
            }
            std::task::Poll::Ready(Some(Err(e))) => std::task::Poll::Ready(Some(Err(e))),
            std::task::Poll::Ready(Some(Ok(frame))) => match frame.into_data() {
                Ok(bytes) => {
                    let d: &[u8] = bytes.as_ref();
                    if this.buf.len() + d.len() <= this.cap {
                        this.buf.extend_from_slice(d);
                    } else if this.buf.len() < this.cap {
                        let take = this.cap - this.buf.len();
                        this.buf.extend_from_slice(&d[..take]);
                        this.truncated = true;
                    } else {
                        this.truncated = true;
                    }
                    std::task::Poll::Ready(Some(Ok(bytes)))
                }
                Err(_trailer_frame) => {
                    // trailer 帧不进 data 流（hudsucker from_stream 只承载
                    // data 帧）；waker 即时再轮询下一帧。
                    cx.waker().wake_by_ref();
                    std::task::Poll::Pending
                }
            },
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

impl Drop for TeeBody {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        // 流被弃（未走 clean-end）分两态：
        // - 从未 poll（空体请求 hyper 直弃流）：零字节完整面（fidelity=full，
        //   len=0）——不是中断，不应误标 truncated；
        // - 已 poll 未到流末（连接中断/上游早响应弃流）：部分字节如实登记
        //   ——不落 CAS（W2 超帽不落 CAS 同口径），fidelity=truncated + len
        //   未知；响应侧登记即推行 → partial 行落地（W3-D2.1）。
        let outcome = if !self.polled && self.buf.is_empty() {
            BodyOutcome {
                sha256: None,
                uri: None,
                len: Some(0),
                mime: self.meta.content_type.clone(),
                fidelity: "full",
                snippet: None,
            }
        } else {
            BodyOutcome {
                sha256: None,
                uri: None,
                len: None,
                mime: self.meta.content_type.clone(),
                fidelity: "truncated",
                snippet: None,
            }
        };
        if let Some(acc) = &self.meta.accum {
            if self.meta.direction == "request" {
                acc.register_request_body(outcome);
            } else {
                acc.register_response_body(outcome);
            }
        }
    }
}

/// CAS + 024 blob + BodyOutcome 组装（W3-D2.1 capture_finish 同点 join）。
/// capture_cas=false 或空体/超帽 → 派生列缺失的如实产出（fidelity 标注）。
fn capture_body_outcome(
    shared: &Arc<ListenerShared>,
    meta: &CaptureMeta,
    bytes: &[u8],
    truncated: bool,
    cap: u64,
) -> BodyOutcome {
    let base = BodyOutcome {
        sha256: None,
        uri: None,
        len: if truncated {
            None // 超帽/中断：全量长度未知，如实缺测
        } else {
            Some(bytes.len() as i64)
        },
        mime: meta.content_type.clone(),
        fidelity: if truncated { "truncated" } else { "full" },
        snippet: body_snippet(bytes),
    };
    if truncated || bytes.is_empty() || !shared.options.capture_cas {
        capture_finish(shared, meta, bytes, truncated, cap);
        return base;
    }
    match wsw_message::cas::store_body(&shared.options.artifacts_root, bytes, cap) {
        Ok(body_ref) => {
            if let Some(store) = shared.persist() {
                let row = wsw_message::store::BodyBlobRow {
                    sha256: body_ref.sha256.clone(),
                    uri: body_ref.uri.clone(),
                    size_bytes: body_ref.size as i64,
                    mime_type: meta.content_type.clone(),
                    first_seen_exchange_id: Some(meta.exchange_id.clone()),
                    created_at: persist::now_iso(),
                };
                if let Err(e) =
                    store.with_conn(|conn| wsw_message::store::register_body_blob(conn, &row))
                {
                    tracing_error!(
                        class = "parse_framing",
                        error_code = "blob_register_failed",
                        cause = %e,
                        exchange_id = %meta.exchange_id,
                        "wsw_body_blob register failed"
                    );
                }
            }
            shared
                .counters
                .captures_total
                .fetch_add(1, Ordering::Relaxed);
            tracing::info!(
                class = "capture",
                exchange_id = %meta.exchange_id,
                direction = meta.direction,
                sha256 = %body_ref.sha256,
                size = body_ref.size,
                "body captured (CAS + wsw_body_blob)"
            );
            BodyOutcome {
                sha256: Some(body_ref.sha256),
                uri: Some(body_ref.uri),
                len: Some(body_ref.size as i64),
                ..base
            }
        }
        Err(e) => {
            tracing_error!(
                class = "parse_framing",
                error_code = "cas_store_failed",
                cause = %e,
                exchange_id = %meta.exchange_id,
                "CAS store failed"
            );
            base
        }
    }
}

/// CAS + 024 blob 登记（D9 捕获事实面；capture_cas=false 时零写面）。
fn capture_finish(
    shared: &Arc<ListenerShared>,
    meta: &CaptureMeta,
    bytes: &[u8],
    truncated: bool,
    cap: u64,
) {
    if truncated {
        tracing::warn!(
            class = "parse_framing",
            error_code = "body_cap_exceeded",
            fidelity = "truncated",
            exchange_id = %meta.exchange_id,
            direction = meta.direction,
            "body exceeded capture cap; no CAS row (truncation recorded only)"
        );
        return;
    }
    if bytes.is_empty() || !shared.options.capture_cas {
        return;
    }
    match wsw_message::cas::store_body(&shared.options.artifacts_root, bytes, cap) {
        Ok(body_ref) => {
            if let Some(store) = shared.persist() {
                let row = wsw_message::store::BodyBlobRow {
                    sha256: body_ref.sha256.clone(),
                    uri: body_ref.uri.clone(),
                    size_bytes: body_ref.size as i64,
                    mime_type: meta.content_type.clone(),
                    first_seen_exchange_id: Some(meta.exchange_id.clone()),
                    created_at: persist::now_iso(),
                };
                if let Err(e) =
                    store.with_conn(|conn| wsw_message::store::register_body_blob(conn, &row))
                {
                    tracing_error!(
                        class = "parse_framing",
                        error_code = "blob_register_failed",
                        cause = %e,
                        exchange_id = %meta.exchange_id,
                        "wsw_body_blob register failed"
                    );
                }
            }
            shared
                .counters
                .captures_total
                .fetch_add(1, Ordering::Relaxed);
            tracing::info!(
                class = "capture",
                exchange_id = %meta.exchange_id,
                direction = meta.direction,
                sha256 = %body_ref.sha256,
                size = body_ref.size,
                "body captured (CAS + wsw_body_blob)"
            );
        }
        Err(e) => {
            tracing_error!(
                class = "parse_framing",
                error_code = "cas_store_failed",
                cause = %e,
                exchange_id = %meta.exchange_id,
                "CAS store failed"
            );
        }
    }
}

/// 目标提取（查点 A 入参）：绝对形式用 URI authority，否则 Host 头；
/// scheme 取 URI（无 = http，MITM 后明文段）。
fn extract_target(req: &Request<Body>) -> (String, String, u16, String) {
    let method = req.method().as_str().to_string();
    let uri = req.uri().clone();
    let scheme = uri.scheme_str().unwrap_or("http").to_string();
    let authority = uri
        .host()
        .map(|h| h.to_string())
        .or_else(|| {
            req.headers()
                .get("host")
                .and_then(|v| v.to_str().ok())
                .map(|h| h.to_string())
        })
        .unwrap_or_default();
    let (host, port) = split_authority(&authority, uri.port_u16(), &scheme);
    let path = if uri.path().is_empty() {
        "/"
    } else {
        uri.path()
    };
    (method, host, port, path.to_string())
}

/// authority → (host, port)：host 去括号；端口缺省按 scheme（http=80/https=443）。
fn split_authority(authority: &str, explicit: Option<u16>, scheme: &str) -> (String, u16) {
    let default_port = if scheme == "https" { 443 } else { 80 };
    let host_port = match authority.rsplit_once('@') {
        Some((_, h)) => h,
        None => authority,
    };
    if let Some(stripped) = host_port.strip_prefix('[') {
        if let Some((inner, rest)) = stripped.split_once(']') {
            let port = rest
                .strip_prefix(':')
                .and_then(|p| p.parse::<u16>().ok())
                .or(explicit)
                .unwrap_or(default_port);
            return (inner.to_string(), port);
        }
    }
    match host_port.rsplit_once(':') {
        Some((h, p)) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) => {
            (h.to_string(), p.parse::<u16>().unwrap_or(default_port))
        }
        _ => (host_port.to_string(), explicit.unwrap_or(default_port)),
    }
}

/// 拒绝响应构造（scope_blocked 403 / parse_framing 400 / 上游错误由
/// handle_error 面）——响应体仅固定短串，不回显头/体细节。
fn reject_response(status: StatusCode, decision: &str, reason: &str) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("x-wsw-scope-decision", decision)
        .header("x-wsw-reason", reason)
        .body(Body::from(format!("WSW-2: {decision} ({reason})")))
        .expect("reject response")
}

/// 测试面：捕获元数据构造（#[doc(hidden)]；tests/mitm_loopback.rs 消费）。
#[doc(hidden)]
pub fn __test_capture_meta(exchange_id: &str) -> CaptureMeta {
    CaptureMeta {
        exchange_id: exchange_id.to_string(),
        direction: "request",
        content_type: "application/octet-stream".to_string(),
        accum: None,
    }
}

/// 测试面：capture_finish 直调（#[doc(hidden)]）。
#[doc(hidden)]
pub fn __test_capture_finish(
    shared: &Arc<ListenerShared>,
    meta: &CaptureMeta,
    bytes: &[u8],
    truncated: bool,
    cap: u64,
) {
    capture_finish(shared, meta, bytes, truncated, cap);
}

/// WSW 处理器（Clone per-connection；`current` 承载请求→响应同连接关联）。
#[derive(Clone)]
pub struct WswHandler {
    shared: Arc<ListenerShared>,
    /// 当前连接的最近请求目标（响应方向捕获/关联用）。
    current: Option<Arc<CurrentExchange>>,
    /// WS 帧捕获状态（WSW-5 D3；Arc 跨 Clone 共享——hudsucker 每连接克隆
    /// handler，双侧泵各持一份克隆，连接登记面必须共享）。
    ws_state: Arc<WsCaptureState>,
}

/// WS 连接捕获登记（endpoint+client_addr 键 → 连接组装面；握手行懒建——
/// hudsucker WebSocketHandler 无连接开启钩子，首帧到达 = 连接面事实起点，
/// D3 挂点单点纪律）。
struct WsCaptureState {
    conns: Mutex<BTreeMap<String, Arc<WsConnCapture>>>,
}

/// 单 WS 连接的捕获面（exchange_id 一次定型 + 帧序号单调）。
struct WsConnCapture {
    /// 握手行 exchange_id（protocol='ws'）。
    exchange_id: String,
    /// 帧序号（连接内单调，UNIQUE(connection_exchange_id, sequence_no) 锚）。
    next_seq: AtomicU64,
    /// 连接 scope 三态（握手时刻查点 A 投影，帧行同值落列）。
    scope_decision: Option<&'static str>,
}

#[derive(Clone)]
struct CurrentExchange {
    exchange_id: String,
    host: String,
    path: String,
    content_type: String,
    /// exchange 组装器（WSW-3 单行纪律 join 面）。
    accum: Option<Arc<ExchangeCapture>>,
}

impl WswHandler {
    /// 构造。
    pub fn new(shared: Arc<ListenerShared>) -> Self {
        Self {
            shared,
            current: None,
            ws_state: Arc::new(WsCaptureState {
                conns: Mutex::new(BTreeMap::new()),
            }),
        }
    }

    /// 共享面视图（测试/对账 API 消费 coordinator）。
    pub fn shared(&self) -> &Arc<ListenerShared> {
        &self.shared
    }
}

impl HttpHandler for WswHandler {
    /// 全量 MITM：盲隧道（should_intercept_connect=false 路径）会绕过 scope
    /// 闸与查点 C IP 审计（hudsucker internal.rs:310 直连 TcpStream）——
    /// 一律拦截使每个 CONNECT 后的解密请求都过闸（安全默认；WS 帧仍经
    /// WebSocketHandler 透传挂载）。
    async fn should_intercept_connect(&mut self, _ctx: &HttpContext, req: &Request<Body>) -> bool {
        // 日志只落归一 host:port（split_authority 剥 userinfo——absolute-form
        // `user:pass@host:443` 形态不把凭据落日志，任务书 §五.12 零例外口径；
        // scope 闸侧 extract_target 本已剥，此为日志面对齐）。
        let uri = req.uri();
        let (host, port) = split_authority(
            uri.authority().map(|a| a.as_str()).unwrap_or(""),
            uri.port_u16(),
            uri.scheme_str().unwrap_or("https"),
        );
        tracing::info!(
            class = "scope_trace",
            kind = "connect",
            authority = format!("{host}:{port}"),
            "CONNECT intercepted (full-MITM posture; blind tunnel would bypass scope gate)"
        );
        true
    }

    async fn handle_request(&mut self, ctx: &HttpContext, req: Request<Body>) -> RequestOrResponse {
        // 0) accept 源二次校验（PLAN §7.1 :512-514；fail-closed 403）。
        if !is_loopback_peer(ctx.client_addr.ip()) {
            self.shared
                .counters
                .scope_blocked_total
                .fetch_add(1, Ordering::Relaxed);
            self.shared
                .counters
                .bump_error(ProxyErrorClass::ScopeBlocked);
            tracing_error!(
                class = "scope_blocked",
                point = "accept_source",
                remote_peer = %ctx.client_addr,
                "non-loopback peer rejected at request entry (accept-source second check)"
            );
            return reject_response(
                StatusCode::FORBIDDEN,
                "out_of_scope_blocked",
                "non_loopback_peer",
            )
            .into();
        }
        self.shared
            .counters
            .requests_total
            .fetch_add(1, Ordering::Relaxed);

        // 1) 目标提取 + 查点 A（连接前闸；重定向第二跳同样走这里）。
        let (method, host, port, path) = extract_target(&req);
        let target = ScopeTarget {
            scheme: "http",
            host: &host,
            port,
            path: &path,
        };
        let mut scope_decision: Option<&'static str> = Some("in_scope");
        match scope::check(&self.shared.rules, &target) {
            scope::ScopeDecision::Allowed { traces } => {
                for t in &traces {
                    tracing::info!(
                        class = "scope_trace",
                        kind = t.kind,
                        detail = %t.detail,
                        host = %host,
                        "request passed scope gate (trace line)"
                    );
                }
            }
            scope::ScopeDecision::Blocked { reason } => {
                self.shared
                    .counters
                    .bump_error(ProxyErrorClass::ScopeBlocked);
                match self.shared.policy {
                    OutOfScopePolicy::Blocked => {
                        self.shared
                            .counters
                            .scope_blocked_total
                            .fetch_add(1, Ordering::Relaxed);
                        tracing_error!(
                            class = "scope_blocked",
                            point = "pre_connect",
                            scope_decision = "out_of_scope_blocked",
                            reason = %reason.code(),
                            host = %host,
                            port,
                            path = %path,
                            "request rejected by scope gate (403)"
                        );
                        return reject_response(
                            StatusCode::FORBIDDEN,
                            "out_of_scope_blocked",
                            reason.code(),
                        )
                        .into();
                    }
                    OutOfScopePolicy::PassiveForward => {
                        self.shared
                            .counters
                            .out_of_scope_passive_total
                            .fetch_add(1, Ordering::Relaxed);
                        scope_decision = Some("out_of_scope_passive");
                        tracing::warn!(
                            class = "scope_trace",
                            kind = "out_of_scope_passive",
                            reason = %reason.code(),
                            host = %host,
                            "out-of-scope request passed through (passive_forward policy, no rewrite)"
                        );
                    }
                }
            }
        }

        // 2) 拦截判定（默认关；命中 = host/method/path 匹配器）。
        let intercepting = self.shared.options.intercept_enabled
            && self
                .shared
                .options
                .intercept_matcher
                .matches(&host, &method, &path);
        let exchange_id = new_exchange_id();
        let content_type = req
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        // WSW-3 请求入口 snapshot（W3-D2.1）：exchange 行固定列一次定型；
        // sequence_no 单调分配；无落库通道时组装器缺省（capture 行为零开销跳过）。
        let accum = if self.shared.exchange_tx.is_some() {
            let seq = self.shared.persist_sequence.fetch_add(1, Ordering::Relaxed) as i64;
            let started_ms = now_ms();
            let scheme = req.uri().scheme_str().unwrap_or("http").to_string();
            let http_version = match req.version() {
                hudsucker::hyper::Version::HTTP_10 => "http/1.0",
                hudsucker::hyper::Version::HTTP_11 => "http/1.1",
                hudsucker::hyper::Version::HTTP_2 => "h2",
                _ => "http/1.1",
            };
            let capture = ExchangeCapture::new(
                &self.shared,
                exchange_id.clone(),
                seq,
                method.clone(),
                scheme,
                format!("{host}:{port}"),
                path.clone(),
                http_version.to_string(),
                scope_decision,
                started_ms,
            );
            capture.register_request_headers(req.headers());
            Some(capture)
        } else {
            None
        };
        self.current = Some(Arc::new(CurrentExchange {
            exchange_id: exchange_id.clone(),
            host: host.clone(),
            path: path.clone(),
            content_type: content_type.clone(),
            accum: accum.clone(),
        }));

        if intercepting {
            return self
                .handle_request_intercepted(req, exchange_id, content_type, accum)
                .await;
        }

        // 3) 透传路径：流式 tee 捕获（≤cap 缓冲，超帽 truncated 不落 CAS）。
        let (mut parts, body) = req.into_parts();
        if self.shared.options.capture_cas {
            let meta = CaptureMeta {
                exchange_id,
                direction: "request",
                content_type,
                accum,
            };
            let tee = TeeBody {
                inner: body,
                shared: Arc::clone(&self.shared),
                meta,
                buf: Vec::new(),
                cap: self.shared.options.config.max_body_bytes,
                truncated: false,
                finished: false,
                polled: false,
            };
            // 体流替换后原 Content-Length 不再可靠：移除，由 hyper 重定界。
            parts.headers.remove(CONTENT_LENGTH);
            return Request::from_parts(parts, Body::from_stream(tee)).into();
        }
        Request::from_parts(parts, body).into()
    }

    async fn handle_response(&mut self, _ctx: &HttpContext, res: Response<Body>) -> Response<Body> {
        self.shared
            .counters
            .responses_total
            .fetch_add(1, Ordering::Relaxed);
        self.shared.note_success();
        let (mut parts, body) = res.into_parts();
        if !self.shared.options.capture_cas {
            return Response::from_parts(parts, body);
        }
        let (exchange_id, host, content_type, accum) = match &self.current {
            Some(cur) => (
                cur.exchange_id.clone(),
                cur.host.clone(),
                parts
                    .headers
                    .get(CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or(&cur.content_type)
                    .to_string(),
                cur.accum.clone(),
            ),
            None => (
                new_exchange_id(),
                "-".to_string(),
                "application/octet-stream".to_string(),
                None,
            ),
        };
        let status = parts.status.as_u16();
        // 响应头登记（W3-D2.1：TTFB 时刻 + status + 头投影立即定型）。
        if let Some(acc) = &accum {
            acc.register_response_head(status, &parts.headers);
        }
        tracing::info!(
            class = "capture",
            exchange_id = %exchange_id,
            host = %host,
            status,
            "response flowing through MITM (tee capture armed)"
        );
        let meta = CaptureMeta {
            exchange_id,
            direction: "response",
            content_type,
            accum,
        };
        let tee = TeeBody {
            inner: body,
            shared: Arc::clone(&self.shared),
            meta,
            buf: Vec::new(),
            cap: self.shared.options.config.max_body_bytes,
            truncated: false,
            finished: false,
            polled: false,
        };
        parts.headers.remove(CONTENT_LENGTH);
        Response::from_parts(parts, Body::from_stream(tee))
    }

    async fn handle_error(
        &mut self,
        _ctx: &HttpContext,
        err: hudsucker::hyper_util::client::legacy::Error,
    ) -> Response<Body> {
        // 下溯错误链找查点 C 拒绝记录（ScopeRejectRecord downcast → 403；
        // 决策实现 = [`respond_to_scope_reject`]，集成测试喂同链形错误走同一路径）。
        if let Some(resp) = respond_to_scope_reject(&self.shared, &err) {
            return resp;
        }
        let mut is_tls_error = false;
        {
            let mut src: Option<&(dyn StdError + 'static)> = Some(&err);
            while let Some(e) = src {
                let msg = e.to_string().to_ascii_lowercase();
                if msg.contains("certificate")
                    || msg.contains("tls")
                    || msg.contains("alert")
                    || msg.contains("invalid peer")
                {
                    is_tls_error = true;
                }
                src = e.source();
            }
        }
        let class = if is_tls_error {
            ProxyErrorClass::UpstreamTls
        } else {
            ProxyErrorClass::UpstreamConnect
        };
        self.shared.counters.bump_error(class);
        tracing_error!(
            class = class.as_str(),
            error_code = "forward_failed",
            cause_chain = %cause_chain_summary(&err),
            "upstream forward failed (502)"
        );
        Response::builder()
            .status(StatusCode::BAD_GATEWAY)
            .body(Body::from("WSW-2: upstream error"))
            .expect("502 body")
    }
}

impl WswHandler {
    /// 拦截路径：聚合读（帽值）→ CAS 捕获 → 入队 → 等裁决 → 三动作。
    async fn handle_request_intercepted(
        &mut self,
        req: Request<Body>,
        exchange_id: String,
        content_type: String,
        accum: Option<Arc<ExchangeCapture>>,
    ) -> RequestOrResponse {
        let cap = self.shared.options.config.max_body_bytes;
        // CL 前检（头帽/体帽 → parse_framing 400）。
        if let Some(cl) = req
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
        {
            if let Ok(n) = cl.parse::<u64>() {
                if n > cap as u64 {
                    self.shared
                        .counters
                        .bump_error(ProxyErrorClass::ParseFraming);
                    tracing_error!(
                        class = "parse_framing",
                        error_code = "body_cap_exceeded",
                        declared = n,
                        cap,
                        "intercepted request body exceeds cap (400)"
                    );
                    return reject_response(
                        StatusCode::BAD_REQUEST,
                        "parse_framing",
                        "body_cap_exceeded",
                    )
                    .into();
                }
            }
        }
        let (mut parts, body) = req.into_parts();
        let bytes = match body.collect().await {
            Ok(c) => {
                let b: Bytes = c.to_bytes();
                if b.len() > cap {
                    self.shared
                        .counters
                        .bump_error(ProxyErrorClass::ParseFraming);
                    tracing_error!(
                        class = "parse_framing",
                        error_code = "body_cap_exceeded",
                        actual = b.len(),
                        cap,
                        "intercepted request body exceeds cap (400)"
                    );
                    return reject_response(
                        StatusCode::BAD_REQUEST,
                        "parse_framing",
                        "body_cap_exceeded",
                    )
                    .into();
                }
                b.to_vec()
            }
            Err(e) => {
                self.shared
                    .counters
                    .bump_error(ProxyErrorClass::ParseFraming);
                tracing_error!(
                    class = "parse_framing",
                    error_code = "body_collect_failed",
                    cause = %e,
                    "intercepted request body collect failed (400)"
                );
                return reject_response(
                    StatusCode::BAD_REQUEST,
                    "parse_framing",
                    "body_collect_failed",
                )
                .into();
            }
        };

        // 脱敏日志投影：redact 命中的头名列表（值不落日志）。
        let outcome = wsw_message::parse::parse_request(&bytes);
        if let Some(msg) = &outcome.message {
            let redacted =
                wsw_message::redact::redact(msg, &wsw_message::redact::RedactRules::default());
            tracing::info!(
                class = "intercept",
                exchange_id = %exchange_id,
                redacted_header_names = redacted.redacted_header_names.join(","),
                redacted_count = redacted.redacted_header_names.len(),
                body_len = bytes.len(),
                "intercepted request parsed (redacted projection; header values never logged)"
            );
        }

        // CAS 捕获（拦截路径聚合字节完整落 CAS）+ exchange 请求侧登记
        // （聚合面字节已知：headers/body 立即定型；响应侧走 handle_response tee）。
        let meta = CaptureMeta {
            exchange_id: exchange_id.clone(),
            direction: "request",
            content_type,
            accum: accum.clone(),
        };
        let outcome = capture_body_outcome(&self.shared, &meta, &bytes, false, cap as u64);
        if let Some(acc) = &accum {
            acc.register_request_body(outcome);
        }

        // 入队（内存 + persist 双面）。
        let (method, host, path) = (
            parts.method.as_str().to_string(),
            self.current
                .as_ref()
                .map(|c| c.host.clone())
                .unwrap_or_default(),
            self.current
                .as_ref()
                .map(|c| c.path.clone())
                .unwrap_or_default(),
        );
        let enqueue = self.shared.coordinator.enqueue(
            &host,
            &method,
            &path,
            &exchange_id,
            &self.shared.options.listener_id,
            &self.shared.options.session_id,
        );
        let (intercept_id, rx) = match enqueue {
            Ok(pair) => {
                self.shared
                    .counters
                    .intercept_total
                    .fetch_add(1, Ordering::Relaxed);
                tracing::info!(
                    class = "intercept",
                    exchange_id = %exchange_id,
                    intercept_id = %pair.0,
                    host = %host,
                    method = %method,
                    path = %path,
                    "request queued for interception"
                );
                pair
            }
            Err(QueueError::Full) => {
                // 队满 → EXPIRED + 可见记录 + 自动放行（PLAN :536）。
                tracing::warn!(
                    class = "scope_trace",
                    kind = "intercept_queue_full_autoforward",
                    exchange_id = %exchange_id,
                    "intercept queue full; request auto-forwarded with visible record"
                );
                return Request::from_parts(parts, Body::from(bytes)).into();
            }
            Err(e) => {
                tracing_error!(
                    class = "parse_framing",
                    error_code = "intercept_enqueue_failed",
                    cause = ?e,
                    "intercept enqueue failed; auto-forward"
                );
                return Request::from_parts(parts, Body::from(bytes)).into();
            }
        };

        // TTL 等待（ListenerConfig.queue_ttl_seconds）。
        let ttl = std::time::Duration::from_millis(self.shared.coordinator.ttl_ms());
        let resolution = match tokio::time::timeout(ttl, rx).await {
            Ok(Ok(res)) => Some(res),
            Ok(Err(_dropped)) => None, // waiter 侧异常消失：按过期收口
            Err(_elapsed) => {
                self.shared
                    .coordinator
                    .on_timeout(&intercept_id, &persist::now_iso());
                None
            }
        };

        match resolution {
            Some(InterceptResolution::Forward) => {
                let _ = self.shared.coordinator.resolve(
                    &intercept_id,
                    InterceptResolution::Forward,
                    &persist::now_iso(),
                );
                Request::from_parts(parts, Body::from(bytes)).into()
            }
            Some(InterceptResolution::Drop) => {
                // Drop 已在 resolve 侧落账；对客户端 503 + reason 头。
                // 本地响应不经 handle_response：partial 行立即落（W3-D2.1）。
                if let Some(acc) = &accum {
                    acc.mark_no_response();
                }
                reject_response(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "intercept_dropped",
                    "dropped_by_intercept",
                )
                .into()
            }
            Some(InterceptResolution::ModifiedForward(edited)) => {
                // 查点 A 重跑（W2-D6 Modified-Forward Scope 二次校验）。
                let (host2, port2, path2) = (host.clone(), port_of(&parts), path.clone());
                let target = ScopeTarget {
                    scheme: "http",
                    host: &host2,
                    port: port2,
                    path: &path2,
                };
                if let scope::ScopeDecision::Blocked { reason } =
                    scope::check(&self.shared.rules, &target)
                {
                    self.shared
                        .counters
                        .scope_blocked_total
                        .fetch_add(1, Ordering::Relaxed);
                    self.shared
                        .counters
                        .bump_error(ProxyErrorClass::ScopeBlocked);
                    tracing_error!(
                        class = "scope_blocked",
                        point = "modified_forward",
                        reason = %reason.code(),
                        "modified-forward failed scope re-check (403)"
                    );
                    // 本地 403 不经 handle_response：partial 行立即落。
                    if let Some(acc) = &accum {
                        acc.mark_no_response();
                    }
                    return reject_response(
                        StatusCode::FORBIDDEN,
                        "out_of_scope_blocked",
                        reason.code(),
                    )
                    .into();
                }
                parts.headers.remove(CONTENT_LENGTH);
                Request::from_parts(parts, Body::from(edited)).into()
            }
            None => {
                // TTL 到期/等待端消失：自动放行（EXPIRED 终态已落账）。
                Request::from_parts(parts, Body::from(bytes)).into()
            }
        }
    }
}

/// parts → 端口（Modified-Forward 二次校验用）。
fn port_of(parts: &hudsucker::hyper::http::request::Parts) -> u16 {
    let uri = &parts.uri;
    uri.port_u16().unwrap_or_else(|| {
        uri.host()
            .map(|h| {
                let _ = h;
                if uri.scheme_str() == Some("https") {
                    443
                } else {
                    80
                }
            })
            .unwrap_or(80)
    })
}

impl WebSocketHandler for WswHandler {
    /// WS 帧结构化捕获 + 透传挂载（WSW-5 D3：opcode/payload/masked 提取 →
    /// CAS payload → 030 帧行通道投递；Some 透传语义不变——POC 先例
    /// main.rs:131-140 同型，捕获在透传路径零旁路零第二 WS 栈）。
    async fn handle_message(&mut self, ctx: &WebSocketContext, msg: Message) -> Option<Message> {
        let (direction, endpoint, client_addr) = match ctx {
            WebSocketContext::ClientToServer { src, dst, .. } => {
                ("client_to_server", dst.clone(), *src)
            }
            WebSocketContext::ServerToClient { src, dst, .. } => {
                ("server_to_client", src.clone(), *dst)
            }
        };
        let Some(opcode) = crate::wsframe::opcode_of(&msg) else {
            // Frame 变体不透出（读取面永不产出）——防御面直接透传。
            return Some(msg);
        };
        let conn = self.ws_conn_entry(&endpoint, client_addr);
        let sequence_no = conn.next_seq.fetch_add(1, Ordering::Relaxed) as i64;
        let payload = crate::wsframe::payload_of(&msg);
        let captured_at = persist::now_iso();
        let scope_decision = conn.scope_decision;
        let exchange_id = conn.exchange_id.clone();

        // CAS payload（W3-D2 同型：text→'text/plain'、binary/控制帧→
        // 'application/octet-stream'；空帧/超帽 fidelity 词形如实标注）。
        let (payload_sha, payload_uri, payload_len_col, payload_fidelity) =
            self.store_ws_payload(&exchange_id, opcode, &payload, captured_at.as_str());

        // 030 帧行投递（有界通道；满载丢行计数可见，热路径零阻塞）。
        let row = persist::WsFrameRow {
            frame_id: persist::generate_frame_id(),
            connection_exchange_id: exchange_id.clone(),
            direction,
            sequence_no,
            opcode: opcode.as_str(),
            payload_len: payload.len() as i64,
            payload_body_sha256: payload_sha,
            payload_body_uri: payload_uri,
            payload_body_len: payload_len_col,
            payload_body_fidelity: payload_fidelity,
            masked: crate::wsframe::masked_by_direction(direction),
            captured_at,
            scope_decision,
        };
        self.shared.push_ws_frame_row(row);

        tracing::info!(
            class = "capture",
            ws_endpoint = %endpoint,
            ws_frame = %msg.len(),
            ws_opcode = opcode.as_str(),
            ws_exchange = %exchange_id,
            ws_sequence = sequence_no,
            "ws frame passthrough (captured; workbench W5)"
        );
        Some(msg)
    }
}

impl WswHandler {
    /// WS 连接登记懒建（endpoint+client_addr 键；首帧 = 连接面事实起点）。
    /// 新键：分配 exchange_id + sequence 基线 + 同步落握手行（protocol='ws'，
    /// 030 帧行 FK 引用前置——握手行先于帧行在库，帧 worker 无乱序窗口）。
    fn ws_conn_entry(
        &self,
        endpoint: &hudsucker::hyper::Uri,
        client_addr: std::net::SocketAddr,
    ) -> Arc<WsConnCapture> {
        let key = format!("{client_addr}|{endpoint}");
        let mut conns = self.ws_state.conns.lock().expect("ws capture mutex");
        if let Some(existing) = conns.get(&key) {
            return Arc::clone(existing);
        }
        let exchange_id = new_exchange_id();
        let scope_decision = self.ws_handshake_scope(endpoint);
        let capture = Arc::new(WsConnCapture {
            exchange_id: exchange_id.clone(),
            next_seq: AtomicU64::new(0),
            scope_decision,
        });
        self.push_ws_handshake_row(&exchange_id, endpoint, scope_decision);
        conns.insert(key, Arc::clone(&capture));
        capture
    }

    /// WS 连接 scope 三态投影（030/wsw_exchange scope_decision 同列语义，
    /// 判据 8：ws/wss scheme 映射 http/https 复走既有查点 A 词形——scope.rs
    /// 零 diff，纯调用面）。
    fn ws_handshake_scope(&self, endpoint: &hudsucker::hyper::Uri) -> Option<&'static str> {
        let host = endpoint.host().unwrap_or("");
        let port = endpoint.port_u16().unwrap_or(match endpoint.scheme_str() {
            Some("wss") => 443,
            _ => 80,
        });
        let path = endpoint.path();
        let mapped_scheme = match endpoint.scheme_str() {
            Some("wss") => "https",
            _ => "http",
        };
        let target = ScopeTarget {
            scheme: mapped_scheme,
            host,
            port,
            path,
        };
        match scope::check(&self.shared.rules, &target) {
            scope::ScopeDecision::Allowed { .. } => Some("in_scope"),
            scope::ScopeDecision::Blocked { .. } => Some("out_of_scope_blocked"),
        }
    }

    /// WS 握手行落库（025 面复用 insert_exchanges；同步单行——030 帧行 FK
    /// 前置；无落库通道/纯内存面跳过）。行形状：protocol='ws' +
    /// source='proxy_capture'（025:54 词形直用）+ capture_fidelity=
    /// degraded_no_wire（握手头区不经本挂点透出——W2-D9 实态如实落库）。
    fn push_ws_handshake_row(
        &self,
        exchange_id: &str,
        endpoint: &hudsucker::hyper::Uri,
        scope_decision: Option<&'static str>,
    ) {
        if self.shared.exchange_tx.is_none() {
            return;
        }
        let seq = self.shared.persist_sequence.fetch_add(1, Ordering::Relaxed) as i64;
        let now = persist::now_iso();
        let authority = endpoint
            .authority()
            .map(|a| a.as_str().to_string())
            .unwrap_or_default();
        let row = ExchangeRow {
            exchange_id: exchange_id.to_string(),
            session_id: self.shared.options.session_id.clone(),
            listener_id: self.shared.options.listener_id.clone(),
            sequence_no: seq,
            direction: "client_to_server",
            source: "proxy_capture",
            protocol: "ws",
            http_version: None,
            tls_version: None,
            alpn: None,
            // RFC6455 §4.1 握手定义性为 GET（协议事实非观测伪造）。
            method: Some("GET".to_string()),
            scheme: endpoint.scheme_str().map(|s| match s {
                "wss" => "wss".to_string(),
                _ => "ws".to_string(),
            }),
            authority: Some(authority),
            path: Some(endpoint.path().to_string()),
            response_status: None,
            request_headers_json: None,
            response_headers_json: None,
            request_body_sha256: None,
            request_body_uri: None,
            request_body_len: None,
            request_mime: None,
            request_body_fidelity: None,
            response_body_sha256: None,
            response_body_uri: None,
            response_body_len: None,
            response_mime: None,
            response_body_fidelity: None,
            started_at: Some(now.clone()),
            completed_at: Some(now),
            duration_ms: None,
            ttfb_ms: None,
            remote_ip: None,
            scope_decision,
            edited: false,
            tags_json: "[]".to_string(),
            notes: None,
            trace_id: None,
            task_id: None,
            tool_run_id: None,
            artifact_id: None,
            evidence_id: None,
            capture_fidelity: "degraded_no_wire",
            parse_status: "ok",
            error_code: None,
            search_text: None,
        };
        self.shared
            .counters
            .ws_connections_total
            .fetch_add(1, Ordering::Relaxed);
        // 握手行同步单行落库（不走通道——030 帧行 FK 引用 wsw_exchange 行
        // 前置：Pool foreign_keys=ON，帧行 worker 与 exchange worker 双通道
        // 无顺序保证，握手行必须先于首帧行在库；fsync 成本 = 每连接一次，
        // 非帧热路径）。失败仅计数——透传行为不变。
        if let Some(store) = self.shared.persist() {
            if let Err(e) = store.insert_exchanges(&[row]) {
                self.shared
                    .counters
                    .ws_connections_total
                    .fetch_sub(1, Ordering::Relaxed);
                tracing_error!(
                    class = "parse_framing",
                    error_code = "ws_handshake_row_insert_failed",
                    cause = %e,
                    exchange_id = %exchange_id,
                    "ws handshake row insert failed (frames for this connection will be dropped by FK)"
                );
            }
        } else {
            // 无 persist 面：握手行无落库点（纯内存模式帧行通道也不存在）。
            self.shared
                .counters
                .ws_connections_total
                .fetch_sub(1, Ordering::Relaxed);
        }
    }

    /// WS 帧 payload CAS 落账（store_body 023 链 + wsw_body_blob 行；空帧 →
    /// NULL 派生列 + len=0 如实在行；capture_cas=false → NULL 派生列）。
    fn store_ws_payload(
        &self,
        exchange_id: &str,
        opcode: crate::wsframe::WsOpcode,
        payload: &[u8],
        _captured_at: &str,
    ) -> (
        Option<String>,
        Option<String>,
        Option<i64>,
        Option<&'static str>,
    ) {
        if payload.is_empty() || !self.shared.options.capture_cas {
            return (
                None,
                None,
                if payload.is_empty() { Some(0) } else { None },
                if payload.is_empty() {
                    Some("full")
                } else {
                    None
                },
            );
        }
        let cap = self.shared.options.config.max_body_bytes as u64;
        let mime = crate::wsframe::payload_mime(opcode);
        match wsw_message::cas::store_body(&self.shared.options.artifacts_root, payload, cap) {
            Ok(body_ref) => {
                let fidelity = match body_ref.fidelity {
                    wsw_message::types::CaptureFidelity::Truncated => "truncated",
                    _ => "full",
                };
                if let Some(store) = self.shared.persist() {
                    let row = wsw_message::store::BodyBlobRow {
                        sha256: body_ref.sha256.clone(),
                        uri: body_ref.uri.clone(),
                        size_bytes: body_ref.size as i64,
                        mime_type: mime.to_string(),
                        first_seen_exchange_id: Some(exchange_id.to_string()),
                        created_at: persist::now_iso(),
                    };
                    if let Err(e) =
                        store.with_conn(|conn| wsw_message::store::register_body_blob(conn, &row))
                    {
                        tracing_error!(
                            class = "parse_framing",
                            error_code = "blob_register_failed",
                            cause = %e,
                            exchange_id = %exchange_id,
                            "wsw_body_blob register failed (ws frame payload)"
                        );
                    }
                }
                self.shared
                    .counters
                    .captures_total
                    .fetch_add(1, Ordering::Relaxed);
                (
                    Some(body_ref.sha256),
                    Some(body_ref.uri),
                    Some(body_ref.size as i64),
                    Some(fidelity),
                )
            }
            Err(e) => {
                tracing_error!(
                    class = "parse_framing",
                    error_code = "cas_store_failed",
                    cause = %e,
                    exchange_id = %exchange_id,
                    "ws frame payload CAS store failed"
                );
                (None, None, Some(payload.len() as i64), None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scope::ScopeRules;

    fn shared(rules: ScopeRules) -> Arc<ListenerShared> {
        Arc::new(ListenerShared::new(
            RuntimeOptions {
                scope_rules: rules,
                ..Default::default()
            },
            None,
        ))
    }

    #[test]
    fn matcher_semantics() {
        let m = InterceptMatcher {
            hosts: vec!["127.0.0.1".to_string()],
            methods: vec!["GET".to_string()],
            path_prefixes: vec!["/p59c".to_string()],
        };
        assert!(m.matches("127.0.0.1", "GET", "/p59c-marker"));
        assert!(!m.matches("127.0.0.1", "POST", "/p59c-marker"));
        assert!(!m.matches("example.com", "GET", "/p59c-marker"));
        assert!(!m.matches("127.0.0.1", "GET", "/other"));
        // 空表 = 全匹配（缺省拦截所有面）。
        assert!(InterceptMatcher::default().matches("a", "b", "c"));
    }

    #[test]
    fn extract_target_forms() {
        // 绝对形式（明文代理）。
        let req = Request::builder()
            .uri("http://127.0.0.1:8443/p59c-marker")
            .body(Body::empty())
            .unwrap();
        let (method, host, port, path) = extract_target(&req);
        assert_eq!(method, "GET");
        assert_eq!(host, "127.0.0.1");
        assert_eq!(port, 8443);
        assert_eq!(path, "/p59c-marker");
        // origin-form + Host 头（CONNECT 后明文段）。
        let req = Request::builder()
            .uri("/index")
            .header("host", "local-range.test:8443")
            .body(Body::empty())
            .unwrap();
        let (_, host, port, path) = extract_target(&req);
        assert_eq!(host, "local-range.test");
        assert_eq!(port, 8443);
        assert_eq!(path, "/index");
    }

    #[test]
    fn cause_chain_summary_shape() {
        #[derive(Debug)]
        struct Root;
        impl fmt::Display for Root {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "root-cause")
            }
        }
        impl StdError for Root {}
        #[derive(Debug)]
        struct Wrap(Root);
        impl fmt::Display for Wrap {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "wrapper")
            }
        }
        impl StdError for Wrap {
            fn source(&self) -> Option<&(dyn StdError + 'static)> {
                Some(&self.0)
            }
        }
        assert_eq!(cause_chain_summary(&Wrap(Root)), "wrapper <- root-cause");
    }

    #[test]
    fn coordinator_enqueue_resolve_and_full() {
        let s = shared(ScopeRules::default());
        let coordinator = &s.coordinator;
        // 容量 1（RuntimeOptions 默认 64 → 用小容量单测 coordinator 直构）。
        let tiny = InterceptCoordinator::new(1, 500, None);
        let (id1, rx1) = tiny
            .enqueue(
                "127.0.0.1",
                "GET",
                "/a",
                "wsw-1-0-0001",
                "wsw-lis-t",
                "wsw-ses-t",
            )
            .unwrap();
        assert!(tiny.pending() == 1);
        // 第二条 = 队满。
        assert!(matches!(
            tiny.enqueue(
                "127.0.0.1",
                "GET",
                "/b",
                "wsw-1-0-0002",
                "wsw-lis-t",
                "wsw-ses-t"
            ),
            Err(QueueError::Full)
        ));
        // 裁决 Drop → waiter 收到 + pending 清零。
        tiny.resolve(&id1, InterceptResolution::Drop, &persist::now_iso())
            .unwrap();
        assert_eq!(rx1.await_blocked(), InterceptResolution::Drop);
        assert_eq!(tiny.pending(), 0);
        let _ = coordinator;
    }

    /// oneshot 同步等待辅助（测试内阻塞读）。
    trait AwaitBlocked {
        fn await_blocked(self) -> InterceptResolution;
    }
    impl AwaitBlocked for oneshot::Receiver<InterceptResolution> {
        fn await_blocked(self) -> InterceptResolution {
            self.blocking_recv().expect("resolution sent")
        }
    }

    #[tokio::test]
    async fn coordinator_ttl_expiry_marks_expired() {
        let tiny = InterceptCoordinator::new(2, 30, None);
        let (id, mut rx) = tiny
            .enqueue(
                "127.0.0.1",
                "GET",
                "/",
                "wsw-2-0-0001",
                "wsw-lis-t",
                "wsw-ses-t",
            )
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
        tiny.on_timeout(&id, &persist::now_iso());
        // waiter 已撤（recv 失败 = 已按过期收口，不再可裁决）。
        assert!(rx.try_recv().is_err());
        assert_eq!(tiny.pending(), 0);
    }
}
