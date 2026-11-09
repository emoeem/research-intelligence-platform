//! P4-3 纯函数单测（M4.2 判据）：意图分类、槽位检测、五权重打分、一票
//! 否决与 matched_signals；意图审计事件带 correlation_id（audit-engine
//! emit 旁路）。

use context_broker::{
    classify, emit_intent_event, score_candidate, vetoes, Candidate, CandidateMetrics,
    ClassifyResult, ContentRef, ContextItemKind, IntentConfig, IntentStatus, ScoreWeights,
    ToolFamily, AMBIGUOUS_CONFIDENCE_THRESHOLD, INTENT_ACTION,
};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root must exist")
}

fn load_config() -> IntentConfig {
    IntentConfig::from_dir(&repo_root().join("rulepacks/rip-rules")).expect("intent config loads")
}

// ---------------------------------------------------------------------------
// 配置加载（版本化映射表）
// ---------------------------------------------------------------------------

#[test]
fn config_carries_thirteen_plan_families_and_version() {
    let config = load_config();
    assert_eq!(config.config_id, "intent-families");
    assert_eq!(config.config_version, "1.0.0");
    assert_eq!(config.families.len(), 13, "计划 §4.2 恰十三族");
    for rule in &config.families {
        ToolFamily::parse(&rule.id)
            .unwrap_or_else(|e| panic!("family {} must parse: {e}", rule.id));
        assert!(!rule.triggers.is_empty(), "{} carries triggers", rule.id);
    }
    assert!(
        config.knowledge_intents.len() >= 4,
        "设计稿 §4.1 四类知识意图"
    );
    assert!(config.task_verbs.len() >= 8, "任务动词表非空");
}

#[test]
fn config_rejects_unknown_family_id_and_empty_triggers() {
    let bad_family = "schema_version = \"1\"\nconfig_id = \"x\"\nconfig_version = \"1\"\n[[family]]\nid = \"Fake / Family\"\ntriggers = [\"t\"]\n[task_verbs]\nverbs = [\"v\"]";
    let err = IntentConfig::from_toml(bad_family).unwrap_err();
    assert!(
        err.to_string().contains("not a plan"),
        "unknown family id must be rejected: {err}"
    );
    let empty_triggers = "config_id = \"x\"\nconfig_version = \"1\"\n[[family]]\nid = \"Knowledge\"\ntriggers = []\n[task_verbs]\nverbs = [\"v\"]";
    assert!(IntentConfig::from_toml(empty_triggers).is_err());
}

// ---------------------------------------------------------------------------
// 分类：归一化 / 族 / 动词 / 槽位 / 低置信回退
// ---------------------------------------------------------------------------

#[test]
fn classify_kingdee_http_task_matches_web_family_with_ok_status() {
    let config = load_config();
    let result = classify("kingdee HTTP 检查", &config);
    assert_eq!(
        result.status,
        IntentStatus::Ok,
        "0.4 family + 0.2 verb >= 0.5"
    );
    assert_eq!(result.query.families, vec![ToolFamily::WebHttp]);
    assert!(result.query.task_verbs.contains(&"检查".to_string()));
    assert!(
        result
            .matched_signals
            .iter()
            .any(|s| s == "family:Web / HTTP:http"),
        "signals: {:?}",
        result.matched_signals
    );
    assert!(result.matched_signals.iter().any(|s| s == "verb:检查"));
    assert!(result.alternatives.is_empty(), "ok 状态无 alternatives");
}

#[test]
fn classify_domain_scan_task_ranks_network_and_detects_domain_slot() {
    let config = load_config();
    let result = classify("扫描 example.com 的端口", &config);
    assert_eq!(result.status, IntentStatus::Ok);
    assert_eq!(result.query.families.first(), Some(&ToolFamily::Network));
    assert!(
        result
            .query
            .target_slots
            .iter()
            .any(|s| format!("{s:?}").contains("Domain")),
        "slots: {:?}",
        result.query.target_slots
    );
    assert!(
        result
            .matched_signals
            .iter()
            .any(|s| s.contains("slot:domain:example.com")),
        "signals: {:?}",
        result.matched_signals
    );
}

