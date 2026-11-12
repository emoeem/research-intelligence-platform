//! 实体 ID 新类型。
//!
//! 口径（UNIFIED_DOMAIN_MODEL §7.1 / migrations/001_init.sql）：Resource 主键沿用
//! 现网稳定 ID，不生成新平台 ID：GitHub 资源 `gh:owner/repo`，其余 `url:hash16`。
//!
//! P7-5 六 ID 集中定义（SESSION-HANDOFF §十追加项 1：trace 统一关联与跨栈
//! 查询）：本模块集中承载六个关联 ID 的类型定义，全仓一律引用此处，
//! 不散落字面量：
//!
//! 1. [`TraceId`]：链路追踪 ID（新类型；agent_run/tool_run/artifact/evidence/
//!    audit_event 五面 trace_id 列的写面口径，迁移 015）。
//! 2. [`TaskId`]：研究任务 ID（research_task.task_id / tool_run.task_id /
//!    evidence.task_ref）。
//! 3. [`AgentRunId`]：Agent 运行 ID（agent_run.run_id；类型别名收口）。
//! 4. [`ToolRunId`]：工具运行 ID（tool_run.run_id；类型别名收口）。
//! 5. [`ArtifactId`]：产物 ID（artifact.artifact_id）。
//! 6. [`EvidenceId`]：证据 ID（evidence.evidence_id）。
//!
//! AgentRunId / ToolRunId 取类型别名而非独立新类型：两张 run 表的 run_id
//! 共享同一冻结契约形状（schemas/tool-contract/request-v1.json run_id，
//! min/maxLength 1..128，TOOL-CONTRACT §4.1），域层 [`RunId`] 单类型校验
//! 即覆盖两族，避免为别名重复实现；语义区分靠别名与本注释承载，跨 run 族
//! 的歧义（如 trace 查询按 ID 探测）由查询层按表探测消解（agent-engine
//! trace 模块）。

use crate::error::DomainError;
use std::fmt;

/// 关联 ID 通用校验上限（与冻结契约 run_id maxLength 一致，P7-5 六 ID
/// 统一口径：非空且不超过 128 字符）。
pub(crate) const CORRELATION_ID_MAX_LEN: usize = 128;

/// 通用长度校验（六 ID 共用：非空且不超过 [`CORRELATION_ID_MAX_LEN`]）。
/// 各新类型的 `try_new` 直接内联同口径检查，本函数保留为词表语义锚。
#[allow(dead_code)]
pub(crate) fn check_correlation_id(value: &str, kind: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("empty {kind} id"));
    }
    if value.chars().count() > CORRELATION_ID_MAX_LEN {
        return Err(format!("{kind} id exceeds {CORRELATION_ID_MAX_LEN} chars"));
    }
    Ok(())
}

/// Resource 稳定 ID。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResourceId(String);

impl ResourceId {
    /// 校验并构造。合法前缀：`gh:` 或 `url:`，且冒号后内容非空。
    pub fn try_new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let reason = match value.as_str() {
            "" => "empty id".to_string(),
            v if v.starts_with("gh:") && v[3..].trim().is_empty() => {
                "gh: prefix requires owner/repo".to_string()
            }
            v if v.starts_with("url:") && v[4..].trim().is_empty() => {
                "url: prefix requires hash16".to_string()
            }
            v if !(v.starts_with("gh:") || v.starts_with("url:")) => {
                "must start with gh: or url:".to_string()
            }
            _ => return Ok(Self(value)),
        };
        Err(DomainError::InvalidResourceId { value, reason })
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

impl fmt::Display for ResourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for ResourceId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ResourceId {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        ResourceId::try_new(value)
    }
}

impl TryFrom<&str> for ResourceId {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        ResourceId::try_new(value.to_string())
    }
}

/// ToolRun 运行 ID（服务端生成，全局唯一，审计链 correlation_id）。
///
/// 口径（TOOL-CONTRACT §4.1；本仓冻结契约 schemas/tool-contract/request-v1.json
/// run_id：minLength 1，maxLength 128）：非空且不超过 128 字符，不做格式强约束，
/// 生成策略（如 `rn-` 前缀 ULID）属服务端派发器实现细节。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RunId(String);

