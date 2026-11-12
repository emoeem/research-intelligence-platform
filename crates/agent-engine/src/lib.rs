//! Phase 7 Agent Engine 执行闭环（ROADMAP §8.3 P7-2 + P7-5 运行控制；执行
//! 基准 docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md §9:454-477 八步流程）。
//!
//! 八步流程落面（计划 §9 原文流程图）：
//! 1. 理解目标：[`broker_link::BrokerLink::assemble_confirmed`]（意图分类 +
//!    ambiguous 确认回路 + L0 到 L3 全层装配，expand 记账进 context_ref_log）；
//! 2. 制定计划：[`agent_loop::Planner`] 端口（内置确定性最小规划器，模型
//!    在环属后续批次，ROADMAP §8.5 风险 3）；
//! 3. 选择工具：capability_refs 绑定 ∩ Broker L2 tool_briefs（每轮上下文
//!    来自 Broker，Agent 禁自查库，P7-3 判据同源）；
//! 4. 调用 Tool Engine：[`tool_link::ToolLink`]（D-TC-05 受理 + 状态机 +
//!    信封，禁直调宿主 Shell，计划 §6:370 禁令同等生效）；
//! 5. 解析结果并登记 Artifact/Evidence：[`evidence::EvidenceRegistrar`]；
//! 6. 判断继续或结束：Planner 每轮决策 + P7-4 九类控制护栏（暂停/恢复/
//!    取消/重试/停滞/重复调用/目标偏移/证据不足提示/预算与资源限制）；
//! 7. 生成报告：[`agent_loop::RunReport`]（manifest/意图/展开/步骤/状态全
//!    面可对账）。
//!
//! 数据面：agent_definition/agent_run（006 迁移）+ 三类 Memory（013 迁移）+
//! 运行控制与步骤轨迹（014 迁移，[`state::RunStateStore`]）+ trace 统一关联
//! （015 迁移五面 trace_id 列，写面统一注入，只读查询面 [`trace::trace_query`]，
//! 六 ID 类型集中定义在 domain::ids）；状态机纯函数在 domain::agent，CAS 持久
//! 化在 [`store::AgentStore`]，控制编排面在 [`control::RunControl`]。
//!
//! 边界（如实声明）：
//! - 工具驱动经 [`tool_link::RunDriver`] 端口注入；MockRunner 注入路径与
//!   Phase 1 契约测试同口径（TOOL-CONTRACT §10.1），真实 python_worker
//!   驱动走 tool-engine adapter（不经 shell，argv 固定清单）。
//! - Artifact 正文持久化属 Artifact Manager 后续批次：给 artifacts_dir
//!   时落文件，缺省登记引用 uri（ripartifact://），如实标注不做伪装。
//! - 暂停/取消为协作式语义（D-TC-04，先例 tool_run.cancel_requested_at）：
//!   控制面只写请求位，执行循环步间消费；RUNNING 期进程崩溃后的孤儿 run
//!   行处置属后续批次，本批如实声明不覆盖。
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod agent_loop;
pub mod broker_link;
pub mod control;
pub mod error;
pub mod evidence;
pub mod memory;
pub mod state;
pub mod store;
pub mod tool_link;
pub mod trace;

pub use agent_loop::{
    Decision, KeywordSearchPlanner, L3Expansion, LoopConfig, PlanContext, PlanStep, Planner,
    ResumeRequest, RunReport, RunRequest, StepRecord,
};
pub use broker_link::{AssembleOutcome, BrokerLink, FirstFamilyConfirmer, IntentConfirmer};
pub use control::{
    actions, canonical_call_key, CancelOutcome, PauseOutcome, RetryOutcome, RunControl,
    RunControlConfig,
};
pub use error::AgentError;
pub use state::{RunStateStore, StoredStep};
pub use store::{AgentStore, StoredDefinition, StoredRun};
pub use tool_link::{
    MockDriver, PythonWorkerDriver, RunDriver, ToolCallOutcome, ToolCallSpec, ToolLink,
};
pub use trace::{chain_to_json, trace_query, TraceChain, DEFAULT_SECTION_LIMIT};

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// 生成 `<prefix>-<hex24>` 形态的引擎内 id（时间戳 + 进程熵 + 进内序号，
/// sha256 截断；与 context-broker assemble::new_id 同源口径，进程熵通道
/// RandomState::new 独立种子，防同秒跨进程碰撞）。
pub(crate) fn new_id(prefix: &str) -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    static PROCESS_SALT: OnceLock<(u64, u64)> = OnceLock::new();
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let (salt_a, salt_b) =
        *PROCESS_SALT.get_or_init(|| (process_entropy_u64(), process_entropy_u64()));
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let material = format!(
        "{nanos}|{salt_a:x}{salt_b:x}|{seq}|{}|agent-engine",
        std::process::id()
    );
    format!(
        "{prefix}-{}",
        &tool_registry::hash::sha256_hex(material.as_bytes())[..24]
    )
}

/// 进程熵抽取（RandomState 种子来自 OS 熵；std 标准通道）。
fn process_entropy_u64() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u64(0x9E37_79B9_7F4A_7C15);
    hasher.finish()
}
