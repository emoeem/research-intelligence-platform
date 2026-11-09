//! P4-1 纯函数单测：est_tokens 估算器基线与预算策略七分项（PHASE4-DESIGN
//! §6 P4-1 判据「estimator 有单测基线」；设计稿 §5.1/§5.2）。
//!
//! 数值基线：total=8000 的默认分配逐项等于设计稿 §9.1 budget_report.allocated
//! 示例（l0=400 / l1=608 / l2_knowledge=1900 / l2_tools=760 / l3_reserved=2052
//! / memory=1140 / reserve_output=1140）。

use context_broker::{
    borrow_allowed, count_chars, est_tokens, BudgetComponent, BudgetPolicy, CharCounts,
};

// ---------------------------------------------------------------------------
// estimator 基线（设计稿 §5.2 公式）
// ---------------------------------------------------------------------------

#[test]
fn estimator_ascii_is_about_one_token_per_four_chars() {
    assert_eq!(est_tokens(""), 0, "empty text costs nothing");
    assert_eq!(est_tokens("abcd"), 1, "4 ascii = 1 token");
    assert_eq!(est_tokens("abc"), 1, "3 ascii rounds up to 1 token");
    assert_eq!(est_tokens("abcdefgh"), 2, "8 ascii = 2 tokens");
}

#[test]
fn estimator_cjk_is_about_one_token_per_char() {
    assert_eq!(est_tokens("中文字"), 3, "3 cjk chars = 3 tokens");
    assert_eq!(est_tokens("工具目录"), 4, "4 cjk chars = 4 tokens");
}

#[test]
fn estimator_mixed_and_other_classes() {
    // 4 ascii / 4 + 2 cjk = 1.0 + 2.0 = 3.0。
    assert_eq!(est_tokens("abcd中文"), 3, "4 ascii + 2 cjk");
    // other 类（é）按 2 字符 1 token：1 字符向上取整为 1。
    assert_eq!(est_tokens("é"), 1);
    assert_eq!(est_tokens("éé"), 1, "2 other chars = 1 token");
    // 混合三类：ascii 1 / 4 + cjk 1 + other 1 / 2 = 0.25 + 1 + 0.5 = 1.75 -> 2。
    assert_eq!(est_tokens("a中\u{e9}"), 2);
}

#[test]
fn char_counts_breakdown_matches_formula() {
    let counts: CharCounts = count_chars("a中\u{e9}");
    assert_eq!(
        counts,
        CharCounts {
            ascii: 1,
            cjk: 1,
            other: 1
        }
    );
    assert_eq!(counts.total(), 3);
    // est_tokens 与 counts 公式一致（不重复扫文本的口径由单一实现保证）。
    let manual =
        (counts.ascii as f64 / 4.0 + counts.cjk as f64 + counts.other as f64 / 2.0).ceil() as u64;
    assert_eq!(est_tokens("a中\u{e9}"), manual);
}

// ---------------------------------------------------------------------------
// 预算策略：默认分配基线（设计稿 §9.1 示例数值）
// ---------------------------------------------------------------------------

#[test]
fn default_policy_at_8000_matches_design_section_9_1_example() {
    let policy = BudgetPolicy::default_for(8000).expect("default policy must validate");
    let alloc = policy.allocate().expect("allocation must succeed");
    assert_eq!(alloc.total_tokens, 8000);
    assert_eq!(alloc.l0_rules, 400, "min(2000, 5% of 8000)");
    assert_eq!(alloc.l1_catalog, 608, "8% of remainder 7600");
    assert_eq!(alloc.l2_knowledge, 1900, "25% of remainder");
    assert_eq!(alloc.l2_tools, 760, "10% of remainder");
    assert_eq!(alloc.l3_reserved, 2052, "27% of remainder");
    assert_eq!(alloc.memory_history, 1140, "15% of remainder");
    assert_eq!(alloc.reserve_output, 1140, "remainder leftover = 15%");
    let sum: u64 = BudgetComponent::ALL.iter().map(|c| alloc.get(*c)).sum();
    assert_eq!(sum, 8000, "seven components must sum to total");
    assert_eq!(policy.policy_version(), "policy-1");
}

