//! Agent 执行闭环（计划 §9:454-477 八步流程的 P7-2 落面 + P7-4 编排批次
//! 运行控制全项，对应 ROADMAP §8.3 P7-5 内容行）。
//!
//! 每轮上下文唯一来源为 Broker（[`broker_link::BrokerLink`]），本模块零
//! 库表直读（知识/工具候选全从 AssembledContext 取）；规划为端口注入
//! （[`Planner`]），内置确定性规划器 [`KeywordSearchPlanner`]，模型在环
//! 属 P7-5 后续（ROADMAP §8.5 风险 3：确定性测试与模型在环分离）。
//!
//! 护栏（P7-2 最小面 + P7-4 九类控制全项，不留「先上循环后补护栏」缺口，
//! ROADMAP §8.5 风险 1）：步数上限 [`LoopConfig::max_tool_calls`]、能力绑定
//! 闸、失败即收尾（重试走 [`control::RunControl::retry`] 新 run 重放）、
//! 暂停/恢复（协作式请求位 + [`Self::resume`] 状态续接）、协作取消
//! （D-TC-04 语义）、停滞检测、重复调用检测、目标偏移检测、证据不足提示、
//! 预算与资源限制（轮次/墙钟预算 + 并行子任务默认关闭）。终态经
//! domain::agent 跃迁表 CAS 落库。
//!
//! 状态续接口径（暂停后续跑）：步骤轨迹逐事件落 agent_run_step（014 迁移，
//! [`state::RunStateStore`]），resume 只执行轨迹之外的步骤（已完成步骤不
//! 重跑）；resume 重装配上下文（每轮上下文唯一来源 Broker），新段首个计划
//! 面重置偏移基准；audit_event 事件链 correlation_id 全程 = run_id。

use crate::broker_link::BrokerLink;
use crate::control::{self, RunControlConfig};
use crate::error::AgentError;
use crate::evidence::{EvidenceRegistrar, Registration};
use crate::memory::MemoryStore;
use crate::state::RunStateStore;
use crate::store::AgentStore;
use crate::tool_link::{ToolCallSpec, ToolLink};
use audit_engine::AuditEngine;
use context_broker::{
    AssembleRequest, AssembledContext, ContextItemKind, ContextLayer, InclusionForm,
};
use domain::{
    AgentId, AgentRun, AgentRunState, CapabilityRef, EvidenceMemoryEntry, MemoryLifecycle,
    MemoryObjectRef, ResearchMemoryEntry, RunId, TaskMemoryEntry,
};
use serde::Serialize;
use serde_json::{json, Value};

/// 闭环配置。
#[derive(Debug, Clone)]
pub struct LoopConfig {
    /// 工作区隔离边界（ToolCallRequest.workspace_id）。
    pub workspace_id: String,
    /// 单次 run 的工具调用步数上限（护栏）。
    pub max_tool_calls: usize,
}

impl Default for LoopConfig {
    fn default() -> Self {
        Self {
            workspace_id: "ws-default".to_string(),
            max_tool_calls: 4,
        }
    }
}

/// 计划上下文（每轮由已装配上下文 + 历史构成，Planner 只读）。
pub struct PlanContext<'a> {
    /// 原始目标文本。
    pub goal: &'a str,
    /// 当前 run id（P7-4：控制回路视图，规划器可据此经控制面发起协作式
    /// 暂停/取消请求；审计 correlation_id 同源）。
    pub run_id: &'a str,
    /// Broker 装配上下文（每轮上下文唯一来源）。
    pub context: &'a AssembledContext,
    /// 能力绑定（definition.capability_refs）。
    pub capability_refs: &'a [CapabilityRef],
    /// 已执行步骤（含失败；resume 时含轨迹续接步）。
    pub history: &'a [StepRecord],
    /// 当前轮次（0 起；resume 从轨迹最大轮次 + 1 起）。
    pub round: usize,
    /// 显式入参（CLI --input；覆盖规划器推导）。
    pub explicit_input: Option<&'a Value>,
    /// 步数上限（护栏视图）。
    pub max_tool_calls: usize,
}

/// 单个计划步骤。
#[derive(Debug, Clone)]
pub struct PlanStep {
    /// 目标工具 id。
    pub tool_id: String,
    /// 版本钉定（None = current）。
    pub version: Option<String>,
    /// 工具入参。
    pub input: Value,
    /// 规划理由（可解释面）。
    pub rationale: String,
}

/// 每轮规划决策。
pub enum Decision {
    /// 继续执行给定步骤。
    Steps(Vec<PlanStep>),
    /// 结束（原因进报告）。
    Finish {
        /// 结束原因。
        reason: String,
    },
}

