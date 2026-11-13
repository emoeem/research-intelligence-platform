//! 执行引擎与 run 循环（P8-2/P8-3）：非智能节点与智能节点执行器的派发、
//! fail-fast run 循环、节点完成即 checkpoint、节点超时显式终态、断点恢复
//! 与失败重试、workflow 级协作暂停/恢复与人工介入。
//!
//! 口径（ROADMAP §9.3 P8-2「四类节点各一个最小 workflow 定义跑通并落
//! WorkflowRun 节点状态」+ P8-3「Agent 节点 + 长任务执行（暂停/恢复/失败
//! 重试/版本管理）」+ 计划 §14:830「运行、暂停、恢复、失败重试」）：
//! - run 状态机六态词表与跃迁表 = domain::WorkflowRunState（P8-1 冻结），
//!   failed 为终态不迁出，失败重试以新 run 承载（[`WorkflowExecutor::retry`]，
//!   先例 agent_engine RunControl::retry）；
//! - 节点五态词表 = domain::WorkflowNodeRunState；attempt 在节点进入
//!   running 时 +1（0 = 未开始）；
//! - LangGraph/Temporal 思路：每个节点到达终态即写 workflow_checkpoint
//!   快照（只追加），resume 读 MAX(seq) 快照恢复，已完成节点不重跑；
//! - fail-fast：任一节点 failed 即 run failed（后续 pending 节点留在
//!   pending，不标 skipped；skipped 专属条件分支未选中语义）；
//! - 节点超时：执行线程看门狗（recv_timeout），超时节点显式 failed
//!   （error code=node_timeout），残留执行线程的终态写入被乐观守卫
//!   丢弃（不会与 retry/resume 的行竞态双写）；
//! - 协作暂停（P8-3，D-TC-04 语义）：pause 只写 018 workflow_run_control
//!   请求位并经 agent-engine RunControl 向运行中 agent_run 传播（014 控
//!   制行）；run 循环波间消费，「等完成再 checkpoint」定稿：当前节点
//!   完成并落 checkpoint 后下一波转 paused，运行中节点不抢占；agent 节点
//!   例外路径：agent 循环步间消费暂停请求后返回 Paused，节点回 pending
//!   （attempt 保留）随 run 转 paused；
//! - 人工介入（P8-3）：human 节点执行即写 awaiting 记录（018 控制行
//!   approval_node_id + approval_request_json，approval_json 置空），run
//!   转 paused 停在 awaiting_approval；resume 注入批准参数（并入 run
//!   state + approval_json 留痕）后节点重派发即成功续跑；
//! - 取消（P8-3 记状态定稿）：cancel 直接跃迁 cancelled 之外，向运行中
//!   实体传播留痕（tool_run.cancel_requested_at / 014 cancel 请求位），
//!   运行中节点不抢占（协作语义与 ToolRun D-TC-04 同源）。
//!
//! 数据流（状态合并）：run state 初始 = run 输入（JSON object）；节点
//! 输出必须为 JSON object 或 null，object 浅合并进 state（后到覆盖）；
//! Condition 输出 `{"condition_result": bool}`，Tool/Knowledge/Parallel
//! 输出为各自载荷 object，Agent 输出为终态报告投影 object，Human 输出为
//! `{<key>: 批准参数}`。字符串叶值以 `$/` 开头按 RFC 6901 JSON Pointer
//! 指向 state 解析（[`resolve_refs`]），解析失败 fail-closed。

pub mod node_agent;
pub mod node_condition;
pub mod node_human;
pub mod node_knowledge;
pub mod node_parallel;
pub mod node_tool;

use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use crate::control::WorkflowControl;
use crate::error::WorkflowError;
use crate::state::{
    merge_output, type_word, NodeEntityColumn, NodeRow, RunEntityColumn, RunRow, RunStore,
};
use crate::store::WorkflowStore;
use context_broker::Broker;
use domain::{
    WorkflowCompat, WorkflowDefinition, WorkflowEdge, WorkflowEdgeKind, WorkflowNodeKind,
    WorkflowNodeRunState, WorkflowRunState,
};
use serde_json::{json, Map, Value};
use tool_engine::Engine;

/// 节点默认超时（600s，与 tool-engine budget 显式帽上限同值）。
pub const DEFAULT_NODE_TIMEOUT_MS: u64 = 600_000;

/// 状态引用前缀（`$/` + RFC 6901 JSON Pointer；字符串叶值整体此前缀时
/// 按 run state 解析，exec::resolve_refs 与各节点执行器同口径）。
pub const STATE_REF_PREFIX: &str = "$/";

/// 节点执行失败载荷（error_json 形状；code 词形 machine 可 grep）。
#[derive(Debug, Clone, PartialEq)]
pub struct NodeFailure {
    /// 失败码（tool_submit_rejected / tool_failed / tool_timed_out /
    /// broker_unavailable / condition_invalid_expression /
    /// state_ref_unresolvable / node_timeout / parallel_child_failed /
    /// output_not_object / params_invalid / node_driver_unavailable /
    /// tool_drive_failed / agent_failed）。
    pub code: String,
    /// 失败消息。
    pub message: String,
}

impl NodeFailure {
    /// 构造。
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    /// JSON 载荷。
    pub fn to_json(&self, run_id: &str, node_id: &str) -> Value {
        json!({
            "run_id": run_id,
            "node_id": node_id,
            "code": self.code,
            "message": self.message,
        })
    }
}

/// Tool 节点运行面端口（把一个 QUEUED 的 tool_run 推到终态；运行面注入点，
/// agent-engine tool_link RunDriver 同款分工：引擎受理/状态机/信封全走
/// tool-engine 既有公开接口，本 crate 不绕过不直调宿主 Shell）。
pub trait ToolRunDriver: Send + Sync {
    /// 驱动到终态。
    fn drive(
        &self,
        engine: &Engine,
        run_id: &str,
    ) -> Result<tool_engine::DriveOutcome, tool_engine::EngineError>;
}

/// 工作流执行器（run 循环 + 节点派发 + 控制面；线程安全，Parallel 子分支
/// 并发执行复用同一实例）。
pub struct WorkflowExecutor {
    pool: database::Pool,
    runs: RunStore,
    definitions: WorkflowStore,
    control: WorkflowControl,
    engine: Engine,
    tool_driver: Option<Arc<dyn ToolRunDriver>>,
    agent_driver: Option<Arc<dyn node_agent::AgentNodeDriver>>,
    broker: Option<Arc<Broker>>,
    node_timeout: Duration,
}

/// 一次 run 的终态结果。
#[derive(Debug, Clone, PartialEq)]
pub struct RunOutcome {
    /// run id。
    pub run_id: String,
    /// 终态（或 cancel 后读回的现态）。
    pub state: WorkflowRunState,
    /// 全节点状态行（按 node_id 升序）。
    pub nodes: Vec<NodeRow>,
}