impl RunId {
    /// 长度上限（与 request-v1.json run_id maxLength 一致）。
    pub const MAX_LEN: usize = 128;

    /// 校验并构造：非空且不超过 [`RunId::MAX_LEN`] 字符。
    pub fn try_new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let reason = if value.is_empty() {
            "empty run id".to_string()
        } else if value.chars().count() > Self::MAX_LEN {
            format!("run id exceeds {} chars", Self::MAX_LEN)
        } else {
            return Ok(Self(value));
        };
        Err(DomainError::InvalidRunId { value, reason })
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

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for RunId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for RunId {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        RunId::try_new(value)
    }
}

impl TryFrom<&str> for RunId {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        RunId::try_new(value.to_string())
    }
}

/// Artifact 全局唯一 ID（服务端登记时生成，TOOL-CONTRACT §5.2）。
///
/// 口径（本仓冻结契约 schemas/tool-contract/result-v1.json $defs.artifact
/// artifact_id：minLength 1）：非空即可，生成与解析格式属 Artifact Manager
/// 实现细节，域层不做强约束。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ArtifactId(String);

impl ArtifactId {
    /// 校验并构造：非空。
    pub fn try_new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        if value.is_empty() {
            return Err(DomainError::InvalidArtifactId {
                value,
                reason: "empty artifact id".to_string(),
            });
        }
        Ok(Self(value))
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

impl fmt::Display for ArtifactId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for ArtifactId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ArtifactId {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        ArtifactId::try_new(value)
    }
}

impl TryFrom<&str> for ArtifactId {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        ArtifactId::try_new(value.to_string())
    }
}

/// TraceId：链路追踪 ID（P7-5 新类型）。
///
/// 一次研究动作跨实体族的关联锚：agent_run / tool_run / artifact / evidence /
/// audit_event 行的 trace_id 列（迁移 015）统一落本 ID，audit-engine 写面
/// 注入（correlation_id 体系扩展）。生成策略（`tr-` 前缀 + 引擎内 id 派生）
/// 属服务端实现细节，域层只做非空与长度校验；跨 run 语义（重试链共享
/// 同一 trace_id）由编排层（agent-engine RunControl::retry 继承）承载。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TraceId(String);

impl TraceId {
    /// 长度上限（与 [`CORRELATION_ID_MAX_LEN`] 一致）。
    pub const MAX_LEN: usize = CORRELATION_ID_MAX_LEN;

    /// 校验并构造：非空且不超过 [`TraceId::MAX_LEN`] 字符。
    pub fn try_new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let reason = if value.is_empty() {
            "empty trace id".to_string()
        } else if value.chars().count() > Self::MAX_LEN {
            format!("trace id exceeds {} chars", Self::MAX_LEN)
        } else {
            return Ok(Self(value));
        };
        Err(DomainError::InvalidTraceId { value, reason })
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

impl fmt::Display for TraceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for TraceId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for TraceId {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        TraceId::try_new(value)
    }
}

impl TryFrom<&str> for TraceId {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        TraceId::try_new(value.to_string())
    }
}

/// TaskId：研究任务 ID（P7-5 新类型）。
///
/// research_task.task_id（现网稳定 ID `rtis-YYYYMMDD-HHMMSS-ms`，002 迁移）
/// 与 tool_run.task_id / evidence.task_ref / agent_run.task_id 的统一校验
/// 口径：非空且不超过 128 字符（现网形状更细，域层不做强约束）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaskId(String);

impl TaskId {
    /// 长度上限。
    pub const MAX_LEN: usize = CORRELATION_ID_MAX_LEN;

    /// 校验并构造：非空且不超过 [`TaskId::MAX_LEN`] 字符。
    pub fn try_new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let reason = if value.is_empty() {
            "empty task id".to_string()
        } else if value.chars().count() > Self::MAX_LEN {
            format!("task id exceeds {} chars", Self::MAX_LEN)
        } else {
            return Ok(Self(value));
        };
        Err(DomainError::InvalidTaskId { value, reason })
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

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for TaskId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for TaskId {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        TaskId::try_new(value)
    }
}

