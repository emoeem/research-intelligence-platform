//! RIP3 情报核领域类型：Document 域 ID 与状态词表（03-DOMAIN-MODEL.md 定稿，
//! P1-2 起落地 PostgreSQL；本模块只放纯类型，SQL 面在 crates/intel-db）。
//!
//! ID 惯例沿用 ids.rs（前缀小写连字符为服务端实现细节，域层只校验非空与
//! 长度上限；前缀在 intel-db 生成侧钉死：doc-/drev-/blk-/src-）。

use crate::error::DomainError;
use std::fmt;

/// 生成文档域新类型 ID 的宏（校验非空与 128 上限，Display/AsRef/TryFrom 全套）。
macro_rules! document_id_type {
    ($(#[$doc:meta])* $name:ident, $err_variant:ident, $desc:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            /// 长度上限（与平台六 ID 口径一致，ids.rs MAX_LEN=128）。
            pub const MAX_LEN: usize = 128;

            /// 校验并构造：非空且不超过 MAX_LEN。
            pub fn try_new(value: impl Into<String>) -> Result<Self, DomainError> {
                let value = value.into();
                let reason = if value.is_empty() {
                    concat!("empty ", $desc, " id").to_string()
                } else if value.chars().count() > Self::MAX_LEN {
                    format!(concat!($desc, " id exceeds {} chars"), Self::MAX_LEN)
                } else {
                    return Ok(Self(value));
                };
                Err(DomainError::$err_variant { value, reason })
            }

            /// 内部值视图。
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// 消费取值。
            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = DomainError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::try_new(value)
            }
        }

        impl TryFrom<&str> for $name {
            type Error = DomainError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                Self::try_new(value.to_string())
            }
        }
    };
}

document_id_type!(
    /// 文档 ID（doc- 前缀，03-DOMAIN-MODEL 1.1）。
    DocumentId,
    InvalidDocumentId,
    "document"
);
document_id_type!(
    /// 文档版本 ID（drev- 前缀，03-DOMAIN-MODEL 1.2）。
    DocumentRevisionId,
    InvalidDocumentRevisionId,
    "document revision"
);
document_id_type!(
    /// 结构块 ID（blk- 前缀，03-DOMAIN-MODEL 1.3）。
    BlockId,
    InvalidBlockId,
    "document block"
);
document_id_type!(
    /// 来源记录 ID（src- 前缀，03-DOMAIN-MODEL 1.8）。
    SourceRecordId,
    InvalidSourceRecordId,
    "source record"
);

/// 文档可见性词表（03-DOMAIN-MODEL 1.1；检索入口前置过滤键，13-SECURITY 第五节）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentVisibility {
    /// 私有（缺省；不出工作区）。
    Private,
    /// 工作区共享。
    Shared,
    /// 公开。
    Public,
}

impl DocumentVisibility {
    /// 存储词值。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Shared => "shared",
            Self::Public => "public",
        }
    }

    /// 词值解析（fail-closed：未知词拒绝）。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "private" => Ok(Self::Private),
            "shared" => Ok(Self::Shared),
            "public" => Ok(Self::Public),
            other => Err(DomainError::InvalidDocumentVisibility {
                value: other.to_string(),
            }),
        }
    }
}

/// 文档生命周期（软删除优先，R2 10.4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentLifecycle {
    /// 在用。
    Active,
    /// 已软删（可恢复）。
    Deleted,
}

impl DocumentLifecycle {
    /// 存储词值。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::Deleted => "DELETED",
        }
    }
}

/// 版本状态（旧版本不因上传新版本被删除，R2 3.1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevisionStatus {
    /// 当前版本。
    Current,
    /// 已被新版本取代（历史可访问）。
    Superseded,
}

impl RevisionStatus {
    /// 存储词值。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Current => "CURRENT",
            Self::Superseded => "SUPERSEDED",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_id_roundtrip() {
        let id = DocumentId::try_new("doc-abc123").unwrap();
        assert_eq!(id.as_str(), "doc-abc123");
        assert_eq!(id.to_string(), "doc-abc123");
    }

    #[test]
    fn document_id_rejects_empty_and_oversize() {
        assert!(DocumentId::try_new("").is_err());
        let long = "x".repeat(129);
        assert!(DocumentId::try_new(long).is_err());
    }

    #[test]
    fn visibility_parse_is_fail_closed() {
        assert_eq!(DocumentVisibility::parse("private").unwrap(), DocumentVisibility::Private);
        assert!(DocumentVisibility::parse("internal").is_err());
    }

    #[test]
    fn lifecycle_and_revision_words() {
        assert_eq!(DocumentLifecycle::Active.as_str(), "ACTIVE");
        assert_eq!(RevisionStatus::Superseded.as_str(), "SUPERSEDED");
    }
}
