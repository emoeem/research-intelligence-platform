//! 协作协议存储面（MA-3，029 两表）：[`ClaimStore`]（TaskClaim 五态 CAS
//! 队列）+ [`MessageStore`]（ma_message 邮箱）。
//!
//! 设计裁决落点（MA-3-TASK.md §0，语义以裁决原文为准）：
//! - D2 事务化原子领取 = 单行 CAS 单语句（`UPDATE … WHERE claim_id=? AND
//!   state='claimable' [AND 依赖子查询] RETURNING …`）经
//!   `Pool::with_conn` autocommit 执行——SQLite WAL 单写者串行化使单语句
//!   原子性与 BEGIN IMMEDIATE 等价（pool.rs:172 `with_tx` 是 DEFERRED，
//!   显式 IMMEDIATE 须绕过 Pool 事务封装破坏既有纪律，故不用）；命中
//!   1 行即领取成功并同语句回读，败方 0 行零副作用（`QueryReturnedNoRows`
//!   判定后回查现态给出具体错误）。依赖门（dsh R1「依赖未完成的任务不可
//!   领取」）内联同语句：`AND (depends_on_claim_id IS NULL OR EXISTS(SELECT 1
//!   FROM task_claim dep WHERE dep.claim_id = task_claim.depends_on_claim_id
//!   AND dep.state='completed'))`。
//! - 转派/接管（R3）：原 claimer 才可转（CAS 带 `AND state='claimed' AND
//!   claimed_by=?`），transferred 为终态禁写 result（旧 attempt 结果作废
//!   位）；`with_tx` 内两语句：旧行转 transferred + 插入新 claimable 行
//!   （attempt = 旧行 + 1，claimed_by NULL）——竞争失败方对新行再 CAS
//!   领取即「败方接新 attempt」。
//! - 合并/完成（dsh 弱点 3「引擎侧记账」反超）：`complete`/`fail` 仅
//!   claimed_by 本人可写（CAS 带 `AND claimed_by=?`），result/error 落行 +
//!   终态——成员无法伪造他人认领行的状态跃迁。
//! - D3 邮箱消息：投递/读取分记（delivered_at 与 read_at 两列）；投递 =
//!   收件方 fetch 时单语句 `UPDATE … SET delivered_at=? WHERE message_id=?
//!   AND to_caller=? AND delivered_at IS NULL`（首拉落记不覆盖）；读取 =
//!   read 标记单语句 CAS（read_at IS NULL 守卫）；重投 = 拉模型天然持久
//!   ——未读行（read_at NULL）每次 fetch 均返回，read 确认后不再返回；
//!   sweep = 只读未投递行清单/计数（MA-4 SSE 推送面通知源输入，推送通道
//!   本批零实现）。
//! - D7：claim 创建面 = 编排方显式登记（rp-collab CLI / MCP），workflow
//!   执行器 dispatch_node 零改动（零引擎自动建 claim）；create 时递归
//!   CTE 环检测 fail-closed（依赖链深度帽 64 超限即拒 + 自依赖显式拒）。
//!
//! 词形占用：claim_id `tcl-`、message_id `mam-`（029 CHECK 同源；MA-3 前
//! 全仓零命中，A21）。时间戳 = ISO8601 UTC 带毫秒固定宽（029 GLOB 口径，
//! 字典序 = 时间序；wsw-runtime persist.rs 同款历法换算，本 crate 内置
//! 避免跨域依赖）。
//!
//! 闭包错误纪律：`with_conn`/`with_tx` 闭包错误类型须满足
//! `DbError: From<E>`，语义错误以嵌套 `Result`（外层 DbError = 纯存储
//! 失败，内层 [`CollabError`] = 业务语义）出闭包后展平，不在闭包内丢形。

use database::{rusqlite, DbError, Pool};
use rusqlite::OptionalExtension;

/// 协作面 Result 别名。
pub type CollabResult<T> = Result<T, CollabError>;

/// 协作面错误（rp-collab CLI exit 码映射见 bin；存储错误 = DbError 原文携带）。
#[derive(Debug, Clone, PartialEq)]
pub enum CollabError {
    /// claim/message 行不存在。
    NotFound(String),
    /// 行存在但现态不可达成（CAS 败方/终态行/已读消息），现态随错误携带。
    NotClaimable {
        /// 行 id（claim_id 或 message_id）。
        claim_id: String,
        /// 现态词形（或 already_read_or_not_recipient 判定词形）。
        state: String,
    },
    /// 领取失败：依赖门未过（depends_on 行未 completed）。
    DependencyNotCompleted(String),
    /// 写权拒：complete/fail/transfer 调用方不是该行 claimed_by 本人。
    NotClaimer(String),
    /// 创建拒：依赖链环检测命中（自依赖或链深超帽）。
    CycleDetected(String),
    /// 输入非法（形状/词表/JSON 解析/未知或吊销 caller 等）。
    InvalidInput(String),
    /// 底层存储错误（DbError 原文）。
    Storage(String),
}

impl std::fmt::Display for CollabError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CollabError::NotFound(r) => write!(f, "collab row not found: {r}"),
            CollabError::NotClaimable { claim_id, state } => {
                write!(f, "row {claim_id} not actionable (state={state})")
            }
            CollabError::DependencyNotCompleted(r) => {
                write!(f, "claim dependency not completed: {r}")
            }
            CollabError::NotClaimer(r) => write!(f, "caller is not the claimer: {r}"),
            CollabError::CycleDetected(r) => write!(f, "claim dependency cycle detected: {r}"),
            CollabError::InvalidInput(r) => write!(f, "collab input invalid: {r}"),
            CollabError::Storage(r) => write!(f, "collab storage error: {r}"),
        }
    }
}

impl std::error::Error for CollabError {}

impl From<DbError> for CollabError {
    fn from(value: DbError) -> Self {
        CollabError::Storage(value.to_string())
    }
}

impl From<rusqlite::Error> for CollabError {
    fn from(value: rusqlite::Error) -> Self {
        CollabError::Storage(DbError::Sqlite(value).to_string())
    }
}

/// 当前时刻 → ISO8601 UTC 带毫秒（固定宽 `.mmmZ`，029 GLOB 口径；
/// wsw-runtime persist.rs 同款纯整数历法换算，零外部依赖）。
fn now_iso_ms() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let epoch_ms = d.as_millis() as i64;
    let secs = epoch_ms.div_euclid(1000);
    let ms = epoch_ms.rem_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}.{ms:03}Z")
}

/// claim id（`tcl-` 前缀；exec::new_id 同源口径：时间戳 + 进程熵 + 进内序号）。
fn new_claim_id() -> String {
    crate::exec::new_id("tcl")
}

/// message id（`mam-` 前缀；同上）。
fn new_message_id() -> String {
    crate::exec::new_id("mam")
}