/// 规划端口。
pub trait Planner {
    /// 每轮决策。
    fn plan(&self, ctx: &PlanContext<'_>) -> Decision;
}

/// 已执行步骤记录（报告与下一轮规划输入）。
#[derive(Debug, Clone, Serialize)]
pub struct StepRecord {
    /// 工具 id。
    pub tool_id: String,
    /// 执行版本。
    pub tool_version: String,
    /// tool_run id。
    pub tool_run_id: String,
    /// 终态（ToolRunStatus 词形；受理拒绝记 SUBMIT_REJECTED）。
    pub status: String,
    /// 登记的 artifact id。
    pub artifact_id: Option<String>,
    /// 登记的 evidence id。
    pub evidence_id: Option<String>,
    /// 登记产物 sha256（P7-4 停滞检测钉面；未登记为 None）。
    pub content_hash: Option<String>,
    /// 信封摘要。
    pub summary: Option<String>,
    /// 错误信息（信封 error 或受理拒绝原因）。
    pub error: Option<String>,
    /// 执行耗时（信封 usage.elapsed_ms；受理拒绝为 0）。
    pub elapsed_ms: u64,
}

/// L3 展开留痕（expand 记账摘要）。
#[derive(Debug, Clone, Serialize)]
pub struct L3Expansion {
    /// 展开条目 id。
    pub item_id: String,
    /// 本次授予 tokens（l3_reserved 扣账）。
    pub granted_tokens: u64,
    /// 扣账后剩余额度。
    pub remaining_l3: u64,
}

/// run 请求。
pub struct RunRequest {
    /// agent id。
    pub agent_id: String,
    /// 定义版本（None = 最新登记版本）。
    pub agent_version: Option<String>,
    /// 目标文本（意图分类与关键词推导输入）。
    pub goal: String,
    /// 关联研究任务（可空；缺省以 run id 关联）。
    pub task_id: Option<String>,
    /// 显式工具入参（覆盖规划器推导；合工具 input_schema）。
    pub input: Option<Value>,
    /// 规划器。
    pub planner: Box<dyn Planner>,
    /// 意图确认者（ambiguous 回路）。
    pub confirmer: Box<dyn crate::broker_link::IntentConfirmer>,
}

/// 恢复请求（P7-4 暂停后续跑）：run 必须处于 PAUSED；目标文本从
/// agent_run_control.goal_text 续接（不重传），已完成步骤从 agent_run_step
/// 轨迹续接（不重跑）。
pub struct ResumeRequest {
    /// 暂停中的 run id。
    pub run_id: String,
    /// 恢复后的规划器（每轮上下文唯一来源 Broker，新段重置偏移基准）。
    pub planner: Box<dyn Planner>,
    /// 意图确认者（重装配的 ambiguous 回路）。
    pub confirmer: Box<dyn crate::broker_link::IntentConfirmer>,
}

/// 结束报告（计划 §9:454-477 第 8 步；§8.4 CLI 验收输出）。
#[derive(Debug, Clone, Serialize)]
pub struct RunReport {
    /// agent run id（arn- 前缀，审计 correlation_id）。
    pub run_id: String,
    /// agent id。
    pub agent_id: String,
    /// 钉定定义版本。
    pub agent_version: String,
    /// 关联任务。
    pub task_id: Option<String>,
    /// 目标原文。
    pub goal: String,
    /// 终态（AgentRunState 词形；PAUSED = 暂停待恢复，非终态）。
    pub state: String,
    /// 装配清单 id（context_ref_log 对账入口）。
    pub manifest_id: String,
    /// 装配上下文 id。
    pub context_id: String,
    /// 意图状态（ok / ambiguous）。
    pub intent_status: String,
    /// 意图置信度。
    pub intent_confidence: f32,
    /// ambiguous 确认词（按发生顺序）。
    pub confirmations: Vec<String>,
    /// L3 展开留痕。
    pub l3_expansions: Vec<L3Expansion>,
    /// 工具步骤记录（resume 时含轨迹续接步，按 seq 序）。
    pub steps: Vec<StepRecord>,
    /// 警告（弃权确认/展开失败/登记失败/护栏触发等）。
    pub warnings: Vec<String>,
    /// 结束原因。
    pub finish_reason: String,
    /// Research Memory 条目 id（P7-3 可选接线；未挂 Memory、非终态收尾或
    /// 落账失败为 None）。
    pub research_memory_id: Option<String>,
    /// 轨迹续接步数（resume 时 > 0；fresh run 为 0；P7-4 additive 字段）。
    pub resumed_steps: usize,
}

/// 内部驱动参数（run / resume 共用执行尾；区别只在历史续接与起始轮次）。
struct DriveParams<'a> {
    run_id: &'a RunId,
    run_id_str: &'a str,
    actor: &'a str,
    agent_id: &'a str,
    agent_version: &'a str,
    capability_refs: &'a [CapabilityRef],
    task_id: Option<String>,
    goal: String,
    explicit_input: Option<Value>,
    planner: &'a dyn Planner,
    confirmer: &'a mut dyn crate::broker_link::IntentConfirmer,
    /// 轨迹续接步（fresh run 为空；resume 时按 seq 升序）。
    history: Vec<StepRecord>,
    /// 起始轮次（fresh run 0；resume 为轨迹最大轮次 + 1）。
    start_round: usize,
    /// 是否为恢复段（报告留痕 + 警告标注）。
    resumed: bool,
    /// 已执行调用键（重复调用检测；resume 由轨迹重建）。
    executed_keys: Vec<String>,
    /// 已见证据哈希（停滞检测；resume 由轨迹重建）。
    seen_hashes: Vec<String>,
}

/// Agent 执行闭环。
pub struct AgentLoop {
    store: AgentStore,
    state: RunStateStore,
    broker: BrokerLink,
    tool: ToolLink,
    evidence: EvidenceRegistrar,
    audit: Option<AuditEngine>,
    config: LoopConfig,
    control: RunControlConfig,
    memory: Option<MemoryStore>,
}

impl AgentLoop {
    /// 在四件依赖之上构造（审计可选旁路；Memory 缺省关闭，见 [`Self::with_memory`]；
    /// 控制参数缺省 [`RunControlConfig::default`]，见 [`Self::with_control`]）。
    pub fn new(
        store: AgentStore,
        broker: BrokerLink,
        tool: ToolLink,
        evidence: EvidenceRegistrar,
        audit: Option<AuditEngine>,
        config: LoopConfig,
    ) -> Self {
        Self {
            state: RunStateStore::new(store.pool().clone()),
            store,
            broker,
            tool,
            evidence,
            audit,
            config,
            control: RunControlConfig::default(),
            memory: None,
        }
    }

    /// 覆盖运行控制参数（P7-4 九类护栏；缺省即低并发纪律最严面）。
    pub fn with_control(mut self, control: RunControlConfig) -> Self {
        self.control = control;
        self
    }

    /// 挂三类 Memory 数据面（P7-3 可选接线，默认关闭）：run 执行期间写
    /// Task/Evidence/Research 三类记忆，写失败降级 warning 不炸 run。
    pub fn with_memory(mut self, memory: MemoryStore) -> Self {
        self.memory = Some(memory);
        self
    }

