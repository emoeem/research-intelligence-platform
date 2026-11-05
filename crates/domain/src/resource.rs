//! Resource 实体与双轴状态枚举（UNIFIED_DOMAIN_MODEL §7.1/§7.2，K-08/K-16 修正后口径）。

use crate::error::DomainError;
use crate::ids::ResourceId;
use std::fmt;

/// 生命周期轴：处理阶段链（schema CHECK 约束口径，K-16 含验证维度）。
///
/// 与 UNIFIED_DOMAIN_MODEL §7.1 的完整链相比，M1 schema 将
/// NORMALIZED/DEDUPLICATED/CLASSIFIED/EVALUATED 折叠进 CANDIDATE 前后阶段，
/// 以 dryrun 库 CHECK 约束为准。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LifecycleStatus {
    /// 刚发现，未抓取。
    Discovered,
    /// 内容已抓取。
    Fetched,
    /// 已解析。
    Parsed,
    /// 已规范化并入候选。
    Candidate,
    /// 已验证。
    Verified,
    /// 已入索引（目录稳态）。
    Indexed,
    /// 上游长期无更新，待重扫。
    Stale,
}

impl LifecycleStatus {
    /// 全部变体（按生命周期链顺序）。
    pub const ALL: [LifecycleStatus; 7] = [
        LifecycleStatus::Discovered,
        LifecycleStatus::Fetched,
        LifecycleStatus::Parsed,
        LifecycleStatus::Candidate,
        LifecycleStatus::Verified,
        LifecycleStatus::Indexed,
        LifecycleStatus::Stale,
    ];

    /// schema 内存储值（大写下划线风格，与 001_init.sql CHECK 一致）。
    pub fn as_str(&self) -> &'static str {
        match self {
            LifecycleStatus::Discovered => "DISCOVERED",
            LifecycleStatus::Fetched => "FETCHED",
            LifecycleStatus::Parsed => "PARSED",
            LifecycleStatus::Candidate => "CANDIDATE",
            LifecycleStatus::Verified => "VERIFIED",
            LifecycleStatus::Indexed => "INDEXED",
            LifecycleStatus::Stale => "STALE",
        }
    }

    /// 从 schema 存储值解析。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        let lower = value.to_ascii_uppercase();
        Self::ALL
            .into_iter()
            .find(|s| s.as_str() == lower)
            .ok_or_else(|| DomainError::UnknownLifecycleVariant(value.to_string()))
    }
}

impl fmt::Display for LifecycleStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for LifecycleStatus {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        LifecycleStatus::parse(s)
    }
}

/// 治理轴：治理状态，与生命周期正交（K-08）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GovernanceStatus {
    /// 正常可见。
    Active,
    /// 隔离（D5 高风险未审等），resolve 不可见。
    Quarantined,
    /// 已拒绝，resolve 不可见。
    Rejected,
    /// 归档（保留历史）。
    Archived,
}

impl GovernanceStatus {
    /// 全部变体。
    pub const ALL: [GovernanceStatus; 4] = [
        GovernanceStatus::Active,
        GovernanceStatus::Quarantined,
        GovernanceStatus::Rejected,
        GovernanceStatus::Archived,
    ];

    /// schema 内存储值。
    pub fn as_str(&self) -> &'static str {
        match self {
            GovernanceStatus::Active => "ACTIVE",
            GovernanceStatus::Quarantined => "QUARANTINED",
            GovernanceStatus::Rejected => "REJECTED",
            GovernanceStatus::Archived => "ARCHIVED",
        }
    }

    /// 从 schema 存储值解析。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        let lower = value.to_ascii_uppercase();
        Self::ALL
            .into_iter()
            .find(|s| s.as_str() == lower)
            .ok_or_else(|| DomainError::UnknownGovernanceVariant(value.to_string()))
    }
}

impl fmt::Display for GovernanceStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for GovernanceStatus {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        GovernanceStatus::parse(s)
    }
}

/// 质量等级（P1 引擎产出的 A/B/C/D）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum QualityGrade {
    /// A 级。
    A,
    /// B 级。
    B,
    /// C 级。
    C,
    /// D 级。
    D,
}

impl QualityGrade {
    /// 存储值。
    pub fn as_str(&self) -> &'static str {
        match self {
            QualityGrade::A => "A",
            QualityGrade::B => "B",
            QualityGrade::C => "C",
            QualityGrade::D => "D",
        }
    }

    /// 从存储值解析。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value.to_ascii_uppercase().as_str() {
            "A" => Ok(QualityGrade::A),
            "B" => Ok(QualityGrade::B),
            "C" => Ok(QualityGrade::C),
            "D" => Ok(QualityGrade::D),
            other => Err(DomainError::UnknownQualityGrade(other.to_string())),
        }
    }
}

