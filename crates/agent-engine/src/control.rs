//! Agent 运行控制面（P7-4 编排批次，ROADMAP §8.3 P7-5 内容行；计划 §9:474
//! 能力清单：暂停、恢复、取消、重试、停滞检测、重复调用检测、目标偏移
//! 检测、证据不足提示、预算与资源限制）。
//!
//! 九类控制的本批落面（全部与执行循环同批上线，不留「先上循环后补护栏」
//! 缺口，ROADMAP §8.5 风险 1）：
//! - 暂停 / 协作取消：[`RunControl::pause`] / [`RunControl::cancel`] 写
//!   请求位（D-TC-04 语义，先例 tool_run.cancel_requested_at），执行循环
//!   步间消费；QUEUED/PAUSED 的取消走直接跃迁（域跃迁表合法对）。
//! - 恢复：[`crate::agent_loop::AgentLoop::resume`]（执行循环职责，入口
//!   在 loop；本模块提供请求位与轨迹读面）。
//! - 重试：[`RunControl::retry`]，新 run 重放同定义版本（domain::agent
//!   跃迁表不复活终态行，FAILED 源 run 保持 FAILED，新 run QUEUED 带
//!   retry_of_run_id 链）。
//! - 停滞检测：连续 [`RunControlConfig::stall_window`] 步无新证据哈希
//!   （SUCCEEDED 但 artifact content_hash 不再出现新值）即停。
//! - 重复调用检测：(tool_id, 规范化 input) 键已执行即跳过（
//!   [`canonical_call_key`]，serde_json Value 默认 BTreeMap 键序规范）。
//! - 目标偏移检测：初始计划面（首个含步骤的 plan 的工具集合）之外的工具
//!   跳转计偏移事件，连续偏移超过容限即停（确定性代理定义，见
//!   [`drift_step`] 注）。
//! - 证据不足提示：收尾时可用证据（SUCCEEDED 且登记 evidence）少于
//!   [`RunControlConfig::min_evidence_hint`] 即在报告 warnings 留提示。
//! - 预算：步数（LoopConfig::max_tool_calls 既有）+ 轮次
//!   ([`RunControlConfig::max_rounds`]) + 墙钟（
//!   [`RunControlConfig::max_elapsed_ms`]，0 = 不限）。
//! - 资源限制：并行子任务默认关闭（[`RunControlConfig::allow_parallel_subtasks`]，
//!   低并发纪律，ROADMAP §8.5 风险 1），单轮多步骤串行执行并留痕。
//!
//! 控制请求只写信号位、不跃迁状态（跃迁面统一在 [`crate::store::AgentStore`]
//! 的域校验 + CAS），避免控制面绕过状态机。

use crate::error::AgentError;
use crate::state::RunStateStore;
use crate::store::AgentStore;
use domain::{AgentDefinition, AgentRun, AgentRunState};
use serde_json::Value;
use tool_registry::hash::sha256_hex;

/// 审计动作词形（防词形漂移，调用方只引常量）。
pub mod actions {
    /// 控制面受理协作式暂停请求。
    pub const PAUSE_REQUESTED: &str = "agent.run.pause_requested";
    /// 控制面受理协作式取消请求。
    pub const CANCEL_REQUESTED: &str = "agent.run.cancel_requested";
    /// 执行循环消费请求后进入 PAUSED。
    pub const PAUSED: &str = "agent.run.paused";
    /// 执行循环消费请求后进入 CANCELLED（协作式，D-TC-04）。
    pub const CANCELLED: &str = "agent.run.cancelled";
    /// QUEUED/PAUSED 直接取消落地。
    pub const CANCELLED_DIRECT: &str = "agent.run.cancelled_direct";
    /// 暂停 run 恢复执行（PAUSED→RUNNING）。
    pub const RESUMED: &str = "agent.run.resumed";
    /// 重试链建立（新 run 重放；correlation_id 用新 run id）。
    pub const RETRY: &str = "agent.run.retry";
    /// 规划步骤为已执行调用重复，跳过执行。
    pub const STEP_DUPLICATE: &str = "agent.step.duplicate";
    /// 护栏停机（stall/drift/budget/duplicate_exhausted）。
    pub const GUARDRAIL: &str = "agent.run.guardrail";
}