    /// 执行一次 run（八步全流程 + P7-4 控制面）：受理（goal 落
    /// agent_run_control）→ RUNNING → 装配 → 展开 → 规划执行（步间消费
    /// 协作式暂停/取消请求 + 九类护栏）→ 终态落库。任何步骤失败不炸报告
    /// 面，失败进终态与记录。
    pub fn run(&self, req: &mut RunRequest) -> Result<RunReport, AgentError> {
        // 第 1 步前置：定义解析与引用校验（登记后再校验，防 Registry 漂移）。
        let stored = match &req.agent_version {
            Some(v) => self.store.definition(&req.agent_id, v)?,
            None => self.store.latest_definition(&req.agent_id)?,
        }
        .ok_or_else(|| {
            AgentError::Definition(format!("definition {:?} not found", req.agent_id))
        })?;
        self.store.validate_capability_refs(&stored.definition)?;
        let agent_id = stored.definition.agent_id().as_str().to_string();

        // 受理：QUEUED 落行 → RUNNING（CAS 记 started_at）；goal 落控制行
        // （resume 状态续接的装配输入，014 agent_run_control.goal_text）。
        let run_id = AgentStore::new_run_id();
        let run = AgentRun::try_new(
            run_id.clone(),
            req.task_id.clone(),
            stored.definition.agent_id().clone(),
            stored.definition.version().to_string(),
            AgentRunState::Queued,
            vec![],
        )?;
        self.store.create_run(&run)?;
        let run_id_str = run_id.as_str().to_string();
        let actor = format!("agent-engine:{agent_id}");
        self.state.ensure_control_row(&run_id_str, &req.goal)?;
        self.audit_emit(
            "agent.run.accepted",
            &run_id_str,
            &actor,
            json!({"agent_id": agent_id, "goal": req.goal}),
        );
        self.store
            .transition_run(&run_id_str, AgentRunState::Running)?;

        self.drive(DriveParams {
            run_id: &run_id,
            run_id_str: &run_id_str,
            actor: &actor,
            agent_id: &agent_id,
            agent_version: stored.definition.version(),
            capability_refs: stored.definition.capability_refs(),
            task_id: req.task_id.clone(),
            goal: req.goal.clone(),
            explicit_input: req.input.clone(),
            planner: &*req.planner,
            confirmer: req.confirmer.as_mut(),
            history: Vec::new(),
            start_round: 0,
            resumed: false,
            executed_keys: Vec::new(),
            seen_hashes: Vec::new(),
        })
    }

    /// 恢复暂停中的 run（P7-4）：PAUSED → RUNNING（CAS）后按轨迹续接执行，
    /// 已完成步骤不重跑。目标文本从 agent_run_control.goal_text 续接；
    /// 重复调用键与停滞已见哈希从轨迹重建（跨段连续）；偏移基准在新段首个
    /// 计划面重置（恢复后的规划是新的决策上下文）。
    pub fn resume(&self, req: &mut ResumeRequest) -> Result<RunReport, AgentError> {
        let stored = self
            .store
            .run(&req.run_id)?
            .ok_or_else(|| AgentError::RunNotFound(req.run_id.clone()))?;
        if stored.run.state() != AgentRunState::Paused {
            return Err(AgentError::Plan(format!(
                "resume requires PAUSED run; run {} is {}",
                req.run_id,
                stored.run.state().as_str()
            )));
        }
        let goal = self.state.goal_text(&req.run_id)?.ok_or_else(|| {
            AgentError::Plan(format!(
                "run {} has no persisted goal (agent_run_control.goal_text); resume 状态续接缺装配输入",
                req.run_id
            ))
        })?;
        let definition = self
            .store
            .definition(stored.run.agent_id().as_str(), stored.run.agent_version())?
            .ok_or_else(|| {
                AgentError::Definition(format!(
                    "definition {}@{} not found for resume",
                    stored.run.agent_id(),
                    stored.run.agent_version()
                ))
            })?;
        self.store
            .validate_capability_refs(&definition.definition)?;

        let persisted = self.state.steps(&req.run_id)?;
        let agent_id = stored.run.agent_id().as_str().to_string();
        let actor = format!("agent-engine:{agent_id}");

        // CAS PAUSED → RUNNING（域跃迁表合法对），恢复事件留痕。
        self.store
            .transition_run(&req.run_id, AgentRunState::Running)?;
        self.audit_emit(
            control::actions::RESUMED,
            &req.run_id,
            &actor,
            json!({"replayed_steps": persisted.len()}),
        );

        // 轨迹续接：历史步骤 + 重复调用键 + 停滞已见哈希；轮次续接。
        let history: Vec<StepRecord> = persisted.iter().map(|s| s.record.clone()).collect();
        let executed_keys: Vec<String> = persisted
            .iter()
            .map(|s| control::canonical_call_key(&s.record.tool_id, &s.input_json))
            .collect();
        let seen_hashes: Vec<String> = persisted
            .iter()
            .filter_map(|s| s.record.content_hash.clone())
            .filter(|h| !h.is_empty())
            .collect();
        let start_round = persisted.iter().map(|s| s.round).max().unwrap_or(0) + 1;
        let run_id = stored.run.run_id().clone();

        self.drive(DriveParams {
            run_id: &run_id,
            run_id_str: &req.run_id,
            actor: &actor,
            agent_id: &agent_id,
            agent_version: stored.run.agent_version(),
            capability_refs: definition.definition.capability_refs(),
            task_id: stored.run.task_id().map(str::to_string),
            goal,
            explicit_input: None,
            planner: &*req.planner,
            confirmer: req.confirmer.as_mut(),
            history,
            start_round,
            resumed: true,
            executed_keys,
            seen_hashes,
        })
    }

