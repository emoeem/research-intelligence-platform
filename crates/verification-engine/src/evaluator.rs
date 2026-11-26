//! 结果归一化（RIP3 Phase 5 P5-7）：ProviderOutcome → 五态研究结论判定。
//!
//! 五态纪律硬约束（补充计划 §5.7 原文判据，逐条落为规则）：
//! 1. **工具退出码 0 ≠ CONFIRMED**——干净无命中 = NOT_REPRODUCED（本次
//!    条件下未复现），命中 = 证据待复核 INCONCLUSIVE（自动确认禁令）；
//! 2. **执行失败/超时 ≠ NOT_REPRODUCED**——网络失败/超时 → INCONCLUSIVE
//!    且 error_category 在场（与 P5-2 store 守卫联动：error_category 在场
//!    时写 NOT_REPRODUCED 会被拒）；
//! 3. **工具不可用 ≠ NOT_REPRODUCED**——NOT_TESTED（记录覆盖缺口）；
//! 4. **取消 ≠ 任何结论**——NOT_TESTED（未执行完成）；
//! 5. FALSE_POSITIVE 只能由证据复核显式判定（本层不自动产出）。
//!
//! result-normalized.v1 契约（schemas/verification/result-normalized.v1.json）
//! 承载归一化输出的形状（schema_version 冻结为 1，契约测试钉面）。

use crate::contract::SCHEMA_VERSION;
use domain::verification::ResultStatus;
use serde::Serialize;
use serde_json::json;

/// 归一化判定输出（写 store.set_result 前的中间面）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct NormalizedResult {
    pub schema_version: i64,
    /// 五态研究结论。
    pub result_status: String,
    /// 判定依据（CONFIRMED/FALSE_POSITIVE 必填约束由 store 守卫承载）。
    pub result_reason: String,
    /// 失败/超时分类词值（None = 无 error_category；与 store 守卫联动）。
    pub error_category: Option<String>,
}

/// ProviderOutcome 归一化输入面（窄化：只消费判定所需字段）。
pub struct OutcomeView<'a> {
    pub status_word: &'a str,
    /// Provider 结果帧（SUCCEEDED 时为 object：matched_count 等归一化键）。
    pub result: Option<&'a serde_json::Value>,
    /// 容器/进程摘要（失败分类辅助文本）。
    pub summary: &'a str,
}

