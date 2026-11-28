//! PB-010/PB-012 发布回放（Publication Replay）：AI 供体写入只到 CANDIDATE，
//! 人工/服务审批经双闸（硬标记 + 质量地板 + 未计分阻断）才到 APPROVED，
//! 统一检索（approved_only 缺省谓词）随之复得，未批准候选不进检索。
//! bm25 索引（V0006 idx_cards_bm25）随行更新实测：批准后无需独立重建步。
//!
//! RP_TEST_PG_DSN 门控（phase7_maintenance 同口径）：CI 无 PG service 提前
//! 返回；RP_REQUIRE_PG_TESTS=1 时缺失/不可用即 panic（本地隔离 PG 实跑）。

use domain::kcard::{
    KnowledgeCardId, KnowledgeType, ReviewStatus, RiskFlag, SourceTrust, VerificationStatus,
};
use intel_db::card_repo::{self, CardInput};
use intel_db::search::{self, SearchFilters};
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
        max_connections: 4,
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
        "{}{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

/// 供体输入（MCP `rip.submit_knowledge_candidate` → `rp-kc donor-submit` 同一
/// 治理面：AgentDerived + ai_derived 定死，供体永不直写 APPROVED）。
fn donor_input(workspace: &str, title: &str, content: &str, flags: Vec<RiskFlag>) -> CardInput {
    CardInput {
        knowledge_type: KnowledgeType::VulnSignature,
        title: title.to_string(),
        summary: "PB-010 发布回放夹具摘要".to_string(),
        content: content.to_string(),
        structured_content: Some(json!({
            "contract_version": "1.0",
            "fixture": "publication-replay",
        })),
        confidence: 0.6,
        verification: VerificationStatus::Unverified,
        evidence_level: None,
        source_trust: SourceTrust::AgentDerived,
        ai_derived: true,
        quality_flags: flags,
        workspace_id: workspace.to_string(),
        donor_caller_id: Some("caller-pb010-replay".to_string()),
        donor_channel: Some("mcp".to_string()),
    }
}

/// donor-submit 两步（create → decide Candidate，reviewer=donor_caller 留痕）
/// 的库面等价：与 rp-kc donor-submit / MCP 提交通道共用同一治理函数。
fn donor_submit(pool: &intel_db::PgPool, input: &CardInput) -> KnowledgeCardId {
    let card = card_repo::create_card(pool, input).unwrap();
    card_repo::decide(
        pool,
        &card,
        ReviewStatus::Candidate,
        &format!(
            "donor-submit 写入即候选（donor={}）",
            input.donor_caller_id.clone().unwrap()
        ),
        input.donor_caller_id.clone().unwrap().as_str(),
    )
    .unwrap();
    card
}

fn review_status_of(pool: &intel_db::PgPool, card: &KnowledgeCardId) -> String {
    let mut conn = pool.get().unwrap();
    let row = conn
        .query_one(
            "SELECT review_status FROM knowledge_cards WHERE knowledge_id = $1",
            &[&card.as_str()],
        )
        .unwrap();
    row.get(0)
}

fn search_ids(
    pool: &intel_db::PgPool,
    workspace: &str,
    query: &str,
    approved_only: bool,
) -> Vec<String> {
    let mut filters = SearchFilters::new(workspace);
    filters.approved_only = approved_only;
    search::bm25_search(pool, &filters, query, 20)
        .unwrap()
        .into_iter()
        .map(|hit| hit.knowledge_id)
        .collect()
}

