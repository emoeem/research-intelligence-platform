//! RIP3 情报核领域类型：KnowledgeCard 治理域（03-DOMAIN-MODEL.md 1.5/1.6 定稿，
//! Phase 3 起落地 PostgreSQL；本模块只放纯类型与质量分纯函数，SQL 面在
//! crates/intel-db card_repo）。
//!
//! 与既有 [`crate::knowledge`]（SK-1 摄取面，SQLite knowledge_source_entry）
//! 的分工：本模块 = RIP3 KnowledgeCard/Relation 治理域（PG，ADR-0001 绞杀者
//! 路线新增面）；RTIS 摄取面保持现状不动（03 号冻结物第四节边界）。
//!
//! ID 惯例沿用 document.rs 的宏模式（kc-/krel- 前缀为 intel-db 生成侧细节）；
//! 词表单一来源 = 本文件（R2 5.2/5.3/5.4 + 03 号冻结物 1.5/1.6）。

use crate::error::DomainError;
use std::fmt;

/// 生成治理域新类型 ID 的宏（与 document.rs 同构：非空 + 128 上限）。
macro_rules! kcard_id_type {
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

kcard_id_type!(
    /// 知识卡片 ID（kc- 前缀，03-DOMAIN-MODEL 1.5）。
    KnowledgeCardId,
    InvalidKnowledgeCardId,
    "knowledge card"
);
kcard_id_type!(
    /// 知识关系 ID（krel- 前缀，03-DOMAIN-MODEL 1.6）。
    KnowledgeRelationId,
    InvalidKnowledgeRelationId,
    "knowledge relation"
);

/// 知识类型词表（03-DOMAIN-MODEL 1.5；方法论知识，漏洞条目/POC 属 RTIS）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KnowledgeType {
    /// 漏洞原理。
    VulnPrinciple,
    /// 漏洞特征。
    VulnSignature,
    /// 利用条件。
    ExploitCondition,
    /// 检测方法。
    DetectionMethod,
    /// 修复方案。
    FixMethod,
    /// 安全技术。
    SecurityTechnique,
    /// 工具使用。
    ToolUsage,
    /// 研究案例。
    ResearchCase,
}

impl KnowledgeType {
    /// 存储词形（V0004 CHECK 域同词值）。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::VulnPrinciple => "vuln_principle",
            Self::VulnSignature => "vuln_signature",
            Self::ExploitCondition => "exploit_condition",
            Self::DetectionMethod => "detection_method",
            Self::FixMethod => "fix_method",
            Self::SecurityTechnique => "security_technique",
            Self::ToolUsage => "tool_usage",
            Self::ResearchCase => "research_case",
        }
    }

    /// 词表解析（未知值拒绝，fail-closed）。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        let words: [(&str, Self); 8] = [
            ("vuln_principle", Self::VulnPrinciple),
            ("vuln_signature", Self::VulnSignature),
            ("exploit_condition", Self::ExploitCondition),
            ("detection_method", Self::DetectionMethod),
            ("fix_method", Self::FixMethod),
            ("security_technique", Self::SecurityTechnique),
            ("tool_usage", Self::ToolUsage),
            ("research_case", Self::ResearchCase),
        ];
        words
            .iter()
            .find(|(w, _)| *w == value)
            .map(|(_, v)| *v)
            .ok_or_else(|| DomainError::InvalidKnowledgeType(value.to_string()))
    }
}

impl fmt::Display for KnowledgeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 审核状态词表（R2 5.4 七态；未经批准默认不进正式研究上下文）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReviewStatus {
    /// 发现（导入/抽取即得）。
    Discovered,
    /// 已解析。
    Parsed,
    /// 候选（待审核；低质量与风险命中停此态）。
    Candidate,
    /// 已批准（可进正式研究上下文）。
    Approved,
    /// 隔离（污染/低质量候选）。
    Quarantined,
    /// 已拒绝。
    Rejected,
    /// 已废弃。
    Deprecated,
}

impl ReviewStatus {
    /// 存储词形（V0004 CHECK 域同词值）。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Discovered => "DISCOVERED",
            Self::Parsed => "PARSED",
            Self::Candidate => "CANDIDATE",
            Self::Approved => "APPROVED",
            Self::Quarantined => "QUARANTINED",
            Self::Rejected => "REJECTED",
            Self::Deprecated => "DEPRECATED",
        }
    }

    /// 词表解析。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        let words: [(&str, Self); 7] = [
            ("DISCOVERED", Self::Discovered),
            ("PARSED", Self::Parsed),
            ("CANDIDATE", Self::Candidate),
            ("APPROVED", Self::Approved),
            ("QUARANTINED", Self::Quarantined),
            ("REJECTED", Self::Rejected),
            ("DEPRECATED", Self::Deprecated),
        ];
        words
            .iter()
            .find(|(w, _)| *w == value)
            .map(|(_, v)| *v)
            .ok_or_else(|| DomainError::InvalidReviewStatus(value.to_string()))
    }
}