#[test]
fn l0_cap_binds_for_large_budgets() {
    let policy = BudgetPolicy::default_for(100_000).expect("default policy must validate");
    let alloc = policy.allocate().expect("allocation must succeed");
    assert_eq!(alloc.l0_rules, 2000, "cap 2000 < 5% of 100000 = 5000");
    let sum: u64 = BudgetComponent::ALL.iter().map(|c| alloc.get(*c)).sum();
    assert_eq!(sum, 100_000, "seven components must sum to total");
}

#[test]
fn allocation_is_exact_even_for_tiny_budgets() {
    let policy = BudgetPolicy::default_for(40).expect("default policy must validate");
    let alloc = policy.allocate().expect("allocation must succeed");
    assert_eq!(alloc.l0_rules, 2, "floor(40 * 0.05) = 2");
    let sum: u64 = BudgetComponent::ALL.iter().map(|c| alloc.get(*c)).sum();
    assert_eq!(sum, 40, "floor rounding is absorbed by reserve_output");
    assert!(
        alloc.reserve_output as f64 >= 0.10 * 38.0 - f64::EPSILON,
        "reserve hard floor 10% of remainder 38"
    );
}

// ---------------------------------------------------------------------------
// 预算策略：JSON 加载与校验
// ---------------------------------------------------------------------------

fn design_default_json() -> String {
    [
        "{",
        "  \"policy_version\": \"policy-1\",",
        "  \"total_tokens\": 8000,",
        "  \"l0_rules_cap_tokens\": 2000,",
        "  \"l0_rules_share\": 0.05,",
        "  \"reserve_output_floor_share\": 0.10,",
        "  \"shares\": {",
        "    \"l1_catalog\": 0.08, \"l2_knowledge\": 0.25, \"l2_tools\": 0.10,",
        "    \"l3_reserved\": 0.27, \"memory_history\": 0.15, \"reserve_output\": 0.15",
        "  }",
        "}",
    ]
    .join("\n")
}

#[test]
fn from_json_parses_design_default_document() {
    let policy =
        BudgetPolicy::from_json(&design_default_json()).expect("valid policy doc must parse");
    let alloc = policy.allocate().expect("allocation must succeed");
    assert_eq!(
        alloc,
        BudgetPolicy::default_for(8000).unwrap().allocate().unwrap()
    );
    assert_eq!(policy.total_tokens(), 8000);
}

#[test]
fn from_json_rejects_malformed_documents() {
    // 非 JSON。
    let err = BudgetPolicy::from_json("not json").unwrap_err();
    assert!(err.to_string().contains("not valid JSON"), "display: {err}");
    // 顶层非对象。
    assert!(BudgetPolicy::from_json("[1,2]").is_err());
    // 缺 shares。
    let missing_shares = r#"{ "policy_version": "p", "total_tokens": 100 }"#;
    let err = BudgetPolicy::from_json(missing_shares).unwrap_err();
    assert!(err.to_string().contains("\"shares\""), "display: {err}");
    // 缺 shares 内一个键。
    let partial =
        r#"{ "policy_version": "p", "total_tokens": 100, "shares": { "l1_catalog": 0.08 } }"#;
    assert!(BudgetPolicy::from_json(partial).is_err());
    // 缺 total_tokens。
    let no_total = r#"{ "policy_version": "p", "shares": { "l1_catalog": 0.08, "l2_knowledge": 0.25, "l2_tools": 0.10, "l3_reserved": 0.27, "memory_history": 0.15, "reserve_output": 0.15 } }"#;
    assert!(BudgetPolicy::from_json(no_total).is_err());
    // 份额和不为 1。
    let bad_sum = r#"{ "policy_version": "p", "total_tokens": 100, "shares": { "l1_catalog": 0.5, "l2_knowledge": 0.25, "l2_tools": 0.10, "l3_reserved": 0.27, "memory_history": 0.15, "reserve_output": 0.15 } }"#;
    let err = BudgetPolicy::from_json(bad_sum).unwrap_err();
    assert!(
        err.to_string().contains("must sum to 1.0"),
        "display: {err}"
    );
}

