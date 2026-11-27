use domain::kcard::{KnowledgeEntityType, RelationReviewStatus};
use intel_db::knowledge_graph::{
    self, EntityRefInput, EntitySourceInput, SkillToolMappingCandidateInput,
};
use intel_db::{open_pool, run_migrations, PgConfig};
use std::sync::Mutex;

static TEST_LOCK: Mutex<()> = Mutex::new(());

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

fn input(
    entity_type: KnowledgeEntityType,
    key: &str,
    source_system: &str,
    source_entity_id: &str,
    version: &str,
    uri: &str,
    hash: &str,
) -> EntityRefInput {
    EntityRefInput {
        entity_type,
        canonical_key: key.to_string(),
        source: EntitySourceInput {
            source_system: source_system.to_string(),
            source_entity_id: source_entity_id.to_string(),
            source_version: version.to_string(),
            canonical_uri: Some(uri.to_string()),
            source_commit: None,
            content_sha256: Some(hash.to_string()),
        },
    }
}

#[test]
fn skill_identity_can_link_to_tool_without_auto_approval() {
    let _guard = TEST_LOCK.lock().unwrap();
    let Some(pool) = test_pool() else { return };
    let mut conn = pool.get().unwrap();
    run_migrations(&mut conn).unwrap();
    // A second migration pass must be a no-op (including V0013).
    run_migrations(&mut conn).unwrap();
    drop(conn);

    let suffix = intel_db::new_entity_id("skill-map-test-");
    let skill = input(
        KnowledgeEntityType::Skill,
        &format!("skill:active-scan:{suffix}"),
        "rip.skill-registry",
        &format!("active-scan:{suffix}"),
        "0.2.0",
        &format!("ripskill://active-scan/{suffix}"),
        &"a".repeat(64),
    );
    let tool = input(
        KnowledgeEntityType::SecurityTool,
        &format!("tool:nmap:{suffix}"),
        "rip.platform.tool-registry",
        &format!("nmap:{suffix}"),
        "registry-v1",
        &format!("riptool://nmap/{suffix}"),
        &"b".repeat(64),
    );
    let evidence = input(
        KnowledgeEntityType::Evidence,
        &format!("skill-body:active-scan:{suffix}"),
        "rip.skill-body",
        &format!("active-scan:{suffix}"),
        "0.2.0",
        &format!("ripskill-body://active-scan/{suffix}"),
        &"c".repeat(64),
    );
    let result = knowledge_graph::add_skill_tool_mapping_candidate(
        &pool,
        &SkillToolMappingCandidateInput {
            skill,
            tool,
            evidence,
            evidence_metadata: serde_json::json!({
                "candidate_rule": "exact tool token in skill body",
                "rule_version": "0.1.0",
                "heuristic_score_is_probability": false
            }),
            relation_source: "skill-map-candidate:0.1.0".to_string(),
            confidence: 0.5,
        },
    )
    .unwrap();
    assert!(result.relation_created);
    assert!(result.evidence_link_created);
    let relation_id = result.relation_id;
    let evidence_source_ref_id = result.evidence.source_ref_id;

    let row = pool.get().unwrap().query_one(
        "SELECT r.review_status, src.entity_type, dst.entity_type,
                (SELECT COUNT(*) FROM knowledge_relation_evidence e WHERE e.relation_id=r.relation_id)
         FROM knowledge_relations r
         JOIN knowledge_entity_refs src ON src.entity_ref_id=r.src_id
         JOIN knowledge_entity_refs dst ON dst.entity_ref_id=r.dst_id
         WHERE r.relation_id=$1",
        &[&relation_id.as_str()],
    ).unwrap();
    assert_eq!(row.get::<_, String>(0), "CANDIDATE");
    assert_eq!(row.get::<_, String>(1), "skill");
    assert_eq!(row.get::<_, String>(2), "security_tool");
    assert_eq!(row.get::<_, i64>(3), 1);

    // Even an evidence-linked mapping cannot be promoted until its source has
    // an explicit trust/license review. This test deliberately leaves it pending.
    assert!(knowledge_graph::decide_relation(
        &pool,
        &relation_id,
        RelationReviewStatus::Approved,
        "must reject unreviewed skill source",
        "test-reviewer",
    )
    .is_err());

    let mut conn = pool.get().unwrap();
    let skill_assessment = conn
        .query_one(
            "SELECT review_status, trust_class, license_status
         FROM knowledge_entity_source_assessments WHERE source_ref_id=$1",
            &[&evidence_source_ref_id],
        )
        .unwrap();
    assert_eq!(skill_assessment.get::<_, String>(0), "UNREVIEWED");
    assert_eq!(skill_assessment.get::<_, String>(1), "UNASSESSED");
    assert_eq!(skill_assessment.get::<_, String>(2), "UNKNOWN");

    // The atomic API is idempotent: re-registering the same candidate must not
    // duplicate the relation or Evidence edge.
    let again = knowledge_graph::add_skill_tool_mapping_candidate(
        &pool,
        &SkillToolMappingCandidateInput {
            skill: input(KnowledgeEntityType::Skill, &format!("skill:active-scan:{suffix}"), "rip.skill-registry", &format!("active-scan:{suffix}"), "0.2.0", &format!("ripskill://active-scan/{suffix}"), &"a".repeat(64)),
            tool: input(KnowledgeEntityType::SecurityTool, &format!("tool:nmap:{suffix}"), "rip.platform.tool-registry", &format!("nmap:{suffix}"), "registry-v1", &format!("riptool://nmap/{suffix}"), &"b".repeat(64)),
            evidence: input(KnowledgeEntityType::Evidence, &format!("skill-body:active-scan:{suffix}"), "rip.skill-body", &format!("active-scan:{suffix}"), "0.2.0", &format!("ripskill-body://active-scan/{suffix}"), &"c".repeat(64)),
            evidence_metadata: serde_json::json!({"candidate_rule":"exact tool token in skill body"}),
            relation_source: "skill-map-candidate:0.1.0".to_string(),
            confidence: 0.5,
        },
    ).unwrap();
    assert!(!again.relation_created);
    assert!(!again.evidence_link_created);

    knowledge_graph::decide_relation(
        &pool,
        &relation_id,
        RelationReviewStatus::Rejected,
        "fixture closes candidate before replay guard check",
        "test-reviewer",
    )
    .unwrap();
    let rejected_replay = knowledge_graph::add_skill_tool_mapping_candidate(
        &pool,
        &SkillToolMappingCandidateInput {
            skill: input(
                KnowledgeEntityType::Skill,
                &format!("skill:active-scan:{suffix}"),
                "rip.skill-registry",
                &format!("active-scan:{suffix}"),
                "0.2.0",
                &format!("ripskill://active-scan/{suffix}"),
                &"a".repeat(64),
            ),
            tool: input(
                KnowledgeEntityType::SecurityTool,
                &format!("tool:nmap:{suffix}"),
                "rip.platform.tool-registry",
                &format!("nmap:{suffix}"),
                "registry-v1",
                &format!("riptool://nmap/{suffix}"),
                &"b".repeat(64),
            ),
            evidence: input(
                KnowledgeEntityType::Evidence,
                &format!("skill-body:active-scan:{suffix}"),
                "rip.skill-body",
                &format!("active-scan:{suffix}"),
                "0.2.0",
                &format!("ripskill-body://active-scan/{suffix}"),
                &"c".repeat(64),
            ),
            evidence_metadata: serde_json::json!({"candidate_rule":"exact tool token in skill body"}),
            relation_source: "skill-map-candidate:0.1.0".to_string(),
            confidence: 0.5,
        },
    );
    assert!(rejected_replay
        .err()
        .unwrap()
        .to_string()
        .contains("non-candidate relation"));
}

