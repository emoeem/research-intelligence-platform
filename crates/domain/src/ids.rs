//! 实体 ID 新类型。
//!
//! 口径（UNIFIED_DOMAIN_MODEL §7.1 / migrations/001_init.sql）：Resource 主键沿用
//! 现网稳定 ID，不生成新平台 ID：GitHub 资源 `gh:owner/repo`，其余 `url:hash16`。

use crate::error::DomainError;
use std::fmt;

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
}
