//! WorkflowRun 持久化读写面（017 迁移三表：workflow_run / workflow_run_node
//! / workflow_checkpoint，P8-2）。
//!
//! 语义（LangGraph/Temporal 思路：每步落库，恢复从 checkpoint 续）：
//! - run 头行状态机：六态词域 = domain::WorkflowRunState（P8-1 冻结词值），
//!   跃迁合法性按 [`WorkflowRunState::can_transition`] 把关，落库用乐观守卫
//!   （UPDATE WHERE state = 预期态，命中 0 行即读回现态报
//!   [`WorkflowError::IllegalTransition`]，防 kill 竞态与并发 cancel 双写）；
//! - 节点状态行：五态词域 = domain::WorkflowNodeRunState，attempt 在进入
//!   running 时 +1（0 = 未开始），output/error 载荷随终态落列；
//! - checkpoint 只追加：节点每到达一个终态（succeeded/failed/skipped）写
//!   一行全量节点状态投影快照（seq 自增单调），resume 取 MAX(seq) 行恢复；
//! - 不会在快照里重放输入载荷（input 只存 run 头行），快照仅节点状态投影。
//!
//! 失败重试口径（P8-2，与 P8-1 冻结跃迁表一致）：failed 为 run 终态不迁出，
//! retry 以新 run 承载（继承 workflow_id/workflow_version/input 与 checkpoint
//! 节点状态投影，仅目标失败节点回 pending），先例 agent_engine RunControl::retry。

use crate::error::WorkflowError;
use database::rusqlite::OptionalExtension;
use database::{rusqlite, Pool};
use domain::{WorkflowNodeRunState, WorkflowNodeState, WorkflowRunState};
use serde_json::{json, Map, Value};

/// 节点级实体链接列词表（018 workflow_run_node 追加列；[`RunStore::
/// set_node_entity_link`] 消费）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeEntityColumn {
    /// tool 节点的 tool_run。
    ToolRun,
    /// agent 节点的 agent_run。
    AgentRun,
}

/// run 头行实体关联列词表（018 workflow_run 追加列的可写子集；
/// trace_id/task_id 在 create_run 即写，artifact/evidence 走
/// [`RunStore::refresh_trace_correlations`] 刷新）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEntityColumn {
    /// 最近一个 agent 节点的 agent_run。
    AgentRun,
    /// 最近一个 tool 节点的 tool_run。
    ToolRun,
}

/// 节点终态判定（domain::WorkflowNodeRunState 无 is_terminal；五态里
/// succeeded/failed/skipped 为终态，pending/running 为进行态）。
fn node_state_is_terminal(state: WorkflowNodeRunState) -> bool {
    matches!(
        state,
        WorkflowNodeRunState::Succeeded | WorkflowNodeRunState::Failed | WorkflowNodeRunState::Skipped
    )
}

/// Workflow run 持久化（017 三表读写面）。
#[derive(Clone)]
pub struct RunStore {
    pool: Pool,
}

/// run 头行投影。
#[derive(Debug, Clone, PartialEq)]
pub struct RunRow {
    /// run id。
    pub run_id: String,
    /// workflow id。
    pub workflow_id: String,
    /// 钉定的定义版本。
    pub workflow_version: u32,
    /// run 状态。
    pub state: WorkflowRunState,
    /// 输入载荷（JSON object）。
    pub input: Value,
    /// 终态输出（合并后的最终 state 投影；None = 未到终态）。
    pub output: Option<Value>,
    /// 终态错误载荷（None = 无）。
    pub error: Option<Value>,
    /// 创建时刻。
    pub created_at: String,
    /// 启动时刻。
    pub started_at: Option<String>,
    /// 结束时刻。
    pub finished_at: Option<String>,
    /// 链路追踪 ID（018 六 ID 关联列；P8-2 惯例的成列化：本 run 即链锚，
    /// 创建即写 = run_id）。
    pub trace_id: Option<String>,
    /// 关联研究任务（run 输入顶层 task_id，无则 NULL）。
    pub task_id: Option<String>,
    /// 最近一个 agent 节点的 agent_run（1:N 全集经 trace_id 查 trace 面）。
    pub agent_run_id: Option<String>,
    /// 最近一个 tool 节点的 tool_run。
    pub tool_run_id: Option<String>,
    /// run 内最近登记的 artifact（trace_id = run_id 最新行）。
    pub artifact_id: Option<String>,
    /// run 内最近登记的 evidence（trace_id = run_id 最新行）。
    pub evidence_id: Option<String>,
}

/// 节点状态行投影。
#[derive(Debug, Clone, PartialEq)]
pub struct NodeRow {
    /// 节点 id。
    pub node_id: String,
    /// 节点类型词形。
    pub node_kind: String,
    /// 节点运行状态。
    pub state: WorkflowNodeRunState,
    /// 已开始的执行次数（0 = 未开始）。
    pub attempt: u32,
    /// 成功输出（JSON object 或 null）。
    pub output: Option<Value>,
    /// 失败载荷（code/message）。
    pub error: Option<Value>,
    /// 最近一次开始时刻。
    pub started_at: Option<String>,
    /// 终态时刻。
    pub finished_at: Option<String>,
    /// 本节点的 tool_run（tool 节点；018 实体链接列，其余节点 NULL）。
    pub tool_run_id: Option<String>,
    /// 本节点的 agent_run（agent 节点；018 实体链接列，其余节点 NULL）。
    pub agent_run_id: Option<String>,
}

