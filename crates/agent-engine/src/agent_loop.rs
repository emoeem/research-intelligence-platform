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
    est_tokens, AssembleRequest, AssembledContext, ContextItemKind, ContextLayer, InclusionForm,
};
use domain::{
    AgentId, AgentRun, AgentRunState, CapabilityRef, EvidenceMemoryEntry, MemoryLifecycle,
    MemoryObjectRef, ResearchMemoryEntry, RunId, TaskMemoryEntry,
};
use tool_registry::hash::sha256_hex;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;
use database::rusqlite;

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
    /// 链路追踪 ID（P7-5）：Some 时整个 run（含步骤 tool_run/artifact/
    /// evidence/audit 事件）落同一 trace_id（迁移 015 五面列）；None 时
    /// 受理面生成新 `tr-` trace（一次 run 一个新链，跨 run 关联靠 task_id
    /// 与 RunControl::retry 的 trace 继承）。
    pub trace_id: Option<String>,
    /// run id 钉定（P8-3 additive）：Some 时受理面以该 id 落行（workflow
    /// agent 节点派发前预生成并先行落节点链接列，跨进程控制传播据此发现
    /// 运行中实体）；None = 受理面生成（既有行为，零改动）。
    pub run_id: Option<String>,
    /// 显式工具入参（覆盖规划器推导；合工具 input_schema）。
    pub input: Option<Value>,
    /// caller 归属（MA-2 D4①：None = legacy/default，行为零变化——串行
    /// 路径、既有护栏、NULL 落列逐字节现状；Some 且 caller_budget 有行时
    /// 受理面跨 run 并发帽硬拒（D5.4）+ 速率/总预算闸（D5.2/D5.3）+ 并行
    /// 支路放行三与条件（D3）。caller 是运行时维度，不进任何 JSON 契约）。
    pub caller_id: Option<String>,
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
    /// 链路追踪 ID（P7-5：请求传入或受理面生成，五面落库同链）。
    pub trace_id: Option<String>,
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
    /// 上下文折叠事件留痕（CB-1 B1/B3 additive：折叠轮区间/折入步数/est
    /// 前后值/digest/确定性摘要/落库 memory_id；折叠默认关为空数组——
    /// 折叠只改规划器可见投影面，[`RunReport::steps`] 全量不缩水，红线 5）。
    pub fold_events: Vec<FoldEvent>,
}

/// 内部驱动参数（run / resume 共用执行尾；区别只在历史续接与起始轮次）。
struct DriveParams<'a> {
    run_id: &'a RunId,
    run_id_str: &'a str,
    /// 链路追踪 ID（P7-5；audit/tool_run/artifact/evidence 统一注入）。
    trace_id: Option<String>,
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
    /// 轨迹续接步轮次表（与 history 等长；CB-1 D3 轮界跨度表内建——fresh
    /// 为空，resume 由 StoredStep.round 重建，state.rs SELECT 已含 round 列）。
    history_rounds: Vec<usize>,
    /// 起始轮次（fresh run 0；resume 为轨迹最大轮次 + 1）。
    start_round: usize,
    /// 是否为恢复段（报告留痕 + 警告标注）。
    resumed: bool,
    /// 已执行调用键（重复调用检测；resume 由轨迹重建）。
    executed_keys: Vec<String>,
    /// 已见证据哈希（停滞检测；resume 由轨迹重建）。
    seen_hashes: Vec<String>,
    /// caller 归属（MA-2 D4：run 全程传递——agent_run.caller_id 落列、
    /// tool_run.caller_id 归属、预算面闸与并行支路放行判定共用；None =
    /// legacy/default 零回归路径）。
    caller_id: Option<&'a str>,
}

/// MA-2 速率窗口进程内计数复核器（D5.2：std AtomicUsize + Instant，零新
/// 依赖）。窗口过期即滑动复位；计数口径 = 本 run 已派发步数（与 027
/// tool_run.caller_id 行 COUNT 互补：DB 计数跨 run 权威、进程计数覆盖
/// fan-out 线程组内提交行尚未落库可见的窗口内漏计，取 max 复核）。
struct CallerRateTracker {
    window_ms: u64,
    window_start: Mutex<Instant>,
    steps: AtomicUsize,
}

impl CallerRateTracker {
    fn new(window_ms: u64) -> Self {
        Self {
            window_ms,
            window_start: Mutex::new(Instant::now()),
            steps: AtomicUsize::new(0),
        }
    }

    /// 当前窗口内步数（窗口过期即复位清零）。
    fn count(&self) -> usize {
        let mut start = self.window_start.lock().expect("rate tracker mutex");
        if start.elapsed().as_millis() as u64 >= self.window_ms {
            *start = Instant::now();
            self.steps.store(0, Ordering::SeqCst);
            0
        } else {
            self.steps.load(Ordering::SeqCst)
        }
    }

    /// 派发一步即计数（fan-out 派发器主线程调用）。
    fn bump(&self) {
        self.steps.fetch_add(1, Ordering::SeqCst);
    }
}

/// 并行支路单轮停止语义（drive 主循环据此收敛）。
enum ParallelStop {
    /// 控制消费（协作式取消/暂停）：跃迁已在消费点完成，drive 收敛到该态。
    Control(AgentRunState),
    /// 护栏/预算硬停（finish_reason 已定，审计已落）。
    Reason(String),
}

/// 并行支路单轮产出（drive 主循环合并进既有轮变量）。
struct ParallelRoundOut {
    executed_in_round: usize,
    duplicates_in_round: usize,
    any_failed: bool,
    stop: Option<ParallelStop>,
}

/// fan-out 子步共享依赖（scoped threads 借用；各字段 Sync——ToolLink
/// Engine/驱动与 EvidenceRegistrar 均为 Pool/纯数据持有，写面经 Pool
/// 串行化，node_parallel.rs 先例同源）。
struct ParallelDeps<'a> {
    tool: &'a ToolLink,
    evidence: &'a EvidenceRegistrar,
    run_id: &'a RunId,
    agent_id: &'a str,
    trace_id: Option<&'a str>,
    task_id: Option<&'a str>,
    caller_id: Option<&'a str>,
    workspace_id: &'a str,
}

/// fan-out 派发队列形状：共享 VecDeque（None = 关闭毒丸）+ Condvar
/// （空队列等待派发；worker 退出只认毒丸，防派发间隙误判关闭提前退出）。
type DispatchQueue<T> = (Mutex<VecDeque<Option<T>>>, Condvar);

/// fan-out 子步产出（主线程收齐终态后按 plan 顺序统一落：D3 收敛语义）。
struct ParallelStepOut {
    /// plan 内派发序（确定性 seq = steps.len() + ordinal）。
    ordinal: usize,
    /// 预派发 seq（UNIQUE(run_id, seq) 确定性，014:99）。
    seq: usize,
    record: StepRecord,
    /// Evidence 登记产出（主线程 memory 记账用；登记失败为 None + warning）。
    reg: Option<crate::evidence::RegisteredOutcome>,
    /// 子线程侧降级 warning（evidence 登记失败等）。
    warnings: Vec<String>,
}

// ==== CB-1 上下文折叠面（B1/B3；docs/rip3/CB-1-TASK.md D1-D3/D6）====
//
// 折叠语义只对「发给规划器的」生效（D3：PlanContext 投影），steps 权威
// 面全量逐字节（红线 5）；折叠产物落 agent_task_memory（013 记忆面既有
// 表，D1 零迁移）；计量全走 context_broker::est_tokens 单点（D6）；折叠
// 函数纯确定性——同转录前缀 → 同折叠视图/digest（D1，resume 由重算兜底）。

/// 折叠合成行 tool_id（CB-1 D3 词形；`transcript-fold` 词形自别——
/// compress/压缩 词形被 013 memory 面占用（A13），折叠面不复用）。
pub const FOLD_TOOL_ID: &str = "transcript:fold";

/// 折叠合成行 tool_version（CB-1 D3 折叠版本常量；折叠纯函数语义变更时
/// 递增，重算对账随版本对齐）。
pub const TRANSCRIPT_FOLD_VERSION: &str = "1";

