//! P5-2 存储集成测试（真实 020 迁移 schema 上的守卫与 CRUD + 契约出站校验）。

use database::{migrate, DbPath, Pool, PoolConfig};
use std::path::PathBuf;
use std::time::Duration;
use verification_engine::contract::{
    validate_event_doc, validate_item_doc, validate_plan_doc, SCHEMA_VERSION,
};
use verification_engine::fingerprint::dedup_fingerprint;
use verification_engine::store::{NewHypothesis, NewItem, PlanStore, PlanStoreError};

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

/// P5-9b Evidence 检索面扩展（计划域列表 + 单条回读，021 既有表零迁移）。
#[test]
fn evidence_plan_scope_listing_and_detail_get() {
    let store = store();
    let plan = store.create_plan(Some("rtis-p5-9b-ev"), "default", Some("P5-9b 证据检索面"), None, None).unwrap();
    store.set_plan_status(&plan.plan_id, "ready").unwrap();
    let item = store.add_item(&plan.plan_id, sample_item()).unwrap();

    let (record, _) = store
        .record_evidence(
            &item.item_id,
            &plan.plan_id,
            None,
            Some("tr-exec-p59b"),
            "provider_result",
            &evidence_fixture(),
        )
        .unwrap();

    // 计划域列表（Evidence 页列表消费）
    let listed = store.list_evidence_by_plan(&plan.plan_id).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].evidence_id, record.evidence_id);
    assert_eq!(listed[0].tool_run_id, None);
    assert_eq!(listed[0].trace_id.as_deref(), Some("tr-exec-p59b"));

    // 单条回读（Evidence 详情面）
    let got = store
        .get_evidence(&record.evidence_id)
        .unwrap()
        .expect("detail row found");
    assert_eq!(got.payload, evidence_fixture());
    assert_eq!(got.content_hash, record.content_hash);

    // 守卫：域 id 形状违规拒绝 + 未命中 None
    assert!(matches!(
        store.list_evidence_by_plan("vi-wrong-domain"),
        Err(PlanStoreError::GuardRejected(_))
    ));
    assert!(matches!(
        store.get_evidence("vi-not-evidence"),
        Err(PlanStoreError::GuardRejected(_))
    ));
    assert!(store.get_evidence("re-does-not-exist").unwrap().is_none());
}

/// P5-8 假设生命周期（真实 022 research_hypothesis schema）：登记 → 推进 → validated 终局。
#[test]
fn hypothesis_lifecycle_on_real_schema() {
    let store = store();
    let plan = store
        .create_plan(
            Some("rt-p58-e2e"),
            "default",
            Some("P5-8 hypothesis 集成测试场景"),
            Some("127.0.0.1"),
            Some(&serde_json::json!({ "max_requests": 15, "max_rounds": 2 })),
        )
        .unwrap();

    let h = store
        .add_hypothesis(
            &plan.plan_id,
            NewHypothesis {
                dimension: "assets".to_owned(),
                content: "9443 端口服务可能暴露未鉴权管理面".to_owned(),
                source_evidence: "assets 维 item 登记但未判定（unjudged 盲区）".to_owned(),
                association_reason: "盲区直接关联该目标端口".to_owned(),
                preconditions: Some("本地靶场服务可达".to_owned()),
                expected_observation: Some("管理面路径返回 200 或 401 特征".to_owned()),
                negative_control: Some("未开放端口同型探测无匹配".to_owned()),
                suggested_verification: Some("nuclei 指纹模板".to_owned()),
                cost_risk: Some("低于 10 请求 low_impact".to_owned()),
                stop_condition: "连续两轮 NOT_REPRODUCED 或 round 达 max_rounds".to_owned(),
                round_number: 1,
            },
            2,
        )
        .unwrap();
    assert!(h.hypothesis_id.starts_with("hi-"));
    assert_eq!(h.status, "proposed");
    assert_eq!(h.round_number, 1);
    assert_eq!(h.dimension, "assets");
    assert_eq!(h.negative_control.as_deref(), Some("未开放端口同型探测无匹配"));
    assert_eq!(h.schema_version, SCHEMA_VERSION);

    store
        .set_hypothesis_status(&h.hypothesis_id, "in_plan")
        .unwrap();
    store
        .set_hypothesis_status(&h.hypothesis_id, "validated")
        .unwrap();
    let done = store.get_hypothesis(&h.hypothesis_id).unwrap().unwrap();
    assert_eq!(done.status, "validated");
    let rows = store.list_hypotheses(&plan.plan_id).unwrap();
    assert_eq!(rows.len(), 1);
}