/// 运行控制配置（护栏参数；默认值即低并发纪律的最严面）。
#[derive(Debug, Clone)]
pub struct RunControlConfig {
    /// 单次 run 规划轮次上限（预算面；0 = 不限）。
    pub max_rounds: usize,
    /// 停滞窗口：连续 N 步无新证据哈希即停（停滞检测）。
    pub stall_window: usize,
    /// 目标偏移容限：连续 N 轮偏移事件内仍放行，超过即停。
    pub drift_tolerance: usize,
    /// 证据不足提示阈值：可用证据（SUCCEEDED 且登记 evidence）少于该值
    /// 时在报告 warnings 留提示。
    pub min_evidence_hint: usize,
    /// 墙钟预算（毫秒，0 = 不限；步间检查）。
    pub max_elapsed_ms: u64,
    /// 并行子任务开关：默认 false（低并发纪律，ROADMAP §8.5 风险 1）；
    /// false 时单轮多步骤一律串行执行并在报告留痕。
    pub allow_parallel_subtasks: bool,
}

impl Default for RunControlConfig {
    fn default() -> Self {
        Self {
            max_rounds: 8,
            stall_window: 2,
            drift_tolerance: 1,
            min_evidence_hint: 1,
            max_elapsed_ms: 0,
            allow_parallel_subtasks: false,
        }
    }
}

/// 暂停请求结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PauseOutcome {
    /// 已受理：请求位已写，执行循环步间消费后进 PAUSED（协作式）。
    Requested,
    /// 拒绝：当前状态不可发起（附当前状态词形）。
    Refused(String),
}

/// 取消结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancelOutcome {
    /// 协作式：请求位已写，执行循环步间消费后进 CANCELLED（D-TC-04）。
    Requested,
    /// 立即取消：run 不在执行循环内（QUEUED/PAUSED），直接跃迁 CANCELLED。
    CancelledNow,
    /// 拒绝：终态不可取消（附当前状态词形）。
    Refused(String),
}

/// 重试结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryOutcome {
    /// 新 run id（QUEUED，带 retry_of_run_id 链）。
    pub new_run_id: String,
    /// 重放源 run id（保持 FAILED 终态不复活）。
    pub source_run_id: String,
}

/// 运行控制面（编排请求位写入、直接取消、重试链；检测器为纯函数）。
pub struct RunControl {
    store: AgentStore,
    state: RunStateStore,
}

impl RunControl {
    /// 在存储与控制持久化面之上构造。
    pub fn new(store: AgentStore, state: RunStateStore) -> Self {
        Self { store, state }
    }

    /// 发起协作式暂停（仅 RUNNING）：写请求位，执行循环步间消费。
    pub fn pause(&self, run_id: &str) -> Result<PauseOutcome, AgentError> {
        let Some(state) = self.state.run_state(run_id)? else {
            return Err(AgentError::RunNotFound(run_id.to_string()));
        };
        if !state.pause_requestable() {
            return Ok(PauseOutcome::Refused(state.as_str().to_string()));
        }
        let accepted = self.state.request_pause(run_id)?;
        Ok(if accepted {
            PauseOutcome::Requested
        } else {
            PauseOutcome::Refused(state.as_str().to_string())
        })
    }

    /// 取消（D-TC-04 协作式语义）：
    /// - RUNNING：写请求位，执行循环步间消费后进 CANCELLED；
    /// - QUEUED / PAUSED：不在执行循环内，直接跃迁 CANCELLED（域跃迁表
    ///   合法对，CAS 落库）；
    /// - 终态：拒绝（附当前状态）。
    pub fn cancel(&self, run_id: &str) -> Result<CancelOutcome, AgentError> {
        let Some(state) = self.state.run_state(run_id)? else {
            return Err(AgentError::RunNotFound(run_id.to_string()));
        };
        if !state.cancel_requestable() {
            return Ok(CancelOutcome::Refused(state.as_str().to_string()));
        }
        if state == AgentRunState::Running {
            let accepted = self.state.request_cancel(run_id)?;
            return Ok(if accepted {
                CancelOutcome::Requested
            } else {
                CancelOutcome::Refused(state.as_str().to_string())
            });
        }
        // QUEUED / PAUSED：直接跃迁（域校验 + CAS 在 store）。
        self.store
            .transition_run(run_id, AgentRunState::Cancelled)?;
        Ok(CancelOutcome::CancelledNow)
    }