impl fmt::Display for QualityGrade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Resource 实体（M2 收编字段集）。
///
/// 说明：platform 库 resource 表共 40+ 列，含 quality_* 六维明细与
/// last_checked / last_source_commit 等溯源列。本批只收编 resolve 与
/// 实体身份所需字段，其余明细列待 M1 第二批字段映射评审后收编，
/// 避免提前承诺未评审字段。
#[derive(Debug, Clone, PartialEq)]
pub struct Resource {
    /// 稳定 ID（`gh:owner/repo` / `url:hash16`）。
    pub resource_id: ResourceId,
    /// 目录内别名 ID（RES-*），M1 未迁移，暂空。
    pub identity: Option<String>,
    /// 规范化 URL。
    pub canonical_url: String,
    /// 资源类型（github_repo / url 等）。
    pub resource_type: String,
    /// 名称。
    pub name: String,
    /// 描述。
    pub description: Option<String>,
    /// 所有者（GitHub owner）。
    pub owner: Option<String>,
    /// 仓库名。
    pub repo_name: Option<String>,
    /// 生命周期轴。
    pub lifecycle: LifecycleStatus,
    /// 治理轴。
    pub governance: GovernanceStatus,
    /// 源 status 原值（保真线索）。
    pub source_status: Option<String>,
    /// 源 P1 综合分（quality_score）。
    pub quality_score: Option<f64>,
    /// 源 trust_level（A/B/C/D）。
    pub trust_level: Option<QualityGrade>,
    /// 折入字段：质量终分。
    pub quality_final_score: Option<f64>,
    /// 折入字段：质量等级。
    pub quality_grade: Option<QualityGrade>,
    /// 折入字段：是否因质量原因禁止采纳为依赖。
    pub quality_adoption_blocked: Option<bool>,
    /// 折入字段：评分理由（JSON 文本）。
    pub quality_reasons: Option<String>,
    /// 首见时间。
    pub first_seen: Option<String>,
    /// 末见时间。
    pub last_seen: Option<String>,
    /// 来源系统标识。
    pub source_system: String,
    /// 创建时间。
    pub created_at: String,
    /// 更新时间。
    pub updated_at: String,
    /// schema 版本。
    pub schema_version: i64,
}

impl Resource {
    /// 治理轴是否对 resolve 可见（QUARANTINED / REJECTED / ARCHIVED 不可见）。
    pub fn is_resolvable(&self) -> bool {
        self.governance == GovernanceStatus::Active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_roundtrip() {
        for s in LifecycleStatus::ALL {
            assert_eq!(LifecycleStatus::parse(s.as_str()).unwrap(), s);
        }
        assert!(LifecycleStatus::parse("NO_SUCH").is_err());
    }

    #[test]
    fn governance_roundtrip() {
        for s in GovernanceStatus::ALL {
            assert_eq!(GovernanceStatus::parse(s.as_str()).unwrap(), s);
        }
        assert!(GovernanceStatus::parse("nope").is_err());
    }

    #[test]
    fn grade_parse_and_order() {
        assert_eq!(QualityGrade::parse("b").unwrap(), QualityGrade::B);
        assert!(QualityGrade::parse("E").is_err());
        assert!(QualityGrade::A < QualityGrade::D);
    }

    #[test]
    fn resolvable_only_when_active() {
        let base = || Resource {
            resource_id: ResourceId::try_new("gh:o/r").unwrap(),
            identity: None,
            canonical_url: "https://example.com".to_string(),
            resource_type: "github_repo".to_string(),
            name: "n".to_string(),
            description: None,
            owner: None,
            repo_name: None,
            lifecycle: LifecycleStatus::Indexed,
            governance: GovernanceStatus::Active,
            source_status: None,
            quality_score: None,
            trust_level: None,
            quality_final_score: None,
            quality_grade: None,
            quality_adoption_blocked: None,
            quality_reasons: None,
            first_seen: None,
            last_seen: None,
            source_system: "eri-catalog".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            schema_version: 1,
        };
        assert!(base().is_resolvable());
        let mut q = base();
        q.governance = GovernanceStatus::Quarantined;
        assert!(!q.is_resolvable());
    }
}
