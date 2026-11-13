//! Workflow 运行控制面（P8-3）：协作暂停请求位、人工介入（awaiting_approval）
//! 记录与批准注入、运行中实体传播。
//!
//! 语义（014 agent_run_control 同款 consume-on-observe，不重复实现控制面）：
//! - workflow 级 pause/cancel 请求位落 018 workflow_run_control（D-TC-04：
//!   控制面只写请求时刻，run 循环波间消费，运行中节点不抢占）；
//! - agent 节点的暂停/取消传播一律复用 agent-engine [`agent_engine::
//!   control::RunControl`] 与 014 agent_run_control 控制行（运行中的
//!   agent_run 经 trace_id = workflow run_id 反查后写请求位，agent 循环
//!   步间消费后转 PAUSED，节点执行器据此返回 Paused）；
//! - 人工介入：human 节点执行即写 awaiting 记录（approval_node_id +
//!   approval_request_json，approval_json 置空 = 停在 awaiting_approval），
//!   批准参数经 resume 注入后写 approval_json（历史留痕，不清空
//!   approval_node_id）；「停在 awaiting_approval」的判定 =
//!   approval_node_id 非空 且 approval_json 为空 且 run 现态 paused。

use crate::error::WorkflowError;
use database::rusqlite::OptionalExtension;
use database::{rusqlite, Pool};
use domain::WorkflowRunState;
use serde_json::Value;

/// 控制行投影（018 workflow_run_control）。
#[derive(Debug, Clone, PartialEq)]
pub struct ControlRow {
    /// run id。
    pub run_id: String,
    /// 协作暂停请求时刻（None = 无请求；消费后清空）。
    pub pause_requested_at: Option<String>,
    /// 取消请求留痕时刻（None = 无；不消费）。
    pub cancel_requested_at: Option<String>,
    /// 人工介入节点 id（awaiting 与 approved 共用锚）。
    pub approval_node_id: Option<String>,
    /// 人工介入请求载荷（prompt/请求形状原样 JSON 串）。
    pub approval_request_json: Option<String>,
    /// 批准参数（resume 注入后写入；None = 尚未批准）。
    pub approval_json: Option<String>,
    /// 人工介入记录最近更新时刻。
    pub approval_updated_at: Option<String>,
}

impl ControlRow {
    /// 是否停在 awaiting_approval（有介入节点且未批准）。
    pub fn is_awaiting_approval(&self) -> bool {
        self.approval_node_id.is_some() && self.approval_json.is_none()
    }

    /// 人工介入请求载荷（解析失败保留 None，不伪造）。
    pub fn approval_request(&self) -> Option<Value> {
        self.approval_request_json
            .as_deref()
            .and_then(|raw| serde_json::from_str(raw).ok())
    }
}

/// Workflow run 控制面（018 workflow_run_control 读写 + 运行中实体传播）。
#[derive(Clone)]
pub struct WorkflowControl {
    pool: Pool,
}

