//! Agent 域模型（2.0 新实体，P7-1 提前通道批次，纯类型零依赖）。
//!
//! 口径（计划 §12:699-700 实体行 + PHASE-ROADMAP §8.3 P7-1）：
//! - AgentDefinition：agent_id、version、context_policy、capability_refs
//! - AgentRun：run_id、task_id、agent_id、state、memory_refs
//!
//! 边界（如实声明）：
//! - 绑定引用校验为纯函数：谓词（工具是否存在）由调用方注入（P7-2 接
//!   Registry 查询），domain 不触存储；指向不存在工具即拒绝（P7-1 判据）。
//! - AgentRunState 为保守核心词表（Queued/Paused/Running/四终态），覆盖
//!   计划 §14:821「暂停恢复取消重试」的最小状态面；P7-2 运行时设计若需
//!   扩展在此枚举上加变体并升迁移 CHECK，不回改本批已登记行。
//! - capability_refs 的工具 id 形状沿用冻结契约 D-TC-02 正则（P1-7b 修订后
//!   段内 [a-z0-9_-]），与 tool-manifest/v1.json id 同步。
//! - Memory 域模型（P7-3 追加，013 迁移三表）：Task/Evidence/Research 三类
//!   记忆条目、生命周期（合法迁移对按类别区分，Evidence/Research 无驱逐）
//!   与研究记忆回链引用（manifest item/artifact/evidence 三型）。反重复
//!   建模口径：Memory 条目只承载指针、快照与摘要，不承载 evidence 实体
//!   字段（评审记录 docs/phase7/AGENT-MEMORY-REVIEW.md）。

use crate::error::DomainError;
use crate::ids::{ArtifactId, RunId};
use std::fmt;

/// Agent 定义标识（非空，不超过 128 字符；命名方案属实现细节）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AgentId(String);

/// 单字符集校验：与冻结 schema id 段内字符集一致。
fn seg_chars_ok(seg: &str) -> bool {
    seg.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// AgentId：非空且不超过 128 字符（不做格式强约束，与 RunId 同风格）。
impl AgentId {
    /// 长度上限。
    pub const MAX_LEN: usize = 128;

    /// 校验并构造。
    pub fn try_new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let reason = if value.is_empty() {
            "empty agent id".to_string()
        } else if value.chars().count() > Self::MAX_LEN {
            format!("agent id exceeds {} chars", Self::MAX_LEN)
        } else {
            return Ok(Self(value));
        };
        Err(DomainError::InvalidAgentId { value, reason })
    }

    /// 内部值视图。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 消费自身，返回内部 String。
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for AgentId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for AgentId {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        AgentId::try_new(value)
    }
}

impl TryFrom<&str> for AgentId {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        AgentId::try_new(value.to_string())
    }
}

/// capability_refs 单条绑定：目标工具 id（D-TC-02 形状）+ 可选版本钉定
/// （None = Registry current）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CapabilityRef {
    tool_id: String,
    version: Option<String>,
}

impl CapabilityRef {
    /// 校验并构造：tool_id 必须匹配 `^[a-z][a-z0-9_-]*(\.[a-z][a-z0-9_-]*)+$`
    ///（≤128 字符）；version 缺省 None，给出时须为 X.Y.Z 数字语义化版本。
    pub fn try_new(
        tool_id: impl Into<String>,
        version: Option<String>,
    ) -> Result<Self, DomainError> {
        let tool_id = tool_id.into();
        if tool_id.len() > 128 {
            return Err(DomainError::InvalidAgentDefinition {
                field: "capability_refs.tool_id".to_string(),
                reason: format!("tool id exceeds 128 chars: {tool_id:?}"),
            });
        }
        let segs: Vec<&str> = tool_id.split('.').collect();
        let shape_ok = segs.len() >= 2
            && segs.iter().all(|s| {
                !s.is_empty() && s.starts_with(|c: char| c.is_ascii_lowercase()) && seg_chars_ok(s)
            });
        if !shape_ok {
            return Err(DomainError::InvalidAgentDefinition {
                field: "capability_refs.tool_id".to_string(),
                reason: format!(
                    "tool id {tool_id:?} must match ^[a-z][a-z0-9_-]*(\\.[a-z][a-z0-9_-]*)+$"
                ),
            });
        }
        if let Some(v) = &version {
            if !is_semver(v) {
                return Err(DomainError::InvalidAgentDefinition {
                    field: "capability_refs.version".to_string(),
                    reason: format!("version {v:?} is not X.Y.Z numeric semver"),
                });
            }
        }
        Ok(Self { tool_id, version })
    }

    /// 目标工具 id。
    pub fn tool_id(&self) -> &str {
        &self.tool_id
    }

    /// 钉定版本（None = current）。
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }
}

/// `^[0-9]+\.[0-9]+\.[0-9]+$` 的纯 std 实现。
fn is_semver(v: &str) -> bool {
    let segs: Vec<&str> = v.split('.').collect();
    segs.len() == 3
        && segs
            .iter()
            .all(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
}

/// Agent 定义（计划 §12:699 AgentDefinition 实体行）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDefinition {
    agent_id: AgentId,
    version: String,
    /// 上下文策略原文（JSON 文本，Phase 4 Context Broker 契约对齐前为透传面）。
    context_policy: String,
    capability_refs: Vec<CapabilityRef>,
}

impl AgentDefinition {
    /// 校验并构造。context_policy 非空（JSON 原文，域层不解析其内部形状）。
    pub fn try_new(
        agent_id: AgentId,
        version: String,
        context_policy: String,
        capability_refs: Vec<CapabilityRef>,
    ) -> Result<Self, DomainError> {
        if !is_semver(&version) {
            return Err(DomainError::InvalidAgentDefinition {
                field: "version".to_string(),
                reason: format!("version {version:?} is not X.Y.Z numeric semver"),
            });
        }
        if context_policy.trim().is_empty() {
            return Err(DomainError::InvalidAgentDefinition {
                field: "context_policy".to_string(),
                reason: "context_policy must be non-empty".to_string(),
            });
        }
        Ok(Self {
            agent_id,
            version,
            context_policy,
            capability_refs,
        })
    }

    /// agent_id。
    pub fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }

    /// 定义版本。
    pub fn version(&self) -> &str {
        &self.version
    }

    /// 上下文策略原文。
    pub fn context_policy(&self) -> &str {
        &self.context_policy
    }

    /// 绑定引用列表。
    pub fn capability_refs(&self) -> &[CapabilityRef] {
        &self.capability_refs
    }

    /// 绑定引用校验（ROADMAP §8.3 P7-1 判据：指向不存在工具即拒绝）。
    /// 谓词按工具 id 注入（P7-2 由 Registry 查询接线）；版本钉定与 Registry
    /// 既有版本的比对属接线层职责，不在纯函数内。
    pub fn validate_capability_refs(
        &self,
        tool_exists: &dyn Fn(&str) -> bool,
    ) -> Result<(), DomainError> {
        for r in &self.capability_refs {
            if !tool_exists(r.tool_id()) {
                return Err(DomainError::UnknownCapabilityTarget {
                    tool_id: r.tool_id().to_string(),
                });
            }
        }
        Ok(())
    }
}

