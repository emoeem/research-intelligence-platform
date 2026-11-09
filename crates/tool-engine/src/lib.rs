//! Phase 1 工具引擎骨架（TOOL-CONTRACT §10.1 落地物第 4 行，ROADMAP P1-5）。
//!
//! 职责链：[`request`] 受理与 D-TC-05 提交校验（拒绝不落行）→ [`state`]
//! 跃迁表约束下的状态推进（tool_run 行同步更新）→ [`envelope`] 信封组装
//! 与 §5.4 截断 → [`mock_runner`] 注入路径驱动。契约测试最小集见
//! tests/contract_min_set.rs（§10.2 七项）。
//!
//! 边界：真实 runtime 适配器（python_worker 派发器、容器 Runner）属 P1-6
//! 与 Phase 2；本 crate 只冻结受理、状态机、信封三面，运行面由 Mock Runner
//! 以可注入配置驱动。Manifest 级 max_output_bytes 帽未持久化（tool 表无该
//! 列，不新增列），引擎按 worker_protocol v1 域 [256, 67108864] 与
//! timeout_s 列帽校验（如实声明）。

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod adapter;
pub mod envelope;
pub mod mock_runner;
pub mod request;
pub mod state;

use std::time::Instant;

use database::{DbError, Pool};
use domain::ToolRunStatus;
use serde_json::{json, Value};
use tool_registry::Registry;

use crate::mock_runner::{MockRunner, Outcome};
use crate::request::{Rejection, SubmitRequest};

/// 引擎错误。
#[derive(Debug, Clone, PartialEq)]
pub enum EngineError {
    /// D-TC-05 提交校验拒绝（不产生 run 行）。
    Rejected(Rejection),
    /// run 或行不存在。
    NotFound(String),
    /// 非法跃迁（§3.2 表之外）。
    InvalidTransition {
        /// 起始状态。
        from: ToolRunStatus,
        /// 目标状态。
        to: ToolRunStatus,
    },
    /// drive 只受理 QUEUED 态的 run。
    NotStartable {
        /// 当前状态。
        status: ToolRunStatus,
        /// run id。
        run_id: String,
    },
    /// 等待注入下既不恢复也不取消，无法推进。
    Stuck(String),
    /// 信封组装/校验失败。
    Envelope(String),
    /// 底层存储错误。
    Storage(String),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::Rejected(r) => write!(f, "submission rejected (D-TC-05): {}", r.reason()),
            EngineError::NotFound(id) => write!(f, "run {id:?} not found"),
            EngineError::InvalidTransition { from, to } => {
                write!(
                    f,
                    "illegal transition {from:?} -> {to:?} (TOOL-CONTRACT 3.2)"
                )
            }
            EngineError::NotStartable { status, run_id } => {
                write!(f, "run {run_id} not startable in state {status:?}")
            }
            EngineError::Stuck(r) => write!(f, "runner stuck: {r}"),
            EngineError::Envelope(r) => write!(f, "envelope error: {r}"),
            EngineError::Storage(r) => write!(f, "storage error: {r}"),
        }
    }
}

impl std::error::Error for EngineError {}

impl From<DbError> for EngineError {
    fn from(e: DbError) -> Self {
        EngineError::Storage(e.to_string())
    }
}

impl From<Rejection> for EngineError {
    fn from(r: Rejection) -> Self {
        EngineError::Rejected(r)
    }
}

impl From<tool_registry::RegistryError> for EngineError {
    fn from(e: tool_registry::RegistryError) -> Self {
        EngineError::Storage(e.to_string())
    }
}

/// 受理结果。
#[derive(Debug, Clone, PartialEq)]
pub struct Submitted {
    /// run id。
    pub run_id: String,
    /// 解析后的工具 id。
    pub tool_id: String,
    /// 解析后的执行版本。
    pub tool_version: String,
    /// QUEUED 落库时刻。
    pub queued_at: String,
}