impl WorkflowControl {
    /// 构造（调用方先行 database::migrate 保证 018 表存在）。
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// 控制行（无 = None；run 首次被控制前允许缺行）。
    pub fn control_row(&self, run_id: &str) -> Result<Option<ControlRow>, WorkflowError> {
        let row = self
            .pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT run_id, pause_requested_at, cancel_requested_at, approval_node_id,
                            approval_request_json, approval_json, approval_updated_at
                     FROM workflow_run_control WHERE run_id = ?1",
                    [run_id],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, Option<String>>(1)?,
                            r.get::<_, Option<String>>(2)?,
                            r.get::<_, Option<String>>(3)?,
                            r.get::<_, Option<String>>(4)?,
                            r.get::<_, Option<String>>(5)?,
                            r.get::<_, Option<String>>(6)?,
                        ))
                    },
                )
                .optional()
            })
            .map_err(WorkflowError::from)?;
        Ok(row.map(
            |(run_id, pause_requested_at, cancel_requested_at, approval_node_id, approval_request_json, approval_json, approval_updated_at)| ControlRow {
                run_id,
                pause_requested_at,
                cancel_requested_at,
                approval_node_id,
                approval_request_json,
                approval_json,
                approval_updated_at,
            },
        ))
    }

    /// 确保控制行存在（幂等；run 受理后即可调用）。
    pub fn ensure_control_row(&self, run_id: &str) -> Result<(), WorkflowError> {
        let now = domain::time::now_iso();
        self.pool
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO workflow_run_control (run_id, created_at, updated_at, schema_version)
                     VALUES (?1, ?2, ?2, 1)
                     ON CONFLICT(run_id) DO NOTHING",
                    rusqlite::params![run_id, now],
                )
                .map_err(database::DbError::Sqlite)
            })
            .map_err(WorkflowError::from)?;
        Ok(())
    }

    /// 写协作暂停请求（D-TC-04 请求位；仅非终态 run 受理，终态拒绝返回
    /// false）。已 paused 的 run 无需请求（返回 false，无副作用）。
    pub fn request_pause(&self, run_id: &str, state: WorkflowRunState) -> Result<bool, WorkflowError> {
        if state.is_terminal() || state == WorkflowRunState::Paused {
            return Ok(false);
        }
        let now = domain::time::now_iso();
        self.ensure_control_row(run_id)?;
        let changed = self
            .pool
            .with_conn(|c| {
                c.execute(
                    "UPDATE workflow_run_control SET pause_requested_at = ?2, updated_at = ?2
                     WHERE run_id = ?1",
                    rusqlite::params![run_id, now],
                )
                .map_err(database::DbError::Sqlite)
            })
            .map_err(WorkflowError::from)?;
        Ok(changed == 1)
    }

    /// 消费暂停请求（run 循环波间调用；consume-on-observe，消费即清空）。
    /// 返回消费前是否有请求。
    pub fn consume_pause_request(&self, run_id: &str) -> Result<bool, WorkflowError> {
        let now = domain::time::now_iso();
        let changed = self
            .pool
            .with_conn(|c| {
                c.execute(
                    "UPDATE workflow_run_control SET pause_requested_at = NULL, updated_at = ?2
                     WHERE run_id = ?1 AND pause_requested_at IS NOT NULL",
                    rusqlite::params![run_id, now],
                )
                .map_err(database::DbError::Sqlite)
            })
            .map_err(WorkflowError::from)?;
        Ok(changed == 1)
    }

    /// 写取消请求留痕（run 头行直接跃迁 cancelled 由执行器完成，本列记
    /// 请求时刻与传播事实；历史面不消费）。
    pub fn record_cancel(&self, run_id: &str) -> Result<(), WorkflowError> {
        let now = domain::time::now_iso();
        self.ensure_control_row(run_id)?;
        self.pool
            .with_conn(|c| {
                c.execute(
                    "UPDATE workflow_run_control SET cancel_requested_at = ?2, updated_at = ?2
                     WHERE run_id = ?1",
                    rusqlite::params![run_id, now],
                )
                .map_err(database::DbError::Sqlite)
            })
            .map_err(WorkflowError::from)?;
        Ok(())
    }

    /// 写人工介入记录（human 节点执行时调用；awaiting = approval_json 置空）。
    pub fn put_approval_request(
        &self,
        run_id: &str,
        node_id: &str,
        request: &Value,
    ) -> Result<(), WorkflowError> {
        let now = domain::time::now_iso();
        let request_text = serde_json::to_string(request)
            .map_err(|e| WorkflowError::InvalidInput(format!("approval request serialize failed: {e}")))?;
        self.ensure_control_row(run_id)?;
        self.pool
            .with_conn(|c| {
                c.execute(
                    "UPDATE workflow_run_control SET approval_node_id = ?2,
                     approval_request_json = ?3, approval_json = NULL, approval_updated_at = ?4,
                     updated_at = ?4 WHERE run_id = ?1",
                    rusqlite::params![run_id, node_id, request_text, now],
                )
                .map_err(database::DbError::Sqlite)
            })
            .map_err(WorkflowError::from)?;
        Ok(())
    }

    /// 注入批准参数（resume 携带 approval 时调用；awaiting 收口 =
    /// approval_json 落值，approval_node_id 保留作节点对账锚）。
    pub fn resolve_approval(&self, run_id: &str, approval: &Value) -> Result<(), WorkflowError> {
        let now = domain::time::now_iso();
        let approval_text = serde_json::to_string(approval)
            .map_err(|e| WorkflowError::InvalidInput(format!("approval serialize failed: {e}")))?;
        self.ensure_control_row(run_id)?;
        self.pool
            .with_conn(|c| {
                c.execute(
                    "UPDATE workflow_run_control SET approval_json = ?2, approval_updated_at = ?3,
                     updated_at = ?3 WHERE run_id = ?1",
                    rusqlite::params![run_id, approval_text, now],
                )
                .map_err(database::DbError::Sqlite)
            })
            .map_err(WorkflowError::from)?;
        Ok(())
    }

    /// 停在 awaiting_approval 的控制行视图（None = 不在等待）。
    pub fn awaiting_approval(&self, run_id: &str) -> Result<Option<ControlRow>, WorkflowError> {
        let row = self.control_row(run_id)?;
        Ok(row.filter(|r| r.is_awaiting_approval()))
    }

    /// 向运行中的 agent_run 传播协作暂停（014 agent_run_control 请求位；
    /// 返回受理的 run 数）。agent 循环步间消费后转 PAUSED。
    pub fn propagate_pause_to_agents(&self, run_id: &str) -> Result<usize, WorkflowError> {
        let agent_run_ids: Vec<String> = self
            .pool
            .with_conn(|c| {
                let mut stmt = c
                    .prepare(
                        "SELECT run_id FROM agent_run WHERE trace_id = ?1 AND state = 'RUNNING'",
                    )
                    .map_err(database::DbError::Sqlite)?;
                let rows = stmt
                    .query_map([run_id], |r| r.get::<_, String>(0))
                    .map_err(database::DbError::Sqlite)?
                    .collect::<Result<Vec<_>, rusqlite::Error>>()
                    .map_err(database::DbError::Sqlite)?;
                Ok::<_, database::DbError>(rows)
            })
            .map_err(WorkflowError::from)?;
        let mut accepted = 0usize;
        if agent_run_ids.is_empty() {
            return Ok(0);
        }
        let store = agent_engine::store::AgentStore::open(
            self.pool.clone(),
            tool_registry::Registry::open(self.pool.clone()),
        );
        let control = agent_engine::control::RunControl::new(
            store,
            agent_engine::state::RunStateStore::new(self.pool.clone()),
        );
        for agent_run_id in agent_run_ids {
            match control.pause(&agent_run_id) {
                Ok(agent_engine::control::PauseOutcome::Requested) => accepted += 1,
                Ok(agent_engine::control::PauseOutcome::Refused(_)) => {}
                Err(agent_engine::error::AgentError::RunNotFound(_)) => {}
                Err(e) => return Err(WorkflowError::Exec(format!(
                    "agent pause propagation failed for {agent_run_id}: {e}"
                ))),
            }
        }
        Ok(accepted)
    }

    /// 向运行中实体传播取消留痕（D-TC-04 记状态）：agent_run（RUNNING →
    /// 014 cancel 请求位；QUEUED/PAUSED → 直接取消）+ tool_run（RUNNING/
    /// WAITING_FOR_RUNNER → cancel_requested_at 落列）。返回留痕条数。
    pub fn propagate_cancel_to_entities(&self, run_id: &str) -> Result<usize, WorkflowError> {
        let mut marked = 0usize;
        let agent_runs: Vec<(String, String)> = self
            .pool
            .with_conn(|c| {
                let mut stmt = c
                    .prepare("SELECT run_id, state FROM agent_run WHERE trace_id = ?1")
                    .map_err(database::DbError::Sqlite)?;
                let rows = stmt
                    .query_map([run_id], |r| {
                        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                    })
                    .map_err(database::DbError::Sqlite)?
                    .collect::<Result<Vec<_>, rusqlite::Error>>()
                    .map_err(database::DbError::Sqlite)?;
                Ok::<_, database::DbError>(rows)
            })
            .map_err(WorkflowError::from)?;
        if !agent_runs.is_empty() {
            let store = agent_engine::store::AgentStore::open(
                self.pool.clone(),
                tool_registry::Registry::open(self.pool.clone()),
            );
            let control = agent_engine::control::RunControl::new(
                store,
                agent_engine::state::RunStateStore::new(self.pool.clone()),
            );
            for (agent_run_id, state) in agent_runs {
                let state = state.to_uppercase();
                let outcome = match state.as_str() {
                    "RUNNING" | "QUEUED" | "PAUSED" => control.cancel(&agent_run_id),
                    // 终态 agent run 无可传播面。
                    _ => Ok(agent_engine::control::CancelOutcome::Refused(state)),
                };
                match outcome {
                    Ok(
                        agent_engine::control::CancelOutcome::Requested
                        | agent_engine::control::CancelOutcome::CancelledNow,
                    ) => marked += 1,
                    Ok(agent_engine::control::CancelOutcome::Refused(_)) => {}
                    Err(agent_engine::error::AgentError::RunNotFound(_)) => {}
                    Err(e) => {
                        return Err(WorkflowError::Exec(format!(
                            "agent cancel propagation failed for {agent_run_id}: {e}"
                        )))
                    }
                }
            }
        }
        let tool_runs: Vec<String> = self
            .pool
            .with_conn(|c| {
                let mut stmt = c
                    .prepare(
                        "SELECT run_id FROM tool_run WHERE trace_id = ?1
                         AND status IN ('RUNNING','WAITING_FOR_RUNNER')",
                    )
                    .map_err(database::DbError::Sqlite)?;
                let rows = stmt
                    .query_map([run_id], |r| r.get::<_, String>(0))
                    .map_err(database::DbError::Sqlite)?
                    .collect::<Result<Vec<_>, rusqlite::Error>>()
                    .map_err(database::DbError::Sqlite)?;
                Ok::<_, database::DbError>(rows)
            })
            .map_err(WorkflowError::from)?;
        if !tool_runs.is_empty() {
            let engine = tool_engine::Engine::open(
                self.pool.clone(),
                tool_registry::Registry::open(self.pool.clone()),
            );
            for tool_run_id in tool_runs {
                if engine.request_cancel(&tool_run_id).unwrap_or(false) {
                    marked += 1;
                }
            }
        }
        Ok(marked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::RunStore;
    use database::{migrate, DbPath, Pool, PoolConfig};
    use serde_json::json;
    use std::path::PathBuf;

    const MIGRATIONS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../migrations");

    fn temp_control(tag: &str) -> (PathBuf, Pool, RunStore, WorkflowControl) {
        let dir = std::env::temp_dir().join(format!(
            "rip-wf-control-test-{}-{}",
            tag,
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("test.db");
        let _ = std::fs::remove_file(&path);
        let pool = Pool::open(PoolConfig {
            path: DbPath::File(path.clone()),
            readonly: false,
            max_connections: 1,
            busy_timeout: std::time::Duration::from_secs(5),
        })
        .expect("open pool");
        migrate(&pool, std::path::Path::new(MIGRATIONS_DIR)).expect("migrate");
        (
            path,
            pool.clone(),
            RunStore::new(pool.clone()),
            WorkflowControl::new(pool),
        )
    }

    #[test]
    fn pause_request_is_consumed_once() {
        let (_dir, _pool, runs, control) = temp_control("pause");
        runs.create_run("wfr-c1", "wf-x", 1, &json!({}), &[])
            .expect("create");
        // 非 running 态也可写请求位（queued 受理；执行器侧只对 running 消费）。
        assert!(control.request_pause("wfr-c1", WorkflowRunState::Running).expect("request"));
        let row = control.control_row("wfr-c1").expect("row").expect("some");
        assert!(row.pause_requested_at.is_some());
        // consume-on-observe：消费一次即清空，二次消费返回 false。
        assert!(control.consume_pause_request("wfr-c1").expect("consume"));
        assert!(!control.consume_pause_request("wfr-c1").expect("consume again"));
        // paused run 拒绝请求位（无副作用）。
        assert!(!control.request_pause("wfr-c1", WorkflowRunState::Paused).expect("paused"));
        // 终态拒绝。
        assert!(!control.request_pause("wfr-c1", WorkflowRunState::Succeeded).expect("terminal"));
    }

    #[test]
    fn approval_request_and_resolve_round_trip() {
        let (_dir, _pool, runs, control) = temp_control("approval");
        runs.create_run("wfr-c2", "wf-x", 1, &json!({}), &[])
            .expect("create");
        control
            .put_approval_request("wfr-c2", "review", &json!({"prompt": "批准?", "key": "approval"}))
            .expect("request");
        let awaiting = control.awaiting_approval("wfr-c2").expect("awaiting").expect("some");
        assert_eq!(awaiting.approval_node_id.as_deref(), Some("review"));
        assert!(awaiting.is_awaiting_approval());
        control
            .resolve_approval("wfr-c2", &json!({"ok": true}))
            .expect("resolve");
        let after = control.control_row("wfr-c2").expect("row").expect("some");
        // 批准后不再 awaiting，但 node 锚与请求载荷保留（对账历史面）。
        assert!(!after.is_awaiting_approval());
        assert_eq!(after.approval_node_id.as_deref(), Some("review"));
        assert_eq!(after.approval_request(), Some(json!({"prompt": "批准?", "key": "approval"})));
        assert_eq!(
            after.approval_json.as_deref(),
            Some("{\"ok\":true}")
        );
    }

    #[test]
    fn cancel_and_agent_propagation_are_safe_without_entities() {
        let (_dir, _pool, runs, control) = temp_control("cancel");
        runs.create_run("wfr-c3", "wf-x", 1, &json!({}), &[])
            .expect("create");
        control.record_cancel("wfr-c3").expect("record");
        let row = control.control_row("wfr-c3").expect("row").expect("some");
        assert!(row.cancel_requested_at.is_some());
        // 无运行中实体：传播计数 0，不炸。
        assert_eq!(control.propagate_pause_to_agents("wfr-c3").expect("pause prop"), 0);
        assert_eq!(control.propagate_cancel_to_entities("wfr-c3").expect("cancel prop"), 0);
    }
}
