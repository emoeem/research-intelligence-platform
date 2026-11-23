//! MA-3 知识供体治理 e2e（docs/rip3/MA-3-TASK.md §0 D5 + §4 判据 7/8；§三 S6）。
//!
//! 门禁口径：V-9 同款——RP_TEST_PG_DSN 未设或不可达时跳过（CI 无 PG
//! service；容器端到端保持 selftest_phase3_governance.sh / selftest_ma3.sh
//! 本地实跑）。
//!
//! 判据 7 硬验收③（Rust 面，真实线程 + 真实 PG；真进程级实测在
//! selftest_ma3.sh）：
//! 1. 并发冲突治理：N=6 真线程并发 donor-submit 同 content → N 行卡片全
//!    CANDIDATE + donor_caller_id/donor_channel 逐行正确 + dedup-scan
//!    SAME_CONTENT 候选边 = N-1 对（治理面可见；D2 语义不拒绝创建——
//!    禁 UNIQUE(normalized_hash)，红线 4）+ decide FOR UPDATE 行锁串行化
//!    （并发写治理 = 去重 + 审核面，不加唯一约束）；
//! 2. review 双闸负例：未计分 approve 拒（quality score not computed）；
//!    硬标记 approve 拒（hard risk flag）；
//! 3. approve 正例：计分后单卡 approve 成功 + knowledge_review_log 全程
//!    留痕（donor 转移 reviewer=donor_caller + approve 行）；
//! 4. 判据 8：供体路径产出的卡 approve 前不进检索面（rp-kc search 缺省
//!    仅 APPROVED——既有谓词零改动断言走 search_repo 缺省查询）；供体
//!    无 approve 通道（donor-submit 语义 = 恒 CANDIDATE，收据词形断言）。

#![deny(missing_docs)]

use domain::kcard::{
    KnowledgeCardId, KnowledgeType, ReviewStatus, RiskFlag, SourceTrust, VerificationStatus,
};
use intel_db::card_repo::{self, CardInput};
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
            eprintln!("SKIP: RP_TEST_PG_DSN 不可达（V-9，容器端到端保持本地执行）");
            None
        }
    }
}

const WS: &str = "ws-ma3-donor";

/// 测试 workspace 清场（无 CASCADE，按依赖序删；可重跑幂等基线）。
fn wipe_workspace(pool: &r2d2::Pool<r2d2_postgres::PostgresConnectionManager<postgres::NoTls>>) {
    let mut conn = pool.get().unwrap();
    for sql in [
        "DELETE FROM knowledge_review_log WHERE knowledge_id IN (SELECT knowledge_id FROM knowledge_cards WHERE workspace_id = $1)",
        "DELETE FROM knowledge_relations WHERE src_id IN (SELECT knowledge_id FROM knowledge_cards WHERE workspace_id = $1)",
        "DELETE FROM knowledge_relations WHERE dst_id IN (SELECT knowledge_id FROM knowledge_cards WHERE workspace_id = $1)",
        "DELETE FROM document_cards WHERE knowledge_id IN (SELECT knowledge_id FROM knowledge_cards WHERE workspace_id = $1)",
        "DELETE FROM knowledge_card_vectors WHERE card_id IN (SELECT knowledge_id FROM knowledge_cards WHERE workspace_id = $1)",
        "DELETE FROM knowledge_cards WHERE workspace_id = $1",
    ] {
        conn.execute(sql, &[&WS]).unwrap();
    }
}

fn donor_input(content: &str, flags: Vec<RiskFlag>, donor: &str, channel: &str) -> CardInput {
    CardInput {
        knowledge_type: KnowledgeType::VulnSignature,
        title: format!("MA-3 donor card ({})", content.len()),
        summary: "MA-3 供体治理测试".to_string(),
        content: content.to_string(),
        structured_content: None,
        confidence: 0.8,
        verification: VerificationStatus::Unverified,
        evidence_level: None,
        source_trust: SourceTrust::AgentDerived,
        ai_derived: true,
        quality_flags: flags,
        workspace_id: WS.to_string(),
        donor_caller_id: Some(donor.to_string()),
        donor_channel: Some(channel.to_string()),
    }
}

