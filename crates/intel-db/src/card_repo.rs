//! KnowledgeCard 治理域存取面（Phase 3 P3-1，03-DOMAIN-MODEL 1.5/1.6 + R2 第五部分）。
//!
//! 职责：卡片 CRUD（审核态词表 + 风险标记数组）、文档 N:M 关联、知识关系
//! 十型（UNIQUE(src,dst,type) 幂等；CONTRADICTS 命中即给双方打 CONTRADICTED
//! 标记并回退候选进审核，R2 5.2 D5）、D2 规范化哈希（去重键）、质量分六权重
//! （domain::kcard 纯函数 + 本模块可追溯启发式子面）、审核日志（append-only，
//! R2 5.4 审核原因/历史/回退可追溯）。
//!
//! 低质量不进正式上下文（R2 第十四部分 Phase 3 行）：[`LOW_QUALITY_FLOOR`]
//! 之下拒绝 APPROVED（调用方须先补强或隔离）；硬性风险标记
//!（POSSIBLE_PROMPT_INJECTION，13 号冻结物第三节）命中即禁 APPROVED。
//! D1 sha256 文件级去重复用 P1 content_objects 面（同文件多来源单对象已交付）。

use crate::{new_entity_id, IntelError, IntelResult, PgPool};
use domain::kcard::{
    quality_score, EvidenceLevel, KnowledgeCardId, KnowledgeRelationId, KnowledgeType, QualityInputs,
    QualityWeights, RelationType, ReviewStatus, RiskFlag, SourceTrust, VerificationStatus,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// 低质量地板：质量分低于此值拒绝 APPROVED（R2 5.3「不能只凭总分判定可信」
/// 的反向护栏——总分只是必要条件之一；风险标记为独立充分条件）。
pub const LOW_QUALITY_FLOOR: f32 = 0.35;

/// D2 规范化规则（R2 5.2：统一换行与编码、去无意义空白、保留代码与关键标点）。
/// 规则（按序，不改语义内容，原始版本由卡片 content 列承载）：
/// 1. CRLF/CR 统一 LF；2. 行首全角空格剥离（CJK 排版空白，非代码缩进），
///    其余行首缩进保留（代码语义不破坏）；3. 行尾空白剥离；
/// 4. 三连以上空行折叠为一行；5. 首尾空行剪除。
pub fn normalize_text(text: &str) -> String {
    let unified = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut out: Vec<String> = Vec::new();
    let mut blank_run = 0usize;
    for line in unified.split('\n') {
        let line = line.trim_start_matches('\u{3000}').replace('\u{3000}', " ");
        let rstripped = line.trim_end();
        if rstripped.trim().is_empty() {
            blank_run += 1;
            if blank_run <= 1 && !out.is_empty() {
                out.push(String::new());
            }
            continue;
        }
        blank_run = 0;
        out.push(rstripped.to_string());
    }
    // 首尾空行剪除（首行空行已被上面的 out.is_empty() 跳过；尾部再剪）。
    while out.last().map(|l| l.is_empty()).unwrap_or(false) {
        out.pop();
    }
    out.join("\n")
}

/// D2 规范化哈希（sha256 hex；规范化文本的寻址键）。
pub fn normalized_hash(text: &str) -> String {
    let normalized = normalize_text(text);
    let mut h = Sha256::new();
    h.update(normalized.as_bytes());
    format!("{:x}", h.finalize())
}

fn check_flags(words: &[String]) -> IntelResult<Vec<RiskFlag>> {
    words
        .iter()
        .map(|w| RiskFlag::parse(w).map_err(IntelError::from))
        .collect()
}

fn flags_to_strs(flags: &[RiskFlag]) -> Vec<String> {
    flags.iter().map(|f| f.as_str().to_string()).collect()
}

/// 卡片创建输入（审核态落 DISCOVERED；质量分由 score 面计算后回填）。
#[derive(Debug, Clone)]
pub struct CardInput {
    pub knowledge_type: KnowledgeType,
    pub title: String,
    pub summary: String,
    /// 卡片正文（D2 规范化哈希输入；原文随列保留）。
    pub content: String,
    /// 类型化载荷（可选）。
    pub structured_content: Option<Value>,
    pub confidence: f32,
    pub verification: VerificationStatus,
    pub evidence_level: Option<EvidenceLevel>,
    pub source_trust: SourceTrust,
    /// 原则 C：AI 派生必标。
    pub ai_derived: bool,
    /// 风险标记（D2/D3/D4/D5 与导入面产出的判定依据随行）。
    pub quality_flags: Vec<RiskFlag>,
    pub workspace_id: String,
}

/// 创建卡片（幂等键不在本面：D2 相同哈希产 SAME_CONTENT 候选边，不拒绝创建，
/// R2 5.2「语义相似不得直接触发自动合并」对 D2 同理——合并走审核）。
pub fn create_card(pool: &PgPool, input: &CardInput) -> IntelResult<KnowledgeCardId> {
    if input.title.trim().is_empty() {
        return Err(IntelError::Invalid("title must be non-empty".to_string()));
    }
    if !(0.0..=1.0).contains(&input.confidence) {
        return Err(IntelError::Invalid("confidence must be within [0,1]".to_string()));
    }
    let id = KnowledgeCardId::try_new(new_entity_id("kc-"))?;
    let norm = normalized_hash(&input.content);
    let mut conn = pool.get()?;
    conn.execute(
        "INSERT INTO knowledge_cards (knowledge_id, knowledge_type, title, summary, content,
             structured_content, confidence, verification_status, review_status, evidence_level,
             source_trust, ai_derived, normalized_hash, quality_flags, workspace_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'DISCOVERED',$9,$10,$11,$12,$13,$14)",
        &[
            &id.as_str(),
            &input.knowledge_type.as_str(),
            &input.title,
            &input.summary,
            &input.content,
            &input.structured_content,
            &input.confidence,
            &input.verification.as_str(),
            &input.evidence_level.map(|e| e.as_str()),
            &input.source_trust.as_str(),
            &input.ai_derived,
            &norm,
            &flags_to_strs(&input.quality_flags),
            &input.workspace_id,
        ],
    )?;
    Ok(id)
}

