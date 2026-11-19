//! 024 三表持久化 store（WSW-2-ADR W2-D4/D6；`migrations/024_wsw_intercept.sql`）。
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
//!
//! 纪律：rusqlite 经 `database` 重导出使用不直连；迁移执行器唯一为
//! `database::migrate`；只在测试内存库与开发库副本执行（`platform*.db`
//! 生产文件零触碰，023 文件头 :30 纪律同源）。

use crate::{InterceptState, ListenerState};
use database::{DbError, DbPath, DbResult, Pool, PoolConfig};
use std::path::{Path, PathBuf};
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

/// 024 三表 store（Pool 包裹；rusqlite 经 database 重导出）。
#[derive(Clone)]
pub struct PersistStore {
    pool: Pool,
    /// 库路径（内存库 None）。
    path: Option<PathBuf>,
}

impl PersistStore {
    /// 文件库打开（调用面随后必须 `run_migrations`）。
    pub fn open_file(path: &Path) -> DbResult<Self> {
        Ok(Self {
            pool: Pool::open(PoolConfig::writable_file(path))?,
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

    /// 迁移执行（唯一入口 `database::migrate`；024 后 user_version=24）。
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
        .expect("migrate 001..024");
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