/// P5-8 假设守卫负例矩阵：无证据不生成 / 轮次预算 / 维度词表 / validated 终局不迁出。
#[test]
fn hypothesis_guards_evidence_round_and_terminal() {
    let store = store();
    let plan = store
        .create_plan(
            Some("rt-p58-e2e"),
            "default",
            Some("P5-8 hypothesis 守卫负例场景"),
            Some("127.0.0.1"),
            Some(&serde_json::json!({ "max_requests": 15, "max_rounds": 2 })),
        )
        .unwrap();
    let base = || NewHypothesis {
        dimension: "assets".to_owned(),
        content: "假设内容".to_owned(),
        source_evidence: "来源依据".to_owned(),
        association_reason: "关联原因".to_owned(),
        preconditions: None,
        expected_observation: None,
        negative_control: None,
        suggested_verification: None,
        cost_risk: None,
        stop_condition: "停止条件".to_owned(),
        round_number: 1,
    };

    // 必填四项任一为空（trim 后）即拒：无证据不生成
    let mut h1 = base();
    h1.content = "  ".to_owned();
    let mut h2 = base();
    h2.source_evidence = String::new();
    let mut h3 = base();
    h3.association_reason = " ".to_owned();
    let mut h4 = base();
    h4.stop_condition = String::new();
    for h in [h1, h2, h3, h4] {
        assert!(matches!(
            store.add_hypothesis(&plan.plan_id, h, 2).unwrap_err(),
            PlanStoreError::GuardRejected(_)
        ));
    }

    // 轮次预算：round 0 与 round 3（超 max_rounds=2）均拒
    let mut r0 = base();
    r0.round_number = 0;
    assert!(matches!(
        store.add_hypothesis(&plan.plan_id, r0, 2).unwrap_err(),
        PlanStoreError::GuardRejected(_)
    ));
    let mut r3 = base();
    r3.round_number = 3;
    assert!(matches!(
        store.add_hypothesis(&plan.plan_id, r3, 2).unwrap_err(),
        PlanStoreError::GuardRejected(_)
    ));

    // 维度词表外拒绝
    let mut bad_dim = base();
    bad_dim.dimension = "not_a_dimension".to_owned();
    assert!(matches!(
        store.add_hypothesis(&plan.plan_id, bad_dim, 2).unwrap_err(),
        PlanStoreError::GuardRejected(_)
    ));

    // validated 终局不迁出（dropped 同守卫）与未知状态词拒绝
    let ok = store.add_hypothesis(&plan.plan_id, base(), 2).unwrap();
    store
        .set_hypothesis_status(&ok.hypothesis_id, "validated")
        .unwrap();
    assert!(matches!(
        store.set_hypothesis_status(&ok.hypothesis_id, "proposed").unwrap_err(),
        PlanStoreError::GuardRejected(_)
    ));
    assert!(matches!(
        store.set_hypothesis_status(&ok.hypothesis_id, "dropped").unwrap_err(),
        PlanStoreError::GuardRejected(_)
    ));
    assert!(matches!(
        store.set_hypothesis_status(&ok.hypothesis_id, "flying").unwrap_err(),
        PlanStoreError::GuardRejected(_)
    ));
}
