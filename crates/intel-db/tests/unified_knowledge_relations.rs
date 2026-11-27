use domain::kcard::{
    EntitySourceReviewStatus, KnowledgeEntityRefId, KnowledgeEntityType, RelationReviewStatus,
    RelationType, SourceLicenseStatus, SourceTrustClass,
};
use intel_db::knowledge_graph::{self, EntityRefInput, EntitySourceInput, RelationEvidenceRole};
use intel_db::{open_pool, run_migrations, PgConfig};
use refinery::Target;
use std::sync::Mutex;
use std::time::{Duration, UNIX_EPOCH};

static TEST_LOCK: Mutex<()> = Mutex::new(());

mod all_migrations {
    use refinery::embed_migrations;
    embed_migrations!("../../migrations_pg");
}

fn test_pool() -> Option<r2d2::Pool<r2d2_postgres::PostgresConnectionManager<postgres::NoTls>>> {
    let dsn = std::env::var("RP_TEST_PG_DSN").ok()?;
    match open_pool(&PgConfig {
        dsn,
        max_connections: 4,
    }) {
        Ok(pool) => Some(pool),
        Err(_) => {
            eprintln!("SKIP: RP_TEST_PG_DSN is unavailable");
            None
        }
    }
}

fn source(system: &str, id: &str, version: &str, uri: &str, hash: &str) -> EntitySourceInput {
    EntitySourceInput {
        source_system: system.to_string(),
        source_entity_id: id.to_string(),
        source_version: version.to_string(),
        canonical_uri: Some(uri.to_string()),
        source_commit: Some(version.to_string()),
        content_sha256: Some(hash.to_string()),
    }
}

fn input(entity_type: KnowledgeEntityType, key: &str, src: EntitySourceInput) -> EntityRefInput {
    EntityRefInput {
        entity_type,
        canonical_key: key.to_string(),
        source: src,
    }
}

