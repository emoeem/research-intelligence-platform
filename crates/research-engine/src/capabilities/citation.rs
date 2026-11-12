//! Citation（引文）确定性 mock。
//!
//! 边界声明：每个锚按输入顺序编号生成引文（footnote 标记 [n]，inline 标记
//! (n)），citation_id 由 synthesis_id 与 marker 联合摘要确定性派生；锚逐条
//! 过域层 EvidenceId / ArtifactId 校验，两字段全缺的锚为契约违规。样式不
//! 改变内容，只改变标记形状；Phase 8 接真实引文面（含回链可解析校验）。

use crate::capabilities::CapabilityExec;
use crate::contract::{
    digest16, CapabilityFailure, CapabilityId, Correlation, ProvenanceAnchor,
};
use domain::ids::{ArtifactId, EvidenceId};
use serde::{Deserialize, Serialize};

/// 引文风格。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationStyle {
    /// 行内标记 (n)。
    Inline,
    /// 脚注标记 [n]。
    Footnote,
}

impl CitationStyle {
    /// 词表字符串（summary 文案用）。
    pub fn as_str(&self) -> &'static str {
        match self {
            CitationStyle::Inline => "inline",
            CitationStyle::Footnote => "footnote",
        }
    }
}

/// 引文锚引用（evidence_id / artifact_id 至少给出一个，Schema anyOf 同口径）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CitationAnchorRef {
    /// 证据锚（与 artifact_id 二选一以上）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_id: Option<String>,
    /// 产物锚（与 evidence_id 二选一以上）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
}

/// citation 输入载荷。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CitationInput {
    /// 被引综合成果 ID（透传回指）。
    pub synthesis_id: String,
    /// 锚清单（至少 1 条）。
    pub anchors: Vec<CitationAnchorRef>,
    /// 引文风格（缺省 inline）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<CitationStyle>,
}

/// 单条引文。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Citation {
    /// 引文 ID（确定性派生）。
    pub citation_id: String,
    /// 引文标记（footnote 为 [n]，inline 为 (n)）。
    pub marker: String,
    /// 证据锚（可缺省）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_id: Option<String>,
    /// 产物锚（可缺省）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
}

/// citation 输出载荷。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CitationOutput {
    /// 引文清单（与 anchors 等长同序）。
    pub citations: Vec<Citation>,
    /// 实际采用的引文风格。
    pub style: CitationStyle,
    /// 被引综合成果 ID 原文透传。
    pub synthesis_ref: String,
}

/// citation 能力标记类型。
pub struct CitationCapability;

impl CapabilityExec for CitationCapability {
    type Input = CitationInput;
    type Output = CitationOutput;
    const ID: CapabilityId = CapabilityId::Citation;

    fn run(
        _correlation: &Correlation,
        input: Self::Input,
    ) -> Result<(Self::Output, String, Vec<ProvenanceAnchor>), CapabilityFailure> {
        if input.synthesis_id.trim().is_empty() {
            return Err(CapabilityFailure::validation(
                "CAP-CITATION-SYNTHESIS-EMPTY",
                "synthesis_id 不得为空白文本",
            ));
        }
        if input.anchors.is_empty() {
            return Err(CapabilityFailure::validation(
                "CAP-CITATION-NO-ANCHORS",
                "anchors 不得为空",
            ));
        }
        let style = input.style.unwrap_or(CitationStyle::Inline);
        let mut citations = Vec::with_capacity(input.anchors.len());
        let mut anchors = Vec::new();
        for (index, anchor_ref) in input.anchors.iter().enumerate() {
            let evidence = match &anchor_ref.evidence_id {
                Some(raw) => Some(EvidenceId::try_new(raw.clone()).map_err(|e| {
                    CapabilityFailure::validation(
                        "CAP-CITATION-EVIDENCE-INVALID",
                        format!("anchor evidence_id 不符合 EvidenceId 口径: {e}"),
                    )
                })?),
                None => None,
            };
            let artifact = match &anchor_ref.artifact_id {
                Some(raw) => Some(ArtifactId::try_new(raw.clone()).map_err(|e| {
                    CapabilityFailure::validation(
                        "CAP-CITATION-ARTIFACT-INVALID",
                        format!("anchor artifact_id 不符合 ArtifactId 口径: {e}"),
                    )
                })?),
                None => None,
            };
            if evidence.is_none() && artifact.is_none() {
                return Err(CapabilityFailure::validation(
                    "CAP-CITATION-ANCHOR-EMPTY",
                    "锚必须至少给出 evidence_id 或 artifact_id 之一",
                ));
            }
            if let Some(evidence) = &evidence {
                anchors.push(ProvenanceAnchor::evidence(evidence));
            }
            if let Some(artifact) = &artifact {
                anchors.push(ProvenanceAnchor::artifact(artifact));
            }
            let marker = match style {
                CitationStyle::Footnote => format!("[{}]", index + 1),
                CitationStyle::Inline => format!("({})", index + 1),
            };
            citations.push(Citation {
                citation_id: format!(
                    "cite-{}",
                    digest16(&["cite", &input.synthesis_id, &marker])
                ),
                marker,
                evidence_id: evidence.as_ref().map(|e| e.to_string()),
                artifact_id: artifact.as_ref().map(|a| a.to_string()),
            });
        }
        let citation_count = citations.len();
        let summary = format!(
            "生成 {citation_count} 条 {} 引文（synthesis {}，mock）",
            style.as_str(),
            input.synthesis_id
        );
        Ok((
            CitationOutput {
                citations,
                style,
                synthesis_ref: input.synthesis_id,
            },
            summary,
            anchors,
        ))
    }
}