/// AgentRun 状态（保守核心词表，见模块头注边界声明）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentRunState {
    /// 已受理待执行。
    Queued,
    /// 暂停（计划 §14:821 暂停恢复能力）。
    Paused,
    /// 执行中。
    Running,
    /// 终态：成功。
    Succeeded,
    /// 终态：失败。
    Failed,
    /// 终态：取消（协作式语义，与 ToolRun D-TC-04 一致）。
    Cancelled,
}

impl AgentRunState {
    /// 全部变体。
    pub const ALL: [AgentRunState; 6] = [
        AgentRunState::Queued,
        AgentRunState::Paused,
        AgentRunState::Running,
        AgentRunState::Succeeded,
        AgentRunState::Failed,
        AgentRunState::Cancelled,
    ];

    /// 全部合法状态对（P7-2 状态机补充，计划 §14:821「暂停恢复取消重试」
    /// 最小面；与 agent_run.state CHECK 六值域一致）：
    /// - QUEUED → RUNNING（开始执行）/ QUEUED → CANCELLED（未开始即取消）；
    /// - RUNNING → PAUSED / SUCCEEDED / FAILED / CANCELLED；
    /// - PAUSED → RUNNING（恢复）/ PAUSED → CANCELLED。
    ///
    /// 重试（FAILED → QUEUED）不入表：P7-5 运行控制批次按「新 run 重放
    /// 同定义版本」设计（与 tool_run 契约同向），不复活终态行。
    pub const LEGAL_TRANSITIONS: [(AgentRunState, AgentRunState); 8] = [
        (AgentRunState::Queued, AgentRunState::Running),
        (AgentRunState::Queued, AgentRunState::Cancelled),
        (AgentRunState::Running, AgentRunState::Paused),
        (AgentRunState::Running, AgentRunState::Succeeded),
        (AgentRunState::Running, AgentRunState::Failed),
        (AgentRunState::Running, AgentRunState::Cancelled),
        (AgentRunState::Paused, AgentRunState::Running),
        (AgentRunState::Paused, AgentRunState::Cancelled),
    ];

    /// 存储值（大写，与 agent_run.state CHECK 域一致）。
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentRunState::Queued => "QUEUED",
            AgentRunState::Paused => "PAUSED",
            AgentRunState::Running => "RUNNING",
            AgentRunState::Succeeded => "SUCCEEDED",
            AgentRunState::Failed => "FAILED",
            AgentRunState::Cancelled => "CANCELLED",
        }
    }

    /// 是否终态。
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            AgentRunState::Succeeded | AgentRunState::Failed | AgentRunState::Cancelled
        )
    }

    /// 协作式暂停是否可发起（P7-4 运行控制）：仅 RUNNING。暂停请求是
    /// 一次性信号位（执行循环步间消费），不在状态机内新增词值；QUEUED
    /// 尚未开跑不涉暂停，PAUSED 本就是暂停态，终态不可变。
    pub fn pause_requestable(self) -> bool {
        self == AgentRunState::Running
    }

    /// 协作式取消是否可发起（P7-4 运行控制，与 ToolRun D-TC-04 语义一致）：
    /// RUNNING 走请求位（执行循环步间消费后跃迁 CANCELLED）；QUEUED/PAUSED
    /// 不在执行循环内，直接跃迁 CANCELLED（两者到 CANCELLED 的对都在
    /// [`Self::LEGAL_TRANSITIONS`] 表内）；终态不可取消。
    pub fn cancel_requestable(self) -> bool {
        matches!(
            self,
            AgentRunState::Queued | AgentRunState::Running | AgentRunState::Paused
        )
    }

    /// 迁移是否合法（含终态不可迁出、自迁移非法；表见 [`Self::LEGAL_TRANSITIONS`]）。
    pub fn can_transition(self, to: AgentRunState) -> bool {
        if self.is_terminal() || self == to {
            return false;
        }
        Self::LEGAL_TRANSITIONS
            .iter()
            .any(|&(from, target)| from == self && target == to)
    }
}

impl fmt::Display for AgentRunState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Agent 运行（计划 §12:700 AgentRun 实体行）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRun {
    run_id: RunId,
    task_id: Option<String>,
    agent_id: AgentId,
    /// 本次运行钉定的定义版本（重放依据）。
    agent_version: String,
    state: AgentRunState,
    memory_refs: Vec<String>,
}

impl AgentRun {
    /// 校验并构造。task_id 给出时须非空；memory_refs 条目须非空。
    pub fn try_new(
        run_id: RunId,
        task_id: Option<String>,
        agent_id: AgentId,
        agent_version: String,
        state: AgentRunState,
        memory_refs: Vec<String>,
    ) -> Result<Self, DomainError> {
        if !is_semver(&agent_version) {
            return Err(DomainError::InvalidAgentDefinition {
                field: "agent_version".to_string(),
                reason: format!("agent_version {agent_version:?} is not X.Y.Z numeric semver"),
            });
        }
        if let Some(t) = &task_id {
            if t.trim().is_empty() {
                return Err(DomainError::InvalidAgentDefinition {
                    field: "task_id".to_string(),
                    reason: "task_id must be non-empty when present".to_string(),
                });
            }
        }
        if memory_refs.iter().any(|m| m.trim().is_empty()) {
            return Err(DomainError::InvalidAgentDefinition {
                field: "memory_refs".to_string(),
                reason: "memory_refs items must be non-empty".to_string(),
            });
        }
        Ok(Self {
            run_id,
            task_id,
            agent_id,
            agent_version,
            state,
            memory_refs,
        })
    }

    /// run_id。
    pub fn run_id(&self) -> &RunId {
        &self.run_id
    }

    /// 关联研究任务。
    pub fn task_id(&self) -> Option<&str> {
        self.task_id.as_deref()
    }

    /// agent_id。
    pub fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }

    /// 钉定定义版本。
    pub fn agent_version(&self) -> &str {
        &self.agent_version
    }

    /// 状态。
    pub fn state(&self) -> AgentRunState {
        self.state
    }

    /// 记忆引用列表。
    pub fn memory_refs(&self) -> &[String] {
        &self.memory_refs
    }

    /// 状态迁移（P7-2 状态机补充）：域校验 [`AgentRunState::can_transition`]
    /// 通过后返回新状态实例，非法对返回 [`DomainError::InvalidAgentTransition`]。
    /// 持久化 CAS 更新由 agent-engine 存储层执行（domain 不触存储）。
    pub fn transition_to(mut self, to: AgentRunState) -> Result<Self, DomainError> {
        let from = self.state;
        if !from.can_transition(to) {
            return Err(DomainError::InvalidAgentTransition {
                from: from.as_str().to_string(),
                to: to.as_str().to_string(),
            });
        }
        self.state = to;
        Ok(self)
    }

    /// 追加记忆引用（去重；空串已由 [`Self::try_new`] 拒绝，追加面同规则）。
    pub fn with_memory_ref(mut self, memory_ref: impl Into<String>) -> Result<Self, DomainError> {
        let memory_ref = memory_ref.into();
        if memory_ref.trim().is_empty() {
            return Err(DomainError::InvalidAgentDefinition {
                field: "memory_refs".to_string(),
                reason: "memory_refs items must be non-empty".to_string(),
            });
        }
        if !self.memory_refs.contains(&memory_ref) {
            self.memory_refs.push(memory_ref);
        }
        Ok(self)
    }
}