#[test]
fn cross_entity_relation_requires_current_evidence_before_approval() {
    let _guard = TEST_LOCK.lock().unwrap();
    let Some(pool) = test_pool() else { return };
    let mut conn = pool.get().unwrap();
    run_migrations(&mut conn).unwrap();
    drop(conn);

    let suffix = intel_db::new_entity_id("test-");
    let vuln_input = input(
        KnowledgeEntityType::Vulnerability,
        &format!("CVE-2099-{}", suffix),
        source(
            "cisa-kev-test",
            &suffix,
            "kev-commit-a",
            "https://example.invalid/kev",
            &"a".repeat(64),
        ),
    );
    let template_input = input(
        KnowledgeEntityType::DetectionTemplate,
        &format!("template-{suffix}"),
        source(
            "nuclei-test",
            &suffix,
            "templates-commit-a",
            "https://example.invalid/template.yaml",
            &"b".repeat(64),
        ),
    );
    let vuln = knowledge_graph::register_entity_ref(&pool, &vuln_input).unwrap();
    let template = knowledge_graph::register_entity_ref(&pool, &template_input).unwrap();
    let vuln_again = knowledge_graph::register_entity_ref(&pool, &vuln_input).unwrap();
    assert_eq!(vuln.entity_ref_id, vuln_again.entity_ref_id);
    assert_eq!(vuln.source_ref_id, vuln_again.source_ref_id);

    let from = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let to = from + Duration::from_secs(86_400);
    let (relation_id, created) = knowledge_graph::add_entity_relation_with_validity(
        &pool,
        &vuln.entity_ref_id,
        &template.entity_ref_id,
        knowledge_graph::RelationAssertion {
            relation_type: RelationType::Detects,
            evidence: None,
            source: "test-import:1",
            confidence: 0.9,
            valid_from: Some(from),
            valid_to: Some(to),
        },
    )
    .unwrap();
    assert!(created);

    let mut conn = pool.get().unwrap();
    let candidate = conn
        .query_one(
            "SELECT relation_type, review_status, src_id, confidence, valid_from, valid_to
             FROM knowledge_relations WHERE relation_id=$1",
            &[&relation_id.as_str()],
        )
        .unwrap();
    assert_eq!(candidate.get::<_, String>(0), "DETECTS");
    assert_eq!(candidate.get::<_, String>(1), "CANDIDATE");
    assert_eq!(candidate.get::<_, String>(2), vuln.entity_ref_id.as_str());
    assert_eq!(candidate.get::<_, Option<f32>>(3), Some(0.9));
    assert_eq!(
        candidate.get::<_, Option<std::time::SystemTime>>(4),
        Some(from)
    );
    assert_eq!(
        candidate.get::<_, Option<std::time::SystemTime>>(5),
        Some(to)
    );
    drop(conn);

    assert!(knowledge_graph::add_entity_relation(
        &pool,
        &vuln.entity_ref_id,
        &template.entity_ref_id,
        RelationType::Exploits,
        None,
        "test-import:1",
        1.1,
    )
    .is_err());

    let no_evidence = knowledge_graph::decide_relation(
        &pool,
        &relation_id,
        RelationReviewStatus::Approved,
        "approval without evidence must fail",
        "reviewer",
    );
    assert!(no_evidence.is_err());

    let evidence = knowledge_graph::register_entity_ref(
        &pool,
        &input(
            KnowledgeEntityType::Evidence,
            &format!("evidence-{suffix}"),
            source(
                "rip-evidence-test",
                &suffix,
                "evidence-v1",
                "ripartifact://test/evidence",
                &"c".repeat(64),
            ),
        ),
    )
    .unwrap();
    let default_assessment = pool
        .get()
        .unwrap()
        .query_one(
            "SELECT review_status, trust_class FROM knowledge_entity_source_assessments
             WHERE source_ref_id=$1",
            &[&evidence.source_ref_id],
        )
        .unwrap();
    assert_eq!(default_assessment.get::<_, String>(0), "UNREVIEWED");
    assert_eq!(default_assessment.get::<_, String>(1), "UNASSESSED");
    let default_record =
        knowledge_graph::get_entity_source_assessment(&pool, &evidence.source_ref_id).unwrap();
    assert_eq!(
        default_record.review_status,
        EntitySourceReviewStatus::Unreviewed
    );
    assert_eq!(default_record.trust_class, SourceTrustClass::Unassessed);
    assert!(knowledge_graph::link_relation_evidence(
        &pool,
        &relation_id,
        &evidence.entity_ref_id,
        "missing-evidence-source",
        RelationEvidenceRole::Supports,
    )
    .is_err());
    assert!(knowledge_graph::link_relation_evidence(
        &pool,
        &relation_id,
        &evidence.entity_ref_id,
        &evidence.source_ref_id,
        RelationEvidenceRole::Supports,
    )
    .unwrap());
    assert!(!knowledge_graph::link_relation_evidence(
        &pool,
        &relation_id,
        &evidence.entity_ref_id,
        &evidence.source_ref_id,
        RelationEvidenceRole::Supports,
    )
    .unwrap());

    let unassessed_approval = knowledge_graph::review_entity_source(
        &pool,
        &evidence.source_ref_id,
        EntitySourceReviewStatus::Approved,
        &knowledge_graph::EntitySourceAssessmentInput {
            trust_class: SourceTrustClass::Unassessed,
            license_expression: "UNKNOWN".to_string(),
            license_status: SourceLicenseStatus::Unknown,
        },
        "Unassessed sources cannot be approved",
        "reviewer",
    );
    assert!(unassessed_approval.is_err());
    assert!(knowledge_graph::decide_relation(
        &pool,
        &relation_id,
        RelationReviewStatus::Approved,
        "unreviewed Evidence must not approve a relation",
        "reviewer",
    )
    .is_err());
    let assessment = knowledge_graph::EntitySourceAssessmentInput {
        trust_class: SourceTrustClass::Unknown,
        license_expression: "UNKNOWN".to_string(),
        license_status: SourceLicenseStatus::Unknown,
    };
    knowledge_graph::review_entity_source(
        &pool,
        &evidence.source_ref_id,
        EntitySourceReviewStatus::Approved,
        &assessment,
        "Reviewed the evidence source and provenance",
        "reviewer",
    )
    .unwrap();
    let approved_source =
        knowledge_graph::get_entity_source_assessment(&pool, &evidence.source_ref_id).unwrap();
    assert_eq!(
        approved_source.review_status,
        EntitySourceReviewStatus::Approved
    );
    assert_eq!(approved_source.trust_class, SourceTrustClass::Unknown);
    let source_history =
        knowledge_graph::entity_source_review_history(&pool, &evidence.source_ref_id).unwrap();
    assert_eq!(source_history.len(), 1);
    assert_eq!(
        source_history[0].from_status,
        EntitySourceReviewStatus::Unreviewed
    );
    assert_eq!(
        source_history[0].to_status,
        EntitySourceReviewStatus::Approved
    );
    assert_eq!(
        source_history[0].to_license_status,
        SourceLicenseStatus::Unknown
    );
    knowledge_graph::decide_relation(
        &pool,
        &relation_id,
        RelationReviewStatus::Approved,
        "source evidence and snapshot hash reviewed",
        "reviewer",
    )
    .unwrap();
    let reviewed = pool
        .get()
        .unwrap()
        .query_one(
            "SELECT r.review_status, l.from_status, l.to_status, COUNT(e.*)::bigint
             FROM knowledge_relations r
             JOIN knowledge_relation_review_log l ON l.relation_id=r.relation_id
             JOIN knowledge_relation_evidence e ON e.relation_id=r.relation_id
             WHERE r.relation_id=$1
             GROUP BY r.review_status,l.from_status,l.to_status",
            &[&relation_id.as_str()],
        )
        .unwrap();
    assert_eq!(reviewed.get::<_, String>(0), "APPROVED");
    assert_eq!(reviewed.get::<_, String>(1), "CANDIDATE");
    assert_eq!(reviewed.get::<_, String>(2), "APPROVED");
    assert_eq!(reviewed.get::<_, i64>(3), 1);

    knowledge_graph::review_entity_source(
        &pool,
        &evidence.source_ref_id,
        EntitySourceReviewStatus::Rejected,
        &assessment,
        "Source attribution was disproved",
        "reviewer",
    )
    .unwrap();
    let invalidated = pool
        .get()
        .unwrap()
        .query_one(
            "SELECT r.review_status, l.from_status, l.to_status
             FROM knowledge_relations r
             JOIN knowledge_relation_review_log l ON l.relation_id=r.relation_id
             WHERE r.relation_id=$1 AND l.to_status='REJECTED'",
            &[&relation_id.as_str()],
        )
        .unwrap();
    assert_eq!(invalidated.get::<_, String>(0), "REJECTED");
    assert_eq!(invalidated.get::<_, String>(1), "APPROVED");
    assert_eq!(invalidated.get::<_, String>(2), "REJECTED");
    let relation_review_events: i64 = pool
        .get()
        .unwrap()
        .query_one(
            "SELECT COUNT(*) FROM knowledge_relation_review_log WHERE relation_id=$1",
            &[&relation_id.as_str()],
        )
        .unwrap()
        .get(0);
    assert_eq!(relation_review_events, 2);
    let source_review_events: i64 = pool
        .get()
        .unwrap()
        .query_one(
            "SELECT COUNT(*) FROM knowledge_entity_source_review_log
             WHERE source_ref_id=$1",
            &[&evidence.source_ref_id],
        )
        .unwrap()
        .get(0);
    assert_eq!(source_review_events, 2);
    assert_eq!(
        knowledge_graph::get_entity_source_assessment(&pool, &evidence.source_ref_id)
            .unwrap()
            .review_status,
        EntitySourceReviewStatus::Rejected
    );
    assert!(pool
        .get()
        .unwrap()
        .execute(
            "DELETE FROM knowledge_entity_source_review_log WHERE source_ref_id=$1",
            &[&evidence.source_ref_id],
        )
        .is_err());
    assert!(pool
        .get()
        .unwrap()
        .execute(
            "UPDATE knowledge_relation_review_log SET reason='rewritten' WHERE relation_id=$1",
            &[&relation_id.as_str()],
        )
        .is_err());
}

