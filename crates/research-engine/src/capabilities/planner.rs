//! Research Planner（研究计划）确定性 mock。
//!
//! 边界声明：固定六步流水线（发现、抽取、核验、综合、引文、评估）按
//! `max_steps` 截断；plan_id / goal_digest / step_id 全部由 sha256 对输入
//! 确定性派生（同输入恒同输出，无时钟无随机）。Phase 8 以 agent_loop 真实
//! 规划替换实现体时，request-v1/result-v1 契约面不变。

use crate::capabilities::CapabilityExec;
use crate::contract::{
    digest16, CapabilityFailure, CapabilityId, Correlation, ProvenanceAnchor,
};
use serde::{Deserialize, Serialize};

/// 计划步骤指向的下游能力词表（含 tool.call 占位；不含 research.planner 自指）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepCapability {
    /// 源发现步骤。
    #[serde(rename = "research.source-discovery")]
    SourceDiscovery,
    /// 证据抽取步骤。
    #[serde(rename = "research.evidence-extraction")]
    EvidenceExtraction,
    /// 核验步骤。
    #[serde(rename = "research.verification")]
    Verification,
    /// 综合步骤。
    #[serde(rename = "research.synthesis")]
    Synthesis,
    /// 引文步骤。
    #[serde(rename = "research.citation")]
    Citation,
    /// 评估步骤。
    #[serde(rename = "research.evaluator")]
    Evaluator,
    /// 平台工具调用占位（Phase 8 真实编排接入）。
    #[serde(rename = "tool.call")]
    ToolCall,
}

/// planner 输入载荷（request-v1 input 同形）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerInput {
    /// 研究目标文本（非空）。
    pub goal: String,
    /// 计划步数上限（缺省取全流水线 6 步）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_steps: Option<u8>,
    /// 工作区隔离边界（透传锚，mock 不消费）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
}

/// 计划步骤。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanStep {
    /// 步骤 ID（plan- 内确定性派生）。
    pub step_id: String,
    /// 步骤标题（mock 固定模板）。
    pub title: String,
    /// 该步指向的下游能力。
    pub capability: StepCapability,
    /// 该步存在理由（mock 固定模板）。
    pub rationale: String,
}

/// planner 输出载荷（result-v1 result 同形）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerOutput {
    /// 计划 ID（goal 确定性派生）。
    pub plan_id: String,
    /// 目标文本摘要。
    pub goal_digest: String,
    /// 计划步骤序列。
    pub steps: Vec<PlanStep>,
    /// 步骤数。
    pub step_count: usize,
}

/// planner 能力标记类型。
pub struct PlannerCapability;

/// 固定六步流水线（capability，标题，理由）。
const PIPELINE: [(StepCapability, &str, &str); 6] = [
    (
        StepCapability::SourceDiscovery,
        "发现知识源与既有证据入口",
        "先定位可用面再动手：无源则后续步骤无输入（mock 固定流水线第 1 步）",
    ),
    (
        StepCapability::EvidenceExtraction,
        "抽取证据与产物",
        "把原始材料固化为可引用的证据锚（mock 固定流水线第 2 步）",
    ),
    (
        StepCapability::Verification,
        "核验断言与证据支撑",
        "未核验的结论不进综合（mock 固定流水线第 3 步）",
    ),
    (
        StepCapability::Synthesis,
        "综合成结构化研究产出",
        "把通过核验的证据组装为主题化段落（mock 固定流水线第 4 步）",
    ),
    (
        StepCapability::Citation,
        "为综合成果生成引文锚",
        "结论必须可回链到证据（mock 固定流水线第 5 步）",
    ),
    (
        StepCapability::Evaluator,
        "评估产出质量",
        "按检查项评分收口，低于阈值转人工复核（mock 固定流水线第 6 步）",
    ),
];

impl CapabilityExec for PlannerCapability {
    type Input = PlannerInput;
    type Output = PlannerOutput;
    const ID: CapabilityId = CapabilityId::Planner;

    fn run(
        _correlation: &Correlation,
        input: Self::Input,
    ) -> Result<(Self::Output, String, Vec<ProvenanceAnchor>), CapabilityFailure> {
        if input.goal.trim().is_empty() {
            return Err(CapabilityFailure::validation(
                "CAP-PLANNER-GOAL-EMPTY",
                "goal 不得为空白文本",
            ));
        }
        // Schema 层 floor 为 2；typed 层对库内直调放宽到流水线下界并同样截断。
        let max_steps = input
            .max_steps
            .map(|v| v as usize)
            .unwrap_or(PIPELINE.len())
            .clamp(1, PIPELINE.len());
        let plan_id = format!("plan-{}", digest16(&["plan", &input.goal]));
        let goal_digest = digest16(&["goal", &input.goal]);
        let steps: Vec<PlanStep> = PIPELINE
            .iter()
            .take(max_steps)
            .enumerate()
            .map(|(index, (capability, title, rationale))| PlanStep {
                step_id: format!("step-{}", digest16(&[&plan_id, &index.to_string()])),
                title: (*title).to_string(),
                capability: *capability,
                rationale: (*rationale).to_string(),
            })
            .collect();
        let step_count = steps.len();
        let plan_id_out = plan_id.clone();
        let summary = format!(
            "生成研究计划 {plan_id}：{step_count} 步，目标摘要 {goal_digest}（mock 固定流水线）"
        );
        Ok((
            PlannerOutput {
                plan_id: plan_id_out,
                goal_digest,
                steps,
                step_count,
            },
            summary,
            Vec::new(),
        ))
    }
}
