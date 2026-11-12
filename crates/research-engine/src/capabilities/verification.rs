//! Verification（核验）确定性 mock。
//!
//! 边界声明：判定规则是纯结构化的确定性规则，不做真实内容比对
//! （CONTRADICTED 判定属 Phase 8）：断言的互异合法 EvidenceId 数达到策略
//! 阈值即 VERIFIED（strict 至少 2，advisory 至少 1）；含非法 EvidenceId
//! 引用或数量不足即 UNVERIFIED，非法引用不进 checked_evidence_ids、只进
//! notes。同输入恒同输出。

use crate::capabilities::CapabilityExec;
use crate::contract::{CapabilityFailure, CapabilityId, Correlation, ProvenanceAnchor};
use domain::ids::EvidenceId;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 核验策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationPolicy {
    /// 严格：互异合法证据至少 2 条。
    Strict,
    /// 宽松（缺省）：互异合法证据至少 1 条。
    Advisory,
}

/// 单条断言。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Claim {
    /// 断言 ID（调用方命名）。
    pub claim_id: String,
    /// 支撑证据引用（可空：空列表恒判 UNVERIFIED）。
    pub evidence_ids: Vec<String>,
}

/// verification 输入载荷。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationInput {
    /// 断言清单（至少 1 条）。
    pub claims: Vec<Claim>,
    /// 核验策略（缺省 advisory）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<VerificationPolicy>,
}

/// 判定词表（mock 无 CONTRADICTED）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum VerdictKind {
    /// 通过：互异合法证据达到策略阈值。
    Verified,
    /// 未通过：非法引用或数量不足。
    Unverified,
}

/// 单条判定。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verdict {
    /// 断言 ID（原样回传）。
    pub claim_id: String,
    /// 判定。
    pub verdict: VerdictKind,
    /// 实际参与核验的合法证据 ID（去重保序）。
    pub checked_evidence_ids: Vec<String>,
    /// 判定说明（mock 固定模板）。
    pub notes: String,
}

/// verification 输出载荷。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationOutput {
    /// 判定清单（与 claims 等长同序）。
    pub verdicts: Vec<Verdict>,
    /// VERIFIED 计数。
    pub verified_count: usize,
    /// UNVERIFIED 计数。
    pub unverified_count: usize,
}

/// verification 能力标记类型。
pub struct VerificationCapability;

impl CapabilityExec for VerificationCapability {
    type Input = VerificationInput;
    type Output = VerificationOutput;
    const ID: CapabilityId = CapabilityId::Verification;

    fn run(
        _correlation: &Correlation,
        input: Self::Input,
    ) -> Result<(Self::Output, String, Vec<ProvenanceAnchor>), CapabilityFailure> {
        if input.claims.is_empty() {
            return Err(CapabilityFailure::validation(
                "CAP-VERIFICATION-NO-CLAIMS",
                "claims 不得为空",
            ));
        }
        let policy = input.policy.unwrap_or(VerificationPolicy::Advisory);
        let policy_str = match policy {
            VerificationPolicy::Strict => "strict",
            VerificationPolicy::Advisory => "advisory",
        };
        let threshold = match policy {
            VerificationPolicy::Strict => 2usize,
            VerificationPolicy::Advisory => 1usize,
        };
        let mut verdicts = Vec::with_capacity(input.claims.len());
        let mut anchors = Vec::new();
        let mut verified = 0usize;
        let mut unverified = 0usize;
        for claim in &input.claims {
            if claim.claim_id.trim().is_empty() {
                return Err(CapabilityFailure::validation(
                    "CAP-VERIFICATION-CLAIM-EMPTY",
                    "claim_id 不得为空白文本",
                ));
            }
            // 去重保序 + 域校验：非法引用单独记录，不进 checked 列。
            let mut checked: Vec<EvidenceId> = Vec::new();
            let mut seen: HashSet<String> = HashSet::new();
            let mut invalid: Vec<String> = Vec::new();
            for raw in &claim.evidence_ids {
                if !seen.insert(raw.clone()) {
                    continue;
                }
                match EvidenceId::try_new(raw.clone()) {
                    Ok(evidence) => checked.push(evidence),
                    Err(_) => invalid.push(raw.clone()),
                }
            }
            if checked.len() >= threshold {
                verified += 1;
                for evidence in &checked {
                    anchors.push(ProvenanceAnchor::evidence(evidence));
                }
            } else {
                unverified += 1;
            }
            let mut notes = format!(
                "策略 {policy_str} 要求互异合法证据至少 {threshold} 条，实际 {} 条",
                checked.len()
            );
            if !invalid.is_empty() {
                notes.push_str(&format!("；非法 EvidenceId 引用 {} 条", invalid.len()));
            }
            verdicts.push(Verdict {
                claim_id: claim.claim_id.clone(),
                verdict: if checked.len() >= threshold {
                    VerdictKind::Verified
                } else {
                    VerdictKind::Unverified
                },
                checked_evidence_ids: checked.iter().map(|e| e.to_string()).collect(),
                notes,
            });
        }
        let claim_count = verdicts.len();
        let summary = format!(
            "核验 {claim_count} 条断言：VERIFIED {verified} / UNVERIFIED {unverified}（policy {policy_str}，mock）"
        );
        Ok((
            VerificationOutput {
                verdicts,
                verified_count: verified,
                unverified_count: unverified,
            },
            summary,
            anchors,
        ))
    }
}