#[test]
fn classify_low_confidence_marks_ambiguous_with_alternatives() {
    let config = load_config();
    // 仅单次族命中（0.4 < 0.5）：ambiguous 且附族候选。
    let result = classify("android", &config);
    assert_eq!(result.status, IntentStatus::Ambiguous);
    assert!(result.query.confidence < AMBIGUOUS_CONFIDENCE_THRESHOLD);
    assert!(
        result.alternatives.contains(&ToolFamily::MobileApk),
        "top3 候选族随 ambiguous 附带: {:?}",
        result.alternatives
    );
    // 全无命中：ambiguous 且 alternatives 为空（无可推荐）。
    let none = classify("随便看看", &config);
    assert_eq!(none.status, IntentStatus::Ambiguous);
    assert!(none.query.families.is_empty());
    assert!(none.alternatives.is_empty());
}

#[test]
fn detect_slots_covers_six_kinds_with_evidence() {
    let hex64 = "a".repeat(64);
    let text = format!(
        "访问 http://example.com/x 与 192.168.1.1 和 gh:a/b 样本 {hex64} 报告 report.pdf 应用 app.apk"
    );
    let slots = context_broker::detect_slots(&text.to_lowercase());
    let slots_of = |name: &str| slots.iter().any(|s| format!("{:?}", s.slot).contains(name));
    assert!(slots_of("Url"), "slots: {slots:?}");
    assert!(slots_of("Domain"), "slots: {slots:?}");
    assert!(slots_of("Ip"), "slots: {slots:?}");
    assert!(slots_of("CodeRepo"), "slots: {slots:?}");
    assert!(slots_of("Binary"), "slots: {slots:?}");
    assert!(slots_of("Document"), "slots: {slots:?}");
    assert!(slots_of("Apk"), "slots: {slots:?}");
    assert!(
        slots
            .iter()
            .any(|s| s.slot == context_broker::TargetSlot::Domain && s.evidence == "example.com"),
        "domain evidence: {slots:?}"
    );
}

// ---------------------------------------------------------------------------
// 打分：五权重 / 一票否决 / matched_signals
// ---------------------------------------------------------------------------

fn kingdee_candidate() -> Candidate {
    Candidate {
        item_id: "kb:900008".to_string(),
        kind: ContextItemKind::Knowledge,
        layer: context_broker::ContextLayer::L2,
        title: "K3Cloud ServiceFacade 错误码判读".to_string(),
        summary: "kingdee k3cloud 差分脚本，kingdee 判定方法".to_string(),
        family: Some("knowledge".to_string()),
        governance: None,
        quality_grade: None,
        reference: ContentRef {
            source: "rtis".to_string(),
            ref_id: "kb:900008".to_string(),
            ref_version: None,
            content_hash: None,
            uri: Some("rtis://poc/900008".to_string()),
        },
        est_tokens: 0,
    }
}

#[test]
fn default_weights_sum_to_one_and_match_design_constants() {
    let w = ScoreWeights::default();
    assert_eq!(w.family_match, 0.40);
    assert_eq!(w.quality, 0.25);
    assert_eq!(w.health, 0.20);
    assert_eq!(w.recency, 0.10);
    assert_eq!(w.past_success, 0.05);
    assert!(
        (w.family_match + w.quality + w.health + w.recency + w.past_success - 1.0).abs() < 1e-9
    );
}