/// 折叠确定性摘要词形前缀（CB-1 D1：无时间戳无进程内计数，同转录前缀 →
/// 同摘要；run-folds 回查与 resume 漂移检测按此前缀识别折叠行）。
pub const FOLD_SUMMARY_PREFIX: &str = "transcript-fold:";

/// 折叠事件留痕（CB-1 B1/B3 additive；RunReport.fold_events 元素，折叠行
/// content_json 出处载荷与摘要词形同源数据面——Phase 6 检索面折叠摘要
/// 引用输入，§8 前置输出①）。
#[derive(Debug, Clone, Serialize)]
pub struct FoldEvent {
    /// 折叠覆盖轮区间（含端点 [start, end]）。
    pub rounds: [usize; 2],
    /// 折入步数（被折叠的原始 StepRecord 条数）。
    pub steps_folded: usize,
    /// 折叠前段 est_tokens（context_broker::est_tokens 单点计量，D6）。
    pub est_before: u64,
    /// 折叠后合成行 est_tokens（摘要 sizing 同源计量）。
    pub est_after: u64,
    /// 折前段 canonical JSON 的 sha256（确定性 digest；fresh 折叠与 resume
    /// 重算对账锚点，判据 2）。
    pub digest: String,
    /// 确定性摘要词形（`transcript-fold:` 前缀自别）。
    pub summary: String,
    /// 落 agent_task_memory 的折叠行 memory_id（未挂折叠落库面或落库失败
    /// 为 None；回查走 rp-agent run-folds）。
    pub memory_id: Option<String>,
}

/// 已应用折叠（drive 内部载具：FoldEvent 报告/落库载荷 + 投影合成行）。
struct AppliedFold {
    event: FoldEvent,
    synthetic: StepRecord,
}

/// 折叠覆盖段产出（纯函数输出）。
struct FoldSegment {
    /// 覆盖轮区间（含端点）。
    rounds: (usize, usize),
    steps_folded: usize,
    est_before: u64,
    est_after: u64,
    digest: String,
    summary: String,
    synthetic: StepRecord,
}

/// 折叠纯函数（CB-1 D1/D3/D6）：对 [a, b] 轮区间内的步骤段构造确定性折叠
/// 产出——同转录前缀 → 同摘要/digest/est 前后值（无时间戳无进程内计数）。
/// 折前段 canonical JSON = serde_json 规范序列化（Value 默认 BTreeMap 键
/// 序纪律同 canonical_call_key），sha256 走 tool-registry::hash 既有依赖
/// （D7 零新依赖）。区间内无步骤返回 None。
fn fold_segment(
    steps: &[StepRecord],
    rounds: &[usize],
    a: usize,
    b: usize,
) -> Option<FoldSegment> {
    debug_assert_eq!(steps.len(), rounds.len(), "fold span table parity");
    let mut segment: Vec<StepRecord> = Vec::new();
    for (i, record) in steps.iter().enumerate() {
        if rounds[i] >= a && rounds[i] <= b {
            segment.push(record.clone());
        }
    }
    if segment.is_empty() {
        return None;
    }
    // est_before：折前段序列化 est（est_tokens 单点；tokenizer 在位/缺席
    // 只影响数值不改变触发判定结构，D6）。
    let canonical = serde_json::to_string(&segment).unwrap_or_else(|_| "[]".to_string());
    let digest = sha256_hex(canonical.as_bytes());
    let est_before = est_tokens(&canonical);
    // 确定性摘要词形（D1）：tools = 折入步去重排序 tool_id 集；
    // artifacts = 折入步登记 artifact 计数。
    let mut tools: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for record in &segment {
        tools.insert(record.tool_id.as_str());
    }
    let artifacts = segment
        .iter()
        .filter(|s| s.artifact_id.as_deref().is_some_and(|a| !a.is_empty()))
        .count();
    let summary = format!(
        "{FOLD_SUMMARY_PREFIX}rounds={a}-{b};steps={};tools={{{}}};artifacts={{{}}}",
        segment.len(),
        tools.into_iter().collect::<Vec<&str>>().join(","),
        artifacts
    );
    let est_after = est_tokens(&summary);
    let synthetic = StepRecord {
        tool_id: FOLD_TOOL_ID.to_string(),
        tool_version: TRANSCRIPT_FOLD_VERSION.to_string(),
        tool_run_id: String::new(),
        // ToolRunStatus 词形内（D3：合成行词形全部落在既有词表内）。
        status: "SUCCEEDED".to_string(),
        artifact_id: None,
        evidence_id: None,
        content_hash: None,
        summary: Some(summary.clone()),
        error: None,
        elapsed_ms: 0,
    };
    Some(FoldSegment {
        rounds: (a, b),
        steps_folded: segment.len(),
        est_before,
        est_after,
        digest,
        summary,
        synthetic,
    })
}

