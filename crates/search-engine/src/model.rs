//! 检索查询模型与命中形状（MODULE_BOUNDARIES §8.4：`Query(q, filters) -> Hits`）。

use serde::Serialize;

/// 检索查询。
#[derive(Debug, Clone, PartialEq)]
pub struct SearchQuery {
    /// 关键词（ALIASES 别名扩展 + CJK_BRIDGE 桥接后做 FTS 匹配与覆盖计分）。
    /// 空关键词 = 过滤浏览模式（无 bm25 轴，仅 quality 轴归一）。
    pub keywords: Vec<String>,
    /// 过滤条件（对索引文档元数据做等值过滤）。
    pub filters: SearchFilters,
    /// 返回上限（0 = 取默认 10）。
    pub limit: usize,
    /// 分页偏移。
    pub offset: usize,
}

impl SearchQuery {
    /// 构造查询：limit 缺省 10。
    pub fn new(keywords: Vec<String>) -> Self {
        Self {
            keywords,
            filters: SearchFilters::default(),
            limit: 10,
            offset: 0,
        }
    }
}

/// 过滤条件（全部为等值匹配；None = 不过滤）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchFilters {
    /// 质量等级（"A".."D"，对应 quality_grade）。
    pub grade: Option<String>,
    /// license 状态（IDENTIFIED / NO_LICENSE_FILE / FILE_UNIDENTIFIED / UNKNOWN；
    /// 缺 github_metadata 证据的文档入库时归一为 UNKNOWN，与 eri 输出口径一致）。
    pub license_status: Option<String>,
    /// 维护状态（ACTIVE / LOW_ACTIVITY / STALE / ARCHIVED / UNKNOWN）。
    pub maintenance_status: Option<String>,
    /// 资源类型（github_repo / url 等）。
    pub resource_type: Option<String>,
}

/// 检索权重快照（混合排序三轴，见 weights.rs）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct WeightsSnapshot {
    /// FTS bm25 轴权重。
    pub fts_bm25: f64,
    /// quality 轴权重。
    pub quality: f64,
    /// keyword 覆盖轴权重。
    pub keyword_coverage: f64,
}

/// 单条命中。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchHit {
    /// 稳定 ID。
    pub resource_id: String,
    /// 名称。
    pub name: String,
    /// 规范化 URL。
    pub canonical_url: String,
    /// 描述（可空）。
    pub description: Option<String>,
    /// 资源类型。
    pub resource_type: String,
    /// 质量终分（platform resource.quality_final_score）。
    pub quality_final_score: Option<f64>,
    /// 质量等级。
    pub quality_grade: Option<String>,
    /// license 状态（缺证归一 UNKNOWN）。
    pub license_status: Option<String>,
    /// 维护状态（缺证归一 UNKNOWN）。
    pub maintenance_status: Option<String>,
    /// topics（catalog github_metadata.topics 补齐）。
    pub topics: Vec<String>,
    /// 来源清单章节（platform resource_source 的 alias/name：source_section 去重集）。
    pub source_sections: Vec<String>,
    /// 混合总分（0..1，4 位小数；缺证轴归一后输出）。
    pub score: f64,
    /// 归一化 bm25 分（0..1；浏览模式无关键词时为 None）。
    pub fts_score: Option<f64>,
    /// keyword 覆盖证据值（eri 口径 min(1.0, cov*1.5)）。
    pub keyword_coverage: f64,
    /// 词元级命中的关键词（含别名展开与 CJK 桥接词）。
    pub keyword_matches: Vec<String>,
    /// 可解释理由（各轴证据值 + 缺证轴提示）。
    pub reasons: Vec<String>,
    /// 来源标注（文档各字段的出处声明）。
    pub source_annotations: Vec<String>,
}

/// 检索结果。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hits {
    /// 查询 ID（srq-<紧凑时间戳20位>-<输入哈希前6位>）。
    pub query_id: String,
    /// 过滤 + FTS 匹配后的总命中数（分页前）。
    pub total_matched: usize,
    /// 本页命中。
    pub hits: Vec<SearchHit>,
    /// "ok" | "no_match"。
    pub search_status: String,
    /// 权重配置快照。
    pub weights: WeightsSnapshot,
    /// 检索耗时（毫秒）。
    pub elapsed_ms: f64,
}

/// 索引文档（构建期行，platform + catalog 双源折叠后的一致快照）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct IndexDoc {
    /// 稳定 ID。
    pub resource_id: String,
    /// 名称。
    pub name: String,
    /// 描述。
    pub description: Option<String>,
    /// 规范化 URL。
    pub canonical_url: String,
    /// 资源类型。
    pub resource_type: String,
    /// 生命周期轴快照。
    pub lifecycle: String,
    /// 治理轴快照。
    pub governance: String,
    /// 质量终分。
    pub quality_final_score: Option<f64>,
    /// 质量等级。
    pub quality_grade: Option<String>,
    /// license 状态（缺证归一 UNKNOWN）。
    pub license_status: String,
    /// 维护状态（缺证归一 UNKNOWN）。
    pub maintenance_status: String,
    /// topics（catalog 补齐；JSON 数组解析后的元素）。
    pub topics: Vec<String>,
    /// 语言（catalog 补齐）。
    pub language: Option<String>,
    /// 来源章节（resource_source.source_section 去重）。
    pub source_sections: Vec<String>,
    /// 来源仓库（resource_source.source_repository 去重）。
    pub source_repositories: Vec<String>,
}

/// 索引构建报告。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BuildReport {
    /// 索引文档数。
    pub doc_count: usize,
    /// FTS 行数（与 doc_count 一致，一对一入索引）。
    pub fts_rows: usize,
    /// 是否为删表重建。
    pub rebuilt: bool,
    /// 索引库 schema 版本。
    pub schema_version: i64,
    /// 构建耗时（毫秒）。
    pub elapsed_ms: f64,
}
