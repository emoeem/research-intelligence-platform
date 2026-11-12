//! AgentRun 运行控制持久化面（014 迁移两表：agent_run_control +
//! agent_run_step，P7-4 编排批次，ROADMAP §8.3 P7-5 内容行）。
//!
//! 职责边界：
//! - 协作式控制请求位（[`RunStateStore::request_pause`] /
//!   [`RunStateStore::request_cancel`]）：只写请求时刻，不跃迁状态；执行
//!   循环步间消费（consume-on-observe，[`RunStateStore::consume_pause_request`]
//!   / [`RunStateStore::consume_cancel_request`]，一次性信号，消费即清空）。
//!   语义与 ToolRun D-TC-04 一致（先例 tool_run.cancel_requested_at，005
//!   迁移）；发起态校验用 domain::agent 谓词（RUNNING 才可请求暂停/协作
//!   取消，QUEUED/PAUSED 的取消走直接跃迁，在 [`crate::control`] 控制面）。
//! - 步骤轨迹（[`RunStateStore::record_step`] / [`RunStateStore::steps`]）：
//!   agent_run_step 逐步骤追加，resume 状态续接的持久化权威面（暂停后续跑
//!   只执行轨迹之外的步骤，已完成步骤不重跑）；audit_event 是旁路日志
//!   （emit_best_effort 可缓冲），两者同 correlation_id 可对账但职责不混。
//! - goal_text / retry_of_run_id 落 agent_run_control（不动 006 表形状）。
//!
//! 不在本层做的事：状态跃迁 CAS（复用 [`crate::store::AgentStore::transition_run`]，
//! 域校验 + 时间戳口径单一来源）；控制面编排（[`crate::control::RunControl`]）。

use crate::agent_loop::StepRecord;
use crate::error::AgentError;
use database::rusqlite::OptionalExtension;
use database::{rusqlite, Pool};
use domain::{AgentRunState, RunId};
use serde_json::Value;

/// 轨迹行（agent_run_step 投影 + 入参快照）。
#[derive(Debug, Clone)]
pub struct StoredStep {
    /// run 内步序（0 起）。
    pub seq: usize,
    /// 产生轮次（0 起）。
    pub round: usize,
    /// 步骤记录（与 agent_loop::StepRecord 同形）。
    pub record: StepRecord,
    /// 计划入参快照（重复调用检测跨 resume 重建键集合用）。
    pub input_json: Value,
}

/// run 控制持久化存储（pool 之上；无 Registry 依赖）。
#[derive(Clone)]
pub struct RunStateStore {
    pool: Pool,
}

impl RunStateStore {
    /// 在连接池之上打开。
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// 当前 run 状态（直读 agent_run.state 词值；run 不存在返回 None）。
    pub fn run_state(&self, run_id: &str) -> Result<Option<AgentRunState>, AgentError> {
        let state: Option<String> = self
            .pool
            .with_conn(|c| {
                let mut stmt = c.prepare("SELECT state FROM agent_run WHERE run_id = ?1")?;
                stmt.query_row([run_id], |r| r.get::<_, String>(0))
                    .optional()
                    .map(|o: Option<String>| o)
            })
            .map_err(AgentError::from)?;
        Ok(state.map(|s| {
            AgentRunState::ALL
                .into_iter()
                .find(|v| v.as_str() == s)
                .unwrap_or_else(|| panic!("agent_run {run_id:?} has unknown state {s:?}"))
        }))
    }

    // -----------------------------------------------------------------
    // 控制请求位（协作式，D-TC-04 语义）
    // -----------------------------------------------------------------

    /// 写协作式暂停请求（仅 RUNNING 且未挂请求；返回是否受理）。
    /// 不跃迁状态：暂停生效由执行循环消费请求后 CAS RUNNING→PAUSED。
    pub fn request_pause(&self, run_id: &str) -> Result<bool, AgentError> {
        self.request_control(run_id, "pause_requested_at", |state| {
            state.pause_requestable()
        })
    }

    /// 写协作式取消请求（仅 RUNNING 且未挂请求；返回是否受理）。QUEUED/
    /// PAUSED 的取消不走请求位（直接跃迁，控制面编排）。
    pub fn request_cancel(&self, run_id: &str) -> Result<bool, AgentError> {
        self.request_control(run_id, "cancel_requested_at", |state| {
            state == AgentRunState::Running
        })
    }