// ===========================================================================
// Memory 域模型（P7-3 三类 Memory 与 Context Memory 雏形；013 迁移三表）
// ===========================================================================

/// Memory 类别（013 三表对应词表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryKind {
    /// Task Memory：run 内轮次工作记忆（agent_task_memory）。
    Task,
    /// Evidence Memory：证据/产物引用台账（agent_evidence_memory）。
    Evidence,
    /// Research Memory：Context Memory 雏形，可追溯研究成果（agent_research_memory）。
    Research,
}

impl MemoryKind {
    /// 全部变体。
    pub const ALL: [MemoryKind; 3] = [MemoryKind::Task, MemoryKind::Evidence, MemoryKind::Research];

    /// 存储词形（小写，与 013 表语义一致）。
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryKind::Task => "task",
            MemoryKind::Evidence => "evidence",
            MemoryKind::Research => "research",
        }
    }
}

impl fmt::Display for MemoryKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Memory 生命周期（013 CHECK 词值：ACTIVE/COMPRESSED/EVICTED/ARCHIVED；
/// 合法迁移对按 [`MemoryKind`] 区分，见 [`Self::can_transition_kind`]）。
///
/// 契约（计划 §9:478 原文「原始证据不能因上下文压缩而丢失」）：Evidence
/// Memory 无 EVICTED/ARCHIVED（证据指针台账永不驱逐），Research Memory 无
/// EVICTED（成果回链永不驱逐）；压缩/驱逐只作用于 Task Memory 详情载荷，
/// evidence/artifact 既有表零改动。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryLifecycle {
    /// 在用（详情载荷在场）。
    Active,
    /// 已压缩（详情置空，摘要与回链留存）。
    Compressed,
    /// 已驱逐（详情与摘要全置空，行留墓碑；仅 Task Memory 合法）。
    Evicted,
    /// 已归档（详情置空，摘要与回链留存；仅 Research Memory 合法）。
    Archived,
}

impl MemoryLifecycle {
    /// 全部变体。
    pub const ALL: [MemoryLifecycle; 4] = [
        MemoryLifecycle::Active,
        MemoryLifecycle::Compressed,
        MemoryLifecycle::Evicted,
        MemoryLifecycle::Archived,
    ];

    /// Task Memory 合法对：ACTIVE→COMPRESSED、ACTIVE→EVICTED、COMPRESSED→EVICTED。
    pub const TASK_TRANSITIONS: [(MemoryLifecycle, MemoryLifecycle); 3] = [
        (MemoryLifecycle::Active, MemoryLifecycle::Compressed),
        (MemoryLifecycle::Active, MemoryLifecycle::Evicted),
        (MemoryLifecycle::Compressed, MemoryLifecycle::Evicted),
    ];

    /// Evidence Memory 合法对：仅 ACTIVE→COMPRESSED（指针台账只压缩不驱逐）。
    pub const EVIDENCE_TRANSITIONS: [(MemoryLifecycle, MemoryLifecycle); 1] =
        [(MemoryLifecycle::Active, MemoryLifecycle::Compressed)];

    /// Research Memory 合法对：ACTIVE→COMPRESSED、ACTIVE→ARCHIVED、
    /// COMPRESSED→ARCHIVED（成果永不驱逐）。
    pub const RESEARCH_TRANSITIONS: [(MemoryLifecycle, MemoryLifecycle); 3] = [
        (MemoryLifecycle::Active, MemoryLifecycle::Compressed),
        (MemoryLifecycle::Active, MemoryLifecycle::Archived),
        (MemoryLifecycle::Compressed, MemoryLifecycle::Archived),
    ];

    /// 存储值（大写，与 013 lifecycle CHECK 域一致）。
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryLifecycle::Active => "ACTIVE",
            MemoryLifecycle::Compressed => "COMPRESSED",
            MemoryLifecycle::Evicted => "EVICTED",
            MemoryLifecycle::Archived => "ARCHIVED",
        }
    }

    /// 是否终态（EVICTED/ARCHIVED；COMPRESSED 仍可推进，见各类跃迁表）。
    pub fn is_terminal(&self) -> bool {
        matches!(self, MemoryLifecycle::Evicted | MemoryLifecycle::Archived)
    }

    /// 迁移是否合法（按 [`MemoryKind`] 查跃迁表；终态不可迁出、自迁移非法）。
    pub fn can_transition_kind(self, kind: MemoryKind, to: MemoryLifecycle) -> bool {
        if self.is_terminal() || self == to {
            return false;
        }
        let table: &[(MemoryLifecycle, MemoryLifecycle)] = match kind {
            MemoryKind::Task => &Self::TASK_TRANSITIONS,
            MemoryKind::Evidence => &Self::EVIDENCE_TRANSITIONS,
            MemoryKind::Research => &Self::RESEARCH_TRANSITIONS,
        };
        table
            .iter()
            .any(|&(from, target)| from == self && target == to)
    }

    /// 从存储词形解析（013 CHECK 域之外的值拒绝）。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        Self::ALL
            .into_iter()
            .find(|s| s.as_str() == value)
            .ok_or_else(|| DomainError::UnknownMemoryLifecycle(value.to_string()))
    }
}

impl fmt::Display for MemoryLifecycle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 研究记忆回链类型（013 refs_json 注释原文三型）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryRefType {
    /// 装配清单条目（context_manifest_item，需 manifest_id + item_id）。
    ManifestItem,
    /// 产物（005 artifact 表）。
    Artifact,
    /// 证据（001 evidence 表）。
    Evidence,
}

impl MemoryRefType {
    /// 存储词形（refs_json ref_type 值）。
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryRefType::ManifestItem => "manifest_item",
            MemoryRefType::Artifact => "artifact",
            MemoryRefType::Evidence => "evidence",
        }
    }

    /// 从存储词形解析。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "manifest_item" => Ok(MemoryRefType::ManifestItem),
            "artifact" => Ok(MemoryRefType::Artifact),
            "evidence" => Ok(MemoryRefType::Evidence),
            other => Err(DomainError::InvalidAgentMemory {
                field: "refs_json.ref_type".to_string(),
                reason: format!("unknown ref type {other:?}"),
            }),
        }
    }
}

impl fmt::Display for MemoryRefType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 研究记忆回链引用（每条 Research Memory 至少一条，可解析回链）。
///
/// URI 形态（[`Self::to_uri`] / [`Self::parse`]）：
/// `mem://manifest/{manifest_id}/{item_id}`、`mem://artifact/{artifact_id}`、
/// `mem://evidence/{evidence_id}`。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MemoryObjectRef {
    ref_type: MemoryRefType,
    /// 仅 ManifestItem：装配清单 id（context_manifest.manifest_id）。
    manifest_id: Option<String>,
    /// Artifact/Evidence：对象 id；ManifestItem：清单条目 item_id。
    ref_id: String,
}

