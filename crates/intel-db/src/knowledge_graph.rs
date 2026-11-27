//! Cross-authority knowledge identity references and reviewable relations.
//! Entity facts and evidence bodies remain in their owning stores.

use crate::{new_entity_id, IntelError, IntelResult, PgPool};
use domain::kcard::{
    EntitySourceReviewStatus, KnowledgeEntityRefId, KnowledgeEntityType, KnowledgeRelationId,
    RelationReviewStatus, RelationType, SourceLicenseStatus, SourceTrustClass,
};
use serde_json::Value;
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct EntitySourceInput {
    pub source_system: String,
    pub source_entity_id: String,
    pub source_version: String,
    pub canonical_uri: Option<String>,
    pub source_commit: Option<String>,
    pub content_sha256: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EntityRefInput {
    pub entity_type: KnowledgeEntityType,
    pub canonical_key: String,
    pub source: EntitySourceInput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeEntityRefRecord {
    pub entity_ref_id: KnowledgeEntityRefId,
    pub source_ref_id: String,
    pub entity_type: KnowledgeEntityType,
    pub canonical_key: String,
}

#[derive(Debug, Clone)]
pub struct EntitySourceAssessmentInput {
    pub trust_class: SourceTrustClass,
    pub license_expression: String,
    pub license_status: SourceLicenseStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntitySourceAssessmentRecord {
    pub source_ref_id: String,
    pub source_system: String,
    pub source_entity_id: String,
    pub source_version: String,
    pub source_state: String,
    pub canonical_uri: Option<String>,
    pub content_sha256: Option<String>,
    pub observed_at: String,
    pub review_status: EntitySourceReviewStatus,
    pub trust_class: SourceTrustClass,
    pub license_expression: String,
    pub license_status: SourceLicenseStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntitySourceReviewEvent {
    pub from_status: EntitySourceReviewStatus,
    pub to_status: EntitySourceReviewStatus,
    pub from_trust_class: SourceTrustClass,
    pub to_trust_class: SourceTrustClass,
    pub from_license_expression: String,
    pub to_license_expression: String,
    pub from_license_status: SourceLicenseStatus,
    pub to_license_status: SourceLicenseStatus,
    pub reason: String,
    pub reviewer: String,
    pub created_at: String,
}

/// Relation assertion metadata shared by the public and transactional write paths.
#[derive(Debug, Clone, Copy)]
pub struct RelationAssertion<'a> {
    pub relation_type: RelationType,
    pub evidence: Option<&'a Value>,
    pub source: &'a str,
    pub confidence: f32,
    pub valid_from: Option<SystemTime>,
    pub valid_to: Option<SystemTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationEvidenceRole {
    Supports,
    Contradicts,
    Context,
}

impl RelationEvidenceRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::Supports => "supports",
            Self::Contradicts => "contradicts",
            Self::Context => "context",
        }
    }
}

/// Register a stable identity and one immutable source snapshot.
pub fn register_entity_ref(
    pool: &PgPool,
    input: &EntityRefInput,
) -> IntelResult<KnowledgeEntityRefRecord> {
    validate_entity_ref_input(input)?;
    let mut conn = pool.get()?;
    let mut tx = conn.transaction()?;
    let record = register_entity_ref_tx(&mut tx, input)?;
    tx.commit()?;
    Ok(record)
}

pub(crate) fn register_entity_ref_tx(
    tx: &mut postgres::Transaction<'_>,
    input: &EntityRefInput,
) -> IntelResult<KnowledgeEntityRefRecord> {
    validate_entity_ref_input(input)?;

    let proposed_id = if input.entity_type == KnowledgeEntityType::KnowledgeCard {
        input.canonical_key.clone()
    } else {
        new_entity_id("ker-")
    };
    tx.execute(
        "INSERT INTO knowledge_entity_refs (entity_ref_id, entity_type, canonical_key)
         VALUES ($1,$2,$3) ON CONFLICT (entity_type,canonical_key) DO NOTHING",
        &[
            &proposed_id,
            &input.entity_type.as_str(),
            &input.canonical_key,
        ],
    )?;
    let entity_ref_id: String = tx
        .query_one(
            "SELECT entity_ref_id FROM knowledge_entity_refs
             WHERE entity_type=$1 AND canonical_key=$2",
            &[&input.entity_type.as_str(), &input.canonical_key],
        )?
        .get(0);

    let proposed_source_ref_id = new_entity_id("kesrc-");
    tx.execute(
        "INSERT INTO knowledge_entity_sources (
             source_ref_id, entity_ref_id, source_system, source_entity_id,
             source_version, canonical_uri, source_commit, content_sha256
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
         ON CONFLICT (entity_ref_id,source_system,source_entity_id,source_version) DO NOTHING",
        &[
            &proposed_source_ref_id,
            &entity_ref_id,
            &input.source.source_system,
            &input.source.source_entity_id,
            &input.source.source_version,
            &input.source.canonical_uri,
            &input.source.source_commit,
            &input.source.content_sha256,
        ],
    )?;
    let row = tx.query_one(
        "SELECT source_ref_id, canonical_uri, source_commit, content_sha256
         FROM knowledge_entity_sources
         WHERE entity_ref_id=$1 AND source_system=$2 AND source_entity_id=$3 AND source_version=$4",
        &[
            &entity_ref_id,
            &input.source.source_system,
            &input.source.source_entity_id,
            &input.source.source_version,
        ],
    )?;
    let source_ref_id: String = row.get(0);
    if row.get::<_, Option<String>>(1) != input.source.canonical_uri
        || row.get::<_, Option<String>>(2) != input.source.source_commit
        || row.get::<_, Option<String>>(3) != input.source.content_sha256
    {
        return Err(IntelError::Invalid(
            "source snapshot identity is immutable; use a new source_version for changed metadata"
                .to_string(),
        ));
    }
    tx.execute(
        "INSERT INTO knowledge_entity_source_assessments (source_ref_id)
         VALUES ($1) ON CONFLICT (source_ref_id) DO NOTHING",
        &[&source_ref_id],
    )?;

    Ok(KnowledgeEntityRefRecord {
        entity_ref_id: KnowledgeEntityRefId::try_new(entity_ref_id)?,
        source_ref_id,
        entity_type: input.entity_type,
        canonical_key: input.canonical_key.clone(),
    })
}

fn validate_entity_ref_input(input: &EntityRefInput) -> IntelResult<()> {
    for (field, value) in [
        ("canonical_key", input.canonical_key.as_str()),
        ("source_system", input.source.source_system.as_str()),
        ("source_entity_id", input.source.source_entity_id.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(IntelError::Invalid(format!("{field} must be non-empty")));
        }
    }
    if input.source.source_version.trim().is_empty() && input.source.source_version.as_str() != "" {
        return Err(IntelError::Invalid(
            "source_version cannot contain only whitespace".to_string(),
        ));
    }
    if let Some(uri) = &input.source.canonical_uri {
        if uri.trim().is_empty() {
            return Err(IntelError::Invalid(
                "canonical_uri cannot be empty when present".to_string(),
            ));
        }
    }
    if let Some(commit) = &input.source.source_commit {
        if commit.trim().is_empty() {
            return Err(IntelError::Invalid(
                "source_commit cannot be empty when present".to_string(),
            ));
        }
    }
    if let Some(hash) = &input.source.content_sha256 {
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(IntelError::Invalid(
                "content_sha256 must be 64 lowercase hexadecimal characters".to_string(),
            ));
        }
    }
    Ok(())
}

