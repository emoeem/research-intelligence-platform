//! Verification Planner（RIP3 Phase 5 P5-4）：把候选知识转成有限、可解释、
//! 可去重的检测计划。
//!
//! 职责（补充计划 §5.5）：
//! 1. **适用性过滤**：消费 Matcher 的命中/排除（P5-3 结构化判定输出），
//!    excluded 候选不生成执行项，只作为跳过原因记录；
//! 2. **重复指纹**：每条计划项生成 dedup_fingerprint（P5-2 六段指纹），
//!    并经 [`PlanStore::find_latest_by_fingerprint`] 查历史已执行项；
//! 3. **缓存决策**：同指纹历史项存在且为 CONFIRMED/NOT_REPRODUCED（条件
//!    未变化）→ cache_decision=cache_hit，跳过执行并引用历史证据；指纹
//!    不同（POC/Provider/画像/参数任一变化）→ fresh——缓存只用于避免
//!    重复执行，不把历史结果当当前实时证据（§5.8 纪律）；
//! 4. **预算后端生效**：计划项预算之和受计划预算封顶（本函数在生成侧
//!    裁剪；执行侧复核在 P5-6 派发面），超预算候选 → skipped
//!    （budget_exhausted），不是静默丢弃；
//! 5. **风险与成本排序**：passive → low_impact → active_controlled，
//!    同级按渠道相关度分降序（低影响优先，§5.5 规则 5）；
//! 6. **执行前预览**：输出 PlanDraft（含执行项/缓存命中/跳过三面 + 每条
//!    原因），Start 前可完整展示。

use crate::fingerprint::dedup_fingerprint;
use crate::matcher::MatchResult;
use crate::store::{NewItem, PlanStore, PlanStoreError};
use serde::Serialize;
use serde_json::Value;

/// 计划预算（planning 侧生效；执行侧 P5-6 复核）。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PlanBudget {
    /// 计划总请求预算（Σ item.request_budget ≤ max_requests）。
    #[serde(default = "default_max_requests")]
    pub max_requests: i64,
    /// 单项默认超时（秒）。
    #[serde(default = "default_timeout")]
    pub timeout_s: i64,
    /// 并发上限（默认 1，低并发纪律）。
    #[serde(default = "default_concurrency")]
    pub concurrency: i64,
    /// 假设研究最大轮次（P5-8 §5.10；默认 3，防低价值假设无限探索）。
    #[serde(default = "default_max_rounds")]
    pub max_rounds: i64,
}

fn default_max_requests() -> i64 {
    30
}
fn default_timeout() -> i64 {
    60
}
fn default_concurrency() -> i64 {
    1
}
fn default_max_rounds() -> i64 {
    3
}

impl PlanBudget {
    pub fn to_value(&self) -> Value {
        serde_json::json!({
            "max_requests": self.max_requests,
            "timeout_s": self.timeout_s,
            "concurrency": self.concurrency,
            "max_rounds": self.max_rounds,
        })
    }
}

/// 计划项风险序（planner 排序键第一维）。
fn risk_rank(risk_class: &str) -> u8 {
    match risk_class {
        "passive" => 0,
        "low_impact" => 1,
        _ => 2,
    }
}

/// 缓存决策（§5.8：缓存只避免重复执行，不当实时证据）。
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CacheDecision {
    /// 无同指纹历史，需执行。
    Fresh,
    /// 同指纹已执行且五态终局——跳过执行并引用历史项（证据来源即历史项）。
    CacheHit {
        prior_item_id: String,
        prior_result: String,
    },
}

/// 一条已裁剪的执行项（NewItem + 预览语义字段）。
#[derive(Debug, Clone, Serialize)]
pub struct PlannedItem {
    pub item: NewItem,
    pub cache_decision: CacheDecision,
    /// 预算来源：计划总预算余量裁剪后的单项预算。
    pub budget_source: String,
}

/// 计划项被跳过的原因面（excluded / 预算耗尽 / 信息不足）。
#[derive(Debug, Clone, Serialize)]
pub struct SkippedCandidate {
    pub channel: &'static str,
    pub ref_id: String,
    pub title: String,
    pub skip_reasons: Vec<String>,
}

/// 执行前预览（Plan Draft）。
#[derive(Debug, Clone, Serialize)]
pub struct PlanDraft {
    pub target_ref: String,
    pub planned: Vec<PlannedItem>,
    pub skipped: Vec<SkippedCandidate>,
    /// 计划总请求预算（planning 侧已封顶校验后的实际 Σ）。
    pub total_request_budget: i64,
    /// 计划总预算帽（来自 PlanBudget.max_requests）。
    pub budget_cap: i64,
    pub budget: Value,
}