impl MemoryObjectRef {
    /// 内部构造（id 非空且不超过 128 字符）。
    fn new(
        ref_type: MemoryRefType,
        manifest_id: Option<String>,
        ref_id: String,
    ) -> Result<Self, DomainError> {
        if let Some(m) = &manifest_id {
            if !mem_id_ok(m) {
                return Err(DomainError::InvalidAgentMemory {
                    field: "refs_json.manifest_id".to_string(),
                    reason: format!("manifest id {m:?} must be non-empty and within 128 chars"),
                });
            }
        }
        if !mem_id_ok(&ref_id) {
            return Err(DomainError::InvalidAgentMemory {
                field: "refs_json.ref_id".to_string(),
                reason: format!("ref id {ref_id:?} must be non-empty and within 128 chars"),
            });
        }
        Ok(Self {
            ref_type,
            manifest_id,
            ref_id,
        })
    }

    /// 构造 manifest item 回链（manifest_id 与 item_id 均必填）。
    pub fn manifest_item(
        manifest_id: impl Into<String>,
        item_id: impl Into<String>,
    ) -> Result<Self, DomainError> {
        Self::new(
            MemoryRefType::ManifestItem,
            Some(manifest_id.into()),
            item_id.into(),
        )
    }

    /// 构造 artifact 回链。
    pub fn artifact(id: impl Into<String>) -> Result<Self, DomainError> {
        Self::new(MemoryRefType::Artifact, None, id.into())
    }

    /// 构造 evidence 回链。
    pub fn evidence(id: impl Into<String>) -> Result<Self, DomainError> {
        Self::new(MemoryRefType::Evidence, None, id.into())
    }

    /// 回链类型。
    pub fn ref_type(&self) -> MemoryRefType {
        self.ref_type
    }

    /// 装配清单 id（仅 ManifestItem）。
    pub fn manifest_id(&self) -> Option<&str> {
        self.manifest_id.as_deref()
    }

    /// 对象 id（ManifestItem 时为 item_id）。
    pub fn ref_id(&self) -> &str {
        &self.ref_id
    }

    /// URI 形态。
    pub fn to_uri(&self) -> String {
        match self.ref_type {
            MemoryRefType::ManifestItem => format!(
                "mem://manifest/{}/{}",
                self.manifest_id.as_deref().unwrap_or(""),
                self.ref_id
            ),
            MemoryRefType::Artifact => format!("mem://artifact/{}", self.ref_id),
            MemoryRefType::Evidence => format!("mem://evidence/{}", self.ref_id),
        }
    }

    /// 从 URI 解析（三型三形态，其余拒绝）。
    pub fn parse(uri: &str) -> Result<Self, DomainError> {
        let reject = |reason: String| {
            Err(DomainError::InvalidAgentMemory {
                field: "refs_json.uri".to_string(),
                reason,
            })
        };
        let Some(rest) = uri.strip_prefix("mem://") else {
            return reject(format!("uri {uri:?} must start with mem://"));
        };
        let segs: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
        match segs.as_slice() {
            ["manifest", manifest_id, item_id] => {
                Self::manifest_item(manifest_id.to_string(), item_id.to_string())
            }
            ["artifact", id] => Self::artifact(id.to_string()),
            ["evidence", id] => Self::evidence(id.to_string()),
            _ => reject(format!(
                "uri {uri:?} must be mem://manifest/{{mid}}/{{iid}}, mem://artifact/{{id}} or mem://evidence/{{id}}"
            )),
        }
    }
}

/// Memory id/id 段字符校验：非空且不超过 128 字符。
fn mem_id_ok(value: &str) -> bool {
    !value.is_empty() && value.chars().count() <= 128
}

/// sha256 快照校验：64 位小写 hex。
fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

/// Task Memory 条目（agent_task_memory 行域对象；run 内工作记忆）。
///
/// 载荷不变式：ACTIVE 详情在场；COMPRESSED 详情置空且摘要非空；EVICTED
/// 详情与摘要全置空（墓碑行）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskMemoryEntry {
    memory_id: String,
    run_id: RunId,
    task_id: Option<String>,
    round: u32,
    content_json: String,
    summary: String,
    lifecycle: MemoryLifecycle,
}

impl TaskMemoryEntry {
    /// 校验并构造（载荷与生命周期不变式不一致即拒绝）。
    pub fn try_new(
        memory_id: impl Into<String>,
        run_id: RunId,
        task_id: Option<String>,
        round: u32,
        content_json: impl Into<String>,
        summary: impl Into<String>,
        lifecycle: MemoryLifecycle,
    ) -> Result<Self, DomainError> {
        let memory_id = memory_id.into();
        let content_json = content_json.into();
        let summary = summary.into();
        if !mem_id_ok(&memory_id) {
            return Err(DomainError::InvalidAgentMemory {
                field: "memory_id".to_string(),
                reason: format!("memory id {memory_id:?} must be non-empty and within 128 chars"),
            });
        }
        if let Some(t) = &task_id {
            if !mem_id_ok(t) {
                return Err(DomainError::InvalidAgentMemory {
                    field: "task_id".to_string(),
                    reason: format!("task id {t:?} must be non-empty and within 128 chars"),
                });
            }
        }
        let consistent = match lifecycle {
            MemoryLifecycle::Active => !content_json.trim().is_empty(),
            MemoryLifecycle::Compressed => content_json.is_empty() && !summary.trim().is_empty(),
            MemoryLifecycle::Evicted => content_json.is_empty() && summary.is_empty(),
            MemoryLifecycle::Archived => false,
        };
        if !consistent {
            return Err(DomainError::InvalidAgentMemory {
                field: "content_json/summary".to_string(),
                reason: format!(
                    "payload inconsistent with lifecycle {lifecycle} for task memory {memory_id:?}"
                ),
            });
        }
        Ok(Self {
            memory_id,
            run_id,
            task_id,
            round,
            content_json,
            summary,
            lifecycle,
        })
    }

    /// memory id。
    pub fn memory_id(&self) -> &str {
        &self.memory_id
    }

    /// 归属 run。
    pub fn run_id(&self) -> &RunId {
        &self.run_id
    }

    /// 关联任务。
    pub fn task_id(&self) -> Option<&str> {
        self.task_id.as_deref()
    }

    /// 产生轮次。
    pub fn round(&self) -> u32 {
        self.round
    }

    /// 详情载荷（JSON 原文）。
    pub fn content_json(&self) -> &str {
        &self.content_json
    }

    /// 摘要。
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// 生命周期。
    pub fn lifecycle(&self) -> MemoryLifecycle {
        self.lifecycle
    }