/// 归一化判定（§5.7 五态纪律规则的唯一实现点）。
pub fn evaluate(view: &OutcomeView) -> NormalizedResult {
    let base = NormalizedResult {
        schema_version: SCHEMA_VERSION,
        result_status: ResultStatus::NotTested.as_str().to_owned(),
        result_reason: String::new(),
        error_category: None,
    };
    match view.status_word {
        "SUCCEEDED" => {
            let matched = view
                .result
                .and_then(|r| r.get("matched_count"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            if matched == 0 {
                NormalizedResult {
                    result_status: ResultStatus::NotReproduced.as_str().to_owned(),
                    result_reason:
                        "clean run (exit 0): template loaded, zero matches under current conditions"
                            .to_owned(),
                    error_category: None,
                    ..base
                }
            } else {
                // 退出码 0 + 命中 ≠ CONFIRMED：模板命中是信号，确认要证据复核
                // （expected_observation 对齐或人工判定，FALSE_POSITIVE 同源）。
                NormalizedResult {
                    result_status: ResultStatus::Inconclusive.as_str().to_owned(),
                    result_reason: format!(
                        "template matched {matched} finding(s): evidence review required before any conclusion"
                    ),
                    error_category: None,
                    ..base
                }
            }
        }
        "TIMED_OUT" => NormalizedResult {
            result_status: ResultStatus::Inconclusive.as_str().to_owned(),
            result_reason: format!(
                "timeout: execution did not complete under budget ({}); absence of findings must not be inferred",
                truncate(view.summary, 120)
            ),
            error_category: Some(domain::verification::ErrorCategory::Timeout.as_str().to_owned()),
            ..base
        },
        "CANCELLED" => NormalizedResult {
            result_status: ResultStatus::NotTested.as_str().to_owned(),
            result_reason: "cancelled before completion (user/coordinator stop); no conclusion"
                .to_owned(),
            error_category: None,
            ..base
        },
        // FAILED 与其他词值：按 summary 的失败文本粗分类（provider 层已映射
        // error_category 的词值直接透传；此处兜底 EXECUTION 类）。
        _ => {
            let lower = view.summary.to_lowercase();
            let (status, category, reason) = if lower.contains("tool_unavailable")
                || lower.contains("image")
                || lower.contains("spawn")
            {
                (
                    ResultStatus::NotTested.as_str(),
                    domain::verification::ErrorCategory::ToolUnavailable.as_str(),
                    "tool/provider unavailable: coverage gap recorded, no conclusion",
                )
            } else if lower.contains("network") || lower.contains("connect") || lower.contains("dns")
            {
                (
                    ResultStatus::Inconclusive.as_str(),
                    domain::verification::ErrorCategory::Network.as_str(),
                    "network failure during execution: inconclusive",
                )
            } else if lower.contains("auth") {
                (
                    ResultStatus::Inconclusive.as_str(),
                    domain::verification::ErrorCategory::Auth.as_str(),
                    "authentication/session failure: inconclusive",
                )
            } else {
                (
                    ResultStatus::Inconclusive.as_str(),
                    domain::verification::ErrorCategory::Unknown.as_str(),
                    "execution failed: inconclusive (no findings must not be inferred)",
                )
            };
            NormalizedResult {
                result_status: status.to_owned(),
                result_reason: format!("{reason} ({})", truncate(view.summary, 120)),
                error_category: Some(category.to_owned()),
                ..base
            }
        }
    }
}

/// result-normalized.v1 文档（store set_result 事件 payload 消费）。
pub fn normalized_doc(item_id: &str, normalized: &NormalizedResult) -> serde_json::Value {
    json!({
        "schema_version": normalized.schema_version,
        "item_id": item_id,
        "result_status": normalized.result_status,
        "result_reason": normalized.result_reason,
        "error_category": normalized.error_category,
    })
}

fn truncate(value: &str, cap: usize) -> String {
    if value.chars().count() <= cap {
        value.to_owned()
    } else {
        let cut: String = value.chars().take(cap).collect();
        format!("{cut}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::validate_normalized_doc;
    use serde_json::json;

    fn view<'a>(
        status: &'a str,
        result: &'a Option<serde_json::Value>,
        summary: &'a str,
    ) -> OutcomeView<'a> {
        OutcomeView {
            status_word: status,
            result: result.as_ref(),
            summary,
        }
    }

    /// §11.3 场景 8 反面：退出码 0 ≠ CONFIRMED（干净无命中 = NOT_REPRODUCED）。
    #[test]
    fn exit_zero_clean_run_is_not_confirmed() {
        let normalized = evaluate(&view(
            "SUCCEEDED",
            &Some(json!({"matched_count": 0})),
            "clean",
        ));
        assert_eq!(normalized.result_status, "NOT_REPRODUCED");
        assert_eq!(
            normalized.result_reason,
            "clean run (exit 0): template loaded, zero matches under current conditions"
        );
        assert!(normalized.error_category.is_none());
    }

    /// 命中 ≠ CONFIRMED（自动确认禁令）：证据待复核 INCONCLUSIVE。
    #[test]
    fn matches_require_evidence_review() {
        let normalized = evaluate(&view(
            "SUCCEEDED",
            &Some(json!({"matched_count": 2})),
            "findings",
        ));
        assert_eq!(normalized.result_status, "INCONCLUSIVE");
        assert!(normalized
            .result_reason
            .contains("evidence review required"));
    }

    /// §11.3 场景 6：网络超时 ≠ NOT_REPRODUCED（INCONCLUSIVE + 分类在场）。
    #[test]
    fn timeout_is_inconclusive_not_not_reproduced() {
        let normalized = evaluate(&view("TIMED_OUT", &None, "watchdog fired"));
        assert_eq!(normalized.result_status, "INCONCLUSIVE");
        assert_eq!(normalized.error_category.as_deref(), Some("timeout"));
    }

    /// §11.3 场景 7：工具不可用 = NOT_TESTED（覆盖缺口，不冒结论）。
    #[test]
    fn tool_unavailable_is_not_tested() {
        let normalized = evaluate(&view("FAILED", &None, "submit rejected: image not present"));
        assert_eq!(normalized.result_status, "NOT_TESTED");
        assert_eq!(
            normalized.error_category.as_deref(),
            Some("tool_unavailable")
        );
    }

    /// 取消 = NOT_TESTED（无结论）。
    #[test]
    fn cancelled_is_not_tested() {
        let normalized = evaluate(&view("CANCELLED", &None, "cooperative cancel"));
        assert_eq!(normalized.result_status, "NOT_TESTED");
        assert!(normalized.error_category.is_none());
    }

    /// 网络失败 → INCONCLUSIVE + network 分类（§5.9 分类纪律）。
    #[test]
    fn network_failure_inconclusive() {
        let normalized = evaluate(&view("FAILED", &None, "connect: connection refused"));
        assert_eq!(normalized.result_status, "INCONCLUSIVE");
        assert_eq!(normalized.error_category.as_deref(), Some("network"));
    }

    /// 归一化文档形状（schema_version 与词值在位）。
    #[test]
    fn normalized_doc_shape() {
        let normalized = evaluate(&view("SUCCEEDED", &Some(json!({"matched_count": 0})), "x"));
        let doc = normalized_doc("vi-1", &normalized);
        assert_eq!(doc["schema_version"], 1);
        assert_eq!(doc["item_id"], "vi-1");
        assert_eq!(doc["result_status"], "NOT_REPRODUCED");
    }

    /// result-normalized.v1 契约正例矩阵：evaluator 全部产出形态（五态中
    /// 自动可产出的四态 + error_category 七词值中实产的三类）过 Schema。
    /// FALSE_POSITIVE/CONFIRMED 不在本层自动产出（纪律），其形状合法性由
    /// Schema 词表与 store ResultGuard 承载。
    #[test]
    fn normalized_output_conforms_to_contract() {
        let cases = [
            (
                "SUCCEEDED",
                Some(json!({"matched_count": 0})),
                "clean",
                "NOT_REPRODUCED",
            ),
            (
                "SUCCEEDED",
                Some(json!({"matched_count": 3})),
                "findings",
                "INCONCLUSIVE",
            ),
            ("TIMED_OUT", None, "watchdog fired", "INCONCLUSIVE"),
            ("CANCELLED", None, "cooperative cancel", "NOT_TESTED"),
            (
                "FAILED",
                None,
                "submit rejected: image not present",
                "NOT_TESTED",
            ),
            (
                "FAILED",
                None,
                "connect: connection refused",
                "INCONCLUSIVE",
            ),
        ];
        for (status_word, result, summary, expected_status) in cases {
            let normalized = evaluate(&view(status_word, &result, summary));
            assert_eq!(
                normalized.result_status, expected_status,
                "status mapping drifted for {status_word}/{summary}"
            );
            let doc = normalized_doc("vi-20261117-120000-001-abcd", &normalized);
            validate_normalized_doc(&doc)
                .unwrap_or_else(|errs| panic!("contract rejected {doc:?}: {errs:?}"));
        }
    }

    /// result-normalized.v1 契约负例：工具词值混入五态字段、分类词表外、
    /// 空 reason、版本漂移、ID 形状违规、多余字段一律拒绝。
    #[test]
    fn contract_rejects_malformed_normalized_docs() {
        let base = json!({
            "schema_version": 1,
            "item_id": "vi-20261117-120000-001-abcd",
            "result_status": "NOT_REPRODUCED",
            "result_reason": "clean run",
            "error_category": null,
        });
        assert!(validate_normalized_doc(&base).is_ok());
        let rejects = [
            (
                "工具词值混入 result_status",
                json!({
                    "schema_version": 1,
                    "item_id": "vi-20261117-120000-001-abcd",
                    "result_status": "SUCCEEDED",
                    "result_reason": "exit 0",
                    "error_category": null,
                }),
            ),
            (
                "error_category 词表外",
                json!({
                    "schema_version": 1,
                    "item_id": "vi-20261117-120000-001-abcd",
                    "result_status": "INCONCLUSIVE",
                    "result_reason": "failed",
                    "error_category": "crash",
                }),
            ),
            (
                "空 reason",
                json!({
                    "schema_version": 1,
                    "item_id": "vi-20261117-120000-001-abcd",
                    "result_status": "NOT_TESTED",
                    "result_reason": "",
                    "error_category": null,
                }),
            ),
            (
                "schema_version 漂移",
                json!({
                    "schema_version": 2,
                    "item_id": "vi-20261117-120000-001-abcd",
                    "result_status": "NOT_TESTED",
                    "result_reason": "x",
                    "error_category": null,
                }),
            ),
            (
                "item_id 形状违规",
                json!({
                    "schema_version": 1,
                    "item_id": "vi-1",
                    "result_status": "NOT_TESTED",
                    "result_reason": "x",
                    "error_category": null,
                }),
            ),
            (
                "多余字段",
                json!({
                    "schema_version": 1,
                    "item_id": "vi-20261117-120000-001-abcd",
                    "result_status": "NOT_TESTED",
                    "result_reason": "x",
                    "error_category": null,
                    "matched_count": 0,
                }),
            ),
        ];
        for (label, doc) in rejects {
            assert!(
                validate_normalized_doc(&doc).is_err(),
                "contract must reject: {label}"
            );
        }
    }
}