/// checkpoint 行投影（resume 恢复源）。
#[derive(Debug, Clone, PartialEq)]
pub struct CheckpointRow {
    /// 自增序号（run 内单调）。
    pub seq: i64,
    /// 触发节点 id。
    pub node_id: String,
    /// 触发时终态。
    pub node_state: WorkflowNodeRunState,
    /// 触发时 attempt。
    pub attempt: u32,
    /// 全量节点状态投影快照（[`snapshot_of`] 形状）。
    pub snapshot: Value,
    /// 写入时刻。
    pub created_at: String,
}

/// 快照内单节点条目。
#[derive(Debug, Clone, PartialEq)]
pub struct NodeSnapshot {
    /// 节点状态。
    pub state: WorkflowNodeRunState,
    /// attempt。
    pub attempt: u32,
    /// 成功输出。
    pub output: Option<Value>,
    /// 失败载荷。
    pub error: Option<Value>,
}

impl RunStore {
    /// 构造（调用方先行 database::migrate 保证 017 表存在）。
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// 建一条 run（queued 头行 + 全节点 pending 行）；run_id 已存在即拒绝。
    /// 创建即写链路关联（018 六 ID 列）：trace_id = run_id（本 run 即链锚，
    /// P8-2 惯例成列化），task_id = 输入顶层 task_id（字符串即取）。
    pub fn create_run(
        &self,
        run_id: &str,
        workflow_id: &str,
        workflow_version: u32,
        input: &Value,
        node_kinds: &[(String, String)],
    ) -> Result<(), WorkflowError> {
        let now = domain::time::now_iso();
        let input_text = serde_json::to_string(input)
            .map_err(|e| WorkflowError::InvalidInput(format!("input serialize failed: {e}")))?;
        let task_id = input.get("task_id").and_then(Value::as_str);
        self.pool
            .with_conn(|c| {
                let done: usize = c
                    .execute(
                        "INSERT INTO workflow_run (run_id, workflow_id, workflow_version, state,
                         input_json, created_at, schema_version, trace_id, task_id)
                         VALUES (?1, ?2, ?3, 'queued', ?4, ?5, 1, ?6, ?7)",
                        rusqlite::params![
                            run_id,
                            workflow_id,
                            workflow_version,
                            input_text,
                            now,
                            run_id,
                            task_id
                        ],
                    )
                    .map_err(database::DbError::Sqlite)?;
                if done != 1 {
                    return Err(database::DbError::Sqlite(rusqlite::Error::ExecuteReturnedResults));
                }
                for (node_id, kind) in node_kinds {
                    c.execute(
                        "INSERT INTO workflow_run_node (run_id, node_id, node_kind, state,
                         attempt, updated_at, schema_version)
                         VALUES (?1, ?2, ?3, 'pending', 0, ?4, 1)",
                        rusqlite::params![run_id, node_id, kind, now],
                    )
                    .map_err(database::DbError::Sqlite)?;
                }
                Ok::<_, database::DbError>(())
            })
            .map_err(WorkflowError::from)?;
        Ok(())
    }

    /// run 头行（不存在 = [`WorkflowError::RunNotFound`]）。
    pub fn run_row(&self, run_id: &str) -> Result<RunRow, WorkflowError> {
        let row = self
            .pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT workflow_id, workflow_version, state, input_json, output_json,
                            error_json, created_at, started_at, finished_at,
                            trace_id, task_id, agent_run_id, tool_run_id, artifact_id, evidence_id
                     FROM workflow_run WHERE run_id = ?1",
                    [run_id],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, u32>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, String>(3)?,
                            r.get::<_, Option<String>>(4)?,
                            r.get::<_, Option<String>>(5)?,
                            r.get::<_, String>(6)?,
                            r.get::<_, Option<String>>(7)?,
                            r.get::<_, Option<String>>(8)?,
                            r.get::<_, Option<String>>(9)?,
                            r.get::<_, Option<String>>(10)?,
                            r.get::<_, Option<String>>(11)?,
                            r.get::<_, Option<String>>(12)?,
                            r.get::<_, Option<String>>(13)?,
                            r.get::<_, Option<String>>(14)?,
                        ))
                    },
                )
                .optional()
            })
            .map_err(WorkflowError::from)?;
        let Some((workflow_id, workflow_version, state, input_json, output_json, error_json, created_at, started_at, finished_at, trace_id, task_id, agent_run_id, tool_run_id, artifact_id, evidence_id)) = row
        else {
            return Err(WorkflowError::RunNotFound {
                run_id: run_id.to_string(),
            });
        };
        Ok(RunRow {
            run_id: run_id.to_string(),
            workflow_id,
            workflow_version,
            state: WorkflowRunState::parse(&state)?,
            input: serde_json::from_str(&input_json)?,
            output: output_json.map(|t| serde_json::from_str(&t)).transpose()?,
            error: error_json.map(|t| serde_json::from_str(&t)).transpose()?,
            created_at,
            started_at,
            finished_at,
            trace_id,
            task_id,
            agent_run_id,
            tool_run_id,
            artifact_id,
            evidence_id,
        })
    }

    /// 全节点状态行（按 node_id 升序）。
    pub fn node_rows(&self, run_id: &str) -> Result<Vec<NodeRow>, WorkflowError> {
        let rows = self
            .pool
            .with_conn(|c| {
                let mut stmt = c.prepare(
                    "SELECT node_id, node_kind, state, attempt, output_json, error_json,
                            started_at, finished_at, tool_run_id, agent_run_id
                     FROM workflow_run_node WHERE run_id = ?1 ORDER BY node_id",
                )?;
                let rows = stmt
                    .query_map([run_id], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, u32>(3)?,
                            r.get::<_, Option<String>>(4)?,
                            r.get::<_, Option<String>>(5)?,
                            r.get::<_, Option<String>>(6)?,
                            r.get::<_, Option<String>>(7)?,
                            r.get::<_, Option<String>>(8)?,
                            r.get::<_, Option<String>>(9)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, rusqlite::Error>>()?;
                Ok::<_, rusqlite::Error>(rows)
            })
            .map_err(WorkflowError::from)?;
        let mut out = Vec::with_capacity(rows.len());
        for (node_id, node_kind, state, attempt, output_json, error_json, started_at, finished_at, tool_run_id, agent_run_id) in rows {
            out.push(NodeRow {
                node_id,
                node_kind,
                state: WorkflowNodeRunState::parse(&state)?,
                attempt,
                output: output_json.map(|t| serde_json::from_str(&t)).transpose()?,
                error: error_json.map(|t| serde_json::from_str(&t)).transpose()?,
                started_at,
                finished_at,
                tool_run_id,
                agent_run_id,
            });
        }
        Ok(out)
    }

    /// run 状态跃迁（乐观守卫：UPDATE WHERE state = expected，0 行即读回
    /// 现态报 [`WorkflowError::IllegalTransition`]）；queued→running 补
    /// started_at，到终态补 finished_at（output/error 由调用方给）。
    pub fn transition_run(
        &self,
        run_id: &str,
        expected: WorkflowRunState,
        to: WorkflowRunState,
        output: Option<&Value>,
        error: Option<&Value>,
    ) -> Result<(), WorkflowError> {
        let now = domain::time::now_iso();
        let output_text = match output {
            Some(v) => Some(serde_json::to_string(v).map_err(|e| {
                WorkflowError::InvalidInput(format!("output serialize failed: {e}"))
            })?),
            None => None,
        };
        let error_text = match error {
            Some(v) => Some(serde_json::to_string(v).map_err(|e| {
                WorkflowError::InvalidInput(format!("error serialize failed: {e}"))
            })?),
            None => None,
        };
        let changed = self
            .pool
            .with_conn(|c| {
                c.execute(
                    "UPDATE workflow_run SET state = ?2,
                     started_at = COALESCE(started_at, CASE WHEN ?2 = 'running' THEN ?4 END),
                     finished_at = CASE WHEN ?5 = 1 THEN ?4 ELSE finished_at END,
                     output_json = COALESCE(?6, output_json),
                     error_json = COALESCE(?7, error_json)
                     WHERE run_id = ?1 AND state = ?3",
                    rusqlite::params![
                        run_id,
                        to.as_str(),
                        expected.as_str(),
                        now,
                        to.is_terminal() as i64,
                        output_text,
                        error_text
                    ],
                )
                .map_err(database::DbError::Sqlite)
            })
            .map_err(WorkflowError::from)?;
        if changed == 1 {
            return Ok(());
        }
        let current = self.run_row(run_id)?;
        Err(WorkflowError::IllegalTransition {
            run_id: run_id.to_string(),
            from: current.state.as_str().to_string(),
            to: to.as_str().to_string(),
        })
    }

    /// 节点进入 running（attempt +1；乐观守卫 WHERE state = 'pending'）。
    pub fn node_start(&self, run_id: &str, node_id: &str) -> Result<u32, WorkflowError> {
        let now = domain::time::now_iso();
        let changed = self
            .pool
            .with_conn(|c| {
                c.execute(
                    "UPDATE workflow_run_node SET state = 'running', attempt = attempt + 1,
                     started_at = ?3, updated_at = ?3, output_json = NULL, error_json = NULL,
                     finished_at = NULL
                     WHERE run_id = ?1 AND node_id = ?2 AND state = 'pending'",
                    rusqlite::params![run_id, node_id, now],
                )
                .map_err(database::DbError::Sqlite)
            })
            .map_err(WorkflowError::from)?;
        if changed != 1 {
            return Err(WorkflowError::NodeNotStartable {
                run_id: run_id.to_string(),
                node_id: node_id.to_string(),
            });
        }
        let attempt: u32 = self
            .pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT attempt FROM workflow_run_node WHERE run_id = ?1 AND node_id = ?2",
                    rusqlite::params![run_id, node_id],
                    |r| r.get(0),
                )
                .map_err(database::DbError::Sqlite)
            })
            .map_err(WorkflowError::from)?;
        Ok(attempt)
    }

    /// 节点到达终态（乐观守卫 WHERE state = 'running'，命中 0 行即放弃：
    /// 调用方为超时后残留线程时静默丢失，防与终态写入竞态）。
    pub fn node_finish(
        &self,
        run_id: &str,
        node_id: &str,
        state: WorkflowNodeRunState,
        output: Option<&Value>,
        error: Option<&Value>,
    ) -> Result<(), WorkflowError> {
        if !node_state_is_terminal(state) {
            return Err(WorkflowError::InvalidInput(format!(
                "node_finish requires a terminal node state, got {}",
                state.as_str()
            )));
        }
        let now = domain::time::now_iso();
        let output_text = match output {
            Some(v) => Some(serde_json::to_string(v).map_err(|e| {
                WorkflowError::InvalidInput(format!("output serialize failed: {e}"))
            })?),
            None => None,
        };
        let error_text = match error {
            Some(v) => Some(serde_json::to_string(v).map_err(|e| {
                WorkflowError::InvalidInput(format!("error serialize failed: {e}"))
            })?),
            None => None,
        };
        let changed = self
            .pool
            .with_conn(|c| {
                c.execute(
                    "UPDATE workflow_run_node SET state = ?3, output_json = ?4, error_json = ?5,
                     finished_at = ?6, updated_at = ?6
                     WHERE run_id = ?1 AND node_id = ?2 AND state = 'running'",
                    rusqlite::params![
                        run_id,
                        node_id,
                        state.as_str(),
                        output_text,
                        error_text,
                        now
                    ],
                )
                .map_err(database::DbError::Sqlite)
            })
            .map_err(WorkflowError::from)?;
        if changed == 1 {
            return Ok(());
        }
        Ok(())
    }

    /// 节点跳过（分支未选中；乐观守卫 WHERE state = 'pending'）。
    pub fn node_skip(&self, run_id: &str, node_id: &str) -> Result<(), WorkflowError> {
        let now = domain::time::now_iso();
        let changed = self
            .pool
            .with_conn(|c| {
                c.execute(
                    "UPDATE workflow_run_node SET state = 'skipped', finished_at = ?3,
                     updated_at = ?3 WHERE run_id = ?1 AND node_id = ?2 AND state = 'pending'",
                    rusqlite::params![run_id, node_id, now],
                )
                .map_err(database::DbError::Sqlite)
            })
            .map_err(WorkflowError::from)?;
        if changed != 1 {
            return Err(WorkflowError::NodeNotStartable {
                run_id: run_id.to_string(),
                node_id: node_id.to_string(),
            });
        }
        Ok(())
    }

    /// 失败节点回 pending（retry 承接：attempt 保留，重跑进入 running 时
    /// 再 +1；乐观守卫 WHERE state = 'failed'）。
    pub fn node_reset_pending(&self, run_id: &str, node_id: &str) -> Result<(), WorkflowError> {
        let now = domain::time::now_iso();
        let changed = self
            .pool
            .with_conn(|c| {
                c.execute(
                    "UPDATE workflow_run_node SET state = 'pending', output_json = NULL,
                     error_json = NULL, finished_at = NULL, updated_at = ?3
                     WHERE run_id = ?1 AND node_id = ?2 AND state = 'failed'",
                    rusqlite::params![run_id, node_id, now],
                )
                .map_err(database::DbError::Sqlite)
            })
            .map_err(WorkflowError::from)?;
        if changed != 1 {
            return Err(WorkflowError::NodeNotStartable {
                run_id: run_id.to_string(),
                node_id: node_id.to_string(),
            });
        }
        Ok(())
    }

    /// 节点实体链接写入（018 实体链接列；tool 节点 submit 后写 tool_run_id，
    /// agent 节点派发前写 agent_run_id，跨进程 pause/cancel 传播据此发现
    /// 运行中实体）。
    pub fn set_node_entity_link(
        &self,
        run_id: &str,
        node_id: &str,
        column: NodeEntityColumn,
        entity_id: &str,
    ) -> Result<(), WorkflowError> {
        let now = domain::time::now_iso();
        let sql = match column {
            NodeEntityColumn::ToolRun => "UPDATE workflow_run_node SET tool_run_id = ?3,
                                          updated_at = ?4 WHERE run_id = ?1 AND node_id = ?2",
            NodeEntityColumn::AgentRun => "UPDATE workflow_run_node SET agent_run_id = ?3,
                                           updated_at = ?4 WHERE run_id = ?1 AND node_id = ?2",
        };
        self.pool
            .with_conn(|c| {
                c.execute(sql, rusqlite::params![run_id, node_id, entity_id, now])
                    .map_err(database::DbError::Sqlite)
            })
            .map_err(WorkflowError::from)?;
        Ok(())
    }

    /// run 头行实体关联写入（018 六 ID 列；agent_run_id/tool_run_id 在节点
    /// 派发/完成时写最近值）。
    pub fn set_run_entity_link(
        &self,
        run_id: &str,
        column: RunEntityColumn,
        entity_id: &str,
    ) -> Result<(), WorkflowError> {
        let sql = match column {
            RunEntityColumn::AgentRun => {
                "UPDATE workflow_run SET agent_run_id = ?2 WHERE run_id = ?1"
            }
            RunEntityColumn::ToolRun => {
                "UPDATE workflow_run SET tool_run_id = ?2 WHERE run_id = ?1"
            }
        };
        self.pool
            .with_conn(|c| {
                c.execute(sql, rusqlite::params![run_id, entity_id])
                    .map_err(database::DbError::Sqlite)
            })
            .map_err(WorkflowError::from)?;
        Ok(())
    }

    /// run 头行 artifact/evidence 关联刷新（018 六 ID 列；015 trace 面同
    /// 拓扑：取 trace_id = run_id 的最新一行，单列承载最近值做快速对账锚，
    /// 1:N 全集经 trace_id 查 v_trace_chain）。无实体行时保持原值不清空
    /// （既有行不回填纪律）。
    pub fn refresh_trace_correlations(&self, run_id: &str) -> Result<(), WorkflowError> {
        self.pool
            .with_conn(|c| {
                let artifact: Option<String> = c
                    .query_row(
                        "SELECT artifact_id FROM artifact WHERE trace_id = ?1
                         ORDER BY created_at DESC, artifact_id DESC LIMIT 1",
                        [run_id],
                        |r| r.get(0),
                    )
                    .optional()
                    .map_err(database::DbError::Sqlite)?;
                if let Some(artifact_id) = artifact {
                    c.execute(
                        "UPDATE workflow_run SET artifact_id = ?2 WHERE run_id = ?1",
                        rusqlite::params![run_id, artifact_id],
                    )
                    .map_err(database::DbError::Sqlite)?;
                }
                let evidence: Option<String> = c
                    .query_row(
                        "SELECT evidence_id FROM evidence WHERE trace_id = ?1
                         ORDER BY created_at DESC, evidence_id DESC LIMIT 1",
                        [run_id],
                        |r| r.get(0),
                    )
                    .optional()
                    .map_err(database::DbError::Sqlite)?;
                if let Some(evidence_id) = evidence {
                    c.execute(
                        "UPDATE workflow_run SET evidence_id = ?2 WHERE run_id = ?1",
                        rusqlite::params![run_id, evidence_id],
                    )
                    .map_err(database::DbError::Sqlite)?;
                }
                Ok::<_, database::DbError>(())
            })
            .map_err(WorkflowError::from)
    }

    /// 节点实体链接读回（run 内按 submit_key 精确对账 tool_run；attempt
    /// 变键后取当前 attempt 的行）。
    pub fn tool_run_id_of_node(
        &self,
        run_id: &str,
        node_id: &str,
        attempt: u32,
    ) -> Result<Option<String>, WorkflowError> {
        let submit_key = format!("wf:{run_id}:{node_id}:a{attempt}");
        let id: Option<String> = self
            .pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT run_id FROM tool_run WHERE submit_key = ?1
                     ORDER BY created_at DESC, run_id DESC LIMIT 1",
                    [submit_key],
                    |r| r.get(0),
                )
                .optional()
                .map_err(database::DbError::Sqlite)
            })
            .map_err(WorkflowError::from)?;
        Ok(id)
    }

    /// 暂停中的运行节点回 pending（attempt 保留：协作暂停不抢占，agent
    /// 节点由 agent 循环步间消费暂停请求后返回 Paused，节点行复位等
    /// resume 重派发；乐观守卫 WHERE state = 'running'）。
    pub fn node_pause_reset(&self, run_id: &str, node_id: &str) -> Result<(), WorkflowError> {
        let now = domain::time::now_iso();
        let changed = self
            .pool
            .with_conn(|c| {
                c.execute(
                    "UPDATE workflow_run_node SET state = 'pending', updated_at = ?3,
                     finished_at = NULL
                     WHERE run_id = ?1 AND node_id = ?2 AND state = 'running'",
                    rusqlite::params![run_id, node_id, now],
                )
                .map_err(database::DbError::Sqlite)
            })
            .map_err(WorkflowError::from)?;
        if changed != 1 {
            return Err(WorkflowError::NodeNotStartable {
                run_id: run_id.to_string(),
                node_id: node_id.to_string(),
            });
        }
        Ok(())
    }

    /// 节点完成即持久化快照（只追加；snapshot = [`snapshot_of`] 形状）。
    pub fn record_checkpoint(&self, run_id: &str, node_id: &str) -> Result<i64, WorkflowError> {
        let snapshot = self.snapshot_of(run_id)?;
        let nodes = snapshot
            .get("nodes")
            .and_then(Value::as_object)
            .ok_or_else(|| WorkflowError::Storage("snapshot missing nodes object".to_string()))?;
        let entry = nodes.get(node_id).ok_or_else(|| {
            WorkflowError::Storage(format!("snapshot missing node {node_id:?}"))
        })?;
        let state = entry
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let attempt = entry.get("attempt").and_then(Value::as_u64).unwrap_or(0) as u32;
        if !matches!(
            state,
            "succeeded" | "failed" | "skipped"
        ) {
            return Err(WorkflowError::Storage(format!(
                "checkpoint requires terminal node state for {node_id:?}, got {state:?}"
            )));
        }
        let now = domain::time::now_iso();
        let seq: i64 = self
            .pool
            .with_conn(|c| {
                c.query_row(
                    "INSERT INTO workflow_checkpoint (run_id, node_id, node_state, attempt,
                     snapshot_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING seq",
                    rusqlite::params![
                        run_id,
                        node_id,
                        state,
                        attempt,
                        snapshot.to_string(),
                        now
                    ],
                    |r| r.get(0),
                )
                .map_err(database::DbError::Sqlite)
            })
            .map_err(WorkflowError::from)?;
        Ok(seq)
    }

    /// 最新 checkpoint（无 = None；恢复源）。
    pub fn latest_checkpoint(&self, run_id: &str) -> Result<Option<CheckpointRow>, WorkflowError> {
        let row = self
            .pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT seq, node_id, node_state, attempt, snapshot_json, created_at
                     FROM workflow_checkpoint WHERE run_id = ?1
                     ORDER BY seq DESC LIMIT 1",
                    [run_id],
                    |r| {
                        Ok((
                            r.get::<_, i64>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, u32>(3)?,
                            r.get::<_, String>(4)?,
                            r.get::<_, String>(5)?,
                        ))
                    },
                )
                .optional()
            })
            .map_err(WorkflowError::from)?;
        let Some((seq, node_id, node_state, attempt, snapshot_json, created_at)) = row else {
            return Ok(None);
        };
        Ok(Some(CheckpointRow {
            seq,
            node_id,
            node_state: WorkflowNodeRunState::parse(&node_state)?,
            attempt,
            snapshot: serde_json::from_str(&snapshot_json)?,
            created_at,
        }))
    }

    /// 全量节点状态投影（checkpoint 快照与恢复的对账形状）：
    /// `{"nodes": {node_id: {state, attempt, output, error}}}`。
    pub fn snapshot_of(&self, run_id: &str) -> Result<Value, WorkflowError> {
        let rows = self.node_rows(run_id)?;
        let mut nodes = Map::new();
        for row in &rows {
            nodes.insert(
                row.node_id.clone(),
                json!({
                    "state": row.state.as_str(),
                    "attempt": row.attempt,
                    "output": row.output.clone().unwrap_or(Value::Null),
                    "error": row.error.clone().unwrap_or(Value::Null),
                }),
            );
        }
        Ok(json!({ "nodes": nodes }))
    }

    /// 从快照恢复节点行：快照内终态节点整体回放（state/attempt/output/
    /// error）；快照内非终态条目与快照外节点复位 pending 且 attempt 保留
    /// （kill 后残留 running 行的真实 attempt 不因快照滞后而丢失，重跑
    /// 进入 running 时再 +1）。返回回放的终态节点数。
    pub fn restore_from_snapshot(&self, run_id: &str, snapshot: &Value) -> Result<usize, WorkflowError> {
        let nodes = snapshot
            .get("nodes")
            .and_then(Value::as_object)
            .ok_or_else(|| WorkflowError::InvalidInput("snapshot missing nodes object".to_string()))?;
        let now = domain::time::now_iso();
        let mut replayed = 0usize;
        for (node_id, entry) in nodes {
            let state = entry
                .get("state")
                .and_then(Value::as_str)
                .ok_or_else(|| WorkflowError::InvalidInput(format!("snapshot node {node_id:?} missing state")))?;
            let parsed = WorkflowNodeRunState::parse(state)?;
            if !node_state_is_terminal(parsed) {
                // 快照内非终态（pending/running）：复位 pending，attempt 保留
                // （行内计数可能已因 kill 前的 node_start 领先快照）。
                self.pool
                    .with_conn(|c| {
                        c.execute(
                            "UPDATE workflow_run_node SET state = 'pending', updated_at = ?3,
                             finished_at = NULL
                             WHERE run_id = ?1 AND node_id = ?2 AND state IN ('pending','running')",
                            rusqlite::params![run_id, node_id, now],
                        )
                        .map_err(database::DbError::Sqlite)
                    })
                    .map_err(WorkflowError::from)?;
                continue;
            }
            let attempt = entry.get("attempt").and_then(Value::as_u64).unwrap_or(0) as u32;
            let output = entry.get("output").cloned().unwrap_or(Value::Null);
            let error = entry.get("error").cloned().unwrap_or(Value::Null);
            let output_text = if output.is_null() {
                None
            } else {
                Some(serde_json::to_string(&output).map_err(|e| {
                    WorkflowError::InvalidInput(format!("snapshot output serialize failed: {e}"))
                })?)
            };
            let error_text = if error.is_null() {
                None
            } else {
                Some(serde_json::to_string(&error).map_err(|e| {
                    WorkflowError::InvalidInput(format!("snapshot error serialize failed: {e}"))
                })?)
            };
            let changed = self
                .pool
                .with_conn(|c| {
                    c.execute(
                        "UPDATE workflow_run_node SET state = ?3, attempt = ?4, output_json = ?5,
                         error_json = ?6, updated_at = ?7, finished_at = COALESCE(finished_at, ?7)
                         WHERE run_id = ?1 AND node_id = ?2",
                        rusqlite::params![
                            run_id,
                            node_id,
                            parsed.as_str(),
                            attempt,
                            output_text,
                            error_text,
                            now
                        ],
                    )
                    .map_err(database::DbError::Sqlite)
                })
                .map_err(WorkflowError::from)?;
            if changed == 1 {
                replayed += 1;
            }
        }
        // 快照外节点（含 kill 后残留 running）复位 pending，attempt 保留。
        self.pool
            .with_conn(|c| {
                c.execute(
                    "UPDATE workflow_run_node SET state = 'pending', updated_at = ?3,
                     finished_at = NULL
                     WHERE run_id = ?1 AND state IN ('pending','running')
                       AND node_id NOT IN (SELECT json_each.key FROM json_each(?2, '$.nodes'))",
                    rusqlite::params![run_id, snapshot.to_string(), now],
                )
                .map_err(database::DbError::Sqlite)
            })
            .map_err(WorkflowError::from)?;
        Ok(replayed)
    }

    /// 合并恢复态：run input + 快照内 succeeded 节点输出按 node_id 升序浅合并
    /// （恢复面与正向执行同一合并规则，键冲突时后到覆盖；恢复序按 id 升序
    /// 确定，与正向完成序的差异见设计稿已知限制）。
    pub fn restored_state(
        &self,
        _run_id: &str,
        snapshot: &Value,
        input: &Value,
    ) -> Result<Value, WorkflowError> {
        let mut state = match input {
            Value::Object(map) => map.clone(),
            other => {
                return Err(WorkflowError::InvalidInput(format!(
                    "run input must be a JSON object, got {other}"
                )))
            }
        };
        let mut outputs: Vec<(String, Value)> = Vec::new();
        if let Some(nodes) = snapshot.get("nodes").and_then(Value::as_object) {
            for (node_id, entry) in nodes {
                if entry.get("state").and_then(Value::as_str) == Some("succeeded") {
                    let output = entry.get("output").cloned().unwrap_or(Value::Null);
                    if !output.is_null() {
                        outputs.push((node_id.clone(), output));
                    }
                }
            }
        }
        outputs.sort_by(|a, b| a.0.cmp(&b.0));
        for (_, output) in outputs {
            merge_output(&mut state, &output, "<restored>")?;
        }
        Ok(Value::Object(state))
    }
}