impl WorkflowExecutor {
    /// 构造（调用方先行 database::migrate 保证 016/017/018 表存在）。
    pub fn new(pool: database::Pool) -> Self {
        let engine = Engine::open(pool.clone(), tool_registry::Registry::open(pool.clone()));
        Self {
            runs: RunStore::new(pool.clone()),
            definitions: WorkflowStore::new(pool.clone()),
            control: WorkflowControl::new(pool.clone()),
            engine,
            pool,
            tool_driver: None,
            agent_driver: None,
            broker: None,
            node_timeout: Duration::from_millis(DEFAULT_NODE_TIMEOUT_MS),
        }
    }

    /// 注入 Tool 节点运行面（缺省未注入时 Tool 节点执行即失败
    /// code=node_driver_unavailable；CLI 装 python_worker 真实派发，测试装
    /// Mock 注入，TOOL-CONTRACT §10.1 注入口径）。
    pub fn with_tool_driver(mut self, driver: Arc<dyn ToolRunDriver>) -> Self {
        self.tool_driver = Some(driver);
        self
    }

    /// 注入 Agent 节点运行面（P8-3；缺省未注入时 Agent 节点执行即失败
    /// code=node_driver_unavailable；CLI 装 agent-engine AgentLoop 真实
    /// 闭环，测试装 Mock 注入）。
    pub fn with_agent_driver(mut self, driver: Arc<dyn node_agent::AgentNodeDriver>) -> Self {
        self.agent_driver = Some(driver);
        self
    }

    /// 注入 Knowledge 节点检索面（缺省未注入时 Knowledge 节点执行即失败
    /// code=broker_unavailable；CLI 按 rp-agent 同款装配全源 Broker）。
    pub fn with_broker(mut self, broker: Arc<Broker>) -> Self {
        self.broker = Some(broker);
        self
    }