    /// 生命周期推进（域校验 [`MemoryLifecycle::can_transition_kind`]，非法对拒绝）。
    pub fn transition_to(mut self, to: MemoryLifecycle) -> Result<Self, DomainError> {
        let from = self.lifecycle;
        if !from.can_transition_kind(MemoryKind::Task, to) {
            return Err(DomainError::InvalidMemoryTransition {
                kind: MemoryKind::Task.as_str().to_string(),
                from: from.as_str().to_string(),
                to: to.as_str().to_string(),
            });
        }
        // 载荷随生命周期推进重整（与不变式一致）。
        match to {
            MemoryLifecycle::Compressed => {
                self.content_json = String::new();
                if self.summary.trim().is_empty() {
                    self.summary = format!("compressed task memory (round {})", self.round);
                }
            }
            MemoryLifecycle::Evicted => {
                self.content_json = String::new();
                self.summary = String::new();
            }
            _ => {}
        }
        self.lifecycle = to;
        Ok(self)
    }
}

/// Evidence Memory 条目（agent_evidence_memory 行域对象；证据/产物引用台账）。
///
/// 反重复建模：本条目只承载指针（evidence/artifact id）与登记时刻快照
/// （content_hash/storage_ref），不承载 evidence 实体字段；生命周期仅
/// ACTIVE/COMPRESSED（驱逐/归档在构造即拒绝）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceMemoryEntry {
    memory_id: String,
    run_id: RunId,
    evidence_id: String,
    artifact_id: Option<ArtifactId>,
    content_hash: String,
    storage_ref: String,
    lifecycle: MemoryLifecycle,
}

impl EvidenceMemoryEntry {
    /// 校验并构造（hash 须 64 位小写 hex；快照字段非空；EVICTED/ARCHIVED 拒绝）。
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        memory_id: impl Into<String>,
        run_id: RunId,
        evidence_id: impl Into<String>,
        artifact_id: Option<ArtifactId>,
        content_hash: impl Into<String>,
        storage_ref: impl Into<String>,
        lifecycle: MemoryLifecycle,
    ) -> Result<Self, DomainError> {
        let memory_id = memory_id.into();
        let evidence_id = evidence_id.into();
        let content_hash = content_hash.into();
        let storage_ref = storage_ref.into();
        if !mem_id_ok(&memory_id) {
            return Err(DomainError::InvalidAgentMemory {
                field: "memory_id".to_string(),
                reason: format!("memory id {memory_id:?} must be non-empty and within 128 chars"),
            });
        }
        if !mem_id_ok(&evidence_id) {
            return Err(DomainError::InvalidAgentMemory {
                field: "evidence_id".to_string(),
                reason: format!(
                    "evidence id {evidence_id:?} must be non-empty and within 128 chars"
                ),
            });
        }
        if !is_sha256_hex(&content_hash) {
            return Err(DomainError::InvalidAgentMemory {
                field: "content_hash".to_string(),
                reason: format!("content hash {content_hash:?} must be 64 lowercase hex chars"),
            });
        }
        if storage_ref.trim().is_empty() {
            return Err(DomainError::InvalidAgentMemory {
                field: "storage_ref".to_string(),
                reason: "storage ref must be non-empty".to_string(),
            });
        }
        if !matches!(
            lifecycle,
            MemoryLifecycle::Active | MemoryLifecycle::Compressed
        ) {
            return Err(DomainError::InvalidMemoryTransition {
                kind: MemoryKind::Evidence.as_str().to_string(),
                from: MemoryLifecycle::Active.as_str().to_string(),
                to: lifecycle.as_str().to_string(),
            });
        }
        Ok(Self {
            memory_id,
            run_id,
            evidence_id,
            artifact_id,
            content_hash,
            storage_ref,
            lifecycle,
        })
    }

    /// memory id。
    pub fn memory_id(&self) -> &str {
        &self.memory_id
    }

    /// 归属 run。
    pub fn run_id(&self) -> &RunId {
        &self.run_id
    }

    /// 证据指针（权威行在 001 evidence 表）。
    pub fn evidence_id(&self) -> &str {
        &self.evidence_id
    }

    /// 同哈希对账的产物指针。
    pub fn artifact_id(&self) -> Option<&ArtifactId> {
        self.artifact_id.as_ref()
    }

    /// 登记时刻 content_hash 快照（漂移检测钉面）。
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }

    /// 登记时刻解析入口快照（= evidence.storage_ref）。
    pub fn storage_ref(&self) -> &str {
        &self.storage_ref
    }

    /// 生命周期。
    pub fn lifecycle(&self) -> MemoryLifecycle {
        self.lifecycle
    }

    /// 生命周期推进（仅 ACTIVE→COMPRESSED；驱逐/归档拒绝）。
    pub fn transition_to(mut self, to: MemoryLifecycle) -> Result<Self, DomainError> {
        let from = self.lifecycle;
        if !from.can_transition_kind(MemoryKind::Evidence, to) {
            return Err(DomainError::InvalidMemoryTransition {
                kind: MemoryKind::Evidence.as_str().to_string(),
                from: from.as_str().to_string(),
                to: to.as_str().to_string(),
            });
        }
        self.lifecycle = to;
        Ok(self)
    }
}

/// Research Memory 条目（agent_research_memory 行域对象；Context Memory 雏形）。
///
/// 载荷不变式：ACTIVE 详情在场且回链非空；COMPRESSED/ARCHIVED 详情置空，
/// 标题/摘要/回链原样保留（可追溯面）；EVICTED 在构造即拒绝（成果永不驱逐）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchMemoryEntry {
    memory_id: String,
    run_id: RunId,
    task_id: Option<String>,
    agent_id: AgentId,
    title: String,
    summary: String,
    content_json: String,
    refs: Vec<MemoryObjectRef>,
    lifecycle: MemoryLifecycle,
}

