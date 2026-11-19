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

use crate::persist::{self, InterceptDirection, InterceptRow, PersistStore};
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
    /// downstream_tls 连续错误（RUNNING→DEGRADED 评估，DESIGN §2.1）。
    pub consecutive_downstream_tls: AtomicU64,
    /// 启动时刻。
    pub started_at: String,
}

impl ListenerShared {
    /// 构造（listener start 面消费）。
    pub fn new(options: RuntimeOptions, persist: Option<PersistStore>) -> Self {
        let coordinator = InterceptCoordinator::new(
            options.config.queue_capacity,
            options.config.queue_ttl_seconds * 1000,
            persist.clone(),
        );
        Self {
            policy: OutOfScopePolicy::parse(&options.out_of_scope_policy),
            rules: options.scope_rules.clone(),
            options,
            counters: Counters::default(),
            state: Mutex::new(ListenerState::Stopped),
            coordinator,
            persist,
            consecutive_downstream_tls: AtomicU64::new(0),
            started_at: persist::now_iso(),
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
#[derive(Clone)]
#[doc(hidden)]
pub struct CaptureMeta {
    exchange_id: String,
    direction: &'static str,
    content_type: String,
}

/// 流式 tee 捕获体（透传路径）：≤cap 缓冲；流末完成 CAS + wsw_body_blob
/// 登记（capture_cas 开启时）；超帽 truncated 不落 CAS（避免半真数据入
/// CAS，仅日志记录——PLAN :482 fidelity 可见性）。
struct TeeBody {
    inner: Body,
    shared: Arc<ListenerShared>,
    meta: CaptureMeta,
    buf: Vec<u8>,
    cap: usize,
    truncated: bool,
}

impl futures::Stream for TeeBody {
    type Item = Result<Bytes, hudsucker::Error>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_frame(cx) {
            std::task::Poll::Ready(None) => {
                let meta = this.meta.clone();
                let shared = Arc::clone(&this.shared);
                let buf = std::mem::take(&mut this.buf);
                let truncated = this.truncated;
                let cap = this.cap;
                // 流末注册：小任务做 CAS 写 + 024 blob 行（异步面）。
                tokio::spawn(async move {
                    capture_finish(&shared, &meta, &buf, truncated, cap as u64);
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
}

#[derive(Debug, Clone)]
struct CurrentExchange {
    exchange_id: String,
    host: String,
    path: String,
    content_type: String,
}

impl WswHandler {
    /// 构造。
    pub fn new(shared: Arc<ListenerShared>) -> Self {
        Self {
            shared,
            current: None,
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
        self.current = Some(Arc::new(CurrentExchange {
            exchange_id: exchange_id.clone(),
            host: host.clone(),
            path: path.clone(),
            content_type: content_type.clone(),
        }));

        if intercepting {
            return self
                .handle_request_intercepted(req, exchange_id, content_type)
                .await;
        }

        // 3) 透传路径：流式 tee 捕获（≤cap 缓冲，超帽 truncated 不落 CAS）。
        let (mut parts, body) = req.into_parts();
        if self.shared.options.capture_cas {
            let meta = CaptureMeta {
                exchange_id,
                direction: "request",
                content_type,
            };
            let tee = TeeBody {
                inner: body,
                shared: Arc::clone(&self.shared),
                meta,
                buf: Vec::new(),
                cap: self.shared.options.config.max_body_bytes,
                truncated: false,
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
        let (exchange_id, host, content_type) = match &self.current {
            Some(cur) => (
                cur.exchange_id.clone(),
                cur.host.clone(),
                parts
                    .headers
                    .get(CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or(&cur.content_type)
                    .to_string(),
            ),
            None => (
                new_exchange_id(),
                "-".to_string(),
                "application/octet-stream".to_string(),
            ),
        };
        let status = parts.status.as_u16();
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
        };
        let tee = TeeBody {
            inner: body,
            shared: Arc::clone(&self.shared),
            meta,
            buf: Vec::new(),
            cap: self.shared.options.config.max_body_bytes,
            truncated: false,
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

        // CAS 捕获（拦截路径聚合字节完整落 CAS）。
        let meta = CaptureMeta {
            exchange_id: exchange_id.clone(),
            direction: "request",
            content_type,
        };
        capture_finish(&self.shared, &meta, &bytes, false, cap as u64);

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
    /// WS 帧透传挂载（W5 Workbench 留；本批只证明挂载面，POC 先例 main.rs:131-140）。
    async fn handle_message(&mut self, ctx: &WebSocketContext, msg: Message) -> Option<Message> {
        let endpoint = match ctx {
            WebSocketContext::ClientToServer { dst, .. } => dst.to_string(),
            WebSocketContext::ServerToClient { src, .. } => src.to_string(),
        };
        tracing::info!(
            class = "capture",
            ws_endpoint = %endpoint,
            ws_frame = %msg.len(),
            "ws frame passthrough (mounted; workbench W5)"
        );
        Some(msg)
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