/// Read the current source snapshot and its separate reviewer assessment.
pub fn get_entity_source_assessment(
    pool: &PgPool,
    source_ref_id: &str,
) -> IntelResult<EntitySourceAssessmentRecord> {
    let mut conn = pool.get()?;
    let row = conn.query_opt(
        "SELECT s.source_ref_id, s.source_system, s.source_entity_id, s.source_version,
                s.source_state, s.canonical_uri, s.content_sha256, s.observed_at,
                a.review_status, a.trust_class, a.license_expression, a.license_status
         FROM knowledge_entity_sources s
         JOIN knowledge_entity_source_assessments a ON a.source_ref_id=s.source_ref_id
         WHERE s.source_ref_id=$1",
        &[&source_ref_id],
    )?;
    let Some(row) = row else {
        return Err(IntelError::Invalid(format!(
            "knowledge source reference {source_ref_id} not found"
        )));
    };
    let observed_at: SystemTime = row.get(7);
    let parse_error = |error: domain::error::DomainError| IntelError::Invalid(error.to_string());
    Ok(EntitySourceAssessmentRecord {
        source_ref_id: row.get(0),
        source_system: row.get(1),
        source_entity_id: row.get(2),
        source_version: row.get(3),
        source_state: row.get(4),
        canonical_uri: row.get(5),
        content_sha256: row.get(6),
        observed_at: crate::document_repo::timestamp_to_string(observed_at),
        review_status: EntitySourceReviewStatus::parse(&row.get::<_, String>(8))
            .map_err(parse_error)?,
        trust_class: SourceTrustClass::parse(&row.get::<_, String>(9)).map_err(parse_error)?,
        license_expression: row.get(10),
        license_status: SourceLicenseStatus::parse(&row.get::<_, String>(11))
            .map_err(parse_error)?,
    })
}