/// 验证状态词表（原则 C：AI 派生内容必须标记与验证状态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerificationStatus {
    /// 未验证。
    Unverified,
    /// 已验证。
    Verified,
}

impl VerificationStatus {
    /// 存储词形。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unverified => "UNVERIFIED",
            Self::Verified => "VERIFIED",
        }
    }

    /// 词表解析。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "UNVERIFIED" => Ok(Self::Unverified),
            "VERIFIED" => Ok(Self::Verified),
            other => Err(DomainError::InvalidVerificationStatus(other.to_string())),
        }
    }
}

/// 知识关系类型词表（R2 3.1 十型；D4 语义相似只产候选边不自动合并）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelationType {
    /// 同内容（D2 规范化哈希一致）。
    SameContent,
    /// 近似重复（D3 MinHash+LSH 候选）。
    NearDuplicate,
    /// 相关。
    RelatedTo,
    /// 支持。
    Supports,
    /// 矛盾（D5：进审核而非删一方）。
    Contradicts,
    /// 取代。
    Supersedes,
    /// 修复。
    Fixes,
    /// 检测。
    Detects,
    /// 派生自。
    DerivedFrom,
    /// 引用。
    References,
}

impl RelationType {
    /// 存储词形（V0004 CHECK 域同词值）。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SameContent => "SAME_CONTENT",
            Self::NearDuplicate => "NEAR_DUPLICATE",
            Self::RelatedTo => "RELATED_TO",
            Self::Supports => "SUPPORTS",
            Self::Contradicts => "CONTRADICTS",
            Self::Supersedes => "SUPERSEDES",
            Self::Fixes => "FIXES",
            Self::Detects => "DETECTS",
            Self::DerivedFrom => "DERIVED_FROM",
            Self::References => "REFERENCES",
        }
    }

    /// 词表解析。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        let words: [(&str, Self); 10] = [
            ("SAME_CONTENT", Self::SameContent),
            ("NEAR_DUPLICATE", Self::NearDuplicate),
            ("RELATED_TO", Self::RelatedTo),
            ("SUPPORTS", Self::Supports),
            ("CONTRADICTS", Self::Contradicts),
            ("SUPERSEDES", Self::Supersedes),
            ("FIXES", Self::Fixes),
            ("DETECTS", Self::Detects),
            ("DERIVED_FROM", Self::DerivedFrom),
            ("REFERENCES", Self::References),
        ];
        words
            .iter()
            .find(|(w, _)| *w == value)
            .map(|(_, v)| *v)
            .ok_or_else(|| DomainError::InvalidRelationType(value.to_string()))
    }
}

/// 风险标记词表（R2 5.3 七类；独立于质量总分，命中即候选进审核）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RiskFlag {
    /// 来源不可信。
    UntrustedSource,
    /// 许可证未知。
    LicenseUnknown,
    /// 疑似提示词注入。
    PossiblePromptInjection,
    /// 已过时。
    Outdated,
    /// 被矛盾（D5 命中）。
    Contradicted,
    /// 抽取置信度低。
    LowExtractionConfidence,
    /// 未验证断言。
    UnverifiedClaim,
}

impl RiskFlag {
    /// 存储词形（quality_flags 数组元素）。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UntrustedSource => "UNTRUSTED_SOURCE",
            Self::LicenseUnknown => "LICENSE_UNKNOWN",
            Self::PossiblePromptInjection => "POSSIBLE_PROMPT_INJECTION",
            Self::Outdated => "OUTDATED",
            Self::Contradicted => "CONTRADICTED",
            Self::LowExtractionConfidence => "LOW_EXTRACTION_CONFIDENCE",
            Self::UnverifiedClaim => "UNVERIFIED_CLAIM",
        }
    }

    /// 词表解析。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        let words: [(&str, Self); 7] = [
            ("UNTRUSTED_SOURCE", Self::UntrustedSource),
            ("LICENSE_UNKNOWN", Self::LicenseUnknown),
            ("POSSIBLE_PROMPT_INJECTION", Self::PossiblePromptInjection),
            ("OUTDATED", Self::Outdated),
            ("CONTRADICTED", Self::Contradicted),
            ("LOW_EXTRACTION_CONFIDENCE", Self::LowExtractionConfidence),
            ("UNVERIFIED_CLAIM", Self::UnverifiedClaim),
        ];
        words
            .iter()
            .find(|(w, _)| *w == value)
            .map(|(_, v)| *v)
            .ok_or_else(|| DomainError::InvalidRiskFlag(value.to_string()))
    }

    /// 硬性隔离标记：命中即禁 APPROVED（13 号冻结物第三节）。
    pub fn is_hard_block(self) -> bool {
        matches!(self, Self::PossiblePromptInjection)
    }
}

