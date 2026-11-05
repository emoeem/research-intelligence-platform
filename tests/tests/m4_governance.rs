//! M4：治理集成测试（真实 001 迁移 schema 上的状态机/守卫 + 真实隔离清单入队）。

use database::{DbPath, Pool, PoolConfig};
use domain::{GovernanceStatus, LifecycleStatus};
use quality_engine::{proposals_from_quarantine_dir, Governor, ResourceUpdate, ReviewStore};
use rp_eri_parity::{fresh_tmp_dir, migrations_dir};
use std::time::Duration;

/// 真实迁移 schema 上：Governor 状态机与 VERIFIED 守卫的行为。
#[test]
fn governor_on_real_schema() {
    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    database::migrate(&pool, &migrations_dir()).unwrap();
    // 造一行全生命周期数据：DISCOVERED 起步
    pool.with_conn(|c| {
        c.execute(
            "INSERT INTO resource (resource_id, canonical_url, resource_type, name,
                lifecycle, governance, source_system, created_at, updated_at)
             VALUES ('gh:test/gov-e2e', 'https://github.com/test/gov-e2e', 'github_repo',
                'Gov E2E', 'DISCOVERED', 'ACTIVE', 'm4-test', 't', 't')",
            [],
        )
    })
    .unwrap();
    let gov = Governor::new(pool.clone());

    // 全生命周期链推进（真实 schema CHECK 允许的同一套状态值）
    for step in [
        LifecycleStatus::Fetched,
        LifecycleStatus::Parsed,
        LifecycleStatus::Candidate,
        LifecycleStatus::Verified,
        LifecycleStatus::Indexed,
    ] {
        gov.apply_lifecycle("gh:test/gov-e2e", step).unwrap();
    }
    assert_eq!(
        gov.current_lifecycle("gh:test/gov-e2e").unwrap(),
        LifecycleStatus::Indexed
    );

    // VERIFIED 守卫虽已推进过 VERIFIED，但守卫只对"当前为 VERIFIED"的行生效；
    // 用 CANDIDATE 行验证守卫触发（001 schema 真表 + 真守卫逻辑）
    pool.with_conn(|c| {
        c.execute(
            "INSERT INTO resource (resource_id, canonical_url, resource_type, name,
                lifecycle, governance, source_system, created_at, updated_at)
             VALUES ('gh:test/gov-verified', 'https://github.com/test/gov-verified',
                'github_repo', 'Gov Verified', 'VERIFIED', 'ACTIVE', 'm4-test', 't', 't')",
            [],
        )
    })
    .unwrap();
    let err = gov
        .update_resource(
            "gh:test/gov-verified",
            &ResourceUpdate {
                name: Some("AI overwrite".to_string()),
                quality_final_score: Some(99.0),
                ..ResourceUpdate::default()
            },
        )
        .unwrap_err();
    assert!(
        matches!(err, quality_engine::GovernanceError::NeedReview { .. }),
        "VERIFIED row must reject AI updates: {err:?}"
    );
    // 内容未被覆盖
    let name: String = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT name FROM resource WHERE resource_id='gh:test/gov-verified'",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(name, "Gov Verified");

    // 治理轴正交：隔离 INDEXED 行合法，隔离后 STALE 循环照常
    gov.apply_governance("gh:test/gov-e2e", GovernanceStatus::Quarantined)
        .unwrap();
    gov.apply_lifecycle("gh:test/gov-e2e", LifecycleStatus::Stale)
        .unwrap();
}

/// 真实隔离清单（migrations/quarantine/）-> 治理库 review 表全链路：
/// propose 幂等入队 + decide 写决议 + 待决清单收敛。
#[test]
fn review_queue_on_real_quarantine_dir() {
    let dir = migrations_dir().join("quarantine");
    if !dir.is_dir() {
        eprintln!(
            "SKIP review_queue_on_real_quarantine_dir: {} missing",
            dir.display()
        );
        return;
    }
    let tmp = fresh_tmp_dir("m4-governance");
    let pool = Pool::open(PoolConfig::writable_file(
        tmp.join("platform-governance.db"),
    ))
    .unwrap();
    let store = ReviewStore::open(pool).unwrap();

    let (proposals, files) = proposals_from_quarantine_dir(&dir).unwrap();
    assert_eq!(proposals.len(), 70, "68 d5 pairs + 2 assets isolation");
    assert_eq!(files.len(), 2);

    let report = store.propose_all(&proposals).unwrap();
    assert_eq!(report.inserted, 70);
    assert_eq!(store.pending().unwrap().len(), 70);

    // 幂等重放（同 (source_system, source_pk) 跳过）
    let report2 = store.propose_all(&proposals).unwrap();
    assert_eq!(report2.inserted, 0);
    assert_eq!(report2.skipped, 70);

    // 决议两条真实候选（记录式决议，不自动改资源实体，M1 口径）
    let first = &proposals[0];
    let row = store
        .decide(
            &first.review_id(),
            "KEEP_BOTH",
            "m4-integration",
            Some(0.9),
            Some("复核独立"),
        )
        .unwrap();
    assert_eq!(row.decision, "KEEP_BOTH");
    assert!(row.reviewed_at.is_some());
    let second = &proposals[1];
    store
        .decide(
            &second.review_id(),
            "NEEDS_MORE_EVIDENCE",
            "m4-integration",
            None,
            None,
        )
        .unwrap();

    assert_eq!(store.pending().unwrap().len(), 68);
    let counts = store.counts().unwrap();
    let map: std::collections::HashMap<String, i64> = counts.into_iter().collect();
    assert_eq!(map.get("KEEP_BOTH"), Some(&1));
    assert_eq!(map.get("NEEDS_MORE_EVIDENCE"), Some(&1));
    assert_eq!(map.get("PENDING"), Some(&68));
}
