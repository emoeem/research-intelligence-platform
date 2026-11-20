//! 024 三表 + 025 四表持久化 store（WSW-2-ADR W2-D4/D6；WSW-3-ADR W3-D1/D2/D4/D7；
//! `migrations/024_wsw_intercept.sql` + `migrations/025_wsw_exchange.sql`）。
//!
//! - [`ListenerRow`]/[`SessionRow`]：listener/session 登记面（状态机列 +
//!   规则 JSON 引用）；
//! - [`InterceptRow`]：拦截队列行式状态机（W2-D4「行即事实」：五态迁移落在
//!   行的 `state` 列 + `resolved_at`；终态唯一性由 SQL `WHERE state='queued'`
//!   双面强制，与内存 [`crate::PendingQueue`] 语义一致）；
//! - [`PersistStore::reconcile_on_start`]：重启对账（QUEUED 未过期回载内存
//!   队列、过期标 EXPIRED；W2-D6）；
//! - [`PersistStore::expire_all_queued`]：停机全量强制 EXPIRED 终态
//!   （PLAN §7.5 :553 红线）。
//! - [`ExchangeRow`]：025 exchange 行（单行纪律：一 exchange 一行，请求+响应
//!   同行；Body 大内容不入行，行内只落 CAS BodyRef 派生列；W3-D1）。
//!   写权 = Rust-only（capture 通道 listener worker 批量事务 + send 通道
//!   rp-wsw send；W3-D7 分治表）；
//! - [`TabRow`]/[`TabVersionRow`]：Repeater tab 登记与 append-only 版本行
//!   （UNIQUE(tab_id, version_no) 锚：禁 UPDATE 禁 DELETE；W3-D1）；
//! - [`SendRow`]：发送历史六态行式状态机（PLAN §6.4 :501 词表；终态唯一性
//!   SQL `WHERE state IN ('pending','sending')` 强制，intercept 同款口径）。
//!
//! WAL：`PersistStore::open_file` 后显式 `enable_wal()`（`database::Pool::
//! enable_wal`，`pool.rs:258` 既有 API；rp_server 只读连接并发不互锁；W2-D2）。
//!
//! 纪律：rusqlite 经 `database` 重导出使用不直连；迁移执行器唯一为
//! `database::migrate`；只在测试内存库与开发库副本执行（`platform*.db`
//! 生产文件零触碰，023 文件头 :30 纪律同源）。

use crate::{InterceptState, ListenerState};
use database::{DbError, DbPath, DbResult, Pool, PoolConfig};
use std::path::{Path, PathBuf};
use tracing::error as tracing_error;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// `&dyn ToSql` 数组字面量宏（rusqlite execute/query_map 直传形态）。
macro_rules! sql_params {
    [$($v:expr),* $(,)?] => {
        [$(&$v as &dyn database::rusqlite::ToSql),*]
    };
}

/// 拦截方向词表（024 CHECK 列）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterceptDirection {
    /// 请求方向。
    Request,
    /// 响应方向。
    Response,
}

impl InterceptDirection {
    /// 024 CHECK 词表串。
    pub fn as_str(self) -> &'static str {
        match self {
            InterceptDirection::Request => "request",
            InterceptDirection::Response => "response",
        }
    }

    /// 词表串解析（非法即 Err）。
    pub fn from_str_db(s: &str) -> Result<Self, DbError> {
        match s {
            "request" => Ok(InterceptDirection::Request),
            "response" => Ok(InterceptDirection::Response),
            other => Err(DbError::Validation(format!(
                "invalid wsw_intercept.direction: {other:?}"
            ))),
        }
    }
}

/// InterceptState ↔ 024 CHECK 词表映射（'queued'/'forwarded'/'dropped'/
/// 'modified_forwarded'/'expired'）。
pub fn state_to_db(state: InterceptState) -> &'static str {
    match state {
        InterceptState::Queued => "queued",
        InterceptState::Forwarded => "forwarded",
        InterceptState::Dropped => "dropped",
        InterceptState::ModifiedForwarded => "modified_forwarded",
        InterceptState::Expired => "expired",
    }
}

/// 024 词表串 → InterceptState（非法即 Err，不静默映射）。
pub fn state_from_db(s: &str) -> Result<InterceptState, DbError> {
    match s {
        "queued" => Ok(InterceptState::Queued),
        "forwarded" => Ok(InterceptState::Forwarded),
        "dropped" => Ok(InterceptState::Dropped),
        "modified_forwarded" => Ok(InterceptState::ModifiedForwarded),
        "expired" => Ok(InterceptState::Expired),
        other => Err(DbError::Validation(format!(
            "invalid wsw_intercept.state: {other:?}"
        ))),
    }
}

/// ListenerState ↔ 024 CHECK 词表映射。
pub fn listener_state_to_db(state: ListenerState) -> &'static str {
    match state {
        ListenerState::Stopped => "stopped",
        ListenerState::Starting => "starting",
        ListenerState::Running => "running",
        ListenerState::Degraded => "degraded",
        ListenerState::Stopping => "stopping",
        ListenerState::Failed => "failed",
    }
}

/// 024 词表串 → ListenerState（非法即 Err）。
pub fn listener_state_from_db(s: &str) -> Result<ListenerState, DbError> {
    match s {
        "stopped" => Ok(ListenerState::Stopped),
        "starting" => Ok(ListenerState::Starting),
        "running" => Ok(ListenerState::Running),
        "degraded" => Ok(ListenerState::Degraded),
        "stopping" => Ok(ListenerState::Stopping),
        "failed" => Ok(ListenerState::Failed),
        other => Err(DbError::Validation(format!(
            "invalid wsw_listener.state: {other:?}"
        ))),
    }
}

/// `wsw_listener` 行（024 表）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListenerRow {
    /// `wsw-lis-` 前缀 ID（wsw-message ids.rs:130-135 词表）。
    pub listener_id: String,
    /// 名称。
    pub name: String,
    /// 绑定地址（必为 loopback，后端校验双保险）。
    pub bind: String,
    /// ListenerState 六态。
    pub state: ListenerState,
    /// ListenerConfig JSON + scope 规则引用（W2-D4 config_json 列）。
    pub config_json: String,
    /// 创建时刻（ISO8601 UTC）。
    pub created_at: String,
    /// 更新时刻（ISO8601 UTC）。
    pub updated_at: String,
}

/// `wsw_session` 行（024 表）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRow {
    /// `wsw-ses-` 前缀 ID（wsw-message ids.rs:122-128 词表）。
    pub session_id: String,
    /// 标题。
    pub title: String,
    /// include/exclude/allow_private/policy 规则源（D5）。
    pub scope_rules_json: String,
    /// out-of-scope 命中策略（blocked/passive_forward；024 CHECK）。
    pub out_of_scope_policy: String,
    /// 创建时刻。
    pub created_at: String,
    /// 更新时刻。
    pub updated_at: String,
}