    /// 重试（新 run 重放同定义版本）：仅 FAILED 源 run 可重试；新 run
    /// QUEUED 落行 + retry_of_run_id 链，源 run 保持 FAILED 终态不复活。
    /// 新 run 的执行走 [`crate::agent_loop::AgentLoop::run`] 正常流程
    /// （fresh run），本方法只建链不执行。
    pub fn retry(&self, run_id: &str) -> Result<RetryOutcome, AgentError> {
        let stored = self
            .store
            .run(run_id)?
            .ok_or_else(|| AgentError::RunNotFound(run_id.to_string()))?;
        if stored.run.state() != AgentRunState::Failed {
            return Err(AgentError::Plan(format!(
                "retry requires FAILED source run; run {run_id} is {}",
                stored.run.state().as_str()
            )));
        }
        let new_run_id = AgentStore::new_run_id();
        let replay = AgentRun::try_new(
            new_run_id.clone(),
            stored.run.task_id().map(str::to_string),
            stored.run.agent_id().clone(),
            stored.run.agent_version().to_string(),
            AgentRunState::Queued,
            vec![],
        )?;
        // P7-5：重试链继承源 run 的 trace_id（跨 run 关联同链，迁移 015；
        // 源 run 无关联列即新 run 同样 NULL，不伪造）。
        self.store.create_run(&replay, stored.trace_id.as_deref())?;
        self.state.set_retry_of(new_run_id.as_str(), run_id)?;
        Ok(RetryOutcome {
            new_run_id: new_run_id.as_str().to_string(),
            source_run_id: run_id.to_string(),
        })
    }

    /// 定义视图（重试重放与报告面用；重试执行方按源 run 钉定版本重读）。
    pub fn definition(
        &self,
        agent_id: &str,
        version: &str,
    ) -> Result<Option<AgentDefinition>, AgentError> {
        Ok(self
            .store
            .definition(agent_id, version)?
            .map(|s| s.definition))
    }
}

/// 重复调用检测键：(tool_id, 规范化 input) 的 sha256。
///
/// 规范化依赖 serde_json Value 的默认 BTreeMap 对象键序（workspace 未开
/// preserve_order 特性），同语义输入必得同键；tool_id 进哈希材料，同工具
/// 不同输入不判重。
pub fn canonical_call_key(tool_id: &str, input: &Value) -> String {
    let canonical = serde_json::to_string(input).unwrap_or_else(|_| "null".to_string());
    let material = format!("{tool_id}|{canonical}");
    sha256_hex(material.as_bytes())
}

/// 目标偏移判定（确定性代理定义）：工具 id 是否在初始计划面之外。
///
/// 初始计划面 = 本 run 首个产出步骤的 plan 的工具集合（执行循环捕获）。
/// 偏移事件的语义是「执行目标从原计划面跳开」，连续偏移超过
/// [`RunControlConfig::drift_tolerance`] 即停（执行循环编排）； capability
/// 绑定闸仍然先行（面外工具直接拒绝，不进偏移计数）。
pub fn drift_step(initial_surface: &[String], tool_id: &str) -> bool {
    !initial_surface.iter().any(|t| t == tool_id)
}

/// 停滞判定：单步是否「无新证据」（progress = SUCCEEDED 且登记 evidence
/// 且 artifact content_hash 出现此前未出现过的新值）。
///
/// 失败步在 P7-2 起即失败收尾（fail-fast），不进停滞累计；SUCCEEDED 但
/// 哈希重复（同输入重放或空结果）计无进展步。
pub fn step_has_new_evidence(
    record: &crate::agent_loop::StepRecord,
    seen_hashes: &[String],
) -> bool {
    if record.status != "SUCCEEDED" {
        return false;
    }
    let Some(h) = &record.content_hash else {
        return false;
    };
    !h.is_empty() && !seen_hashes.iter().any(|s| s == h)
}

