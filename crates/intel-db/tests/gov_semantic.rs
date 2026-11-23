//! P3-3 集成测试：D4 语义候选（rip.embed.text 容器 + V0005 向量面 +
//! semantic_scan）。
//!
//! 门禁口径：V-9 双守卫（PG DSN + 验收镜像 digest 钉死引用在本地镜像库）。
//!
//! 判据：同义改写两卡（词法面抓不到，bge 语义面召回）+ 无关卡 → embed-cards
//! 幂等补向量（512 维）→ semantic-scan 阈值上建 NEAR_DUPLICATE 候选边
//! （relation_source = gov_worker.semantic:0.1.0，evidence rule = D4 semantic
//! cosine）；**禁自动合并**（候选边非卡片合并）；二跑幂等。

use domain::kcard::{KnowledgeType, SourceTrust, VerificationStatus};
use intel_db::card_repo::{self, CardInput};
use intel_db::embed_worker::{self, EmbedWorkerConfig};
use intel_db::{open_pool, run_migrations, PgConfig};

fn skip_unless_pg() -> Option<r2d2::Pool<r2d2_postgres::PostgresConnectionManager<postgres::NoTls>>>
{
    let dsn = std::env::var("RP_TEST_PG_DSN").ok()?;
    match open_pool(&PgConfig {
        dsn,
        max_connections: 4,
    }) {
        Ok(p) => Some(p),
        Err(_) => {
            eprintln!("SKIP: RP_TEST_PG_DSN 不可达（V-9）");
            None
        }
    }
}

fn card_input(title: &str, content: &str) -> CardInput {
    CardInput {
        knowledge_type: KnowledgeType::VulnPrinciple,
        title: title.to_string(),
        summary: "semantic 测试".to_string(),
        content: content.to_string(),
        structured_content: None,
        confidence: 0.8,
        verification: VerificationStatus::Unverified,
        evidence_level: None,
        source_trust: SourceTrust::ExternalImport,
        ai_derived: false,
        quality_flags: vec![],
        workspace_id: "ws-p3-sem".to_string(),
        donor_caller_id: None,
        donor_channel: None,
    }
}

#[test]
fn d4_semantic_candidates_from_paraphrase_pair() {
    let Some(pool) = skip_unless_pg() else { return };
    let mut conn = pool.get().unwrap();
    run_migrations(&mut conn).unwrap();
    drop(conn);

    let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root.join("containerfiles/rip-embed/rip.embed.text.json"))
            .expect("embed manifest"),
    )
    .unwrap();
    let image = manifest["runtime"]["image"].as_str().unwrap().to_string();
    let exists = std::process::Command::new("podman")
        .args(["image", "exists", &image])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !exists {
        eprintln!("SKIP: 验收镜像 {image} 不在本地镜像库（V-9，受控环境端到端；模型为 gitignore 运行时数据，镜像须受控环境构建）");
        return;
    }

    // 同义改写对（词面差异大，bge 语义近）+ 无关卡。
    let _a = card_repo::create_card(
        &pool,
        &card_input(
            "sem-改写甲",
            "反序列化漏洞的根本成因是对不可信数据未经类型校验就直接还原对象，正确做法是白名单校验输入类型。",
        ),
    )
    .unwrap();
    let _b = card_repo::create_card(
        &pool,
        &card_input(
            "sem-改写乙",
            "对象反序列化风险的来源在于把不受信任的输入直接重建为程序对象，应当先做类型白名单校验再还原。",
        ),
    )
    .unwrap();
    let _c = card_repo::create_card(
        &pool,
        &card_input("sem-无关", "端口扫描器的 TCP SYN 探测原理与速率限制策略。"),
    )
    .unwrap();

    let cfg = EmbedWorkerConfig {
        run_db: repo_root.join("rip-data/parse/test-ops.db"),
        sqlite_migrations_dir: repo_root.join("migrations"),
        manifest_dir: repo_root.join("containerfiles/rip-embed"),
        workspace_root: repo_root.join("rip-data/parse/test-ws"),
        ..Default::default()
    };

    // 首跑：三卡补向量。
    let first = embed_worker::embed_cards(&pool, "ws-p3-sem", &cfg).unwrap();
    assert_eq!(first.embedded, 3, "三卡首次嵌入");
    // 幂等：二跑 embedded=0（已有向量跳过）。
    let second = embed_worker::embed_cards(&pool, "ws-p3-sem", &cfg).unwrap();
    assert_eq!(second.embedded, 0, "二跑零新建");
    assert!(second.skipped_existing >= 3, "已有向量如实计跳过");

    // 语义扫描（阈值 0.80：bge-small 短文本同义改写对实测 0.83 区间）。
    let outcome = embed_worker::semantic_scan(&pool, "ws-p3-sem", 0.80, 10, &cfg).unwrap();
    assert_eq!(outcome.searched, 3, "三向量参扫");
    assert!(outcome.relations_created >= 1, "同义改写对必须建 D4 候选边");
    // 幂等：二跑零新建。
    let again = embed_worker::semantic_scan(&pool, "ws-p3-sem", 0.80, 10, &cfg).unwrap();
    assert_eq!(again.relations_created, 0, "二跑幂等零新建");

    // 边内容：source/rule 均为 D4 语义面；卡片本体未被合并（仍两行）。
    let mut conn = pool.get().unwrap();
    let rows = conn
        .query(
            "SELECT relation_source, relation_evidence->>'rule', confidence FROM knowledge_relations
             WHERE relation_type = 'NEAR_DUPLICATE'
               AND src_id IN (SELECT knowledge_id FROM knowledge_cards WHERE workspace_id = 'ws-p3-sem')",
            &[],
        )
        .unwrap();
    assert!(!rows.is_empty(), "NEAR_DUPLICATE 边在库");
    let mut has_semantic = false;
    for r in rows {
        let source: String = r.get(0);
        if source == "gov_worker.semantic:0.1.0" {
            has_semantic = true;
            let rule: Option<String> = r.get(1);
            assert_eq!(rule.as_deref(), Some("D4 semantic cosine"));
        }
    }
    assert!(has_semantic, "D4 语义面边必须在列");
    let cards = conn
        .query_one(
            "SELECT COUNT(*) FROM knowledge_cards WHERE workspace_id = 'ws-p3-sem'",
            &[],
        )
        .unwrap()
        .get::<_, i64>(0);
    assert_eq!(cards, 3, "候选边不合并卡片本体（禁自动合并，R2 5.2）");
}