/// 驱动结果：终态信封 + 状态轨迹。
#[derive(Debug, Clone, PartialEq)]
pub struct DriveOutcome {
    /// 终态信封（已过 envelope::validate）。
    pub envelope: envelope::ToolRunResult,
    /// 本次驱动经过的状态序列（含起点 QUEUED 与终点）。
    pub transcript: Vec<ToolRunStatus>,
}

/// 行投影（tool_run 既有列的引擎视图；adapter 模块复用）。
#[derive(Debug, Clone)]
pub(crate) struct RunRow {
    pub(crate) run_id: String,
    pub(crate) tool_id: String,
    pub(crate) tool_version: String,
    pub(crate) submit_key: Option<String>,
    pub(crate) status: ToolRunStatus,
    /// replay_json.input_sanitized（提交时的规整输入，§4.2 的 task 来源）。
    pub(crate) input_sanitized: Value,
}

/// 工具引擎。
#[derive(Clone)]
pub struct Engine {
    pool: Pool,
    registry: Registry,
}

impl Engine {
    /// 在连接池 + 注册表之上打开引擎（同一测试库共享一个 Registry）。
    pub fn open(pool: Pool, registry: Registry) -> Self {
        Self { pool, registry }
    }

    /// 注册表视图（adapter 路径取 output_schema 用）。
    pub(crate) fn registry(&self) -> &Registry {
        &self.registry
    }