impl TryFrom<&str> for TaskId {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        TaskId::try_new(value.to_string())
    }
}

/// EvidenceId：证据 ID（P7-5 新类型）。
///
/// evidence.evidence_id（001 迁移；登记面生成 `ev-` 前缀引擎内 id）：
/// 非空且不超过 128 字符。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EvidenceId(String);

impl EvidenceId {
    /// 长度上限。
    pub const MAX_LEN: usize = CORRELATION_ID_MAX_LEN;

    /// 校验并构造：非空且不超过 [`EvidenceId::MAX_LEN`] 字符。
    pub fn try_new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let reason = if value.is_empty() {
            "empty evidence id".to_string()
        } else if value.chars().count() > Self::MAX_LEN {
            format!("evidence id exceeds {} chars", Self::MAX_LEN)
        } else {
            return Ok(Self(value));
        };
        Err(DomainError::InvalidEvidenceId { value, reason })
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

impl fmt::Display for EvidenceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for EvidenceId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for EvidenceId {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        EvidenceId::try_new(value)
    }
}

impl TryFrom<&str> for EvidenceId {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        EvidenceId::try_new(value.to_string())
    }
}

/// AgentRunId：Agent 运行 ID（agent_run.run_id）。
///
/// 类型别名收口（本模块头注「六 ID 集中定义」）：与 [`ToolRunId`] 共享
/// [`RunId`] 的冻结契约校验形状。
pub type AgentRunId = RunId;

/// ToolRunId：工具运行 ID（tool_run.run_id）。
///
/// 类型别名收口（本模块头注「六 ID 集中定义」）：与 [`AgentRunId`] 共享
/// [`RunId`] 的冻结契约校验形状。
pub type ToolRunId = RunId;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_stable_prefixes() {
        assert!(ResourceId::try_new("gh:owner/repo").is_ok());
        assert!(ResourceId::try_new("url:0123456789abcdef").is_ok());
    }

    #[test]
    fn rejects_other_prefixes_and_empty() {
        assert!(ResourceId::try_new("").is_err());
        assert!(ResourceId::try_new("poc:xx").is_err());
        assert!(ResourceId::try_new("gh:").is_err());
        assert!(ResourceId::try_new("url:").is_err());
    }

    #[test]
    fn display_roundtrip() {
        let id = ResourceId::try_new("gh:a/b").unwrap();
        assert_eq!(id.to_string(), "gh:a/b");
        assert_eq!(id.as_str(), "gh:a/b");
    }

    // ---- P7-5 六 ID 集中类型 ----

    #[test]
    fn trace_id_accepts_and_rejects() {
        assert!(TraceId::try_new("tr-abc123").is_ok());
        assert!(TraceId::try_new("").is_err());
        let long = "x".repeat(CORRELATION_ID_MAX_LEN + 1);
        assert!(TraceId::try_new(long).is_err());
        let ok = "x".repeat(CORRELATION_ID_MAX_LEN);
        assert!(TraceId::try_new(ok).is_ok());
    }

    #[test]
    fn task_id_accepts_and_rejects() {
        assert!(TaskId::try_new("rtis-20261113-010203-001").is_ok());
        assert!(TaskId::try_new("").is_err());
        assert!(TaskId::try_new("x".repeat(CORRELATION_ID_MAX_LEN + 1)).is_err());
    }

    #[test]
    fn evidence_id_accepts_and_rejects() {
        assert!(EvidenceId::try_new("ev-abc123").is_ok());
        assert!(EvidenceId::try_new("").is_err());
        assert!(EvidenceId::try_new("x".repeat(CORRELATION_ID_MAX_LEN + 1)).is_err());
    }

    #[test]
    fn run_family_aliases_share_run_id_shape() {
        // AgentRunId / ToolRunId 是 RunId 别名：同一构造与校验面。
        let run: ToolRunId = RunId::try_new("rn-1").unwrap();
        let agent: AgentRunId = run.clone();
        assert_eq!(run.as_str(), "rn-1");
        assert_eq!(agent.as_str(), "rn-1");
        assert!(ToolRunId::try_new("").is_err());
        assert!(AgentRunId::try_new("x".repeat(RunId::MAX_LEN + 1)).is_err());
    }
}
