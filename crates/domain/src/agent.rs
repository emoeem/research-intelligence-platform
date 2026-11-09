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

use crate::error::DomainError;
use crate::ids::RunId;
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
}