#[test]
fn entity_source_snapshot_is_immutable_and_relation_validity_is_checked() {
    let _guard = TEST_LOCK.lock().unwrap();
    let Some(pool) = test_pool() else { return };
    let mut conn = pool.get().unwrap();
    run_migrations(&mut conn).unwrap();
    drop(conn);

    let suffix = intel_db::new_entity_id("test-");
    let original = input(
        KnowledgeEntityType::TechnicalDocument,
        &format!("doc-{suffix}"),
        source(
            "docs-test",
            &suffix,
            "commit-a",
            "https://example.invalid/doc",
            &"d".repeat(64),
        ),
    );
    knowledge_graph::register_entity_ref(&pool, &original).unwrap();
    let mut changed_snapshot = original.clone();
    changed_snapshot.source.content_sha256 = Some("e".repeat(64));
    assert!(knowledge_graph::register_entity_ref(&pool, &changed_snapshot).is_err());

    let other = knowledge_graph::register_entity_ref(
        &pool,
        &input(
            KnowledgeEntityType::SecurityTool,
            &format!("tool-{suffix}"),
            source(
                "tool-registry-test",
                &suffix,
                "tool-v1",
                "https://example.invalid/tool",
                &"f".repeat(64),
            ),
        ),
    )
    .unwrap();
    let left = knowledge_graph::register_entity_ref(&pool, &original).unwrap();
    let start = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let earlier_end = start - Duration::from_secs(1);
    assert!(knowledge_graph::add_entity_relation_with_validity(
        &pool,
        &left.entity_ref_id,
        &other.entity_ref_id,
        knowledge_graph::RelationAssertion {
            relation_type: RelationType::UsesTool,
            evidence: None,
            source: "test-import:1",
            confidence: 0.7,
            valid_from: Some(start),
            valid_to: Some(earlier_end),
        },
    )
    .is_err());

    let unknown = KnowledgeEntityRefId::try_new("missing-reference").unwrap();
    assert!(knowledge_graph::add_entity_relation(
        &pool,
        &left.entity_ref_id,
        &unknown,
        RelationType::RelatedTo,
        None,
        "test-import:1",
        0.5,
    )
    .is_err());

    let evidence_without_snapshot = knowledge_graph::register_entity_ref(
        &pool,
        &input(
            KnowledgeEntityType::Evidence,
            &format!("evidence-without-snapshot-{suffix}"),
            EntitySourceInput {
                source_system: "manual-test".to_string(),
                source_entity_id: suffix.clone(),
                source_version: "v1".to_string(),
                canonical_uri: None,
                source_commit: None,
                content_sha256: None,
            },
        ),
    )
    .unwrap();
    assert!(knowledge_graph::review_entity_source(
        &pool,
        &evidence_without_snapshot.source_ref_id,
        EntitySourceReviewStatus::Approved,
        &knowledge_graph::EntitySourceAssessmentInput {
            trust_class: SourceTrustClass::Unknown,
            license_expression: "UNKNOWN".to_string(),
            license_status: SourceLicenseStatus::Unknown,
        },
        "Evidence without a snapshot cannot be approved",
        "reviewer",
    )
    .is_err());
}