    /// 执行尾（run / resume 共用）：装配 → 展开 → 规划执行（步间控制消费
    /// + 九类护栏）→ 终态落库 → 报告。
    fn drive(&self, p: DriveParams<'_>) -> Result<RunReport, AgentError> {
        let started_at = std::time::Instant::now();
        let cfg = self.control.clone();

        // 第 2 步：Context Broker 装配（ambiguous 确认回路）。
        let task_id = p
            .task_id
            .clone()
            .unwrap_or_else(|| p.run_id_str.to_string());
        let assemble = self.broker.assemble_confirmed(
            &AssembleRequest {
                task_id,
                text: p.goal.clone(),
                correlation_id: p.run_id_str.to_string(),
                actor: p.actor.to_string(),
            },
            p.confirmer,
        )?;
        let context = assemble.context;
        let mut warnings = context.warnings.clone();
        if p.resumed {
            warnings.push(format!(
                "resumed: {} persisted steps replayed from agent_run_step (state continuity, no re-execution)",
                p.history.len()
            ));
        }
        let confirmations = assemble.confirmations;

        // 第 3 步：L3 按需展开（L2 Summary 形态的 Knowledge/Plugin 条目，
        // EXPANDED 记账进 context_ref_log；展开失败降级 warning 不炸 run）。
        let mut l3_expansions: Vec<L3Expansion> = Vec::new();
        let view = self.broker.manifest(&context.manifest_id)?;
        let expand_target = view.items.iter().find(|i| {
            i.layer == ContextLayer::L2
                && i.included == InclusionForm::Summary
                && matches!(i.kind, ContextItemKind::Knowledge | ContextItemKind::Plugin)
        });
        match expand_target {
            Some(item) => match self.broker.expand(&context.manifest_id, &item.item_id) {
                Ok(exp) => l3_expansions.push(L3Expansion {
                    item_id: exp.item_id,
                    granted_tokens: exp.granted_tokens,
                    remaining_l3: exp.remaining_l3,
                }),
                Err(e) => warnings.push(format!("l3 expand declined for {:?}: {e}", item.item_id)),
            },
            None => warnings.push(
                "no L2 knowledge/plugin summary item in manifest; L3 expansion skipped".to_string(),
            ),
        }

        // 第 4 到 7 步：规划 + 工具调用 + 登记 + 继续判断（P7-4 控制面内嵌：
        // 步间消费协作式暂停/取消请求，九类护栏即时生效）。
        let mut steps: Vec<StepRecord> = p.history.clone();
        let mut executed_keys = p.executed_keys.clone();
        let mut seen_hashes = p.seen_hashes.clone();
        let mut finish_reason = String::new();
        let mut any_failed = false;
        let mut control_state: Option<AgentRunState> = None;
        let mut surface: Option<Vec<String>> = None;
        let mut drift_streak = 0usize;
        let mut no_progress_streak = 0usize;
        let mut round = p.start_round;

        'rounds: loop {
            // 控制消费：取消先于暂停（取消是终止意愿，优先级最高）。消费点
            // 立即完成 CAS 跃迁（RUNNING→CANCELLED / RUNNING→PAUSED，域校验
            // + CAS 统一走 store），审计随跃迁落。
            if self.state.consume_cancel_request(p.run_id_str)? {
                control_state = Some(AgentRunState::Cancelled);
                finish_reason = format!(
                    "cancelled (cooperative, D-TC-04): request consumed before round {round} step {}",
                    steps.len()
                );
                self.store
                    .transition_run(p.run_id_str, AgentRunState::Cancelled)?;
                self.audit_emit(
                    control::actions::CANCELLED,
                    p.run_id_str,
                    p.actor,
                    json!({"before_step": steps.len(), "round": round, "cooperative": true}),
                );
                break 'rounds;
            }
            if self.state.consume_pause_request(p.run_id_str)? {
                control_state = Some(AgentRunState::Paused);
                finish_reason = format!(
                    "paused (cooperative): request consumed before round {round} step {}; resume via AgentLoop::resume",
                    steps.len()
                );
                self.store
                    .transition_run(p.run_id_str, AgentRunState::Paused)?;
                self.audit_emit(
                    control::actions::PAUSED,
                    p.run_id_str,
                    p.actor,
                    json!({"before_step": steps.len(), "round": round}),
                );
                break 'rounds;
            }
            // 预算：轮次上限。
            if cfg.max_rounds > 0 && round >= cfg.max_rounds {
                finish_reason = format!("guardrail: budget max_rounds={} reached", cfg.max_rounds);
                self.audit_emit(
                    control::actions::GUARDRAIL,
                    p.run_id_str,
                    p.actor,
                    json!({"kind": "budget_rounds", "round": round, "limit": cfg.max_rounds}),
                );
                break 'rounds;
            }
            // 预算：步数上限（P7-2 既有护栏）。
            if steps.len() >= self.config.max_tool_calls {
                finish_reason = format!(
                    "guardrail: max_tool_calls={} reached",
                    self.config.max_tool_calls
                );
                break 'rounds;
            }
            let ctx = PlanContext {
                goal: &p.goal,
                run_id: p.run_id_str,
                context: &context,
                capability_refs: p.capability_refs,
                history: &steps,
                round,
                explicit_input: p.explicit_input.as_ref(),
                max_tool_calls: self.config.max_tool_calls,
            };
            match p.planner.plan(&ctx) {
                Decision::Finish { reason } => {
                    finish_reason = reason;
                    break 'rounds;
                }
                Decision::Steps(plan_steps) => {
                    if plan_steps.is_empty() {
                        finish_reason = "planner produced no executable steps".to_string();
                        break 'rounds;
                    }
                    // 资源限制：并行子任务默认关闭（低并发纪律），单轮多步骤
                    // 串行执行并留痕。
                    if plan_steps.len() > 1 && !cfg.allow_parallel_subtasks {
                        warnings.push(format!(
                            "parallel subtasks disabled (low-concurrency policy): {} planned steps of round {round} serialized",
                            plan_steps.len()
                        ));
                    }
                    // 初始计划面捕获（首个产出步骤的 plan；偏移检测基准）。
                    if surface.is_none() {
                        surface = Some(plan_steps.iter().map(|s| s.tool_id.clone()).collect());
                    }
                    let mut executed_in_round = 0usize;
                    let mut duplicates_in_round = 0usize;
                    for step in plan_steps {
                        // 每步控制消费（步间粒度；cancel 先于 pause）。消费点
                        // 立即完成 CAS 跃迁，审计随跃迁落。
                        if self.state.consume_cancel_request(p.run_id_str)? {
                            control_state = Some(AgentRunState::Cancelled);
                            finish_reason = format!(
                                "cancelled (cooperative, D-TC-04): request consumed before step {}",
                                steps.len()
                            );
                            self.store
                                .transition_run(p.run_id_str, AgentRunState::Cancelled)?;
                            self.audit_emit(
                                control::actions::CANCELLED,
                                p.run_id_str,
                                p.actor,
                                json!({"before_step": steps.len(), "round": round, "cooperative": true}),
                            );
                            break 'rounds;
                        }
                        if self.state.consume_pause_request(p.run_id_str)? {
                            control_state = Some(AgentRunState::Paused);
                            finish_reason = format!(
                                "paused (cooperative): request consumed before step {}; resume via AgentLoop::resume",
                                steps.len()
                            );
                            self.store
                                .transition_run(p.run_id_str, AgentRunState::Paused)?;
                            self.audit_emit(
                                control::actions::PAUSED,
                                p.run_id_str,
                                p.actor,
                                json!({"before_step": steps.len(), "round": round}),
                            );
                            break 'rounds;
                        }
                        // 预算：墙钟（0 = 不限）。
                        if control::elapsed_over_budget(started_at, cfg.max_elapsed_ms) {
                            finish_reason = format!(
                                "guardrail: budget max_elapsed_ms={} reached",
                                cfg.max_elapsed_ms
                            );
                            self.audit_emit(
                                control::actions::GUARDRAIL,
                                p.run_id_str,
                                p.actor,
                                json!({"kind": "budget_elapsed", "limit_ms": cfg.max_elapsed_ms}),
                            );
                            break 'rounds;
                        }
                        // 预算：步数上限。
                        if steps.len() >= self.config.max_tool_calls {
                            finish_reason = format!(
                                "guardrail: max_tool_calls={} reached",
                                self.config.max_tool_calls
                            );
                            break 'rounds;
                        }
                        // 能力绑定闸：绑定外工具拒绝（防规划器漂移出能力面）。
                        if !p
                            .capability_refs
                            .iter()
                            .any(|r| r.tool_id() == step.tool_id)
                        {
                            any_failed = true;
                            finish_reason = format!(
                                "guardrail: plan step tool {:?} is not in capability_refs",
                                step.tool_id
                            );
                            break 'rounds;
                        }
                        // 目标偏移检测：初始计划面外的工具跳转计偏移事件，
                        // 连续偏移超过容限即停（容限内放行并留痕）。
                        let surface_ref = surface
                            .as_deref()
                            .expect("surface captured before step loop");
                        if control::drift_step(surface_ref, &step.tool_id) {
                            drift_streak += 1;
                            if drift_streak > cfg.drift_tolerance {
                                finish_reason = format!(
                                    "guardrail: goal drift detected (tool {:?} outside initial plan surface, {drift_streak} consecutive drifted steps)",
                                    step.tool_id
                                );
                                self.audit_emit(
                                    control::actions::GUARDRAIL,
                                    p.run_id_str,
                                    p.actor,
                                    json!({"kind": "goal_drift", "tool_id": step.tool_id,
                                           "drift_streak": drift_streak, "tolerance": cfg.drift_tolerance}),
                                );
                                break 'rounds;
                            }
                            warnings.push(format!(
                                "goal drift event: tool {:?} outside initial plan surface (round {round}); within tolerance {}",
                                step.tool_id, cfg.drift_tolerance
                            ));
                        } else {
                            drift_streak = 0;
                        }
                        // 重复调用检测：(tool_id, 规范化 input) 已执行即跳过。
                        let call_key = control::canonical_call_key(&step.tool_id, &step.input);
                        if executed_keys.iter().any(|k| k == &call_key) {
                            duplicates_in_round += 1;
                            warnings.push(format!(
                                "duplicate call detected: tool {:?} with identical canonical input already executed; skipped",
                                step.tool_id
                            ));
                            self.audit_emit(
                                control::actions::STEP_DUPLICATE,
                                p.run_id_str,
                                p.actor,
                                json!({"tool_id": step.tool_id, "round": round}),
                            );
                            continue;
                        }
                        // 执行 + 轨迹落库。
                        let seq = steps.len();
                        let record = self.execute_step(
                            p.run_id,
                            seq,
                            round,
                            p.agent_id,
                            &step,
                            p.task_id.as_deref(),
                            &context,
                            &mut warnings,
                        );
                        executed_keys.push(call_key);
                        if record.status != "SUCCEEDED" {
                            any_failed = true;
                        }
                        // 停滞检测：连续无新证据哈希步数达窗口即停（窗口 0
                        // 视为关闭，与 max_rounds/max_elapsed_ms 同口径）。
                        if control::step_has_new_evidence(&record, &seen_hashes) {
                            no_progress_streak = 0;
                            if let Some(h) = &record.content_hash {
                                seen_hashes.push(h.clone());
                            }
                        } else {
                            no_progress_streak += 1;
                        }
                        steps.push(record.clone());
                        self.audit_emit(
                            "agent.step.completed",
                            p.run_id_str,
                            p.actor,
                            json!({
                                "tool_id": step.tool_id,
                                "seq": seq,
                                "round": round,
                                "status": record.status,
                                "tool_run_id": record.tool_run_id,
                                "artifact_id": record.artifact_id,
                                "evidence_id": record.evidence_id,
                                "steps_done": steps.len(),
                            }),
                        );
                        if cfg.stall_window > 0 && no_progress_streak >= cfg.stall_window {
                            finish_reason = format!(
                                "guardrail: stall detected ({no_progress_streak} steps without new evidence hash)"
                            );
                            self.audit_emit(
                                control::actions::GUARDRAIL,
                                p.run_id_str,
                                p.actor,
                                json!({"kind": "stall", "streak": no_progress_streak,
                                       "window": cfg.stall_window}),
                            );
                            break 'rounds;
                        }
                        // 失败即收尾（重试走 RunControl::retry 新 run 重放）。
                        if any_failed {
                            if finish_reason.is_empty() {
                                finish_reason =
                                    "tool step failed; stopping (retry via RunControl::retry)"
                                        .to_string();
                            }
                            break 'rounds;
                        }
                        executed_in_round += 1;
                    }
                    // 整轮全是重复跳步：无可执行进展，停（防规划器死循环）。
                    if finish_reason.is_empty() && executed_in_round == 0 && duplicates_in_round > 0
                    {
                        finish_reason =
                            "guardrail: duplicate call detected (all planned steps duplicate executed calls); no executable progress"
                                .to_string();
                        self.audit_emit(
                            control::actions::GUARDRAIL,
                            p.run_id_str,
                            p.actor,
                            json!({"kind": "duplicate_exhausted", "round": round,
                                   "duplicates": duplicates_in_round}),
                        );
                    }
                    if any_failed && finish_reason.is_empty() {
                        finish_reason =
                            "tool step failed; stopping (retry via RunControl::retry)".to_string();
                    }
                }
            }
            if any_failed || !finish_reason.is_empty() {
                break;
            }
            round += 1;
        }
        if finish_reason.is_empty() {
            finish_reason = "plan loop exited without explicit finish".to_string();
        }

