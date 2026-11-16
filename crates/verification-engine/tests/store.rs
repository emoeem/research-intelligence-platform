//! P5-2 存储集成测试（真实 020 迁移 schema 上的守卫与 CRUD + 契约出站校验）。

use database::{migrate, DbPath, Pool, PoolConfig};
use std::path::PathBuf;
use std::time::Duration;
use verification_engine::contract::{
    validate_event_doc, validate_item_doc, validate_plan_doc, SCHEMA_VERSION,
};
use verification_engine::fingerprint::dedup_fingerprint;
use verification_engine::store::{NewItem, PlanStore, PlanStoreError};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn store() -> PlanStore {
    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("memory pool");
    migrate(&pool, &repo_root().join("migrations")).expect("migrations");
    PlanStore::new(pool)
}

fn sample_item() -> NewItem {
    NewItem {
        knowledge_ref: None,
        rtis_poc_id: Some(900008),
        provider_id: "rtis-poc".to_owned(),
        provider_version: Some("0.1.0".to_owned()),
        target_ref: "https://target.example:443/".to_owned(),
        dedup_fingerprint: dedup_fingerprint(
            "https://target.example:443/",
            "/K3Cloud/",
            "rtis-poc:900008",
            "rtis-poc@0.1.0",
            "{}",
            "-",
        ),
        applicability_status: "applicable".to_owned(),
        applicability_reasons: vec!["component match: Kingdee K3Cloud".to_owned()],
        risk_class: "low_impact".to_owned(),
        request_budget: 10,
        timeout_budget_s: 60,
        execution_order: 0,
        expected_observation: Some("差分回显形态出现".to_owned()),
        dimension: "assets".to_owned(),
    }
}

#[test]
fn plan_lifecycle_and_item_flow_on_real_schema() {
    let store = store();
    let plan = store
        .create_plan(
            Some("rtis-20261102-220853-7827"),
            "default",
            Some("K3Cloud ServiceFacade 探测"),
            Some("https://target.example"),
            Some(&serde_json::json!({
                "max_requests": 30, "timeout_s": 600, "concurrency": 1, "max_rounds": 2
            })),
        )
        .unwrap();
    assert!(plan.plan_id.starts_with("vp-"));
    assert_eq!(plan.status, "draft");
    assert_eq!(plan.schema_version, SCHEMA_VERSION);
    // 出站形状过契约
    validate_plan_doc(&serde_json::to_value(&plan).unwrap()).unwrap();

    let item = store.add_item(&plan.plan_id, sample_item()).unwrap();
    assert!(item.item_id.starts_with("vi-"));
    validate_item_doc(&serde_json::to_value(&item).unwrap()).unwrap();

    // 计划列表摘要：项数与 NOT_TESTED 预置计数
    let rows = store.list_plans(10).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].item_count, 1);

    // 定稿 → ready（终态守卫前面）
    store.set_plan_status(&plan.plan_id, "ready").unwrap();
    let ready = store.get_plan(&plan.plan_id).unwrap().unwrap();
    assert_eq!(ready.status, "ready");

    // 五态写入：带 error_category 禁止 NOT_REPRODUCED（执行失败 ≠ 未复现）
    let fp_item = store
        .set_result(&item.item_id, "INCONCLUSIVE", Some("网络超时，判据未观测"), Some("timeout"), None, None)
        .unwrap();
    assert_eq!(fp_item.result_status.as_deref(), Some("INCONCLUSIVE"));
    assert_eq!(fp_item.error_category.as_deref(), Some("timeout"));
    validate_item_doc(&serde_json::to_value(&fp_item).unwrap()).unwrap();

    // CONFIRMED 必须带依据
    let err = store
        .set_result(&item.item_id, "CONFIRMED", None, None, None, None)
        .unwrap_err();
    assert!(matches!(err, PlanStoreError::GuardRejected(_)));

    let confirmed = store
        .set_result(
            &item.item_id,
            "CONFIRMED",
            Some("差分回显形态出现且负对照未出现"),
            None,
            Some("run-p5-2-test"),
            Some("tr-p5-2-test"),
        )
        .unwrap();
    assert_eq!(confirmed.result_status.as_deref(), Some("CONFIRMED"));
    assert_eq!(confirmed.tool_run_id.as_deref(), Some("run-p5-2-test"));

    // 事件流：plan_created + plan_status + item_planned + result_assigned（词表封闭）
    let events = store.list_events(&plan.plan_id).unwrap();
    let types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert!(types.contains(&"plan_created"));
    assert!(types.contains(&"plan_status"));
    assert!(types.contains(&"item_planned"));
    assert!(types.contains(&"result_assigned"));
    for event in &events {
        validate_event_doc(&serde_json::to_value(event).unwrap()).unwrap();
    }

    // 去重指纹查找：CONFIRMED 项命中
    let latest = store
        .find_latest_by_fingerprint(&sample_item().dedup_fingerprint)
        .unwrap()
        .unwrap();
    assert_eq!(latest.item_id, item.item_id);
}