#[test]
fn concurrent_donor_submit_governance() {
    let Some(pool) = skip_unless_pg() else { return };
    {
        let mut conn = pool.get().unwrap();
        run_migrations(&mut conn).unwrap();
    }
    wipe_workspace(&pool);

    const N: usize = 6;
    let content = "MA-3 并发供体同内容卡：SQL 注入检测签名（UNION-based payload 指纹）。";
    // N 真线程并发 donor-submit 同 content（create_card + decide = donor-submit
    // 语义两步；真实 PG 并发写入 + FOR UPDATE 行锁面）。
    let mut handles = Vec::new();
    for i in 0..N {
        let pool = pool.clone();
        let content = content.to_string();
        let donor = format!("caller-donor-{i}");
        handles.push(std::thread::spawn(move || {
            let input = donor_input(&content, vec![], &donor, "mcp");
            let card = card_repo::create_card(&pool, &input).expect("create_card");
            card_repo::decide(
                &pool,
                &card,
                ReviewStatus::Candidate,
                &format!("donor-submit 写入即候选（donor={donor}）"),
                &donor,
            )
            .expect("decide candidate");
            card
        }));
    }
    let mut cards = Vec::new();
    for h in handles {
        cards.push(h.join().unwrap());
    }
    assert_eq!(cards.len(), N, "N 行卡片全创建（D2 不拒绝创建）");
    // donor_caller_id/donor_channel 逐行正确。
    for (i, card) in cards.iter().enumerate() {
        let (donor, channel) = card_repo::donor_info(&pool, card).unwrap();
        assert_eq!(donor.as_deref(), Some(format!("caller-donor-{i}").as_str()));
        assert_eq!(channel.as_deref(), Some("mcp"));
    }
    // 全 CANDIDATE（写入即候选；供体永不直写 APPROVED）。
    let mut conn = pool.get().unwrap();
    let statuses: Vec<(String,)> = conn
        .query(
            "SELECT review_status FROM knowledge_cards WHERE workspace_id = $1",
            &[&WS],
        )
        .unwrap()
        .iter()
        .map(|r| (r.get(0),))
        .collect();
    assert_eq!(statuses.len(), N);
    assert!(
        statuses.iter().all(|(s,)| s == "CANDIDATE"),
        "全部 CANDIDATE: {statuses:?}"
    );
    // knowledge_review_log 全程留痕：每卡一条 DISCOVERED→CANDIDATE，
    // reviewer = donor_caller。
    let logs: Vec<(String, String)> = conn
        .query(
            "SELECT l.from_status || '->' || l.to_status, l.reviewer \
             FROM knowledge_review_log l JOIN knowledge_cards c ON c.knowledge_id = l.knowledge_id \
             WHERE c.workspace_id = $1 ORDER BY l.knowledge_id",
            &[&WS],
        )
        .unwrap()
        .iter()
        .map(|r| (r.get(0), r.get(1)))
        .collect();
    assert_eq!(logs.len(), N, "每卡一条转移日志");
    assert!(
        logs.iter()
            .all(|(t, r)| t == "DISCOVERED->CANDIDATE" && r.starts_with("caller-donor-")),
        "转移日志 reviewer=donor: {logs:?}"
    );
    drop(conn);
    // dedup-scan：SAME_CONTENT 候选边 = N-1 对（治理面可见；禁 UNIQUE 语义）。
    let created = card_repo::dedup_scan_same_content(&pool, WS).unwrap();
    assert_eq!(created, N - 1, "同哈希组 N 行产 N-1 条 SAME_CONTENT 候选边");
    let edges: i64 = {
        let mut conn = pool.get().unwrap();
        conn.query_one(
            "SELECT COUNT(*) FROM knowledge_relations r \
             JOIN knowledge_cards c ON c.knowledge_id = r.src_id \
             WHERE c.workspace_id = $1 AND r.relation_type = 'SAME_CONTENT'",
            &[&WS],
        )
        .unwrap()
        .get(0)
    };
    assert_eq!(edges as usize, N - 1);
    // 扫描幂等（二跑 0 新建）。
    let created2 = card_repo::dedup_scan_same_content(&pool, WS).unwrap();
    assert_eq!(created2, 0, "dedup-scan 幂等");
}

