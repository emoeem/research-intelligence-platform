//! 通用知识来源 Domain 实体与词表（SK-1，P6-9 第 1 步 ADR 裁决落地，
//! 裁决书 docs/phase6/SK1-INGEST-ADR.md 第三节）。
//!
//! 四实体：Source Snapshot（来源快照，折叠进条目列族）、Catalog Entry
//! （链接目录条目）、Document（Markdown 正文）、ContentRef（下游统一引用，
//! 复用 [`crate::context::ContentRef`] 承载）。词表三枚举对应三张 DDL CHECK
//! 域：EntryKind 六值对应 011 迁移 knowledge_source_entry.content_kind 与
//! 010 knowledge_ingest.content_kind 的互补并集；ReviewStatus/RightsPolicy
//! 对应 011 同名列 CHECK 域。
//!
//! 两表分工（ADR 第四节）：RTIS 三类（poc/skill_md/method_case）唯一权威
//! 落点是 knowledge_ingest（010，冻结兼容面）；SK-1 三类（catalog_entry/
//! document_markdown/skill_candidate）唯一权威落点是 knowledge_source_entry
//! （011，通用面）。任何一类内容只有一个权威落点。

use crate::error::DomainError;
use std::fmt;

/// 知识内容类型（六值：010 词表三类 + SK-1 三类，互补不相交）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EntryKind {
    /// RTIS POC 条目（knowledge.db poc 表逐行，010）。
    #[default]
    Poc,
    /// RTIS 技能文档（skills/*.md 全文，010）。
    SkillMd,
    /// RTIS 编排案例正文（pocs/ 递归文件，010）。
    MethodCase,
    /// 链接目录条目（SK-1，canonical URL 身份）。
    CatalogEntry,
    /// Markdown 文档正文（SK-1，包内路径身份）。
    DocumentMarkdown,
    /// Skill/Agent/MCP 候选元数据（SK-1，不代表已安装）。
    SkillCandidate,
}

impl EntryKind {
    /// 全部变体（010 三类在前，SK-1 三类在后）。
    pub const ALL: [EntryKind; 6] = [
        EntryKind::Poc,
        EntryKind::SkillMd,
        EntryKind::MethodCase,
        EntryKind::CatalogEntry,
        EntryKind::DocumentMarkdown,
        EntryKind::SkillCandidate,
    ];

    /// RTIS 既有三类（010 词表，knowledge_ingest 权威落点）。
    pub fn is_rtis(&self) -> bool {
        matches!(
            self,
            EntryKind::Poc | EntryKind::SkillMd | EntryKind::MethodCase
        )
    }

    /// SK-1 新增三类（knowledge_source_entry 权威落点，011）。
    pub fn is_extended(&self) -> bool {
        !self.is_rtis()
    }

    /// 存储值（010/011 迁移 content_kind 列 CHECK 域）。
    pub fn as_str(&self) -> &'static str {
        match self {
            EntryKind::Poc => "poc",
            EntryKind::SkillMd => "skill_md",
            EntryKind::MethodCase => "method_case",
            EntryKind::CatalogEntry => "catalog_entry",
            EntryKind::DocumentMarkdown => "document_markdown",
            EntryKind::SkillCandidate => "skill_candidate",
        }
    }

    /// 从存储值解析（六值并集词表）。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        Self::ALL
            .into_iter()
            .find(|v| v.as_str() == value)
            .ok_or_else(|| DomainError::UnknownContextVariant {
                field: "entry_kind".to_string(),
                value: value.to_string(),
            })
    }
}

impl fmt::Display for EntryKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 审核状态（011 review_status 列 CHECK 域；计划 §7 第 4 条：初始
/// DISCOVERED，人工 License/Supply-chain/Capability 审查通过才 APPROVED，
/// Search/Context 只收录 APPROVED；上游删除转 DEPRECATED 不物理删除）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ReviewStatus {
    /// 摄取即得，未经人工审查。
    #[default]
    Discovered,
    /// 进入审查队列。
    Unreviewed,
    /// 审查通过：进 Search 第三源与 Context PluginSource。
    Approved,
    /// 审查拒绝：对模型面不可见。
    Rejected,
    /// 上游已删除/废弃：保留行与历史 Evidence 引用。
    Deprecated,
}

impl ReviewStatus {
    /// 全部变体。
    pub const ALL: [ReviewStatus; 5] = [
        ReviewStatus::Discovered,
        ReviewStatus::Unreviewed,
        ReviewStatus::Approved,
        ReviewStatus::Rejected,
        ReviewStatus::Deprecated,
    ];

    /// 存储值（011 review_status 列 CHECK 域）。
    pub fn as_str(&self) -> &'static str {
        match self {
            ReviewStatus::Discovered => "DISCOVERED",
            ReviewStatus::Unreviewed => "UNREVIEWED",
            ReviewStatus::Approved => "APPROVED",
            ReviewStatus::Rejected => "REJECTED",
            ReviewStatus::Deprecated => "DEPRECATED",
        }
    }

    /// 从存储值解析。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        Self::ALL
            .into_iter()
            .find(|v| v.as_str() == value)
            .ok_or_else(|| DomainError::UnknownContextVariant {
                field: "review_status".to_string(),
                value: value.to_string(),
            })
    }
}

