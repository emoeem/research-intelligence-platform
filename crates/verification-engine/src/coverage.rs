//! 覆盖矩阵与自适应研究（RIP3 Phase 5 P5-8，补充计划 §7 P5-8 + §5.10）。
//!
//! 职责边界：本模块是覆盖矩阵推导与假设生成规则层（纯函数 + store 写面
//! 组合），与 evaluator 五态判定、scheduler 调度决策正交：
//! - [`coverage_matrix`]：从计划项推导七维覆盖格（每维 confirmed/
//!   not_reproduced/inconclusive/not_tested/false_positive 计数）；完成
//!   标准「未测试项与失败项分开显示」「无命中不等于无漏洞」由五态分维度
//!   计数承载；
//! - 盲区（blind spot）：有项但全部未判定或零项的维度——自适应研究的
//!   输入信号；
//! - [`generate_blind_spot_hypotheses`]：盲区维度生成假设草稿（Research
//!   Hypothesis，不是 Finding；§5.10 纪律「假设没有证据时不能生成
//!   Finding」，source_evidence 必填由 store 守卫强制）；
//! - 轮次预算：max_rounds 守卫在 store.add_hypothesis（round 超限拒绝，
//!   停止原因留痕「round budget exhausted」）。

use crate::store::{NewHypothesis, PlanItemRecord};
use domain::verification::CoverageDimension;

/// 单维覆盖格（五态分计数；untested = 全部项都还没结论）。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CoverageCell {
    pub dimension: &'static str,
    pub item_count: usize,
    pub confirmed: usize,
    pub not_reproduced: usize,
    pub inconclusive: usize,
    pub not_tested: usize,
    pub false_positive: usize,
}

impl CoverageCell {
    /// 该维是否盲区：零项，或有项但没有任何非 NOT_TESTED 的结论。
    pub fn is_blind_spot(&self) -> bool {
        self.item_count == 0
            || (self.confirmed
                + self.not_reproduced
                + self.inconclusive
                + self.false_positive)
                == 0
    }

    /// 盲区形态（零项 = 未覆盖；有项无结论 = 未判定）。停止原因可见性面。
    pub fn blind_spot_kind(&self) -> &'static str {
        if self.item_count == 0 {
            "uncovered (no plan items in this dimension)"
        } else {
            "unjudged (items present but no conclusive five-state result)"
        }
    }
}

/// 七维覆盖矩阵。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CoverageMatrix {
    pub cells: Vec<CoverageCell>,
    /// 盲区维度词值列表（uncovered 优先于 unjudged 排序面）。
    pub blind_spots: Vec<&'static str>,
}

/// 从计划项推导七维覆盖矩阵（P5-8 完成标准的推导面；items 全量传入，
/// excluded 项也计入 item_count 但不算结论——排除是计划决策不是测试事实）。
pub fn coverage_matrix(items: &[PlanItemRecord]) -> CoverageMatrix {
    let mut cells = Vec::with_capacity(CoverageDimension::ALL.len());
    let mut blind_spots = Vec::new();
    for dimension in CoverageDimension::ALL {
        let mut cell = CoverageCell {
            dimension: dimension.as_str(),
            item_count: 0,
            confirmed: 0,
            not_reproduced: 0,
            inconclusive: 0,
            not_tested: 0,
            false_positive: 0,
        };
        for item in items {
            if item.dimension != dimension.as_str() {
                continue;
            }
            cell.item_count += 1;
            match item.result_status.as_deref() {
                Some("CONFIRMED") => cell.confirmed += 1,
                Some("NOT_REPRODUCED") => cell.not_reproduced += 1,
                Some("INCONCLUSIVE") => cell.inconclusive += 1,
                Some("NOT_TESTED") => cell.not_tested += 1,
                Some("FALSE_POSITIVE") => cell.false_positive += 1,
                _ => {}
            }
        }
        if cell.is_blind_spot() {
            blind_spots.push(dimension.as_str());
        }
        cells.push(cell);
    }
    CoverageMatrix { cells, blind_spots }
}