#[test]
fn migration_backfills_identity_refs_without_rewriting_existing_relations() {
    let _guard = TEST_LOCK.lock().unwrap();
    let Ok(dsn) = std::env::var("RP_TEST_PG_DSN") else {
        return;
    };
    let config: postgres::Config = dsn.parse().unwrap();
    let mut client = config.connect(postgres::NoTls).unwrap();
    let schema = intel_db::new_entity_id("ripkg_m1_");
    client
        .batch_execute(&format!(
            "CREATE SCHEMA {schema}; SET search_path TO {schema}, public"
        ))
        .unwrap();

    all_migrations::migrations::runner()
        .set_target(Target::Version(10))
        .run(&mut client)
        .unwrap();
    client
        .execute(
            "INSERT INTO knowledge_cards (knowledge_id, knowledge_type, title, workspace_id)
             VALUES ('kc-m1-src','vuln_principle','source','m1'),
                    ('kc-m1-dst','detection_method','destination','m1')",
            &[],
        )
        .unwrap();
    client
        .execute(
            "INSERT INTO knowledge_relations (
                 relation_id,src_id,dst_id,relation_type,relation_evidence,
                 relation_source,confidence
             ) VALUES ('krel-m1-preserve','kc-m1-src','kc-m1-dst','DETECTS',
                       '{\"source\":\"fixture\"}'::jsonb,'m1-fixture:1',0.75)",
            &[],
        )
        .unwrap();

    all_migrations::migrations::runner()
        .set_target(Target::Version(11))
        .run(&mut client)
        .unwrap();
    let preserved = client
        .query_one(
            "SELECT relation_id,src_id,dst_id,relation_type,relation_evidence::text,
                    relation_source,confidence,review_status
             FROM knowledge_relations WHERE relation_id='krel-m1-preserve'",
            &[],
        )
        .unwrap();
    assert_eq!(preserved.get::<_, String>(0), "krel-m1-preserve");
    assert_eq!(preserved.get::<_, String>(1), "kc-m1-src");
    assert_eq!(preserved.get::<_, String>(2), "kc-m1-dst");
    assert_eq!(preserved.get::<_, String>(3), "DETECTS");
    assert_eq!(preserved.get::<_, String>(4), "{\"source\": \"fixture\"}");
    assert_eq!(preserved.get::<_, String>(5), "m1-fixture:1");
    assert_eq!(preserved.get::<_, f32>(6), 0.75);
    assert_eq!(preserved.get::<_, String>(7), "CANDIDATE");
    let refs: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM knowledge_entity_refs
             WHERE entity_type='knowledge_card' AND canonical_key IN ('kc-m1-src','kc-m1-dst')",
            &[],
        )
        .unwrap()
        .get(0);
    let sources: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM knowledge_entity_sources
             WHERE source_system='rip.intel-db' AND source_entity_id IN ('kc-m1-src','kc-m1-dst')",
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(refs, 2);
    assert_eq!(sources, 2);
    let review_events: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM knowledge_relation_review_log
             WHERE relation_id='krel-m1-preserve'",
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(review_events, 0);

    all_migrations::migrations::runner()
        .set_target(Target::Version(12))
        .run(&mut client)
        .unwrap();
    let assessment = client
        .query_one(
            "SELECT COUNT(*),
                    COUNT(*) FILTER (WHERE review_status='UNREVIEWED'
                                     AND trust_class='UNASSESSED'),
                    COUNT(*) FILTER (WHERE review_status<>'UNREVIEWED')
             FROM knowledge_entity_source_assessments",
            &[],
        )
        .unwrap();
    assert_eq!(assessment.get::<_, i64>(0), 2);
    assert_eq!(assessment.get::<_, i64>(1), 2);
    assert_eq!(assessment.get::<_, i64>(2), 0);

    client
        .batch_execute(&format!("DROP SCHEMA {schema} CASCADE"))
        .unwrap();
}