/// claim kind 词表（029 CHECK 两值逐字；MA-4 扩词值须升 030+ 不回改）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimKind {
    /// agent 子任务（外部 AI / 成员认领执行）。
    AgentSubtask,
    /// review 任务（复用 wf-review-card-v1 审核工作流形态与 rp-kc review
    /// 双闸，不自造复审体系——D7 R6 最小落法）。
    Review,
}

impl ClaimKind {
    /// 029 CHECK 词形。
    pub fn as_str(self) -> &'static str {
        match self {
            ClaimKind::AgentSubtask => "agent_subtask",
            ClaimKind::Review => "review",
        }
    }

    /// 词形解析（词表外拒绝）。
    pub fn parse(raw: &str) -> Result<Self, CollabError> {
        match raw {
            "agent_subtask" => Ok(ClaimKind::AgentSubtask),
            "review" => Ok(ClaimKind::Review),
            other => Err(CollabError::InvalidInput(format!(
                "unknown claim kind {other:?} (agent_subtask/review)"
            ))),
        }
    }
}

/// claim 五态词表（029 CHECK 逐字；跃迁全部 CAS 强制，本枚举只承载读回）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimState {
    /// 可领取（依赖门过的 claimable 行进 ready_list）。
    Claimable,
    /// 已领取（claimed_by 在位）。
    Claimed,
    /// 已完成（result_json 在位）。
    Completed,
    /// 已失败（error_json 在位）。
    Failed,
    /// 已转派（终态，旧 attempt 结果作废位；新 attempt 行另建）。
    Transferred,
}

impl ClaimState {
    /// 029 CHECK 词形。
    pub fn as_str(self) -> &'static str {
        match self {
            ClaimState::Claimable => "claimable",
            ClaimState::Claimed => "claimed",
            ClaimState::Completed => "completed",
            ClaimState::Failed => "failed",
            ClaimState::Transferred => "transferred",
        }
    }

    /// 词形解析（词表外拒绝）。
    pub fn parse(raw: &str) -> Result<Self, CollabError> {
        match raw {
            "claimable" => Ok(ClaimState::Claimable),
            "claimed" => Ok(ClaimState::Claimed),
            "completed" => Ok(ClaimState::Completed),
            "failed" => Ok(ClaimState::Failed),
            "transferred" => Ok(ClaimState::Transferred),
            other => Err(CollabError::InvalidInput(format!(
                "unknown claim state {other:?} (029 CHECK 词表五态)"
            ))),
        }
    }
}

/// claim 行投影（029 task_claim 十八列全量读回）。
#[derive(Debug, Clone, PartialEq)]
pub struct ClaimRow {
    /// claim id（`tcl-` 前缀）。
    pub claim_id: String,
    /// kind 词形（agent_subtask/review）。
    pub kind: ClaimKind,
    /// 人读标题。
    pub title: String,
    /// 任务载荷 JSON 文本（≤1MiB，029 CHECK）。
    pub payload_json: String,
    /// 关联 workflow run（可空引用列）。
    pub run_id: Option<String>,
    /// 关联节点 id（可空引用列）。
    pub node_id: Option<String>,
    /// 关联 agent 定义 id（可空引用列）。
    pub agent_id: Option<String>,
    /// 依赖 claim（可空自引用；依赖门 = 该行 completed 才可领）。
    pub depends_on_claim_id: Option<String>,
    /// 五态现态。
    pub state: ClaimState,
    /// 认领人 caller（NULL = 未领取）。
    pub claimed_by: Option<String>,
    /// attempt 计数（首建 1；transfer 新行 +1）。
    pub attempt: u32,
    /// completed 终态载荷。
    pub result_json: Option<String>,
    /// failed 终态载荷。
    pub error_json: Option<String>,
    /// 创建人 caller。
    pub created_by: String,
    /// 创建时刻（ISO8601 带毫秒）。
    pub created_at: String,
    /// 领取时刻。
    pub claimed_at: Option<String>,
    /// 终态时刻。
    pub finished_at: Option<String>,
    /// 最近更新时刻。
    pub updated_at: String,
}

/// claim 创建入参（D7：编排方显式登记；kind/title/payload/created_by 必填，
/// 引用列可空）。
#[derive(Debug, Clone)]
pub struct ClaimCreateInput {
    /// kind 词表值。
    pub kind: ClaimKind,
    /// 人读标题（非空 ≤512）。
    pub title: String,
    /// 任务载荷 JSON（合法 JSON 文本 ≤1MiB）。
    pub payload_json: String,
    /// 关联 workflow run（`wfr-` 词形守卫）。
    pub run_id: Option<String>,
    /// 关联节点 id（≤128）。
    pub node_id: Option<String>,
    /// 关联 agent 定义 id（≤128）。
    pub agent_id: Option<String>,
    /// 依赖 claim（存在性 + 环检测 fail-closed）。
    pub depends_on_claim_id: Option<String>,
    /// 创建人 caller（必须存在且在用——029 真 FK + 校验面双保险）。
    pub created_by: String,
}

/// TaskClaim 存储面（database::Pool 复用；写入唯一入口——D7 编排方显式
/// 登记 + 判据 4/5 的 CAS 原语承载）。
#[derive(Debug, Clone)]
pub struct ClaimStore {
    pool: Pool,
}

const CLAIM_COLUMNS: &str = "claim_id, kind, title, payload_json, run_id, node_id, agent_id, \
     depends_on_claim_id, state, claimed_by, attempt, result_json, error_json, \
     created_by, created_at, claimed_at, finished_at, updated_at";

const DEPENDENCY_GATE: &str = "(depends_on_claim_id IS NULL \
       OR EXISTS(SELECT 1 FROM task_claim dep \
                 WHERE dep.claim_id = task_claim.depends_on_claim_id \
                   AND dep.state = 'completed'))";

/// 029 task_claim 行元组（与 CLAIM_COLUMNS 列序一一对应）。
type ClaimDbRow = (
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
    i64,
    Option<String>,
    Option<String>,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
);

fn row_to_claim(row: ClaimDbRow) -> CollabResult<ClaimRow> {
    Ok(ClaimRow {
        claim_id: row.0,
        kind: ClaimKind::parse(&row.1)?,
        title: row.2,
        payload_json: row.3,
        run_id: row.4,
        node_id: row.5,
        agent_id: row.6,
        depends_on_claim_id: row.7,
        state: ClaimState::parse(&row.8)?,
        claimed_by: row.9,
        attempt: row
            .10
            .try_into()
            .map_err(|_| CollabError::InvalidInput(format!("attempt {} out of range", row.10)))?,
        result_json: row.11,
        error_json: row.12,
        created_by: row.13,
        created_at: row.14,
        claimed_at: row.15,
        finished_at: row.16,
        updated_at: row.17,
    })
}