    /// 请求位写入共用体：发起态谓词不过即 false（不报错，与 ToolRun
    /// request_cancel 返回 bool 同口径）；CAS UPDATE 零行视作并发漂移，
    /// 同样返回 false。
    fn request_control(
        &self,
        run_id: &str,
        column: &str,
        requestable: impl Fn(AgentRunState) -> bool,
    ) -> Result<bool, AgentError> {
        let Some(state) = self.run_state(run_id)? else {
            return Err(AgentError::RunNotFound(run_id.to_string()));
        };
        if !requestable(state) {
            return Ok(false);
        }
        let now = domain::time::now_iso();
        let sql = format!(
            "UPDATE agent_run_control SET {column} = ?2, updated_at = ?3
             WHERE run_id = ?1 AND {column} IS NULL"
        );
        let updated = self
            .pool
            .with_conn(|c| c.execute(&sql, rusqlite::params![run_id, now, now]))
            .map_err(AgentError::from)?;
        if updated == 0 {
            // 控制行缺席（fresh run 未落行）时补建后重试一次。
            let ensured = self.ensure_control_row(run_id, "")?;
            if !ensured {
                return Ok(false);
            }
            let retried = self
                .pool
                .with_conn(|c| c.execute(&sql, rusqlite::params![run_id, now, now]))
                .map_err(AgentError::from)?;
            if retried == 0 {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// 消费暂停请求（一次性信号）：请求在场即清空并返回 true。
    /// 执行循环在步间调用；消费后由循环 CAS RUNNING→PAUSED。
    pub fn consume_pause_request(&self, run_id: &str) -> Result<bool, AgentError> {
        self.consume_control(run_id, "pause_requested_at")
    }

    /// 消费取消请求（一次性信号）：请求在场即清空并返回 true。
    /// 执行循环在步间调用；消费后由循环 CAS RUNNING→CANCELLED。
    pub fn consume_cancel_request(&self, run_id: &str) -> Result<bool, AgentError> {
        self.consume_control(run_id, "cancel_requested_at")
    }

    fn consume_control(&self, run_id: &str, column: &str) -> Result<bool, AgentError> {
        let sql = format!(
            "UPDATE agent_run_control SET {column} = NULL, updated_at = ?2
             WHERE run_id = ?1 AND {column} IS NOT NULL"
        );
        let now = domain::time::now_iso();
        let updated = self
            .pool
            .with_conn(|c| c.execute(&sql, rusqlite::params![run_id, now]))
            .map_err(AgentError::from)?;
        Ok(updated > 0)
    }

    // -----------------------------------------------------------------
    // goal_text / retry 链（agent_run_control）
    // -----------------------------------------------------------------

    /// 落控制行（fresh run 创建即写 goal；已存在则不动，幂等）。
    /// 返回是否新建了行。
    pub fn ensure_control_row(&self, run_id: &str, goal_text: &str) -> Result<bool, AgentError> {
        let now = domain::time::now_iso();
        let inserted = self
            .pool
            .with_conn(|c| {
                c.execute(
                    "INSERT OR IGNORE INTO agent_run_control
                     (run_id, goal_text, created_at, updated_at, schema_version)
                     VALUES (?1, ?2, ?3, ?3, 1)",
                    rusqlite::params![run_id, goal_text, now],
                )
            })
            .map_err(AgentError::from)?;
        Ok(inserted > 0)
    }

    /// 目标原文（resume 重装配输入；控制行缺席或空串返回 None）。
    pub fn goal_text(&self, run_id: &str) -> Result<Option<String>, AgentError> {
        let goal: Option<String> = self
            .pool
            .with_conn(|c| {
                let mut stmt =
                    c.prepare("SELECT goal_text FROM agent_run_control WHERE run_id = ?1")?;
                stmt.query_row([run_id], |r| r.get::<_, String>(0))
                    .optional()
            })
            .map_err(AgentError::from)?;
        Ok(goal.filter(|g| !g.trim().is_empty()))
    }

    /// 登记重试链（新 run 行创建后写 retry_of_run_id）。
    pub fn set_retry_of(&self, run_id: &str, source_run_id: &str) -> Result<(), AgentError> {
        self.ensure_control_row(run_id, "")?;
        let now = domain::time::now_iso();
        self.pool
            .with_conn(|c| {
                c.execute(
                    "UPDATE agent_run_control SET retry_of_run_id = ?2, updated_at = ?3
                     WHERE run_id = ?1",
                    rusqlite::params![run_id, source_run_id, now],
                )
            })
            .map_err(AgentError::from)?;
        Ok(())
    }

    /// 重试源 run（无重试链返回 None）。
    pub fn retry_of(&self, run_id: &str) -> Result<Option<String>, AgentError> {
        let source: Option<Option<String>> = self
            .pool
            .with_conn(|c| {
                let mut stmt =
                    c.prepare("SELECT retry_of_run_id FROM agent_run_control WHERE run_id = ?1")?;
                stmt.query_row([run_id], |r| r.get::<_, Option<String>>(0))
                    .optional()
            })
            .map_err(AgentError::from)?;
        Ok(source.flatten())
    }

    // -----------------------------------------------------------------
    // 步骤轨迹（agent_run_step）
    // -----------------------------------------------------------------

    /// 追加一条步骤轨迹（seq 由调用方给：run 内全局步序）。
    /// UNIQUE(run_id, seq) 冲突即拒绝（重复落账属编程错误，不静默覆盖）。
    pub fn record_step(
        &self,
        run_id: &RunId,
        seq: usize,
        round: usize,
        input: &Value,
        record: &StepRecord,
    ) -> Result<(), AgentError> {
        let now = domain::time::now_iso();
        let input_text = serde_json::to_string(input)
            .map_err(|e| AgentError::Storage(format!("serialize step input: {e}")))?;
        self.pool
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO agent_run_step
                     (run_id, seq, round, tool_id, tool_version, tool_run_id, status,
                      artifact_id, evidence_id, input_json, content_hash, summary, error,
                      elapsed_ms, created_at, updated_at, schema_version)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15, 1)",
                    rusqlite::params![
                        run_id.as_str(),
                        seq as i64,
                        round as i64,
                        record.tool_id,
                        record.tool_version,
                        record.tool_run_id,
                        record.status,
                        record.artifact_id,
                        record.evidence_id,
                        input_text,
                        record.content_hash,
                        record.summary,
                        record.error,
                        record.elapsed_ms as i64,
                        now
                    ],
                )
            })
            .map_err(AgentError::from)?;
        Ok(())
    }

    /// 全量轨迹（按 seq 升序；resume 续接依据）。
    pub fn steps(&self, run_id: &str) -> Result<Vec<StoredStep>, AgentError> {
        type Row = (
            i64,
            i64,
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            i64,
        );
        let rows: Vec<Row> = self
            .pool
            .with_conn(|c| {
                let mut stmt = c.prepare(
                    "SELECT seq, round, tool_id, tool_version, tool_run_id, status,
                            artifact_id, evidence_id, input_json, content_hash, summary,
                            error, elapsed_ms
                     FROM agent_run_step WHERE run_id = ?1 ORDER BY seq ASC",
                )?;
                let rows = stmt
                    .query_map([run_id], |r| {
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
                        ))
                    })?
                    .collect::<Result<Vec<_>, rusqlite::Error>>()?;
                Ok::<_, rusqlite::Error>(rows)
            })
            .map_err(AgentError::from)?;
        let mut out = Vec::with_capacity(rows.len());
        for (
            seq,
            round,
            tool_id,
            tool_version,
            tool_run_id,
            status,
            artifact_id,
            evidence_id,
            input_json,
            content_hash,
            summary,
            error,
            elapsed_ms,
        ) in rows
        {
            let input: Value = serde_json::from_str(&input_json).unwrap_or(Value::Null);
            let record = StepRecord {
                tool_id,
                tool_version,
                tool_run_id,
                status,
                artifact_id,
                evidence_id,
                content_hash,
                summary,
                error,
                elapsed_ms: u64::try_from(elapsed_ms).unwrap_or(0),
            };
            out.push(StoredStep {
                seq: usize::try_from(seq).unwrap_or(0),
                round: usize::try_from(round).unwrap_or(0),
                record,
                input_json: input,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{DbPath, PoolConfig};
    use std::path::PathBuf;
    use std::time::Duration;

    fn migrated_pool() -> Pool {
        let pool = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(5),
        })
        .unwrap();
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        database::migrate(&pool, &dir).unwrap();
        pool
    }

    fn seed_run(pool: &Pool, run_id: &str, state: AgentRunState) {
        pool.with_conn(|c| -> database::rusqlite::Result<()> {
            c.execute(
                "INSERT OR IGNORE INTO agent_definition (agent_id, version, name, context_policy,
                 capability_refs_json, created_at, updated_at, schema_version)
                 VALUES ('agent.ctrl.test','0.1.0','t','{}','[]','2026-01-01','2026-01-01',1)",
                [],
            )?;
            c.execute(
                "INSERT INTO agent_run (run_id, task_id, agent_id, agent_version, state,
                 memory_refs_json, created_at, updated_at, schema_version)
                 VALUES (?1, NULL, 'agent.ctrl.test', '0.1.0', ?2, '[]',
                 '2026-01-01', '2026-01-01', 1)",
                rusqlite::params![run_id, state.as_str()],
            )?;
            Ok(())
        })
        .unwrap();
    }

    fn step_record(status: &str, hash: Option<&str>) -> StepRecord {
        StepRecord {
            tool_id: "rip.kb.keyword_search".to_string(),
            tool_version: "0.1.0".to_string(),
            tool_run_id: "rn-1".to_string(),
            status: status.to_string(),
            artifact_id: Some("art-1".to_string()),
            evidence_id: Some("ev-1".to_string()),
            content_hash: hash.map(str::to_string),
            summary: None,
            error: None,
            elapsed_ms: 5,
        }
    }

    /// 协作式请求位：仅 RUNNING 可写，消费一次性（消费后不可再消费）。
    #[test]
    fn control_requests_only_running_and_consume_once() {
        let pool = migrated_pool();
        let store = RunStateStore::new(pool.clone());
        seed_run(&pool, "arn-ctl-run", AgentRunState::Running);
        store.ensure_control_row("arn-ctl-run", "目标").unwrap();

        assert!(store.request_pause("arn-ctl-run").unwrap());
        assert!(store.consume_pause_request("arn-ctl-run").unwrap());
        // 消费即清空：二次消费 false。
        assert!(!store.consume_pause_request("arn-ctl-run").unwrap());
        assert!(store.request_cancel("arn-ctl-run").unwrap());
        assert!(store.consume_cancel_request("arn-ctl-run").unwrap());
        assert!(!store.consume_cancel_request("arn-ctl-run").unwrap());

        // 终态/暂停态不可写请求位（谓词拒绝，返回 false 不报错）。
        seed_run(&pool, "arn-ctl-done", AgentRunState::Succeeded);
        store.ensure_control_row("arn-ctl-done", "").unwrap();
        assert!(!store.request_pause("arn-ctl-done").unwrap());
        assert!(!store.request_cancel("arn-ctl-done").unwrap());
        seed_run(&pool, "arn-ctl-paused", AgentRunState::Paused);
        store.ensure_control_row("arn-ctl-paused", "").unwrap();
        assert!(!store.request_pause("arn-ctl-paused").unwrap());
        assert!(!store.request_cancel("arn-ctl-paused").unwrap());

        // 不存在的 run 报 RunNotFound。
        assert!(matches!(
            store.request_pause("arn-ctl-nope"),
            Err(AgentError::RunNotFound(_))
        ));
    }

    /// 步骤轨迹：追加 + 升序读回 + UNIQUE(run_id, seq) 防重。
    #[test]
    fn step_trajectory_roundtrip_and_unique_seq() {
        let pool = migrated_pool();
        let store = RunStateStore::new(pool.clone());
        seed_run(&pool, "arn-traj", AgentRunState::Running);
        let run_id = RunId::try_new("arn-traj").unwrap();
        store
            .record_step(
                &run_id,
                0,
                0,
                &serde_json::json!({"keyword": "a"}),
                &step_record("SUCCEEDED", Some("aa")),
            )
            .unwrap();
        store
            .record_step(
                &run_id,
                1,
                1,
                &serde_json::json!({"keyword": "b"}),
                &step_record("FAILED", Some("bb")),
            )
            .unwrap();

        let steps = store.steps("arn-traj").unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].seq, 0);
        assert_eq!(steps[0].round, 0);
        assert_eq!(steps[0].record.status, "SUCCEEDED");
        assert_eq!(steps[0].record.content_hash.as_deref(), Some("aa"));
        assert_eq!(steps[0].input_json, serde_json::json!({"keyword": "a"}));
        assert_eq!(steps[1].seq, 1);
        assert_eq!(steps[1].record.status, "FAILED");

        // 同 (run_id, seq) 重复落账拒绝（UNIQUE 冲突不静默覆盖）。
        assert!(store
            .record_step(
                &run_id,
                0,
                0,
                &serde_json::json!({}),
                &step_record("SUCCEEDED", None)
            )
            .is_err());
    }

    /// goal_text 与 retry 链读写。
    #[test]
    fn goal_text_and_retry_link() {
        let pool = migrated_pool();
        let store = RunStateStore::new(pool.clone());
        seed_run(&pool, "arn-goal", AgentRunState::Failed);
        store.ensure_control_row("arn-goal", "检索 nmap").unwrap();
        assert_eq!(
            store.goal_text("arn-goal").unwrap().as_deref(),
            Some("检索 nmap")
        );
        // 幂等：已存在行不覆盖 goal。
        store.ensure_control_row("arn-goal", "别的").unwrap();
        assert_eq!(
            store.goal_text("arn-goal").unwrap().as_deref(),
            Some("检索 nmap")
        );

        // 重试链：目标 run 行需先在场（agent_run_control FK）。
        seed_run(&pool, "arn-goal-r2", AgentRunState::Queued);
        store.set_retry_of("arn-goal-r2", "arn-goal").unwrap();
        assert_eq!(
            store.retry_of("arn-goal-r2").unwrap().as_deref(),
            Some("arn-goal")
        );
        assert!(store.retry_of("arn-goal").unwrap().is_none());
        // 空 goal 视为无（resume 拒绝面）。
        seed_run(&pool, "arn-empty-goal", AgentRunState::Paused);
        store.ensure_control_row("arn-empty-goal", "").unwrap();
        assert!(store.goal_text("arn-empty-goal").unwrap().is_none());
    }
}