#[test]
fn skill_can_reference_vulnerability_with_typed_relation_and_evidence() {
    let _guard = TEST_LOCK.lock().unwrap();
    let Some(pool) = test_pool() else { return };
    let mut conn = pool.get().unwrap();
    run_migrations(&mut conn).unwrap();
    drop(conn);

    let suffix = intel_db::new_entity_id("skill-vuln-test-");
    let generic = intel_db::knowledge_graph::SkillEntityReferenceCandidateInput {
        skill: input(
            KnowledgeEntityType::Skill,
            &format!("skill:framework-log4j2:{suffix}"),
            "rip.skill-registry",
            &format!("framework-log4j2:{suffix}"),
            "0.2.0",
            &format!("ripskill://framework-log4j2/{suffix}"),
            &"d".repeat(64),
        ),
        target: input(
            KnowledgeEntityType::Vulnerability,
            &format!("CVE-2021-44228:{suffix}"),
            "knowledge.cisa-kev",
            &format!("CVE-2021-44228:{suffix}"),
            "0.1.2@fixture",
            &format!("cve://CVE-2021-44228/{suffix}"),
            &"e".repeat(64),
        ),
        evidence: input(
            KnowledgeEntityType::Evidence,
            &format!("skill-body:framework-log4j2:{suffix}"),
            "rip.skill-body",
            &format!("framework-log4j2:{suffix}"),
            "0.2.0",
            &format!("ripskill-body://framework-log4j2/{suffix}"),
            &"f".repeat(64),
        ),
        relation_type: domain::kcard::RelationType::References,
        evidence_metadata: serde_json::json!({"cve":"CVE-2021-44228","line":42,"exact_identifier_match":true}),
        relation_source: "skill-reference-candidate:test".to_string(),
        confidence: 0.5,
    };
    let result =
        intel_db::knowledge_graph::add_skill_entity_reference_candidate(&pool, &generic).unwrap();
    assert!(result.relation_created);
    assert!(result.evidence_link_created);
    let row = pool.get().unwrap().query_one(
        "SELECT r.relation_type,r.review_status,src.entity_type,dst.entity_type,
                (SELECT COUNT(*) FROM knowledge_relation_evidence e WHERE e.relation_id=r.relation_id)
         FROM knowledge_relations r
         JOIN knowledge_entity_refs src ON src.entity_ref_id=r.src_id
         JOIN knowledge_entity_refs dst ON dst.entity_ref_id=r.dst_id
         WHERE r.relation_id=$1",
        &[&result.relation_id.as_str()],
    ).unwrap();
    assert_eq!(row.get::<_, String>(0), "REFERENCES");
    assert_eq!(row.get::<_, String>(1), "CANDIDATE");
    assert_eq!(row.get::<_, String>(2), "skill");
    assert_eq!(row.get::<_, String>(3), "vulnerability");
    assert_eq!(row.get::<_, i64>(4), 1);

    let invalid_pair = intel_db::knowledge_graph::SkillEntityReferenceCandidateInput {
        relation_type: domain::kcard::RelationType::UsesTool,
        ..generic
    };
    assert!(
        intel_db::knowledge_graph::add_skill_entity_reference_candidate(&pool, &invalid_pair)
            .is_err()
    );
}