/// `wsw_intercept` 行（024 表；拦截队列行式状态机）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterceptRow {
    /// `wsw-itc-` 前缀 ID（批内定稿，沿 ids.rs 范式生成侧承载）。
    pub intercept_id: String,
    /// 幂等键（UNIQUE；DESIGN §3.2 wsw_intercept_event 幂等语义并入行）。
    pub event_id: String,
    /// listener 引用（`wsw-lis-`）。
    pub listener_id: String,
    /// session 引用（`wsw-ses-`）。
    pub session_id: String,
    /// exchange 引用（`wsw-` 前缀；元数据表留 WSW-3，D9）。
    pub exchange_id: Option<String>,
    /// 方向（request/response）。
    pub direction: InterceptDirection,
    /// 五态。
    pub state: InterceptState,
    /// host（队列列表过滤最小投影）。
    pub host: Option<String>,
    /// method。
    pub method: Option<String>,
    /// path。
    pub path: Option<String>,
    /// 入队时刻（ISO8601 UTC 带毫秒，字典序=时间序）。
    pub queued_at: String,
    /// TTL 到期时刻（ListenerConfig.queue_ttl_seconds 1..3600）。
    pub expires_at: String,
    /// 终态时刻。
    pub resolved_at: Option<String>,
    /// 拒绝/过期/丢弃原因（错误五态 class 可入）。
    pub reason: Option<String>,
}

/// 重启对账结果（W2-D6）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecoveryOutcome {
    /// 未过期 QUEUED 行（回载内存队列；调用面重算 expires_at = queued_at+ttl）。
    pub reloaded: Vec<InterceptRow>,
    /// 本次对账标 EXPIRED 的行数。
    pub expired_count: usize,
}

/// 序号发生器（进程内单调）。
static SEQ: AtomicU64 = AtomicU64::new(0);

/// 当前时刻 → ISO8601 UTC 带毫秒（固定宽，字典序=时间序）。
pub fn now_iso() -> String {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    epoch_ms_to_iso(d.as_millis() as i64)
}

/// epoch ms → ISO8601 UTC 带毫秒（纯整数历法换算，零外部依赖）。
pub fn epoch_ms_to_iso(epoch_ms: i64) -> String {
    let secs = epoch_ms.div_euclid(1000);
    let ms = epoch_ms.rem_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // civil_from_days（Howard Hinnant 算法；1970-01-01 = days 0）。
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    format!("{year:04}-{month:02}-{d:02}T{h:02}:{m:02}:{s:02}.{ms:03}Z")
}

/// ISO8601 UTC 带毫秒 → epoch ms（`epoch_ms_to_iso` 逆运算；解析失败 None）。
pub fn iso_to_epoch_ms(iso: &str) -> Option<i64> {
    let bytes = iso.as_bytes();
    if bytes.len() != 24 || !iso.ends_with('Z') {
        return None;
    }
    let num = |r: std::ops::Range<usize>| iso.get(r)?.parse::<i64>().ok();
    let year = num(0..4)?;
    let month = num(5..7)?;
    let day = num(8..10)?;
    let h = num(11..13)?;
    let m = num(14..16)?;
    let s = num(17..19)?;
    let ms = num(20..23)?;
    if bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'.'
    {
        return None;
    }
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // days_from_civil（Howard Hinnant 算法逆算）。
    let yy = if month <= 2 { year - 1 } else { year };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some((days * 86_400 + h * 3600 + m * 60 + s) * 1000 + ms)
}

/// 生成 `wsw-itc-<ts>-<seq>-<rand4>` 拦截行 ID（沿 ids.rs:9 范式，生成侧承载）。
pub fn generate_intercept_id() -> String {
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros();
    let ts64 = ts as u64;
    let rand4 = (std::process::id() as u64 ^ ts64 ^ (seq << 32)) & 0xffff;
    format!("wsw-itc-{ts64}-{seq}-{rand4:04x}")
}

/// 024/025 store（Pool 包裹；rusqlite 经 database 重导出）。
#[derive(Clone)]
pub struct PersistStore {
    pool: Pool,
    /// 库路径（内存库 None）。
    path: Option<PathBuf>,
}

impl PersistStore {
    /// 文件库打开（调用面随后必须 `run_migrations`）。WAL 在 open 后显式
    /// 开启（W3-D2.4：rp_server mode=ro 读连接与 Rust 写者并发不互锁）。
    pub fn open_file(path: &Path) -> DbResult<Self> {
        let pool = Pool::open(PoolConfig::writable_file(path))?;
        pool.enable_wal()?;
        Ok(Self {
            pool,
            path: Some(path.to_path_buf()),
        })
    }

    /// 测试内存库打开（单连接池）。
    pub fn open_memory() -> DbResult<Self> {
        Ok(Self {
            pool: Pool::open(PoolConfig {
                path: DbPath::Memory,
                readonly: false,
                max_connections: 1,
                busy_timeout: std::time::Duration::from_secs(5),
            })?,
            path: None,
        })
    }

    /// 迁移执行（唯一入口 `database::migrate`；025 后 user_version=25）。
    pub fn run_migrations(
        &self,
        migrations_dir: &Path,
    ) -> DbResult<Vec<database::AppliedMigration>> {
        database::migrate(&self.pool, migrations_dir)
    }