/// 盲区维度假设草稿（自适应研究引擎 §5.10）：每个盲区维度一条 Research
/// Hypothesis，来源依据 = 覆盖矩阵盲区信号（coverage blind-spot signal，
/// 结构性证据而非编造），负向对照与停止条件内置默认面（round budget +
/// 无证据不升级 Finding）；建议验证面按维度给出 Provider 语义提示。
/// 调用面负责 round 编号与 store.add_hypothesis（max_rounds 守卫）。
pub fn generate_blind_spot_hypotheses(
    matrix: &CoverageMatrix,
    target_ref: &str,
) -> Vec<NewHypothesis> {
    matrix
        .cells
        .iter()
        .filter(|cell| cell.is_blind_spot())
        .map(|cell| {
            let hint = match cell.dimension {
                "api_endpoints" => {
                    "endpoint inventory first (unified search over crawled/observed endpoints), then template probe on missing methods"
                }
                "identity_permissions" => {
                    "auth surface walk (login/session/role boundaries) with low-impact probes only"
                }
                "input_points" => {
                    "input point census (params/headers/body) then matcher pass before any active probe"
                }
                "business_flows" => {
                    "business flow walkthrough (state transitions/race windows) as passive analysis first"
                }
                "file_processing" => {
                    "file upload/parse surface census with harmless extension probes"
                }
                "data_flows" => {
                    "sensitive data flow tracing (storage/transport/logging) as passive inspection"
                }
                _ => "asset enumeration refresh (host/port/service discovery under current conditions)",
            };
            NewHypothesis {
                dimension: cell.dimension.to_owned(),
                content: format!(
                    "dimension {} is a blind spot for target {target_ref}: no conclusive verification result exists under current plan conditions",
                    cell.dimension
                ),
                source_evidence: format!(
                    "coverage blind-spot signal: {} ({})",
                    cell.blind_spot_kind(),
                    if cell.item_count == 0 {
                        "0 items".to_owned()
                    } else {
                        format!("{}/{} items unresolved", cell.not_tested, cell.item_count)
                    }
                ),
                association_reason:
                    "target coverage requires at least one conclusive observation per dimension before any safety conclusion"
                        .to_owned(),
                preconditions: Some("plan ready; provider available; budget remaining".to_owned()),
                expected_observation: Some(
                    "five-state result on at least one plan item in this dimension (NOT_REPRODUCED counts as observed, absence of findings must not be inferred as absence of vulnerabilities)".to_owned(),
                ),
                negative_control: Some(
                    "clean-run control: template with never-matching matcher must stay NOT_REPRODUCED".to_owned(),
                ),
                suggested_verification: Some(hint.to_owned()),
                cost_risk: Some("low impact (passive/controlled probe first; default risk_class low_impact)".to_owned()),
                stop_condition: "round budget exhausted (max_rounds) or dimension gains a conclusive five-state result".to_owned(),
                round_number: 0, // 调用面填当前轮次
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::PlanItemRecord;

    fn item(dimension: &str, result_status: Option<&str>) -> PlanItemRecord {
        PlanItemRecord {
            schema_version: 1,
            item_id: format!("vi-20261117-000000-000-{dimension}"),
            plan_id: "vp-20261117-000000-000-test".to_owned(),
            knowledge_ref: None,
            rtis_poc_id: None,
            provider_id: "nuclei".to_owned(),
            provider_version: None,
            target_ref: "http://127.0.0.1:1".to_owned(),
            dedup_fingerprint: "a".repeat(64),
            applicability_status: "applicable".to_owned(),
            applicability_reasons: vec![],
            risk_class: "low_impact".to_owned(),
            request_budget: 10,
            timeout_budget_s: 60,
            execution_order: 0,
            expected_observation: None,
            dimension: dimension.to_owned(),
            result_status: result_status.map(str::to_owned),
            result_reason: None,
            error_category: None,
            tool_run_id: None,
            trace_id: None,
            created_at: "2026-11-17T00:00:00Z".to_owned(),
            updated_at: "2026-11-17T00:00:00Z".to_owned(),
        }
    }

    /// 未测试项与失败项分开显示：五态分计数逐维正确。
    #[test]
    fn matrix_counts_five_states_per_dimension() {
        let items = vec![
            item("assets", Some("NOT_REPRODUCED")),
            item("assets", Some("INCONCLUSIVE")),
            item("input_points", Some("NOT_TESTED")),
            item("input_points", Some("CONFIRMED")),
            item("api_endpoints", None),
        ];
        let matrix = coverage_matrix(&items);
        let assets = matrix.cells.iter().find(|c| c.dimension == "assets").unwrap();
        assert_eq!(assets.item_count, 2);
        assert_eq!(assets.not_reproduced, 1);
        assert_eq!(assets.inconclusive, 1);
        let input = matrix.cells.iter().find(|c| c.dimension == "input_points").unwrap();
        assert_eq!(input.not_tested, 1);
        assert_eq!(input.confirmed, 1);
        let api = matrix.cells.iter().find(|c| c.dimension == "api_endpoints").unwrap();
        assert_eq!(api.item_count, 1);
        assert_eq!(api.not_tested, 0);
        assert!(api.is_blind_spot(), "unjudged dimension is a blind spot");
    }

    /// 盲区判定：零维未覆盖；有项全部未判定 = unjudged；NOT_TESTED 不算结论。
    #[test]
    fn blind_spot_kinds() {
        let items = vec![
            item("assets", Some("NOT_TESTED")),
            item("data_flows", Some("FALSE_POSITIVE")),
        ];
        let matrix = coverage_matrix(&items);
        assert!(matrix.blind_spots.contains(&"assets"), "NOT_TESTED-only dimension is blind");
        assert!(matrix.blind_spots.contains(&"api_endpoints"), "zero-item dimension is blind");
        assert!(!matrix.blind_spots.contains(&"data_flows"), "FALSE_POSITIVE is a conclusion");
        let assets = matrix.cells.iter().find(|c| c.dimension == "assets").unwrap();
        assert_eq!(assets.blind_spot_kind(), "unjudged (items present but no conclusive five-state result)");
    }

    /// 假设草稿：每个盲区维度一条；来源依据带盲区信号；负向对照与停止
    /// 条件内置；round_number 留给调用面。
    #[test]
    fn hypotheses_generated_for_blind_spots_only() {
        let items = vec![item("assets", Some("NOT_REPRODUCED"))];
        let matrix = coverage_matrix(&items);
        let drafts = generate_blind_spot_hypotheses(&matrix, "http://t");
        // assets 有结论不生成；其余六维全盲 → 六条草稿
        assert_eq!(drafts.len(), 6);
        assert!(drafts.iter().all(|h| !h.dimension.is_empty()));
        assert!(drafts.iter().all(|h| h.source_evidence.contains("coverage blind-spot signal")));
        assert!(drafts.iter().all(|h| h.stop_condition.contains("max_rounds")));
        assert!(drafts.iter().all(|h| h.negative_control.is_some()));
    }
}