/// 风险分级推断（§5.5 规则 5：默认低影响；Provider 能力不得升级风险）。
fn infer_risk_class(hit_channel: &str, severity: Option<&str>) -> &'static str {
    // P5-2 词表三值；rtis-poc 元数据面默认 low_impact（受控主动检测由
    // 调用方显式升级，planner 不自动升级风险）。
    let _ = severity;
    match hit_channel {
        "pg-intel" => "low_impact",
        _ => "low_impact",
    }
}

/// 把 Matcher 结果转成计划草稿（不落库；落库走 [`PlanStore::add_item`]）。
pub fn draft_from_match(
    match_result: &MatchResult,
    budget: &PlanBudget,
    store: &PlanStore,
    poc_ref_for: impl Fn(&crate::matcher::MatchHit) -> String,
    params_json: &str,
    authctx: &str,
) -> PlanDraft {
    let mut planned: Vec<PlannedItem> = Vec::new();
    let mut skipped: Vec<SkippedCandidate> = Vec::new();

    // 1) Matcher excluded 候选 → skipped（带排除原因）。
    for excluded in &match_result.excluded {
        skipped.push(SkippedCandidate {
            channel: excluded.channel,
            ref_id: excluded.ref_id.clone(),
            title: excluded.title.clone(),
            skip_reasons: excluded.exclude_reasons.clone(),
        });
    }

    // 2) 命中候选 → 排序（风险升序，同级渠道分降序）后逐条预算裁剪。
    let mut candidates: Vec<&crate::matcher::MatchHit> = match_result.hits.iter().collect();
    candidates.sort_by(|a, b| {
        let risk_a = risk_rank(infer_risk_class(a.channel, a.severity.as_deref()));
        let risk_b = risk_rank(infer_risk_class(b.channel, b.severity.as_deref()));
        risk_a.cmp(&risk_b).then(
            b.channel_score
                .partial_cmp(&a.channel_score)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });

    let mut remaining = budget.max_requests;
    for (order, hit) in candidates.iter().enumerate() {
        let provider_id = if hit.channel == "rtis-poc" {
            "rtis-poc".to_owned()
        } else {
            "pg-intel".to_owned()
        };
        let provider = format!("{provider_id}@0.1.0");
        let poc_ref = poc_ref_for(hit);
        let risk_class = infer_risk_class(hit.channel, hit.severity.as_deref());
        let request_budget = 10i64.min(remaining);
        if request_budget <= 0 {
            skipped.push(SkippedCandidate {
                channel: hit.channel,
                ref_id: hit.ref_id.clone(),
                title: hit.title.clone(),
                skip_reasons: vec![format!(
                    "budget_exhausted: plan cap {remaining} requests remaining (planning cap {})",
                    budget.max_requests
                )],
            });
            continue;
        }
        let fingerprint = dedup_fingerprint(
            &match_result.target_ref,
            "",
            &poc_ref,
            &provider,
            params_json,
            authctx,
        );
        // 3) 缓存决策：同指纹历史终局项 → cache_hit。
        let cache_decision = match store.find_latest_by_fingerprint(&fingerprint) {
            Ok(Some(prior)) if prior.result_status.is_some() => CacheDecision::CacheHit {
                prior_item_id: prior.item_id.clone(),
                prior_result: prior.result_status.clone().unwrap_or_default(),
            },
            _ => CacheDecision::Fresh,
        };
        // 缓存命中项不占请求预算（不重复执行；引用历史证据）。
        let budget_source = if matches!(cache_decision, CacheDecision::CacheHit { .. }) {
            "cache_hit (no request consumed)".to_owned()
        } else {
            remaining -= request_budget;
            format!(
                "fresh: {request_budget} of plan cap {}",
                budget.max_requests
            )
        };
        planned.push(PlannedItem {
            item: NewItem {
                knowledge_ref: if hit.channel == "pg-intel" {
                    Some(hit.ref_id.clone())
                } else {
                    None
                },
                rtis_poc_id: if hit.channel == "rtis-poc" {
                    hit.ref_id.parse::<i64>().ok()
                } else {
                    None
                },
                provider_id,
                provider_version: Some("0.1.0".to_owned()),
                target_ref: match_result.target_ref.clone(),
                dedup_fingerprint: fingerprint,
                applicability_status: "applicable".to_owned(),
                applicability_reasons: hit.match_reasons.clone(),
                risk_class: risk_class.to_owned(),
                request_budget,
                timeout_budget_s: budget.timeout_s,
                execution_order: order as i64,
                expected_observation: Some(hit.title.clone()),
                // 七维归属缺省 assets（P5-8）：知识命中到维度的映射随画像面
                // 扩展；CLI item-add --dimension 已可显式归属。
                dimension: "assets".to_owned(),
            },
            cache_decision,
            budget_source,
        });
    }

    PlanDraft {
        target_ref: match_result.target_ref.clone(),
        total_request_budget: planned
            .iter()
            .filter(|p| matches!(p.cache_decision, CacheDecision::Fresh))
            .map(|p| p.item.request_budget)
            .sum(),
        budget_cap: budget.max_requests,
        planned,
        skipped,
        budget: budget.to_value(),
    }
}

/// 落库草稿（plan-create + 逐条 item-add；缓存命中项也落库——cache_decision
/// 随 applicability_reasons 记录，供 UI 与 P5-6 调度面读取）。
pub fn persist_draft(
    store: &PlanStore,
    draft: &PlanDraft,
    task_ref: Option<&str>,
    workspace_id: &str,
    title: Option<&str>,
) -> Result<String, PlanStoreError> {
    let plan = store.create_plan(
        task_ref,
        workspace_id,
        title,
        Some(&draft.target_ref),
        Some(&draft.budget),
    )?;
    for planned in &draft.planned {
        let mut item = planned.item.clone();
        // 缓存命中标注进原因面（可解释纪律：UI 显示跳过原因与历史来源）。
        if let CacheDecision::CacheHit {
            prior_item_id,
            prior_result,
        } = &planned.cache_decision
        {
            item.applicability_reasons.push(format!(
                "cache_hit: prior {prior_item_id} result={prior_result}"
            ));
        }
        item.applicability_reasons
            .push(planned.budget_source.clone());
        store.add_item(&plan.plan_id, item)?;
    }
    Ok(plan.plan_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matcher::{ChannelStat, MatchExcluded, MatchHit};
    use database::{migrate, DbPath, Pool, PoolConfig};
    use std::path::PathBuf;
    use std::time::Duration;

    fn store() -> PlanStore {
        let pool = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(5),
        })
        .unwrap();
        migrate(
            &pool,
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations"),
        )
        .unwrap();
        PlanStore::new(pool)
    }

    fn hit(ref_id: &str, score: f64) -> MatchHit {
        MatchHit {
            channel: "rtis-poc",
            ref_id: ref_id.to_owned(),
            title: format!("poc {ref_id}"),
            component: Some("kingdee".to_owned()),
            versions: Some("8.2".to_owned()),
            severity: Some("high".to_owned()),
            verified: true,
            channel_score: score,
            match_reasons: vec!["component match: kingdee".to_owned()],
        }
    }

    fn match_result(hits: Vec<MatchHit>, excluded: Vec<MatchExcluded>) -> MatchResult {
        let hit_count = hits.len();
        let has_applicable = !hits.is_empty();
        MatchResult {
            target_ref: "https://t.example:443".to_owned(),
            components: vec!["kingdee".to_owned()],
            hits,
            excluded,
            channels: vec![ChannelStat {
                channel: "rtis-poc",
                query: "kingdee".to_owned(),
                returned: hit_count,
                latency_ms: 10,
                error: None,
            }],
            has_applicable,
            elapsed_ms: 10,
        }
    }

    fn budget(max: i64) -> PlanBudget {
        PlanBudget {
            max_requests: max,
            timeout_s: 60,
            concurrency: 1,
            max_rounds: 3,
        }
    }

    #[test]
    fn budget_cap_enforced_in_planning_backend() {
        let store = store();
        let result = match_result(vec![hit("1", 3.0), hit("2", 2.0), hit("3", 1.0)], vec![]);
        let draft = draft_from_match(
            &result,
            &budget(15),
            &store,
            |h| format!("rtis-poc:{}", h.ref_id),
            "{}",
            "-",
        );
        // cap 15 / 每项 10 → 两项 fresh + 第三项预算耗尽 skipped（不是静默丢弃）
        let fresh = draft
            .planned
            .iter()
            .filter(|p| matches!(p.cache_decision, CacheDecision::Fresh))
            .count();
        assert_eq!(fresh, 2);
        // 每项 min(10, remaining)：cap 15 → 10 + 5 = 15（Σ ≤ cap，后端封顶生效）
        assert_eq!(draft.total_request_budget, 15);
        assert!(
            draft.total_request_budget <= draft.budget_cap,
            "Σ fresh item budget must respect cap"
        );
        let budget_skips = draft
            .skipped
            .iter()
            .filter(|s| s.skip_reasons[0].starts_with("budget_exhausted"))
            .count();
        assert_eq!(budget_skips, 1);
    }

    #[test]
    fn cache_hit_on_same_fingerprint_and_no_request_consumed() {
        let store = store();
        let result = match_result(vec![hit("900008", 3.0)], vec![]);
        let first = draft_from_match(
            &result,
            &budget(30),
            &store,
            |h| format!("rtis-poc:{}", h.ref_id),
            "{}",
            "-",
        );
        let plan_id = persist_draft(&store, &first, None, "default", Some("first")).unwrap();
        // 给首跑项一个终局五态（模拟已执行）
        let items = store.list_items(&plan_id).unwrap();
        store
            .set_result(
                &items[0].item_id,
                "NOT_REPRODUCED",
                Some("本轮未复现"),
                None,
                None,
                None,
            )
            .unwrap();

        // 同条件重放 → cache_hit（同指纹 + 终局态），不占预算
        let second = draft_from_match(
            &result,
            &budget(30),
            &store,
            |h| format!("rtis-poc:{}", h.ref_id),
            "{}",
            "-",
        );
        assert!(matches!(
            second.planned[0].cache_decision,
            CacheDecision::CacheHit { .. }
        ));
        assert_eq!(
            second.total_request_budget, 0,
            "cache hit must not consume request budget"
        );
        assert!(second.planned[0].budget_source.contains("cache_hit"));
    }

    #[test]
    fn poc_version_change_invalidates_cache() {
        let store = store();
        let result = match_result(vec![hit("900008", 3.0)], vec![]);
        let first = draft_from_match(
            &result,
            &budget(30),
            &store,
            |h| format!("rtis-poc:{}", h.ref_id),
            "{}",
            "-",
        );
        let plan_id = persist_draft(&store, &first, None, "default", None).unwrap();
        let items = store.list_items(&plan_id).unwrap();
        store
            .set_result(
                &items[0].item_id,
                "CONFIRMED",
                Some("差分回显命中"),
                None,
                None,
                None,
            )
            .unwrap();

        // POC 引用版本变化 → 指纹变化 → fresh（缓存正确失效）
        let second = draft_from_match(
            &result,
            &budget(30),
            &store,
            |h| format!("rtis-poc:{}@v2", h.ref_id),
            "{}",
            "-",
        );
        assert!(
            matches!(second.planned[0].cache_decision, CacheDecision::Fresh),
            "poc version change must invalidate cache"
        );
        // 目标画像变化同理（params 段）
        let third = draft_from_match(
            &result,
            &budget(30),
            &store,
            |h| format!("rtis-poc:{}", h.ref_id),
            "{\"deep\":true}",
            "-",
        );
        assert!(matches!(
            third.planned[0].cache_decision,
            CacheDecision::Fresh
        ));
    }

    #[test]
    fn excluded_candidates_become_skipped_with_reasons() {
        let store = store();
        let excluded = MatchExcluded {
            channel: "pg-intel",
            ref_id: "kc-x".to_owned(),
            title: "无锚知识".to_owned(),
            component: None,
            versions: None,
            exclude_reasons: vec!["no component anchor (cannot verify applicability)".to_owned()],
        };
        let result = match_result(vec![], vec![excluded]);
        let draft = draft_from_match(&result, &budget(30), &store, |_| String::new(), "{}", "-");
        assert!(draft.planned.is_empty());
        assert_eq!(draft.skipped.len(), 1);
        assert_eq!(
            draft.skipped[0].skip_reasons[0],
            "no component anchor (cannot verify applicability)"
        );
    }

    #[test]
    fn ordering_low_impact_first_by_score_desc() {
        let store = store();
        let mut a = hit("a", 1.0);
        a.channel_score = 1.0;
        let mut b = hit("b", 2.0);
        b.channel_score = 2.0;
        let result = match_result(vec![a, b], vec![]);
        let draft = draft_from_match(
            &result,
            &budget(30),
            &store,
            |h| format!("rtis-poc:{}", h.ref_id),
            "{}",
            "-",
        );
        // 同风险级：渠道分高者先
        assert!(draft.planned[0].item.dedup_fingerprint != draft.planned[1].item.dedup_fingerprint);
        assert_eq!(draft.planned[0].item.execution_order, 0);
        assert_eq!(draft.planned[1].item.execution_order, 1);
        // 同风险级按渠道分降序：score 2.0 的 b 排首位（expected_observation = 标题）
        assert_eq!(
            draft.planned[0].item.expected_observation.as_deref(),
            Some("poc b")
        );
        assert_eq!(
            draft.planned[1].item.expected_observation.as_deref(),
            Some("poc a")
        );
    }
}