/// Read append-only source assessment history in decision order.
pub fn entity_source_review_history(
    pool: &PgPool,
    source_ref_id: &str,
) -> IntelResult<Vec<EntitySourceReviewEvent>> {
    let mut conn = pool.get()?;
    let rows = conn.query(
        "SELECT from_status, to_status, from_trust_class, to_trust_class,
                from_license_expression, to_license_expression, from_license_status,
                to_license_status, reason, reviewer, created_at
         FROM knowledge_entity_source_review_log
         WHERE source_ref_id=$1 ORDER BY created_at, review_id",
        &[&source_ref_id],
    )?;
    rows.into_iter()
        .map(|row| {
            let created_at: SystemTime = row.get(10);
            let parse_error =
                |error: domain::error::DomainError| IntelError::Invalid(error.to_string());
            Ok(EntitySourceReviewEvent {
                from_status: EntitySourceReviewStatus::parse(&row.get::<_, String>(0))
                    .map_err(parse_error)?,
                to_status: EntitySourceReviewStatus::parse(&row.get::<_, String>(1))
                    .map_err(parse_error)?,
                from_trust_class: SourceTrustClass::parse(&row.get::<_, String>(2))
                    .map_err(parse_error)?,
                to_trust_class: SourceTrustClass::parse(&row.get::<_, String>(3))
                    .map_err(parse_error)?,
                from_license_expression: row.get(4),
                to_license_expression: row.get(5),
                from_license_status: SourceLicenseStatus::parse(&row.get::<_, String>(6))
                    .map_err(parse_error)?,
                to_license_status: SourceLicenseStatus::parse(&row.get::<_, String>(7))
                    .map_err(parse_error)?,
                reason: row.get(8),
                reviewer: row.get(9),
                created_at: crate::document_repo::timestamp_to_string(created_at),
            })
        })
        .collect()
}

/// Add a relation between registered entity references. New relations are always
/// candidates; evidence linkage and an explicit review are separate operations.
pub fn add_entity_relation(
    pool: &PgPool,
    src: &KnowledgeEntityRefId,
    dst: &KnowledgeEntityRefId,
    relation_type: RelationType,
    evidence: Option<&Value>,
    source: &str,
    confidence: f32,
) -> IntelResult<(KnowledgeRelationId, bool)> {
    add_entity_relation_with_validity(
        pool,
        src,
        dst,
        RelationAssertion {
            relation_type,
            evidence,
            source,
            confidence,
            valid_from: None,
            valid_to: None,
        },
    )
}

pub fn add_entity_relation_with_validity(
    pool: &PgPool,
    src: &KnowledgeEntityRefId,
    dst: &KnowledgeEntityRefId,
    relation: RelationAssertion<'_>,
) -> IntelResult<(KnowledgeRelationId, bool)> {
    let mut conn = pool.get()?;
    let mut tx = conn.transaction()?;
    let result = add_relation_tx(&mut tx, src.as_str(), dst.as_str(), relation)?;
    tx.commit()?;
    Ok(result)
}