/// 卡片-文档 N:M 关联（版本锚定；幂等：PRIMARY KEY 冲突忽略）。
pub fn link_document(
    pool: &PgPool,
    document_id: &domain::DocumentId,
    revision_id: &domain::DocumentRevisionId,
    card: &KnowledgeCardId,
) -> IntelResult<()> {
    let mut conn = pool.get()?;
    conn.execute(
        "INSERT INTO document_cards (document_id, knowledge_id, document_revision_id)
         VALUES ($1,$2,$3)
         ON CONFLICT (document_id, knowledge_id, document_revision_id) DO NOTHING",
        &[&document_id.as_str(), &card.as_str(), &revision_id.as_str()],
    )?;
    Ok(())
}

/// 追加知识关系（幂等：UNIQUE(src,dst,type) 命中返回既有关系）。
/// D5 硬性：CONTRADICTS 命中即给 dst 打 CONTRADICTED 标记并把审核态回退到
/// CANDIDATE（进审核而非删一方），全程落审核日志。
pub fn add_relation(
    pool: &PgPool,
    src: &KnowledgeCardId,
    dst: &KnowledgeCardId,
    relation_type: RelationType,
    evidence: Option<&Value>,
    source: &str,
    confidence: f32,
) -> IntelResult<(KnowledgeRelationId, bool)> {
    if !(0.0..=1.0).contains(&confidence) {
        return Err(IntelError::Invalid("confidence must be within [0,1]".to_string()));
    }
    if source.trim().is_empty() {
        return Err(IntelError::Invalid("relation source must be non-empty (R2 3.1)".to_string()));
    }
    let mut conn = pool.get()?;
    let mut tx = conn.transaction()?;
    let existing: Option<String> = tx
        .query_opt(
            "SELECT relation_id FROM knowledge_relations WHERE src_id=$1 AND dst_id=$2 AND relation_type=$3",
            &[&src.as_str(), &dst.as_str(), &relation_type.as_str()],
        )?
        .map(|r| r.get(0));
    if let Some(rid) = existing {
        tx.commit()?;
        return Ok((KnowledgeRelationId::try_new(rid)?, false));
    }
    let rid = KnowledgeRelationId::try_new(new_entity_id("krel-"))?;
    tx.execute(
        "INSERT INTO knowledge_relations (relation_id, src_id, dst_id, relation_type,
             relation_evidence, relation_source, confidence)
         VALUES ($1,$2,$3,$4,$5,$6,$7)",
        &[
            &rid.as_str(),
            &src.as_str(),
            &dst.as_str(),
            &relation_type.as_str(),
            &evidence.cloned().unwrap_or(Value::Null),
            &source,
            &confidence,
        ],
    )?;
    if relation_type == RelationType::Contradicts {
        // D5 硬性：命中 CONTRADICTS 即回退候选进审核（from 态先取后改，日志可追溯）。
        let from_status: String = tx
            .query_one(
                "SELECT review_status FROM knowledge_cards WHERE knowledge_id = $1 FOR UPDATE",
                &[&dst.as_str()],
            )?
            .get(0);
        tx.execute(
            "UPDATE knowledge_cards
             SET quality_flags = (SELECT array_agg(DISTINCT f) FROM unnest(quality_flags || ARRAY['CONTRADICTED']) AS f),
                 review_status = CASE WHEN review_status = 'APPROVED' THEN 'CANDIDATE' ELSE review_status END,
                 updated_at = now()
             WHERE knowledge_id = $1",
            &[&dst.as_str()],
        )?;
        tx.execute(
            "INSERT INTO knowledge_review_log (knowledge_id, from_status, to_status, decision, reason, reviewer)
             VALUES ($1,$2,'CANDIDATE','contradicted',$3,'deducer')",
            &[
                &dst.as_str(),
                &from_status,
                &format!("CONTRADICTS by {}", src.as_str()),
            ],
        )?;
    }
    tx.commit()?;
    Ok((rid, true))
}