/// 闭包内读回一行 claim（列序固定；调用方须保证行存在）。
fn query_claim_row(
    c: &rusqlite::Connection,
    sql_where: &str,
    param: &str,
) -> rusqlite::Result<ClaimDbRow> {
    c.query_row(
        &format!("SELECT {CLAIM_COLUMNS} FROM task_claim WHERE {sql_where}"),
        [param],
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
                r.get(10)?,
                r.get(11)?,
                r.get(12)?,
                r.get(13)?,
                r.get(14)?,
                r.get(15)?,
                r.get(16)?,
                r.get(17)?,
            ))
        },
    )
}

impl ClaimStore {
    /// 构造（调用方先行 database::migrate 保证 029 表存在）。
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// caller 在用判定（FK 只保证存在；吊销 caller 不可认领/创建/收发——
    /// 026 revoked_at NULL = 在用口径，fail-closed）。
    fn caller_in_use(c: &rusqlite::Connection, caller_id: &str) -> Result<bool, DbError> {
        let revoked: Option<Option<String>> = c
            .query_row(
                "SELECT revoked_at FROM caller WHERE caller_id = ?1",
                [caller_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(matches!(revoked, Some(None)))
    }

    /// 依赖链环检测（递归 CTE 自 dep 沿链上溯；深度帽 64 超限即判环拒
    /// ——fail-closed；自依赖在建面显式拒）。Err = 环检测命中。
    fn check_dependency_chain(c: &rusqlite::Connection, dep: &str) -> Result<(), CollabError> {
        let max_depth: i64 = c
            .query_row(
                "WITH RECURSIVE chain(claim_id, dep, depth) AS ( \
                   SELECT claim_id, depends_on_claim_id, 1 FROM task_claim WHERE claim_id = ?1 \
                   UNION ALL \
                   SELECT tc.claim_id, tc.depends_on_claim_id, chain.depth + 1 \
                     FROM task_claim tc JOIN chain ON chain.dep = tc.claim_id \
                    WHERE chain.depth < 64 \
                 ) SELECT COALESCE(MAX(depth), 0) FROM chain",
                [dep],
                |r| r.get(0),
            )
            .map_err(CollabError::from)?;
        if max_depth >= 64 {
            return Err(CollabError::CycleDetected(format!(
                "dependency chain from {dep} exceeds depth cap 64 (cycle suspected)"
            )));
        }
        Ok(())
    }

    /// 建 claim（state=claimable，attempt=1；依赖存在性 + 环检测 +
    /// 创建人在用三 fail-closed；JSON 载荷解析守卫）。
    pub fn create(&self, input: &ClaimCreateInput) -> CollabResult<ClaimRow> {
        if input.title.trim().is_empty() || input.title.len() > 512 {
            return Err(CollabError::InvalidInput(
                "title must be non-empty and <= 512 chars".to_string(),
            ));
        }
        if input.payload_json.len() > 1024 * 1024 {
            return Err(CollabError::InvalidInput(
                "payload_json exceeds 1MiB cap".to_string(),
            ));
        }
        serde_json::from_str::<serde_json::Value>(&input.payload_json).map_err(|e| {
            CollabError::InvalidInput(format!("payload_json is not valid JSON: {e}"))
        })?;
        if input.created_by.trim().is_empty() {
            return Err(CollabError::InvalidInput(
                "created_by caller required".to_string(),
            ));
        }
        if let Some(dep) = &input.depends_on_claim_id {
            if !dep.starts_with("tcl-") {
                return Err(CollabError::InvalidInput(format!(
                    "depends_on_claim_id {dep:?} must be a tcl- claim id"
                )));
            }
        }
        if let Some(run) = &input.run_id {
            if !run.starts_with("wfr-") {
                return Err(CollabError::InvalidInput(format!(
                    "run_id {run:?} must be a wfr- run id"
                )));
            }
        }
        let claim_id = new_claim_id();
        if input
            .depends_on_claim_id
            .as_deref()
            .map(|d| d == claim_id)
            .unwrap_or(false)
        {
            return Err(CollabError::CycleDetected(
                "claim cannot depend on itself".to_string(),
            ));
        }
        let now = now_iso_ms();
        let outcome: CollabResult<ClaimDbRow> = self
            .pool
            .with_conn(|c| -> Result<CollabResult<ClaimDbRow>, DbError> {
                if !Self::caller_in_use(c, &input.created_by)? {
                    return Ok(Err(CollabError::InvalidInput(format!(
                        "created_by caller {:?} unknown or revoked",
                        input.created_by
                    ))));
                }
                if let Some(dep) = &input.depends_on_claim_id {
                    let exists: i64 = c.query_row(
                        "SELECT COUNT(*) FROM task_claim WHERE claim_id = ?1",
                        [dep],
                        |r| r.get(0),
                    )?;
                    if exists == 0 {
                        return Ok(Err(CollabError::InvalidInput(format!(
                            "depends_on claim {dep} not found"
                        ))));
                    }
                    if let Err(e) = Self::check_dependency_chain(c, dep) {
                        return Ok(Err(e));
                    }
                }
                c.execute(
                    "INSERT INTO task_claim (claim_id, kind, title, payload_json, run_id, \
                     node_id, agent_id, depends_on_claim_id, state, claimed_by, attempt, \
                     result_json, error_json, created_by, created_at, claimed_at, \
                     finished_at, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'claimable', NULL, 1, \
                     NULL, NULL, ?9, ?10, NULL, NULL, ?10)",
                    rusqlite::params![
                        claim_id,
                        input.kind.as_str(),
                        input.title,
                        input.payload_json,
                        input.run_id,
                        input.node_id,
                        input.agent_id,
                        input.depends_on_claim_id,
                        input.created_by,
                        now,
                    ],
                )?;
                Ok(Ok(query_claim_row(c, "claim_id = ?1", &claim_id)?))
            })
            .map_err(CollabError::from)?;
        row_to_claim(outcome?)
    }

    /// ready_list（依赖门过滤读面：state=claimable 且依赖门过的行；
    /// created_at 升序）。
    pub fn ready_list(&self, limit: usize) -> CollabResult<Vec<ClaimRow>> {
        let rows: Vec<ClaimDbRow> = self
            .pool
            .with_conn(|c| -> Result<Vec<ClaimDbRow>, DbError> {
                let mut stmt = c.prepare(&format!(
                    "SELECT {CLAIM_COLUMNS} FROM task_claim \
                     WHERE state = 'claimable' AND {DEPENDENCY_GATE} \
                     ORDER BY created_at, claim_id LIMIT ?1"
                ))?;
                let rows = stmt
                    .query_map([limit.min(1000) as i64], |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                            r.get(7)?,
                            r.get(8)?,
                            r.get(9)?,
                            r.get(10)?,
                            r.get(11)?,
                            r.get(12)?,
                            r.get(13)?,
                            r.get(14)?,
                            r.get(15)?,
                            r.get(16)?,
                            r.get(17)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .map_err(CollabError::from)?;
        rows.into_iter().map(row_to_claim).collect()
    }

    /// 事务化原子领取（D2 单行 CAS 单语句；胜方恰一，败方 0 行零副作用）。
    /// 依赖门内联同语句（dsh R1）。
    pub fn claim(&self, claim_id: &str, caller: &str) -> CollabResult<ClaimRow> {
        let now = now_iso_ms();
        let outcome: CollabResult<ClaimDbRow> = self
            .pool
            .with_conn(|c| -> Result<CollabResult<ClaimDbRow>, DbError> {
                if !Self::caller_in_use(c, caller)? {
                    return Ok(Err(CollabError::InvalidInput(format!(
                        "claiming caller {caller:?} unknown or revoked"
                    ))));
                }
                match c.query_row(
                    &format!(
                        "UPDATE task_claim SET state = 'claimed', claimed_by = ?2, \
                         claimed_at = ?3, updated_at = ?3 \
                         WHERE claim_id = ?1 AND state = 'claimable' AND {DEPENDENCY_GATE} \
                         RETURNING {CLAIM_COLUMNS}"
                    ),
                    rusqlite::params![claim_id, caller, now],
                    |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                            r.get(7)?,
                            r.get(8)?,
                            r.get(9)?,
                            r.get(10)?,
                            r.get(11)?,
                            r.get(12)?,
                            r.get(13)?,
                            r.get(14)?,
                            r.get(15)?,
                            r.get(16)?,
                            r.get(17)?,
                        ))
                    },
                ) {
                    Ok(row) => Ok(Ok(row)),
                    Err(rusqlite::Error::QueryReturnedNoRows) => {
                        Ok(Err(Self::explain_claim_failure(c, claim_id)))
                    }
                    Err(e) => Err(e.into()),
                }
            })
            .map_err(CollabError::from)?;
        row_to_claim(outcome?)
    }

    /// CAS 败方回查现态给具体错误（NotClaimable / DependencyNotCompleted /
    /// NotFound；0 行零副作用的判定面）。
    fn explain_claim_failure(c: &rusqlite::Connection, claim_id: &str) -> CollabError {
        let res: Result<(Option<String>, Option<String>), rusqlite::Error> = c.query_row(
            "SELECT state, depends_on_claim_id FROM task_claim WHERE claim_id = ?1",
            [claim_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        );
        match res {
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                CollabError::NotFound(format!("claim {claim_id} not found"))
            }
            Err(e) => CollabError::from(e),
            Ok((state, dep)) => {
                let state = state.unwrap_or_default();
                if state == "claimable" {
                    if let Some(dep) = dep {
                        let dep_state: Option<String> = c
                            .query_row(
                                "SELECT state FROM task_claim WHERE claim_id = ?1",
                                [&dep],
                                |r| r.get(0),
                            )
                            .ok();
                        if dep_state.as_deref() != Some("completed") {
                            return CollabError::DependencyNotCompleted(dep);
                        }
                    }
                }
                CollabError::NotClaimable {
                    claim_id: claim_id.to_string(),
                    state,
                }
            }
        }
    }

    /// 完成（仅 claimed_by 本人可写；CAS 带 `AND claimed_by=?`——成员无法
    /// 伪造他人认领行的状态跃迁，dsh 弱点 3 引擎侧记账反超）。result_json
    /// 必须是合法 JSON。
    pub fn complete(
        &self,
        claim_id: &str,
        caller: &str,
        result_json: &str,
    ) -> CollabResult<ClaimRow> {
        serde_json::from_str::<serde_json::Value>(result_json).map_err(|e| {
            CollabError::InvalidInput(format!("result_json is not valid JSON: {e}"))
        })?;
        self.cas_to_terminal(claim_id, caller, "completed", Some(result_json), None)
    }

    /// 失败（仅 claimed_by 本人可写；error_json 必须是合法 JSON）。
    pub fn fail(&self, claim_id: &str, caller: &str, error_json: &str) -> CollabResult<ClaimRow> {
        serde_json::from_str::<serde_json::Value>(error_json)
            .map_err(|e| CollabError::InvalidInput(format!("error_json is not valid JSON: {e}")))?;
        self.cas_to_terminal(claim_id, caller, "failed", None, Some(error_json))
    }

    /// claimed → 终态（completed/failed）共用 CAS：`AND state='claimed'
    /// AND claimed_by=?`；0 行回查区分 NotClaimer / NotClaimable / NotFound。
    fn cas_to_terminal(
        &self,
        claim_id: &str,
        caller: &str,
        to: &str,
        result_json: Option<&str>,
        error_json: Option<&str>,
    ) -> CollabResult<ClaimRow> {
        let now = now_iso_ms();
        let outcome: CollabResult<ClaimDbRow> = self
            .pool
            .with_conn(|c| -> Result<CollabResult<ClaimDbRow>, DbError> {
                let updated = c.execute(
                    "UPDATE task_claim SET state = ?2, result_json = ?3, error_json = ?4, \
                     finished_at = ?5, updated_at = ?5 \
                     WHERE claim_id = ?1 AND state = 'claimed' AND claimed_by = ?6",
                    rusqlite::params![claim_id, to, result_json, error_json, now, caller],
                )?;
                if updated != 1 {
                    return Ok(Err(Self::explain_terminal_failure(c, claim_id, caller)));
                }
                Ok(Ok(query_claim_row(c, "claim_id = ?1", claim_id)?))
            })
            .map_err(CollabError::from)?;
        row_to_claim(outcome?)
    }

    /// 终态 CAS 败方回查（他人写 = NotClaimer；态错 = NotClaimable；无行 =
    /// NotFound）。
    fn explain_terminal_failure(
        c: &rusqlite::Connection,
        claim_id: &str,
        caller: &str,
    ) -> CollabError {
        let res: Result<(Option<String>, Option<String>), rusqlite::Error> = c.query_row(
            "SELECT state, claimed_by FROM task_claim WHERE claim_id = ?1",
            [claim_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        );
        match res {
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                CollabError::NotFound(format!("claim {claim_id} not found"))
            }
            Err(e) => CollabError::from(e),
            Ok((state, claimed_by)) => {
                let state = state.unwrap_or_default();
                if state != "claimed" {
                    return CollabError::NotClaimable {
                        claim_id: claim_id.to_string(),
                        state,
                    };
                }
                CollabError::NotClaimer(format!(
                    "claim {claim_id} is claimed by {:?}, not {caller:?}",
                    claimed_by.unwrap_or_default()
                ))
            }
        }
    }

    /// 转派/接管（R3）：仅 claimed_by 本人可转；`with_tx` 两语句——旧行
    /// claimed→transferred 终态（旧 attempt 结果作废位，禁写 result）+
    /// 插入新 claimable 行（attempt+1，claimed_by NULL）。返回新行（败方
    /// 对新行再 CAS 领取）。
    pub fn transfer(&self, claim_id: &str, caller: &str) -> CollabResult<ClaimRow> {
        let now = now_iso_ms();
        let new_id = new_claim_id();
        let outcome: CollabResult<ClaimDbRow> = self
            .pool
            .with_tx(|c| -> Result<CollabResult<ClaimDbRow>, DbError> {
                let updated = c.execute(
                    "UPDATE task_claim SET state = 'transferred', updated_at = ?3 \
                     WHERE claim_id = ?1 AND state = 'claimed' AND claimed_by = ?2",
                    rusqlite::params![claim_id, caller, now],
                )?;
                if updated != 1 {
                    return Ok(Err(Self::explain_terminal_failure(c, claim_id, caller)));
                }
                let copied = c.execute(
                    "INSERT INTO task_claim (claim_id, kind, title, payload_json, run_id, \
                     node_id, agent_id, depends_on_claim_id, state, claimed_by, attempt, \
                     result_json, error_json, created_by, created_at, claimed_at, \
                     finished_at, updated_at) \
                     SELECT ?1, kind, title, payload_json, run_id, node_id, agent_id, \
                     depends_on_claim_id, 'claimable', NULL, attempt + 1, NULL, NULL, \
                     created_by, created_at, NULL, NULL, ?2 \
                     FROM task_claim WHERE claim_id = ?3",
                    rusqlite::params![new_id, now, claim_id],
                )?;
                if copied != 1 {
                    return Err(DbError::Validation(format!(
                        "transfer copy failed for {claim_id}"
                    )));
                }
                Ok(Ok(query_claim_row(c, "claim_id = ?1", &new_id)?))
            })
            .map_err(CollabError::from)?;
        row_to_claim(outcome?)
    }

    /// 点查 claim 行（NotFound 词形）。
    pub fn show(&self, claim_id: &str) -> CollabResult<ClaimRow> {
        let outcome: CollabResult<ClaimDbRow> = self
            .pool
            .with_conn(|c| -> Result<CollabResult<ClaimDbRow>, DbError> {
                match query_claim_row(c, "claim_id = ?1", claim_id) {
                    Ok(row) => Ok(Ok(row)),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(Err(CollabError::NotFound(
                        format!("claim {claim_id} not found"),
                    ))),
                    Err(e) => Err(e.into()),
                }
            })
            .map_err(CollabError::from)?;
        row_to_claim(outcome?)
    }
}

/// 邮箱消息行投影（029 ma_message 十列全量读回）。
#[derive(Debug, Clone, PartialEq)]
pub struct MessageRow {
    /// message id（`mam-` 前缀）。
    pub message_id: String,
    /// 发件 caller（真 FK→caller）。
    pub from_caller: String,
    /// 收件 caller（真 FK→caller）。
    pub to_caller: String,
    /// 关联 claim（可空引用列）。
    pub claim_id: Option<String>,
    /// 关联 run（可空引用列）。
    pub run_id: Option<String>,
    /// 消息载荷 JSON（≤1MiB）。
    pub payload_json: String,
    /// 投递时刻（NULL = 未投递；首拉落记不覆盖）。
    pub delivered_at: Option<String>,
    /// 读取时刻（NULL = 未读；未读行每次 fetch 重投可见）。
    pub read_at: Option<String>,
    /// 创建时刻。
    pub created_at: String,
    /// 最近更新时刻。
    pub updated_at: String,
}

/// 029 ma_message 行元组（与 MESSAGE_COLUMNS 列序一一对应）。
type MessageDbRow = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
);

const MESSAGE_COLUMNS: &str = "message_id, from_caller, to_caller, claim_id, run_id, \
     payload_json, delivered_at, read_at, created_at, updated_at";

fn row_to_message(row: MessageDbRow) -> MessageRow {
    MessageRow {
        message_id: row.0,
        from_caller: row.1,
        to_caller: row.2,
        claim_id: row.3,
        run_id: row.4,
        payload_json: row.5,
        delivered_at: row.6,
        read_at: row.7,
        created_at: row.8,
        updated_at: row.9,
    }
}

/// 邮箱消息入参（from/to 必须在用 caller——路由主语强一致 fail-closed）。
#[derive(Debug, Clone)]
pub struct MessageSendInput {
    /// 发件 caller。
    pub from_caller: String,
    /// 收件 caller。
    pub to_caller: String,
    /// 消息载荷 JSON（合法 JSON ≤1MiB）。
    pub payload_json: String,
    /// 关联 claim（可空引用列）。
    pub claim_id: Option<String>,
    /// 关联 run（可空引用列，`wfr-` 词形守卫）。
    pub run_id: Option<String>,
}

/// ma_message 存储面（D3：投递/读取分记 + 拉模型持久重投 + sweep 清单面）。
#[derive(Debug, Clone)]
pub struct MessageStore {
    pool: Pool,
}

impl MessageStore {
    /// 构造（调用方先行 database::migrate 保证 029 表存在）。
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// 发送（真 FK→caller 兜底 + 在用校验 fail-closed；载荷 JSON 解析守卫）。
    pub fn send(&self, input: &MessageSendInput) -> CollabResult<MessageRow> {
        if input.payload_json.len() > 1024 * 1024 {
            return Err(CollabError::InvalidInput(
                "payload_json exceeds 1MiB cap".to_string(),
            ));
        }
        serde_json::from_str::<serde_json::Value>(&input.payload_json).map_err(|e| {
            CollabError::InvalidInput(format!("payload_json is not valid JSON: {e}"))
        })?;
        if let Some(run) = &input.run_id {
            if !run.starts_with("wfr-") {
                return Err(CollabError::InvalidInput(format!(
                    "run_id {run:?} must be a wfr- run id"
                )));
            }
        }
        if let Some(claim) = &input.claim_id {
            if !claim.starts_with("tcl-") {
                return Err(CollabError::InvalidInput(format!(
                    "claim_id {claim:?} must be a tcl- claim id"
                )));
            }
        }
        let message_id = new_message_id();
        let now = now_iso_ms();
        let outcome: CollabResult<MessageDbRow> = self
            .pool
            .with_conn(|c| -> Result<CollabResult<MessageDbRow>, DbError> {
                for (role, caller) in [
                    ("from_caller", &input.from_caller),
                    ("to_caller", &input.to_caller),
                ] {
                    if !ClaimStore::caller_in_use(c, caller)? {
                        return Ok(Err(CollabError::InvalidInput(format!(
                            "{role} caller {caller:?} unknown or revoked"
                        ))));
                    }
                }
                c.execute(
                    "INSERT INTO ma_message (message_id, from_caller, to_caller, claim_id, \
                     run_id, payload_json, delivered_at, read_at, created_at, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL, ?7, ?7)",
                    rusqlite::params![
                        message_id,
                        input.from_caller,
                        input.to_caller,
                        input.claim_id,
                        input.run_id,
                        input.payload_json,
                        now,
                    ],
                )?;
                let row = c.query_row(
                    &format!("SELECT {MESSAGE_COLUMNS} FROM ma_message WHERE message_id = ?1"),
                    [&message_id],
                    |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                            r.get(7)?,
                            r.get(8)?,
                            r.get(9)?,
                        ))
                    },
                )?;
                Ok(Ok(row))
            })
            .map_err(CollabError::from)?;
        Ok(row_to_message(outcome?))
    }

    /// 收件（拉模型）：未读行（read_at IS NULL）全部返回——未读行每次
    /// fetch 均可见（持久重投语义），read 确认后不再返回；首次返回时对
    /// 未投递行逐条单语句投递 UPDATE（`WHERE message_id=? AND to_caller=?
    /// AND delivered_at IS NULL`，首拉落记不覆盖）。created_at 升序。
    pub fn fetch(&self, to_caller: &str, limit: usize) -> CollabResult<Vec<MessageRow>> {
        let mut rows: Vec<MessageDbRow> = self
            .pool
            .with_conn(|c| -> Result<Vec<MessageDbRow>, DbError> {
                let mut stmt = c.prepare(&format!(
                    "SELECT {MESSAGE_COLUMNS} FROM ma_message \
                     WHERE to_caller = ?1 AND read_at IS NULL \
                     ORDER BY created_at, message_id LIMIT ?2"
                ))?;
                let rows = stmt
                    .query_map(rusqlite::params![to_caller, limit.min(1000) as i64], |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                            r.get(7)?,
                            r.get(8)?,
                            r.get(9)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .map_err(CollabError::from)?;
        let now = now_iso_ms();
        for row in &mut rows {
            if row.6.is_none() {
                let delivered = self
                    .pool
                    .with_conn(|c| {
                        c.execute(
                            "UPDATE ma_message SET delivered_at = ?2, updated_at = ?2 \
                             WHERE message_id = ?1 AND to_caller = ?3 AND delivered_at IS NULL",
                            rusqlite::params![row.0, now, to_caller],
                        )
                    })
                    .map_err(CollabError::from)?;
                if delivered == 1 {
                    row.6 = Some(now.clone());
                    row.9 = now.clone();
                }
            }
        }
        Ok(rows.into_iter().map(row_to_message).collect())
    }

    /// 读标记（单语句 CAS，read_at IS NULL 守卫；to_caller 守卫防越权读
    /// 他人邮箱）。已读/非本人/不存在 = 错误。
    pub fn read(&self, message_id: &str, to_caller: &str) -> CollabResult<MessageRow> {
        let now = now_iso_ms();
        let outcome: CollabResult<MessageDbRow> = self
            .pool
            .with_conn(|c| -> Result<CollabResult<MessageDbRow>, DbError> {
                let updated = c.execute(
                    "UPDATE ma_message SET read_at = ?2, updated_at = ?2 \
                     WHERE message_id = ?1 AND to_caller = ?3 AND read_at IS NULL",
                    rusqlite::params![message_id, now, to_caller],
                )?;
                if updated != 1 {
                    let exists: i64 = c.query_row(
                        "SELECT COUNT(*) FROM ma_message WHERE message_id = ?1",
                        [message_id],
                        |r| r.get(0),
                    )?;
                    return Ok(Err(if exists == 0 {
                        CollabError::NotFound(format!("message {message_id} not found"))
                    } else {
                        CollabError::NotClaimable {
                            claim_id: message_id.to_string(),
                            state: "already_read_or_not_recipient".to_string(),
                        }
                    }));
                }
                let row = c.query_row(
                    &format!("SELECT {MESSAGE_COLUMNS} FROM ma_message WHERE message_id = ?1"),
                    [message_id],
                    |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                            r.get(7)?,
                            r.get(8)?,
                            r.get(9)?,
                        ))
                    },
                )?;
                Ok(Ok(row))
            })
            .map_err(CollabError::from)?;
        Ok(row_to_message(outcome?))
    }

    /// sweep（只读未投递行清单/计数；MA-4 SSE 推送面通知源输入——推送
    /// 失败重投的输入源，推送通道本批零实现）。返回（清单, 未投递总数）。
    pub fn sweep(&self, limit: usize) -> CollabResult<(Vec<MessageRow>, i64)> {
        self.pool
            .with_conn(|c| -> Result<(Vec<MessageRow>, i64), DbError> {
                let total: i64 = c.query_row(
                    "SELECT COUNT(*) FROM ma_message WHERE delivered_at IS NULL",
                    [],
                    |r| r.get(0),
                )?;
                let mut stmt = c.prepare(&format!(
                    "SELECT {MESSAGE_COLUMNS} FROM ma_message \
                     WHERE delivered_at IS NULL \
                     ORDER BY created_at, message_id LIMIT ?1"
                ))?;
                let rows = stmt
                    .query_map([limit.min(1000) as i64], |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                            r.get(7)?,
                            r.get(8)?,
                            r.get(9)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok((rows.into_iter().map(row_to_message).collect(), total))
            })
            .map_err(CollabError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{DbPath, PoolConfig};
    use std::path::Path;

    /// 仓库根 migrations 目录（cargo test CWD 为 crate 目录）。
    const MIGRATIONS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../migrations");

    /// 内存库迁移装配（001..029 全链；caller 登记 026 面）。
    fn migrated_pool() -> Pool {
        let pool = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: std::time::Duration::from_secs(5),
        })
        .expect("open memory pool");
        database::migrate(&pool, Path::new(MIGRATIONS_DIR)).expect("migrate 001..029");
        pool
    }

    /// 026 caller 登记辅助（测试库内装配数据面）。
    fn add_caller(pool: &Pool, caller_id: &str) {
        pool.with_conn(|c| {
            c.execute(
                "INSERT INTO caller (caller_id, display_name, permission_level, created_at) \
                 VALUES (?1, ?2, 'T1', '2026-11-22T00:00:00.000Z')",
                rusqlite::params![caller_id, format!("name-{caller_id}")],
            )
        })
        .expect("add caller");
    }

    fn sample_input(created_by: &str) -> ClaimCreateInput {
        ClaimCreateInput {
            kind: ClaimKind::AgentSubtask,
            title: "probe example.com".to_string(),
            payload_json: r#"{"target":"example.com"}"#.to_string(),
            run_id: Some("wfr-test-1".to_string()),
            node_id: Some("node1".to_string()),
            agent_id: Some("ag-test".to_string()),
            depends_on_claim_id: None,
            created_by: created_by.to_string(),
        }
    }

    #[test]
    fn create_and_show_roundtrip() {
        let pool = migrated_pool();
        add_caller(&pool, "caller-a");
        let store = ClaimStore::new(pool);
        let row = store.create(&sample_input("caller-a")).expect("create");
        assert_eq!(row.state, ClaimState::Claimable);
        assert_eq!(row.attempt, 1);
        assert!(row.claim_id.starts_with("tcl-"));
        assert!(row.claimed_by.is_none());
        let shown = store.show(&row.claim_id).expect("show");
        assert_eq!(shown, row);
    }

    #[test]
    fn create_validates_caller_shape_and_cycle() {
        let pool = migrated_pool();
        add_caller(&pool, "caller-a");
        let store = ClaimStore::new(pool);
        // 未知 caller 创建拒。
        let err = store
            .create(&sample_input("caller-ghost"))
            .expect_err("unknown caller rejected");
        assert!(matches!(err, CollabError::InvalidInput(_)));
        // 不存在的依赖拒。
        let mut input = sample_input("caller-a");
        input.depends_on_claim_id = Some("tcl-nope".to_string());
        let err = store.create(&input).expect_err("missing dep");
        assert!(matches!(err, CollabError::InvalidInput(_)));
        // 非法 run_id 词形拒。
        let mut input = sample_input("caller-a");
        input.run_id = Some("nope".to_string());
        let err = store.create(&input).expect_err("bad run_id");
        assert!(matches!(err, CollabError::InvalidInput(_)));
        // 非法 JSON 载荷拒。
        let mut input = sample_input("caller-a");
        input.payload_json = "{bad".to_string();
        let err = store.create(&input).expect_err("bad payload");
        assert!(matches!(err, CollabError::InvalidInput(_)));
    }

    #[test]
    fn create_rejects_cycle_via_recursive_cte() {
        let pool = migrated_pool();
        add_caller(&pool, "caller-a");
        let store = ClaimStore::new(pool.clone());
        // 构造链 a <- b <- c <- d（深链合法）。
        let a = store
            .create(&ClaimCreateInput {
                kind: ClaimKind::Review,
                title: "a".into(),
                payload_json: "{}".into(),
                run_id: None,
                node_id: None,
                agent_id: None,
                depends_on_claim_id: None,
                created_by: "caller-a".into(),
            })
            .expect("a");
        let b = store
            .create(&ClaimCreateInput {
                depends_on_claim_id: Some(a.claim_id.clone()),
                title: "b".into(),
                ..sample_input("caller-a")
            })
            .expect("b");
        let c = store
            .create(&ClaimCreateInput {
                depends_on_claim_id: Some(b.claim_id.clone()),
                title: "c".into(),
                ..sample_input("caller-a")
            })
            .expect("c");
        let d = store
            .create(&ClaimCreateInput {
                depends_on_claim_id: Some(c.claim_id.clone()),
                title: "d".into(),
                ..sample_input("caller-a")
            })
            .expect("deep chain ok");
        assert_eq!(d.attempt, 1);
        // 环检测面：手工把 a 的依赖改指 c 制造环（绕过 create 校验直改
        // 测试库），随后 create 触发链上溯命中深度帽判环拒。
        pool.with_conn(|conn| {
            conn.execute(
                "UPDATE task_claim SET depends_on_claim_id = ?2 WHERE claim_id = ?1",
                rusqlite::params![a.claim_id, c.claim_id],
            )
        })
        .expect("force cycle in test db");
        let err = store
            .create(&ClaimCreateInput {
                depends_on_claim_id: Some(a.claim_id.clone()),
                title: "e".into(),
                ..sample_input("caller-a")
            })
            .expect_err("cycle must be rejected");
        assert!(matches!(err, CollabError::CycleDetected(_)));
    }

    #[test]
    fn claim_cas_winner_and_dependency_gate() {
        let pool = migrated_pool();
        add_caller(&pool, "caller-a");
        add_caller(&pool, "caller-b");
        let store = ClaimStore::new(pool);
        // 依赖门正例：依赖 completed 后可领。
        let dep = store.create(&sample_input("caller-a")).expect("dep");
        store.claim(&dep.claim_id, "caller-a").expect("claim dep");
        let dep = store
            .complete(&dep.claim_id, "caller-a", r#"{"ok":true}"#)
            .expect("complete dep");
        assert_eq!(dep.state, ClaimState::Completed);
        let mut input = sample_input("caller-a");
        input.depends_on_claim_id = Some(dep.claim_id.clone());
        let child = store.create(&input).expect("child");
        let claimed = store
            .claim(&child.claim_id, "caller-b")
            .expect("claim after dep completed");
        assert_eq!(claimed.state, ClaimState::Claimed);
        assert_eq!(claimed.claimed_by.as_deref(), Some("caller-b"));
        assert_eq!(claimed.attempt, 1);
        // 二次领取 = 败方（NotClaimable，0 行零副作用）。
        let err = store.claim(&child.claim_id, "caller-a").expect_err("lost");
        assert!(matches!(err, CollabError::NotClaimable { .. }));
        // 依赖未完成负例：新行依赖 claimable（未 completed）行 → 拒。
        let blocking = store.create(&sample_input("caller-a")).expect("blocking");
        let mut gated = sample_input("caller-a");
        gated.depends_on_claim_id = Some(blocking.claim_id.clone());
        let gated = store.create(&gated).expect("gated create ok");
        let err = store
            .claim(&gated.claim_id, "caller-b")
            .expect_err("dependency gate");
        assert!(matches!(err, CollabError::DependencyNotCompleted(_)));
        // ready_list 只含依赖门过的行。
        let ready = store.ready_list(100).expect("ready list");
        assert!(ready.iter().all(|r| r.claim_id != gated.claim_id));
        assert!(ready.iter().any(|r| r.claim_id == blocking.claim_id));
        // 完成后进依赖门。
        store
            .claim(&blocking.claim_id, "caller-a")
            .expect("claim blocking");
        store
            .complete(&blocking.claim_id, "caller-a", r#"{"done":1}"#)
            .expect("complete blocking");
        let ready = store.ready_list(100).expect("ready list after");
        assert!(ready.iter().any(|r| r.claim_id == gated.claim_id));
    }

    #[test]
    fn complete_fail_only_by_claimer() {
        let pool = migrated_pool();
        add_caller(&pool, "caller-a");
        add_caller(&pool, "caller-b");
        let store = ClaimStore::new(pool);
        let row = store.create(&sample_input("caller-a")).expect("create");
        store.claim(&row.claim_id, "caller-b").expect("claim");
        // 他人完成拒（NotClaimer）。
        let err = store
            .complete(&row.claim_id, "caller-a", r#"{"x":1}"#)
            .expect_err("not claimer");
        assert!(matches!(err, CollabError::NotClaimer(_)));
        // 本人完成成功。
        let done = store
            .complete(&row.claim_id, "caller-b", r#"{"report":"ok"}"#)
            .expect("complete");
        assert_eq!(done.state, ClaimState::Completed);
        assert_eq!(done.result_json.as_deref(), Some(r#"{"report":"ok"}"#));
        assert!(done.finished_at.is_some());
        // 终态后再写拒（NotClaimable）。
        let err = store
            .fail(&row.claim_id, "caller-b", r#"{"e":1}"#)
            .expect_err("terminal");
        assert!(matches!(err, CollabError::NotClaimable { .. }));
        // failed 终态 error_json 落行。
        let row2 = store.create(&sample_input("caller-a")).expect("create2");
        store.claim(&row2.claim_id, "caller-b").expect("claim2");
        let failed = store
            .fail(&row2.claim_id, "caller-b", r#"{"error":"boom"}"#)
            .expect("fail");
        assert_eq!(failed.state, ClaimState::Failed);
        assert_eq!(failed.error_json.as_deref(), Some(r#"{"error":"boom"}"#));
        assert!(failed.result_json.is_none());
        // 非法 result JSON 拒。
        let row3 = store.create(&sample_input("caller-a")).expect("create3");
        store.claim(&row3.claim_id, "caller-b").expect("claim3");
        let err = store
            .complete(&row3.claim_id, "caller-b", "{bad")
            .expect_err("bad result json");
        assert!(matches!(err, CollabError::InvalidInput(_)));
    }

    #[test]
    fn transfer_only_claimer_and_attempt_chain() {
        let pool = migrated_pool();
        add_caller(&pool, "caller-a");
        add_caller(&pool, "caller-b");
        add_caller(&pool, "caller-c");
        let store = ClaimStore::new(pool);
        let row = store.create(&sample_input("caller-a")).expect("create");
        store.claim(&row.claim_id, "caller-b").expect("claim");
        // 他人转派拒。
        let err = store
            .transfer(&row.claim_id, "caller-a")
            .expect_err("not claimer transfer");
        assert!(matches!(err, CollabError::NotClaimer(_)));
        // 本人转派：旧行 transferred 终态 + 新行 claimable attempt+1。
        let new_row = store.transfer(&row.claim_id, "caller-b").expect("transfer");
        assert!(new_row.claim_id.starts_with("tcl-"));
        assert_ne!(new_row.claim_id, row.claim_id);
        assert_eq!(new_row.state, ClaimState::Claimable);
        assert_eq!(new_row.attempt, row.attempt + 1);
        assert!(new_row.claimed_by.is_none());
        assert_eq!(new_row.title, row.title);
        assert_eq!(new_row.payload_json, row.payload_json);
        let old = store.show(&row.claim_id).expect("old row");
        assert_eq!(old.state, ClaimState::Transferred);
        assert!(old.result_json.is_none(), "transferred 为终态禁写 result");
        // transferred 禁写 result（终态拒）。
        let err = store
            .complete(&row.claim_id, "caller-b", r#"{}"#)
            .expect_err("transferred terminal");
        assert!(matches!(err, CollabError::NotClaimable { .. }));
        // 败方（caller-c）对新行再 CAS 领取 = 接新 attempt。
        let re_claimed = store.claim(&new_row.claim_id, "caller-c").expect("reclaim");
        assert_eq!(re_claimed.state, ClaimState::Claimed);
        assert_eq!(re_claimed.claimed_by.as_deref(), Some("caller-c"));
        assert_eq!(re_claimed.attempt, 2);
    }

    #[test]
    fn message_send_fetch_read_sequence() {
        let pool = migrated_pool();
        add_caller(&pool, "caller-a");
        add_caller(&pool, "caller-b");
        let store = MessageStore::new(pool.clone());
        // send→fetch：delivered_at 落、read_at NULL。
        let msg = store
            .send(&MessageSendInput {
                from_caller: "caller-a".to_string(),
                to_caller: "caller-b".to_string(),
                payload_json: r#"{"hello":"world"}"#.to_string(),
                claim_id: None,
                run_id: Some("wfr-test-1".to_string()),
            })
            .expect("send");
        assert!(msg.message_id.starts_with("mam-"));
        assert!(msg.delivered_at.is_none());
        assert!(msg.read_at.is_none());
        let fetched = store.fetch("caller-b", 100).expect("fetch");
        assert_eq!(fetched.len(), 1);
        assert!(fetched[0].delivered_at.is_some(), "首拉落记 delivered_at");
        assert!(fetched[0].read_at.is_none());
        // 未读行重复 fetch 可见（持久重投语义）。
        let again = store.fetch("caller-b", 100).expect("refetch");
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].message_id, msg.message_id);
        // read 标记后 fetch 不再返回。
        store.read(&msg.message_id, "caller-b").expect("read");
        let after = store.fetch("caller-b", 100).expect("fetch after read");
        assert!(after.is_empty());
        // 重复 read = 拒（read_at IS NULL 守卫）。
        let err = store
            .read(&msg.message_id, "caller-b")
            .expect_err("double read");
        assert!(matches!(err, CollabError::NotClaimable { .. }));
    }

    #[test]
    fn message_validates_recipients_and_sweep() {
        let pool = migrated_pool();
        add_caller(&pool, "caller-a");
        let store = MessageStore::new(pool.clone());
        // 未知收件人拒（FK/校验双保险）。
        let err = store
            .send(&MessageSendInput {
                from_caller: "caller-a".to_string(),
                to_caller: "caller-ghost".to_string(),
                payload_json: "{}".to_string(),
                claim_id: None,
                run_id: None,
            })
            .expect_err("unknown recipient");
        assert!(matches!(err, CollabError::InvalidInput(_)));
        // 非法 run_id 词形拒。
        let err = store
            .send(&MessageSendInput {
                from_caller: "caller-a".to_string(),
                to_caller: "caller-a".to_string(),
                payload_json: "{}".to_string(),
                claim_id: None,
                run_id: Some("nope".to_string()),
            })
            .expect_err("bad run_id");
        assert!(matches!(err, CollabError::InvalidInput(_)));
        // sweep：未投递行清单/计数。
        add_caller(&pool, "caller-b");
        store
            .send(&MessageSendInput {
                from_caller: "caller-a".to_string(),
                to_caller: "caller-b".to_string(),
                payload_json: r#"{"q":1}"#.to_string(),
                claim_id: None,
                run_id: None,
            })
            .expect("send2");
        let (rows, total) = store.sweep(100).expect("sweep");
        assert_eq!(total, 1);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].delivered_at.is_none());
        // 投递后 sweep 归零。
        store.fetch("caller-b", 100).expect("fetch");
        let (rows, total) = store.sweep(100).expect("sweep after fetch");
        assert_eq!(total, 0);
        assert!(rows.is_empty());
    }
}