#[test]
fn new_rejects_out_of_range_parameters() {
    let shares = context_broker::policy::ComponentShares::default_shares();
    // 零总量。
    assert!(BudgetPolicy::new("p".into(), 0, 2000, 0.05, shares, 0.10).is_err());
    // 零 l0 封顶。
    assert!(BudgetPolicy::new("p".into(), 8000, 0, 0.05, shares, 0.10).is_err());
    // l0 份额越界。
    assert!(BudgetPolicy::new("p".into(), 8000, 2000, 0.0, shares, 0.10).is_err());
    assert!(BudgetPolicy::new("p".into(), 8000, 2000, 1.5, shares, 0.10).is_err());
    // 空版本号。
    assert!(BudgetPolicy::new("  ".into(), 8000, 2000, 0.05, shares, 0.10).is_err());
    // 硬下限高于 reserve 份额。
    assert!(BudgetPolicy::new("p".into(), 8000, 2000, 0.05, shares, 0.20).is_err());
}

#[test]
fn reserve_hard_floor_is_enforced_at_allocation_time() {
    // 畸形份额：其余五项吃掉余量，reserve 只剩零头（构造校验放过，分配拒绝）。
    let squeezed = context_broker::policy::ComponentShares {
        l1_catalog: 0.10,
        l2_knowledge: 0.10,
        l2_tools: 0.10,
        l3_reserved: 0.10,
        memory_history: 0.60,
        reserve_output: 0.0,
    };
    // floor 0.0 与 share 0.0 一致，构造通过；分配结果 reserve = 0 >= floor 0，也通过。
    let policy = BudgetPolicy::new("p".into(), 1000, 2000, 0.05, squeezed, 0.0)
        .expect("floor 0.0 equals share 0.0");
    let alloc = policy
        .allocate()
        .expect("reserve floor 0 allows zero reserve");
    assert_eq!(alloc.reserve_output, 0);
    assert_eq!(alloc.l0_rules, 50, "floor(1000 * 0.05)");
    assert_eq!(alloc.memory_history, 570, "60% of remainder 950");
    let sum: u64 = BudgetComponent::ALL.iter().map(|c| alloc.get(*c)).sum();
    assert_eq!(sum, 1000, "seven components must sum to total");
}

// ---------------------------------------------------------------------------
// 借用规则（设计稿 §5.1：l3 可借给 l2，reserve_output 不可触碰）
// ---------------------------------------------------------------------------

#[test]
fn borrow_rules_allow_only_l3_to_l2() {
    assert!(borrow_allowed(
        BudgetComponent::L3Reserved,
        BudgetComponent::L2Knowledge
    ));
    assert!(borrow_allowed(
        BudgetComponent::L3Reserved,
        BudgetComponent::L2Tools
    ));
    // 反向不允许。
    assert!(!borrow_allowed(
        BudgetComponent::L2Knowledge,
        BudgetComponent::L3Reserved
    ));
    assert!(!borrow_allowed(
        BudgetComponent::L2Tools,
        BudgetComponent::L3Reserved
    ));
    // reserve_output 双向禁触。
    assert!(!borrow_allowed(
        BudgetComponent::L3Reserved,
        BudgetComponent::ReserveOutput
    ));
    assert!(!borrow_allowed(
        BudgetComponent::ReserveOutput,
        BudgetComponent::L2Knowledge
    ));
    // L0/L1 永不驱逐也不外借（设计稿 §5.3 第 1、5 条）。
    assert!(!borrow_allowed(
        BudgetComponent::L0Rules,
        BudgetComponent::L2Knowledge
    ));
    assert!(!borrow_allowed(
        BudgetComponent::L1Catalog,
        BudgetComponent::L2Tools
    ));
    // 自借无意义。
    assert!(!borrow_allowed(
        BudgetComponent::L3Reserved,
        BudgetComponent::L3Reserved
    ));
}

#[test]
fn budget_component_names_match_design_section_5_1_rows() {
    assert_eq!(BudgetComponent::L0Rules.as_str(), "l0_rules");
    assert_eq!(BudgetComponent::L1Catalog.as_str(), "l1_catalog");
    assert_eq!(BudgetComponent::L2Knowledge.as_str(), "l2_knowledge");
    assert_eq!(BudgetComponent::L2Tools.as_str(), "l2_tools");
    assert_eq!(BudgetComponent::L3Reserved.as_str(), "l3_reserved");
    assert_eq!(BudgetComponent::MemoryHistory.as_str(), "memory_history");
    assert_eq!(BudgetComponent::ReserveOutput.as_str(), "reserve_output");
}