pub(crate) fn add_relation_tx(
    tx: &mut postgres::Transaction<'_>,
    src: &str,
    dst: &str,
    relation: RelationAssertion<'_>,
) -> IntelResult<(KnowledgeRelationId, bool)> {
    if src == dst {
        return Err(IntelError::Invalid(
            "a knowledge relation must connect distinct entity references".to_string(),
        ));
    }
    if !(0.0..=1.0).contains(&relation.confidence) {
        return Err(IntelError::Invalid(
            "confidence must be within [0,1]".to_string(),
        ));
    }
    if relation.source.trim().is_empty() {
        return Err(IntelError::Invalid(
            "relation source must be non-empty".to_string(),
        ));
    }
    if matches!(
        (relation.valid_from, relation.valid_to),
        (Some(start), Some(end)) if end < start
    ) {
        return Err(IntelError::Invalid(
            "relation valid_to must be greater than or equal to valid_from".to_string(),
        ));
    }

    let proposed_id = new_entity_id("krel-");
    let inserted = tx.query_opt(
        "INSERT INTO knowledge_relations (
             relation_id, src_id, dst_id, relation_type, relation_evidence,
             relation_source, confidence, review_status, valid_from, valid_to
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,'CANDIDATE',$8,$9)
         ON CONFLICT (src_id,dst_id,relation_type) DO NOTHING
         RETURNING relation_id",
        &[
            &proposed_id,
            &src,
            &dst,
            &relation.relation_type.as_str(),
            &relation.evidence.cloned().unwrap_or(Value::Null),
            &relation.source,
            &relation.confidence,
            &relation.valid_from,
            &relation.valid_to,
        ],
    )?;
    let (id, created) = match inserted {
        Some(row) => (row.get::<_, String>(0), true),
        None => (
            tx.query_one(
                "SELECT relation_id FROM knowledge_relations
                 WHERE src_id=$1 AND dst_id=$2 AND relation_type=$3",
                &[&src, &dst, &relation.relation_type.as_str()],
            )?
            .get(0),
            false,
        ),
    };
    Ok((KnowledgeRelationId::try_new(id)?, created))
}

/// Attach one immutable, content-addressed Evidence source snapshot to a relation.
pub fn link_relation_evidence(
    pool: &PgPool,
    relation_id: &KnowledgeRelationId,
    evidence_ref_id: &KnowledgeEntityRefId,
    evidence_source_ref_id: &str,
    role: RelationEvidenceRole,
) -> IntelResult<bool> {
    if evidence_source_ref_id.trim().is_empty() {
        return Err(IntelError::Invalid(
            "evidence_source_ref_id must be non-empty".to_string(),
        ));
    }
    let mut conn = pool.get()?;
    let mut tx = conn.transaction()?;
    let source = tx.query_opt(
        "SELECT s.content_sha256, s.canonical_uri, s.source_state
         FROM knowledge_entity_sources s
         JOIN knowledge_entity_refs e ON e.entity_ref_id=s.entity_ref_id
         WHERE s.source_ref_id=$1 AND e.entity_ref_id=$2 AND e.entity_type='evidence'",
        &[&evidence_source_ref_id, &evidence_ref_id.as_str()],
    )?;
    let Some(source) = source else {
        return Err(IntelError::Invalid(
            "evidence source reference is missing or is not an evidence entity".to_string(),
        ));
    };
    let hash: Option<String> = source.get(0);
    let uri: Option<String> = source.get(1);
    let state: String = source.get(2);
    if hash.is_none() || uri.as_deref().is_none_or(str::is_empty) || state != "CURRENT" {
        return Err(IntelError::Invalid(
            "relation evidence requires a current source snapshot with URI and SHA-256".to_string(),
        ));
    }
    let inserted = tx.execute(
        "INSERT INTO knowledge_relation_evidence (
             relation_id, evidence_entity_ref_id, evidence_source_ref_id, evidence_role
         ) VALUES ($1,$2,$3,$4) ON CONFLICT DO NOTHING",
        &[
            &relation_id.as_str(),
            &evidence_ref_id.as_str(),
            &evidence_source_ref_id,
            &role.as_str(),
        ],
    )?;
    tx.commit()?;
    Ok(inserted == 1)
}

