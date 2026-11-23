//! Phase 7 PostgreSQL integration: report feedback, evidence provenance,
//! idempotent replay, human-only promotion, and maintenance audit.
use intel_db::maintenance;
use intel_db::report_repo::{self, StoreReportInput};
use intel_db::{open_pool, run_migrations, PgConfig};
use serde_json::json;

fn pool() -> Option<intel_db::PgPool> {
    let dsn = std::env::var("RP_TEST_PG_DSN").ok()?;
    let pool = open_pool(&PgConfig {
        dsn,
        max_connections: 8,
    })
    .ok()?;
    let mut conn = pool.get().ok()?;
    run_migrations(&mut conn).ok()?;
    Some(pool)
}

fn suffix() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

fn fixture(task_id: &str) -> StoreReportInput {
    let mut report_ir: serde_json::Value = serde_json::from_str(include_str!(
        "../../report-engine/tests/fixtures/main-report.json"
    ))
    .unwrap();
    report_ir["report_id"] = json!(format!("rep-p7-{}", suffix()));
    report_ir["task_ref"]["task_id"] = json!(task_id);
    report_ir["findings"].as_array_mut().unwrap().truncate(1);
    report_ir["findings"][0]["finding_id"] = json!(format!("find-p7-{}", suffix()));
    report_ir["findings"][0]["evidence"][0]["evidence_id"] = json!(format!("re-{}", suffix()));
    report_ir["findings"][0]["evidence"][0]["content_hash"] = json!("a".repeat(64));
    StoreReportInput {
        task_id: task_id.to_string(),
        report_ir,
    }
}

#[test]
fn phase7_report_feedback_provenance_and_human_promotion() {
    let Some(pool) = pool() else { return };
    let tag = suffix();
    let task = format!("task-p7-{tag}");
    let report = report_repo::store_report(&pool, &fixture(&task)).unwrap();
    let first =
        maintenance::ingest_report(&pool, &report.report_id, &format!("ws-p7-{tag}")).unwrap();
    assert_eq!(first.findings_seen, 1);
    assert_eq!(first.candidates_created, 1);
    assert_eq!(first.evidence_refs_recorded, 1);

    let replay =
        maintenance::ingest_report(&pool, &report.report_id, &format!("ws-p7-{tag}")).unwrap();
    assert_eq!(
        replay.findings_seen, 0,
        "same report replay must be idempotent"
    );
    assert_eq!(replay.candidates_created, 0);
    assert_eq!(replay.evidence_refs_recorded, 0);

    let mut conn = pool.get().unwrap();
    let row = conn.query_one(
        "SELECT candidate_id,review_status FROM knowledge_feedback_candidates WHERE report_id=$1",
        &[&report.report_id],
    ).unwrap();
    let candidate_id: String = row.get(0);
    assert_eq!(row.get::<_, String>(1), "PENDING_REVIEW");
    let listed = maintenance::list_feedback_candidates(
        &pool,
        &format!("ws-p7-{tag}"),
        Some("PENDING_REVIEW"),
        20,
    )
    .unwrap();
    assert_eq!(listed["count"], 1);
    let detailed = maintenance::get_feedback_candidate(&pool, &candidate_id)
        .unwrap()
        .unwrap();
    assert_eq!(detailed["evidence_provenance"].as_array().unwrap().len(), 1);
    let provenance: i64 = conn
        .query_one(
            "SELECT COUNT(*) FROM knowledge_evidence_provenance WHERE candidate_id=$1",
            &[&candidate_id],
        )
        .unwrap()
        .get(0);
    assert_eq!(provenance, 1);
    drop(conn);

    let card = maintenance::promote_feedback_candidate(
        &pool,
        &candidate_id,
        "reviewer-p7",
        "Evidence reviewed; promote to candidate queue",
    )
    .unwrap();
    let mut conn = pool.get().unwrap();
    let row = conn.query_one(
        "SELECT review_status,source_trust,ai_derived FROM knowledge_cards WHERE knowledge_id=$1",
        &[&card.as_str()],
    ).unwrap();
    assert_eq!(row.get::<_, String>(0), "CANDIDATE");
    assert_eq!(row.get::<_, String>(1), "agent_derived");
    assert!(row.get::<_, bool>(2));
    assert_eq!(
        row.get::<_, String>(0),
        "CANDIDATE",
        "feedback must never auto-approve"
    );
    drop(conn);

    let audit = maintenance::run_maintenance_audit(&pool, &format!("ws-p7-{tag}")).unwrap();
    assert!(!audit.run_id.is_empty());
    let mut conn = pool.get().unwrap();
    let status: String = conn
        .query_one(
            "SELECT status FROM maintenance_runs WHERE run_id=$1",
            &[&audit.run_id],
        )
        .unwrap()
        .get(0);
    assert_eq!(status, "COMPLETED");
    let report = maintenance::load_maintenance_report(&pool, &audit.run_id)
        .unwrap()
        .unwrap();
    assert_eq!(report["status"], "COMPLETED");
    assert!(report["findings"].is_array());
}

