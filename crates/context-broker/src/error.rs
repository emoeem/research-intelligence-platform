//! Context Broker 错误类型（纯 std 手写 Display，与 domain::error 同风格，
//! 无 thiserror；锁定最少依赖集 DEC-001）。

use std::fmt;

/// Context Broker 错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextBrokerError {
    /// 预算策略非法（加载或校验拒绝；field 定位到策略字段，设计稿 §5.1）。
    Policy {
        /// 被拒绝的策略字段名（policy_version/total_tokens/shares/...）。
        field: String,
        /// 拒绝原因。
        reason: String,
    },
    /// 装配或展开输入非法（空 id、越界值等；P4-1 仅策略面使用，
    /// assemble/expand 面在 P4-4 起扩展使用）。
    InvalidInput {
        /// 被拒绝的输入字段名。
        field: String,
        /// 拒绝原因。
        reason: String,
    },
    /// 上游源取数失败（ContextSource 实现的 IO、解析或查询错误；
    /// P4-2 起由 rulepack/registry/plugin 源使用，设计稿 §10 source.rs 面）。
    Source {
        /// 源 id（ContextSource::id，如 rulepack/registry/plugin）。
        source: String,
        /// 失败原因。
        reason: String,
    },
}

impl fmt::Display for ContextBrokerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContextBrokerError::Policy { field, reason } => {
                write!(f, "budget policy rejected for {field:?}: {reason}")
            }
            ContextBrokerError::InvalidInput { field, reason } => {
                write!(f, "invalid context broker input {field:?}: {reason}")
            }
            ContextBrokerError::Source { source, reason } => {
                write!(f, "context source {source:?} failed: {reason}")
            }
        }
    }
}

impl std::error::Error for ContextBrokerError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_names_each_variant_with_field_context() {
        let policy_err = ContextBrokerError::Policy {
            field: "shares".to_string(),
            reason: "must sum to 1.0".to_string(),
        };
        assert!(
            policy_err
                .to_string()
                .contains("budget policy rejected for \"shares\""),
            "display: {policy_err}"
        );
        let input_err = ContextBrokerError::InvalidInput {
            field: "manifest_id".to_string(),
            reason: "must not be empty".to_string(),
        };
        assert!(
            input_err
                .to_string()
                .contains("invalid context broker input \"manifest_id\""),
            "display: {input_err}"
        );
    }
}