    /// 库路径视图。
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// listener 登记（INSERT OR REPLACE：状态机行级 upsert）。
    pub fn register_listener(&self, row: &ListenerRow) -> DbResult<()> {
        validate_listener_row(row)?;
        self.pool.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO wsw_listener \
                 (listener_id, name, bind, state, config_json, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                sql_params![
                    row.listener_id,
                    row.name,
                    row.bind,
                    listener_state_to_db(row.state),
                    row.config_json,
                    row.created_at,
                    row.updated_at
                ],
            )?;
            Ok::<(), database::rusqlite::Error>(())
        })
    }

    /// session 登记（INSERT OR REPLACE）。
    pub fn register_session(&self, row: &SessionRow) -> DbResult<()> {
        validate_session_row(row)?;
        self.pool.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO wsw_session \
                 (session_id, title, scope_rules_json, out_of_scope_policy, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                sql_params![
                    row.session_id,
                    row.title,
                    row.scope_rules_json,
                    row.out_of_scope_policy,
                    row.created_at,
                    row.updated_at
                ],
            )?;
            Ok::<(), database::rusqlite::Error>(())
        })
    }

    /// 拦截入队（INSERT；event_id UNIQUE 幂等键：同键重复直接报错——幂等重放
    /// 不静默成功，register_body_blob 冲突防呆同款口径）。
    pub fn enqueue_intercept(&self, row: &InterceptRow) -> DbResult<()> {
        validate_intercept_row(row)?;
        self.pool.with_conn(|conn| {
            conn.execute(
                "INSERT INTO wsw_intercept \
                 (intercept_id, event_id, listener_id, session_id, exchange_id, direction, state, \
                  host, method, path, queued_at, expires_at, resolved_at, reason) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                sql_params![
                    row.intercept_id,
                    row.event_id,
                    row.listener_id,
                    row.session_id,
                    row.exchange_id,
                    row.direction.as_str(),
                    state_to_db(row.state),
                    row.host,
                    row.method,
                    row.path,
                    row.queued_at,
                    row.expires_at,
                    row.resolved_at,
                    row.reason
                ],
            )?;
            Ok::<(), database::rusqlite::Error>(())
        })
    }

    /// 终态推进（行式状态机 SQL 强制：仅 `state='queued'` 行可推进；返回
    /// false = 已终态或不存在——与内存 `PendingQueue::resolve` AlreadyResolved
    /// 语义一致）。
    pub fn resolve_intercept(
        &self,
        intercept_id: &str,
        next: InterceptState,
        resolved_at: &str,
        reason: Option<&str>,
    ) -> DbResult<bool> {
        let target = state_to_db(next);
        if target == "queued" {
            return Err(DbError::Validation("cannot resolve to queued".into()));
        }
        let n = self.pool.with_conn(|conn| {
            conn.execute(
                "UPDATE wsw_intercept SET state=?1, resolved_at=?2, reason=?3 \
                 WHERE intercept_id=?4 AND state='queued'",
                sql_params![target, resolved_at, reason, intercept_id],
            )
        })?;
        Ok(n == 1)
    }

    /// 停机面：全部 QUEUED 强制 EXPIRED（PLAN §7.5 :553）。
    pub fn expire_all_queued(&self, at: &str, reason: &str) -> DbResult<usize> {
        self.pool.with_conn(|conn| {
            conn.execute(
                "UPDATE wsw_intercept SET state='expired', resolved_at=?1, reason=?2 \
                 WHERE state='queued'",
                sql_params![at, reason],
            )
        })
    }

    /// 重启对账：QUEUED 行按 `expires_at` 分流——已过 → UPDATE expired；
    /// 未过 → 回载（W2-D6）。`now` 为 ISO8601 UTC 带毫秒。
    pub fn reconcile_on_start(&self, now: &str, reason: &str) -> DbResult<RecoveryOutcome> {
        let now_ms = iso_to_epoch_ms(now)
            .ok_or_else(|| DbError::Validation(format!("bad iso timestamp: {now:?}")))?;
        let queued = self.list_queued()?;
        let mut outcome = RecoveryOutcome::default();
        for row in queued {
            let expires_ms = iso_to_epoch_ms(&row.expires_at).ok_or_else(|| {
                DbError::Validation(format!("bad expires_at: {:?}", row.expires_at))
            })?;
            if now_ms >= expires_ms {
                self.resolve_intercept(
                    &row.intercept_id,
                    InterceptState::Expired,
                    now,
                    Some(reason),
                )?;
                outcome.expired_count += 1;
            } else {
                outcome.reloaded.push(row);
            }
        }
        Ok(outcome)
    }

    /// 当前 QUEUED 行（对账/状态面）。
    pub fn list_queued(&self) -> DbResult<Vec<InterceptRow>> {
        self.list_intercepts_state("queued")
    }

    /// 按状态列行（只读摘要面；rp_server intercept 队列摘要消费）。
    pub fn list_intercepts_state(&self, state: &str) -> DbResult<Vec<InterceptRow>> {
        self.pool.with_conn(|conn| -> DbResult<Vec<InterceptRow>> {
            let mut stmt = conn.prepare(
                "SELECT intercept_id, event_id, listener_id, session_id, exchange_id, direction, \
                        state, host, method, path, queued_at, expires_at, resolved_at, reason \
                 FROM wsw_intercept WHERE state=?1 ORDER BY queued_at DESC LIMIT 500",
            )?;
            let rows = stmt.query_map(sql_params![state], map_intercept_row)?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
    }

    /// 全部拦截行（测试/对账面）。
    pub fn list_all_intercepts(&self) -> DbResult<Vec<InterceptRow>> {
        self.pool.with_conn(|conn| -> DbResult<Vec<InterceptRow>> {
            let mut stmt = conn.prepare(
                "SELECT intercept_id, event_id, listener_id, session_id, exchange_id, direction, \
                        state, host, method, path, queued_at, expires_at, resolved_at, reason \
                 FROM wsw_intercept ORDER BY queued_at DESC LIMIT 2000",
            )?;
            let rows = stmt.query_map([], map_intercept_row)?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
    }

    /// 拦截行计数（状态面）。
    pub fn intercept_count(&self) -> DbResult<u64> {
        self.pool.with_conn(|conn| {
            conn.query_row("SELECT COUNT(*) FROM wsw_intercept", [], |r| r.get(0))
        })
    }

    /// 连接视图（只读核对面；测试/工具用）。
    pub fn with_conn<T, E>(
        &self,
        f: impl FnOnce(&database::rusqlite::Connection) -> Result<T, E>,
    ) -> DbResult<T>
    where
        DbError: From<E>,
    {
        self.pool.with_conn(f)
    }
}

fn map_intercept_row(
    row: &database::rusqlite::Row<'_>,
) -> database::rusqlite::Result<InterceptRow> {
    let state_s: String = row.get(6)?;
    let state = state_from_db(&state_s)
        .map_err(|e| database::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    let dir_s: String = row.get(5)?;
    let direction = InterceptDirection::from_str_db(&dir_s)
        .map_err(|e| database::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    Ok(InterceptRow {
        intercept_id: row.get(0)?,
        event_id: row.get(1)?,
        listener_id: row.get(2)?,
        session_id: row.get(3)?,
        exchange_id: row.get(4)?,
        direction,
        state,
        host: row.get(7)?,
        method: row.get(8)?,
        path: row.get(9)?,
        queued_at: row.get(10)?,
        expires_at: row.get(11)?,
        resolved_at: row.get(12)?,
        reason: row.get(13)?,
    })
}

/// listener 行校验（与 024 CHECK 双保险：ID 前缀 + bind 必 loopback + 词表）。
fn validate_listener_row(row: &ListenerRow) -> Result<(), DbError> {
    if !row.listener_id.starts_with("wsw-lis-") {
        return Err(DbError::Validation(format!(
            "listener_id must start with wsw-lis-: {:?}",
            row.listener_id
        )));
    }
    let bind_ip = row
        .bind
        .rsplit_once(':')
        .and_then(|(h, _)| h.trim_matches(['[', ']']).parse::<std::net::IpAddr>().ok())
        .ok_or_else(|| DbError::Validation(format!("bad bind: {:?}", row.bind)))?;
    if !bind_ip.is_loopback() {
        return Err(DbError::Validation(format!(
            "bind must be loopback: {:?}",
            row.bind
        )));
    }
    listener_state_from_db(listener_state_to_db(row.state))?;
    Ok(())
}

/// session 行校验（ID 前缀 + 策略词表 + 规则 JSON 可解析）。
fn validate_session_row(row: &SessionRow) -> Result<(), DbError> {
    if !row.session_id.starts_with("wsw-ses-") {
        return Err(DbError::Validation(format!(
            "session_id must start with wsw-ses-: {:?}",
            row.session_id
        )));
    }
    if !matches!(
        row.out_of_scope_policy.as_str(),
        "blocked" | "passive_forward"
    ) {
        return Err(DbError::Validation(format!(
            "invalid out_of_scope_policy: {:?}",
            row.out_of_scope_policy
        )));
    }
    crate::scope::ScopeRules::from_json(&row.scope_rules_json)
        .map_err(|e| DbError::Validation(format!("bad scope_rules_json: {e}")))?;
    Ok(())
}

/// intercept 行校验（ID 前缀 + 幂等键非空 + 终态字段一致性）。
fn validate_intercept_row(row: &InterceptRow) -> Result<(), DbError> {
    if !row.intercept_id.starts_with("wsw-itc-") {
        return Err(DbError::Validation(format!(
            "intercept_id must start with wsw-itc-: {:?}",
            row.intercept_id
        )));
    }
    if !row.listener_id.starts_with("wsw-lis-") {
        return Err(DbError::Validation(format!(
            "listener_id must start with wsw-lis-: {:?}",
            row.listener_id
        )));
    }
    if !row.session_id.starts_with("wsw-ses-") {
        return Err(DbError::Validation(format!(
            "session_id must start with wsw-ses-: {:?}",
            row.session_id
        )));
    }
    if row.event_id.trim().is_empty() {
        return Err(DbError::Validation("event_id must be non-empty".into()));
    }
    if let Some(ex) = &row.exchange_id {
        if !ex.starts_with("wsw-") {
            return Err(DbError::Validation(format!(
                "exchange_id must start with wsw-: {ex:?}"
            )));
        }
    }
    if row.state != InterceptState::Queued && row.resolved_at.is_none() {
        return Err(DbError::Validation(
            "terminal-state rows require resolved_at".into(),
        ));
    }
    if iso_to_epoch_ms(&row.queued_at).is_none() || iso_to_epoch_ms(&row.expires_at).is_none() {
        return Err(DbError::Validation(
            "queued_at/expires_at must be ISO8601 UTC ms (YYYY-MM-DDTHH:MM:SS.mmmZ)".into(),
        ));
    }
    Ok(())
}

// ===========================================================================
// 025：wsw_exchange / wsw_repeater_tab / wsw_repeater_tab_version /
//      wsw_repeater_send（WSW-3-ADR W3-D1/D4/D7）
// ===========================================================================

/// 生成 `wsw-rpt-<micros>-<seq>-<rand4>` Repeater tab ID（ids.rs:9 范式）。
pub fn generate_tab_id() -> String {
    format!("wsw-rpt-{}", generic_unique_suffix())
}

/// 生成 `wsw-snd-<micros>-<seq>-<rand4>` 发送行 ID（ids.rs:9 范式）。
pub fn generate_send_id() -> String {
    format!("wsw-snd-{}", generic_unique_suffix())
}

fn generic_unique_suffix() -> String {
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros();
    let ts64 = ts as u64;
    let rand4 = (std::process::id() as u64 ^ ts64 ^ (seq << 32)) & 0xffff;
    format!("{ts64}-{seq}-{rand4:04x}")
}

/// `wsw_exchange` 行（025 表；单行纪律：一 exchange 一行，请求+响应同行）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExchangeRow {
    pub exchange_id: String,
    pub session_id: String,
    pub listener_id: String,
    pub sequence_no: i64,
    /// 'client_to_server'/'server_to_client'（capture 行恒请求方向）。
    pub direction: &'static str,
    /// 'proxy_capture'/'repeater'/'fuzzer'/'import'。
    pub source: &'static str,
    /// 'http'/'ws'（本批 capture/send 均为 'http'）。
    pub protocol: &'static str,
    pub http_version: Option<String>,
    pub tls_version: Option<String>,
    pub alpn: Option<String>,
    pub method: Option<String>,
    pub scheme: Option<String>,
    pub authority: Option<String>,
    pub path: Option<String>,
    pub response_status: Option<i64>,
    /// Parsed 头序保持投影 JSON（detail 专用；列表端点不选此列）。
    pub request_headers_json: Option<String>,
    pub response_headers_json: Option<String>,
    pub request_body_sha256: Option<String>,
    pub request_body_uri: Option<String>,
    pub request_body_len: Option<i64>,
    pub request_mime: Option<String>,
    /// 'full'/'truncated'（CAS BodyRef.c 继承；None = 无体/未捕获）。
    pub request_body_fidelity: Option<&'static str>,
    pub response_body_sha256: Option<String>,
    pub response_body_uri: Option<String>,
    pub response_body_len: Option<i64>,
    pub response_mime: Option<String>,
    pub response_body_fidelity: Option<&'static str>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub ttfb_ms: Option<i64>,
    pub remote_ip: Option<String>,
    /// 'in_scope'/'out_of_scope_passive'/'out_of_scope_blocked'；None=Unknown。
    pub scope_decision: Option<&'static str>,
    pub edited: bool,
    /// JSON 数组串（默认 '[]'）。
    pub tags_json: String,
    pub notes: Option<String>,
    pub trace_id: Option<String>,
    pub task_id: Option<String>,
    pub tool_run_id: Option<String>,
    pub artifact_id: Option<String>,
    pub evidence_id: Option<String>,
    /// 'full'/'truncated'/'degraded_no_wire'/'import'。
    pub capture_fidelity: &'static str,
    /// 'ok'/'partial'/'failed'。
    pub parse_status: &'static str,
    pub error_code: Option<String>,
    /// 有界搜索投影（≤8KiB；凭据头值排除——DESIGN §4.5；insert 侧生成）。
    pub search_text: Option<String>,
}

/// `wsw_repeater_tab` 行（025 表）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabRow {
    pub tab_id: String,
    pub session_id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    /// NULL = 打开。
    pub closed_at: Option<String>,
}

/// `wsw_repeater_tab_version` 行（025 表；append-only，禁 UPDATE 禁 DELETE）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabVersionRow {
    pub tab_id: String,
    pub version_no: i64,
    /// OutboundRequest serde JSON（体 base64 内联，8MiB 帽）。
    pub request_json: String,
    pub source_exchange_id: Option<String>,
    /// 'manual'/'from_history'/'restore'/'send_snapshot'。
    pub origin: String,
    pub created_at: String,
}

/// `wsw_repeater_send` 行（025 表；六态行式状态机）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SendRow {
    pub send_id: String,
    pub tab_id: String,
    pub version_no: i64,
    pub session_id: String,
    pub exchange_id: Option<String>,
    /// 'pending'/'sending'/'succeeded'/'failed'/'cancelled'/'timed_out'。
    pub state: String,
    pub request_outbound_sha256: Option<String>,
    pub redirect_chain_json: Option<String>,
    pub response_status: Option<i64>,
    pub response_body_sha256: Option<String>,
    pub response_body_len: Option<i64>,
    pub response_mime: Option<String>,
    pub remote_ip: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub ttfb_ms: Option<i64>,
    pub error_code: Option<String>,
    pub created_at: String,
}

/// send 行终态推进 patch（SQL WHERE state IN ('pending','sending') 强制）。
#[derive(Debug, Clone, Default)]
pub struct SendPatch {
    pub state: &'static str,
    pub exchange_id: Option<String>,
    pub request_outbound_sha256: Option<String>,
    pub redirect_chain_json: Option<String>,
    pub response_status: Option<i64>,
    pub response_body_sha256: Option<String>,
    pub response_body_len: Option<i64>,
    pub response_mime: Option<String>,
    pub remote_ip: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub ttfb_ms: Option<i64>,
    pub error_code: Option<String>,
}

/// 批量落行结果（salvage 语义：单行失败不炸整批，计数可见）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BatchInsertOutcome {
    pub inserted: usize,
    pub failed: usize,
}