#[test]
fn score_combines_five_components_and_kw_signals() {
    let config = load_config();
    let intent: ClassifyResult = classify("kingdee http 检索", &config);
    let candidate = kingdee_candidate();
    let metrics = CandidateMetrics {
        quality_grade: Some("A".to_string()),
        health: Some(1.0),
        recency: Some(0.5),
        past_success: None,
        governance: None,
    };
    let scored = score_candidate(&candidate, &intent, &metrics, &ScoreWeights::default());
    assert_eq!(scored.veto_reason, None);
    // w1*1.0 + w2*1.0 + w3*1.0 + w4*0.5 + w5*0.0 = 0.40+0.25+0.20+0.05 = 0.90。
    assert!(
        (scored.score - 0.90).abs() < 1e-9,
        "score: {}",
        scored.score
    );
    assert_eq!(
        scored.components.family_match, 1.0,
        "family knowledge ∈ intent"
    );
    assert_eq!(scored.components.past_success, 0.0, "Phase 4 恒 0 只留通路");
    assert!(
        scored.matched_signals.iter().any(|s| s == "kw:kingdee"),
        "词元级命中信号: {:?}",
        scored.matched_signals
    );
    assert!(
        scored
            .matched_signals
            .iter()
            .any(|s| s == "family-match:Knowledge"),
        "signals: {:?}",
        scored.matched_signals
    );
}

#[test]
fn governance_veto_rejects_three_states_and_cannot_be_outscored() {
    for gov in ["QUARANTINED", "REJECTED", "ARCHIVED"] {
        assert!(vetoes(Some(gov)).is_some(), "{gov} must be vetoed");
    }
    assert!(vetoes(Some("ACTIVE")).is_none());
    assert!(
        vetoes(None).is_none(),
        "未知治理值不做否决（源面可见性已过滤）"
    );

    // 否决不可被分数翻盘：满分候选带 QUARANTINED 仍给出 veto_reason。
    let config = load_config();
    let intent = classify("kingdee http 检索", &config);
    let mut candidate = kingdee_candidate();
    candidate.quality_grade = Some("A".to_string());
    let metrics = CandidateMetrics {
        quality_grade: Some("A".to_string()),
        health: Some(1.0),
        recency: Some(1.0),
        past_success: Some(1.0),
        governance: Some("QUARANTINED".to_string()),
    };
    let scored = score_candidate(&candidate, &intent, &metrics, &ScoreWeights::default());
    assert!(
        scored
            .veto_reason
            .as_deref()
            .unwrap_or("")
            .contains("QUARANTINED"),
        "高分候选仍被一票否决: {:?}",
        scored.veto_reason
    );
}

// ---------------------------------------------------------------------------
// 审计：意图事件带 correlation_id（emit_best_effort 旁路）
// ---------------------------------------------------------------------------

#[test]
fn intent_event_carries_correlation_id_via_audit_engine() {
    use audit_engine::AuditEngine;
    use database::{DbPath, Pool, PoolConfig};
    use std::time::Duration;

    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    database::migrate(&pool, &repo_root().join("migrations")).unwrap();
    let engine = AuditEngine::new(pool.clone());

    let config = load_config();
    let result = classify("kingdee HTTP 检索", &config);
    let emitted = emit_intent_event(&engine, &result, "corr-p43-1", "test-agent");
    assert!(emitted, "合法事件应落库");

    let (count, action, corr): (i64, String, String) = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT count(*), action, correlation_id FROM audit_event WHERE correlation_id = 'corr-p43-1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
        })
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(action, INTENT_ACTION);
    assert_eq!(corr, "corr-p43-1");
    let detail: String = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT detail FROM audit_event WHERE correlation_id = 'corr-p43-1'",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert!(detail.contains("Web / HTTP"), "detail: {detail}");
    assert!(detail.contains("confidence"), "detail: {detail}");
}

#[test]
fn intent_event_best_effort_never_fails_caller() {
    use audit_engine::AuditEngine;
    use database::{DbPath, Pool, PoolConfig};
    use std::time::Duration;

    // 无 audit_event 表的裸内存库：emit 失败只缓冲，调用方拿到 false 不炸。
    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    let engine = AuditEngine::new(pool);
    let config = load_config();
    let result = classify("kingdee HTTP 检索", &config);
    let emitted = emit_intent_event(&engine, &result, "corr-p43-2", "test-agent");
    assert!(!emitted, "写失败旁路返回 false，不影响装配");
}