#[test]
fn guard_rejects_failure_classified_as_not_reproduced() {
    let store = store();
    let plan = store.create_plan(None, "default", None, None, None).unwrap();
    let item = store.add_item(&plan.plan_id, sample_item()).unwrap();
    let err = store
        .set_result(&item.item_id, "NOT_REPRODUCED", Some("没跑出来"), Some("network"), None, None)
        .unwrap_err();
    assert!(matches!(err, PlanStoreError::GuardRejected(_)));
    // 词表外状态拒绝
    let err2 = store
        .set_result(&item.item_id, "PASSED", Some("退出码 0"), None, None, None)
        .unwrap_err();
    assert!(matches!(err2, PlanStoreError::GuardRejected(_)));
    // 工具退出码 0 不等于 CONFIRMED：不带依据的 CONFIRMED 被拒（上面已验），
    // FALSE_POSITIVE 同口径
    let err3 = store
        .set_result(&item.item_id, "FALSE_POSITIVE", Some(" "), None, None, None)
        .unwrap_err();
    assert!(matches!(err3, PlanStoreError::GuardRejected(_)));
}

#[test]
fn excluded_item_requires_reason_and_terminal_plan_frozen() {
    let store = store();
    let plan = store.create_plan(None, "default", None, None, None).unwrap();
    let mut item = sample_item();
    item.applicability_status = "excluded".to_owned();
    item.applicability_reasons = vec![];
    let err = store.add_item(&plan.plan_id, item).unwrap_err();
    assert!(matches!(err, PlanStoreError::GuardRejected(_)));

    // 终态不可迁出
    store.set_plan_status(&plan.plan_id, "cancelled").unwrap();
    let err2 = store.set_plan_status(&plan.plan_id, "ready").unwrap_err();
    assert!(matches!(err2, PlanStoreError::GuardRejected(_)));

    // 指纹形状守卫
    let mut item3 = sample_item();
    item3.dedup_fingerprint = "not-a-fingerprint".to_owned();
    let err3 = store.add_item(&plan.plan_id, item3).unwrap_err();
    assert!(matches!(err3, PlanStoreError::GuardRejected(_)));
}

#[test]
fn bad_ids_rejected_as_not_found() {
    let store = store();
    assert!(matches!(
        store.get_plan("plan-1").unwrap_err(),
        PlanStoreError::NotFound(_)
    ));
    assert!(matches!(
        store.get_item("item-1").unwrap_err(),
        PlanStoreError::NotFound(_)
    ));
}

// ---------------------------------------------------------------------------
// 证据档（RIP3 P5-7，021 research_evidence）：采集/SHA256/重复识别/检索
// ---------------------------------------------------------------------------


fn evidence_fixture() -> serde_json::Value {
    serde_json::json!({
        "provider": "nuclei",
        "status_word": "SUCCEEDED",
        "result": {"matched_count": 0, "nuclei_version": "3.11.1"},
    })
}

#[test]
fn evidence_record_hash_and_retrieval() {
    let store = store();
    let plan = store.create_plan(Some("rtis-p5-7-ev"), "default", Some("证据档"), None, None).unwrap();
    store.set_plan_status(&plan.plan_id, "ready").unwrap();
    let item = store.add_item(&plan.plan_id, sample_item()).unwrap();

    // 采集：SHA256 64 hex + 事件联动 evidence_collected
    let (record, dup) = store
        .record_evidence(
            &item.item_id,
            &plan.plan_id,
            None,
            Some("tr-exec-test-1"),
            "provider_result",
            &evidence_fixture(),
        )
        .unwrap();
    assert!(!dup, "first capture is not a duplicate");
    assert!(record.evidence_id.starts_with("re-"));
    assert_eq!(record.content_hash.len(), 64);
    assert_eq!(record.payload, evidence_fixture());

    // 同语义载荷重复采集 → 跳过落档（dedup=true，返回既有记录）
    let (dup_record, dup2) = store
        .record_evidence(
            &item.item_id,
            &plan.plan_id,
            None,
            Some("tr-exec-test-1"),
            "provider_result",
            &evidence_fixture(),
        )
        .unwrap();
    assert!(dup2, "same payload must dedup");
    assert_eq!(dup_record.evidence_id, record.evidence_id);

    // 检索面：一档在列
    let listed = store.list_evidence(&item.item_id).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].trace_id.as_deref(), Some("tr-exec-test-1"));

    // 事件流留痕：evidence_collected 两条（首采 + dedup 标记）
    let events = store.list_events(&plan.plan_id).unwrap();
    let ev_events: Vec<_> = events.iter().filter(|e| e.event_type == "evidence_collected").collect();
    assert_eq!(ev_events.len(), 2, "capture + dedup marker events");
}

#[test]
fn evidence_type_and_id_shape_guards() {
    let store = store();
    let plan = store.create_plan(Some("rtis-p5-7-ev2"), "default", Some("证据守卫"), None, None).unwrap();
    store.set_plan_status(&plan.plan_id, "ready").unwrap();
    let item = store.add_item(&plan.plan_id, sample_item()).unwrap();

    // evidence_type 词表外拒绝
    let err = store
        .record_evidence(&item.item_id, &plan.plan_id, None, None, "screenshot", &evidence_fixture())
        .unwrap_err();
    assert!(matches!(err, PlanStoreError::GuardRejected(_)));

    // item/plan ID 形状违规拒绝
    let err2 = store
        .record_evidence("vi-1", &plan.plan_id, None, None, "provider_result", &evidence_fixture())
        .unwrap_err();
    assert!(matches!(err2, PlanStoreError::GuardRejected(_)));
    let err3 = store
        .record_evidence(&item.item_id, "plan-1", None, None, "provider_result", &evidence_fixture())
        .unwrap_err();
    assert!(matches!(err3, PlanStoreError::GuardRejected(_)));
}