/// 卡片摘要视图。
#[derive(Debug, serde::Serialize)]
pub struct CardSummary {
    pub knowledge_id: String,
    pub knowledge_type: String,
    pub title: String,
    pub review_status: String,
    pub verification_status: String,
    pub quality_score: Option<f32>,
    pub quality_flags: Vec<String>,
    pub evidence_level: Option<String>,
    pub source_trust: String,
    pub normalized_hash: Option<String>,
    pub created_at: String,
}

/// 按审核态列卡（workspace 前置谓词在 SQL 内，13 号冻结物第五节口径）。
pub fn list_by_review(
    pool: &PgPool,
    workspace_id: &str,
    status: Option<ReviewStatus>,
) -> IntelResult<Vec<CardSummary>> {
    let mut conn = pool.get()?;
    let rows = match status {
        Some(s) => conn.query(
            "SELECT knowledge_id, knowledge_type, title, review_status, verification_status,
                    quality_score, quality_flags, evidence_level, source_trust, normalized_hash, created_at
             FROM knowledge_cards WHERE workspace_id = $1 AND review_status = $2
             ORDER BY created_at DESC",
            &[&workspace_id, &s.as_str()],
        )?,
        None => conn.query(
            "SELECT knowledge_id, knowledge_type, title, review_status, verification_status,
                    quality_score, quality_flags, evidence_level, source_trust, normalized_hash, created_at
             FROM knowledge_cards WHERE workspace_id = $1
             ORDER BY created_at DESC",
            &[&workspace_id],
        )?,
    };
    Ok(rows
        .iter()
        .map(|r| {
            let ts: std::time::SystemTime = r.get(10);
            CardSummary {
                knowledge_id: r.get(0),
                knowledge_type: r.get(1),
                title: r.get(2),
                review_status: r.get(3),
                verification_status: r.get(4),
                quality_score: r.get(5),
                quality_flags: r.get(6),
                evidence_level: r.get(7),
                source_trust: r.get(8),
                normalized_hash: r.get(9),
                created_at: crate::document_repo::timestamp_to_string(ts),
            }
        })
        .collect())
}

/// 卡片详情（含关系与文档关联）。
#[derive(Debug, serde::Serialize)]
pub struct CardDetail {
    #[serde(flatten)]
    pub summary: CardSummary,
    pub summary_text: String,
    pub content: String,
    pub structured_content: Option<Value>,
    pub confidence: f32,
    pub ai_derived: bool,
    pub documents: Vec<String>,
    pub relations: Vec<Value>,
    pub review_log: Vec<Value>,
}