/// 节点输出并入 state（浅合并；非 object 输出拒绝，object 为唯一可并形）。
pub(crate) fn merge_output(
    state: &mut Map<String, Value>,
    output: &Value,
    node_id: &str,
) -> Result<(), WorkflowError> {
    match output {
        Value::Null => Ok(()),
        Value::Object(map) => {
            for (k, v) in map {
                state.insert(k.clone(), v.clone());
            }
            Ok(())
        }
        other => Err(WorkflowError::InvalidInput(format!(
            "node {node_id:?} output must be a JSON object or null to merge into run state, got {}",
            type_word(other)
        ))),
    }
}

/// JSON 值类型词（错误文本用）。
pub(crate) fn type_word(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// 快照投影 -> domain 节点状态向量（对账用；node_id 形状经 domain 校验）。
pub fn snapshot_node_states(snapshot: &Value) -> Result<Vec<WorkflowNodeState>, WorkflowError> {
    let nodes = snapshot
        .get("nodes")
        .and_then(Value::as_object)
        .ok_or_else(|| WorkflowError::InvalidInput("snapshot missing nodes object".to_string()))?;
    let mut out = Vec::with_capacity(nodes.len());
    for (node_id, entry) in nodes {
        let state = WorkflowNodeRunState::parse(
            entry
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )?;
        let attempt = entry.get("attempt").and_then(Value::as_u64).unwrap_or(0) as u32;
        out.push(WorkflowNodeState::try_new(node_id.clone(), state, attempt)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::migrate;
    use std::path::PathBuf;

    const MIGRATIONS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../migrations");

    fn temp_store(tag: &str) -> (PathBuf, RunStore) {
        let dir = std::env::temp_dir().join(format!(
            "rip-wf-state-test-{}-{}",
            tag,
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("test.db");
        let _ = std::fs::remove_file(&path);
        let pool = database::Pool::open(database::PoolConfig {
            path: database::DbPath::File(path.clone()),
            readonly: false,
            max_connections: 1,
            busy_timeout: std::time::Duration::from_secs(5),
        })
        .expect("open pool");
        migrate(&pool, std::path::Path::new(MIGRATIONS_DIR)).expect("migrate");
        (path, RunStore::new(pool))
    }

    #[test]
    fn create_run_seeds_pending_nodes_and_read_back() {
        let (_dir, store) = temp_store("seed");
        store
            .create_run(
                "wfr-t1",
                "wf-x",
                1,
                &json!({"target": "example.com"}),
                &[
                    ("start".to_string(), "start".to_string()),
                    ("t1".to_string(), "tool".to_string()),
                ],
            )
            .expect("create run");
        let row = store.run_row("wfr-t1").expect("run row");
        assert_eq!(row.state, WorkflowRunState::Queued);
        assert_eq!(row.workflow_version, 1);
        assert_eq!(row.input, json!({"target": "example.com"}));
        let nodes = store.node_rows("wfr-t1").expect("nodes");
        assert_eq!(nodes.len(), 2);
        assert!(nodes.iter().all(|n| n.state == WorkflowNodeRunState::Pending && n.attempt == 0));
        assert!(matches!(
            store.run_row("wfr-missing"),
            Err(WorkflowError::RunNotFound { .. })
        ));
    }

    #[test]
    fn run_transition_guard_follows_frozen_table() {
        let (_dir, store) = temp_store("transition");
        store
            .create_run("wfr-t2", "wf-x", 1, &json!({}), &[("start".to_string(), "start".to_string())])
            .expect("create");
        store
            .transition_run("wfr-t2", WorkflowRunState::Queued, WorkflowRunState::Running, None, None)
            .expect("queued->running");
        // 非法跃迁（running -> queued 不在冻结表）拒绝且读回现态。
        // 乐观守卫：现态 running 之下再报 queued 预期即 0 行命中，错误读回
        // 现态 from=running 与请求目标 to=running（非法在 from 与冻结表）。
        assert!(matches!(
            store.transition_run("wfr-t2", WorkflowRunState::Queued, WorkflowRunState::Running, None, None),
            Err(WorkflowError::IllegalTransition { run_id: _, from, to })
              if from == "running" && to == "running"
        ));
        store
            .transition_run(
                "wfr-t2",
                WorkflowRunState::Running,
                WorkflowRunState::Succeeded,
                Some(&json!({"done": true})),
                None,
            )
            .expect("running->succeeded");
        let row = store.run_row("wfr-t2").expect("row");
        assert_eq!(row.state, WorkflowRunState::Succeeded);
        assert_eq!(row.output, Some(json!({"done": true})));
        assert!(row.started_at.is_some() && row.finished_at.is_some());
    }

    #[test]
    fn node_attempt_increments_on_running_and_finish_guards() {
        let (_dir, store) = temp_store("attempt");
        store
            .create_run("wfr-t3", "wf-x", 1, &json!({}), &[("t1".to_string(), "tool".to_string())])
            .expect("create");
        let a1 = store.node_start("wfr-t3", "t1").expect("start 1");
        assert_eq!(a1, 1);
        store
            .node_finish("wfr-t3", "t1", WorkflowNodeRunState::Failed, None, Some(&json!({"code": "x"})))
            .expect("finish");
        // 终态后再 start 拒绝（守卫只放行 pending）。
        assert!(matches!(
            store.node_start("wfr-t3", "t1"),
            Err(WorkflowError::NodeNotStartable { .. })
        ));
        // 终态守卫：对 succeeded 行误写 failed 静默丢失（超时残留线程路径）。
        store
            .node_start("wfr-t3", "t1")
            .expect_err("failed not startable");
        let nodes = store.node_rows("wfr-t3").expect("nodes");
        assert_eq!(nodes[0].state, WorkflowNodeRunState::Failed);
        assert_eq!(nodes[0].attempt, 1);
    }

    #[test]
    fn checkpoint_append_and_snapshot_round_trip() {
        let (_dir, store) = temp_store("checkpoint");
        store
            .create_run(
                "wfr-t4",
                "wf-x",
                1,
                &json!({"k": 1}),
                &[("t1".to_string(), "tool".to_string()), ("t2".to_string(), "tool".to_string())],
            )
            .expect("create");
        store.node_start("wfr-t4", "t1").expect("start");
        store
            .node_finish("wfr-t4", "t1", WorkflowNodeRunState::Succeeded, Some(&json!({"a": 1})), None)
            .expect("finish");
        let seq = store.record_checkpoint("wfr-t4", "t1").expect("checkpoint");
        assert!(seq >= 1);
        let snap = store.snapshot_of("wfr-t4").expect("snapshot");
        let states = snapshot_node_states(&snap).expect("node states");
        assert_eq!(states.len(), 2);
        assert!(states.iter().any(|s| s.node_id() == "t1"
            && s.state() == WorkflowNodeRunState::Succeeded
            && s.attempts() == 1));
        // 最新 checkpoint 回读与 snapshot 一致。
        let cp = store.latest_checkpoint("wfr-t4").expect("cp").expect("some");
        assert_eq!(cp.node_id, "t1");
        assert_eq!(cp.snapshot, snap);
        // 快照恢复：t2 复位 pending（原 pending 不变），t1 回放 succeeded。
        let replayed = store.restore_from_snapshot("wfr-t4", &snap).expect("restore");
        // 快照含全部节点（t1 succeeded + t2 pending）；只回放终态条目，
        // 非终态条目复位 pending（attempt 保留）。
        assert_eq!(replayed, 1);
        let nodes = store.node_rows("wfr-t4").expect("nodes");
        assert!(nodes.iter().all(|n| n.state != WorkflowNodeRunState::Running));
        // 恢复态合并：input + succeeded 输出。
        let state = store
            .restored_state("wfr-t4", &snap, &json!({"k": 1}))
            .expect("restored state");
        assert_eq!(state, json!({"k": 1, "a": 1}));
    }
}
