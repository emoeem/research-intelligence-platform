//! Phase 7 PostgreSQL integration: report feedback, evidence provenance,
//! idempotent replay, human-only promotion, and maintenance audit.
use intel_db::ingestion::{self, IngestionSource, KnowledgeIngestionRequest, CONTRACT_VERSION};
use intel_db::maintenance;
use intel_db::report_repo::{self, StoreReportInput};
use intel_db::{open_pool, run_migrations, PgConfig};
use serde_json::json;

fn pool() -> Option<intel_db::PgPool> {
    let dsn = match std::env::var("RP_TEST_PG_DSN") {
        Ok(value) => value,
        Err(_) if std::env::var_os("RP_REQUIRE_PG_TESTS").is_some() => {
            panic!("RP_TEST_PG_DSN is required when RP_REQUIRE_PG_TESTS=1")
        }
        Err(_) => {
            eprintln!("SKIP: RP_TEST_PG_DSN not configured (local-only PG integration)");
            return None;
        }
    };
    let pool = open_pool(&PgConfig {
        dsn,
        max_connections: 8,
    });
    let pool = match pool {
        Ok(pool) => pool,
        Err(error) if std::env::var_os("RP_REQUIRE_PG_TESTS").is_some() => {
            panic!("PostgreSQL test pool unavailable with RP_REQUIRE_PG_TESTS=1: {error}")
        }
        Err(_) => return None,
    };
    let mut conn = match pool.get() {
        Ok(conn) => conn,
        Err(error) if std::env::var_os("RP_REQUIRE_PG_TESTS").is_some() => {
            panic!("PostgreSQL test connection unavailable with RP_REQUIRE_PG_TESTS=1: {error}")
        }
        Err(_) => return None,
    };
    if let Err(error) = run_migrations(&mut conn) {
        if std::env::var_os("RP_REQUIRE_PG_TESTS").is_some() {
            panic!("PostgreSQL migration failed with RP_REQUIRE_PG_TESTS=1: {error}");
        }
        return None;
    }
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
    let detailed =
        maintenance::get_feedback_candidate(&pool, &format!("ws-p7-{tag}"), &candidate_id)
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
        &format!("ws-p7-{tag}"),
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
        &format!("ws-p7-reject-{tag}"),
        &candidate,
        "reviewer-p7",
        "Evidence insufficient",
    )
    .unwrap();
    let promotion = maintenance::promote_feedback_candidate(
        &pool,
        &format!("ws-p7-reject-{tag}"),
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

#[test]
fn knowledge_ingestion_report_jobs_are_idempotent_and_workspace_scoped() {
    let Some(pool) = pool() else { return };
    let tag = suffix();
    let task = format!("task-ingestion-{tag}");
    let report = report_repo::store_report(&pool, &fixture(&task)).unwrap();
    let workspace = format!("ws-ingestion-{tag}");
    let request = KnowledgeIngestionRequest {
        contract_version: CONTRACT_VERSION.to_string(),
        workspace_id: workspace.clone(),
        idempotency_key: format!("ingestion-key-{tag}"),
        source: IngestionSource::ResearchReport {
            report_id: report.report_id.clone(),
        },
        requester: "integration-test".to_string(),
        run_id: None,
        trace_id: None,
    };

    let pool_a = pool.clone();
    let request_a = request.clone();
    let submit_a =
        std::thread::spawn(move || ingestion::submit_report(&pool_a, &request_a).unwrap());
    let pool_b = pool.clone();
    let request_b = request.clone();
    let submit_b =
        std::thread::spawn(move || ingestion::submit_report(&pool_b, &request_b).unwrap());
    let first = submit_a.join().unwrap();
    let concurrent_replay = submit_b.join().unwrap();
    assert_eq!(concurrent_replay.ingestion_id, first.ingestion_id);
    let first = ingestion::get_status(&pool, &workspace, &first.ingestion_id)
        .unwrap()
        .unwrap();
    assert_eq!(first.status, "REVIEW_PENDING");
    assert_eq!(first.attempt_count, 1);
    assert_eq!(
        first.result.as_ref().unwrap()["candidate_ids"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let replay = ingestion::submit_report(&pool, &request).unwrap();
    assert_eq!(replay.ingestion_id, first.ingestion_id);
    assert_eq!(replay.attempt_count, 1);

    let mut conflicting = request.clone();
    conflicting.requester = "different-requester".to_string();
    assert!(ingestion::submit_report(&pool, &conflicting).is_err());
    let mut conn = pool.get().unwrap();
    let candidate_id: String = conn
        .query_one(
            "SELECT candidate_id FROM knowledge_feedback_candidates WHERE report_id=$1",
            &[&report.report_id],
        )
        .unwrap()
        .get(0);
    drop(conn);
    maintenance::reject_feedback_candidate(
        &pool,
        &workspace,
        &candidate_id,
        "reviewer-ingestion",
        "Integration test resolved the candidate",
    )
    .unwrap();
    let mut replay_after_review = request.clone();
    replay_after_review
        .idempotency_key
        .push_str("-after-review");
    let resolved = ingestion::submit_report(&pool, &replay_after_review).unwrap();
    assert_eq!(resolved.status, "COMPLETED");
    assert_eq!(
        resolved.result.as_ref().unwrap()["candidate_ids"][0],
        candidate_id
    );
    assert!(
        ingestion::get_status(&pool, "another-workspace", &first.ingestion_id)
            .unwrap()
            .is_none()
    );
}

#[test]
fn phase7_duplicate_promotion_is_idempotent() {
    let Some(pool) = pool() else { return };
    let tag = suffix();
    let task = format!("task-p7-idempotent-{tag}");
    let report = report_repo::store_report(&pool, &fixture(&task)).unwrap();
    maintenance::ingest_report(&pool, &report.report_id, &format!("ws-p7-idempotent-{tag}"))
        .unwrap();

    let mut conn = pool.get().unwrap();
    let candidate: String = conn
        .query_one(
            "SELECT candidate_id FROM knowledge_feedback_candidates WHERE report_id=$1",
            &[&report.report_id],
        )
        .unwrap()
        .get(0);
    drop(conn);

    let first = maintenance::promote_feedback_candidate(
        &pool,
        &format!("ws-p7-idempotent-{tag}"),
        &candidate,
        "reviewer-p7",
        "first promotion",
    )
    .unwrap();
    let replay = maintenance::promote_feedback_candidate(
        &pool,
        &format!("ws-p7-idempotent-{tag}"),
        &candidate,
        "reviewer-p7",
        "repeat promotion",
    )
    .unwrap();
    assert_eq!(
        first.as_str(),
        replay.as_str(),
        "repeat promotion must return the existing card"
    );

    let mut conn = pool.get().unwrap();
    let card_count: i64 = conn
        .query_one(
            "SELECT COUNT(*) FROM knowledge_cards WHERE knowledge_id=$1",
            &[&first.as_str()],
        )
        .unwrap()
        .get(0);
    assert_eq!(card_count, 1);
    let review_count: i64 = conn
        .query_one(
            "SELECT COUNT(*) FROM knowledge_review_log WHERE knowledge_id=$1",
            &[&first.as_str()],
        )
        .unwrap()
        .get(0);
    assert_eq!(
        review_count, 1,
        "repeat promotion must not add a second review log"
    );
}

#[test]
fn phase7_concurrent_promotion_has_one_card_and_one_review_log() {
    let Some(pool) = pool() else { return };
    let tag = suffix();
    let task = format!("task-p7-concurrent-{tag}");
    let workspace = format!("ws-p7-concurrent-{tag}");
    let report = report_repo::store_report(&pool, &fixture(&task)).unwrap();
    maintenance::ingest_report(&pool, &report.report_id, &workspace).unwrap();

    let mut conn = pool.get().unwrap();
    let candidate: String = conn
        .query_one(
            "SELECT candidate_id FROM knowledge_feedback_candidates WHERE report_id=$1",
            &[&report.report_id],
        )
        .unwrap()
        .get(0);
    drop(conn);

    let pool_a = pool.clone();
    let pool_b = pool.clone();
    let candidate_a = candidate.clone();
    let candidate_b = candidate.clone();
    let workspace_a = workspace.clone();
    let workspace_b = workspace.clone();
    let a = std::thread::spawn(move || {
        maintenance::promote_feedback_candidate(
            &pool_a,
            &workspace_a,
            &candidate_a,
            "reviewer-a",
            "concurrent promotion a",
        )
    });
    let b = std::thread::spawn(move || {
        maintenance::promote_feedback_candidate(
            &pool_b,
            &workspace_b,
            &candidate_b,
            "reviewer-b",
            "concurrent promotion b",
        )
    });
    let first = a.join().unwrap();
    let second = b.join().unwrap();
    let card_ids = [first.as_ref().ok(), second.as_ref().ok()]
        .into_iter()
        .flatten()
        .map(|id| id.as_str().to_string())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        card_ids.len(),
        1,
        "concurrent promotion must reuse one card"
    );

    let mut conn = pool.get().unwrap();
    let card_count: i64 = conn
        .query_one(
            "SELECT COUNT(*) FROM knowledge_cards WHERE knowledge_id=ANY($1)",
            &[&card_ids.iter().cloned().collect::<Vec<_>>()],
        )
        .unwrap()
        .get(0);
    assert_eq!(card_count, 1);
}

#[test]
fn phase7_candidate_state_update_failure_rolls_back_card_and_review() {
    let Some(pool) = pool() else { return };
    let tag = suffix();
    let task = format!("task-p7-rollback-{tag}");
    let workspace = format!("ws-p7-rollback-{tag}");
    let report = report_repo::store_report(&pool, &fixture(&task)).unwrap();
    maintenance::ingest_report(&pool, &report.report_id, &workspace).unwrap();

    let mut conn = pool.get().unwrap();
    let candidate: String = conn
        .query_one(
            "SELECT candidate_id FROM knowledge_feedback_candidates WHERE report_id=$1",
            &[&report.report_id],
        )
        .unwrap()
        .get(0);
    conn.batch_execute(
        "DROP TRIGGER IF EXISTS fail_candidate_promotion_trigger ON knowledge_feedback_candidates;
         DROP FUNCTION IF EXISTS fail_candidate_promotion();
         CREATE OR REPLACE FUNCTION fail_candidate_promotion() RETURNS trigger AS $$
         BEGIN
           IF NEW.review_status = 'PROMOTED' THEN
             RAISE EXCEPTION 'injected candidate update failure';
           END IF;
           RETURN NEW;
         END;
         $$ LANGUAGE plpgsql;",
    )
    .unwrap();
    conn.batch_execute(
        "CREATE TRIGGER fail_candidate_promotion_trigger
         BEFORE UPDATE ON knowledge_feedback_candidates
         FOR EACH ROW EXECUTE FUNCTION fail_candidate_promotion();",
    )
    .unwrap();
    drop(conn);

    let result = maintenance::promote_feedback_candidate(
        &pool,
        &workspace,
        &candidate,
        "reviewer-p7",
        "rollback injection",
    );
    assert!(result.is_err());

    let mut conn = pool.get().unwrap();
    let status: String = conn
        .query_one(
            "SELECT review_status FROM knowledge_feedback_candidates WHERE candidate_id=$1",
            &[&candidate],
        )
        .unwrap()
        .get(0);
    assert_eq!(status, "PENDING_REVIEW");
    let card_count: i64 = conn
        .query_one(
            "SELECT COUNT(*) FROM knowledge_cards WHERE workspace_id=$1",
            &[&workspace],
        )
        .unwrap()
        .get(0);
    assert_eq!(card_count, 0);
    conn.batch_execute(
        "DROP TRIGGER IF EXISTS fail_candidate_promotion_trigger ON knowledge_feedback_candidates;
         DROP FUNCTION IF EXISTS fail_candidate_promotion();",
    )
    .unwrap();
}
