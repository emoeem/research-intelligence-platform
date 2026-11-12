//! Synthesis（综合）确定性 mock。
//!
//! 边界声明：每个输入项原样展开为一节（note 进 body，无 note 落固定占位
//! 文案），主题只影响 synthesis_id / topic_digest 的确定性派生，不做真实
//! 归并与改写；Phase 8 以真实综合实现替换 body 生成逻辑，契约面不变。

use crate::capabilities::CapabilityExec;
use crate::contract::{
    digest16, CapabilityFailure, CapabilityId, Correlation, ProvenanceAnchor,
};
use domain::ids::{ArtifactId, EvidenceId};
use serde::{Deserialize, Serialize};

/// 综合输入项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SynthesisItem {
    /// 证据锚（必填）。
    pub evidence_id: String,
    /// 产物锚（可缺省）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    /// 素材文本（mock 原样进段落 body）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// synthesis 输入载荷。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SynthesisInput {
    /// 综合主题（非空）。
    pub topic: String,
    /// 输入项（至少 1 条）。
    pub items: Vec<SynthesisItem>,
}

/// 综合段落。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SynthesisSection {
    /// 段落标题（mock 固定模板）。
    pub title: String,
    /// 段落正文（note 原样或固定占位）。
    pub body: String,
    /// 段落锚定的证据 ID（与输入项一一对应）。
    pub anchored_evidence_ids: Vec<String>,
}

/// synthesis 输出载荷。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SynthesisOutput {
    /// 综合 ID（主题确定性派生）。
    pub synthesis_id: String,
    /// 段落序列（与 items 等长同序）。
    pub sections: Vec<SynthesisSection>,
    /// 主题文本摘要。
    pub topic_digest: String,
}

/// synthesis 能力标记类型。
pub struct SynthesisCapability;

impl CapabilityExec for SynthesisCapability {
    type Input = SynthesisInput;
    type Output = SynthesisOutput;
    const ID: CapabilityId = CapabilityId::Synthesis;

    fn run(
        _correlation: &Correlation,
        input: Self::Input,
    ) -> Result<(Self::Output, String, Vec<ProvenanceAnchor>), CapabilityFailure> {
        if input.topic.trim().is_empty() {
            return Err(CapabilityFailure::validation(
                "CAP-SYNTHESIS-TOPIC-EMPTY",
                "topic 不得为空白文本",
            ));
        }
        if input.items.is_empty() {
            return Err(CapabilityFailure::validation(
                "CAP-SYNTHESIS-NO-ITEMS",
                "items 不得为空",
            ));
        }
        let synthesis_id = format!("syn-{}", digest16(&["syn", &input.topic]));
        let topic_digest = digest16(&["topic", &input.topic]);
        let mut sections = Vec::with_capacity(input.items.len());
        let mut anchors = Vec::new();
        for (index, item) in input.items.iter().enumerate() {
            let evidence = EvidenceId::try_new(item.evidence_id.clone()).map_err(|e| {
                CapabilityFailure::validation(
                    "CAP-SYNTHESIS-EVIDENCE-INVALID",
                    format!("item evidence_id 不符合 EvidenceId 口径: {e}"),
                )
            })?;
            let artifact = match &item.artifact_id {
                Some(raw) => Some(ArtifactId::try_new(raw.clone()).map_err(|e| {
                    CapabilityFailure::validation(
                        "CAP-SYNTHESIS-ARTIFACT-INVALID",
                        format!("item artifact_id 不符合 ArtifactId 口径: {e}"),
                    )
                })?),
                None => None,
            };
            anchors.push(ProvenanceAnchor::evidence(&evidence));
            if let Some(artifact) = &artifact {
                anchors.push(ProvenanceAnchor::artifact(artifact));
            }
            let body = match &item.note {
                Some(note) => note.clone(),
                None => format!(
                    "确定性综合占位：主题「{}」下证据 {} 的 mock 段落（Phase 8 接真实综合）",
                    input.topic,
                    evidence.as_str()
                ),
            };
            sections.push(SynthesisSection {
                title: format!("第 {} 节：证据 {}", index + 1, evidence.as_str()),
                body,
                anchored_evidence_ids: vec![evidence.to_string()],
            });
        }
        let section_count = sections.len();
        let summary =
            format!("综合 {section_count} 节（synthesis {synthesis_id}，mock 逐项展开）");
        Ok((
            SynthesisOutput {
                synthesis_id,
                sections,
                topic_digest,
            },
            summary,
            anchors,
        ))
    }
}