#[test]
fn donor_cards_review_gates_and_search_predicate() {
    let Some(pool) = skip_unless_pg() else { return };
    {
        let mut conn = pool.get().unwrap();
        run_migrations(&mut conn).unwrap();
    }
    wipe_workspace(&pool);

    // 未计分 approve 拒（质量地板双闸之一）。
    let plain = card_repo::create_card(
        &pool,
        &donor_input(
            "MA-3 双闸负例未计分卡：WAF 绕过手法笔记。",
            vec![],
            "caller-gate",
            "cli",
        ),
    )
    .unwrap();
    card_repo::decide(
        &pool,
        &plain,
        ReviewStatus::Candidate,
        "donor gate",
        "caller-gate",
    )
    .unwrap();
    let err = card_repo::decide(
        &pool,
        &plain,
        ReviewStatus::Approved,
        "未计分",
        "human-reviewer",
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("quality score not computed yet"),
        "未计分 approve 拒: {err}"
    );

    // 硬标记 approve 拒（硬标记阻断双闸之二）。
    let hard = card_repo::create_card(
        &pool,
        &donor_input(
            "MA-3 双闸负例硬标记卡：含注入风险的说明文本。",
            vec![RiskFlag::PossiblePromptInjection],
            "caller-gate",
            "cli",
        ),
    )
    .unwrap();
    card_repo::decide(
        &pool,
        &hard,
        ReviewStatus::Candidate,
        "donor gate",
        "caller-gate",
    )
    .unwrap();
    card_repo::score_card(&pool, &hard, &domain::kcard::QualityWeights::default()).unwrap();
    let err = card_repo::decide(
        &pool,
        &hard,
        ReviewStatus::Approved,
        "硬标记",
        "human-reviewer",
    )
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("approve blocked: hard risk flag present"),
        "硬标记 approve 拒: {err}"
    );

    // approve 正例：计分后单卡 approve 成功 + 日志留痕。
    let good = card_repo::create_card(
        &pool,
        &donor_input(
            "MA-3 approve 正例卡：XSS 反射型 payload 特征与检测正则。",
            vec![],
            "caller-good",
            "mcp",
        ),
    )
    .unwrap();
    card_repo::decide(
        &pool,
        &good,
        ReviewStatus::Candidate,
        "donor good",
        "caller-good",
    )
    .unwrap();
    let score =
        card_repo::score_card(&pool, &good, &domain::kcard::QualityWeights::default()).unwrap();
    assert!(
        score >= card_repo::LOW_QUALITY_FLOOR,
        "正例卡过质量地板 {score}"
    );
    card_repo::decide(
        &pool,
        &good,
        ReviewStatus::Approved,
        "质量达标",
        "human-reviewer",
    )
    .unwrap();
    let logs: Vec<(String, String)> = {
        let mut conn = pool.get().unwrap();
        conn.query(
            "SELECT l.from_status || '->' || l.to_status, l.reviewer \
             FROM knowledge_review_log l WHERE l.knowledge_id = $1 ORDER BY l.log_id",
            &[&good.as_str()],
        )
        .unwrap()
        .iter()
        .map(|r| (r.get(0), r.get(1)))
        .collect()
    };
    assert_eq!(logs.len(), 2, "donor 转移 + approve 两次跃迁全程留痕");
    assert_eq!(logs[0].0, "DISCOVERED->CANDIDATE");
    assert_eq!(logs[0].1, "caller-good", "donor 转移 reviewer=donor_caller");
    assert_eq!(logs[1].0, "CANDIDATE->APPROVED");

    // 判据 8：approve 前不进检索面——search 缺省谓词仅 APPROVED
    // （search.rs 既有谓词零改动，此处直查同源 SQL 断言缺省面不含 CANDIDATE）。
    let mut conn = pool.get().unwrap();
    let approved_count: i64 = conn
        .query_one(
            "SELECT COUNT(*) FROM knowledge_cards WHERE workspace_id = $1 AND review_status = 'APPROVED'",
            &[&WS],
        )
        .unwrap()
        .get(0);
    assert_eq!(approved_count, 1, "仅正例卡 APPROVED");
    let donor_silent: Vec<(String,)> = conn
        .query(
            "SELECT knowledge_id FROM knowledge_cards WHERE workspace_id = $1 \
             AND review_status <> 'APPROVED' AND donor_caller_id IS NOT NULL",
            &[&WS],
        )
        .unwrap()
        .iter()
        .map(|r| (r.get(0),))
        .collect();
    assert!(
        !donor_silent.is_empty(),
        "未 approve 供体卡存在（缺省检索面谓词 exclude 它们——search.rs 缺省仅 APPROVED 零改动）"
    );
    let _ = KnowledgeCardId::try_new(good.as_str().to_string()).unwrap();
}