        // 第 8 步前置：终态落库（域跃迁表 + CAS）。控制消费路径（暂停/取消）
        // 的跃迁已在消费点完成，此处只落自然终态。
        let final_state = control_state.unwrap_or(if any_failed {
            AgentRunState::Failed
        } else {
            AgentRunState::Succeeded
        });
        if control_state.is_none() {
            self.store.transition_run(p.run_id_str, final_state)?;
            self.audit_emit(
                "agent.run.finished",
                p.run_id_str,
                p.actor,
                json!({
                    "state": final_state.as_str(),
                    "steps": steps.len(),
                    "finish_reason": finish_reason,
                }),
            );
        }

        // 证据不足提示（P7-4）：可用证据（SUCCEEDED 且登记 evidence）低于
        // 阈值即在报告留提示（提示不改变终态）。
        if control::insufficient_evidence(&steps, cfg.min_evidence_hint) {
            let usable = steps
                .iter()
                .filter(|s| s.status == "SUCCEEDED" && s.evidence_id.is_some())
                .count();
            warnings.push(format!(
                "insufficient evidence: {usable} usable evidence rows (SUCCEEDED with registered evidence) below min {}; conclusions 待验证",
                cfg.min_evidence_hint
            ));
        }

        // 第 9 步（P7-3 可选）：Research Memory 落账（Context Memory 雏形）。
        // 仅终态收尾落账（PAUSED 是中间态，恢复收尾时再落）；回链不足
        // 或写失败降级 warning，不炸 run。
        let manifest_id = context.manifest_id.clone();
        let mut research_memory_id: Option<String> = None;
        if final_state.is_terminal() {
            if let Some(memory) = &self.memory {
                match self.record_research_memory(
                    memory,
                    p.run_id,
                    &p.goal,
                    p.task_id.as_deref(),
                    p.agent_id,
                    &manifest_id,
                    final_state,
                    &finish_reason,
                    &l3_expansions,
                    &steps,
                ) {
                    Ok(id) => research_memory_id = Some(id),
                    Err(e) => warnings.push(format!("research memory record failed: {e}")),
                }
            }
        }

