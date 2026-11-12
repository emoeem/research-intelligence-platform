//! Evaluator（评估）确定性 mock。
//!
//! 边界声明：三项检查的评分是纯结构的 mock 启发式（非真实质量评估），
//! 全部由被评对象 ID 的形状确定性得出：
//! - completeness：ID 非空即 100（Schema 层已拒空，库内直调兜底 0）；
//! - provenance_coverage：ID 带对应族规范前缀（synthesis 为 syn-、citation
//!   为 cite-、plan 为 plan-）即 100，否则 40；
//! - consistency：ID 不超过 64 字符即 100，否则 60。
//!
//! overall_score 为均值向下取整，verdict 以 60 为阈值二分（PASS / REVIEW）。
//! Phase 8 接真实评估面时替换评分逻辑，契约面不变。

use crate::capabilities::CapabilityExec;
use crate::contract::{CapabilityFailure, CapabilityId, Correlation, ProvenanceAnchor};
use serde::{Deserialize, Serialize};

/// 被评对象族。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubjectKind {
    /// 综合成果（syn- 前缀）。
    Synthesis,
    /// 引文清单（cite- 前缀）。
    Citation,
    /// 研究计划（plan- 前缀）。
    Plan,
}

impl SubjectKind {
    /// 对应产出面的规范前缀。
    pub fn canonical_prefix(&self) -> &'static str {
        match self {
            SubjectKind::Synthesis => "syn-",
            SubjectKind::Citation => "cite-",
            SubjectKind::Plan => "plan-",
        }
    }

    /// 词表字符串（summary 文案用）。
    pub fn as_str(&self) -> &'static str {
        match self {
            SubjectKind::Synthesis => "synthesis",
            SubjectKind::Citation => "citation",
            SubjectKind::Plan => "plan",
        }
    }
}

/// 被评对象引用。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubjectRef {
    /// 对象族。
    pub kind: SubjectKind,
    /// 对象 ID（非空）。
    pub id: String,
}

/// 检查项词表。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluatorCheck {
    /// 完整性。
    Completeness,
    /// 锚覆盖率。
    ProvenanceCoverage,
    /// 一致性。
    Consistency,
}

/// evaluator 输入载荷。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluatorInput {
    /// 被评对象。
    pub subject: SubjectRef,
    /// 检查项（1 到 3 项，互异）。
    pub checks: Vec<EvaluatorCheck>,
}

/// 单项得分。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckScore {
    /// 检查项。
    pub check: EvaluatorCheck,
    /// 得分（0 到 100）。
    pub score: u8,
    /// 是否过线（mock 阈值 60）。
    pub passed: bool,
}

/// 评估判定词表。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum EvaluatorVerdict {
    /// 通过：overall_score 不低于 60。
    Pass,
    /// 转人工复核：低于 60。
    Review,
}

/// evaluator 输出载荷。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluatorOutput {
    /// 被评对象（原样回传）。
    pub subject: SubjectRef,
    /// 各检查项得分（与 checks 等长同序）。
    pub scores: Vec<CheckScore>,
    /// 总分（均值向下取整）。
    pub overall_score: u8,
    /// 判定。
    pub verdict: EvaluatorVerdict,
}

/// evaluator 能力标记类型。
pub struct EvaluatorCapability;

impl CapabilityExec for EvaluatorCapability {
    type Input = EvaluatorInput;
    type Output = EvaluatorOutput;
    const ID: CapabilityId = CapabilityId::Evaluator;

    fn run(
        _correlation: &Correlation,
        input: Self::Input,
    ) -> Result<(Self::Output, String, Vec<ProvenanceAnchor>), CapabilityFailure> {
        if input.checks.is_empty() {
            return Err(CapabilityFailure::validation(
                "CAP-EVALUATOR-NO-CHECKS",
                "checks 不得为空",
            ));
        }
        if input.subject.id.trim().is_empty() {
            return Err(CapabilityFailure::validation(
                "CAP-EVALUATOR-SUBJECT-EMPTY",
                "subject.id 不得为空白文本",
            ));
        }
        // 互异校验（Schema uniqueItems 同口径的 typed 层兜底）。
        let mut seen = std::collections::HashSet::new();
        for check in &input.checks {
            if !seen.insert(std::mem::discriminant(check)) {
                return Err(CapabilityFailure::validation(
                    "CAP-EVALUATOR-CHECK-DUPLICATE",
                    "checks 含重复检查项",
                ));
            }
        }
        let prefix = input.subject.kind.canonical_prefix();
        let scores: Vec<CheckScore> = input
            .checks
            .iter()
            .map(|check| {
                let score: u64 = match check {
                    EvaluatorCheck::Completeness => {
                        if input.subject.id.trim().is_empty() {
                            0
                        } else {
                            100
                        }
                    }
                    EvaluatorCheck::ProvenanceCoverage => {
                        if input.subject.id.starts_with(prefix) {
                            100
                        } else {
                            40
                        }
                    }
                    EvaluatorCheck::Consistency => {
                        if input.subject.id.chars().count() <= 64 {
                            100
                        } else {
                            60
                        }
                    }
                };
                CheckScore {
                    check: *check,
                    score: score as u8,
                    passed: score >= 60,
                }
            })
            .collect();
        let total: u64 = scores.iter().map(|s| s.score as u64).sum();
        let overall = (total / scores.len() as u64) as u8;
        let verdict = if overall >= 60 {
            EvaluatorVerdict::Pass
        } else {
            EvaluatorVerdict::Review
        };
        let check_count = scores.len();
        let summary = format!(
            "评估 {} {}：overall {overall} / verdict {}（mock 启发式，检查项 {check_count} 项）",
            input.subject.kind.as_str(),
            input.subject.id,
            match verdict {
                EvaluatorVerdict::Pass => "PASS",
                EvaluatorVerdict::Review => "REVIEW",
            }
        );
        Ok((
            EvaluatorOutput {
                subject: input.subject,
                scores,
                overall_score: overall,
                verdict,
            },
            summary,
            Vec::new(),
        ))
    }
}
