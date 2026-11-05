//! Resolve 查询模型与 eri 兼容输出形状。
//!
//! 兼容口径（MODULE_BOUNDARIES §8.4）：Resolve 的 JSON 输出与 eri_p2.resolve
//! 保持字段级兼容（字段名、类型、null 语义一致），迁移期双轨。

use domain::{GovernanceStatus, LifecycleStatus};
use serde::Serialize;

/// Resolve 查询。
#[derive(Debug, Clone, PartialEq)]
pub struct ResolveQuery {
    /// 任务 ID（审计归属；eri 口径缺省 "adhoc"）。
    pub task_id: String,
    /// 关键词（eri 口径：与 technologies 合并后做别名词元匹配）。
    pub keywords: Vec<String>,
    /// 过滤条件。
    pub filters: ResolveFilters,
    /// 返回上限（eri DEFAULT_TOP_K = 5）。
    pub limit: usize,
}

impl ResolveQuery {
    /// 构造查询：task_id 缺省 "adhoc"，limit 缺省 5，与 eri 口径一致。
    pub fn new(keywords: Vec<String>) -> Self {
        Self {
            task_id: "adhoc".to_string(),
            keywords,
            filters: ResolveFilters::default(),
            limit: 5,
        }
    }
}

/// Resolve 过滤条件（在治理可见性规则之上追加）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolveFilters {
    /// 限定资源类型（如 github_repo）。
    pub resource_type: Option<String>,
    /// 限定生命周期状态。
    pub lifecycle: Option<LifecycleStatus>,
    /// 限定治理状态；None = 引擎默认（仅 ACTIVE 可见）。
    pub governance: Option<GovernanceStatus>,
}

/// 单条命中（与 eri matched_resources 元素字段级兼容）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResourceHit {
    /// 稳定 ID。
    pub resource_id: String,
    /// 名称。
    pub name: String,
    /// 规范化 URL。
    pub canonical_url: String,
    /// 描述（platform 库可空，序列化为 null）。
    pub description: Option<String>,
    /// 资源类型。
    pub resource_type: String,
    /// 命中的分类/章节（platform 库无 resource_categories 表，恒空）。
    pub category_matches: Vec<String>,
    /// 词元级命中的关键词。
    pub keyword_matches: Vec<String>,
    /// 映射到的 skill（platform 库无 skill_mappings 表，恒空）。
    pub skill_relationships: Vec<String>,
    /// 映射到的 workflow（恒空，同上）。
    pub workflow_relationships: Vec<String>,
    /// 质量终分（quality_final_score）。
    pub quality: Option<f64>,
    /// 质量等级。
    pub grade: Option<String>,
    /// license 状态（platform 库无 license 元数据列，恒 UNKNOWN，见 M2_REPORT 差异表）。
    pub license_status: String,
    /// 维护状态（同上，恒 UNKNOWN）。
    pub maintenance_status: String,
    /// 是否可采纳为依赖（license 无证据时为 false）。
    pub adoptable_as_dependency: bool,
    /// license 注意事项。
    pub license_note: Option<String>,
    /// 相关性得分（0..1，4 位小数）。
    pub relevance_score: f64,
    /// 得分理由。
    pub relevance_reasons: Vec<String>,
}

/// 检索统计（eri retrieval_stats 字段级兼容）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RetrievalStats {
    /// SQL 预筛后的候选数。
    pub candidates: usize,
    /// 打分阶段被过滤数。
    pub filtered: usize,
    /// 实际返回数。
    pub returned: usize,
    /// 缓存命中数（本批恒 0）。
    pub cache_hits: usize,
}

/// 已被内部 skill 覆盖的资源引用（eri already_covered_by_internal 元素形状）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CoveredRef {
    /// 资源 ID。
    pub resource_id: String,
    /// 覆盖它的 skill/workflow。
    pub covered_by: String,
}

/// 能力缺口候选（eri capability_gaps 元素形状）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityGap {
    /// 资源 ID。
    pub resource_id: String,
    /// 缺口理由。
    pub reason: String,
}

/// Resolve 结果（eri resolve 返回 JSON 顶层字段级兼容）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResolveResult {
    /// 查询 ID（erq-<ts>-<hash>）。
    pub query_id: String,
    /// 任务 ID。
    pub task_id: String,
    /// 命中列表。
    pub matched_resources: Vec<ResourceHit>,
    /// 检索统计。
    pub retrieval_stats: RetrievalStats,
    /// 已被内部 skill 覆盖（platform 库无映射表，恒空）。
    pub already_covered_by_internal: Vec<CoveredRef>,
    /// 能力缺口候选。
    pub capability_gaps: Vec<CapabilityGap>,
    /// "ok" 或 "no_match"。
    pub search_status: String,
    /// 权重配置快照。
    pub weights_config: WeightsConfig,
}

/// 权重配置快照（eri weights_config 字段级兼容）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct WeightsConfig {
    /// keyword 轴权重。
    pub keyword: f64,
    /// quality 轴权重。
    pub quality: f64,
}