/// Record a reviewed source assessment without changing the immutable snapshot.
pub fn review_entity_source(
    pool: &PgPool,
    source_ref_id: &str,
    to_status: EntitySourceReviewStatus,
    assessment: &EntitySourceAssessmentInput,
    reason: &str,
    reviewer: &str,
) -> IntelResult<()> {
    if source_ref_id.trim().is_empty()
        || reason.trim().is_empty()
        || reviewer.trim().is_empty()
        || assessment.license_expression.trim().is_empty()
    {
        return Err(IntelError::Invalid(
            "source review requires a source reference, license expression, reason and reviewer"
                .to_string(),
        ));
    }
    if to_status == EntitySourceReviewStatus::Approved
        && assessment.trust_class == SourceTrustClass::Unassessed
    {
        return Err(IntelError::Invalid(
            "source approval requires an assessed trust class".to_string(),
        ));
    }

    let mut conn = pool.get()?;
    let mut tx = conn.transaction()?;
    let row = tx.query_opt(
        "SELECT s.source_state, s.canonical_uri, s.content_sha256, e.entity_type,
                a.review_status, a.trust_class, a.license_expression, a.license_status
         FROM knowledge_entity_sources s
         JOIN knowledge_entity_refs e ON e.entity_ref_id=s.entity_ref_id
         JOIN knowledge_entity_source_assessments a ON a.source_ref_id=s.source_ref_id
         WHERE s.source_ref_id=$1 FOR UPDATE OF a",
        &[&source_ref_id],
    )?;
    let Some(row) = row else {
        return Err(IntelError::Invalid(format!(
            "knowledge source reference {source_ref_id} not found"
        )));
    };
    let source_state: String = row.get(0);
    let canonical_uri: Option<String> = row.get(1);
    let content_sha256: Option<String> = row.get(2);
    let entity_type: String = row.get(3);
    let from_status: String = row.get(4);
    let from_trust_class: String = row.get(5);
    let from_license_expression: String = row.get(6);
    let from_license_status: String = row.get(7);

    if to_status == EntitySourceReviewStatus::Approved {
        if source_state != "CURRENT" {
            return Err(IntelError::Invalid(
                "source approval requires a current source snapshot".to_string(),
            ));
        }
        if entity_type == KnowledgeEntityType::Evidence.as_str()
            && (canonical_uri.as_deref().is_none_or(str::is_empty) || content_sha256.is_none())
        {
            return Err(IntelError::Invalid(
                "Evidence source approval requires a URI and SHA-256".to_string(),
            ));
        }
    }

    tx.execute(
        "UPDATE knowledge_entity_source_assessments
         SET review_status=$2, trust_class=$3, license_expression=$4,
             license_status=$5, updated_at=now()
         WHERE source_ref_id=$1",
        &[
            &source_ref_id,
            &to_status.as_str(),
            &assessment.trust_class.as_str(),
            &assessment.license_expression,
            &assessment.license_status.as_str(),
        ],
    )?;
    tx.execute(
        "INSERT INTO knowledge_entity_source_review_log (
             source_ref_id, from_status, to_status, from_trust_class, to_trust_class,
             from_license_expression, to_license_expression, from_license_status,
             to_license_status, reason, reviewer
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        &[
            &source_ref_id,
            &from_status,
            &to_status.as_str(),
            &from_trust_class,
            &assessment.trust_class.as_str(),
            &from_license_expression,
            &assessment.license_expression,
            &from_license_status,
            &assessment.license_status.as_str(),
            &reason,
            &reviewer,
        ],
    )?;

    if from_status == EntitySourceReviewStatus::Approved.as_str()
        && to_status != EntitySourceReviewStatus::Approved
        && entity_type == KnowledgeEntityType::Evidence.as_str()
    {
        let next_relation_status = match to_status {
            EntitySourceReviewStatus::Rejected => RelationReviewStatus::Rejected,
            EntitySourceReviewStatus::Deprecated => RelationReviewStatus::Deprecated,
            _ => RelationReviewStatus::Candidate,
        };
        let relations = tx.query(
            "SELECT r.relation_id, r.review_status
             FROM knowledge_relations r
             JOIN knowledge_relation_evidence re ON re.relation_id=r.relation_id
             WHERE re.evidence_source_ref_id=$1 AND r.review_status='APPROVED'
             FOR UPDATE OF r",
            &[&source_ref_id],
        )?;
        for relation in relations {
            let relation_id: String = relation.get(0);
            let relation_from_status: String = relation.get(1);
            tx.execute(
                "UPDATE knowledge_relations SET review_status=$2 WHERE relation_id=$1",
                &[&relation_id, &next_relation_status.as_str()],
            )?;
            tx.execute(
                "INSERT INTO knowledge_relation_review_log (
                     relation_id, from_status, to_status, reason, reviewer
                 ) VALUES ($1,$2,$3,$4,$5)",
                &[
                    &relation_id,
                    &relation_from_status,
                    &next_relation_status.as_str(),
                    &format!(
                        "Evidence source {source_ref_id} changed from APPROVED to {}: {reason}",
                        to_status.as_str()
                    ),
                    &reviewer,
                ],
            )?;
        }
    }

    tx.commit()?;
    Ok(())
}