/// 取卡片详情。
pub fn get_card(pool: &PgPool, card: &KnowledgeCardId) -> IntelResult<CardDetail> {
    let mut conn = pool.get()?;
    let row = conn
        .query_opt(
            "SELECT knowledge_id, knowledge_type, title, summary, content, structured_content,
                    confidence, verification_status, review_status, quality_score, quality_flags,
                    evidence_level, source_trust, ai_derived, normalized_hash, created_at
             FROM knowledge_cards WHERE knowledge_id = $1",
            &[&card.as_str()],
        )?
        .ok_or_else(|| IntelError::Invalid(format!("card {} not found", card.as_str())))?;
    let docs = conn.query(
        "SELECT document_id FROM document_cards WHERE knowledge_id = $1",
        &[&card.as_str()],
    )?;
    let rels = conn.query(
        "SELECT relation_id, src_id, dst_id, relation_type, relation_source, confidence
         FROM knowledge_relations WHERE src_id = $1 OR dst_id = $1",
        &[&card.as_str()],
    )?;
    let log = conn.query(
        "SELECT from_status, to_status, decision, reason, reviewer, created_at
         FROM knowledge_review_log WHERE knowledge_id = $1 ORDER BY created_at",
        &[&card.as_str()],
    )?;
    let ts: std::time::SystemTime = row.get(15);
    let summary = CardSummary {
        knowledge_id: row.get(0),
        knowledge_type: row.get(1),
        title: row.get(2),
        review_status: row.get(8),
        verification_status: row.get(7),
        quality_score: row.get(9),
        quality_flags: row.get(10),
        evidence_level: row.get(11),
        source_trust: row.get(12),
        normalized_hash: row.get(14),
        created_at: crate::document_repo::timestamp_to_string(ts),
    };
    Ok(CardDetail {
        documents: docs.iter().map(|r| r.get(0)).collect(),
        relations: rels
            .iter()
            .map(|r| {
                serde_json::json!({
                    "relation_id": r.get::<_, String>(0),
                    "src_id": r.get::<_, String>(1),
                    "dst_id": r.get::<_, String>(2),
                    "relation_type": r.get::<_, String>(3),
                    "relation_source": r.get::<_, String>(4),
                    "confidence": r.get::<_, Option<f32>>(5),
                })
            })
            .collect(),
        review_log: log
            .iter()
            .map(|r| {
                let c: std::time::SystemTime = r.get(5);
                serde_json::json!({
                    "from_status": r.get::<_, String>(0),
                    "to_status": r.get::<_, String>(1),
                    "decision": r.get::<_, String>(2),
                    "reason": r.get::<_, String>(3),
                    "reviewer": r.get::<_, String>(4),
                    "created_at": crate::document_repo::timestamp_to_string(c),
                })
            })
            .collect(),
        summary_text: row.get(3),
        content: row.get(4),
        structured_content: row.get(5),
        confidence: row.get(6),
        ai_derived: row.get(13),
        summary,
    })
}