/// exchange 行校验（025 CHECK 双保险：ID 前缀 + 词表 + ISO8601 ms）。
fn validate_exchange_row(row: &ExchangeRow) -> Result<(), DbError> {
    if !row.exchange_id.starts_with("wsw-") {
        return Err(DbError::Validation(format!(
            "exchange_id must start with wsw-: {:?}",
            row.exchange_id
        )));
    }
    if !row.session_id.starts_with("wsw-ses-") {
        return Err(DbError::Validation(format!(
            "session_id must start with wsw-ses-: {:?}",
            row.session_id
        )));
    }
    if !row.listener_id.starts_with("wsw-lis-") {
        return Err(DbError::Validation(format!(
            "listener_id must start with wsw-lis-: {:?}",
            row.listener_id
        )));
    }
    if !matches!(row.direction, "client_to_server" | "server_to_client") {
        return Err(DbError::Validation(format!("invalid direction: {:?}", row.direction)));
    }
    if !matches!(row.source, "proxy_capture" | "repeater" | "fuzzer" | "import") {
        return Err(DbError::Validation(format!("invalid source: {:?}", row.source)));
    }
    if !matches!(row.protocol, "http" | "ws") {
        return Err(DbError::Validation(format!("invalid protocol: {:?}", row.protocol)));
    }
    if !matches!(
        row.capture_fidelity,
        "full" | "truncated" | "degraded_no_wire" | "import"
    ) {
        return Err(DbError::Validation(format!(
            "invalid capture_fidelity: {:?}",
            row.capture_fidelity
        )));
    }
    if !matches!(row.parse_status, "ok" | "partial" | "failed") {
        return Err(DbError::Validation(format!(
            "invalid parse_status: {:?}",
            row.parse_status
        )));
    }
    if let Some(d) = row.scope_decision {
        if !matches!(d, "in_scope" | "out_of_scope_passive" | "out_of_scope_blocked") {
            return Err(DbError::Validation(format!("invalid scope_decision: {d:?}")));
        }
    }
    if let Some(t) = &row.started_at {
        if iso_to_epoch_ms(t).is_none() {
            return Err(DbError::Validation(format!(
                "started_at must be ISO8601 UTC ms: {t:?}"
            )));
        }
    }
    if let Some(t) = &row.completed_at {
        if iso_to_epoch_ms(t).is_none() {
            return Err(DbError::Validation(format!(
                "completed_at must be ISO8601 UTC ms: {t:?}"
            )));
        }
    }
    if let Some(sha) = &row.request_body_sha256 {
        if !wsw_message::cas::is_valid_sha256(sha) {
            return Err(DbError::Validation("bad request_body_sha256".into()));
        }
    }
    if let Some(sha) = &row.response_body_sha256 {
        if !wsw_message::cas::is_valid_sha256(sha) {
            return Err(DbError::Validation("bad response_body_sha256".into()));
        }
    }
    Ok(())
}