/// 许可与持久化策略（Manifest provides.kinds.<kind>.license_policy 词表；
/// 011 rights_policy 列 CHECK 域，SK-1 ADR 第五节）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RightsPolicy {
    /// 只登记条目元数据与原始 URL，不持久化外链正文（目录型默认）。
    #[default]
    CatalogOnly,
    /// 仅审核通过（APPROVED）条目进检索与上下文面。
    ApprovedOnly,
    /// 只存元数据与内容哈希，正文列允许为空。
    MetadataOnly,
}

impl RightsPolicy {
    /// 全部变体。
    pub const ALL: [RightsPolicy; 3] = [
        RightsPolicy::CatalogOnly,
        RightsPolicy::ApprovedOnly,
        RightsPolicy::MetadataOnly,
    ];

    /// 存储值（Manifest provides.kinds 声明值与 011 rights_policy 列 CHECK 域）。
    pub fn as_str(&self) -> &'static str {
        match self {
            RightsPolicy::CatalogOnly => "catalog_only",
            RightsPolicy::ApprovedOnly => "approved_only",
            RightsPolicy::MetadataOnly => "metadata_only",
        }
    }

    /// 从存储值解析。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        Self::ALL
            .into_iter()
            .find(|v| v.as_str() == value)
            .ok_or_else(|| DomainError::UnknownContextVariant {
                field: "rights_policy".to_string(),
                value: value.to_string(),
            })
    }
}

/// 来源快照（ADR 3.1：不单独建表，折叠进 knowledge_source_entry 的
/// source_* 列族；同一快照的条目共享相同值，聚合可 GROUP BY 得出）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SourceSnapshot {
    /// 来源类型（plugin source.type 原值：git/local_dir/...）。
    pub source_type: String,
    /// 来源主页或仓库 URL。
    pub canonical_url: String,
    /// 来源内解析入口（包内相对路径）。
    pub source_path: String,
    /// 快照基准 Commit（Git 来源；本地来源可为空）。
    pub source_commit: Option<String>,
}

/// 通用知识来源条目（ADR 3.2/3.3：catalog_entry 与 document_markdown/
/// skill_candidate 的统一落库形态，011 knowledge_source_entry 行实体）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KnowledgeSourceEntry {
    /// 溯源三元组：来源插件 id。
    pub plugin_id: String,
    /// 溯源：来源插件版本。
    pub plugin_version: String,
    /// 语料标识（manifest provides.corpus_id）。
    pub corpus_id: String,
    /// 内容类型。
    pub entry_kind: EntryKind,
    /// 稳定条目键（catalog_entry=规范化 URL；document/skill=包内相对路径）。
    pub item_key: String,
    /// 条目标题。
    pub title: String,
    /// 描述/摘要（metadata_only 时可为空）。
    pub description: Option<String>,
    /// 正文（catalog_entry 为条目行文本；document 为去 frontmatter 全文）。
    pub body: String,
    /// 正文字节数。
    pub body_bytes: i64,
    /// 正文 sha256（幂等与 Evidence 复核键）。
    pub content_sha256: String,
    /// 元数据 sha256（source_location/category_path 等的变更侦测）。
    pub metadata_sha256: String,
    /// 分类路径（章节标题路径，自顶向下）。
    pub category_path: Vec<String>,
    /// 规范化 URL（catalog_entry 必有，其余可为空）。
    pub canonical_url: Option<String>,
    /// 来源位置（文件、行号、章节锚点）。
    pub source_location: SourceLocation,
    /// 来源快照。
    pub source: SourceSnapshot,
    /// 许可状态（来源仓库许可原值）。
    pub license_status: String,
    /// 许可与持久化策略。
    pub rights_policy: RightsPolicy,
    /// 审核状态（初始 DISCOVERED）。
    pub review_status: ReviewStatus,
}

/// 源位置（计划 §8 P6-9 第 4 步：源位置；ADR 4.2 source_location_json 落库形态）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SourceLocation {
    /// 包内相对文件路径。
    pub file: String,
    /// 1 起始行号（正文解析可得时）。
    pub line: Option<u64>,
    /// 章节锚点（GitHub 风格 slug）。
    pub anchor: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_kind_round_trip_and_partition() {
        for k in EntryKind::ALL {
            assert_eq!(EntryKind::parse(k.as_str()).unwrap(), k);
        }
        assert!(EntryKind::parse("catalog_entry").unwrap().is_extended());
        assert!(EntryKind::parse("poc").unwrap().is_rtis());
        assert!(EntryKind::parse("document_markdown").is_ok());
        assert!(EntryKind::parse("wiki").is_err());
    }

    #[test]
    fn review_status_and_policy_round_trip() {
        for s in ReviewStatus::ALL {
            assert_eq!(ReviewStatus::parse(s.as_str()).unwrap(), s);
        }
        for p in RightsPolicy::ALL {
            assert_eq!(RightsPolicy::parse(p.as_str()).unwrap(), p);
        }
        assert!(ReviewStatus::parse("PENDING").is_err());
        assert!(RightsPolicy::parse("open").is_err());
    }

    #[test]
    fn entry_default_review_is_discovered() {
        let e = KnowledgeSourceEntry {
            review_status: ReviewStatus::Discovered,
            ..Default::default()
        };
        assert_eq!(e.review_status.as_str(), "DISCOVERED");
    }
}
