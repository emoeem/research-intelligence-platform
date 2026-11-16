//! 执行调度面（RIP3 Phase 5 P5-6）：派发循环的调度决策纯函数层。
//!
//! 职责边界：工具派发本体在 provider.rs（RtisPoc/Nuclei）与 rp-verify
//! execute-plan 循环；本模块只承载调度决策（可测纯函数），与五态判定
//! ([`crate::evaluator`])正交：
//! - 幂等恢复：只派发未判定项（result_status 为 NULL）——服务重启或暂停
//!   恢复后重跑 execute-plan 不重复派发已完成项（P5-6 完成标准「服务重启
//!   后不出现重复派发」）；
//! - 运行期执行侧预算复核（P5-4 G 遗留落地）：已派发消耗 + 本项预算 ≤
//!   计划 cap，是 planner 生成侧封顶（Σ fresh ≤ cap）之外的第二道闸
//!   （§5.9「限制请求量与总预算」）；
//! - 预算耗尽项归类：NOT_TESTED + budget_exhausted（未执行 ≠ 任何结论；
//!   覆盖缺口留痕，不得统计为通过）。

use crate::store::PlanItemRecord;

/// 幂等恢复筛选：返回 (待派发项, 已判定跳过数)。
///
/// 已判定 = result_status 非空（五态任意写入即该轮派发面已完成；重启/
/// 恢复重跑时原样跳过，结论不重算不覆盖）。
pub fn pending_items(items: Vec<PlanItemRecord>) -> (Vec<PlanItemRecord>, usize) {
    let skipped_done = items.iter().filter(|i| i.result_status.is_some()).count();
    let pending: Vec<PlanItemRecord> = items
        .into_iter()
        .filter(|i| i.result_status.is_none())
        .collect();
    (pending, skipped_done)
}

/// 运行期预算复核：`None` = 计划未设 cap（单项 request_budget 仍生效）；
/// `Some(cap)` 时 `consumed + item_budget <= cap` 才放行。consumed 为本轮
/// 已放行项的 request_budget 之和（执行侧镜像 planner 的 Σ≤cap 口径）。
pub fn runtime_budget_ok(consumed: i64, item_budget: i64, plan_cap: Option<i64>) -> bool {
    match plan_cap {
        None => true,
        Some(cap) => consumed + item_budget <= cap,
    }
}

/// 预算耗尽项的留痕结论（NOT_TESTED + budget_exhausted；reason 带剩余量
/// 与 cap，供 UI 与补充研究消费）。
pub fn budget_exhausted_result(consumed: i64, item_budget: i64, plan_cap: i64) -> NormalizedOutcome {
    NormalizedOutcome {
        result_status: domain::verification::ResultStatus::NotTested.as_str().to_owned(),
        result_reason: format!(
            "runtime budget exhausted: consumed {consumed} + item {item_budget} exceeds plan cap {plan_cap}; item not executed"
        ),
        error_category: Some(
            domain::verification::ErrorCategory::BudgetExhausted.as_str().to_owned(),
        ),
    }
}

/// 调度面结论（set_result 散字段承载；复用 evaluator::NormalizedResult 的
/// 字段形状但独立定义——调度决策不经过 ProviderOutcome 判定链）。
#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedOutcome {
    pub result_status: String,
    pub result_reason: String,
    pub error_category: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::PlanItemRecord;

    fn item(item_id: &str, result_status: Option<&str>, request_budget: i64) -> PlanItemRecord {
        PlanItemRecord {
            schema_version: 1,
            item_id: item_id.to_owned(),
            plan_id: "vp-20261117-010203-004-abcd".to_owned(),
            knowledge_ref: None,
            rtis_poc_id: None,
            provider_id: "nuclei".to_owned(),
            provider_version: None,
            target_ref: "http://127.0.0.1:1".to_owned(),
            dedup_fingerprint: "a".repeat(64),
            applicability_status: "applicable".to_owned(),
            applicability_reasons: vec![],
            risk_class: "low_impact".to_owned(),
            request_budget,
            timeout_budget_s: 60,
            execution_order: 0,
            expected_observation: None,
            result_status: result_status.map(str::to_owned),
            result_reason: None,
            error_category: None,
            tool_run_id: None,
            trace_id: None,
            created_at: "2026-11-17T01:02:03Z".to_owned(),
            updated_at: "2026-11-17T01:02:03Z".to_owned(),
        }
    }

    /// 幂等恢复：已判定项（五态任意）跳过，未判定项原样保留。
    #[test]
    fn pending_items_skips_resolved_and_keeps_pending() {
        let items = vec![
            item("vi-20261117-010203-004-0001", Some("NOT_REPRODUCED"), 10),
            item("vi-20261117-010203-004-0002", Some("INCONCLUSIVE"), 10),
            item("vi-20261117-010203-004-0003", None, 10),
            item("vi-20261117-010203-004-0004", None, 5),
        ];
        let (pending, skipped) = pending_items(items);
        assert_eq!(skipped, 2, "resolved items must not be redispatched");
        assert_eq!(
            pending.iter().map(|i| i.item_id.as_str()).collect::<Vec<_>>(),
            ["vi-20261117-010203-004-0003", "vi-20261117-010203-004-0004"]
        );
    }

    /// 幂等恢复空面：全部已判定的恢复重跑 = 零派发。
    #[test]
    fn pending_items_all_resolved_yields_empty_dispatch() {
        let items = vec![item("vi-20261117-010203-004-0001", Some("NOT_TESTED"), 10)];
        let (pending, skipped) = pending_items(items);
        assert!(pending.is_empty());
        assert_eq!(skipped, 1);
    }

    /// 运行期预算复核：Σ 放行 + 本项 ≤ cap 才放行（执行侧镜像生成侧封顶）。
    #[test]
    fn runtime_budget_gate() {
        assert!(runtime_budget_ok(0, 10, Some(15)));
        assert!(runtime_budget_ok(5, 10, Some(15)));
        assert!(!runtime_budget_ok(10, 10, Some(15)), "consumed+item must respect cap");
        assert!(!runtime_budget_ok(15, 1, Some(15)));
        // 无 cap 计划不设执行侧门（单项预算仍由派发面约束）。
        assert!(runtime_budget_ok(100, 10, None));
    }

    /// 预算耗尽留痕：NOT_TESTED + budget_exhausted + 剩余量说明在场。
    #[test]
    fn budget_exhausted_outcome_is_not_tested_with_category() {
        let outcome = budget_exhausted_result(10, 10, 15);
        assert_eq!(outcome.result_status, "NOT_TESTED");
        assert_eq!(outcome.error_category.as_deref(), Some("budget_exhausted"));
        assert!(outcome.result_reason.contains("cap 15"));
        // 与 store ResultGuard 联动自洽：error_category 在场时 NOT_TESTED 合法
        // （守卫只禁 NOT_REPRODUCED 与 error_category 同场）。
        let guard = crate::store::ResultGuard::check(
            &outcome.result_status,
            Some(&outcome.result_reason),
            outcome.error_category.as_deref(),
        );
        assert!(guard.is_ok(), "budget exhausted outcome must pass ResultGuard");
    }
}