/// 折叠投影（纯函数，CB-1 D3）：已折叠段以每段一条合成行原位替代，未折叠
/// 段原样透传；无折叠事件返回 None（调用方直用全量切片——默认关路径零
/// 分配零行为差）。`rounds` 与 `steps` 等长（轮界跨度表内建，D3）。
fn fold_projection(
    steps: &[StepRecord],
    rounds: &[usize],
    folds: &[AppliedFold],
) -> Option<Vec<StepRecord>> {
    debug_assert_eq!(steps.len(), rounds.len(), "fold span table parity");
    if folds.is_empty() {
        return None;
    }
    let mut out: Vec<StepRecord> = Vec::with_capacity(steps.len());
    let mut ei = 0usize;
    let mut i = 0usize;
    while i < steps.len() {
        if ei < folds.len() && rounds[i] == folds[ei].event.rounds[0] {
            out.push(folds[ei].synthetic.clone());
            let b = folds[ei].event.rounds[1];
            while i < steps.len() && rounds[i] <= b && rounds[i] >= folds[ei].event.rounds[0] {
                i += 1;
            }
            ei += 1;
            continue;
        }
        out.push(steps[i].clone());
        i += 1;
    }
    // 残余事件（覆盖轮区间在轨迹中无对应步骤的异常形态）按序补尾合成行，
    // 保证投影与 fold_events 事件数一致（不丢事件）。
    while ei < folds.len() {
        out.push(folds[ei].synthetic.clone());
        ei += 1;
    }
    Some(out)
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
    /// 折叠行落库面（CB-1 D1：TaskMemoryStore 直连 013 agent_task_memory；
    /// None = 折叠只投影不落库。与 [`Self::with_memory`] 三类记忆接线相互
    /// 独立——折叠开启不隐式开启步骤记忆）。
    fold_memory: Option<crate::memory::TaskMemoryStore>,
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
            fold_memory: None,
        }
    }

    /// 覆盖运行控制参数（P7-4 九类护栏；缺省即低并发纪律最严面）。
    pub fn with_control(mut self, control: RunControlConfig) -> Self {
        self.control = control;
        self
    }

    /// 挂折叠行落库面（CB-1 D1：折叠产物经 TaskMemoryStore::append 落
    /// agent_task_memory 013 既有表；None = 折叠只投影不落库，回查面
    /// run-folds 将如实输出空数组）。
    pub fn with_fold_memory(mut self, fold_memory: crate::memory::TaskMemoryStore) -> Self {
        self.fold_memory = Some(fold_memory);
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
        // trace_id（P7-5）：请求传入或受理面生成（domain::TraceId 校验），
        // agent_run 行随 create_run 落关联列（迁移 015）。
        let trace_id = match &req.trace_id {
            Some(t) => {
                let validated = domain::TraceId::try_new(t.clone())?;
                validated.into_inner()
            }
            None => crate::new_id("tr"),
        };
        let run_id = match &req.run_id {
            Some(pinned) => RunId::try_new(pinned.clone())?,
            None => AgentStore::new_run_id(),
        };
        // MA-2（D5.4）受理面跨 run 并发帽：caller 有 budget 行且
        // max_concurrent_runs > 0 时，名下在飞 run 数（QUEUED/RUNNING 记账，
        // D1 公式）已达帽即 BudgetDenied 硬拒——run 不落任何行（create_run
        // 前置）。
        if let Some(caller) = req.caller_id.as_deref() {
            if let Some(budget) = self.control.caller_budgets.get(caller) {
                if budget.max_concurrent_runs > 0 {
                    let active = self.store.count_active_runs_by_caller(caller)?;
                    if active >= budget.max_concurrent_runs {
                        return Err(AgentError::BudgetDenied(format!(
                            "caller {caller} concurrent runs {active} at cap {} (max_concurrent_runs); new run rejected before create_run",
                            budget.max_concurrent_runs
                        )));
                    }
                }
            }
        }
        let run = AgentRun::try_new(
            run_id.clone(),
            req.task_id.clone(),
            stored.definition.agent_id().clone(),
            stored.definition.version().to_string(),
            AgentRunState::Queued,
            vec![],
        )?;
        self.store
            .create_run(&run, Some(&trace_id), req.caller_id.as_deref())?;
        let run_id_str = run_id.as_str().to_string();
        let actor = format!("agent-engine:{agent_id}");
        self.state.ensure_control_row(&run_id_str, &req.goal)?;
        // 受理留痕：caller 归属仅在 Some 时进 payload（None = legacy 行
        // 逐字节现状，不添键）。
        let mut accepted_detail = json!({"agent_id": agent_id, "goal": req.goal, "trace_id": trace_id});
        if let Some(caller) = req.caller_id.as_deref() {
            accepted_detail["caller_id"] = json!(caller);
        }
        self.audit_emit(
            "agent.run.accepted",
            &run_id_str,
            Some(&trace_id),
            &actor,
            accepted_detail,
        );
        self.store
            .transition_run(&run_id_str, AgentRunState::Running)?;

        self.drive(DriveParams {
            run_id: &run_id,
            run_id_str: &run_id_str,
            trace_id: Some(trace_id),
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
            history_rounds: Vec::new(),
            start_round: 0,
            resumed: false,
            executed_keys: Vec::new(),
            seen_hashes: Vec::new(),
            caller_id: req.caller_id.as_deref(),
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
        // P7-5：恢复段沿用源 run 的 trace_id（agent_run 行关联列）。
        let trace_id = stored.trace_id.clone();
        self.audit_emit(
            control::actions::RESUMED,
            &req.run_id,
            trace_id.as_deref(),
            &actor,
            json!({"replayed_steps": persisted.len()}),
        );

        // 轨迹续接：历史步骤 + 重复调用键 + 停滞已见哈希；轮次续接。
        let history: Vec<StepRecord> = persisted.iter().map(|s| s.record.clone()).collect();
        // CB-1（D3）：轮界跨度表由 StoredStep.round 重建（与 history 等长）。
        let history_rounds: Vec<usize> = persisted.iter().map(|s| s.round).collect();
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
            trace_id,
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
            history_rounds,
            start_round,
            resumed: true,
            executed_keys,
            seen_hashes,
            // MA-2：resume 沿用源 run 的 caller 归属（agent_run 行读列，
            // 与 trace_id 续接同口径）。
            caller_id: stored.caller_id.as_deref(),
        })
    }

    /// 执行尾（run / resume 共用）：装配 → 展开 → 规划执行（步间控制消费
    /// + 九类护栏）→ 终态落库 → 报告。
    ///
    /// MA-2：caller 预算面（D5）——有 budget 行时每轮前过 caller 总预算闸
    /// 与速率闸；并行支路（D3）按放行三与条件在单轮内 fork-join（串行路径
    /// 逻辑零改动）。
    fn drive(&self, p: DriveParams<'_>) -> Result<RunReport, AgentError> {
        let started_at = std::time::Instant::now();
        let cfg = self.control.clone();

        // MA-2（D7/D2）：caller 预算面解析（装配层内存映射查行；缺行 =
        // 现状零回归：并行关、帽不限）+ 速率窗口进程内计数复核器
        // （std AtomicUsize + Instant，零新依赖；窗口过期即滑动复位）。
        let caller_budget: Option<&crate::control::CallerBudget> = p
            .caller_id
            .and_then(|c| cfg.caller_budgets.get(c));
        let rate_tracker = caller_budget
            .filter(|b| b.rate_max_steps > 0)
            .map(|b| CallerRateTracker::new(b.rate_window_ms));

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
                trace_id: p.trace_id.clone(),
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

        // CB-1（D2/D3）折叠状态：轮界跨度表内建（fresh 空表起步，随步执行
        // 记录；resume 由 StoredStep.round 重建）+ 事件列 + 触发基线。折叠
        // 默认关路径：跨度表记录为旁路零行为影响，事件列恒空，投影恒 = 全量
        // 切片（红线 4/5）。
        let fold_enabled = cfg.fold.enabled;
        let mut step_rounds: Vec<usize> = p.history_rounds.clone();
        let mut applied_folds: Vec<AppliedFold> = Vec::new();
        let mut fold_est_baseline: u64 =
            est_tokens(&serde_json::to_string(&steps).unwrap_or_else(|_| String::new()));
        // 上次折叠覆盖末轮（无折叠 = 0；触发防抖轮距与热窗下界共同依据）。
        let mut fold_boundary: usize = 0;
        // 下一段未折叠轮起点（折叠只从最旧未折段向前推进，不重折）。
        let mut fold_next_start: usize = 0;

        // CB-1（D1/D3）resume 折叠重算 + 漂移检测：持久化折叠行（
        // agent_task_memory，`transcript-fold:` 词形识别）覆盖轮区间重算
        // digest 对账——一致则原样续接，不一致 warning 留痕且重算视图胜出
        // （D1「resume 由重算兜底、持久化行作回查与漂移检测面」；判据 2
        // 漂移检测负例可触发面）。折叠关闭时不重算不检测（全量直投）。
        if p.resumed && fold_enabled {
            if let Some(fold_mem) = &self.fold_memory {
                match fold_mem.list_by_run(p.run_id_str) {
                    Ok(rows) => {
                        for row in rows
                            .iter()
                            .filter(|r| r.entry.summary().starts_with(FOLD_SUMMARY_PREFIX))
                        {
                            let payload: Value = serde_json::from_str(row.entry.content_json())
                                .unwrap_or(Value::Null);
                            let a = payload
                                .get("rounds")
                                .and_then(|r| r.get(0))
                                .and_then(Value::as_u64)
                                .map(|v| usize::try_from(v).unwrap_or(0));
                            let b = payload
                                .get("rounds")
                                .and_then(|r| r.get(1))
                                .and_then(Value::as_u64)
                                .map(|v| usize::try_from(v).unwrap_or(0));
                            let persisted_digest =
                                payload.get("digest").and_then(Value::as_str).unwrap_or("");
                            match (a, b) {
                                (Some(a), Some(b)) => match fold_segment(&steps, &step_rounds, a, b)
                                {
                                    Some(seg) => {
                                        if seg.digest != persisted_digest {
                                            warnings.push(format!(
                                                "transcript fold drift detected on resume: rounds {a}-{b} recomputed digest {} != persisted {persisted_digest} (recomputed view wins; persisted row kept for audit)",
                                                seg.digest
                                            ));
                                        }
                                        fold_boundary = fold_boundary.max(b);
                                        fold_next_start = fold_next_start.max(b.saturating_add(1));
                                        applied_folds.push(AppliedFold {
                                            event: FoldEvent {
                                                rounds: [a, b],
                                                steps_folded: seg.steps_folded,
                                                est_before: seg.est_before,
                                                est_after: seg.est_after,
                                                digest: seg.digest,
                                                summary: seg.summary,
                                                memory_id: Some(
                                                    row.entry.memory_id().to_string(),
                                                ),
                                            },
                                            synthetic: seg.synthetic,
                                        });
                                    }
                                    None => warnings.push(format!(
                                        "transcript fold replay skipped: rounds {a}-{b} has no steps in persisted trajectory (row {})",
                                        row.entry.memory_id()
                                    )),
                                },
                                _ => warnings.push(format!(
                                    "transcript fold replay skipped: row {} has unparsable rounds payload",
                                    row.entry.memory_id()
                                )),
                            }
                        }
                    }
                    Err(e) => {
                        warnings.push(format!("transcript fold replay read failed: {e}"));
                    }
                }
            }
        }

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
                    p.trace_id.as_deref(),
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
                    p.trace_id.as_deref(),
                    p.actor,
                    json!({"before_step": steps.len(), "round": round}),
                );
                break 'rounds;
            }
            // CB-1（D2）轮界两级触发：增长阈值软收缩 + 轮次间隔防抖（控制
            // 消费之后、护栏判定之前）。触发即对热窗外最旧未折段软折叠——
            // 只改规划器可见投影（红线 5：steps 权威面零改动，预算记账数
            // 真实步数，折叠不放宽任何护栏）。默认关路径本块零执行。
            if fold_enabled && round > 0 {
                let est_now =
                    est_tokens(&serde_json::to_string(&steps).unwrap_or_else(|_| String::new()));
                let growth = est_now.saturating_sub(fold_est_baseline);
                let interval_ok =
                    round.saturating_sub(fold_boundary) >= cfg.fold.fold_min_interval_rounds;
                if growth >= cfg.fold.fold_growth_tokens && interval_ok {
                    // 热窗：最近 N 轮永不折叠（可折轮 < round - N）。
                    let hot_floor = round.saturating_sub(cfg.fold.fold_hot_window_rounds);
                    if fold_next_start < hot_floor {
                        if let Some(seg) =
                            fold_segment(&steps, &step_rounds, fold_next_start, hot_floor - 1)
                        {
                            let memory_id = self.persist_fold_row(&p, &seg, &mut warnings);
                            fold_boundary = seg.rounds.1;
                            fold_next_start = seg.rounds.1.saturating_add(1);
                            fold_est_baseline = est_now;
                            self.audit_emit(
                                control::actions::TRANSCRIPT_FOLD,
                                p.run_id_str,
                                p.trace_id.as_deref(),
                                p.actor,
                                json!({
                                    "rounds": [seg.rounds.0, seg.rounds.1],
                                    "steps_folded": seg.steps_folded,
                                    "est_before": seg.est_before,
                                    "est_after": seg.est_after,
                                    "digest": seg.digest,
                                    "memory_id": memory_id,
                                }),
                            );
                            applied_folds.push(AppliedFold {
                                event: FoldEvent {
                                    rounds: [seg.rounds.0, seg.rounds.1],
                                    steps_folded: seg.steps_folded,
                                    est_before: seg.est_before,
                                    est_after: seg.est_after,
                                    digest: seg.digest,
                                    summary: seg.summary,
                                    memory_id,
                                },
                                synthetic: seg.synthetic,
                            });
                        }
                    }
                }
            }
            // 预算：轮次上限。
            if cfg.max_rounds > 0 && round >= cfg.max_rounds {
                finish_reason = format!("guardrail: budget max_rounds={} reached", cfg.max_rounds);
                self.audit_emit(
                    control::actions::GUARDRAIL,
                    p.run_id_str,
                    p.trace_id.as_deref(),
                    p.actor,
                    json!({"kind": "budget_rounds", "round": round, "limit": cfg.max_rounds}),
                );
                break 'rounds;
            }
            // MA-2（D5.3）caller 总预算闸：caller 级单 run 总步数帽（0 =
            // 沿用 LoopConfig::max_tool_calls 既有）；已执行步数到位即整轮
            // 硬停（规划步数不执行，超帽步骤不进入本轮）。
            if let Some(budget) = caller_budget {
                if budget.budget_max_steps_per_run > 0
                    && steps.len() >= budget.budget_max_steps_per_run
                {
                    finish_reason = format!(
                        "guardrail: caller total budget (budget_max_steps_per_run={}) reached",
                        budget.budget_max_steps_per_run
                    );
                    self.audit_emit(
                        control::actions::GUARDRAIL,
                        p.run_id_str,
                        p.trace_id.as_deref(),
                        p.actor,
                        json!({"kind": "budget_caller_total", "caller_id": budget.caller_id,
                               "limit": budget.budget_max_steps_per_run, "steps_done": steps.len()}),
                    );
                    break 'rounds;
                }
            }
            // MA-2（D5.2）caller 速率闸：窗口（rate_window_ms）内该 caller
            // 步数 ≥ rate_max_steps 即 fan-out/单步前整轮硬停，超帽步骤不
            // 执行。窗口计数 = tool_run.caller_id 行 COUNT（027 数据源）+
            // 进程内计数复核（AtomicUsize + Instant，取两者较大值防并发
            // 提交行尚未可见的窗口内漏计）。
            if let (Some(budget), Some(tracker)) = (caller_budget, rate_tracker.as_ref()) {
                if budget.rate_max_steps > 0 {
                    let db_count =
                        self.count_caller_window_steps(&budget.caller_id, budget.rate_window_ms)?;
                    let effective = db_count.max(tracker.count());
                    if effective >= budget.rate_max_steps {
                        finish_reason = "guardrail: caller rate budget".to_string();
                        self.audit_emit(
                            control::actions::GUARDRAIL,
                            p.run_id_str,
                            p.trace_id.as_deref(),
                            p.actor,
                            json!({"kind": "budget_caller_rate", "caller_id": budget.caller_id,
                                   "window_ms": budget.rate_window_ms,
                                   "limit": budget.rate_max_steps,
                                   "window_steps": db_count,
                                   "proc_window_steps": tracker.count()}),
                        );
                        break 'rounds;
                    }
                }
            }
            // 预算：步数上限（P7-2 既有护栏）。
            if steps.len() >= self.config.max_tool_calls {
                finish_reason = format!(
                    "guardrail: max_tool_calls={} reached",
                    self.config.max_tool_calls
                );
                break 'rounds;
            }
            // CB-1（D3）规划器可见投影：已有折叠事件时传折叠视图（已折段
            // 每段一条合成行原位替代），否则全量切片（默认关路径逐字节现状
            // ——无折叠事件返回 None 直用 &steps）。
            let fold_view: Vec<StepRecord>;
            let history_ref: &[StepRecord] = match fold_projection(&steps, &step_rounds, &applied_folds)
            {
                Some(view) => {
                    fold_view = view;
                    &fold_view
                }
                None => &steps,
            };
            let ctx = PlanContext {
                goal: &p.goal,
                run_id: p.run_id_str,
                context: &context,
                capability_refs: p.capability_refs,
                history: history_ref,
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
                    // MA-2（D3）并行支路放行三与条件：allow_parallel_subtasks
                    // && caller_id=Some && caller_budget.max_parallel_subtasks
                    // ≥ 2。任一不满足走既有串行路径（含既有 warning 留痕）；
                    // 单轮单步骤恒走串行（无 fork 收益，路径零 diff）。
                    let parallel_ready = cfg.allow_parallel_subtasks
                        && p.caller_id.is_some()
                        && caller_budget
                            .map(|b| b.max_parallel_subtasks >= 2)
                            .unwrap_or(false);
                    if plan_steps.len() > 1 && !parallel_ready {
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
                    if parallel_ready && plan_steps.len() > 1 {
                        // ---- MA-2 并行支路（fork-join，node_parallel.rs 语义
                        // 先例：声明面并发度、引擎不放大、写面经 Pool 串行化、
                        // 确定性顺序合并）----
                        let budget = caller_budget.expect("budget present when parallel_ready");
                        let parallelism = plan_steps
                            .len()
                            .min(budget.max_parallel_subtasks)
                            .max(1);
                        let round_out = self.run_round_parallel(
                            &p,
                            round,
                            &plan_steps,
                            parallelism,
                            budget,
                            rate_tracker.as_ref(),
                            &surface,
                            &context,
                            started_at,
                            &mut steps,
                            &mut step_rounds,
                            &mut executed_keys,
                            &mut seen_hashes,
                            &mut warnings,
                            &mut no_progress_streak,
                            &mut drift_streak,
                        )?;
                        executed_in_round = round_out.executed_in_round;
                        duplicates_in_round = round_out.duplicates_in_round;
                        any_failed = any_failed || round_out.any_failed;
                        match round_out.stop {
                            Some(ParallelStop::Control(state)) => {
                                control_state = Some(state);
                                break 'rounds;
                            }
                            Some(ParallelStop::Reason(reason)) => {
                                finish_reason = reason;
                                break 'rounds;
                            }
                            None => {}
                        }
                    } else {
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
            p.trace_id.as_deref(),
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
                                p.trace_id.as_deref(),
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
                                p.trace_id.as_deref(),
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
            p.trace_id.as_deref(),
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
                                p.trace_id.as_deref(),
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
                            p.trace_id.as_deref(),
                            p.agent_id,
                            &step,
                            p.task_id.as_deref(),
                            p.caller_id,
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
                        // CB-1（D3）轮界跨度表记录（旁路；折叠默认关零行为影响）。
                        step_rounds.push(round);
                        self.audit_emit(
                            "agent.step.completed",
                            p.run_id_str,
                            p.trace_id.as_deref(),
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
                                p.trace_id.as_deref(),
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
                            p.trace_id.as_deref(),
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
                p.trace_id.as_deref(),
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
            trace_id: p.trace_id.clone(),
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
            fold_events: applied_folds.iter().map(|f| f.event.clone()).collect(),
        })
    }

    /// 折叠行落 agent_task_memory（CB-1 D1：013 记忆面既有表零迁移；memory_id
    /// 沿 `mem-t-` 生成器，round = 折叠覆盖末轮，content_json = 折叠出处载荷，
    /// summary = 确定性摘要词形）。未挂折叠落库面或写失败返回 None + 降级
    /// warning 不炸 run（record_step_memory 降级先例同款）。
    fn persist_fold_row(
        &self,
        p: &DriveParams<'_>,
        seg: &FoldSegment,
        warnings: &mut Vec<String>,
    ) -> Option<String> {
        let fold_mem = self.fold_memory.as_ref()?;
        let content = json!({
            "kind": "transcript-fold",
            "rounds": [seg.rounds.0, seg.rounds.1],
            "steps_folded": seg.steps_folded,
            "est_before": seg.est_before,
            "est_after": seg.est_after,
            "digest": seg.digest,
        });
        let entry = TaskMemoryEntry::try_new(
            crate::new_id("mem-t"),
            p.run_id.clone(),
            p.task_id.clone(),
            u32::try_from(seg.rounds.1).unwrap_or(0),
            content.to_string(),
            seg.summary.clone(),
            MemoryLifecycle::Active,
        );
        match entry {
            Ok(entry) => match fold_mem.append(&entry) {
                Ok(()) => Some(entry.memory_id().to_string()),
                Err(e) => {
                    warnings.push(format!("transcript fold memory append failed: {e}"));
                    None
                }
            },
            Err(e) => {
                warnings.push(format!("transcript fold memory build failed: {e}"));
                None
            }
        }
    }

    /// MA-2（D5.2）速率窗口计数：tool_run.caller_id 行 COUNT（027 数据源），
    /// 窗口 = [now - rate_window_ms, now]，created_at 与窗口下界均为秒级
    /// ISO8601（domain::time 口径），字典序可比。
    fn count_caller_window_steps(
        &self,
        caller_id: &str,
        window_ms: u64,
    ) -> Result<usize, AgentError> {
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let cutoff = domain::time::format_epoch_secs(now_secs.saturating_sub(window_ms / 1000));
        let count: i64 = self
            .store
            .pool()
            .with_conn(|c| {
                c.query_row(
                    "SELECT COUNT(*) FROM tool_run WHERE caller_id = ?1 AND created_at >= ?2",
                    rusqlite::params![caller_id, cutoff],
                    |r| r.get(0),
                )
            })
            .map_err(AgentError::from)?;
        Ok(usize::try_from(count).unwrap_or(0))
    }

    /// MA-2（D3/D5/D8）并行支路单轮执行器：fork-join（node_parallel.rs 语义
    /// 先例——声明面并发度 = min(plan_steps.len(), max_parallel_subtasks)，
    /// 帽以上排队不并行非拒绝；每子步独立 tool_run/agent_run_step 行；写面
    /// 经 Pool 串行化；结果按 plan 顺序确定性合并）。每步仍过既有步闸：
    /// max_tool_calls / 能力绑定闸 / 重复检测 / 目标偏移（容限内放行留痕）
    /// / 墙钟 / 速率复核（派发间隙）；cancel/pause 在 fan-out 前与派发间隙
    /// 消费；任一子步失败按既有 fail-fast 收敛，主线程收齐终态后统一落
    /// agent_run_step（seq = steps.len() + ordinal，UNIQUE(run_id, seq)
    /// 确定性）与审计。
    #[allow(clippy::too_many_arguments)]
    fn run_round_parallel(
        &self,
        p: &DriveParams<'_>,
        round: usize,
        plan_steps: &[PlanStep],
        parallelism: usize,
        budget: &crate::control::CallerBudget,
        rate_tracker: Option<&CallerRateTracker>,
        surface: &Option<Vec<String>>,
        context: &AssembledContext,
        started_at: Instant,
        steps: &mut Vec<StepRecord>,
        step_rounds: &mut Vec<usize>,
        executed_keys: &mut Vec<String>,
        seen_hashes: &mut Vec<String>,
        warnings: &mut Vec<String>,
        no_progress_streak: &mut usize,
        drift_streak: &mut usize,
    ) -> Result<ParallelRoundOut, AgentError> {
        let cfg = self.control.clone();
        let mut duplicates_in_round = 0usize;
        let mut executed_keys_in_round: Vec<String> = Vec::new();

        // ---- 阶段 A：派发过滤（主线程 plan 顺序，确定性）——既有步闸镜像。
        // max_tool_calls / 能力绑定 / 偏移 / 重复在派发前判定；硬停即整轮
        // 收敛（不再派发，超帽步骤不执行）。(reason, any_failed) 元组：
        // 能力绑定闸拒绝 = fail-fast（any_failed 语义）。
        let mut filter_stop: Option<(String, bool)> = None;
        let mut dispatch: Vec<(usize, &PlanStep, String)> = Vec::new(); // (plan 序, step, call_key)
        for (plan_i, step) in plan_steps.iter().enumerate() {
            // 步数预算（串行路径同口径：步数到位即停，超帽步骤不执行）。
            if steps.len() + dispatch.len() >= self.config.max_tool_calls {
                filter_stop = Some((
                    format!(
                        "guardrail: max_tool_calls={} reached",
                        self.config.max_tool_calls
                    ),
                    false,
                ));
                break;
            }
            // 能力绑定闸：绑定外工具拒绝（防规划器漂移出能力面）。
            if !p
                .capability_refs
                .iter()
                .any(|r| r.tool_id() == step.tool_id)
            {
                filter_stop = Some((
                    format!(
                        "guardrail: plan step tool {:?} is not in capability_refs",
                        step.tool_id
                    ),
                    true,
                ));
                break;
            }
            // 目标偏移检测：初始计划面外跳转计偏移事件，连续偏移超过容限
            // 即停（容限内放行并留痕；跨轮 streak 与串行路径共用）。
            let surface_ref = surface
                .as_deref()
                .expect("surface captured before step dispatch");
            if control::drift_step(surface_ref, &step.tool_id) {
                *drift_streak += 1;
                if *drift_streak > cfg.drift_tolerance {
                    let reason = format!(
                        "guardrail: goal drift detected (tool {:?} outside initial plan surface, {drift_streak} consecutive drifted steps)",
                        step.tool_id
                    );
                    self.audit_emit(
                        control::actions::GUARDRAIL,
                        p.run_id_str,
                        p.trace_id.as_deref(),
                        p.actor,
                        json!({"kind": "goal_drift", "tool_id": step.tool_id,
                               "drift_streak": drift_streak, "tolerance": cfg.drift_tolerance}),
                    );
                    filter_stop = Some((reason, false));
                    break;
                }
                warnings.push(format!(
                    "goal drift event: tool {:?} outside initial plan surface (round {round}); within tolerance {}",
                    step.tool_id, cfg.drift_tolerance
                ));
            } else {
                *drift_streak = 0;
            }
            // 重复调用检测：(tool_id, 规范化 input) 已执行即跳过（含同轮
            // 派发集合内重复，串行路径 executed_keys 递增语义同源）。
            let call_key = control::canonical_call_key(&step.tool_id, &step.input);
            if executed_keys.iter().any(|k| k == &call_key)
                || executed_keys_in_round.iter().any(|k| k == &call_key)
            {
                duplicates_in_round += 1;
                warnings.push(format!(
                    "duplicate call detected: tool {:?} with identical canonical input already executed; skipped",
                    step.tool_id
                ));
                self.audit_emit(
                    control::actions::STEP_DUPLICATE,
                    p.run_id_str,
                    p.trace_id.as_deref(),
                    p.actor,
                    json!({"tool_id": step.tool_id, "round": round}),
                );
                continue;
            }
            executed_keys_in_round.push(call_key.clone());
            dispatch.push((plan_i, step, call_key));
        }
        if let Some((reason, failed)) = &filter_stop {
            return Ok(ParallelRoundOut {
                executed_in_round: 0,
                duplicates_in_round,
                any_failed: *failed,
                stop: Some(ParallelStop::Reason(reason.clone())),
            });
        }

        // ---- 阶段 B：R5 预检面（D8）+ fan-out 派发（帽以上排队）。
        let db_window_count =
            self.count_caller_window_steps(&budget.caller_id, budget.rate_window_ms)?;
        let rate_limit = if budget.rate_max_steps > 0 {
            Some(budget.rate_max_steps)
        } else {
            None
        };
        let rate_remaining = rate_limit.map(|limit| {
            limit.saturating_sub(db_window_count.max(rate_tracker.map(|t| t.count()).unwrap_or(0)))
        });
        // 预估留痕（实测数据非提示）：warnings + audit_event（D8）。
        warnings.push(format!(
            "parallel fan-out plan: {} tasks, concurrency cap {}, rate window remaining {} steps (caller {}, round {round})",
            dispatch.len(),
            parallelism,
            rate_remaining
                .map(|r| r.to_string())
                .unwrap_or_else(|| "unlimited".to_string()),
            budget.caller_id,
        ));
        self.audit_emit(
            control::actions::PARALLEL_PLAN,
            p.run_id_str,
            p.trace_id.as_deref(),
            p.actor,
            json!({"tasks": dispatch.len(), "cap": parallelism,
                   "rate_window_remaining": rate_remaining,
                   "caller_id": budget.caller_id, "round": round}),
        );
        // 帽以上排队（帽语义非拒绝）：GUARDRAIL kind=budget_caller_parallel。
        if dispatch.len() > parallelism {
            self.audit_emit(
                control::actions::GUARDRAIL,
                p.run_id_str,
                p.trace_id.as_deref(),
                p.actor,
                json!({"kind": "budget_caller_parallel", "tasks": dispatch.len(),
                       "cap": parallelism, "queued": dispatch.len() - parallelism,
                       "caller_id": budget.caller_id}),
            );
        }

        let deps = ParallelDeps {
            tool: &self.tool,
            evidence: &self.evidence,
            run_id: p.run_id,
            agent_id: p.agent_id,
            trace_id: p.trace_id.as_deref(),
            task_id: p.task_id.as_deref(),
            caller_id: p.caller_id,
            workspace_id: &self.config.workspace_id,
        };
        let deps_ref = &deps;
        // 派发队列（共享 deque + Condvar：帽以上步骤排队不并行——并发度 =
        // worker 池大小 = parallelism，队列深度即排队面；Option None =
        // 关闭毒丸；空队列 ≠ 关闭——worker 在 Condvar 等待派发，防派发间隙
        // 误判关闭提前退出）。
        let queue: Arc<DispatchQueue<(usize, usize, PlanStep)>> =
            Arc::new((Mutex::new(VecDeque::new()), Condvar::new()));
        let (out_tx, out_rx) = mpsc::channel::<ParallelStepOut>();
        let mut collected: Vec<ParallelStepOut> = Vec::new();
        let mut control_stop: Option<AgentRunState> = None;
        let mut gap_stop: Option<String> = None;

        std::thread::scope(|scope| -> Result<(), AgentError> {
            for _ in 0..parallelism {
                let queue = Arc::clone(&queue);
                let out_tx = out_tx.clone();
                scope.spawn(move || loop {
                    let mut pending = queue.0.lock().expect("parallel dispatch queue mutex");
                    let item = loop {
                        match pending.pop_front() {
                            Some(item) => break item,
                            // 空队列：等待派发（Condvar；派发器每 push 通知）。
                            None => {
                                pending = queue
                                    .1
                                    .wait(pending)
                                    .expect("parallel dispatch queue condvar");
                            }
                        }
                    };
                    match item {
                        Some((ordinal, seq, step)) => {
                            drop(pending);
                            let out = execute_parallel_step(deps_ref, ordinal, seq, round, &step);
                            let _ = out_tx.send(out);
                        }
                        // 关闭毒丸：worker 退出（out_tx 克隆端随线程结束
                        // 释放，主线程收集循环随之收敛）。
                        None => break,
                    }
                });
            }

            // 派发循环（主线程 = 派发器）：派发间隙消费 cancel/pause、
            // 墙钟与速率复核；入队即计数（进程内复核口径 = 已派发步数）。
            'dispatch: for (ordinal, (_plan_i, step, _call_key)) in dispatch.iter().enumerate() {
                if self.state.consume_cancel_request(p.run_id_str)? {
                    control_stop = Some(AgentRunState::Cancelled);
                    gap_stop = Some(format!(
                        "cancelled (cooperative, D-TC-04): request consumed during parallel dispatch (before dispatch item {ordinal}, {ordinal} dispatched)"
                    ));
                    break 'dispatch;
                }
                if self.state.consume_pause_request(p.run_id_str)? {
                    control_stop = Some(AgentRunState::Paused);
                    gap_stop = Some(format!(
                        "paused (cooperative): request consumed during parallel dispatch (before dispatch item {ordinal}, {ordinal} dispatched); resume via AgentLoop::resume"
                    ));
                    break 'dispatch;
                }
                if control::elapsed_over_budget(started_at, cfg.max_elapsed_ms) {
                    gap_stop = Some(format!(
                        "guardrail: budget max_elapsed_ms={} reached",
                        cfg.max_elapsed_ms
                    ));
                    self.audit_emit(
                        control::actions::GUARDRAIL,
                        p.run_id_str,
                        p.trace_id.as_deref(),
                        p.actor,
                        json!({"kind": "budget_elapsed", "limit_ms": cfg.max_elapsed_ms}),
                    );
                    break 'dispatch;
                }
                // 速率复核（D5.2 进程内计数复核点）：窗口内步数
                // （DB COUNT + 派发计数取大）达帽即停派发，超帽步骤不执行。
                if let (Some(limit), Some(tracker)) = (rate_limit, rate_tracker) {
                    let db =
                        self.count_caller_window_steps(&budget.caller_id, budget.rate_window_ms)?;
                    let effective = db.max(tracker.count());
                    if effective >= limit {
                        gap_stop = Some("guardrail: caller rate budget".to_string());
                        self.audit_emit(
                            control::actions::GUARDRAIL,
                            p.run_id_str,
                            p.trace_id.as_deref(),
                            p.actor,
                            json!({"kind": "budget_caller_rate", "caller_id": budget.caller_id,
                                   "window_ms": budget.rate_window_ms, "limit": limit,
                                   "window_steps": db, "proc_window_steps": tracker.count(),
                                   "phase": "dispatch_gap"}),
                        );
                        break 'dispatch;
                    }
                }
                let seq = steps.len() + ordinal;
                {
                    let mut q = queue.0.lock().expect("parallel dispatch queue mutex");
                    q.push_back(Some((ordinal, seq, (*step).clone())));
                    queue.1.notify_one();
                }
                if let Some(tracker) = rate_tracker {
                    tracker.bump();
                }
            }
            // 关闭毒丸 × worker 数：未消费的排队项（cancel/rate 停派发后的
            // 残余）按序留队不被执行（超帽/超预算步骤不执行），worker 取到
            // 毒丸即退出。
            {
                let mut q = queue.0.lock().expect("parallel dispatch queue mutex");
                for _ in 0..parallelism {
                    q.push_back(None);
                }
                queue.1.notify_all();
            }
            drop(out_tx);
            // 收齐终态（join 收敛顺序与完成顺序解耦，node_parallel 先例）。
            while let Ok(out) = out_rx.recv() {
                collected.push(out);
            }
            Ok(())
        })?;

        // ---- 阶段 C：主线程按 plan 顺序统一落（D3：收齐终态后统一落）。
        collected.sort_by_key(|out| out.ordinal);
        let mut any_failed = false;
        let mut stall_fired: Option<String> = None;
        for (ordinal, out) in collected.iter().enumerate() {
            let (_plan_i, step, call_key) = &dispatch[ordinal];
            debug_assert_eq!(out.ordinal, ordinal, "parallel result order");
            // 子线程降级 warning 并入（evidence 登记失败等）。
            for w in &out.warnings {
                warnings.push(w.clone());
            }
            // 轨迹落库（seq 确定性）。
            self.persist_step_trajectory(p.run_id, out.seq, round, step, &out.record, warnings);
            // memory_refs 记账 + 三类 Memory 落账（与串行 execute_step 的
            // Ok 分支同实现）。
            if let Some(reg) = &out.reg {
                let mem_refs = vec![
                    format!("mem://manifest/{}", context.manifest_id),
                    format!("mem://artifact/{}", reg.artifact_id),
                    format!("mem://evidence/{}", reg.evidence_id),
                ];
                if let Err(e) = self.store.append_memory_refs(p.run_id.as_str(), &mem_refs) {
                    warnings.push(format!("memory_refs append failed: {e}"));
                }
                self.record_step_memory(
                    p.run_id,
                    round,
                    p.task_id.as_deref(),
                    reg,
                    step,
                    &out.record.tool_run_id,
                    &out.record.tool_version,
                    warnings,
                );
            }
            executed_keys.push(call_key.clone());
            if out.record.status != "SUCCEEDED" {
                any_failed = true;
            }
            // 停滞检测记账（连续无新证据哈希；窗口语义与串行同口径）。
            if control::step_has_new_evidence(&out.record, seen_hashes) {
                *no_progress_streak = 0;
                if let Some(h) = &out.record.content_hash {
                    seen_hashes.push(h.clone());
                }
            } else {
                *no_progress_streak += 1;
            }
            steps.push(out.record.clone());
            // CB-1（D3）轮界跨度表记录（旁路；折叠默认关零行为影响）。
            step_rounds.push(round);
            self.audit_emit(
                "agent.step.completed",
                p.run_id_str,
                p.trace_id.as_deref(),
                p.actor,
                json!({
                    "tool_id": step.tool_id,
                    "seq": out.seq,
                    "round": round,
                    "status": out.record.status,
                    "tool_run_id": out.record.tool_run_id,
                    "artifact_id": out.record.artifact_id,
                    "evidence_id": out.record.evidence_id,
                    "steps_done": steps.len(),
                }),
            );
            if stall_fired.is_none()
                && cfg.stall_window > 0
                && *no_progress_streak >= cfg.stall_window
            {
                stall_fired = Some(format!(
                    "guardrail: stall detected ({no_progress_streak} steps without new evidence hash)"
                ));
                self.audit_emit(
                    control::actions::GUARDRAIL,
                    p.run_id_str,
                    p.trace_id.as_deref(),
                    p.actor,
                    json!({"kind": "stall", "streak": no_progress_streak,
                           "window": cfg.stall_window}),
                );
            }
        }

        // ---- 收敛：控制消费（跃迁与审计在收齐后落，派发间隙只消费请求位）。
        if let Some(state) = control_stop {
            self.store.transition_run(p.run_id_str, state)?;
            let action = if state == AgentRunState::Cancelled {
                control::actions::CANCELLED
            } else {
                control::actions::PAUSED
            };
            self.audit_emit(
                action,
                p.run_id_str,
                p.trace_id.as_deref(),
                p.actor,
                json!({"before_step": steps.len(), "round": round,
                       "cooperative": state == AgentRunState::Cancelled}),
            );
            return Ok(ParallelRoundOut {
                executed_in_round: collected.len(),
                duplicates_in_round,
                any_failed,
                stop: Some(ParallelStop::Control(state)),
            });
        }
        if let Some(reason) = gap_stop {
            return Ok(ParallelRoundOut {
                executed_in_round: collected.len(),
                duplicates_in_round,
                any_failed,
                stop: Some(ParallelStop::Reason(reason)),
            });
        }
        if let Some(reason) = stall_fired {
            return Ok(ParallelRoundOut {
                executed_in_round: collected.len(),
                duplicates_in_round,
                any_failed,
                stop: Some(ParallelStop::Reason(reason)),
            });
        }
        Ok(ParallelRoundOut {
            executed_in_round: collected.len(),
            duplicates_in_round,
            any_failed,
            stop: None,
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
    /// Memory 落账（可选接线，写失败降级 warning 不炸 run）。MA-2：新增
    /// caller_id 传参（串行路径 None = 既有行为零变化）。
    #[allow(clippy::too_many_arguments)]
    fn execute_step(
        &self,
        run_id: &RunId,
        seq: usize,
        round: usize,
        trace_id: Option<&str>,
        agent_id: &str,
        step: &PlanStep,
        task_id: Option<&str>,
        caller_id: Option<&str>,
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
            // P7-5：tool_run 行 trace_id 同链落列（迁移 015）。
            trace_id: trace_id.map(str::to_string),
            // MA-2（D4④）：caller 归属经 submit_for_caller 落 tool_run
            // .caller_id 列（027）；None = legacy 直通路径零变化。
            caller_id: caller_id.map(str::to_string),
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
            // P7-5：artifact/evidence 行 trace_id 同链落列（迁移 015）。
            trace_id: trace_id.map(str::to_string),
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

    /// 审计旁路（emit_best_effort；写失败不反压业务）。P7-5：trace_id
    /// 随事件落 audit_event.trace_id 列（迁移 015），与 correlation_id
    /// （恒 = run_id，run 内事件链）并行。
    fn audit_emit(
        &self,
        action: &str,
        run_id: &str,
        trace_id: Option<&str>,
        actor: &str,
        detail: Value,
    ) {
        if let Some(audit) = &self.audit {
            let mut event = domain::audit::AuditEvent::new(actor, action)
                .with_entity("agent_run", run_id)
                .with_correlation(run_id)
                .with_detail(detail.to_string());
            if let Some(trace) = trace_id {
                event = event.with_trace(trace);
            }
            audit.emit_best_effort(event);
        }
    }
}

/// MA-2（D3）fan-out 子步执行（worker 线程内跑）：受理 + 驱动 + Artifact/
/// Evidence 登记；轨迹落库 / memory 记账 / 审计由主线程收齐终态后按 plan
/// 顺序统一落（[`AgentLoop::run_round_parallel`] 阶段 C）。受理拒绝记
/// SUBMIT_REJECTED（与串行 execute_step 同口径）；写面经 Pool 串行化。
fn execute_parallel_step(
    deps: &ParallelDeps<'_>,
    ordinal: usize,
    seq: usize,
    round: usize,
    step: &PlanStep,
) -> ParallelStepOut {
    let _ = round;
    let record = StepRecord {
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
    let mut out = ParallelStepOut {
        ordinal,
        seq,
        record,
        reg: None,
        warnings: Vec::new(),
    };
    let spec = ToolCallSpec {
        tool_id: step.tool_id.clone(),
        version: step.version.clone(),
        input: step.input.clone(),
        task_id: deps.task_id.map(str::to_string),
        workspace_id: deps.workspace_id.to_string(),
        // 幂等提交键按步序区分（与串行 execute_step 同构；seq 确定性
        // 保证同 run 两子步不共享键）。
        submit_key: Some(format!(
            "agent:{}:{}:{}",
            deps.run_id.as_str(),
            seq,
            step.tool_id
        )),
        trace_id: deps.trace_id.map(str::to_string),
        caller_id: deps.caller_id.map(str::to_string),
    };
    let called = match deps.tool.call(&spec) {
        Ok(called) => called,
        Err(e) => {
            out.record.error = Some(e.to_string());
            return out;
        }
    };
    out.record.tool_run_id = called.run_id.clone();
    out.record.tool_version = called.tool_version.clone();
    out.record.status = called.envelope.status.as_str().to_string();
    out.record.summary = called.envelope.summary.clone();
    out.record.elapsed_ms = called.envelope.usage.elapsed_ms;
    if let Some(err) = &called.envelope.error {
        out.record.error = Some(err.message.clone());
    }
    // Artifact/Evidence 登记（SUCCEEDED 取 result，失败取错误对象；同哈希
    // 对账——与串行 execute_step 同实现）。登记失败降级 warning 不炸 run。
    let payload = called.envelope.result.clone().unwrap_or_else(|| {
        json!({
            "error_category": called.envelope.error.as_ref().map(|e| e.category.clone()),
            "error_message": called.envelope.error.as_ref().map(|e| e.message.clone()),
        })
    });
    match deps.evidence.register_tool_result(&Registration {
        agent_id: deps.agent_id.to_string(),
        task_id: deps.task_id.map(str::to_string),
        tool_run_id: called.run_id.clone(),
        tool_id: step.tool_id.clone(),
        tool_version: called.tool_version.clone(),
        payload,
        trace_id: deps.trace_id.map(str::to_string),
    }) {
        Ok(reg_out) => {
            out.record.artifact_id = Some(reg_out.artifact_id.clone());
            out.record.evidence_id = Some(reg_out.evidence_id.clone());
            out.record.content_hash = Some(reg_out.content_hash.clone());
            out.reg = Some(reg_out);
        }
        Err(e) => out.warnings.push(format!("evidence registration failed: {e}")),
    }
    out
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

    /// CB-1 折叠单测步骤构造（轮次由调用方 rounds 表独立给定）。
    fn fold_step(seq: usize, summary: &str) -> StepRecord {
        StepRecord {
            tool_id: format!("rip.kb.tool{}", seq % 2),
            tool_version: "0.1.0".to_string(),
            tool_run_id: format!("rn-{seq}"),
            status: "SUCCEEDED".to_string(),
            artifact_id: Some(format!("art-{seq}")),
            evidence_id: Some(format!("ev-{seq}")),
            content_hash: Some(format!("hash-{seq}")),
            summary: Some(summary.to_string()),
            error: None,
            elapsed_ms: 1,
        }
    }

    /// CB-1（红线 4）折叠最严缺省：FoldConfig 默认全关；RunControlConfig
    /// 既有七字段值零改动（control.rs:114-125 断言面）。
    #[test]
    fn cb1_fold_config_defaults_strictest() {
        let fold = control::FoldConfig::default();
        assert!(!fold.enabled, "折叠默认关（判据 5）");
        assert_eq!(fold.fold_growth_tokens, 12_000);
        assert_eq!(fold.fold_min_interval_rounds, 4);
        assert_eq!(fold.fold_hot_window_rounds, 2);
        let cfg = RunControlConfig::default();
        assert_eq!(cfg.max_rounds, 8);
        assert_eq!(cfg.stall_window, 2);
        assert_eq!(cfg.drift_tolerance, 1);
        assert_eq!(cfg.min_evidence_hint, 1);
        assert_eq!(cfg.max_elapsed_ms, 0);
        assert!(!cfg.allow_parallel_subtasks);
        assert!(cfg.caller_budgets.is_empty());
        assert!(!cfg.fold.enabled);
    }

    /// CB-1（D1/D3）折叠纯函数确定性 + 合成行词形：同转录前缀 → 同
    /// digest/摘要/est；合成行词形落既有词表内。
    #[test]
    fn cb1_fold_segment_deterministic_and_wordforms() {
        let steps: Vec<StepRecord> = (0..3)
            .map(|i| fold_step(i, &format!("step {i} summary text")))
            .collect();
        let rounds: Vec<usize> = vec![0, 0, 1];

        let a = fold_segment(&steps, &rounds, 0, 0).expect("round 0 segment");
        let b = fold_segment(&steps, &rounds, 0, 0).expect("deterministic replay");
        assert_eq!(a.digest, b.digest, "同转录前缀同 digest（判据 2 纯函数面）");
        assert_eq!(a.summary, b.summary, "同转录前缀同摘要");
        assert_eq!(a.est_before, b.est_before);
        assert_eq!(a.est_after, b.est_after);
        assert_eq!(a.steps_folded, 2);
        // est_tokens 单点（D6）：est_after = est_tokens(摘要)。
        assert_eq!(a.est_after, est_tokens(&a.summary));
        // est 记账方向：合成行小于折前段（折叠确有收缩）。
        assert!(a.est_after < a.est_before, "fold shrinks planner view");

        // 确定性摘要词形（D1）：前缀 + 轮区间 + 步数 + 工具集 + artifact 计数，
        // 无时间戳无进程内计数。
        assert!(a.summary.starts_with(FOLD_SUMMARY_PREFIX));
        assert!(a.summary.contains("rounds=0-0"));
        assert!(a.summary.contains("steps=2"));
        assert!(a.summary.contains("tools={rip.kb.tool0,rip.kb.tool1}"));
        assert!(a.summary.contains("artifacts={2}"));

        // 合成行词形（D3）：tool_id/tool_version/tool_run_id/status/elapsed。
        assert_eq!(a.synthetic.tool_id, FOLD_TOOL_ID);
        assert_eq!(a.synthetic.tool_id, "transcript:fold");
        assert_eq!(a.synthetic.tool_version, TRANSCRIPT_FOLD_VERSION);
        assert_eq!(a.synthetic.tool_run_id, "", "空串先例（SUBMIT_REJECTED 同款）");
        assert_eq!(a.synthetic.status, "SUCCEEDED", "ToolRunStatus 词形内");
        assert_eq!(a.synthetic.elapsed_ms, 0);
        assert_eq!(a.synthetic.summary.as_deref(), Some(a.summary.as_str()));
        assert!(a.synthetic.artifact_id.is_none());
        assert!(a.synthetic.evidence_id.is_none());
        assert!(a.synthetic.content_hash.is_none());

        // 空区间拒绝面。
        assert!(fold_segment(&steps, &rounds, 9, 10).is_none());
    }

    /// CB-1（D3）折叠投影：已折段每段一条合成行原位替代 + 未折段原样透传
    /// + 热窗段透传 + 无事件返回 None。
    #[test]
    fn cb1_fold_projection_replaces_folded_segments_in_place() {
        let steps: Vec<StepRecord> = (0..5)
            .map(|i| fold_step(i, &format!("step {i}")))
            .collect();
        // 步轮次：0,0,1,1,2（轮 0/1 各两步，轮 2 一步）。
        let rounds: Vec<usize> = vec![0, 0, 1, 1, 2];
        let seg01 = fold_segment(&steps, &rounds, 0, 1).expect("rounds 0-1");
        let folds = vec![AppliedFold {
            event: FoldEvent {
                rounds: [0, 1],
                steps_folded: seg01.steps_folded,
                est_before: seg01.est_before,
                est_after: seg01.est_after,
                digest: seg01.digest.clone(),
                summary: seg01.summary.clone(),
                memory_id: None,
            },
            synthetic: seg01.synthetic.clone(),
        }];

        // 无事件：None（全量直投，默认关路径零分配）。
        assert!(fold_projection(&steps, &rounds, &[]).is_none());

        // 投影：合成行在位次 0，轮 2 步原样透传；长度 1 + 1 < 全量 5。
        let view = fold_projection(&steps, &rounds, &folds).expect("projection");
        assert_eq!(view.len(), 2, "合成行 + 热窗外余步");
        assert_eq!(view[0].tool_id, FOLD_TOOL_ID);
        assert_eq!(view[0].summary.as_deref(), Some(seg01.summary.as_str()));
        assert_eq!(view[1].tool_id, steps[4].tool_id);
        assert_eq!(view[1].content_hash, steps[4].content_hash);
        assert!(view.len() < steps.len(), "规划器可见面 < 全量步数");
    }
}
