//! Evidence Extraction（证据抽取）确定性 mock。
//!
//! 边界声明：抽取条目由 evidence_refs 逐条经域层 [`domain::ids::EvidenceId`]
//! 校验后确定性派生（content_hash = sha256("mock-extraction" + evidence_id)
//! 全长 64 hex，storage_ref = mock://evidence/<id>），不读 evidence 表真实
//! 内容；artifact_refs 按 ref 顺序逐位挂到对应抽取条目（多余的忽略）。
//! Phase 8 以 evidence/artifact 表读面替换实现体，契约面不变。

use crate::capabilities::CapabilityExec;
use crate::contract::{
    digest_hex, CapabilityFailure, CapabilityId, Correlation, ProvenanceAnchor,
};
use domain::ids::{ArtifactId, EvidenceId};
use serde::{Deserialize, Serialize};

/// 抽取目标类型（与 001 迁移 evidence.evidence_type CHECK 同词表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    /// HTTP 请求响应对。
    HttpPair,
    /// 截图。
    Screenshot,
    /// 文件。
    File,
    /// 日志。
    Log,
    /// 哈希。
    Hash,
}

impl EvidenceKind {
    /// 词表字符串（summary 文案用）。
    pub fn as_str(&self) -> &'static str {
        match self {
            EvidenceKind::HttpPair => "http_pair",
            EvidenceKind::Screenshot => "screenshot",
            EvidenceKind::File => "file",
            EvidenceKind::Log => "log",
            EvidenceKind::Hash => "hash",
        }
    }
}

/// extraction 输入载荷。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractionInput {
    /// 证据引用（至少 1 条）。
    pub evidence_refs: Vec<String>,
    /// 产物引用（按 ref 顺序逐位挂接，可缺省）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifact_refs: Vec<String>,
    /// 抽取目标类型。
    pub extract_as: EvidenceKind,
}

/// 单条抽取结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extraction {
    /// 证据 ID（域校验后原样）。
    pub evidence_id: String,
    /// 挂接产物 ID（可缺省）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    /// 内容哈希（mock 确定性派生，64 hex）。
    pub content_hash: String,
    /// 存储引用（mock 占位地址）。
    pub storage_ref: String,
    /// 抽取说明（mock 固定模板）。
    pub summary: String,
}

/// extraction 输出载荷。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractionOutput {
    /// 抽取条目（与 evidence_refs 等长同序）。
    pub extractions: Vec<Extraction>,
    /// 抽取条数。
    pub extracted_count: usize,
}

/// extraction 能力标记类型。
pub struct ExtractionCapability;

impl CapabilityExec for ExtractionCapability {
    type Input = ExtractionInput;
    type Output = ExtractionOutput;
    const ID: CapabilityId = CapabilityId::EvidenceExtraction;

    fn run(
        _correlation: &Correlation,
        input: Self::Input,
    ) -> Result<(Self::Output, String, Vec<ProvenanceAnchor>), CapabilityFailure> {
        if input.evidence_refs.is_empty() {
            return Err(CapabilityFailure::validation(
                "CAP-EXTRACTION-NO-REFS",
                "evidence_refs 不得为空",
            ));
        }
        let mut extractions = Vec::with_capacity(input.evidence_refs.len());
        let mut anchors = Vec::new();
        for (index, raw_evidence) in input.evidence_refs.iter().enumerate() {
            let evidence = EvidenceId::try_new(raw_evidence.clone()).map_err(|e| {
                CapabilityFailure::validation(
                    "CAP-EXTRACTION-EVIDENCE-INVALID",
                    format!("evidence_ref 不符合 EvidenceId 口径: {e}"),
                )
            })?;
            let artifact = match input.artifact_refs.get(index) {
                Some(raw_artifact) => Some(ArtifactId::try_new(raw_artifact.clone()).map_err(
                    |e| {
                        CapabilityFailure::validation(
                            "CAP-EXTRACTION-ARTIFACT-INVALID",
                            format!("artifact_ref 不符合 ArtifactId 口径: {e}"),
                        )
                    },
                )?),
                None => None,
            };
            anchors.push(ProvenanceAnchor::evidence(&evidence));
            if let Some(artifact) = &artifact {
                anchors.push(ProvenanceAnchor::artifact(artifact));
            }
            extractions.push(Extraction {
                evidence_id: evidence.to_string(),
                artifact_id: artifact.as_ref().map(|a| a.to_string()),
                content_hash: digest_hex(&["mock-extraction", evidence.as_str()]),
                storage_ref: format!("mock://evidence/{}", evidence.as_str()),
                summary: format!(
                    "{} 类证据确定性抽取占位（mock，Phase 8 接 evidence 表真实内容）",
                    input.extract_as.as_str()
                ),
            });
        }
        let extracted_count = extractions.len();
        let summary = format!(
            "抽取 {extracted_count} 条 {} 证据（mock）",
            input.extract_as.as_str()
        );
        Ok((ExtractionOutput { extractions, extracted_count }, summary, anchors))
    }
}