        Ok(RunReport {
            run_id: p.run_id_str.to_string(),
            agent_id: p.agent_id.to_string(),
            agent_version: p.agent_version.to_string(),
            task_id: p.task_id.clone(),
            goal: p.goal.clone(),
            state: final_state.as_str().to_string(),
            manifest_id,
            context_id: context.context_id,
            intent_status: context.intent.status,
            intent_confidence: context.intent.confidence,
            confirmations,
            l3_expansions,
            steps,
            warnings,
            finish_reason,
            research_memory_id,
            resumed_steps: if p.resumed { p.history.len() } else { 0 },
        })
    }

    /// Research Memory 落账（P7-3 可选接线）：回链 = L3 展开条目（manifest
    /// item）+ 各步骤登记的 artifact/evidence；回链不足（无任何可解析引用）
    /// 如实拒绝不伪造，run memory_refs 追加 `mem://memory/{id}`。
    #[allow(clippy::too_many_arguments)]
    fn record_research_memory(
        &self,
        memory: &MemoryStore,
        run_id: &RunId,
        goal: &str,
        task_id: Option<&str>,
        agent_id: &str,
        manifest_id: &str,
        final_state: AgentRunState,
        finish_reason: &str,
        l3_expansions: &[L3Expansion],
        steps: &[StepRecord],
    ) -> Result<String, AgentError> {
        let mut refs: Vec<MemoryObjectRef> = Vec::new();
        for exp in l3_expansions {
            refs.push(MemoryObjectRef::manifest_item(
                manifest_id.to_string(),
                exp.item_id.clone(),
            )?);
        }
        for step in steps {
            if let Some(artifact_id) = &step.artifact_id {
                refs.push(MemoryObjectRef::artifact(artifact_id.clone())?);
            }
            if let Some(evidence_id) = &step.evidence_id {
                refs.push(MemoryObjectRef::evidence(evidence_id.clone())?);
            }
        }
        if refs.is_empty() {
            return Err(AgentError::Definition(
                "no resolvable back-links (no l3 expansion, no registered artifact/evidence); \
                 research memory would be untraceable"
                    .to_string(),
            ));
        }
        let title: String = goal.chars().take(80).collect();
        let summary = format!(
            "agent run {} at state {}; {} tool steps; finish: {}",
            run_id.as_str(),
            final_state.as_str(),
            steps.len(),
            finish_reason
        );
        let content = json!({
            "manifest_id": manifest_id,
            "steps": steps,
        });
        let entry = ResearchMemoryEntry::try_new(
            crate::new_id("mem-r"),
            run_id.clone(),
            task_id.map(str::to_string),
            AgentId::try_new(agent_id)?,
            title,
            summary,
            content.to_string(),
            refs,
            MemoryLifecycle::Active,
        )?;
        memory.research().record(&entry)?;
        self.store.append_memory_refs(
            run_id.as_str(),
            &[format!("mem://memory/{}", entry.memory_id())],
        )?;
        Ok(entry.memory_id().to_string())
    }

    /// 执行单步：受理 + 驱动 + Artifact/Evidence 登记 + 步骤轨迹落库
    /// （agent_run_step，resume 续接权威面）+ memory_refs 记账 + 三类
    /// Memory 落账（可选接线，写失败降级 warning 不炸 run）。
    #[allow(clippy::too_many_arguments)]
    fn execute_step(
        &self,
        run_id: &RunId,
        seq: usize,
        round: usize,
        agent_id: &str,
        step: &PlanStep,
        task_id: Option<&str>,
        context: &AssembledContext,
        warnings: &mut Vec<String>,
    ) -> StepRecord {
        let mut record = StepRecord {
            tool_id: step.tool_id.clone(),
            tool_version: String::new(),
            tool_run_id: String::new(),
            status: "SUBMIT_REJECTED".to_string(),
            artifact_id: None,
            evidence_id: None,
            content_hash: None,
            summary: None,
            error: None,
            elapsed_ms: 0,
        };
        let spec = ToolCallSpec {
            tool_id: step.tool_id.clone(),
            version: step.version.clone(),
            input: step.input.clone(),
            task_id: task_id.map(str::to_string),
            workspace_id: self.config.workspace_id.clone(),
            // 幂等提交键按步序区分（P7-4 多步 run：同工具不同入参的两步
            // 不能共享 idempotency_key 直通面）。
            submit_key: Some(format!(
                "agent:{}:{}:{}",
                run_id.as_str(),
                seq,
                step.tool_id
            )),
        };
        let called = match self.tool.call(&spec) {
            Ok(called) => called,
            Err(e) => {
                record.error = Some(e.to_string());
                self.persist_step_trajectory(run_id, seq, round, step, &record, warnings);
                return record;
            }
        };
        record.tool_run_id = called.run_id.clone();
        record.tool_version = called.tool_version.clone();
        record.status = called.envelope.status.as_str().to_string();
        record.summary = called.envelope.summary.clone();
        record.elapsed_ms = called.envelope.usage.elapsed_ms;
        if let Some(err) = &called.envelope.error {
            record.error = Some(err.message.clone());
        }
        // 第 5 步：解析结果登记 Artifact/Evidence（SUCCEEDED 取 result，
        // 失败取错误对象；同哈希对账）。
        let payload = called.envelope.result.clone().unwrap_or_else(|| {
            json!({
                "error_category": called.envelope.error.as_ref().map(|e| e.category.clone()),
                "error_message": called.envelope.error.as_ref().map(|e| e.message.clone()),
            })
        });
        match self.evidence.register_tool_result(&Registration {
            agent_id: agent_id.to_string(),
            task_id: task_id.map(str::to_string),
            tool_run_id: called.run_id.clone(),
            tool_id: step.tool_id.clone(),
            tool_version: called.tool_version.clone(),
            payload,
        }) {
            Ok(reg_out) => {
                record.artifact_id = Some(reg_out.artifact_id.clone());
                record.evidence_id = Some(reg_out.evidence_id.clone());
                record.content_hash = Some(reg_out.content_hash.clone());
                let mem_refs = vec![
                    format!("mem://manifest/{}", context.manifest_id),
                    format!("mem://artifact/{}", reg_out.artifact_id),
                    format!("mem://evidence/{}", reg_out.evidence_id),
                ];
                if let Err(e) = self.store.append_memory_refs(run_id.as_str(), &mem_refs) {
                    warnings.push(format!("memory_refs append failed: {e}"));
                }
                // P7-3 三类 Memory 落账（可选接线）：Task 工作记忆 + Evidence
                // 引用台账（快照与登记哈希一致，契约面）。
                self.record_step_memory(
                    run_id,
                    round,
                    task_id,
                    &reg_out,
                    step,
                    &called.run_id,
                    &called.tool_version,
                    warnings,
                );
            }
            Err(e) => warnings.push(format!("evidence registration failed: {e}")),
        }
        self.persist_step_trajectory(run_id, seq, round, step, &record, warnings);
        record
    }

    /// 步骤轨迹落库（agent_run_step，014 迁移）：resume 状态续接的持久化
    /// 权威面；写失败降级 warning（旁路纪律与 memory_refs 同口径，轨迹
    /// 缺失时 resume 无法续接该步，如实暴露在报告 warnings）。
    fn persist_step_trajectory(
        &self,
        run_id: &RunId,
        seq: usize,
        round: usize,
        step: &PlanStep,
        record: &StepRecord,
        warnings: &mut Vec<String>,
    ) {
        if let Err(e) = self
            .state
            .record_step(run_id, seq, round, &step.input, record)
        {
            warnings.push(format!("step trajectory persist failed (seq {seq}): {e}"));
        }
    }

    /// 单步 Memory 落账（可选接线）：Task Memory 轮次载荷 + Evidence Memory
    /// 指针台账；任一失败降级 warning，不反压业务步骤。
    #[allow(clippy::too_many_arguments)]
    fn record_step_memory(
        &self,
        run_id: &RunId,
        round: usize,
        task_id: Option<&str>,
        reg_out: &crate::evidence::RegisteredOutcome,
        step: &PlanStep,
        tool_run_id: &str,
        tool_version: &str,
        warnings: &mut Vec<String>,
    ) {
        let Some(memory) = &self.memory else {
            return;
        };
        let content = json!({
            "tool_id": step.tool_id,
            "tool_version": tool_version,
            "tool_run_id": tool_run_id,
            "artifact_id": reg_out.artifact_id,
            "artifact_uri": reg_out.artifact_uri,
            "evidence_id": reg_out.evidence_id,
            "rationale": step.rationale,
        });
        let task_entry = TaskMemoryEntry::try_new(
            crate::new_id("mem-t"),
            run_id.clone(),
            task_id.map(str::to_string),
            u32::try_from(round).unwrap_or(0),
            content.to_string(),
            "",
            MemoryLifecycle::Active,
        );
        match task_entry {
            Ok(entry) => {
                if let Err(e) = memory.task().append(&entry) {
                    warnings.push(format!("task memory append failed: {e}"));
                }
            }
            Err(e) => warnings.push(format!("task memory build failed: {e}")),
        }
        let evidence_entry = EvidenceMemoryEntry::try_new(
            crate::new_id("mem-e"),
            run_id.clone(),
            reg_out.evidence_id.clone(),
            domain::ArtifactId::try_new(reg_out.artifact_id.clone()).ok(),
            reg_out.content_hash.clone(),
            reg_out.artifact_uri.clone(),
            MemoryLifecycle::Active,
        );
        match evidence_entry {
            Ok(entry) => {
                if let Err(e) = memory.evidence().record(&entry) {
                    warnings.push(format!("evidence memory record failed: {e}"));
                }
            }
            Err(e) => warnings.push(format!("evidence memory build failed: {e}")),
        }
    }

    /// 审计旁路（emit_best_effort；写失败不反压业务）。
    fn audit_emit(&self, action: &str, run_id: &str, actor: &str, detail: Value) {
        if let Some(audit) = &self.audit {
            audit.emit_best_effort(
                domain::audit::AuditEvent::new(actor, action)
                    .with_entity("agent_run", run_id)
                    .with_correlation(run_id)
                    .with_detail(detail.to_string()),
            );
        }
    }
}

