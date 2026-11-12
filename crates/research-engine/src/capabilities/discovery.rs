//! Source Discovery（源发现）确定性 mock。
//!
//! 边界声明：候选源来自固定 mock 目录（rulepack / registry_manifest /
//! knowledge_document 三族）加调用方 resource_hints 直录（resource_hint 族），
//! 按 hint 优先、固定目录次之排序，按 limit 截断。artifact / plugin 两族词表
//! 先行冻结，Phase 8 接 context-broker 真实装配面时启用；query 只影响
//! knowledge_document 的派生 ID 与 note，不做真实检索。

use crate::capabilities::CapabilityExec;
use crate::contract::{
    digest16, CapabilityFailure, CapabilityId, Correlation, ProvenanceAnchor,
};
use domain::ids::ResourceId;
use serde::{Deserialize, Serialize};

/// 源族词表（对齐 context-broker 装配面加知识文档与调用方 hint）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// 调用方直录的既有资源。
    ResourceHint,
    /// 规则包目录。
    Rulepack,
    /// 工具 Manifest 索引。
    RegistryManifest,
    /// 产物表挂载源（Phase 8 启用）。
    Artifact,
    /// 插件挂载源（Phase 8 启用）。
    Plugin,
    /// 知识文档。
    KnowledgeDocument,
}

/// discovery 输入载荷。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryInput {
    /// 查询词（非空）。
    pub query: String,
    /// 调用方已持有的稳定资源 ID（优先纳入）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_hints: Vec<String>,
    /// 返回条数上限（缺省 10，上限 20）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u8>,
}

/// 单个发现源。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveredSource {
    /// 源 ID（确定性派生）。
    pub source_id: String,
    /// 源族。
    pub kind: SourceKind,
    /// 稳定资源 ID。
    pub resource_id: String,
    /// 来源说明（mock 固定模板）。
    pub note: String,
}

/// discovery 输出载荷。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryOutput {
    /// 发现源列表（hint 优先，固定目录次之，按 limit 截断）。
    pub sources: Vec<DiscoveredSource>,
    /// 实际返回条数。
    pub total: usize,
}

/// discovery 能力标记类型。
pub struct DiscoveryCapability;

impl CapabilityExec for DiscoveryCapability {
    type Input = DiscoveryInput;
    type Output = DiscoveryOutput;
    const ID: CapabilityId = CapabilityId::SourceDiscovery;

    fn run(
        _correlation: &Correlation,
        input: Self::Input,
    ) -> Result<(Self::Output, String, Vec<ProvenanceAnchor>), CapabilityFailure> {
        if input.query.trim().is_empty() {
            return Err(CapabilityFailure::validation(
                "CAP-DISCOVERY-QUERY-EMPTY",
                "query 不得为空白文本",
            ));
        }
        let limit = input.limit.map(|v| v as usize).unwrap_or(10).clamp(1, 20);
        let mut sources: Vec<DiscoveredSource> = Vec::new();
        // hint 族：逐条过 ResourceId 域校验，非法即整请求 VALIDATION 拒绝。
        for hint in &input.resource_hints {
            let resource = ResourceId::try_new(hint.clone()).map_err(|e| {
                CapabilityFailure::validation(
                    "CAP-DISCOVERY-HINT-INVALID",
                    format!("resource hint 不符合稳定 ID 口径: {e}"),
                )
            })?;
            sources.push(DiscoveredSource {
                source_id: format!(
                    "src-{}",
                    digest16(&["source", "resource_hint", resource.as_str()])
                ),
                kind: SourceKind::ResourceHint,
                resource_id: resource.to_string(),
                note: "调用方 hint 资源（mock 直录）".to_string(),
            });
        }
        // 固定 mock 目录三族（确定性构造，query 只影响第三族派生）。
        sources.push(DiscoveredSource {
            source_id: format!(
                "src-{}",
                digest16(&["source", "rulepack", "gh:rip/rulepacks-rip-rules"])
            ),
            kind: SourceKind::Rulepack,
            resource_id: "gh:rip/rulepacks-rip-rules".to_string(),
            note: "固定 mock 目录：规则包 rip-rules（Phase 8 接 rulepack 目录真实索引）".to_string(),
        });
        let manifest_resource = format!("url:{}", digest16(&["manifests", "tools"]));
        sources.push(DiscoveredSource {
            source_id: format!(
                "src-{}",
                digest16(&["source", "registry_manifest", &manifest_resource])
            ),
            kind: SourceKind::RegistryManifest,
            resource_id: manifest_resource,
            note: "固定 mock 目录：工具 Manifest 索引（Phase 8 接 Manifest 目录索引）".to_string(),
        });
        let knowledge_resource = format!("url:{}", digest16(&["knowledge", &input.query]));
        sources.push(DiscoveredSource {
            source_id: format!(
                "src-{}",
                digest16(&["source", "knowledge_document", &knowledge_resource])
            ),
            kind: SourceKind::KnowledgeDocument,
            resource_id: knowledge_resource,
            note: format!(
                "查询词派生知识文档入口（mock，query 摘要 {}）",
                digest16(&["query", &input.query])
            ),
        });
        sources.truncate(limit);
        let total = sources.len();
        // 全部 resource_id 由构造或域校验保证合法，直接落 resource 锚。
        let anchors = sources
            .iter()
            .filter_map(|source| ResourceId::try_new(source.resource_id.clone()).ok())
            .map(|resource| ProvenanceAnchor::resource(&resource))
            .collect();
        let query_digest = digest16(&["query", &input.query]);
        let summary = format!("发现 {total} 个源（query 摘要 {query_digest}，mock 固定目录加 hint）");
        Ok((DiscoveryOutput { sources, total }, summary, anchors))
    }
}