#[test]
fn phase7_candidate_rejection_is_explicit_and_terminal() {
    let Some(pool) = pool() else { return };
    let tag = suffix();
    let task = format!("task-p7-reject-{tag}");
    let report = report_repo::store_report(&pool, &fixture(&task)).unwrap();
    maintenance::ingest_report(&pool, &report.report_id, &format!("ws-p7-reject-{tag}")).unwrap();
    let mut conn = pool.get().unwrap();
    let candidate: String = conn
        .query_one(
            "SELECT candidate_id FROM knowledge_feedback_candidates WHERE report_id=$1",
            &[&report.report_id],
        )
        .unwrap()
        .get(0);
    drop(conn);
    maintenance::reject_feedback_candidate(
        &pool,
        &candidate,
        "reviewer-p7",
        "Evidence insufficient",
    )
    .unwrap();
    let promotion = maintenance::promote_feedback_candidate(
        &pool,
        &candidate,
        "reviewer-p7",
        "must not promote",
    );
    assert!(
        promotion.is_err(),
        "rejected candidates must not be promoted"
    );
    let mut conn = pool.get().unwrap();
    let status: String = conn
        .query_one(
            "SELECT review_status FROM knowledge_feedback_candidates WHERE candidate_id=$1",
            &[&candidate],
        )
        .unwrap()
        .get(0);
    assert_eq!(status, "REJECTED");
}

#[test]
fn phase7_migration_v0009_is_idempotent() {
    let Some(pool) = pool() else { return };
    let mut conn = pool.get().unwrap();
    let second = run_migrations(&mut conn).unwrap();
    assert!(
        second.is_empty(),
        "migration replay should be a no-op: {second:?}"
    );
    let tables: i64 = conn.query_one(
        "SELECT COUNT(*) FROM information_schema.tables
         WHERE table_schema='public' AND table_name IN
         ('research_tasks','research_task_reports','findings','finding_observations',
          'knowledge_feedback_candidates','knowledge_evidence_provenance',
          'source_monitor_state','source_monitor_events','maintenance_runs','maintenance_findings')",
        &[],
    ).unwrap().get(0);
    assert_eq!(tables, 10);
    let workspace_column: bool = conn
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM information_schema.columns
             WHERE table_schema='public' AND table_name='research_tasks' AND column_name='workspace_id')",
            &[],
        )
        .unwrap()
        .get(0);
    assert!(
        workspace_column,
        "V0010 must scope research tasks by workspace"
    );
}

#[test]
fn phase7_task_workspace_is_immutable_and_fail_closed() {
    let Some(pool) = pool() else { return };
    let tag = suffix();
    let task = format!("task-p7-workspace-{tag}");
    let first_report = report_repo::store_report(&pool, &fixture(&task)).unwrap();
    maintenance::ingest_report(&pool, &first_report.report_id, &format!("ws-a-{tag}")).unwrap();

    let second_report = report_repo::store_report(&pool, &fixture(&task)).unwrap();
    let cross_workspace =
        maintenance::ingest_report(&pool, &second_report.report_id, &format!("ws-b-{tag}"));
    assert!(
        cross_workspace.is_err(),
        "task IDs must not cross workspace boundaries"
    );

    let mut conn = pool.get().unwrap();
    let (workspace, report_count): (String, i64) = conn
        .query_one(
            "SELECT workspace_id,report_count FROM research_tasks WHERE task_id=$1",
            &[&task],
        )
        .map(|row| (row.get(0), row.get(1)))
        .unwrap();
    assert_eq!(workspace, format!("ws-a-{tag}"));
    assert_eq!(
        report_count, 1,
        "rejected cross-workspace report must not link"
    );
}