/// 内置测试规划器（内置测试 Agent 的最小确定性规划面）：
/// - 有历史即收尾（成功 → 闭环完成；失败 → 重试属 RunControl::retry）；
/// - 首轮取 capability_refs ∩ Broker L2 tool_briefs 的第一个工具（上下文
///   唯一来源 Broker），入参取显式 input 或从目标文本推导首个 ASCII 词元。
pub struct KeywordSearchPlanner;

impl Planner for KeywordSearchPlanner {
    fn plan(&self, ctx: &PlanContext<'_>) -> Decision {
        if !ctx.history.is_empty() {
            if ctx.history.iter().any(|s| s.status == "SUCCEEDED") {
                return Decision::Finish {
                    reason: "单工具调用闭环完成（P7-2 缩小版验收）".to_string(),
                };
            }
            return Decision::Finish {
                reason: "首轮工具调用未成功；重试走 RunControl::retry".to_string(),
            };
        }
        let brief_ids: Vec<&str> = ctx
            .context
            .layers
            .l2
            .tool_briefs
            .iter()
            .map(|b| b.tool_id.as_str())
            .collect();
        let target = ctx
            .capability_refs
            .iter()
            .find(|r| brief_ids.contains(&r.tool_id()))
            .or_else(|| ctx.capability_refs.first());
        let Some(reference) = target else {
            return Decision::Finish {
                reason: "capability_refs 为空：无可调用工具".to_string(),
            };
        };
        let input = match ctx.explicit_input {
            Some(v) => v.clone(),
            None => {
                let keyword = derive_keyword(ctx.goal);
                if keyword.is_empty() {
                    return Decision::Finish {
                        reason:
                            "目标无可用关键词（P7-2 最小规划器推导失败，可用 --input 显式给定）"
                                .to_string(),
                    };
                }
                json!({"keyword": keyword})
            }
        };
        Decision::Steps(vec![PlanStep {
            tool_id: reference.tool_id().to_string(),
            version: reference.version().map(str::to_string),
            input,
            rationale: format!(
                "capability_refs 绑定 ∩ Broker L2 tool_briefs；入参来源：{}",
                if ctx.explicit_input.is_some() {
                    "显式 input"
                } else {
                    "目标文本推导"
                }
            ),
        }])
    }
}

/// 关键词推导（最小启发式：首个长度 ≥3 的 ASCII 词元；缺省整段截断
/// 100 字符；P7-3 起由意图槽位与模型在环驱动）。
fn derive_keyword(goal: &str) -> String {
    let token = goal.split_whitespace().find(|t| {
        t.len() >= 3
            && t.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    });
    let raw = token.unwrap_or(goal);
    raw.chars().take(100).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 关键词推导：ASCII 词元优先，CJK 整段截断，空目标拒绝面。
    #[test]
    fn keyword_derivation_heuristic() {
        assert_eq!(derive_keyword("检索 nmap 相关知识并调用登记工具"), "nmap");
        assert_eq!(derive_keyword("keyword-first another"), "keyword-first");
        assert_eq!(derive_keyword("检索金蝶相关信息"), "检索金蝶相关信息");
        assert_eq!(derive_keyword("ab cd"), "ab cd");
        assert_eq!(derive_keyword(""), "");
        let long = "x".repeat(150);
        assert_eq!(derive_keyword(&long).chars().count(), 100);
    }
}
