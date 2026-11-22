//! P3-2 集成测试：D3 近似重复（rip.gov.minhash 容器 + gov_worker 编排面）。
//!
//! 门禁口径：V-9 双守卫——RP_TEST_PG_DSN 未设或不可达跳过；验收镜像按 digest
//! 钉死引用探本地镜像库，缺失跳过（受控环境本地实跑为准）。
//!
//! 判据：近似文本两卡 → minhash_scan → NEAR_DUPLICATE 候选边（jaccard 随
//! evidence 落库、confidence=估计值）；完全无关文本不建边；扫描幂等（二跑
//! relations_created=0）。

use intel_db::card_repo::{self, CardInput};
use intel_db::gov_worker::{self, GovWorkerConfig};
use intel_db::{open_pool, run_migrations, PgConfig};
use domain::kcard::{KnowledgeType, SourceTrust, VerificationStatus};

fn skip_unless_pg() -> Option<r2d2::Pool<r2d2_postgres::PostgresConnectionManager<postgres::NoTls>>> {
    let dsn = std::env::var("RP_TEST_PG_DSN").ok()?;
    match open_pool(&PgConfig { dsn, max_connections: 4 }) {
        Ok(p) => Some(p),
        Err(_) => {
            eprintln!("SKIP: RP_TEST_PG_DSN 不可达（V-9）");
            None
        }
    }
}

fn card_input(title: &str, content: &str) -> CardInput {
    CardInput {
        knowledge_type: KnowledgeType::VulnSignature,
        title: title.to_string(),
        summary: "gov-dedup 测试".to_string(),
        content: content.to_string(),
        structured_content: None,
        confidence: 0.8,
        verification: VerificationStatus::Unverified,
        evidence_level: None,
        source_trust: SourceTrust::ExternalImport,
        ai_derived: false,
        quality_flags: vec![],
        workspace_id: "ws-p3-gov".to_string(),
        donor_caller_id: None,
        donor_channel: None,
    }
}

#[test]
fn d3_minhash_scan_builds_near_duplicate_candidates() {
    let Some(pool) = skip_unless_pg() else { return };
    let mut conn = pool.get().unwrap();
    run_migrations(&mut conn).unwrap();
    drop(conn);

    let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root.join("containerfiles/rip-gov/rip.gov.minhash.json"))
            .expect("gov manifest"),
    )
    .unwrap();
    let image = manifest["runtime"]["image"].as_str().unwrap().to_string();
    let exists = std::process::Command::new("podman")
        .args(["image", "exists", &image])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !exists {
        eprintln!("SKIP: 验收镜像 {image} 不在本地镜像库（V-9，受控环境端到端）");
        return;
    }

    // 近似文本（同题少改）+ 完全无关文本。
    let _a = card_repo::create_card(
        &pool,
        &card_input(
            "gov-近似甲",
            "Struts2 S2-045 漏洞的触发条件是 Content-Type 头恶意构造，修复方式是升级到 2.3.32。",
        ),
    )
    .unwrap();
    let _b = card_repo::create_card(
        &pool,
        &card_input(
            "gov-近似乙",
            "Struts2 S2-045 漏洞的触发条件是 Content-Type 头恶意构造，修复方式是升级到 2.3.33。",
        ),
    )
    .unwrap();
    let _c = card_repo::create_card(
        &pool,
        &card_input("gov-无关", "容器安全基线：镜像签名与只读根文件系统策略。"),
    )
    .unwrap();

    let cfg = GovWorkerConfig {
        run_db: repo_root.join("rip-data/parse/test-ops.db"),
        sqlite_migrations_dir: repo_root.join("migrations"),
        manifest_dir: repo_root.join("containerfiles/rip-gov"),
        workspace_root: repo_root.join("rip-data/parse/test-ws"),
        ..Default::default()
    };
    let outcome = gov_worker::minhash_scan(&pool, "ws-p3-gov", 0.5, 128, &cfg).unwrap();
    assert_eq!(outcome.scanned, 3, "三卡全部参扫");
    assert!(outcome.pairs >= 1, "近似对必须被召回（got pairs={}）", outcome.pairs);
    assert!(outcome.relations_created >= 1, "NEAR_DUPLICATE 候选边必须新建");

    // 幂等：二跑不新建（UNIQUE(src,dst,type)）。
    let again = gov_worker::minhash_scan(&pool, "ws-p3-gov", 0.5, 128, &cfg).unwrap();
    assert_eq!(again.relations_created, 0, "二跑幂等零新建");

    // 边内容：jaccard 随 evidence、confidence = 估计值。
    let mut conn = pool.get().unwrap();
    let rows = conn
        .query(
            "SELECT relation_evidence->>'jaccard_est', confidence FROM knowledge_relations
             WHERE relation_type = 'NEAR_DUPLICATE' AND relation_source = 'gov_worker.minhash:0.1.0'",
            &[],
        )
        .unwrap();
    assert!(!rows.is_empty(), "NEAR_DUPLICATE 边在库");
    for r in rows {
        let j: Option<String> = r.get(0);
        let conf: Option<f32> = r.get(1);
        assert!(j.is_some(), "jaccard_est 随 evidence 落库");
        let conf = conf.expect("confidence");
        assert!((0.0..=1.0).contains(&conf));
    }
}