/// 证据等级词表（对齐 RTIS k_meta K0-K4 语义，00-审计缺口 4 清账面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EvidenceLevel {
    /// K0 捕获。
    K0,
    /// K1 调研。
    K1,
    /// K2 证据支撑。
    K2,
    /// K3 实验验证。
    K3,
    /// K4 可复用。
    K4,
}

impl EvidenceLevel {
    /// 存储词形。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::K0 => "K0",
            Self::K1 => "K1",
            Self::K2 => "K2",
            Self::K3 => "K3",
            Self::K4 => "K4",
        }
    }

    /// 词表解析。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "K0" => Ok(Self::K0),
            "K1" => Ok(Self::K1),
            "K2" => Ok(Self::K2),
            "K3" => Ok(Self::K3),
            "K4" => Ok(Self::K4),
            other => Err(DomainError::InvalidEvidenceLevel(other.to_string())),
        }
    }
}

/// 来源信任词表（13 号冻结物第三节数据通道三件之一，随行标记）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceTrust {
    /// 外部导入。
    ExternalImport,
    /// RTIS 快照。
    RtisSnapshot,
    /// Agent 派生（AI 生成，原则 C 必标 ai_derived）。
    AgentDerived,
    /// 人工录入。
    Manual,
}

impl SourceTrust {
    /// 存储词形。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ExternalImport => "external_import",
            Self::RtisSnapshot => "rtis_snapshot",
            Self::AgentDerived => "agent_derived",
            Self::Manual => "manual",
        }
    }

    /// 词表解析。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "external_import" => Ok(Self::ExternalImport),
            "rtis_snapshot" => Ok(Self::RtisSnapshot),
            "agent_derived" => Ok(Self::AgentDerived),
            "manual" => Ok(Self::Manual),
            other => Err(DomainError::InvalidSourceTrust(other.to_string())),
        }
    }
}

/// 质量分六权重（R2 5.3 初始权重，可配置；独立风险标记不进总分）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QualityWeights {
    /// 来源可靠性。
    pub source_reliability: f32,
    /// 内容完整度。
    pub completeness: f32,
    /// 可验证性。
    pub verifiability: f32,
    /// 时效性。
    pub timeliness: f32,
    /// 独立性。
    pub independence: f32,
    /// 技术准确性。
    pub accuracy: f32,
}

impl Default for QualityWeights {
    fn default() -> Self {
        Self {
            source_reliability: 0.20,
            completeness: 0.15,
            verifiability: 0.20,
            timeliness: 0.15,
            independence: 0.15,
            accuracy: 0.15,
        }
    }
}

impl QualityWeights {
    /// 权重和必须为 1（可配置时的护栏）。
    pub fn validate(&self) -> Result<(), DomainError> {
        let sum = self.source_reliability
            + self.completeness
            + self.verifiability
            + self.timeliness
            + self.independence
            + self.accuracy;
        if (sum - 1.0).abs() > 1e-4 {
            return Err(DomainError::InvalidQualityWeights(format!(
                "weights sum {sum} != 1.0"
            )));
        }
        Ok(())
    }
}

/// 质量分六维输入（各 0 到 1；评分子面由调用方按可追溯依据给出）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QualityInputs {
    /// 来源可靠性。
    pub source_reliability: f32,
    /// 内容完整度。
    pub completeness: f32,
    /// 可验证性。
    pub verifiability: f32,
    /// 时效性。
    pub timeliness: f32,
    /// 独立性。
    pub independence: f32,
    /// 技术准确性。
    pub accuracy: f32,
}

