//! 域错误类型（纯 std，无 thiserror 依赖，锁定最少依赖集）。

use std::fmt;

/// 域层错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    /// Resource ID 不符合 `gh:owner/repo` / `url:hash16` 稳定 ID 口径。
    InvalidResourceId {
        /// 被拒绝的原始值。
        value: String,
        /// 拒绝原因。
        reason: String,
    },
    /// 生命周期轴出现未知变体（schema CHECK 约束之外的值）。
    UnknownLifecycleVariant(String),
    /// 治理轴出现未知变体。
    UnknownGovernanceVariant(String),
    /// 质量等级出现未知变体（合法：A/B/C/D）。
    UnknownQualityGrade(String),
    /// 审计事件缺必填字段（actor / action）。
    InvalidAuditEvent(String),
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DomainError::InvalidResourceId { value, reason } => {
                write!(f, "invalid resource id {value:?}: {reason}")
            }
            DomainError::UnknownLifecycleVariant(v) => {
                write!(f, "unknown lifecycle variant: {v:?}")
            }
            DomainError::UnknownGovernanceVariant(v) => {
                write!(f, "unknown governance variant: {v:?}")
            }
            DomainError::UnknownQualityGrade(v) => {
                write!(f, "unknown quality grade: {v:?}")
            }
            DomainError::InvalidAuditEvent(reason) => {
                write!(f, "invalid audit event: {reason}")
            }
        }
    }
}

impl std::error::Error for DomainError {}