/// 审核决定（append-only 日志 + 状态迁移；转移矩阵 fail-closed）。
pub fn decide(
    pool: &PgPool,
    card: &KnowledgeCardId,
    to: ReviewStatus,
    reason: &str,
    reviewer: &str,
) -> IntelResult<()> {
    let mut conn = pool.get()?;
    let mut tx = conn.transaction()?;
    let row = tx.query_one(
        "SELECT review_status, quality_score, quality_flags FROM knowledge_cards WHERE knowledge_id = $1 FOR UPDATE",
        &[&card.as_str()],
    )?;
    let (from, score, flags): (String, Option<f32>, Vec<String>) = (row.get(0), row.get(1), row.get(2));
    let flags = check_flags(&flags)?;
    let hard_hit = flags.iter().any(|f| f.is_hard_block());
    if to == ReviewStatus::Approved {
        if hard_hit {
            return Err(IntelError::Invalid(format!(
                "approve blocked: hard risk flag present ({:?}); quarantine or remediate first (13-SECURITY 第三节)",
                flags.iter().filter(|f| f.is_hard_block()).map(|f| f.as_str()).collect::<Vec<_>>()
            )));
        }
        if let Some(s) = score {
            if s < LOW_QUALITY_FLOOR {
                return Err(IntelError::Invalid(format!(
                    "approve blocked: quality score {s} below floor {LOW_QUALITY_FLOOR} (低质量不进正式上下文)"
                )));
            }
        } else {
            return Err(IntelError::Invalid(
                "approve blocked: quality score not computed yet (rp-kc score first)".to_string(),
            ));
        }
    }
    let from_parsed = ReviewStatus::parse(&from).map_err(IntelError::from)?;
    let allowed = matches!(
        (from_parsed, to),
        (_, ReviewStatus::Quarantined)
            | (_, ReviewStatus::Rejected)
            | (ReviewStatus::Discovered, ReviewStatus::Parsed)
            | (ReviewStatus::Parsed, ReviewStatus::Candidate)
            | (ReviewStatus::Discovered, ReviewStatus::Candidate)
            | (ReviewStatus::Candidate, ReviewStatus::Approved)
            | (ReviewStatus::Parsed, ReviewStatus::Approved)
            | (ReviewStatus::Discovered, ReviewStatus::Approved)
            | (ReviewStatus::Quarantined, ReviewStatus::Approved)
            | (ReviewStatus::Approved, ReviewStatus::Candidate)
            | (ReviewStatus::Quarantined, ReviewStatus::Candidate)
            | (ReviewStatus::Approved, ReviewStatus::Deprecated)
            | (ReviewStatus::Candidate, ReviewStatus::Deprecated)
            | (_, ReviewStatus::Parsed)
    );
    if !allowed || from == to.as_str() {
        return Err(IntelError::Invalid(format!(
            "review transition {from} -> {} not allowed",
            to.as_str()
        )));
    }
    tx.execute(
        "UPDATE knowledge_cards SET review_status = $2, updated_at = now() WHERE knowledge_id = $1",
        &[&card.as_str(), &to.as_str()],
    )?;
    tx.execute(
        "INSERT INTO knowledge_review_log (knowledge_id, from_status, to_status, decision, reason, reviewer)
         VALUES ($1,$2,$3,$4,$5,$6)",
        &[
            &card.as_str(),
            &from,
            &to.as_str(),
            &to.as_str().to_lowercase(),
            &reason,
            &reviewer,
        ],
    )?;
    tx.commit()?;
    Ok(())
}

/// 来源可靠性与可验证性子面（启发式，依据 = source_trust 词表 + K 分级 +
/// 验证状态；映射表在本函数内，可追溯）。
fn heuristic_inputs(
    source_trust: &str,
    evidence: Option<&str>,
    verification: &str,
    summary: &str,
    structured: Option<&Value>,
    doc_count: i64,
    age_days: i64,
) -> QualityInputs {
    let trust = match SourceTrust::parse(source_trust).ok().unwrap_or(SourceTrust::ExternalImport) {
        SourceTrust::Manual => 0.8,
        SourceTrust::RtisSnapshot => 0.9,
        SourceTrust::ExternalImport => 0.5,
        SourceTrust::AgentDerived => 0.35,
    };
    let k = EvidenceLevel::parse(evidence.unwrap_or("K0")).ok().unwrap_or(EvidenceLevel::K0);
    let source_reliability: f32 = (trust + match k {
        EvidenceLevel::K4 => 0.05_f32,
        EvidenceLevel::K3 => 0.05_f32,
        _ => 0.0_f32,
    })
    .clamp(0.0, 1.0);
    let c_struct: f32 = if structured.is_some() { 0.3 } else { 0.0 };
    let c_summary: f32 = if summary.trim().is_empty() { 0.0 } else { 0.3 };
    let completeness: f32 = (0.4 + c_struct + c_summary).clamp(0.0, 1.0);
    let verifiability = if verification == VerificationStatus::Verified.as_str() {
        1.0
    } else {
        match k {
            EvidenceLevel::K4 | EvidenceLevel::K3 | EvidenceLevel::K2 => 0.7,
            EvidenceLevel::K1 | EvidenceLevel::K0 => 0.4,
        }
    };
    let timeliness: f32 = if age_days <= 30 {
        1.0
    } else {
        (1.0 - 0.6 * ((age_days - 30) as f32 / 700.0)).clamp(0.4, 1.0)
    };
    let independence = (0.4 + 0.2 * doc_count.clamp(0, 3) as f32).clamp(0.0, 1.0);
    let accuracy = if verification == VerificationStatus::Verified.as_str() {
        1.0
    } else {
        0.5
    };
    QualityInputs {
        source_reliability,
        completeness,
        verifiability,
        timeliness,
        independence,
        accuracy,
    }
}