/// 六权重加权质量分（0 到 1；无副作用纯函数，评分子面可追溯）。
pub fn quality_score(weights: &QualityWeights, inputs: &QualityInputs) -> f32 {
    let clamp = |v: f32| v.clamp(0.0, 1.0);
    (weights.source_reliability * clamp(inputs.source_reliability)
        + weights.completeness * clamp(inputs.completeness)
        + weights.verifiability * clamp(inputs.verifiability)
        + weights.timeliness * clamp(inputs.timeliness)
        + weights.independence * clamp(inputs.independence)
        + weights.accuracy * clamp(inputs.accuracy))
    .clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_id_accepts_and_rejects() {
        assert!(KnowledgeCardId::try_new("kc-abc123").is_ok());
        assert!(KnowledgeCardId::try_new("").is_err());
        assert!(KnowledgeCardId::try_new("x".repeat(129)).is_err());
        assert!(KnowledgeRelationId::try_new("krel-abc123").is_ok());
        assert!(KnowledgeRelationId::try_new("").is_err());
    }

    #[test]
    fn word_lists_roundtrip() {
        for w in [
            KnowledgeType::VulnPrinciple,
            KnowledgeType::VulnSignature,
            KnowledgeType::ExploitCondition,
            KnowledgeType::DetectionMethod,
            KnowledgeType::FixMethod,
            KnowledgeType::SecurityTechnique,
            KnowledgeType::ToolUsage,
            KnowledgeType::ResearchCase,
        ] {
            assert_eq!(KnowledgeType::parse(w.as_str()).unwrap(), w);
        }
        for w in [
            ReviewStatus::Discovered,
            ReviewStatus::Parsed,
            ReviewStatus::Candidate,
            ReviewStatus::Approved,
            ReviewStatus::Quarantined,
            ReviewStatus::Rejected,
            ReviewStatus::Deprecated,
        ] {
            assert_eq!(ReviewStatus::parse(w.as_str()).unwrap(), w);
        }
        assert!(ReviewStatus::parse("BOGUS").is_err());
        assert!(KnowledgeType::parse("not_a_type").is_err());
        for w in [
            RelationType::SameContent,
            RelationType::NearDuplicate,
            RelationType::RelatedTo,
            RelationType::Supports,
            RelationType::Contradicts,
            RelationType::Supersedes,
            RelationType::Fixes,
            RelationType::Detects,
            RelationType::DerivedFrom,
            RelationType::References,
        ] {
            assert_eq!(RelationType::parse(w.as_str()).unwrap(), w);
        }
        assert!(RelationType::parse("SAME").is_err());
    }

    #[test]
    fn risk_flag_hard_block_and_roundtrip() {
        for w in [
            RiskFlag::UntrustedSource,
            RiskFlag::LicenseUnknown,
            RiskFlag::PossiblePromptInjection,
            RiskFlag::Outdated,
            RiskFlag::Contradicted,
            RiskFlag::LowExtractionConfidence,
            RiskFlag::UnverifiedClaim,
        ] {
            assert_eq!(RiskFlag::parse(w.as_str()).unwrap(), w);
        }
        assert!(RiskFlag::PossiblePromptInjection.is_hard_block());
        assert!(!RiskFlag::Outdated.is_hard_block());
        assert!(RiskFlag::parse("NOPE").is_err());
    }

    #[test]
    fn quality_score_bounds_and_default_weights() {
        let w = QualityWeights::default();
        w.validate().unwrap();
        let full = QualityInputs {
            source_reliability: 1.0,
            completeness: 1.0,
            verifiability: 1.0,
            timeliness: 1.0,
            independence: 1.0,
            accuracy: 1.0,
        };
        assert!((quality_score(&w, &full) - 1.0).abs() < 1e-4);
        let zero = QualityInputs {
            source_reliability: 0.0,
            completeness: 0.0,
            verifiability: 0.0,
            timeliness: 0.0,
            independence: 0.0,
            accuracy: 0.0,
        };
        assert!((quality_score(&w, &zero) - 0.0).abs() < 1e-4);
        let over = QualityInputs { source_reliability: 2.0, ..zero };
        let s = quality_score(&w, &over);
        assert!((0.0..=1.0).contains(&s));
        let bad = QualityWeights { completeness: 0.30, ..w };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn evidence_and_trust_roundtrip() {
        for level in [
            EvidenceLevel::K0,
            EvidenceLevel::K1,
            EvidenceLevel::K2,
            EvidenceLevel::K3,
            EvidenceLevel::K4,
        ] {
            assert_eq!(EvidenceLevel::parse(level.as_str()).unwrap(), level);
        }
        assert!(EvidenceLevel::parse("K9").is_err());
        for t in [
            SourceTrust::ExternalImport,
            SourceTrust::RtisSnapshot,
            SourceTrust::AgentDerived,
            SourceTrust::Manual,
        ] {
            assert_eq!(SourceTrust::parse(t.as_str()).unwrap(), t);
        }
        assert!(SourceTrust::parse("alien").is_err());
    }
}