impl ResearchMemoryEntry {
    /// 校验并构造（标题/摘要非空，回链至少一条，EVICTED 拒绝）。
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        memory_id: impl Into<String>,
        run_id: RunId,
        task_id: Option<String>,
        agent_id: AgentId,
        title: impl Into<String>,
        summary: impl Into<String>,
        content_json: impl Into<String>,
        refs: Vec<MemoryObjectRef>,
        lifecycle: MemoryLifecycle,
    ) -> Result<Self, DomainError> {
        let memory_id = memory_id.into();
        let title = title.into();
        let summary = summary.into();
        let content_json = content_json.into();
        if !mem_id_ok(&memory_id) {
            return Err(DomainError::InvalidAgentMemory {
                field: "memory_id".to_string(),
                reason: format!("memory id {memory_id:?} must be non-empty and within 128 chars"),
            });
        }
        if title.trim().is_empty() || title.chars().count() > 256 {
            return Err(DomainError::InvalidAgentMemory {
                field: "title".to_string(),
                reason: "title must be non-empty and within 256 chars".to_string(),
            });
        }
        if summary.trim().is_empty() {
            return Err(DomainError::InvalidAgentMemory {
                field: "summary".to_string(),
                reason: "summary must be non-empty".to_string(),
            });
        }
        if refs.is_empty() {
            return Err(DomainError::InvalidAgentMemory {
                field: "refs_json".to_string(),
                reason: "research memory requires at least one resolvable back-link".to_string(),
            });
        }
        let consistent = match lifecycle {
            MemoryLifecycle::Active => !content_json.trim().is_empty(),
            MemoryLifecycle::Compressed | MemoryLifecycle::Archived => content_json.is_empty(),
            MemoryLifecycle::Evicted => false,
        };
        if !consistent {
            return Err(DomainError::InvalidAgentMemory {
                field: "content_json".to_string(),
                reason: format!(
                    "payload inconsistent with lifecycle {lifecycle} for research memory {memory_id:?}"
                ),
            });
        }
        Ok(Self {
            memory_id,
            run_id,
            task_id,
            agent_id,
            title,
            summary,
            content_json,
            refs,
            lifecycle,
        })
    }

    /// memory id。
    pub fn memory_id(&self) -> &str {
        &self.memory_id
    }

    /// 来源 run。
    pub fn run_id(&self) -> &RunId {
        &self.run_id
    }

    /// 关联任务。
    pub fn task_id(&self) -> Option<&str> {
        self.task_id.as_deref()
    }

    /// 产出者 agent id。
    pub fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }

    /// 成果标题。
    pub fn title(&self) -> &str {
        &self.title
    }

    /// 成果摘要。
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// 详情载荷（JSON 原文）。
    pub fn content_json(&self) -> &str {
        &self.content_json
    }

    /// 回链列表（压缩/归档不动）。
    pub fn refs(&self) -> &[MemoryObjectRef] {
        &self.refs
    }

    /// 生命周期。
    pub fn lifecycle(&self) -> MemoryLifecycle {
        self.lifecycle
    }

    /// 生命周期推进（压缩/归档保留 title/summary/refs，仅置空详情；驱逐拒绝）。
    pub fn transition_to(mut self, to: MemoryLifecycle) -> Result<Self, DomainError> {
        let from = self.lifecycle;
        if !from.can_transition_kind(MemoryKind::Research, to) {
            return Err(DomainError::InvalidMemoryTransition {
                kind: MemoryKind::Research.as_str().to_string(),
                from: from.as_str().to_string(),
                to: to.as_str().to_string(),
            });
        }
        if matches!(to, MemoryLifecycle::Compressed | MemoryLifecycle::Archived) {
            self.content_json = String::new();
        }
        self.lifecycle = to;
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_id_validation() {
        assert!(AgentId::try_new("agent.researcher").is_ok());
        assert!(AgentId::try_new("").is_err());
        assert!(AgentId::try_new("a".repeat(AgentId::MAX_LEN + 1)).is_err());
        let id = AgentId::try_new("a1").unwrap();
        assert_eq!(id.as_str(), "a1");
        assert_eq!(id.to_string(), "a1");
    }

    #[test]
    fn capability_ref_shape_and_semver() {
        assert!(CapabilityRef::try_new("rip.doc.parse", None).is_ok());
        // P1-7b 修订后的段内下划线合法。
        assert!(CapabilityRef::try_new("rip.kb.rtis", Some("0.1.0".to_string())).is_ok());
        assert!(CapabilityRef::try_new("Rip.doc", None).is_err());
        assert!(CapabilityRef::try_new("ripdoc", None).is_err());
        assert!(CapabilityRef::try_new("rip.doc", Some("1.2".to_string())).is_err());
    }

    #[test]
    fn definition_builds_and_rejects_bad_fields() {
        let id = AgentId::try_new("agent.researcher").unwrap();
        let refs = vec![
            CapabilityRef::try_new("rip.doc.parse", None).unwrap(),
            CapabilityRef::try_new("rip.kb.rtis", Some("0.1.0".to_string())).unwrap(),
        ];
        let def = AgentDefinition::try_new(
            id.clone(),
            "0.1.0".to_string(),
            "{\"l0_rules\":true}".to_string(),
            refs,
        )
        .unwrap();
        assert_eq!(def.agent_id(), &id);
        assert_eq!(def.capability_refs().len(), 2);

        assert!(
            AgentDefinition::try_new(id.clone(), "0.1".to_string(), "{}".to_string(), vec![])
                .is_err()
        );
        assert!(
            AgentDefinition::try_new(id, "0.1.0".to_string(), "  ".to_string(), vec![]).is_err()
        );
    }

    #[test]
    fn binding_validation_rejects_unknown_tool() {
        // P7-1 判据：指向不存在工具即拒绝。
        let known = ["rip.doc.parse", "rip.kb.rtis"];
        let exists = |t: &str| known.contains(&t);
        let def = AgentDefinition::try_new(
            AgentId::try_new("agent.a").unwrap(),
            "0.1.0".to_string(),
            "{}".to_string(),
            vec![
                CapabilityRef::try_new("rip.doc.parse", None).unwrap(),
                CapabilityRef::try_new("rip.kb.rtis", None).unwrap(),
            ],
        )
        .unwrap();
        assert!(def.validate_capability_refs(&exists).is_ok());

        let bad = AgentDefinition::try_new(
            AgentId::try_new("agent.b").unwrap(),
            "0.1.0".to_string(),
            "{}".to_string(),
            vec![CapabilityRef::try_new("rip.ghost.tool", None).unwrap()],
        )
        .unwrap();
        let err = bad.validate_capability_refs(&exists).unwrap_err();
        assert!(
            matches!(err, DomainError::UnknownCapabilityTarget { ref tool_id } if tool_id == "rip.ghost.tool"),
            "err: {err:?}"
        );
        assert!(err.to_string().contains("rip.ghost.tool"));
    }

    #[test]
    fn run_state_vocabulary_and_terminal_flags() {
        let stored: Vec<&str> = AgentRunState::ALL.iter().map(|s| s.as_str()).collect();
        assert_eq!(
            stored,
            vec![
                "QUEUED",
                "PAUSED",
                "RUNNING",
                "SUCCEEDED",
                "FAILED",
                "CANCELLED"
            ]
        );
        assert!(!AgentRunState::Paused.is_terminal());
        assert!(AgentRunState::Cancelled.is_terminal());
        assert_eq!(AgentRunState::Running.to_string(), "RUNNING");
    }

    #[test]
    fn run_builds_and_rejects_bad_fields() {
        let run = AgentRun::try_new(
            RunId::try_new("rn-agent-1").unwrap(),
            Some("task-42".to_string()),
            AgentId::try_new("agent.researcher").unwrap(),
            "0.1.0".to_string(),
            AgentRunState::Queued,
            vec!["mem://evidence/1".to_string()],
        )
        .unwrap();
        assert_eq!(run.state(), AgentRunState::Queued);
        assert_eq!(run.task_id(), Some("task-42"));

        assert!(AgentRun::try_new(
            RunId::try_new("rn-agent-2").unwrap(),
            Some("  ".to_string()),
            AgentId::try_new("agent.a").unwrap(),
            "0.1.0".to_string(),
            AgentRunState::Queued,
            vec![]
        )
        .is_err());
        assert!(AgentRun::try_new(
            RunId::try_new("rn-agent-3").unwrap(),
            None,
            AgentId::try_new("agent.a").unwrap(),
            "x.y.z".to_string(),
            AgentRunState::Queued,
            vec![]
        )
        .is_err());
        assert!(AgentRun::try_new(
            RunId::try_new("rn-agent-4").unwrap(),
            None,
            AgentId::try_new("agent.a").unwrap(),
            "0.1.0".to_string(),
            AgentRunState::Queued,
            vec![String::new()]
        )
        .is_err());
    }

    #[test]
    fn transition_table_covers_pause_resume_and_cancel() {
        assert_eq!(AgentRunState::LEGAL_TRANSITIONS.len(), 8);
        for (from, to) in AgentRunState::LEGAL_TRANSITIONS {
            assert!(from.can_transition(to), "{from:?} -> {to:?} must be legal");
        }
        // 暂停恢复（计划 §14:821）与取消面。
        assert!(AgentRunState::Running.can_transition(AgentRunState::Paused));
        assert!(AgentRunState::Paused.can_transition(AgentRunState::Running));
        assert!(AgentRunState::Queued.can_transition(AgentRunState::Cancelled));
        assert!(AgentRunState::Paused.can_transition(AgentRunState::Cancelled));
    }

    #[test]
    fn unlisted_or_terminal_transitions_rejected() {
        assert!(!AgentRunState::Queued.can_transition(AgentRunState::Succeeded));
        assert!(!AgentRunState::Queued.can_transition(AgentRunState::Failed));
        assert!(!AgentRunState::Queued.can_transition(AgentRunState::Paused));
        assert!(!AgentRunState::Paused.can_transition(AgentRunState::Succeeded));
        assert!(!AgentRunState::Paused.can_transition(AgentRunState::Failed));
        for t in [
            AgentRunState::Succeeded,
            AgentRunState::Failed,
            AgentRunState::Cancelled,
        ] {
            for to in AgentRunState::ALL {
                assert!(
                    !t.can_transition(to),
                    "terminal {t:?} -> {to:?} must be illegal"
                );
            }
        }
        // 自迁移非法。
        for s in AgentRunState::ALL {
            assert!(
                !s.can_transition(s),
                "self transition {s:?} must be illegal"
            );
        }
    }

    #[test]
    fn cooperative_request_predicates_match_dtc04() {
        // 暂停：仅 RUNNING 可发起协作式请求（P7-4 控制面谓词）。
        assert!(AgentRunState::Running.pause_requestable());
        for s in [
            AgentRunState::Queued,
            AgentRunState::Paused,
            AgentRunState::Succeeded,
            AgentRunState::Failed,
            AgentRunState::Cancelled,
        ] {
            assert!(
                !s.pause_requestable(),
                "{s:?} must not be pause_requestable"
            );
        }
        // 取消：QUEUED/RUNNING/PAUSED 可发起（后两者之外直接跃迁），终态不可。
        assert!(AgentRunState::Queued.cancel_requestable());
        assert!(AgentRunState::Running.cancel_requestable());
        assert!(AgentRunState::Paused.cancel_requestable());
        for s in [
            AgentRunState::Succeeded,
            AgentRunState::Failed,
            AgentRunState::Cancelled,
        ] {
            assert!(
                !s.cancel_requestable(),
                "{s:?} must not be cancel_requestable"
            );
        }
    }

    #[test]
    fn run_transition_to_and_memory_ref_append() {
        let run = AgentRun::try_new(
            RunId::try_new("arn-t1").unwrap(),
            None,
            AgentId::try_new("agent.a").unwrap(),
            "0.1.0".to_string(),
            AgentRunState::Queued,
            vec![],
        )
        .unwrap();
        let running = run.clone().transition_to(AgentRunState::Running).unwrap();
        assert_eq!(running.state(), AgentRunState::Running);
        assert_eq!(running.run_id(), run.run_id());

        // 非法对：QUEUED 直接 SUCCEEDED。
        let err = run
            .clone()
            .transition_to(AgentRunState::Succeeded)
            .unwrap_err();
        assert!(
            matches!(err, DomainError::InvalidAgentTransition { ref from, ref to }
                if from == "QUEUED" && to == "SUCCEEDED"),
            "err: {err:?}"
        );

        // 终态不可迁出。
        let done = running.transition_to(AgentRunState::Succeeded).unwrap();
        assert!(done.clone().transition_to(AgentRunState::Failed).is_err());

        // 记忆引用追加去重，空串拒绝。
        let with_mem = done.with_memory_ref("mem://manifest/m1").unwrap();
        let deduped = with_mem
            .clone()
            .with_memory_ref("mem://manifest/m1")
            .unwrap();
        assert_eq!(deduped.memory_refs(), vec!["mem://manifest/m1"]);
        assert!(deduped.with_memory_ref("  ").is_err());
    }

    #[test]
    fn memory_lifecycle_tables_refuse_evidence_and_research_eviction() {
        // 各类合法对全部可迁移。
        for (from, to) in MemoryLifecycle::TASK_TRANSITIONS {
            assert!(from.can_transition_kind(MemoryKind::Task, to));
        }
        for (from, to) in MemoryLifecycle::EVIDENCE_TRANSITIONS {
            assert!(from.can_transition_kind(MemoryKind::Evidence, to));
        }
        for (from, to) in MemoryLifecycle::RESEARCH_TRANSITIONS {
            assert!(from.can_transition_kind(MemoryKind::Research, to));
        }
        // 契约面（计划 §9:478）：Evidence 无驱逐/归档，Research 无驱逐。
        assert!(!MemoryLifecycle::Active
            .can_transition_kind(MemoryKind::Evidence, MemoryLifecycle::Evicted));
        assert!(!MemoryLifecycle::Active
            .can_transition_kind(MemoryKind::Evidence, MemoryLifecycle::Archived));
        assert!(!MemoryLifecycle::Active
            .can_transition_kind(MemoryKind::Research, MemoryLifecycle::Evicted));
        // 跨类词值拒绝：Task 的 EVICTED 对 Evidence 类不合法。
        assert!(!MemoryLifecycle::Active
            .can_transition_kind(MemoryKind::Evidence, MemoryLifecycle::Evicted));
        // 终态不可迁出、自迁移非法（逐类核验）。
        for kind in MemoryKind::ALL {
            for t in [MemoryLifecycle::Evicted, MemoryLifecycle::Archived] {
                if matches!(kind, MemoryKind::Evidence | MemoryKind::Research)
                    && t == MemoryLifecycle::Evicted
                {
                    continue; // 该类本就无 EVICTED 词值
                }
                for s in MemoryLifecycle::ALL {
                    assert!(
                        !s.can_transition_kind(kind, s),
                        "self transition {s:?} on {kind:?} must be illegal"
                    );
                    if s.is_terminal() {
                        assert!(
                            !s.can_transition_kind(kind, t),
                            "terminal {s:?} on {kind:?} must not transition"
                        );
                    }
                }
            }
        }
        // 存储词形解析：未知值拒绝。
        assert!(matches!(
            MemoryLifecycle::parse("PURGED"),
            Err(DomainError::UnknownMemoryLifecycle(v)) if v == "PURGED"
        ));
    }

    #[test]
    fn task_memory_entry_payload_invariants_and_transitions() {
        let run = RunId::try_new("arn-mem1").unwrap();
        let entry = TaskMemoryEntry::try_new(
            "mem-t-1",
            run.clone(),
            Some("task-42".to_string()),
            0,
            r#"{"tool":"rip.kb.keyword_search"}"#.to_string(),
            "",
            MemoryLifecycle::Active,
        )
        .unwrap();
        assert_eq!(entry.round(), 0);

        // 压缩：详情置空、摘要留存；再驱逐：摘要置空（墓碑）。
        let compressed = entry
            .clone()
            .transition_to(MemoryLifecycle::Compressed)
            .unwrap();
        assert!(compressed.content_json().is_empty());
        assert!(!compressed.summary().is_empty());
        let evicted = compressed.transition_to(MemoryLifecycle::Evicted).unwrap();
        assert!(evicted.content_json().is_empty() && evicted.summary().is_empty());
        // 墓碑不可再推进。
        assert!(evicted
            .clone()
            .transition_to(MemoryLifecycle::Active)
            .is_err());

        // 载荷不变式拒绝：ACTIVE 空详情、EVICTED 带摘要、ARCHIVED 出词值。
        assert!(TaskMemoryEntry::try_new(
            "mem-t-2",
            run.clone(),
            None,
            1,
            "",
            "",
            MemoryLifecycle::Active
        )
        .is_err());
        assert!(TaskMemoryEntry::try_new(
            "mem-t-3",
            run.clone(),
            None,
            1,
            "",
            "leftover",
            MemoryLifecycle::Evicted
        )
        .is_err());
        assert!(TaskMemoryEntry::try_new(
            "mem-t-4",
            run,
            None,
            1,
            "{}",
            "",
            MemoryLifecycle::Archived
        )
        .is_err());
    }

    #[test]
    fn evidence_memory_entry_rejects_eviction_and_bad_hash() {
        let run = RunId::try_new("arn-mem2").unwrap();
        let entry = EvidenceMemoryEntry::try_new(
            "mem-e-1",
            run.clone(),
            "ev-879d",
            Some(ArtifactId::try_new("art-e45e").unwrap()),
            "a".repeat(64),
            "ripartifact://rn-1/art-e45e",
            MemoryLifecycle::Active,
        )
        .unwrap();
        assert_eq!(entry.content_hash().len(), 64);
        // 仅压缩可推进。
        let compressed = entry
            .clone()
            .transition_to(MemoryLifecycle::Compressed)
            .unwrap();
        assert_eq!(compressed.lifecycle(), MemoryLifecycle::Compressed);
        assert!(compressed.transition_to(MemoryLifecycle::Evicted).is_err());

        // 构造期拒绝：EVICTED 出词值、坏 hash、空 storage_ref。
        assert!(EvidenceMemoryEntry::try_new(
            "mem-e-2",
            run.clone(),
            "ev-x",
            None,
            "a".repeat(64),
            "ref",
            MemoryLifecycle::Evicted
        )
        .is_err());
        assert!(EvidenceMemoryEntry::try_new(
            "mem-e-3",
            run.clone(),
            "ev-x",
            None,
            "DEADBEEF",
            "ref",
            MemoryLifecycle::Active
        )
        .is_err());
        assert!(EvidenceMemoryEntry::try_new(
            "mem-e-4",
            run,
            "ev-x",
            None,
            "a".repeat(64),
            "  ",
            MemoryLifecycle::Active
        )
        .is_err());
    }

    #[test]
    fn research_memory_entry_requires_refs_and_keeps_them_through_compress() {
        let run = RunId::try_new("arn-mem3").unwrap();
        let agent = AgentId::try_new("agent.research.minimal").unwrap();
        let refs = vec![
            MemoryObjectRef::manifest_item("ctxm-1", "ki-1").unwrap(),
            MemoryObjectRef::artifact("art-1").unwrap(),
            MemoryObjectRef::evidence("ev-1").unwrap(),
        ];
        let entry = ResearchMemoryEntry::try_new(
            "mem-r-1",
            run,
            Some("task-7".to_string()),
            agent,
            "nmap 检索成果",
            "命中工具与知识各一组",
            r#"{"steps":1}"#.to_string(),
            refs.clone(),
            MemoryLifecycle::Active,
        )
        .unwrap();
        assert_eq!(entry.refs().len(), 3);

        // 压缩：详情置空，标题/摘要/回链原样（可追溯面）。
        let compressed = entry.transition_to(MemoryLifecycle::Compressed).unwrap();
        assert!(compressed.content_json().is_empty());
        assert_eq!(compressed.title(), "nmap 检索成果");
        assert_eq!(compressed.refs().len(), 3);
        assert!(compressed.transition_to(MemoryLifecycle::Evicted).is_err());

        // 构造期拒绝：无回链、EVICTED、压缩态带详情。
        assert!(ResearchMemoryEntry::try_new(
            "mem-r-2",
            RunId::try_new("arn-mem4").unwrap(),
            None,
            AgentId::try_new("agent.a").unwrap(),
            "t",
            "s",
            "{}",
            vec![],
            MemoryLifecycle::Active
        )
        .is_err());
        assert!(ResearchMemoryEntry::try_new(
            "mem-r-3",
            RunId::try_new("arn-mem5").unwrap(),
            None,
            AgentId::try_new("agent.a").unwrap(),
            "t",
            "s",
            "{}",
            refs,
            MemoryLifecycle::Evicted
        )
        .is_err());
    }

    #[test]
    fn memory_object_ref_uri_roundtrip() {
        let m = MemoryObjectRef::manifest_item("ctxm-9", "ki-7").unwrap();
        assert_eq!(m.to_uri(), "mem://manifest/ctxm-9/ki-7");
        let a = MemoryObjectRef::artifact("art-e45e").unwrap();
        assert_eq!(a.to_uri(), "mem://artifact/art-e45e");
        let e = MemoryObjectRef::evidence("ev-879d").unwrap();
        assert_eq!(e.to_uri(), "mem://evidence/ev-879d");

        assert_eq!(
            MemoryObjectRef::parse("mem://manifest/ctxm-9/ki-7").unwrap(),
            m
        );
        assert_eq!(
            MemoryObjectRef::parse("mem://artifact/art-e45e").unwrap(),
            a
        );
        assert_eq!(MemoryObjectRef::parse("mem://evidence/ev-879d").unwrap(), e);

        // 非法形态：非 mem:// 前缀、未知命名空间、manifest 单段、空 id。
        assert!(MemoryObjectRef::parse("http://x/1").is_err());
        assert!(MemoryObjectRef::parse("mem://chat/c1").is_err());
        assert!(MemoryObjectRef::parse("mem://manifest/ctxm-9").is_err());
        assert!(MemoryObjectRef::parse("mem://artifact/").is_err());
        // ref_type 词形解析。
        assert_eq!(
            MemoryRefType::parse("manifest_item").unwrap(),
            MemoryRefType::ManifestItem
        );
        assert!(MemoryRefType::parse("chat").is_err());
    }
}