/// Change a relation's review state and append an immutable reviewer decision.
pub fn decide_relation(
    pool: &PgPool,
    relation_id: &KnowledgeRelationId,
    to_status: RelationReviewStatus,
    reason: &str,
    reviewer: &str,
) -> IntelResult<()> {
    if reason.trim().is_empty() || reviewer.trim().is_empty() {
        return Err(IntelError::Invalid(
            "relation review requires a reason and reviewer".to_string(),
        ));
    }
    let mut conn = pool.get()?;
    let mut tx = conn.transaction()?;
    let row = tx.query_opt(
        "SELECT review_status, confidence FROM knowledge_relations
         WHERE relation_id=$1 FOR UPDATE",
        &[&relation_id.as_str()],
    )?;
    let Some(row) = row else {
        return Err(IntelError::Invalid(format!(
            "knowledge relation {} not found",
            relation_id.as_str()
        )));
    };
    let from_status: String = row.get(0);
    let confidence: Option<f32> = row.get(1);
    if to_status == RelationReviewStatus::Approved {
        if confidence.is_none() {
            return Err(IntelError::Invalid(
                "relation approval requires a confidence value".to_string(),
            ));
        }
        let evidence_count: i64 = tx
            .query_one(
                "SELECT COUNT(*) FROM knowledge_relation_evidence re
             JOIN knowledge_entity_sources s ON s.source_ref_id=re.evidence_source_ref_id
             JOIN knowledge_entity_source_assessments a ON a.source_ref_id=s.source_ref_id
             WHERE re.relation_id=$1 AND s.source_state='CURRENT'
               AND s.content_sha256 IS NOT NULL AND s.canonical_uri IS NOT NULL
               AND a.review_status='APPROVED'",
                &[&relation_id.as_str()],
            )?
            .get(0);
        if evidence_count == 0 {
            return Err(IntelError::Invalid(
                "relation approval requires at least one current, hashed Evidence source"
                    .to_string(),
            ));
        }
    }
    tx.execute(
        "UPDATE knowledge_relations SET review_status=$2 WHERE relation_id=$1",
        &[&relation_id.as_str(), &to_status.as_str()],
    )?;
    tx.execute(
        "INSERT INTO knowledge_relation_review_log (
             relation_id, from_status, to_status, reason, reviewer
         ) VALUES ($1,$2,$3,$4,$5)",
        &[
            &relation_id.as_str(),
            &from_status,
            &to_status.as_str(),
            &reason,
            &reviewer,
        ],
    )?;
    tx.commit()?;
    Ok(())
}
