//! 注册表错误类型（纯 std 手写 Display，与 domain::error 同风格，无 thiserror）。

use std::fmt;

/// TOOL-CONTRACT §2.5 校验九条编号（拒绝时定位到条）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationRule {
    /// 第 1 条：顶层必填字段齐全且 schema_version 为受支持主版本。
    RequiredFields,
    /// 第 2 条：id 不冲突且 version 不回退（§6.2 semver）。
    VersionComparable,
    /// 第 3 条：input_schema / output_schema 是合法 JSON Schema 文档。
    IoSchema,
    /// 第 4 条：runtime 词表与镜像 digest 固定，remote_runner 拒绝。
    Runtime,
    /// 第 5 条：license 非 unknown，image_digest 与 runtime.image 一致。
    Provenance,
    /// 第 6 条：resources.timeout_seconds 与 runtime 上限不冲突。
    Resources,
    /// 第 7 条：secrets[].name 在平台 secrets 存储中存在。
    Secrets,
    /// 第 8 条：examples 逐条合 input_schema / output_schema。
    Examples,
    /// 第 9 条：落库写 tool + tool_version（持久化语义条目）。
    Persistence,
}

impl fmt::Display for ValidationRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = match self {
            ValidationRule::RequiredFields => 1,
            ValidationRule::VersionComparable => 2,
            ValidationRule::IoSchema => 3,
            ValidationRule::Runtime => 4,
            ValidationRule::Provenance => 5,
            ValidationRule::Resources => 6,
            ValidationRule::Secrets => 7,
            ValidationRule::Examples => 8,
            ValidationRule::Persistence => 9,
        };
        write!(f, "rule {n} ({self:?})")
    }
}

/// 注册表错误。
#[derive(Debug, Clone, PartialEq)]
pub enum RegistryError {
    /// 校验拒绝（rule 定位到 §2.5 九条之一）。
    Validation {
        /// 违反的条目。
        rule: ValidationRule,
        /// 涉及的工具 id（可缺）。
        tool_id: Option<String>,
        /// 拒绝原因。
        reason: String,
    },
    /// 存储/查询路径找不到目标。
    NotFound {
        /// 工具 id。
        tool_id: String,
        /// 工具版本（缺省指 current）。
        version: Option<String>,
    },
    /// 底层存储错误。
    Storage(String),
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegistryError::Validation {
                rule,
                tool_id,
                reason,
            } => match tool_id {
                Some(id) => write!(
                    f,
                    "manifest validation rejected [{rule}] for {id}: {reason}"
                ),
                None => write!(f, "manifest validation rejected [{rule}]: {reason}"),
            },
            RegistryError::NotFound { tool_id, version } => match version {
                Some(v) => write!(f, "tool {tool_id} version {v} not found"),
                None => write!(f, "tool {tool_id} not found"),
            },
            RegistryError::Storage(reason) => write!(f, "registry storage error: {reason}"),
        }
    }
}

impl std::error::Error for RegistryError {}

impl From<database::DbError> for RegistryError {
    fn from(e: database::DbError) -> Self {
        RegistryError::Storage(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_display_names_each_of_nine() {
        let rules = [
            ValidationRule::RequiredFields,
            ValidationRule::VersionComparable,
            ValidationRule::IoSchema,
            ValidationRule::Runtime,
            ValidationRule::Provenance,
            ValidationRule::Resources,
            ValidationRule::Secrets,
            ValidationRule::Examples,
            ValidationRule::Persistence,
        ];
        for (i, r) in rules.iter().enumerate() {
            let s = r.to_string();
            assert!(s.starts_with(&format!("rule {} ", i + 1)), "display: {s}");
        }
    }
}