#[test]
fn publication_replay_candidate_to_approved_to_searchable() {
    let Some(pool) = pool() else { return };
    let tag = suffix();
    let workspace = format!("ws-pb010-{tag}");
    let token_ok = format!("pbok{tag}");
    let token_pending = format!("pbpend{tag}");

    // 1. AI 供体写入：两条候选（其一待批准做对照）。
    let approved = donor_submit(
        &pool,
        &donor_input(
            &workspace,
            &format!("PB010 回放 批准路径 {token_ok}"),
            &format!("知识正文 {token_ok} 检索复得断言锚"),
            Vec::new(),
        ),
    );
    let pending = donor_submit(
        &pool,
        &donor_input(
            &workspace,
            &format!("PB010 回放 待批准对照 {token_pending}"),
            &format!("知识正文 {token_pending} 对照锚"),
            Vec::new(),
        ),
    );
    assert_eq!(review_status_of(&pool, &approved), "CANDIDATE");
    assert_eq!(review_status_of(&pool, &pending), "CANDIDATE");

    // 2. 发布前：缺省检索（approved_only）不可复得；候选行已在 bm25 索引内，
    //    被谓词排除（include-unapproved 复得两证：索引在位，闸在谓词）。
    assert!(
        search_ids(&pool, &workspace, &token_ok, true).is_empty(),
        "批准前缺省检索不得复得候选"
    );
    let unapproved_hits = search_ids(&pool, &workspace, &token_ok, false);
    assert!(
        unapproved_hits.contains(&approved.as_str().to_string()),
        "include-unapproved 应复得候选（索引在位）"
    );

    // 3. 审批闸一：未计分 approve 阻断（"quality score not computed yet"）。
    let pre_score = card_repo::decide(
        &pool,
        &approved,
        ReviewStatus::Approved,
        "PB010 回放 未计分阻断断言",
        "human-pb010",
    );
    let err = match pre_score {
        Err(e) => e.to_string(),
        Ok(()) => panic!("未计分 approve 必须阻断"),
    };
    assert!(
        err.contains("not computed"),
        "未计分 approve 错误应点名质量分缺失: {err}"
    );
    assert_eq!(
        review_status_of(&pool, &approved),
        "CANDIDATE",
        "阻断后状态不变"
    );

    // 4. 人工/服务审批：计分 → approve（通过地板闸）。
    let score =
        card_repo::score_card(&pool, &approved, &domain::kcard::QualityWeights::default()).unwrap();
    assert!(
        score >= card_repo::LOW_QUALITY_FLOOR,
        "完整供体形状计分应过地板: {score}"
    );
    card_repo::decide(
        &pool,
        &approved,
        ReviewStatus::Approved,
        "PB010 回放 人工批准（发布）",
        "human-pb010",
    )
    .unwrap();
    assert_eq!(review_status_of(&pool, &approved), "APPROVED");

    // 5. 发布后复得：缺省检索命中，无独立重建步（bm25 索引随行更新实测）。
    let hits = search_ids(&pool, &workspace, &token_ok, true);
    assert_eq!(
        hits,
        vec![approved.as_str().to_string()],
        "批准后缺省检索必须复得且唯一"
    );
    assert!(
        !search_ids(&pool, &workspace, &token_pending, true)
            .contains(&pending.as_str().to_string()),
        "未批准对照候选不得进入缺省检索"
    );

    // 6. 审计链：review_log 留痕 donor 写入即候选 + 人工批准两行。
    let mut conn = pool.get().unwrap();
    let rows = conn
        .query(
            "SELECT from_status, to_status, reviewer FROM knowledge_review_log
             WHERE knowledge_id = $1 ORDER BY log_id",
            &[&approved.as_str()],
        )
        .unwrap();
    let log: Vec<(String, String, String)> = rows
        .iter()
        .map(|r| (r.get(0), r.get(1), r.get(2)))
        .collect();
    assert!(
        log.contains(&(
            "DISCOVERED".to_string(),
            "CANDIDATE".to_string(),
            "caller-pb010-replay".to_string()
        )),
        "donor 写入即候选留痕缺失: {log:?}"
    );
    assert!(
        log.contains(&(
            "CANDIDATE".to_string(),
            "APPROVED".to_string(),
            "human-pb010".to_string()
        )),
        "人工批准留痕缺失: {log:?}"
    );
}

#[test]
fn publication_replay_gate_matrix_blocks_unsafe_approval() {
    let Some(pool) = pool() else { return };
    let tag = suffix();
    let workspace = format!("ws-pb010-gate-{tag}");

    // 闸二：硬标记候选计分后仍不可批准（POSSIBLE_PROMPT_INJECTION）。
    let hard = donor_submit(
        &pool,
        &donor_input(
            &workspace,
            &format!("PB010 回放 硬标记对照 g{tag}"),
            &format!("知识正文 hardgate{tag} 锚"),
            vec![RiskFlag::PossiblePromptInjection],
        ),
    );
    card_repo::score_card(&pool, &hard, &domain::kcard::QualityWeights::default()).unwrap();
    let hard_err = card_repo::decide(
        &pool,
        &hard,
        ReviewStatus::Approved,
        "PB010 回放 硬标记阻断断言",
        "human-pb010",
    )
    .unwrap_err()
    .to_string();
    assert!(
        hard_err.contains("hard risk flag"),
        "硬标记 approve 必须阻断: {hard_err}"
    );
    assert_eq!(
        review_status_of(&pool, &hard),
        "CANDIDATE",
        "硬标记阻断后状态不变（进入人工修复队列）"
    );

    // 闸三：REJECTED 内容永不可发布（转移矩阵 fail-closed）。
    let rejected = donor_submit(
        &pool,
        &donor_input(
            &workspace,
            &format!("PB010 回放 拒绝对照 r{tag}"),
            &format!("知识正文 rejectgate{tag} 锚"),
            Vec::new(),
        ),
    );
    // 先计分：approve 闸序 = 计分/硬标记前置，转移矩阵随后（计分后 REJECTED →
    // APPROVED 才能命中转移矩阵拒绝面而非计分阻断面）。
    card_repo::score_card(&pool, &rejected, &domain::kcard::QualityWeights::default()).unwrap();
    card_repo::decide(
        &pool,
        &rejected,
        ReviewStatus::Rejected,
        "PB010 回放 人工拒绝",
        "human-pb010",
    )
    .unwrap();
    let reject_err = card_repo::decide(
        &pool,
        &rejected,
        ReviewStatus::Approved,
        "PB010 回放 拒绝后批准必须阻断",
        "human-pb010",
    )
    .unwrap_err()
    .to_string();
    assert!(
        reject_err.contains("not allowed"),
        "REJECTED → APPROVED 转移必须 fail-closed: {reject_err}"
    );
    assert_eq!(review_status_of(&pool, &rejected), "REJECTED");
    assert!(
        search_ids(&pool, &workspace, &format!("rejectgate{tag}"), true).is_empty(),
        "被拒内容不得进入缺省检索"
    );
}