    /// 覆盖节点超时（默认 [`DEFAULT_NODE_TIMEOUT_MS`]）。
    pub fn with_node_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.node_timeout = Duration::from_millis(timeout_ms.max(1));
        self
    }

    /// 受理并启动一次 run（queued → running → 终态；fail-fast）。
    ///
    /// 受理门（fail-closed，见 [`accept_definition`]）：compat 必须
    /// graph-v1；input 必须 JSON object 且符合 input_schema（若声明）；
    /// branch 边形状与节点目标词形按 P8-2/P8-3 冻结口径校验。
    pub fn start(
        &self,
        workflow_id: &str,
        version: Option<u32>,
        input: &Value,
    ) -> Result<RunOutcome, WorkflowError> {
        self.start_with_id(workflow_id, version, input, None)
    }

    /// 受理并启动一次 run，钉定 run id（P8-3：API 写面经 rp-workflow
    /// --run-id 异步承载时由调用方预生成，受理面校验形状与唯一性）。
    pub fn start_with_id(
        &self,
        workflow_id: &str,
        version: Option<u32>,
        input: &Value,
        run_id: Option<&str>,
    ) -> Result<RunOutcome, WorkflowError> {
        let stored = self.definitions.get(workflow_id, version)?;
        let def = stored.definition;
        accept_definition(&def, input)?;
        let run_id = match run_id {
            Some(pinned) => {
                let validated = domain::RunId::try_new(pinned.to_string())?;
                validated.as_str().to_string()
            }
            None => new_run_id(),
        };
        let node_kinds = def
            .nodes()
            .iter()
            .map(|n| (n.id().to_string(), n.kind().as_str().to_string()))
            .collect::<Vec<_>>();
        self.runs
            .create_run(&run_id, workflow_id, def.version(), input, &node_kinds)?;
        self.control.ensure_control_row(&run_id)?;
        self.runs.transition_run(&run_id, WorkflowRunState::Queued, WorkflowRunState::Running, None, None)?;
        self.drive_run(&run_id, &def, input.clone())
    }

    /// 协作式暂停（P8-3，D-TC-04 请求位语义）：写 018 workflow_run_control
    /// pause_requested_at 并经 agent-engine RunControl 向运行中 agent_run
    /// 传播（014 控制行）；run 循环波间消费——「等完成再 checkpoint」定稿：
    /// 当前节点完成并落 checkpoint 后下一波转 paused，运行中节点不抢占。
    /// 返回受理时刻的 run 现态（终态拒绝；已 paused 幂等返回）。
    pub fn pause(&self, run_id: &str) -> Result<WorkflowRunState, WorkflowError> {
        let row = self.runs.run_row(run_id)?;
        if row.state.is_terminal() {
            return Err(WorkflowError::IllegalTransition {
                run_id: run_id.to_string(),
                from: row.state.as_str().to_string(),
                to: WorkflowRunState::Paused.as_str().to_string(),
            });
        }
        if row.state == WorkflowRunState::Paused {
            return Ok(row.state);
        }
        self.control.request_pause(run_id, row.state)?;
        self.control.propagate_pause_to_agents(run_id)?;
        Ok(row.state)
    }

    /// 断点恢复（同 run_id）：从最近 checkpoint 续跑，已完成节点保持
    /// succeeded 且 attempt 不变（不重跑），残留 running 行复位 pending
    /// 续跑至终态。接受 running（kill 后）/ paused（P8-3 协作暂停或人工
    /// 介入停等）/ queued（未启动）三种非终态。`inject` 为批准/注入参数
    /// （P8-3 人工介入恢复面）：并入恢复态（浅合并，后到覆盖）并落 018
    /// 控制行 approval_json 留痕；None = 不注入（人工介入停等场景下节点
    /// 重写介入请求再次停等）。
    pub fn resume(&self, run_id: &str) -> Result<RunOutcome, WorkflowError> {
        self.resume_with_injection(run_id, None)
    }

    /// 带批准/注入参数的恢复（P8-3 人工介入节点的批准通道）。
    pub fn resume_with_injection(
        &self,
        run_id: &str,
        inject: Option<&Value>,
    ) -> Result<RunOutcome, WorkflowError> {
        let row = self.runs.run_row(run_id)?;
        match row.state {
            WorkflowRunState::Running | WorkflowRunState::Paused | WorkflowRunState::Queued => {}
            other => {
                return Err(WorkflowError::InvalidInput(format!(
                    "resume requires a non-terminal run, run {run_id} is {}",
                    other.as_str()
                )))
            }
        }
        if let Some(approval) = inject {
            if !approval.is_object() {
                return Err(WorkflowError::InvalidInput(format!(
                    "resume injection must be a JSON object, got {}",
                    type_word(approval)
                )));
            }
            self.control.resolve_approval(run_id, approval)?;
        }
        let stored = self
            .definitions
            .get(&row.workflow_id, Some(row.workflow_version))?;
        let def = stored.definition;
        accept_definition(&def, &row.input)?;
        let checkpoint = self.runs.latest_checkpoint(run_id)?;
        let mut state = match &checkpoint {
            Some(cp) => {
                self.runs.restore_from_snapshot(run_id, &cp.snapshot)?;
                self.runs
                    .restored_state(run_id, &cp.snapshot, &row.input)?
            }
            None => row.input.clone(),
        };
        if let Some(approval) = inject {
            if let Some(map) = state.as_object_mut() {
                if let Some(approved) = approval.as_object() {
                    for (k, v) in approved {
                        map.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        if row.state == WorkflowRunState::Queued {
            self.runs.transition_run(run_id, WorkflowRunState::Queued, WorkflowRunState::Running, None, None)?;
        } else if row.state == WorkflowRunState::Paused {
            self.runs.transition_run(run_id, WorkflowRunState::Paused, WorkflowRunState::Running, None, None)?;
        }
        self.drive_run(run_id, &def, state)
    }

    /// 失败重试（P8-2 口径：failed 为终态不迁出，以新 run 承载）：新 run
    /// 钉同一 (workflow_id, workflow_version) 与 input，节点状态整体继承
    /// 失败 run 最新 checkpoint（已完成节点 attempt 不变不重跑），仅目标
    /// 失败节点回 pending；重跑时该节点 attempt 在原值上 +1（递增）。
    pub fn retry(&self, run_id: &str, node_id: &str) -> Result<RunOutcome, WorkflowError> {
        self.retry_with_id(run_id, node_id, None)
    }

    /// 失败重试并钉定新 run id（P8-3：API 写面异步承载时由调用方预生成，
    /// 受理面校验形状；None = 引擎生成）。
    pub fn retry_with_id(
        &self,
        run_id: &str,
        node_id: &str,
        pinned_new_run_id: Option<&str>,
    ) -> Result<RunOutcome, WorkflowError> {
        let row = self.runs.run_row(run_id)?;
        if row.state != WorkflowRunState::Failed {
            return Err(WorkflowError::InvalidInput(format!(
                "retry requires a failed run, run {run_id} is {}",
                row.state.as_str()
            )));
        }
        let checkpoint = self
            .runs
            .latest_checkpoint(run_id)?
            .ok_or_else(|| WorkflowError::InvalidInput(format!("run {run_id} has no checkpoint to inherit")))?;
        let entry = checkpoint
            .snapshot
            .get("nodes")
            .and_then(Value::as_object)
            .and_then(|nodes| nodes.get(node_id))
            .ok_or_else(|| {
                WorkflowError::InvalidInput(format!("node {node_id:?} not in run {run_id} snapshot"))
            })?;
        if entry.get("state").and_then(Value::as_str) != Some("failed") {
            return Err(WorkflowError::InvalidInput(format!(
                "retry target node {node_id:?} is not failed in run {run_id} snapshot"
            )));
        }
        let stored = self
            .definitions
            .get(&row.workflow_id, Some(row.workflow_version))?;
        let def = stored.definition;
        accept_definition(&def, &row.input)?;
        let new_run_id = match pinned_new_run_id {
            Some(pinned) => domain::RunId::try_new(pinned.to_string())?.into_inner(),
            None => new_run_id(),
        };
        let node_kinds = def
            .nodes()
            .iter()
            .map(|n| (n.id().to_string(), n.kind().as_str().to_string()))
            .collect::<Vec<_>>();
        self.runs
            .create_run(&new_run_id, &row.workflow_id, row.workflow_version, &row.input, &node_kinds)?;
        self.runs.restore_from_snapshot(&new_run_id, &checkpoint.snapshot)?;
        self.runs.node_reset_pending(&new_run_id, node_id)?;
        self.runs
            .transition_run(&new_run_id, WorkflowRunState::Queued, WorkflowRunState::Running, None, None)?;
        let state = self
            .runs
            .restored_state(&new_run_id, &checkpoint.snapshot, &row.input)?;
        self.drive_run(&new_run_id, &def, state)
    }

    /// 协作式取消（queued/running/paused → cancelled，与冻结跃迁表一致；
    /// 终态 run 拒绝）。P8-3 记状态定稿：跃迁落地后写 018 控制行
    /// cancel_requested_at 留痕，并向运行中实体传播取消请求（agent_run
    /// 经 agent-engine RunControl、tool_run 经 tool-engine request_cancel
    /// 写各自请求位/请求列，运行中节点不抢占，协作语义与 ToolRun D-TC-04
    /// 同源）。对正在执行的同 run 进程：下一波 run 跃迁乐观守卫失败，读回
    /// cancelled 后如实返回。
    pub fn cancel(&self, run_id: &str) -> Result<WorkflowRunState, WorkflowError> {
        let row = self.runs.run_row(run_id)?;
        let changed = self
            .pool
            .with_conn(|c| {
                c.execute(
                    "UPDATE workflow_run SET state = 'cancelled',
                     finished_at = COALESCE(finished_at, ?2)
                     WHERE run_id = ?1 AND state IN ('queued','running','paused')",
                    database::rusqlite::params![run_id, domain::time::now_iso()],
                )
                .map_err(database::DbError::Sqlite)
            })
            .map_err(WorkflowError::from)?;
        if changed == 1 {
            self.control.record_cancel(run_id)?;
            self.control.propagate_cancel_to_entities(run_id)?;
            return Ok(WorkflowRunState::Cancelled);
        }
        Err(WorkflowError::IllegalTransition {
            run_id: run_id.to_string(),
            from: row.state.as_str().to_string(),
            to: WorkflowRunState::Cancelled.as_str().to_string(),
        })
    }

    /// 人工介入停等读面（P8-3）：awaiting 控制行视图（None = 不在等待）。
    pub fn awaiting_approval(
        &self,
        run_id: &str,
    ) -> Result<Option<crate::control::ControlRow>, WorkflowError> {
        self.control.awaiting_approval(run_id)
    }

    /// run 头行 + 全节点状态行（status/nodes 读取面）。
    pub fn status(&self, run_id: &str) -> Result<(RunRow, Vec<NodeRow>), WorkflowError> {
        let row = self.runs.run_row(run_id)?;
        let nodes = self.runs.node_rows(run_id)?;
        Ok((row, nodes))
    }

    /// run 循环：波次推进（ready 集合确定性按 node_id 升序），每节点终态
    /// 落 checkpoint；fail-fast；无进展 fail-closed；波间消费协作暂停
    /// （等完成再 checkpoint：消费点在上一节点终态落 checkpoint 之后、
    /// 下一节点派发之前）。
    fn drive_run(
        &self,
        run_id: &str,
        def: &WorkflowDefinition,
        mut state: Value,
    ) -> Result<RunOutcome, WorkflowError> {
        let edges = def.edges().to_vec();
        // 迭代守卫：无环图每轮至少一个节点状态变化（执行或跳过），
        // 上限 4n + 16 轮（ paranoia，理论 n 轮内终局）。
        let max_rounds = def.nodes().len() * 4 + 16;
        for _round in 0..max_rounds {
            // 协作取消：每波读回 run 现态，cancelled 即止。
            let current = self.runs.run_row(run_id)?;
            if current.state == WorkflowRunState::Cancelled {
                return self.outcome(run_id, current.state);
            }
            let rows = self.runs.node_rows(run_id)?;
            let by_id: std::collections::BTreeMap<&str, &NodeRow> =
                rows.iter().map(|r| (r.node_id.as_str(), r)).collect();
            // 终局判定：全节点终态（节点五态：succeeded/failed/skipped）。
            if rows.iter().all(|r| {
                matches!(
                    r.state,
                    WorkflowNodeRunState::Succeeded | WorkflowNodeRunState::Failed | WorkflowNodeRunState::Skipped
                )
            }) {
                let failed = rows.iter().find(|r| r.state == WorkflowNodeRunState::Failed);
                return match failed {
                    Some(row) => {
                        let error = row.error.clone().unwrap_or_else(|| {
                            json!({"node_id": row.node_id, "code": "node_failed"})
                        });
                        self.finish_run(run_id, WorkflowRunState::Failed, None, Some(&error))?;
                        self.outcome(run_id, WorkflowRunState::Failed)
                    }
                    None => {
                        self.finish_run(run_id, WorkflowRunState::Succeeded, Some(&state), None)?;
                        self.outcome(run_id, WorkflowRunState::Succeeded)
                    }
                };
            }
            // 协作暂停消费（P8-3，D-TC-04 consume-on-observe）：消费点在
            // 终局判定之后（run 实际已完成时不吃暂停），当前无运行中节点
            // （上一波节点已终态 + checkpoint），转 paused 即止。
            if self.control.consume_pause_request(run_id)? {
                if let Err(WorkflowError::IllegalTransition { .. }) = self.runs.transition_run(
                    run_id,
                    WorkflowRunState::Running,
                    WorkflowRunState::Paused,
                    None,
                    None,
                ) {
                    let now_state = self.runs.run_row(run_id)?.state;
                    // 竞态收口：cancel/终态优先，paused 请求随终态作废。
                    if now_state.is_terminal() {
                        return self.outcome(run_id, now_state);
                    }
                    return Err(WorkflowError::IllegalTransition {
                        run_id: run_id.to_string(),
                        from: now_state.as_str().to_string(),
                        to: WorkflowRunState::Paused.as_str().to_string(),
                    });
                }
                return self.outcome(run_id, WorkflowRunState::Paused);
            }
            // 波次推进：先跳过（分支未选中级联），再执行 ready。
            let mut skipped_any = false;
            for node in def.nodes() {
                let Some(row) = by_id.get(node.id()) else {
                    return Err(WorkflowError::Storage(format!(
                        "node row missing for {:?} in run {run_id}",
                        node.id()
                    )));
                };
                if row.state != WorkflowNodeRunState::Pending {
                    continue;
                }
                let in_edges: Vec<&WorkflowEdge> = edges
                    .iter()
                    .filter(|e| e.to() == node.id())
                    .collect();
                let views: Vec<EdgeView> = in_edges
                    .iter()
                    .map(|e| edge_view(e, by_id.get(e.from()).copied()))
                    .collect();
                if views.contains(&EdgeView::Selected) {
                    continue; // ready，本波稍后执行
                }
                if views.contains(&EdgeView::Waiting) {
                    continue; // 上游未终态
                }
                // 全部入边 Dead（分支未选中 / 源被跳过）：skip 终态。
                if views.is_empty() {
                    continue; // 无入边且非 start 由静态校验保证不出现；保守留给 ready 面
                }
                self.runs.node_skip(run_id, node.id())?;
                self.runs.record_checkpoint(run_id, node.id())?;
                skipped_any = true;
            }
            if skipped_any {
                continue;
            }
            // ready 节点（node_id 升序取首个；每轮执行一个，状态机确定）。
            let mut ready: Option<&domain::WorkflowNode> = None;
            for node in def.nodes() {
                let row = by_id.get(node.id()).copied();
                let Some(row) = row else {
                    continue;
                };
                if row.state != WorkflowNodeRunState::Pending {
                    continue;
                }
                let in_edges: Vec<&WorkflowEdge> = edges
                    .iter()
                    .filter(|e| e.to() == node.id())
                    .collect();
                if in_edges.is_empty()
                    || in_edges.iter().any(|e| {
                        edge_view(e, by_id.get(e.from()).copied()) == EdgeView::Selected
                    })
                {
                    ready = Some(node);
                    break;
                }
            }
            let Some(node) = ready else {
                // 无 ready 无 skip：无进展 fail-closed（理论不可达，防御面）。
                let error = json!({
                    "run_id": run_id,
                    "code": "no_progress",
                    "message": "no ready node and no skippable node with pending nodes left",
                });
                self.finish_run(run_id, WorkflowRunState::Failed, None, Some(&error))?;
                return self.outcome(run_id, WorkflowRunState::Failed);
            };
            // 节点失败（含 parallel join 失败）= run 终态 failed，读回收敛为
            // outcome；仅真正的内部错误（run 未到终态）向上传播；暂停与
            // 人工介入停等（Paused 载荷）= run paused 即止。
            match self.execute_node(run_id, node, state) {
                Ok(StepFlow::Continue(new_state)) => state = new_state,
                Ok(StepFlow::Pause) => {
                    let current = self.runs.run_row(run_id)?;
                    if current.state == WorkflowRunState::Paused {
                        return self.outcome(run_id, WorkflowRunState::Paused);
                    }
                    if current.state.is_terminal() {
                        return self.outcome(run_id, current.state);
                    }
                    // 消费用户暂停请求（若 pause 与节点级暂停同时在场：本路径
                    // 即消费点，防 stale 请求位让 resume 一波即回 paused）。
                    // 人工介入停等无用户请求时消费返回 false，无副作用。
                    let _ = self.control.consume_pause_request(run_id)?;
                    if let Err(WorkflowError::IllegalTransition { .. }) = self.runs.transition_run(
                        run_id,
                        WorkflowRunState::Running,
                        WorkflowRunState::Paused,
                        None,
                        None,
                    ) {
                        let now_state = self.runs.run_row(run_id)?.state;
                        if now_state.is_terminal() {
                            return self.outcome(run_id, now_state);
                        }
                        return Err(WorkflowError::IllegalTransition {
                            run_id: run_id.to_string(),
                            from: now_state.as_str().to_string(),
                            to: WorkflowRunState::Paused.as_str().to_string(),
                        });
                    }
                    return self.outcome(run_id, WorkflowRunState::Paused);
                }
                Err(err) => {
                    let current = self.runs.run_row(run_id)?;
                    if current.state.is_terminal() {
                        return self.outcome(run_id, current.state);
                    }
                    return Err(err);
                }
            }
        }
        let error = json!({
            "run_id": run_id,
            "code": "no_progress",
            "message": format!("run {run_id} exceeded round guard {max_rounds}"),
        });
        self.finish_run(run_id, WorkflowRunState::Failed, None, Some(&error))?;
        self.outcome(run_id, WorkflowRunState::Failed)
    }

    /// 执行一个 ready 节点（parallel 为 fork-join 协调器；其余为看门狗
    /// 单节点执行），返回合并子输出后的新 state 或暂停信号。
    fn execute_node(
        &self,
        run_id: &str,
        node: &domain::WorkflowNode,
        state: Value,
    ) -> Result<StepFlow, WorkflowError> {
        match node.kind() {
            WorkflowNodeKind::Parallel => node_parallel::execute_fan_out(
                self, run_id, node, state,
            )
            .map(StepFlow::Continue),
            WorkflowNodeKind::Start | WorkflowNodeKind::End => {
                self.runs.node_start(run_id, node.id())?;
                self.runs
                    .node_finish(run_id, node.id(), WorkflowNodeRunState::Succeeded, None, None)?;
                self.runs.record_checkpoint(run_id, node.id())?;
                Ok(StepFlow::Continue(state))
            }
            WorkflowNodeKind::Tool
            | WorkflowNodeKind::Knowledge
            | WorkflowNodeKind::Condition
            | WorkflowNodeKind::Agent
            | WorkflowNodeKind::Human => {
                let (rx, attempt) = self.dispatch_node(run_id, node, &state)?;
                match self.await_dispatch(node.id(), rx)? {
                    AwaitedOutcome::Succeeded(output) => {
                        let output = output.unwrap_or(Value::Null);
                        self.runs.node_finish(
                            run_id,
                            node.id(),
                            WorkflowNodeRunState::Succeeded,
                            Some(&output),
                            None,
                        )?;
                        self.runs.record_checkpoint(run_id, node.id())?;
                        // 实体关联收口（018 六 ID 列）：tool 节点经 submit_key
                        // 精确回查 tool_run；agent 节点链接列在派发前已落；
                        // artifact/evidence 经 015 trace 面刷新最近值。
                        if node.kind() == WorkflowNodeKind::Tool {
                            if let Some(tool_run_id) =
                                self.runs.tool_run_id_of_node(run_id, node.id(), attempt)?
                            {
                                self.runs.set_node_entity_link(
                                    run_id,
                                    node.id(),
                                    NodeEntityColumn::ToolRun,
                                    &tool_run_id,
                                )?;
                                self.runs.set_run_entity_link(
                                    run_id,
                                    RunEntityColumn::ToolRun,
                                    &tool_run_id,
                                )?;
                            }
                        }
                        if matches!(
                            node.kind(),
                            WorkflowNodeKind::Tool | WorkflowNodeKind::Agent
                        ) {
                            self.runs.refresh_trace_correlations(run_id)?;
                        }
                        let mut merged = match state {
                            Value::Object(map) => map,
                            other => {
                                return Err(WorkflowError::InvalidInput(format!(
                                    "run state must be an object, got {}",
                                    type_word(&other)
                                )))
                            }
                        };
                        merge_output(&mut merged, &output, node.id())?;
                        Ok(StepFlow::Continue(Value::Object(merged)))
                    }
                    AwaitedOutcome::Paused => {
                        // agent 协作暂停：节点回 pending（attempt 保留），
                        // run 转 paused 由调用方收口（StepFlow::Pause）。
                        self.runs.node_pause_reset(run_id, node.id())?;
                        Ok(StepFlow::Pause)
                    }
                    AwaitedOutcome::AwaitingApproval(request) => {
                        // human 人工介入：节点保持 running（人的步进未完），
                        // 018 控制行落 awaiting 记录，run 转 paused 由调用方
                        // 收口；resume 注入批准参数后节点重派发（attempt+1）。
                        self.control.put_approval_request(run_id, node.id(), &request)?;
                        Ok(StepFlow::Pause)
                    }
                    AwaitedOutcome::Failed(failure) => {
                        let error = failure.to_json(run_id, node.id());
                        self.runs.node_finish(
                            run_id,
                            node.id(),
                            WorkflowNodeRunState::Failed,
                            None,
                            Some(&error),
                        )?;
                        self.runs.record_checkpoint(run_id, node.id())?;
                        self.finish_run(run_id, WorkflowRunState::Failed, None, Some(&error))?;
                        Err(WorkflowError::Exec(format!(
                            "node {:?} failed: {} ({})",
                            node.id(),
                            failure.code,
                            failure.message
                        )))
                    }
                }
            }
        }
    }

    /// 派发单节点计算到独立线程（看门狗 recv_timeout 超时后线程残留，
    /// 其终态写入被乐观守卫丢弃）；attempt 已在本调用内置 running。返回
    /// （信道, attempt）。agent 节点派发前预生成 agent_run_id 并落节点/run
    /// 链接列（018 实体链接列，跨进程 pause/cancel 传播据此发现运行中实体）。
    fn dispatch_node(
        &self,
        run_id: &str,
        node: &domain::WorkflowNode,
        state: &Value,
    ) -> Result<(mpsc::Receiver<DispatchOutcome>, u32), WorkflowError> {
        let attempt = self.runs.node_start(run_id, node.id())?;
        let kind = node.kind();
        // agent 节点：预生成 agent_run_id（AgentStore::new_run_id 同源）并
        // 先落链接列，端口实现侧必须以此 id 落 agent_run 行。
        let agent_run_id = if kind == WorkflowNodeKind::Agent {
            let pinned = agent_engine::store::AgentStore::new_run_id().into_inner();
            self.runs.set_node_entity_link(
                run_id,
                node.id(),
                NodeEntityColumn::AgentRun,
                &pinned,
            )?;
            self.runs
                .set_run_entity_link(run_id, RunEntityColumn::AgentRun, &pinned)?;
            pinned
        } else {
            String::new()
        };
        let (tx, rx) = mpsc::channel();
        let engine = self.engine.clone();
        let driver = self.tool_driver.clone();
        let agent_driver = self.agent_driver.clone();
        let broker = self.broker.clone();
        let params_json = node.params_json().map(str::to_string);
        let node_id = node.id().to_string();
        let state = state.clone();
        let run_id_owned = run_id.to_string();
        std::thread::spawn(move || {
            let outcome = match kind {
                WorkflowNodeKind::Tool => {
                    match node_tool::execute(
                        &engine, driver.as_deref(), &run_id_owned, &node_id, attempt,
                        params_json.as_deref(), &state,
                    ) {
                        Ok(output) => DispatchOutcome::Succeeded(Some(output)),
                        Err(failure) => DispatchOutcome::Failed(failure),
                    }
                }
                WorkflowNodeKind::Knowledge => {
                    match node_knowledge::execute(
                        broker.as_deref(), &run_id_owned, &node_id,
                        params_json.as_deref(), &state,
                    ) {
                        Ok(output) => DispatchOutcome::Succeeded(Some(output)),
                        Err(failure) => DispatchOutcome::Failed(failure),
                    }
                }
                WorkflowNodeKind::Condition => {
                    match node_condition::execute(params_json.as_deref(), &state) {
                        Ok(output) => DispatchOutcome::Succeeded(Some(output)),
                        Err(failure) => DispatchOutcome::Failed(failure),
                    }
                }
                WorkflowNodeKind::Agent => {
                    let request = node_agent::AgentNodeRequest {
                        run_id: run_id_owned.clone(),
                        node_id: node_id.clone(),
                        attempt,
                        agent_run_id: agent_run_id.clone(),
                        agent_id: String::new(),
                        agent_version: None,
                        goal: String::new(),
                        task_id: None,
                        workspace_id: None,
                    };
                    match node_agent::execute(
                        agent_driver.as_deref(),
                        &request,
                        params_json.as_deref(),
                        &state,
                    ) {
                        node_agent::AgentNodeOutcome::Succeeded { output } => {
                            DispatchOutcome::Succeeded(Some(output))
                        }
                        node_agent::AgentNodeOutcome::Paused { .. } => DispatchOutcome::Paused,
                        node_agent::AgentNodeOutcome::Failed(failure) => {
                            DispatchOutcome::Failed(failure)
                        }
                    }
                }
                WorkflowNodeKind::Human => {
                    match node_human::execute(params_json.as_deref(), &state) {
                        Ok((output, false)) => DispatchOutcome::Succeeded(Some(output)),
                        Ok((request, true)) => DispatchOutcome::AwaitingApproval(request),
                        Err(failure) => DispatchOutcome::Failed(failure),
                    }
                }
                _ => DispatchOutcome::Failed(NodeFailure::new(
                    "node_kind_unsupported",
                    format!("node {node_id:?} kind {} is not executable here", kind.as_str()),
                )),
            };
            let _ = tx.send(outcome);
        });
        Ok((rx, attempt))
    }

    /// 看门狗等待派发结果：超时显式 failed（code=node_timeout）终态。
    fn await_dispatch(
        &self,
        node_id: &str,
        rx: mpsc::Receiver<DispatchOutcome>,
    ) -> Result<AwaitedOutcome, WorkflowError> {
        match rx.recv_timeout(self.node_timeout) {
            Ok(DispatchOutcome::Succeeded(output)) => Ok(AwaitedOutcome::Succeeded(output)),
            Ok(DispatchOutcome::Failed(failure)) => Ok(AwaitedOutcome::Failed(failure)),
            Ok(DispatchOutcome::Paused) => Ok(AwaitedOutcome::Paused),
            Ok(DispatchOutcome::AwaitingApproval(request)) => {
                Ok(AwaitedOutcome::AwaitingApproval(request))
            }
            Err(_) => Ok(AwaitedOutcome::Failed(NodeFailure::new(
                "node_timeout",
                format!(
                    "node {node_id:?} exceeded node timeout {}ms; \
                     residual worker thread output is discarded by the optimistic guard",
                    self.node_timeout.as_millis()
                ),
            ))),
        }
    }

    /// run 行钉定定义回放（resume/retry/parallel fan 拓扑读取共用；
    /// run 钉死 (workflow_id, workflow_version)，不漂移到最新版）。
    fn pinned_definition(&self, run_id: &str) -> Result<WorkflowDefinition, WorkflowError> {
        let row = self.runs.run_row(run_id)?;
        let stored = self
            .definitions
            .get(&row.workflow_id, Some(row.workflow_version))?;
        Ok(stored.definition)
    }

    /// run 级终态跃迁（cancel 竞态：读回 cancelled 如实收尾）。
    fn finish_run(
        &self,
        run_id: &str,
        to: WorkflowRunState,
        output: Option<&Value>,
        error: Option<&Value>,
    ) -> Result<(), WorkflowError> {
        if let Err(WorkflowError::IllegalTransition { .. }) =
            self.runs
                .transition_run(run_id, WorkflowRunState::Running, to, output, error)
        {
            let current = self.runs.run_row(run_id)?;
            if current.state != WorkflowRunState::Cancelled {
                return Err(WorkflowError::IllegalTransition {
                    run_id: run_id.to_string(),
                    from: current.state.as_str().to_string(),
                    to: to.as_str().to_string(),
                });
            }
        }
        Ok(())
    }

    /// 终态结果投影。
    fn outcome(&self, run_id: &str, state: WorkflowRunState) -> Result<RunOutcome, WorkflowError> {
        let nodes = self.runs.node_rows(run_id)?;
        Ok(RunOutcome {
            run_id: run_id.to_string(),
            state,
            nodes,
        })
    }
}

/// 入边视图（ready/skip 判定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EdgeView {
    /// 选中（源 succeeded 且边放行）。
    Selected,
    /// 死边（分支未选中 / 源被跳过 / 源失败）。
    Dead,
    /// 上游未终态。
    Waiting,
}

/// 单节点执行后的推进信号：Continue(state) = 正常续跑；Pause = run 转
/// paused 即止（agent 协作暂停 / human awaiting_approval）。
enum StepFlow {
    Continue(Value),
    Pause,
}

/// 派发等待结果（看门狗收口形状；Pause/Awaiting 的落库在 execute_node）。
enum AwaitedOutcome {
    /// 成功（output 为节点输出；None = 无输出）。
    Succeeded(Option<Value>),
    /// 失败。
    Failed(NodeFailure),
    /// agent run 协作暂停（节点回 pending，agent_run 链接列已落）。
    Paused,
    /// human 人工介入请求（载荷待落 018 控制行）。
    AwaitingApproval(Value),
}

/// 派发线程信道载荷（executors -> 主循环；tool/knowledge/condition 映射
/// Succeeded/Failed，agent 映射 Paused，human 映射 AwaitingApproval）。
enum DispatchOutcome {
    Succeeded(Option<Value>),
    Failed(NodeFailure),
    Paused,
    AwaitingApproval(Value),
}

/// 入边视图计算：seq 恒放行（源 succeeded）；branch 从 condition 按
/// `condition_result == edge.when`（布尔字面量，受理门已校验形状）判定；
/// branch 从 parallel 恒放行（子分支由 fork-join 直接执行到终态，
/// 此处只做可达性收口）。
fn edge_view(edge: &WorkflowEdge, source: Option<&NodeRow>) -> EdgeView {
    let Some(src) = source else {
        return EdgeView::Dead;
    };
    match src.state {
        WorkflowNodeRunState::Succeeded => match edge.kind() {
            WorkflowEdgeKind::Seq => EdgeView::Selected,
            WorkflowEdgeKind::Branch => {
                if src.node_kind == WorkflowNodeKind::Condition.as_str() {
                    let result = src
                        .output
                        .as_ref()
                        .and_then(|o| o.get("condition_result"))
                        .and_then(Value::as_bool);
                    let when = edge
                        .when_json()
                        .and_then(|w| serde_json::from_str::<Value>(w).ok())
                        .and_then(|v| v.as_bool());
                    match (result, when) {
                        (Some(r), Some(w)) if r == w => EdgeView::Selected,
                        _ => EdgeView::Dead,
                    }
                } else {
                    EdgeView::Selected
                }
            }
        },
        WorkflowNodeRunState::Skipped | WorkflowNodeRunState::Failed => EdgeView::Dead,
        WorkflowNodeRunState::Pending | WorkflowNodeRunState::Running => EdgeView::Waiting,
    }
}

/// 受理门（start/resume/retry 共用，fail-closed）：
/// 1. compat 必须 graph-v1（legacy 参考行永不执行）；
/// 2. input 必须 JSON object；input_schema（若声明）经 jsonschema 校验
///    （P8-1 冻结形状门在运行面的消费）；
/// 3. branch 边形状：branch 只允许从 condition/parallel 出；condition 的
///    branch 边 when 必须布尔字面量；parallel 的 branch 边 when 必须缺省；
///    condition 出边必须全 branch（纯函数谓词只走分支）；start/tool/
///    knowledge/agent/human 出边必须全 seq（human 人工介入单续跑语义，
///    P8-3）；
/// 4. condition 节点必须有出边且 expression 可编译（jsonschema 编译期
///    即拒，防运行期才发现坏谓词）；
/// 5. parallel 子分支目标限定 tool/knowledge/condition，且子目标入度恰为
///    1（该 fan 边；diamond 入口绕过 fork 语义，fail-closed）；agent/human
///    节点禁入 fan（智能/人工执行体不做并发子分支）。
pub fn accept_definition(def: &WorkflowDefinition, input: &Value) -> Result<(), WorkflowError> {
    if def.compat() != WorkflowCompat::GraphV1 {
        return Err(WorkflowError::InvalidInput(format!(
            "workflow {} compat {} is not executable (legacy-method-reference rows never execute)",
            def.workflow_id(),
            def.compat().as_str()
        )));
    }
    if input.as_object().is_none() {
        return Err(WorkflowError::InvalidInput(format!(
            "run input must be a JSON object, got {}",
            type_word(input)
        )));
    }
    if let Some(schema_raw) = def.input_schema_json() {
        let schema: Value = serde_json::from_str(schema_raw)
            .map_err(|e| WorkflowError::InvalidInput(format!("input_schema parse failed: {e}")))?;
        let validator = jsonschema::validator_for(&schema)
            .map_err(|e| WorkflowError::InvalidInput(format!("input_schema compile failed: {e}")))?;
        validator.validate(input).map_err(|e| {
            WorkflowError::InvalidInput(format!("run input violates input_schema: {e}"))
        })?;
    }
    let by_id: std::collections::BTreeMap<&str, &domain::WorkflowNode> = def
        .nodes()
        .iter()
        .map(|n| (n.id(), n))
        .collect();
    for node in def.nodes() {
        if node.kind() == WorkflowNodeKind::Condition {
            // expression 可编译（受理期即拒坏谓词）。
            let expr_raw = node.params_json().and_then(|p| {
                serde_json::from_str::<Map<String, Value>>(p)
                    .ok()
                    .and_then(|m| m.get("expression").cloned())
            });
            let Some(expr) = expr_raw else {
                return Err(WorkflowError::InvalidInput(format!(
                    "condition node {:?} missing expression param",
                    node.id()
                )));
            };
            jsonschema::validator_for(&expr).map_err(|e| {
                WorkflowError::InvalidInput(format!(
                    "condition node {:?} expression is not a valid JSON Schema: {e}",
                    node.id()
                ))
            })?;
        }
    }
    for edge in def.edges() {
        let from_kind = by_id
            .get(edge.from())
            .map(|n| n.kind())
            .ok_or_else(|| WorkflowError::InvalidInput(format!("edge from {:?} not in graph", edge.from())))?;
        match (edge.kind(), from_kind) {
            (WorkflowEdgeKind::Branch, WorkflowNodeKind::Condition) => {
                let when = edge
                    .when_json()
                    .and_then(|w| serde_json::from_str::<Value>(w).ok());
                if !matches!(when, Some(Value::Bool(_))) {
                    return Err(WorkflowError::InvalidInput(format!(
                        "condition branch edge {}->{} requires a boolean when literal",
                        edge.from(),
                        edge.to()
                    )));
                }
            }
            (WorkflowEdgeKind::Branch, WorkflowNodeKind::Parallel) => {
                if edge.when_json().is_some() {
                    return Err(WorkflowError::InvalidInput(format!(
                        "parallel branch edge {}->{} must not carry when (fan-out is unconditional)",
                        edge.from(),
                        edge.to()
                    )));
                }
            }
            (WorkflowEdgeKind::Branch, other) => {
                return Err(WorkflowError::InvalidInput(format!(
                    "branch edge {}->{} illegal: branch edges may only leave condition/parallel nodes, got {}",
                    edge.from(),
                    edge.to(),
                    other.as_str()
                )));
            }
            (WorkflowEdgeKind::Seq, WorkflowNodeKind::Condition) => {
                return Err(WorkflowError::InvalidInput(format!(
                    "seq edge {}->{} illegal: condition nodes must branch (pure predicate)",
                    edge.from(),
                    edge.to()
                )));
            }
            (WorkflowEdgeKind::Seq, _) => {}
        }
    }
    for node in def.nodes() {
        let out: Vec<&WorkflowEdge> = def
            .edges()
            .iter()
            .filter(|e| e.from() == node.id())
            .collect();
        match node.kind() {
            WorkflowNodeKind::Condition => {
                if out.is_empty() || out.iter().all(|e| e.kind() == WorkflowEdgeKind::Seq) {
                    return Err(WorkflowError::InvalidInput(format!(
                        "condition node {:?} requires at least one branch out-edge",
                        node.id()
                    )));
                }
            }
            WorkflowNodeKind::Parallel => {
                let fans: Vec<&WorkflowEdge> = out
                    .iter()
                    .copied()
                    .filter(|e| e.kind() == WorkflowEdgeKind::Branch)
                    .collect();
                if fans.is_empty() {
                    return Err(WorkflowError::InvalidInput(format!(
                        "parallel node {:?} requires at least one branch out-edge (fan-out)",
                        node.id()
                    )));
                }
                for fan in &fans {
                    let Some(target) = by_id.get(fan.to()) else {
                        return Err(WorkflowError::InvalidInput(format!(
                            "parallel fan edge {}->{} targets missing node",
                            fan.from(),
                            fan.to()
                        )));
                    };
                    match target.kind() {
                        WorkflowNodeKind::Tool
                        | WorkflowNodeKind::Knowledge
                        | WorkflowNodeKind::Condition => {}
                        other => {
                            return Err(WorkflowError::InvalidInput(format!(
                                "parallel fan edge {}->{} targets {} node; \
                                 fan-out children must be tool/knowledge/condition",
                                fan.from(),
                                fan.to(),
                                other.as_str()
                            )))
                        }
                    }
                    let in_degree = def
                        .edges()
                        .iter()
                        .filter(|e| e.to() == target.id())
                        .count();
                    if in_degree != 1 {
                        return Err(WorkflowError::InvalidInput(format!(
                            "parallel fan target {:?} has in-degree {in_degree}; \
                             fan-out children must have exactly the one fan edge",
                            target.id()
                        )));
                    }
                }
                if out.len() != fans.len() {
                    return Err(WorkflowError::InvalidInput(format!(
                        "parallel node {:?} has non-branch out-edge; parallel fan-out must be all branch",
                        node.id()
                    )));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// `$/` JSON Pointer 状态引用解析（RFC 6901 子集：`~1`→`/`、`~0`→`~`；
/// `$/` 后为空 = 整个 state）。递归处理 object/array 内全部字符串叶值。
pub(crate) fn resolve_refs(value: &Value, state: &Value) -> Result<Value, NodeFailure> {
    match value {
        Value::String(s) => {
            if let Some(pointer) = s.strip_prefix("$/") {
                let resolved = resolve_pointer(state, pointer).ok_or_else(|| {
                    NodeFailure::new(
                        "state_ref_unresolvable",
                        format!("state reference {s:?} does not resolve against run state"),
                    )
                })?;
                Ok(resolved.clone())
            } else {
                Ok(value.clone())
            }
        }
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(resolve_refs(item, state)?);
            }
            Ok(Value::Array(out))
        }
        Value::Object(map) => {
            let mut out = Map::new();
            for (k, v) in map {
                out.insert(k.clone(), resolve_refs(v, state)?);
            }
            Ok(Value::Object(out))
        }
        other => Ok(other.clone()),
    }
}

/// JSON Pointer 求值（RFC 6901 子集；空串 = 整个文档）。
fn resolve_pointer<'a>(document: &'a Value, pointer: &str) -> Option<&'a Value> {
    if pointer.is_empty() {
        return Some(document);
    }
    let mut current = document;
    for token in pointer.split('/') {
        let token = token.replace("~1", "/").replace("~0", "~");
        match current {
            Value::Object(map) => {
                current = map.get(&token)?;
            }
            Value::Array(items) => {
                let index: usize = token.parse().ok()?;
                current = items.get(index)?;
            }
            _ => return None,
        }
    }
    Some(current)
}

/// 引擎内 id 生成（`<prefix>-<hex24>`；agent-engine/context-broker 同源
/// 口径：时间戳 + 进程熵 + 进内序号，进程熵通道 RandomState 独立种子）。
pub(crate) fn new_id(prefix: &str) -> String {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    static SALT: std::sync::OnceLock<(u64, u64)> = std::sync::OnceLock::new();
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let (salt_a, salt_b) = *SALT.get_or_init(|| {
        (
            std::collections::hash_map::RandomState::new().build_hasher().finish(),
            std::collections::hash_map::RandomState::new().build_hasher().finish(),
        )
    });
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write(format!("{nanos}|{salt_a:x}{salt_b:x}|{seq}|{}|workflow-engine", std::process::id()).as_bytes());
    let digest = hasher.finish();
    format!("{prefix}-{digest:016x}{seq:08x}")
}

/// workflow run id（wfr- 前缀）。
pub(crate) fn new_run_id() -> String {
    new_id("wfr")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_refs_pointer_and_fail_closed() {
        let state = json!({"target": "example.com", "nested": {"a": 1}, "list": [10, 20]});
        assert_eq!(
            resolve_refs(&json!("$/target"), &state).expect("ref"),
            json!("example.com")
        );
        assert_eq!(
            resolve_refs(&json!({"k": "$/nested/a"}), &state).expect("nested"),
            json!({"k": 1})
        );
        assert_eq!(resolve_refs(&json!("$/list/1"), &state).expect("index"), json!(20));
        // 非引用字符串原样保留
        assert_eq!(
            resolve_refs(&json!("plain $ not ref"), &state).expect("plain"),
            json!("plain $ not ref")
        );
        let err = resolve_refs(&json!("$/missing"), &state).expect_err("unresolvable");
        assert_eq!(err.code, "state_ref_unresolvable");
    }

    #[test]
    fn edge_view_condition_and_parallel_branch() {
        let edge_true =
            domain::WorkflowEdge::try_new("c", "t", WorkflowEdgeKind::Branch, Some(r#"true"#.to_string()))
                .expect("edge");
        let edge_false =
            domain::WorkflowEdge::try_new("c", "f", WorkflowEdgeKind::Branch, Some(r#"false"#.to_string()))
                .expect("edge");
        let succeeded_condition = NodeRow {
            node_id: "c".to_string(),
            node_kind: "condition".to_string(),
            state: WorkflowNodeRunState::Succeeded,
            attempt: 1,
            output: Some(json!({"condition_result": true})),
            error: None,
            started_at: None,
            finished_at: None,
            tool_run_id: None,
            agent_run_id: None,
        };
        assert_eq!(edge_view(&edge_true, Some(&succeeded_condition)), EdgeView::Selected);
        assert_eq!(edge_view(&edge_false, Some(&succeeded_condition)), EdgeView::Dead);
        let pending = NodeRow {
            state: WorkflowNodeRunState::Pending,
            ..succeeded_condition.clone()
        };
        assert_eq!(edge_view(&edge_true, Some(&pending)), EdgeView::Waiting);
        let skipped = NodeRow {
            state: WorkflowNodeRunState::Skipped,
            ..succeeded_condition.clone()
        };
        assert_eq!(edge_view(&edge_true, Some(&skipped)), EdgeView::Dead);
        let fan = domain::WorkflowEdge::try_new("p", "w1", WorkflowEdgeKind::Branch, None).expect("edge");
        let parallel = NodeRow {
            node_id: "p".to_string(),
            node_kind: "parallel".to_string(),
            state: WorkflowNodeRunState::Succeeded,
            attempt: 1,
            output: None,
            error: None,
            started_at: None,
            finished_at: None,
            tool_run_id: None,
            agent_run_id: None,
        };
        assert_eq!(edge_view(&fan, Some(&parallel)), EdgeView::Selected);
    }

    #[test]
    fn new_run_id_shape_and_uniqueness() {
        let a = new_run_id();
        let b = new_run_id();
        assert!(a.starts_with("wfr-") && a.len() > 8, "{a}");
        assert_ne!(a, b);
        assert!(domain::RunId::try_new(a.clone()).is_ok());
    }
}
