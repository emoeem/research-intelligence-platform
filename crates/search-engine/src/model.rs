//! 检索查询模型与命中形状（MODULE_BOUNDARIES §8.4：`Query(q, filters) -> Hits`）。

use serde::{Deserialize, Serialize};

/// Source identity and immutable provenance projected into the disposable search index.
/// Missing values remain absent; the index never invents source hashes or canonical IDs.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SearchIdentity {
    /// Typed entity family, e.g. vulnerability, detection_template or poc.
    pub entity_type: Option<String>,
    /// Source-independent key within the typed entity family.
    pub canonical_key: Option<String>,
    /// Authority/plugin that supplied this source record.
    pub source_system: Option<String>,
    /// Stable record identifier within the source system.
    pub source_entity_id: Option<String>,
    /// Immutable source/plugin version for this snapshot.
    pub source_version: Option<String>,
    /// SHA-256 of source content when the source actually provides it.
    pub content_sha256: Option<String>,
    /// SHA-256 of source metadata when the source actually provides it.
    pub metadata_sha256: Option<String>,
    /// Exact vulnerability/CVE identifiers referenced by this record.
    pub vulnerability_ids: Vec<String>,
}

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
    /// Nuclei 严重程度（critical/high/medium/low/info）。
    pub nuclei_severity: Option<String>,
    /// Nuclei 协议类型（http/dns/tcp 等）。
    pub nuclei_protocol: Option<String>,
    /// 精确 CVE 标识。
    pub cve_id: Option<String>,
    /// 精确 CWE 标识（Nuclei）。
    pub cwe_id: Option<String>,
    /// 自有 POC 审核证据等级（K0-K4）。
    pub evidence_level: Option<String>,
    /// 自有 POC 生命周期（Candidate/Researched/...），与证据等级独立。
    pub knowledge_lifecycle: Option<String>,
    /// 自有 POC 严重程度。
    pub poc_severity: Option<String>,
    /// 自有 POC 分类。
    pub poc_category: Option<String>,
    /// 自有 POC 精确 CVE。
    pub poc_cve_id: Option<String>,
    /// POC 来源完整性（complete/partial/missing）。
    pub poc_source_integrity: Option<String>,
    /// POC 证据核验结论，不等同于 evidence_level。
    pub poc_verification_result: Option<String>,
    /// POC 去重状态（independent/related/duplicate_candidate 等）。
    pub poc_dedup_status: Option<String>,
    /// POC 独立知识状态。
    pub poc_independent_knowledge_status: Option<String>,
    /// POC 尚缺证据项标识。
    pub poc_missing_evidence: Option<bool>,
    /// Exact typed entity key filter (not a fuzzy title/URL match).
    /// Exact typed entity key filter (not a fuzzy title/URL match).
    pub canonical_key: Option<String>,
    /// Exact CVE/vulnerability identifier filter across indexed source types.
    pub vulnerability_id: Option<String>,
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
    /// topics（catalog 与 Nuclei tags/classification 补齐）。
    pub topics: Vec<String>,
    /// Nuclei 严重程度。
    pub nuclei_severity: Option<String>,
    /// Nuclei 协议列表。
    pub nuclei_protocols: Vec<String>,
    /// CVE 标识列表。
    pub nuclei_cve_ids: Vec<String>,
    /// CWE 标识列表。
    pub nuclei_cwe_ids: Vec<String>,
    /// 来源清单章节（platform resource_source 的 alias/name：source_section 去重集）。
    pub source_sections: Vec<String>,
    /// 自有 POC 审核证据等级（K0-K4）。
    pub evidence_level: Option<String>,
    /// 自有 POC 生命周期（与平台 resource.lifecycle 分开）。
    pub knowledge_lifecycle: Option<String>,
    /// 自有 POC 分类。
    pub poc_category: Option<String>,
    /// 自有 POC 严重程度。
    pub poc_severity: Option<String>,
    /// 自有 POC CVE。
    pub poc_cve_id: Option<String>,
    /// Overlay 中记录的原始来源路径。
    pub source_path: Option<String>,
    /// Overlay 中记录的来源 Git Commit。
    pub source_commit: Option<String>,
    /// Overlay 关系映射到的其他 POC ID（含关系类型）。
    pub related_poc_ids: Vec<String>,
    /// Overlay 关联到的外部案例组或知识实体（不混入 POC ID）。
    pub related_entities: Vec<String>,
    /// Overlay 中登记的上游来源 URL（仅链接，不抓取目标内容）。
    pub poc_source_urls: Vec<String>,
    /// Overlay 的来源完整性、核验结论和去重状态。
    pub poc_source_integrity: Option<String>,
    /// POC 的历史证据核验结论。
    pub poc_verification_result: Option<String>,
    /// POC 的去重状态。
    pub poc_dedup_status: Option<String>,
    /// POC 是否构成独立知识。
    pub poc_independent_knowledge_status: Option<String>,
    /// 缺失证据项标识（不包含证据正文）。
    pub poc_missing_evidence: Vec<String>,
    /// 证据文件路径/引用标识（不包含文件内容）。
    pub poc_evidence_refs: Vec<String>,
    /// 下一步审核门槛说明。
    pub poc_review_gate: Option<String>,
    /// Generic cross-source identity/provenance; absent when source mapping is unknown.
    pub identity: Option<SearchIdentity>,
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
    /// Nuclei 严重程度。
    pub nuclei_severity: Option<String>,
    /// Nuclei 协议列表。
    pub nuclei_protocols: Vec<String>,
    /// CVE 标识列表。
    pub nuclei_cve_ids: Vec<String>,
    /// CWE 标识列表。
    pub nuclei_cwe_ids: Vec<String>,
    /// Nuclei 原始 tags。
    pub nuclei_tags: Vec<String>,
    /// 自有 POC 审核证据等级。
    pub evidence_level: Option<String>,
    /// 自有 POC 生命周期。
    pub knowledge_lifecycle: Option<String>,
    /// 自有 POC 分类。
    pub poc_category: Option<String>,
    /// 自有 POC 严重程度。
    pub poc_severity: Option<String>,
    /// 自有 POC CVE。
    pub poc_cve_id: Option<String>,
    /// POC 原始来源路径。
    pub source_path: Option<String>,
    /// POC 来源 Commit。
    pub source_commit: Option<String>,
    /// 关联 POC ID 与关系类型。
    pub related_poc_ids: Vec<String>,
    /// External case/knowledge entity references linked by the Overlay.
    pub related_entities: Vec<String>,
    /// Upstream source URLs declared by the curation Overlay.
    pub poc_source_urls: Vec<String>,
    /// Generic typed source identity and provenance snapshot.
    pub identity: Option<SearchIdentity>,
    /// Overlay source integrity; never inferred from the legacy verified flag.
    pub poc_source_integrity: Option<String>,
    /// Historical evidence verification result.
    pub poc_verification_result: Option<String>,
    /// Duplicate/relationship curation status.
    pub poc_dedup_status: Option<String>,
    /// Whether this record represents independent knowledge.
    pub poc_independent_knowledge_status: Option<String>,
    /// Missing evidence identifiers, without evidence bodies.
    pub poc_missing_evidence: Vec<String>,
    /// Evidence paths/identifiers, without evidence bodies.
    pub poc_evidence_refs: Vec<String>,
    /// Human-readable next review gate.
    pub poc_review_gate: Option<String>,
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