/// 计算并回填质量分（六权重 R2 5.3 缺省；输入面见 heuristic_inputs，可追溯）。
pub fn score_card(pool: &PgPool, card: &KnowledgeCardId, weights: &QualityWeights) -> IntelResult<f32> {
    weights.validate().map_err(IntelError::from)?;
    let mut conn = pool.get()?;
    let row = conn
        .query_opt(
            "SELECT source_trust, evidence_level, verification_status, summary, structured_content, updated_at,
                    (SELECT COUNT(DISTINCT document_id) FROM document_cards WHERE knowledge_id = $1)
             FROM knowledge_cards WHERE knowledge_id = $1",
            &[&card.as_str()],
        )?
        .ok_or_else(|| IntelError::Invalid(format!("card {} not found", card.as_str())))?;
    let updated: std::time::SystemTime = row.get(5);
    let age_days = updated
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86400)
        .unwrap_or(0);
    let now_days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86400)
        .unwrap_or(0);
    let age = (now_days - age_days) as i64;
    let inputs = heuristic_inputs(
        &row.get::<_, String>(0),
        row.get::<_, Option<String>>(1).as_deref(),
        &row.get::<_, String>(2),
        &row.get::<_, String>(3),
        row.get::<_, Option<Value>>(4).as_ref(),
        row.get::<_, i64>(6),
        age,
    );
    let score = quality_score(weights, &inputs);
    conn.execute(
        "UPDATE knowledge_cards SET quality_score = $2, updated_at = now() WHERE knowledge_id = $1",
        &[&card.as_str(), &score],
    )?;
    Ok(score)
}

/// D2 去重扫描（规范化哈希一致的卡片间建 SAME_CONTENT 候选边；不自动合并，
/// R2 5.2）。返回新建关系数。source = "card_repo.dedup-scan:0.1.0"。
pub fn dedup_scan_same_content(pool: &PgPool, workspace_id: &str) -> IntelResult<usize> {
    let mut conn = pool.get()?;
    let rows = conn.query(
        "SELECT knowledge_id, normalized_hash FROM knowledge_cards
         WHERE workspace_id = $1 AND normalized_hash IS NOT NULL
         ORDER BY created_at",
        &[&workspace_id],
    )?;
    let mut groups: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for r in rows {
        let hash: String = r.get(1);
        groups.entry(hash).or_default().push(r.get(0));
    }
    let mut created = 0usize;
    for (_hash, mut ids) in groups {
        if ids.len() < 2 {
            continue;
        }
        ids.sort();
        let first = ids[0].clone();
        for other in ids.iter().skip(1) {
            let src = KnowledgeCardId::try_new(first.clone()).map_err(IntelError::from)?;
            let dst = KnowledgeCardId::try_new(other.clone()).map_err(IntelError::from)?;
            let n = add_relation(
                pool,
                &src,
                &dst,
                RelationType::SameContent,
                Some(&serde_json::json!({"rule": "D2 normalized_hash equal"})),
                "card_repo.dedup-scan:0.1.0",
                0.95,
            )?;
            if n.1 {
                created += 1;
            }
        }
    }
    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_rules() {
        // CRLF 统一 + 行尾空白 + 全角空格 + 三连空行折叠 + 首尾空行剪除。
        let raw = "第一行  \r\n\u{3000}第二行\r\n\n\n\n第三行";
        let n = normalize_text(raw);
        assert_eq!(n, "第一行\n第二行\n\n第三行");
        // 代码缩进保留（行首不剥离：代码语义不破坏，R2 5.2 保留代码）。
        let code = "def f():\n    return 1";
        assert_eq!(normalize_text(code), "def f():\n    return 1");
        // 尾空行与首空行剪除。
        assert_eq!(normalize_text("\n\n内容\n\n\n"), "内容");
    }

    #[test]
    fn normalized_hash_stable_and_distinct() {
        let a = normalized_hash("同一段内容\n");
        let b = normalized_hash("同一段内容");
        assert_eq!(a, b, "尾随换行不影响规范化哈希");
        let c = normalized_hash("不同内容");
        assert_ne!(a, c);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn score_floor_gate_values() {
        assert!(LOW_QUALITY_FLOOR > 0.0 && LOW_QUALITY_FLOOR < 1.0);
        let w = QualityWeights::default();
        let weak = QualityInputs {
            source_reliability: 0.3,
            completeness: 0.3,
            verifiability: 0.3,
            timeliness: 0.3,
            independence: 0.3,
            accuracy: 0.3,
        };
        assert!(quality_score(&w, &weak) < LOW_QUALITY_FLOOR, "弱输入必须低于地板");
    }
}