/// send 六态词表校验。
fn validate_send_state(state: &str) -> Result<(), DbError> {
    if matches!(
        state,
        "pending" | "sending" | "succeeded" | "failed" | "cancelled" | "timed_out"
    ) {
        Ok(())
    } else {
        Err(DbError::Validation(format!("invalid send state: {state:?}")))
    }
}

impl PersistStore {
    /// 025 水位断言（迁移后调用面；user_version 必须 = 25）。
    pub fn assert_watermark_25(&self) -> DbResult<()> {
        let v: i64 = self
            .pool
            .with_conn(|conn| conn.query_row("PRAGMA user_version", [], |r| r.get(0)))?;
        if v != 25 {
            return Err(DbError::Validation(format!(
                "025 chain head expected user_version=25, got {v}"
            )));
        }
        Ok(())
    }

    /// 批量落 exchange 行（W3-D2.3：单事务 + 逐行 salvage——单行 CHECK/UNIQUE
    /// 冲突只跳过该行计数可见，不炸整批；W3-D2.2 满载丢弃面由 worker 上游
    /// channel 承担，本面是落库终端）。
    pub fn insert_exchanges(&self, rows: &[ExchangeRow]) -> DbResult<BatchInsertOutcome> {
        let mut outcome = BatchInsertOutcome::default();
        self.pool.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;
            for row in rows {
                if let Err(e) = validate_exchange_row(row) {
                    tracing_error!(
                        class = "parse_framing",
                        error_code = "exchange_row_invalid",
                        cause = %e,
                        exchange_id = %row.exchange_id,
                        "exchange row rejected by pre-insert validation"
                    );
                    outcome.failed += 1;
                    continue;
                }
                let res = tx.execute(
                    "INSERT INTO wsw_exchange \
                     (exchange_id, session_id, listener_id, sequence_no, direction, source, \
                      protocol, http_version, tls_version, alpn, method, scheme, authority, path, \
                      response_status, request_headers_json, response_headers_json, \
                      request_body_sha256, request_body_uri, request_body_len, request_mime, \
                      request_body_fidelity, response_body_sha256, response_body_uri, \
                      response_body_len, response_mime, response_body_fidelity, \
                      started_at, completed_at, duration_ms, ttfb_ms, remote_ip, scope_decision, \
                      edited, tags_json, notes, trace_id, task_id, tool_run_id, artifact_id, \
                      evidence_id, capture_fidelity, parse_status, error_code, search_text) \
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20, \
                             ?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,?31,?32,?33,?34,?35,?36,?37,?38, \
                             ?39,?40,?41,?42,?43,?44,?45)",
                    &sql_params![
                        row.exchange_id,
                        row.session_id,
                        row.listener_id,
                        row.sequence_no,
                        row.direction,
                        row.source,
                        row.protocol,
                        row.http_version,
                        row.tls_version,
                        row.alpn,
                        row.method,
                        row.scheme,
                        row.authority,
                        row.path,
                        row.response_status,
                        row.request_headers_json,
                        row.response_headers_json,
                        row.request_body_sha256,
                        row.request_body_uri,
                        row.request_body_len,
                        row.request_mime,
                        row.request_body_fidelity,
                        row.response_body_sha256,
                        row.response_body_uri,
                        row.response_body_len,
                        row.response_mime,
                        row.response_body_fidelity,
                        row.started_at,
                        row.completed_at,
                        row.duration_ms,
                        row.ttfb_ms,
                        row.remote_ip,
                        row.scope_decision,
                        row.edited as i64,
                        row.tags_json,
                        row.notes,
                        row.trace_id,
                        row.task_id,
                        row.tool_run_id,
                        row.artifact_id,
                        row.evidence_id,
                        row.capture_fidelity,
                        row.parse_status,
                        row.error_code,
                        row.search_text
                    ][..],
                );
                match res {
                    Ok(_) => outcome.inserted += 1,
                    Err(e) => {
                        tracing_error!(
                            class = "parse_framing",
                            error_code = "exchange_row_insert_failed",
                            cause = %e,
                            exchange_id = %row.exchange_id,
                            "exchange row insert failed (row salvaged out of batch)"
                        );
                        outcome.failed += 1;
                    }
                }
            }
            tx.commit()?;
            Ok::<(), database::rusqlite::Error>(())
        })?;
        Ok(outcome)
    }

    /// tags/notes 编辑（W3-D7 Python 直写同款列；Rust 侧供测试/工具面）。
    /// 返回 false = 行不存在。
    pub fn update_exchange_tags_notes(
        &self,
        exchange_id: &str,
        tags_json: &str,
        notes: Option<&str>,
    ) -> DbResult<bool> {
        let n = self.pool.with_conn(|conn| {
            conn.execute(
                "UPDATE wsw_exchange SET tags_json=?1, notes=?2 WHERE exchange_id=?3",
                sql_params![tags_json, notes, exchange_id],
            )
        })?;
        Ok(n == 1)
    }

    /// exchange 行数（DESIGN §3.4 retention 行数帽查询面）。
    pub fn exchange_count(&self) -> DbResult<u64> {
        self.pool
            .with_conn(|conn| conn.query_row("SELECT COUNT(*) FROM wsw_exchange", [], |r| r.get(0)))
    }

    /// 下一 sequence_no（listener 内对账面；重启恢复 = MAX+1）。
    pub fn next_sequence_no(&self, listener_id: &str) -> DbResult<i64> {
        self.pool.with_conn(|conn| {
            conn.query_row(
                "SELECT COALESCE(MAX(sequence_no)+1, 0) FROM wsw_exchange WHERE listener_id=?1",
                sql_params![listener_id],
                |r| r.get(0),
            )
        })
    }

    /// Repeater tab 登记（INSERT；同 tab_id 重复报错）。
    pub fn create_tab(&self, row: &TabRow) -> DbResult<()> {
        if !row.tab_id.starts_with("wsw-rpt-") {
            return Err(DbError::Validation(format!(
                "tab_id must start with wsw-rpt-: {:?}",
                row.tab_id
            )));
        }
        if row.title.trim().is_empty() {
            return Err(DbError::Validation("tab title must be non-empty".into()));
        }
        self.pool.with_conn(|conn| {
            conn.execute(
                "INSERT INTO wsw_repeater_tab \
                 (tab_id, session_id, title, created_at, updated_at, closed_at) \
                 VALUES (?1,?2,?3,?4,?5,?6)",
                sql_params![
                    row.tab_id,
                    row.session_id,
                    row.title,
                    row.created_at,
                    row.updated_at,
                    row.closed_at
                ],
            )?;
            Ok::<(), database::rusqlite::Error>(())
        })
    }

    /// tab 关闭（closed_at 落账；控制行级 UPDATE——版本行 append-only 不受影响）。
    pub fn close_tab(&self, tab_id: &str, closed_at: &str) -> DbResult<bool> {
        let n = self.pool.with_conn(|conn| {
            conn.execute(
                "UPDATE wsw_repeater_tab SET closed_at=?1, updated_at=?1 \
                 WHERE tab_id=?2 AND closed_at IS NULL",
                sql_params![closed_at, tab_id],
            )
        })?;
        Ok(n == 1)
    }

    /// 打开 tab 列表（closed_at IS NULL；updated_at DESC）。
    pub fn list_open_tabs(&self, session_id: &str) -> DbResult<Vec<TabRow>> {
        self.pool.with_conn(|conn| -> DbResult<Vec<TabRow>> {
            let mut stmt = conn.prepare(
                "SELECT tab_id, session_id, title, created_at, updated_at, closed_at \
                 FROM wsw_repeater_tab WHERE session_id=?1 AND closed_at IS NULL \
                 ORDER BY updated_at DESC LIMIT 200",
            )?;
            let rows = stmt.query_map(sql_params![session_id], map_tab_row)?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
    }

    /// append-only 版本行落账（UNIQUE(tab_id, version_no) 锚：同键重写报错；
    /// origin 词表 + request_json 8MiB 帽校验；无 UPDATE/DELETE 路径）。
    pub fn append_tab_version(&self, row: &TabVersionRow) -> DbResult<()> {
        if !row.tab_id.starts_with("wsw-rpt-") {
            return Err(DbError::Validation(format!(
                "tab_id must start with wsw-rpt-: {:?}",
                row.tab_id
            )));
        }
        if row.version_no < 1 {
            return Err(DbError::Validation("version_no must be >= 1".into()));
        }
        if !matches!(row.origin.as_str(), "manual" | "from_history" | "restore" | "send_snapshot") {
            return Err(DbError::Validation(format!("invalid origin: {:?}", row.origin)));
        }
        if row.request_json.len() > 8_388_608 {
            return Err(DbError::Validation(
                "request_json exceeds 8MiB cap (413)".into(),
            ));
        }
        if let Some(ex) = &row.source_exchange_id {
            if !ex.starts_with("wsw-") {
                return Err(DbError::Validation(format!(
                    "source_exchange_id must start with wsw-: {ex:?}"
                )));
            }
        }
        self.pool.with_conn(|conn| {
            conn.execute(
                "INSERT INTO wsw_repeater_tab_version \
                 (tab_id, version_no, request_json, source_exchange_id, origin, created_at) \
                 VALUES (?1,?2,?3,?4,?5,?6)",
                sql_params![
                    row.tab_id,
                    row.version_no,
                    row.request_json,
                    row.source_exchange_id,
                    row.origin,
                    row.created_at
                ],
            )?;
            Ok::<(), database::rusqlite::Error>(())
        })
    }

    /// 取版本行（发送面回放依据）。
    pub fn get_tab_version(&self, tab_id: &str, version_no: i64) -> DbResult<Option<TabVersionRow>> {
        self.pool.with_conn(|conn| -> DbResult<Option<TabVersionRow>> {
            let mut stmt = conn.prepare(
                "SELECT tab_id, version_no, request_json, source_exchange_id, origin, created_at \
                 FROM wsw_repeater_tab_version WHERE tab_id=?1 AND version_no=?2",
            )?;
            let mut rows = stmt.query_map(sql_params![tab_id, version_no], map_version_row)?;
            Ok(rows.next().transpose()?)
        })
    }

    /// 最新版本行（tab 打开面回填编辑器）。
    pub fn latest_tab_version(&self, tab_id: &str) -> DbResult<Option<TabVersionRow>> {
        self.pool.with_conn(|conn| -> DbResult<Option<TabVersionRow>> {
            let mut stmt = conn.prepare(
                "SELECT tab_id, version_no, request_json, source_exchange_id, origin, created_at \
                 FROM wsw_repeater_tab_version WHERE tab_id=?1 \
                 ORDER BY version_no DESC LIMIT 1",
            )?;
            let mut rows = stmt.query_map(sql_params![tab_id], map_version_row)?;
            Ok(rows.next().transpose()?)
        })
    }

    /// 版本历史（append-only 全序列；version_no DESC）。
    pub fn list_tab_versions(&self, tab_id: &str, limit: i64) -> DbResult<Vec<TabVersionRow>> {
        self.pool.with_conn(|conn| -> DbResult<Vec<TabVersionRow>> {
            let mut stmt = conn.prepare(
                "SELECT tab_id, version_no, request_json, source_exchange_id, origin, created_at \
                 FROM wsw_repeater_tab_version WHERE tab_id=?1 \
                 ORDER BY version_no DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(sql_params![tab_id, limit.min(500)], map_version_row)?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
    }

    /// send 行登记（state='pending' 起步；INSERT）。
    pub fn insert_send(&self, row: &SendRow) -> DbResult<()> {
        if !row.send_id.starts_with("wsw-snd-") {
            return Err(DbError::Validation(format!(
                "send_id must start with wsw-snd-: {:?}",
                row.send_id
            )));
        }
        validate_send_state(&row.state)?;
        if iso_to_epoch_ms(&row.created_at).is_none() {
            return Err(DbError::Validation(
                "created_at must be ISO8601 UTC ms".into(),
            ));
        }
        self.pool.with_conn(|conn| {
            conn.execute(
                "INSERT INTO wsw_repeater_send \
                 (send_id, tab_id, version_no, session_id, exchange_id, state, \
                  request_outbound_sha256, redirect_chain_json, response_status, \
                  response_body_sha256, response_body_len, response_mime, remote_ip, \
                  started_at, completed_at, duration_ms, ttfb_ms, error_code, created_at) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)",
                sql_params![
                    row.send_id,
                    row.tab_id,
                    row.version_no,
                    row.session_id,
                    row.exchange_id,
                    row.state,
                    row.request_outbound_sha256,
                    row.redirect_chain_json,
                    row.response_status,
                    row.response_body_sha256,
                    row.response_body_len,
                    row.response_mime,
                    row.remote_ip,
                    row.started_at,
                    row.completed_at,
                    row.duration_ms,
                    row.ttfb_ms,
                    row.error_code,
                    row.created_at
                ],
            )?;
            Ok::<(), database::rusqlite::Error>(())
        })
    }

    /// send 行推进（终态唯一性 SQL 强制：仅 pending/sending 可推进；返回
    /// false = 已终态或不存在——intercept `WHERE state='queued'` 同款口径）。
    pub fn advance_send(&self, send_id: &str, patch: &SendPatch) -> DbResult<bool> {
        validate_send_state(patch.state)?;
        let n = self.pool.with_conn(|conn| {
            conn.execute(
                "UPDATE wsw_repeater_send SET \
                   exchange_id=COALESCE(?2, exchange_id), \
                   request_outbound_sha256=COALESCE(?3, request_outbound_sha256), \
                   redirect_chain_json=COALESCE(?4, redirect_chain_json), \
                   response_status=COALESCE(?5, response_status), \
                   response_body_sha256=COALESCE(?6, response_body_sha256), \
                   response_body_len=COALESCE(?7, response_body_len), \
                   response_mime=COALESCE(?8, response_mime), \
                   remote_ip=COALESCE(?9, remote_ip), \
                   started_at=COALESCE(?10, started_at), \
                   completed_at=COALESCE(?11, completed_at), \
                   duration_ms=COALESCE(?12, duration_ms), \
                   ttfb_ms=COALESCE(?13, ttfb_ms), \
                   error_code=COALESCE(?14, error_code), \
                   state=?1 \
                 WHERE send_id=?15 AND state IN ('pending','sending')",
                sql_params![
                    patch.state,
                    patch.exchange_id,
                    patch.request_outbound_sha256,
                    patch.redirect_chain_json,
                    patch.response_status,
                    patch.response_body_sha256,
                    patch.response_body_len,
                    patch.response_mime,
                    patch.remote_ip,
                    patch.started_at,
                    patch.completed_at,
                    patch.duration_ms,
                    patch.ttfb_ms,
                    patch.error_code,
                    send_id
                ],
            )
        })?;
        Ok(n == 1)
    }

    /// 取 send 行（状态轮询面）。
    pub fn get_send(&self, send_id: &str) -> DbResult<Option<SendRow>> {
        self.pool.with_conn(|conn| -> DbResult<Option<SendRow>> {
            let mut stmt = conn.prepare(
                "SELECT send_id, tab_id, version_no, session_id, exchange_id, state, \
                        request_outbound_sha256, redirect_chain_json, response_status, \
                        response_body_sha256, response_body_len, response_mime, remote_ip, \
                        started_at, completed_at, duration_ms, ttfb_ms, error_code, created_at \
                 FROM wsw_repeater_send WHERE send_id=?1",
            )?;
            let mut rows = stmt.query_map(sql_params![send_id], map_send_row)?;
            Ok(rows.next().transpose()?)
        })
    }

    /// send 历史（tab 维度；created_at DESC）。
    pub fn list_sends(&self, tab_id: &str, limit: i64) -> DbResult<Vec<SendRow>> {
        self.pool.with_conn(|conn| -> DbResult<Vec<SendRow>> {
            let mut stmt = conn.prepare(
                "SELECT send_id, tab_id, version_no, session_id, exchange_id, state, \
                        request_outbound_sha256, redirect_chain_json, response_status, \
                        response_body_sha256, response_body_len, response_mime, remote_ip, \
                        started_at, completed_at, duration_ms, ttfb_ms, error_code, created_at \
                 FROM wsw_repeater_send WHERE tab_id=?1 ORDER BY created_at DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(sql_params![tab_id, limit.min(500)], map_send_row)?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
    }

    /// session 取行（send 面 scope 规则回读）。
    pub fn get_session(&self, session_id: &str) -> DbResult<Option<SessionRow>> {
        self.pool.with_conn(|conn| -> DbResult<Option<SessionRow>> {
            let mut stmt = conn.prepare(
                "SELECT session_id, title, scope_rules_json, out_of_scope_policy, created_at, updated_at \
                 FROM wsw_session WHERE session_id=?1",
            )?;
            let mut rows = stmt.query_map(sql_params![session_id], |r| {
                Ok(SessionRow {
                    session_id: r.get(0)?,
                    title: r.get(1)?,
                    scope_rules_json: r.get(2)?,
                    out_of_scope_policy: r.get(3)?,
                    created_at: r.get(4)?,
                    updated_at: r.get(5)?,
                })
            })?;
            Ok(rows.next().transpose()?)
        })
    }
}

fn map_tab_row(row: &database::rusqlite::Row<'_>) -> database::rusqlite::Result<TabRow> {
    Ok(TabRow {
        tab_id: row.get(0)?,
        session_id: row.get(1)?,
        title: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
        closed_at: row.get(5)?,
    })
}

fn map_version_row(
    row: &database::rusqlite::Row<'_>,
) -> database::rusqlite::Result<TabVersionRow> {
    Ok(TabVersionRow {
        tab_id: row.get(0)?,
        version_no: row.get(1)?,
        request_json: row.get(2)?,
        source_exchange_id: row.get(3)?,
        origin: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn map_send_row(row: &database::rusqlite::Row<'_>) -> database::rusqlite::Result<SendRow> {
    Ok(SendRow {
        send_id: row.get(0)?,
        tab_id: row.get(1)?,
        version_no: row.get(2)?,
        session_id: row.get(3)?,
        exchange_id: row.get(4)?,
        state: row.get(5)?,
        request_outbound_sha256: row.get(6)?,
        redirect_chain_json: row.get(7)?,
        response_status: row.get(8)?,
        response_body_sha256: row.get(9)?,
        response_body_len: row.get(10)?,
        response_mime: row.get(11)?,
        remote_ip: row.get(12)?,
        started_at: row.get(13)?,
        completed_at: row.get(14)?,
        duration_ms: row.get(15)?,
        ttfb_ms: row.get(16)?,
        error_code: row.get(17)?,
        created_at: row.get(18)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(event: &str, queued_ms: i64, ttl_ms: i64) -> InterceptRow {
        InterceptRow {
            intercept_id: generate_intercept_id(),
            event_id: event.to_string(),
            listener_id: "wsw-lis-test-0001".to_string(),
            session_id: "wsw-ses-test-0001".to_string(),
            exchange_id: Some("wsw-20261119000000-0-ab12".to_string()),
            direction: InterceptDirection::Request,
            state: InterceptState::Queued,
            host: Some("127.0.0.1".to_string()),
            method: Some("GET".to_string()),
            path: Some("/p59c-marker".to_string()),
            queued_at: epoch_ms_to_iso(queued_ms),
            expires_at: epoch_ms_to_iso(queued_ms + ttl_ms),
            resolved_at: None,
            reason: None,
        }
    }

    fn store() -> PersistStore {
        let s = PersistStore::open_memory().expect("open memory");
        s.run_migrations(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../migrations")
                .as_path(),
        )
        .expect("migrate 001..025");
        s
    }

    #[test]
    fn enqueue_resolve_roundtrip() {
        let s = store();
        let r = row("ev-1", 1_000, 60_000);
        s.enqueue_intercept(&r).expect("enqueue");
        assert_eq!(s.intercept_count().unwrap(), 1);
        assert_eq!(s.list_queued().unwrap().len(), 1);
        // 终态推进：queued → forwarded（SQL WHERE state='queued' 强制）。
        assert!(s
            .resolve_intercept(
                &r.intercept_id,
                InterceptState::Forwarded,
                &epoch_ms_to_iso(2_000),
                None
            )
            .unwrap());
        // 二次推进 = false（AlreadyResolved 语义）。
        assert!(!s
            .resolve_intercept(
                &r.intercept_id,
                InterceptState::Dropped,
                &epoch_ms_to_iso(3_000),
                None
            )
            .unwrap());
        let all = s.list_all_intercepts().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].state, InterceptState::Forwarded);
    }

    #[test]
    fn reconcile_reloads_unexpired_and_expires_stale() {
        let s = store();
        let fresh = row("ev-fresh", 10_000, 600_000);
        let stale = row("ev-stale", 1_000, 5_000); // expires 6_000 < 对账时刻 20_000
        s.enqueue_intercept(&fresh).unwrap();
        s.enqueue_intercept(&stale).unwrap();
        let outcome = s
            .reconcile_on_start(&epoch_ms_to_iso(20_000), "reconcile_expired")
            .unwrap();
        // 未过期回载（queued_at+ttl 重算归调用面）+ 过期→expired。
        assert_eq!(outcome.reloaded.len(), 1);
        assert_eq!(outcome.reloaded[0].event_id, "ev-fresh");
        assert_eq!(outcome.expired_count, 1);
        let all = s.list_all_intercepts().unwrap();
        let stale_row = all.iter().find(|r| r.event_id == "ev-stale").unwrap();
        assert_eq!(stale_row.state, InterceptState::Expired);
        assert_eq!(stale_row.reason.as_deref(), Some("reconcile_expired"));
    }

    #[test]
    fn stop_expires_all_queued() {
        let s = store();
        for i in 0..3 {
            s.enqueue_intercept(&row(&format!("ev-{i}"), 1_000 + i, 600_000))
                .unwrap();
        }
        let n = s
            .expire_all_queued(&epoch_ms_to_iso(99_000), "listener_stopped")
            .unwrap();
        assert_eq!(n, 3);
        assert!(s.list_queued().unwrap().is_empty());
        for r in s.list_all_intercepts().unwrap() {
            assert_eq!(r.state, InterceptState::Expired);
        }
    }

    #[test]
    fn duplicate_event_id_rejected() {
        let s = store();
        let mut r = row("ev-dup", 1_000, 60_000);
        s.enqueue_intercept(&r).unwrap();
        r.intercept_id = generate_intercept_id();
        // 同 event_id 二次入队（幂等键 UNIQUE）必须报错，不静默。
        assert!(s.enqueue_intercept(&r).is_err());
    }

    #[test]
    fn iso_roundtrip_and_ordering() {
        let t1 = epoch_ms_to_iso(0);
        assert_eq!(t1, "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_to_epoch_ms(&t1), Some(0));
        let t2 = epoch_ms_to_iso(1_732_000_000_123);
        assert_eq!(iso_to_epoch_ms(&t2), Some(1_732_000_000_123));
        assert!(t1 < t2, "fixed-width ISO strings order lexicographically");
        assert_eq!(iso_to_epoch_ms("2026-11-19 00:00:00"), None);
        let a = epoch_ms_to_iso(1);
        let b = epoch_ms_to_iso(2);
        assert!(a < b);
    }

    #[test]
    fn listener_and_session_validation() {
        let s = store();
        let lr = ListenerRow {
            listener_id: "wsw-lis-test-0001".to_string(),
            name: "e2e".to_string(),
            bind: "127.0.0.1:3128".to_string(),
            state: ListenerState::Running,
            config_json: "{}".to_string(),
            created_at: now_iso(),
            updated_at: now_iso(),
        };
        s.register_listener(&lr).unwrap();
        let sr = SessionRow {
            session_id: "wsw-ses-test-0001".to_string(),
            title: "e2e session".to_string(),
            scope_rules_json: r#"{"include":["127.0.0.1"],"allow_private_targets":true}"#
                .to_string(),
            out_of_scope_policy: "blocked".to_string(),
            created_at: now_iso(),
            updated_at: now_iso(),
        };
        s.register_session(&sr).unwrap();
        // 非 loopback bind 拒（后端双保险）。
        let bad = ListenerRow {
            bind: "0.0.0.0:3128".to_string(),
            ..lr.clone()
        };
        assert!(s.register_listener(&bad).is_err());
        // 非法策略词表拒。
        let bad_policy = SessionRow {
            out_of_scope_policy: "drop_all".to_string(),
            ..sr.clone()
        };
        assert!(s.register_session(&bad_policy).is_err());
    }
}