/// 证据不足判定：可用证据数（SUCCEEDED 且登记 evidence 的步数）是否低于
/// 提示阈值。
pub fn insufficient_evidence(steps: &[crate::agent_loop::StepRecord], min: usize) -> bool {
    let usable = steps
        .iter()
        .filter(|s| s.status == "SUCCEEDED" && s.evidence_id.is_some())
        .count();
    usable < min
}

/// 墙钟预算判定（毫秒；0 = 不限）。
pub fn elapsed_over_budget(started: std::time::Instant, max_elapsed_ms: u64) -> bool {
    max_elapsed_ms > 0 && started.elapsed().as_millis() >= u128::from(max_elapsed_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_loop::StepRecord;

    fn record(status: &str, hash: Option<&str>) -> StepRecord {
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
            elapsed_ms: 1,
        }
    }

    /// 规范化调用键：键序不影响结果，tool_id 进材料。
    #[test]
    fn canonical_call_key_is_order_stable_and_tool_scoped() {
        let a = serde_json::json!({"keyword": "nmap", "limit": 5});
        let b = serde_json::json!({"limit": 5, "keyword": "nmap"});
        assert_eq!(
            canonical_call_key("rip.kb.keyword_search", &a),
            canonical_call_key("rip.kb.keyword_search", &b)
        );
        // 同键不同工具不判重；同工具不同输入不判重。
        assert_ne!(
            canonical_call_key("rip.kb.keyword_search", &a),
            canonical_call_key("rip.doc.parse", &a)
        );
        assert_ne!(
            canonical_call_key("rip.kb.keyword_search", &a),
            canonical_call_key(
                "rip.kb.keyword_search",
                &serde_json::json!({"keyword": "x"})
            )
        );
    }

    /// 偏移判定与停滞判定的确定性面。
    #[test]
    fn drift_and_stall_predicates() {
        let surface = vec!["rip.kb.keyword_search".to_string()];
        assert!(!drift_step(&surface, "rip.kb.keyword_search"));
        assert!(drift_step(&surface, "rip.doc.parse"));

        // 停滞：SUCCEEDED 且新哈希 = 有进展；哈希重复/缺失 = 无进展；
        // 失败步恒无进展（fail-fast 面不进累计，由循环编排）。
        assert!(step_has_new_evidence(&record("SUCCEEDED", Some("h1")), &[]));
        assert!(!step_has_new_evidence(
            &record("SUCCEEDED", Some("h1")),
            &["h1".to_string()]
        ));
        assert!(!step_has_new_evidence(&record("SUCCEEDED", None), &[]));
        assert!(!step_has_new_evidence(&record("FAILED", Some("h2")), &[]));
        assert!(!step_has_new_evidence(
            &record("SUBMIT_REJECTED", None),
            &[]
        ));
    }

    /// 证据不足判定与墙钟预算判定。
    #[test]
    fn evidence_hint_and_elapsed_budget() {
        assert!(insufficient_evidence(&[record("FAILED", Some("h"))], 1));
        assert!(!insufficient_evidence(&[record("SUCCEEDED", Some("h"))], 1));
        // min=0 恒不触发。
        assert!(!insufficient_evidence(&[], 0));

        assert!(!elapsed_over_budget(std::time::Instant::now(), 0));
        assert!(elapsed_over_budget(
            std::time::Instant::now() - std::time::Duration::from_millis(50),
            10
        ));
    }

    /// 默认配置：并行子任务关闭（低并发纪律，ROADMAP §8.5 风险 1）。
    #[test]
    fn default_config_disallows_parallel_subtasks() {
        let cfg = RunControlConfig::default();
        assert!(!cfg.allow_parallel_subtasks);
        assert_eq!(cfg.stall_window, 2);
        assert_eq!(cfg.drift_tolerance, 1);
        assert_eq!(cfg.min_evidence_hint, 1);
        assert_eq!(cfg.max_elapsed_ms, 0);
        assert_eq!(cfg.max_rounds, 8);
    }
}