    /// D-TC-05 提交校验 + QUEUED 落行。拒绝时不产生任何行（§3.2 第 1 行）。
    pub fn submit(&self, request: &Value) -> Result<Submitted, EngineError> {
        let req: SubmitRequest = request::structural_check(request)?;
        // Registry 面：工具存在。
        let detail = self.registry.query(&req.tool_id).map_err(|e| match e {
            tool_registry::RegistryError::NotFound { .. } => {
                Rejection::UnknownTool(req.tool_id.clone())
            }
            other => Rejection::Structural(format!("registry lookup failed: {other}")),
        })?;
        // 版本解析。
        let tool_version = match &req.tool_version {
            Some(v) => {
                if !detail.versions.iter().any(|vr| vr.version == *v) {
                    return Err(Rejection::UnknownVersion {
                        tool_id: req.tool_id.clone(),
                        version: v.clone(),
                    }
                    .into());
                }
                v.clone()
            }
            None => detail.current_version.clone().ok_or_else(|| {
                Rejection::UnknownTool(format!("{} has no current version", req.tool_id))
            })?,
        };
        // T2 授权引用（§2.4，POLICY_DENIED）。
        let permission = detail.tool.permission_level.clone().unwrap_or_default();
        if permission == "T2" && req.authorization_ref.is_none() {
            return Err(Rejection::MissingAuthorization(req.tool_id.clone()).into());
        }
        // 预算帽（§4.1：显式值不得超过 Manifest 上限，超出按 VALIDATION 拒绝）。
        let cap_timeout = detail.tool.timeout_s.unwrap_or(600).clamp(1, 600);
        if let Some(t) = req.budget.timeout_seconds {
            if t == 0 || t as i64 > cap_timeout {
                return Err(Rejection::BudgetExceeds(format!(
                    "timeout_seconds {t} exceeds manifest cap {cap_timeout}"
                ))
                .into());
            }
        }
        if let Some(m) = req.budget.max_output_bytes {
            if !(256..=67108864).contains(&m) {
                return Err(Rejection::BudgetExceeds(format!(
                    "max_output_bytes {m} outside [256, 67108864] (worker_protocol v1)"
                ))
                .into());
            }
        }
        let resolved_timeout = req.budget.timeout_seconds.unwrap_or(cap_timeout as u64);
        let resolved_max_output = req.budget.max_output_bytes.unwrap_or(1048576);
        // input 须合 Manifest input_schema（§4.1）。
        let (input_schema, _) = self
            .registry
            .schemas(&req.tool_id, Some(&tool_version))
            .map_err(|e| match e {
                tool_registry::RegistryError::NotFound { .. } => Rejection::UnknownVersion {
                    tool_id: req.tool_id.clone(),
                    version: tool_version.clone(),
                },
                other => Rejection::Structural(format!("schema lookup failed: {other}")),
            })?;
        tool_registry::jsonschema_lite::validate_instance(&input_schema, &req.input)
            .map_err(Rejection::InputSchemaViolation)?;

        let now = domain::time::now_iso();
        let budget_json = json!({
            "timeout_seconds": resolved_timeout,
            "max_output_bytes": resolved_max_output,
        });
        let permission_ctx = json!({
            "permission_level": permission,
            "authorization_ref": req.authorization_ref,
        });
        let replay_json = json!({
            "input_sanitized": req.input,
            "submit_key": req.submit_key,
            "cached": false,
        });
        let input_refs_json = if req.input_refs.is_empty() {
            None
        } else {
            let pairs: Vec<Value> = req
                .input_refs
                .iter()
                .map(|(a, m)| json!({"artifact_id": a, "mount_as": m}))
                .collect();
            Some(json!(pairs).to_string())
        };
        let task_id = req.task_id.clone();
        let submit_key = req.submit_key.clone();
        let input_hash =
            tool_registry::hash::sha256_hex(tool_registry::normalize_json(&req.input).as_bytes());
        self.pool
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO tool_run (run_id, tool_id, tool_version, task_id, workspace_id,
                     submit_key, input_hash, status, permission_ctx, budget_json, input_refs_json,
                     replay_json, created_at, updated_at, schema_version)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'QUEUED', ?8, ?9, ?10, ?11, ?12, ?12, 1)",
                    database::rusqlite::params![
                        req.run_id,
                        req.tool_id,
                        tool_version,
                        task_id,
                        req.workspace_id,
                        submit_key,
                        input_hash,
                        permission_ctx.to_string(),
                        budget_json.to_string(),
                        input_refs_json,
                        replay_json.to_string(),
                        now
                    ],
                )?;
                Ok::<(), database::rusqlite::Error>(())
            })
            .map_err(EngineError::from)?;
        Ok(Submitted {
            run_id: req.run_id,
            tool_id: req.tool_id,
            tool_version,
            queued_at: now,
        })
    }

    /// 协作式取消请求（D-TC-04）：仅 RUNNING 与 WAITING_FOR_RUNNER 可发起；
    /// 记录 cancel_requested_at，返回是否受理。
    pub fn request_cancel(&self, run_id: &str) -> Result<bool, EngineError> {
        let row = self.load_row(run_id)?;
        if !state::cancel_requestable(row.status) {
            return Ok(false);
        }
        let now = domain::time::now_iso();
        self.pool
            .with_conn(|c| {
                c.execute(
                    "UPDATE tool_run SET cancel_requested_at = ?2, updated_at = ?2 WHERE run_id = ?1",
                    database::rusqlite::params![run_id, now],
                )?;
                Ok::<(), database::rusqlite::Error>(())
            })
            .map_err(EngineError::from)?;
        Ok(true)
    }

    /// 以 Mock Runner 驱动一个 QUEUED 的 run 走到终态，返回信封与状态轨迹。
    ///
    /// P2-4b 起真实心跳路径经 [`Engine::mark_waiting_for_runner`] +
    /// [`Engine::resume_to_preparing`] + [`Engine::drive_resumed`] 走第 3/4 行
    /// 同一跃迁对（runtime-gateway 判定，见 crates/runtime-gateway）；
    /// `runner.wait_for_runner` 注入开关保留给 Phase 1 契约测试（§10.2）。
    pub fn drive(&self, run_id: &str, runner: &MockRunner) -> Result<DriveOutcome, EngineError> {
        let row = self.load_row(run_id)?;
        if row.status != ToolRunStatus::Queued {
            return Err(EngineError::NotStartable {
                status: row.status,
                run_id: run_id.to_string(),
            });
        }
        let started = Instant::now();
        let mut transcript = vec![ToolRunStatus::Queued];
        let mut current = ToolRunStatus::Queued;

        // 第 3 行：QUEUED → WAITING_FOR_RUNNER 注入。
        if runner.wait_for_runner {
            self.advance(run_id, current, ToolRunStatus::WaitingForRunner, false)?;
            current = ToolRunStatus::WaitingForRunner;
            transcript.push(current);
            if runner.cancel_while_waiting {
                // 第 5 行：等待期内被取消。
                let _accepted = self.request_cancel(run_id)?;
                self.advance(run_id, current, ToolRunStatus::Cancelled, true)?;
                current = ToolRunStatus::Cancelled;
                transcript.push(current);
            } else if runner.recover_after_wait {
                // 第 4 行：心跳恢复。
                self.advance(run_id, current, ToolRunStatus::Preparing, false)?;
                current = ToolRunStatus::Preparing;
                transcript.push(current);
            } else {
                return Err(EngineError::Stuck(run_id.to_string()));
            }
        } else {
            // 第 2 行：调度器选出可用 runner。
            self.advance(run_id, current, ToolRunStatus::Preparing, false)?;
            current = ToolRunStatus::Preparing;
            transcript.push(current);
        }

        self.drive_tail(run_id, &row, runner, started, transcript, current)
    }

    /// QUEUED → WAITING_FOR_RUNNER（§3.2 第 3 行；P2-4b 真实心跳判定入口）。
    ///
    /// Runner 探测失败时由 runtime-gateway 调用：run 停在 WAITING_FOR_RUNNER，
    /// 不伪装成工具失败（计划 §5.6:336 原文语义）。
    pub fn mark_waiting_for_runner(&self, run_id: &str) -> Result<(), EngineError> {
        let row = self.load_row(run_id)?;
        if row.status != ToolRunStatus::Queued {
            return Err(EngineError::NotStartable {
                status: row.status,
                run_id: run_id.to_string(),
            });
        }
        self.advance(
            run_id,
            ToolRunStatus::Queued,
            ToolRunStatus::WaitingForRunner,
            false,
        )
    }

    /// WAITING_FOR_RUNNER → PREPARING（§3.2 第 4 行；心跳恢复判定入口）。
    pub fn resume_to_preparing(&self, run_id: &str) -> Result<(), EngineError> {
        let row = self.load_row(run_id)?;
        if row.status != ToolRunStatus::WaitingForRunner {
            return Err(EngineError::NotStartable {
                status: row.status,
                run_id: run_id.to_string(),
            });
        }
        self.advance(
            run_id,
            ToolRunStatus::WaitingForRunner,
            ToolRunStatus::Preparing,
            false,
        )
    }

    /// 从 PREPARING 续跑（心跳恢复路径，P2-4b）：复用 [`Engine::drive`] 自
    /// 第 6 行起的全部语义（RUNNING 推进、终局判定、信封组装）。
    /// transcript 以 [WAITING_FOR_RUNNER, PREPARING] 开头：前者是此前
    /// [`Engine::mark_waiting_for_runner`] 已落库的真实状态。
    pub fn drive_resumed(
        &self,
        run_id: &str,
        runner: &MockRunner,
    ) -> Result<DriveOutcome, EngineError> {
        let row = self.load_row(run_id)?;
        if row.status != ToolRunStatus::Preparing {
            return Err(EngineError::NotStartable {
                status: row.status,
                run_id: run_id.to_string(),
            });
        }
        let started = Instant::now();
        let transcript = vec![ToolRunStatus::WaitingForRunner, ToolRunStatus::Preparing];
        self.drive_tail(
            run_id,
            &row,
            runner,
            started,
            transcript,
            ToolRunStatus::Preparing,
        )
    }

    /// drive 的公共尾段（第 6 行 RUNNING 推进到终局信封组装）；
    /// drive（QUEUED 起）与 drive_resumed（心跳恢复续跑）共用。
    fn drive_tail(
        &self,
        run_id: &str,
        row: &RunRow,
        runner: &MockRunner,
        started: Instant,
        mut transcript: Vec<ToolRunStatus>,
        mut current: ToolRunStatus,
    ) -> Result<DriveOutcome, EngineError> {
        let mut truncated_flag = false;
        let mut artifacts: Vec<envelope::EnvelopeArtifact> = Vec::new();

        // 第 6 行：RUNNING（进入即记 started_at）。准备期失败则走第 7 行。
        if !current.is_terminal() && !runner.fail_in_preparing {
            // 第 8 行：拉取镜像时 node 失联（PREPARING → WAITING）随恢复回环。
            if runner.preparing_node_loss {
                self.advance(run_id, current, ToolRunStatus::WaitingForRunner, false)?;
                current = ToolRunStatus::WaitingForRunner;
                transcript.push(current);
                self.advance(run_id, current, ToolRunStatus::Preparing, false)?;
                current = ToolRunStatus::Preparing;
                transcript.push(current);
            }
            self.advance(run_id, current, ToolRunStatus::Running, false)?;
            current = ToolRunStatus::Running;
            transcript.push(current);
        }

        // 协作式取消请求（D-TC-04）：取消类终局在 RUNNING 期发起。
        if current == ToolRunStatus::Running
            && matches!(
                runner.outcome,
                Outcome::CancelThenTerminate { .. } | Outcome::CancelButCompletes { .. }
            )
        {
            let _accepted = self.request_cancel(run_id)?;
        }

        // 终局判定（第 5、7、9 到 13 行）。等待期取消已到终态时直接组装信封。
        let resolved = self.resolved_budget(run_id)?;
        let (target, result, summary, error) = if current.is_terminal() {
            (
                current,
                None,
                None,
                Some(envelope::EnvelopeError {
                    category: "CANCELLED".to_string(),
                    code: json!(-32004),
                    message: "cancelled while WAITING_FOR_RUNNER".to_string(),
                    retryable: false,
                    detail_ref: None,
                }),
            )
        } else if runner.fail_in_preparing {
            // 第 7 行：镜像缺失/digest 不符/POLICY_DENIED/INPUT。
            (
                ToolRunStatus::Failed,
                None,
                Some(format!("准备期失败：{}", runner.preparing_error_category)),
                Some(envelope::EnvelopeError {
                    category: runner.preparing_error_category.clone(),
                    code: json!("PREPARING_FAILED"),
                    message: "mock runner failed during PREPARING (image/policy/input)".into(),
                    retryable: false,
                    detail_ref: None,
                }),
            )
        } else {
            match &runner.outcome {
                Outcome::Succeed { result, summary }
                | Outcome::CancelButCompletes { result, summary } => {
                    // §3.2 第 10 行：结构化结果不合 output_schema → FAILED。
                    let (_, output_schema) = self
                        .registry
                        .schemas(&row.tool_id, Some(&row.tool_version))?;
                    if let Err(e) =
                        tool_registry::jsonschema_lite::validate_instance(&output_schema, result)
                    {
                        (
                            ToolRunStatus::Failed,
                            None,
                            Some(format!("结构化结果校验失败：{e}")),
                            Some(envelope::EnvelopeError {
                                category: "EXECUTION".to_string(),
                                code: json!(-32006),
                                message: format!("structured result failed output_schema: {e}"),
                                retryable: false,
                                detail_ref: None,
                            }),
                        )
                    } else {
                        // §5.4 截断：所有 runtime 路径共用同一函数（两侧行为一致）。
                        let outcome = envelope::truncate(run_id, result, resolved.max_output_bytes);
                        truncated_flag = outcome.truncated;
                        if truncated_flag {
                            let artifact_id = outcome.artifact_id.clone().unwrap_or_default();
                            let sha = outcome.original_sha256.clone().unwrap_or_default();
                            let artifact = envelope::EnvelopeArtifact {
                                artifact_id: artifact_id.clone(),
                                role: "primary".to_string(),
                                uri: format!("mock://artifacts/{artifact_id}"),
                                sha256: sha,
                                size_bytes: outcome.original_bytes,
                                mime_type: "application/json".to_string(),
                                provenance: "mock runner 截断原始输出（TOOL-CONTRACT 5.4）"
                                    .to_string(),
                            };
                            self.insert_artifact(run_id, &artifact)?;
                            artifacts.push(artifact);
                        }
                        (
                            ToolRunStatus::Succeeded,
                            Some(outcome.result.clone()),
                            Some(summary.clone()),
                            None,
                        )
                    }
                }
                Outcome::Fail {
                    category,
                    code,
                    message,
                    retryable,
                } => (
                    ToolRunStatus::Failed,
                    None,
                    Some(format!("{category}: {message}")),
                    Some(envelope::EnvelopeError {
                        category: category.clone(),
                        code: code.clone(),
                        message: message.clone(),
                        retryable: *retryable,
                        detail_ref: None,
                    }),
                ),
                Outcome::Timeout { message } => (
                    ToolRunStatus::TimedOut,
                    None,
                    None,
                    Some(envelope::EnvelopeError {
                        category: "TIMEOUT".to_string(),
                        code: json!(-32002),
                        message: message.clone(),
                        retryable: false,
                        detail_ref: None,
                    }),
                ),
                Outcome::CancelThenTerminate { message } => (
                    ToolRunStatus::Cancelled,
                    None,
                    None,
                    Some(envelope::EnvelopeError {
                        category: "CANCELLED".to_string(),
                        code: json!(-32004),
                        message: message.clone(),
                        retryable: false,
                        detail_ref: None,
                    }),
                ),
            }
        };

        if target != current {
            self.advance(run_id, current, target, true)?;
            current = target;
            transcript.push(current);
        }

        let elapsed_ms = started.elapsed().as_millis() as u64;
        let envelope = self.build_envelope(
            row,
            current,
            TerminalParts {
                result: result.clone(),
                error,
                summary,
                artifacts,
                truncated: truncated_flag,
                cached: runner.cached,
                runtime_type: runner.runtime_type.clone(),
                runtime_node_id: Some("mock-node".to_string()),
            },
            elapsed_ms,
        )?;
        Ok(DriveOutcome {
            envelope,
            transcript,
        })
    }

    /// 读取 run 当前状态。
    pub fn run_status(&self, run_id: &str) -> Result<Option<ToolRunStatus>, EngineError> {
        Ok(Some(self.load_row(run_id)?.status))
    }

    /// 底层连接池视图（测试与后续适配器用）。
    pub fn pool(&self) -> &Pool {
        &self.pool
    }

    // ------------------------------------------------------------------
    // 内部：行读写与信封组装
    // ------------------------------------------------------------------

    pub(crate) fn load_row(&self, run_id: &str) -> Result<RunRow, EngineError> {
        self.pool
            .with_conn(|c| {
                let mut stmt = c.prepare(
                    "SELECT run_id, tool_id, tool_version, submit_key, status, replay_json
                     FROM tool_run WHERE run_id = ?1",
                )?;
                let mut rows = stmt.query_map([run_id], |r| {
                    let status_text: String = r.get(4)?;
                    let status = ToolRunStatus::parse(&status_text).map_err(|_| {
                        database::rusqlite::Error::InvalidColumnType(
                            4,
                            "status".to_string(),
                            database::rusqlite::types::Type::Text,
                        )
                    })?;
                    let replay_json: Option<String> = r.get(5)?;
                    let input_sanitized = replay_json
                        .as_deref()
                        .and_then(|text| serde_json::from_str::<Value>(text).ok())
                        .and_then(|v| v.get("input_sanitized").cloned())
                        .unwrap_or_else(|| json!({}));
                    Ok(RunRow {
                        run_id: r.get(0)?,
                        tool_id: r.get(1)?,
                        tool_version: r.get(2)?,
                        submit_key: r.get(3)?,
                        status,
                        input_sanitized,
                    })
                })?;
                match rows.next() {
                    Some(row) => Ok::<Option<RunRow>, database::rusqlite::Error>(Some(row?)),
                    None => Ok(None),
                }
            })?
            .ok_or_else(|| EngineError::NotFound(run_id.to_string()))
    }

    /// 按 §3.2 表推进一行（乐观锁式 status 匹配，非法对与并发漂移都拒绝）。
    pub(crate) fn advance(
        &self,
        run_id: &str,
        from: ToolRunStatus,
        to: ToolRunStatus,
        terminal: bool,
    ) -> Result<(), EngineError> {
        if !state::is_legal(from, to) {
            return Err(EngineError::InvalidTransition { from, to });
        }
        let now = domain::time::now_iso();
        let updated = self
            .pool
            .with_conn(|c| {
                if to == ToolRunStatus::Running {
                    c.execute(
                        "UPDATE tool_run SET status = ?2, started_at = ?3, updated_at = ?3
                         WHERE run_id = ?1 AND status = ?4",
                        database::rusqlite::params![run_id, to.as_str(), now, from.as_str()],
                    )
                } else if terminal {
                    c.execute(
                        "UPDATE tool_run SET status = ?2, finished_at = ?3, updated_at = ?3
                         WHERE run_id = ?1 AND status = ?4",
                        database::rusqlite::params![run_id, to.as_str(), now, from.as_str()],
                    )
                } else {
                    c.execute(
                        "UPDATE tool_run SET status = ?2, updated_at = ?3
                         WHERE run_id = ?1 AND status = ?4",
                        database::rusqlite::params![run_id, to.as_str(), now, from.as_str()],
                    )
                }
            })
            .map_err(EngineError::from)?;
        if updated == 0 {
            return Err(EngineError::InvalidTransition { from, to });
        }
        Ok(())
    }

    pub(crate) fn resolved_budget(&self, run_id: &str) -> Result<ResolvedBudget, EngineError> {
        let budget_json: Option<String> = self
            .pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT budget_json FROM tool_run WHERE run_id = ?1",
                    [run_id],
                    |r| r.get(0),
                )
            })
            .map_err(EngineError::from)?;
        let text = budget_json.ok_or_else(|| EngineError::Storage("budget_json missing".into()))?;
        let v: Value =
            serde_json::from_str(&text).map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(ResolvedBudget {
            timeout_seconds: v
                .get("timeout_seconds")
                .and_then(Value::as_u64)
                .unwrap_or(600),
            max_output_bytes: v
                .get("max_output_bytes")
                .and_then(Value::as_u64)
                .unwrap_or(1048576),
        })
    }

    pub(crate) fn insert_artifact(
        &self,
        run_id: &str,
        artifact: &envelope::EnvelopeArtifact,
    ) -> Result<(), EngineError> {
        let now = domain::time::now_iso();
        self.pool
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO artifact (artifact_id, run_id, role, uri, sha256, size_bytes,
                     mime_type, provenance, created_at, schema_version)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1)",
                    database::rusqlite::params![
                        artifact.artifact_id,
                        run_id,
                        artifact.role,
                        artifact.uri,
                        artifact.sha256,
                        artifact.size_bytes as i64,
                        artifact.mime_type,
                        artifact.provenance,
                        now
                    ],
                )?;
                Ok::<(), database::rusqlite::Error>(())
            })
            .map_err(EngineError::from)?;
        Ok(())
    }

    pub(crate) fn build_envelope(
        &self,
        row: &RunRow,
        status: ToolRunStatus,
        parts: TerminalParts,
        elapsed_ms: u64,
    ) -> Result<envelope::ToolRunResult, EngineError> {
        let result_final = parts.result;
        let output_bytes = result_final
            .as_ref()
            .map(|r| tool_registry::normalize_json(r).len() as u64)
            .unwrap_or(0);
        let envelope = envelope::ToolRunResult {
            protocol_version: "1".to_string(),
            run_id: row.run_id.clone(),
            tool_id: row.tool_id.clone(),
            tool_version: row.tool_version.clone(),
            status,
            result: result_final,
            summary: parts.summary,
            error: parts.error,
            artifacts: parts.artifacts,
            evidence_refs: Vec::new(),
            usage: envelope::Usage {
                elapsed_ms,
                output_bytes,
                truncated: parts.truncated,
                cpu_ms: None,
                max_rss_bytes: None,
            },
            environment: envelope::Environment {
                runtime_type: parts.runtime_type.clone(),
                runtime_node_id: parts.runtime_node_id.clone(),
                image_digest: None,
                worker_protocol_version: if parts.runtime_type == "python_worker" {
                    Some("1".to_string())
                } else {
                    None
                },
                host_fingerprint_hash: None,
            },
            replay: envelope::Replay {
                input_sanitized: row.input_sanitized.clone(),
                submit_key: row.submit_key.clone(),
                cached: parts.cached,
            },
        };
        envelope::validate(&envelope).map_err(EngineError::Envelope)?;
        // 终态附加上下文落行（error/usage/environment/replay/duration）。
        // finished_at/updated_at 已由 advance 写入；此处补信封派生列。
        let usage_json = json!({
            "elapsed_ms": envelope.usage.elapsed_ms,
            "output_bytes": envelope.usage.output_bytes,
            "truncated": envelope.usage.truncated,
            "cpu_ms": envelope.usage.cpu_ms,
            "max_rss_bytes": envelope.usage.max_rss_bytes,
        });
        let environment_json = json!({
            "runtime_type": envelope.environment.runtime_type,
            "runtime_node_id": envelope.environment.runtime_node_id,
            "image_digest": envelope.environment.image_digest,
            "worker_protocol_version": envelope.environment.worker_protocol_version,
            "host_fingerprint_hash": envelope.environment.host_fingerprint_hash,
        });
        let replay_json = json!({
            "input_sanitized": envelope.replay.input_sanitized,
            "submit_key": envelope.replay.submit_key,
            "cached": envelope.replay.cached,
        });
        let error_json = envelope.error.as_ref().map(|e| {
            json!({
                "category": e.category,
                "code": e.code,
                "message": e.message,
                "retryable": e.retryable,
                "detail_ref": e.detail_ref,
            })
            .to_string()
        });
        let error_category = envelope.error.as_ref().map(|e| e.category.clone());
        self.pool
            .with_conn(|c| {
                c.execute(
                    "UPDATE tool_run SET error_category = ?2, error_json = ?3, usage_json = ?4,
                     environment_json = ?5, replay_json = ?6, duration_ms = ?7 WHERE run_id = ?1",
                    database::rusqlite::params![
                        row.run_id,
                        error_category,
                        error_json,
                        usage_json.to_string(),
                        environment_json.to_string(),
                        replay_json.to_string(),
                        elapsed_ms as i64
                    ],
                )?;
                Ok::<(), database::rusqlite::Error>(())
            })
            .map_err(EngineError::from)?;
        Ok(envelope)
    }
}

pub(crate) struct ResolvedBudget {
    /// 解析后的超时秒（§4.2 映射 timeout_ms 的来源）。
    pub(crate) timeout_seconds: u64,
    /// 解析后的输出字节帽。
    pub(crate) max_output_bytes: u64,
}

/// 终态信封组装的载荷束（降低 build_envelope 参数面；mock 与 adapter 共用）。
pub(crate) struct TerminalParts {
    /// 结构化输出（仅 SUCCEEDED）。
    pub(crate) result: Option<Value>,
    /// 标准错误（非 SUCCEEDED）。
    pub(crate) error: Option<envelope::EnvelopeError>,
    /// 人类可读摘要。
    pub(crate) summary: Option<String>,
    /// 本次运行登记的 Artifact。
    pub(crate) artifacts: Vec<envelope::EnvelopeArtifact>,
    /// §5.4 截断标记。
    pub(crate) truncated: bool,
    /// 幂等命中（replay.cached）。
    pub(crate) cached: bool,
    /// 环境指纹 runtime_type。
    pub(crate) runtime_type: String,
    /// 环境指纹 runtime_node_id。
    pub(crate) runtime_node_id: Option<String>,
}
