//! Durable knowledge ingestion protocol for stored reports and parsed documents.
//!
//! The protocol stores source references and processing state, not duplicate raw
//! document bodies. Report findings are delegated to the existing feedback
//! pipeline and remain review candidates until a human decision.

use crate::{maintenance, report_repo, IntelError, IntelResult, PgPool};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

/// Current request contract version.
pub const CONTRACT_VERSION: &str = "1.0";
/// A PROCESSING lease older than this may be reclaimed after a process crash.
const PROCESSING_LEASE_MINUTES: i32 = 30;
const MAX_CANDIDATE_EVIDENCE_ITEMS: i64 = 100;
const MAX_NUCLEI_METADATA_BLOCK_BYTES: usize = 64 * 1024;
const MAX_EXTRACTOR_DIAGNOSTICS: usize = 100;

/// Source adapter discriminator. Additional adapters are added without changing
/// the job lifecycle or result envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IngestionSource {
    /// An already stored and schema-validated Research Report IR.
    ResearchReport {
        /// Stored report identifier.
        report_id: String,
    },
    /// A parsed document revision owned by the Document Domain.
    DocumentRevision {
        /// Stored document identifier.
        document_id: String,
        /// Parsed immutable revision identifier.
        revision_id: String,
    },
}

fn merge_candidate_metadata(existing: &mut Value, incoming: &Value) {
    let existing_metadata = existing
        .get("structured_metadata")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let incoming_metadata = incoming
        .get("structured_metadata")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if existing_metadata == incoming_metadata {
        return;
    }

    let existing_object = existing
        .as_object_mut()
        .expect("candidate payload is a JSON object");
    let variants = existing_object
        .entry("structured_metadata_variants")
        .or_insert_with(|| json!([existing_metadata]));
    let variants = variants
        .as_array_mut()
        .expect("metadata variants is a JSON array");
    if !variants.contains(&incoming_metadata) {
        if variants.len() < 8 {
            variants.push(incoming_metadata);
        } else {
            existing_object.insert(
                "structured_metadata_variants_truncated".to_string(),
                Value::Bool(true),
            );
        }
    }
    existing_object.insert(
        "structured_metadata_conflict".to_string(),
        Value::Bool(true),
    );
}

/// Versioned, idempotent ingestion request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeIngestionRequest {
    /// Request contract version.
    pub contract_version: String,
    /// Target workspace.
    pub workspace_id: String,
    /// Caller-provided idempotency key, unique within a workspace.
    pub idempotency_key: String,
    /// Source reference. Raw content remains in its source authority.
    pub source: IngestionSource,
    /// Requesting user or service identity.
    pub requester: String,
    /// Optional existing run correlation.
    pub run_id: Option<String>,
    /// Optional existing trace correlation.
    pub trace_id: Option<String>,
}

/// Durable job state and latest result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeIngestionStatus {
    /// Stable ingestion identifier.
    pub ingestion_id: String,
    /// Workspace owner.
    pub workspace_id: String,
    /// Source kind.
    pub source_kind: String,
    /// Source reference.
    pub source_ref: String,
    /// Current lifecycle status.
    pub status: String,
    /// Number of claimed processing attempts.
    pub attempt_count: i32,
    /// Result envelope, when processing has completed.
    pub result: Option<Value>,
    /// Failure code, when failed.
    pub error_code: Option<String>,
    /// Failure detail, when failed.
    pub error_message: Option<String>,
    /// Creation time as RFC3339 text.
    pub created_at: String,
    /// Last update time as RFC3339 text.
    pub updated_at: String,
}

fn validate_request(request: &KnowledgeIngestionRequest) -> IntelResult<String> {
    if request.contract_version != CONTRACT_VERSION {
        return Err(IntelError::Invalid(format!(
            "unsupported ingestion contract version: {}",
            request.contract_version
        )));
    }
    for (field, value) in [
        ("workspace_id", request.workspace_id.as_str()),
        ("idempotency_key", request.idempotency_key.as_str()),
        ("requester", request.requester.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(IntelError::Invalid(format!("{field} must be non-empty")));
        }
    }
    if request.workspace_id.len() > 200
        || request.idempotency_key.len() > 256
        || request.requester.len() > 200
    {
        return Err(IntelError::Invalid(
            "workspace_id, idempotency_key or requester exceeds its length limit".into(),
        ));
    }
    match &request.source {
        IngestionSource::ResearchReport { report_id } => {
            if !valid_source_id(report_id, "rep-") {
                return Err(IntelError::Invalid(
                    "research_report source requires a valid rep- report_id".into(),
                ));
            }
        }
        IngestionSource::DocumentRevision {
            document_id,
            revision_id,
        } => {
            if !valid_source_id(document_id, "doc-") || !valid_source_id(revision_id, "drev-") {
                return Err(IntelError::Invalid(
                    "document_revision source requires valid doc- and drev- identifiers".into(),
                ));
            }
        }
    }
    let serialized = serde_json::to_vec(request)
        .map_err(|e| IntelError::Invalid(format!("serialize ingestion request: {e}")))?;
    let digest = Sha256::digest(serialized);
    Ok(format!("{digest:x}"))
}

pub(crate) fn valid_source_id(value: &str, prefix: &str) -> bool {
    let suffix = value.strip_prefix(prefix).unwrap_or_default();
    !suffix.is_empty()
        && value.len() <= 200
        && suffix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// Submit or idempotently replay a stored-report ingestion request.
///
/// The request is persisted before processing. A repeated workspace/key with
/// different content is rejected; concurrent identical submissions share one
/// processing claim. Only confirmed findings with Evidence become candidates
/// through the existing `maintenance::ingest_report` gate.
pub fn submit_report(
    pool: &PgPool,
    request: &KnowledgeIngestionRequest,
) -> IntelResult<KnowledgeIngestionStatus> {
    let request_sha256 = validate_request(request)?;
    let (source_kind, report_id, revision_id) = match &request.source {
        IngestionSource::ResearchReport { report_id } => {
            if report_repo::load_report(pool, report_id)?.is_none() {
                return Err(IntelError::Invalid(format!(
                    "research report not found: {report_id}"
                )));
            }
            ("research_report", Some(report_id.as_str()), None)
        }
        IngestionSource::DocumentRevision {
            document_id,
            revision_id,
        } => {
            let mut conn = pool.get()?;
            let exists: bool = conn
                .query_one(
                    "SELECT EXISTS (
                    SELECT 1 FROM documents d
                    JOIN document_revisions r ON r.document_id=d.document_id
                    WHERE d.document_id=$1 AND r.revision_id=$2
                      AND d.workspace_id=$3
                )",
                    &[document_id, revision_id, &request.workspace_id],
                )?
                .get(0);
            if !exists {
                return Err(IntelError::Invalid(format!(
                    "document revision not found in workspace: {document_id}/{revision_id}"
                )));
            }
            ("document_revision", None, Some(revision_id.as_str()))
        }
    };
    let request_json = serde_json::to_value(request)
        .map_err(|e| IntelError::Invalid(format!("serialize ingestion request: {e}")))?;
    let ingestion_id = crate::new_entity_id("ing-");
    {
        let mut conn = pool.get()?;
        conn.execute(
            "INSERT INTO knowledge_ingestions
             (ingestion_id,contract_version,workspace_id,idempotency_key,request_sha256,
              source_kind,source_ref,source_document_revision_id,request_json,requester,run_id,trace_id)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
             ON CONFLICT (workspace_id,idempotency_key) DO NOTHING",
            &[
                &ingestion_id,
                &request.contract_version,
                &request.workspace_id,
                &request.idempotency_key,
                &request_sha256,
                &source_kind,
                &report_id,
                &revision_id,
                &request_json,
                &request.requester,
                &request.run_id,
                &request.trace_id,
            ],
        )?;
    }
    let mut status = load_by_idempotency(pool, &request.workspace_id, &request.idempotency_key)?
        .ok_or_else(|| IntelError::Invalid("ingestion job was not persisted".into()))?;
    let stored_hash: String = {
        let mut conn = pool.get()?;
        conn.query_one(
            "SELECT request_sha256 FROM knowledge_ingestions WHERE ingestion_id=$1",
            &[&status.ingestion_id],
        )?
        .get(0)
    };
    if stored_hash != request_sha256 {
        return Err(IntelError::Invalid(
            "idempotency_key was already used for a different ingestion request".into(),
        ));
    }
    if claim(pool, &status.ingestion_id)? {
        match process_claimed(pool, request, &status.ingestion_id) {
            Ok(()) => {
                status = get_status(pool, &request.workspace_id, &status.ingestion_id)?
                    .ok_or_else(|| IntelError::Invalid("completed ingestion disappeared".into()))?;
            }
            Err(error) => {
                mark_failed(pool, &status.ingestion_id, &error)?;
                return Err(error);
            }
        }
    }
    Ok(status)
}

/// Load an ingestion status only within the supplied workspace.
pub fn get_status(
    pool: &PgPool,
    workspace_id: &str,
    ingestion_id: &str,
) -> IntelResult<Option<KnowledgeIngestionStatus>> {
    let mut conn = pool.get()?;
    let row = conn.query_opt(
        "SELECT ingestion_id,workspace_id,source_kind,
                COALESCE(source_ref,source_document_revision_id),status,attempt_count,
                result_json,error_code,error_message,created_at::text,updated_at::text
         FROM knowledge_ingestions
         WHERE ingestion_id=$1 AND workspace_id=$2",
        &[&ingestion_id, &workspace_id],
    )?;
    Ok(row.map(|row| KnowledgeIngestionStatus {
        ingestion_id: row.get(0),
        workspace_id: row.get(1),
        source_kind: row.get(2),
        source_ref: row.get(3),
        status: row.get(4),
        attempt_count: row.get(5),
        result: row.get(6),
        error_code: row.get(7),
        error_message: row.get(8),
        created_at: row.get(9),
        updated_at: row.get(10),
    }))
}

/// Load one document-ingestion candidate and bounded, source-labeled evidence
/// snippets, scoped to both its ingestion and workspace.
pub fn get_candidate_evidence(
    pool: &PgPool,
    workspace_id: &str,
    ingestion_id: &str,
    candidate_id: &str,
) -> IntelResult<Option<Value>> {
    if workspace_id.trim().is_empty()
        || !valid_source_id(ingestion_id, "ing-")
        || !valid_source_id(candidate_id, "ingc-")
    {
        return Err(IntelError::Invalid(
            "workspace, ingestion_id or candidate_id is invalid".into(),
        ));
    }
    let mut conn = pool.get()?;
    let candidate = conn.query_opt(
        "SELECT c.candidate_id,c.ingestion_id,c.workspace_id,c.candidate_type,
                c.canonical_key,c.normalized_hash,c.status,c.duplicate_of,
                c.prior_candidate_id,c.existing_entity_ref_id,
                c.created_at::text,c.updated_at::text,c.review_item_id
         FROM knowledge_ingestion_candidates c
         JOIN knowledge_ingestions i ON i.ingestion_id=c.ingestion_id
         WHERE c.candidate_id=$1 AND c.ingestion_id=$2
           AND c.workspace_id=$3 AND i.workspace_id=$3",
        &[&candidate_id, &ingestion_id, &workspace_id],
    )?;
    let Some(candidate) = candidate else {
        return Ok(None);
    };
    let review_payload = match candidate.get::<_, Option<i64>>(12) {
        Some(item_id) => conn
            .query_opt(
                "SELECT payload FROM review_batch_item WHERE item_id=$1",
                &[&item_id],
            )?
            .map(|row| row.get::<_, Value>(0)),
        None => None,
    };
    let rows = conn.query(
        "SELECT e.evidence_id,e.document_id,e.revision_id,e.block_id,e.block_sha256,
                e.location_json,ce.char_start,ce.char_end,ce.matched_text,
                GREATEST(0,ce.char_start-96),
                substring(b.content FROM GREATEST(1,ce.char_start-96+1) FOR 240),
                count(*) OVER()
         FROM knowledge_ingestion_candidate_evidence ce
         JOIN knowledge_ingestion_evidence e USING(evidence_id)
         JOIN document_blocks b USING(block_id)
         WHERE ce.candidate_id=$1
         ORDER BY e.evidence_id,ce.char_start
         LIMIT $2",
        &[&candidate_id, &MAX_CANDIDATE_EVIDENCE_ITEMS],
    )?;
    let evidence: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "evidence_id": row.get::<_, String>(0),
                "document_id": row.get::<_, String>(1),
                "revision_id": row.get::<_, String>(2),
                "block_id": row.get::<_, String>(3),
                "block_sha256": row.get::<_, String>(4),
                "location": row.get::<_, Value>(5),
                "char_start": row.get::<_, i32>(6),
                "char_end": row.get::<_, i32>(7),
                "matched_text": row.get::<_, String>(8),
                "snippet_char_start": row.get::<_, i32>(9),
                "snippet": row.get::<_, String>(10),
                "source_text_untrusted": true
            })
        })
        .collect();
    let evidence_total = rows.first().map(|row| row.get::<_, i64>(11)).unwrap_or(0);
    let evidence_truncated = evidence_total > evidence.len() as i64;
    let similarity_suggestions = conn
        .query(
            "SELECT s.matched_candidate_id,s.cosine_similarity::double precision,
                    s.threshold::double precision,s.embedding_model_id,s.embedding_version,
                    c.canonical_key,c.status,rbi.payload->'structured_metadata',rbi.decision
             FROM knowledge_ingestion_candidate_similarity_suggestions s
             JOIN knowledge_ingestion_candidates c
               ON c.candidate_id=s.matched_candidate_id
              AND c.workspace_id=s.workspace_id
              AND c.candidate_type=s.candidate_type
             LEFT JOIN review_batch_item rbi ON rbi.item_id=c.review_item_id
             WHERE s.ingestion_id=$1 AND s.source_candidate_id=$2 AND s.workspace_id=$3
             ORDER BY s.cosine_similarity DESC,s.matched_candidate_id
             LIMIT 20",
            &[&ingestion_id, &candidate_id, &workspace_id],
        )?
        .iter()
        .map(|row| {
            json!({
                "matched_candidate_id": row.get::<_, String>(0),
                "cosine_similarity": row.get::<_, f64>(1),
                "threshold": row.get::<_, f64>(2),
                "embedding_model_id": row.get::<_, String>(3),
                "embedding_version": row.get::<_, String>(4),
                "canonical_key": row.get::<_, String>(5),
                "status": row.get::<_, String>(6),
                "structured_metadata": row.get::<_, Option<Value>>(7).unwrap_or_else(|| json!({})),
                "review_decision": row.get::<_, Option<String>>(8),
                "review_only": true
            })
        })
        .collect::<Vec<_>>();
    Ok(Some(json!({
        "candidate_id": candidate.get::<_, String>(0),
        "ingestion_id": candidate.get::<_, String>(1),
        "workspace_id": candidate.get::<_, String>(2),
        "candidate_type": candidate.get::<_, String>(3),
        "canonical_key": candidate.get::<_, String>(4),
        "normalized_hash": candidate.get::<_, String>(5),
        "status": candidate.get::<_, String>(6),
        "duplicate_of": candidate.get::<_, Option<String>>(7),
        "prior_candidate_id": candidate.get::<_, Option<String>>(8),
        "existing_entity_ref_id": candidate.get::<_, Option<String>>(9),
        "candidate_schema_version": review_payload.as_ref()
            .and_then(|payload| payload.get("candidate_schema_version"))
            .cloned()
            .unwrap_or(Value::Null),
        "structured_metadata": review_payload.as_ref()
            .and_then(|payload| payload.get("structured_metadata"))
            .cloned()
            .unwrap_or_else(|| json!({})),
        "structured_metadata_untrusted": review_payload.as_ref()
            .and_then(|payload| payload.get("structured_metadata_untrusted"))
            .cloned()
            .unwrap_or(Value::Bool(true)),
        "structured_metadata_conflict": review_payload.as_ref()
            .and_then(|payload| payload.get("structured_metadata_conflict"))
            .cloned()
            .unwrap_or(Value::Bool(false)),
        "structured_metadata_variants": review_payload.as_ref()
            .and_then(|payload| payload.get("structured_metadata_variants"))
            .cloned()
            .unwrap_or_else(|| json!([])),
        "structured_metadata_variants_truncated": review_payload.as_ref()
            .and_then(|payload| payload.get("structured_metadata_variants_truncated"))
            .cloned()
            .unwrap_or(Value::Bool(false)),
        "assertion": review_payload.as_ref()
            .and_then(|payload| payload.get("assertion"))
            .cloned()
            .unwrap_or_else(|| json!("identifier mention only; not a verified vulnerability or weakness claim")),
        "evidence": evidence,
        "evidence_count": evidence_total,
        "evidence_truncated": evidence_truncated,
        "similarity_suggestions": similarity_suggestions,
        "similarity_suggestions_are_review_only": true,
        "created_at": candidate.get::<_, String>(10),
        "updated_at": candidate.get::<_, String>(11)
    })))
}

/// Retry a failed ingestion or reclaim a PROCESSING job whose lease expired.
pub fn retry_ingestion(
    pool: &PgPool,
    workspace_id: &str,
    ingestion_id: &str,
) -> IntelResult<KnowledgeIngestionStatus> {
    let request_json = {
        let mut conn = pool.get()?;
        conn.query_opt(
            "SELECT request_json FROM knowledge_ingestions
             WHERE ingestion_id=$1 AND workspace_id=$2",
            &[&ingestion_id, &workspace_id],
        )?
        .map(|row| row.get::<_, Value>(0))
    }
    .ok_or_else(|| IntelError::Invalid("ingestion not found in workspace".into()))?;
    let request: KnowledgeIngestionRequest = serde_json::from_value(request_json)
        .map_err(|e| IntelError::Invalid(format!("stored ingestion request is invalid: {e}")))?;
    submit_report(pool, &request)
}

fn load_by_idempotency(
    pool: &PgPool,
    workspace_id: &str,
    idempotency_key: &str,
) -> IntelResult<Option<KnowledgeIngestionStatus>> {
    let mut conn = pool.get()?;
    let row = conn.query_opt(
        "SELECT ingestion_id FROM knowledge_ingestions
         WHERE workspace_id=$1 AND idempotency_key=$2",
        &[&workspace_id, &idempotency_key],
    )?;
    match row {
        Some(row) => get_status(pool, workspace_id, &row.get::<_, String>(0)),
        None => Ok(None),
    }
}

fn claim(pool: &PgPool, ingestion_id: &str) -> IntelResult<bool> {
    let mut conn = pool.get()?;
    let changed = conn.execute(
        "UPDATE knowledge_ingestions
         SET status='PROCESSING',attempt_count=attempt_count+1,started_at=now(),
             updated_at=now(),completed_at=NULL,error_code=NULL,error_message=NULL
         WHERE ingestion_id=$1 AND (
             status IN ('PENDING','FAILED')
             OR (status='PROCESSING'
                 AND started_at < now() - make_interval(mins => $2))
         )",
        &[&ingestion_id, &PROCESSING_LEASE_MINUTES],
    )?;
    Ok(changed == 1)
}

fn process_claimed(
    pool: &PgPool,
    request: &KnowledgeIngestionRequest,
    ingestion_id: &str,
) -> IntelResult<()> {
    match &request.source {
        IngestionSource::ResearchReport { report_id } => {
            process_report(pool, request, ingestion_id, report_id)
        }
        IngestionSource::DocumentRevision {
            document_id,
            revision_id,
        } => process_document_revision(pool, request, ingestion_id, document_id, revision_id),
    }
}

fn process_report(
    pool: &PgPool,
    request: &KnowledgeIngestionRequest,
    ingestion_id: &str,
    report_id: &str,
) -> IntelResult<()> {
    let result = maintenance::ingest_report(pool, report_id, &request.workspace_id)?;
    let mut conn = pool.get()?;
    let mut tx = conn.transaction()?;
    let task_id: String = tx
        .query_one(
            "SELECT task_id FROM reports WHERE report_id=$1",
            &[&report_id],
        )?
        .get(0);
    let finding_rows = tx.query(
        "SELECT finding_id FROM finding_observations
         WHERE report_id=$1 AND task_id=$2 ORDER BY finding_id",
        &[&report_id, &task_id],
    )?;
    let candidate_rows = tx.query(
        "SELECT candidate_id,review_status FROM knowledge_feedback_candidates
         WHERE report_id=$1 AND workspace_id=$2 ORDER BY candidate_id",
        &[&report_id, &request.workspace_id],
    )?;
    for row in &finding_rows {
        tx.execute(
            "INSERT INTO knowledge_ingestion_outputs (ingestion_id,output_kind,output_id)
             VALUES ($1,'finding',$2) ON CONFLICT DO NOTHING",
            &[&ingestion_id, &row.get::<_, String>(0)],
        )?;
    }
    for row in &candidate_rows {
        tx.execute(
            "INSERT INTO knowledge_ingestion_outputs (ingestion_id,output_kind,output_id)
             VALUES ($1,'knowledge_candidate',$2) ON CONFLICT DO NOTHING",
            &[&ingestion_id, &row.get::<_, String>(0)],
        )?;
    }
    let finding_ids: Vec<String> = finding_rows.iter().map(|r| r.get(0)).collect();
    let candidate_ids: Vec<String> = candidate_rows.iter().map(|r| r.get(0)).collect();
    let has_pending_review = candidate_rows.iter().any(|row| {
        matches!(
            row.get::<_, String>(1).as_str(),
            "PENDING_REVIEW" | "DUPLICATE_REVIEW"
        )
    });
    let result_json = json!({
        "ingestion_id": ingestion_id,
        "report_id": report_id,
        "task_id": result.task_id,
        "findings_seen": result.findings_seen,
        "candidates_created": result.candidates_created,
        "evidence_refs_recorded": result.evidence_refs_recorded,
        "finding_ids": finding_ids,
        "candidate_ids": candidate_ids,
        "published": false
    });
    let status = if has_pending_review {
        "REVIEW_PENDING"
    } else {
        "COMPLETED"
    };
    tx.execute(
        "UPDATE knowledge_ingestions
         SET status=$2,result_json=$3,updated_at=now(),completed_at=now()
         WHERE ingestion_id=$1 AND status='PROCESSING'",
        &[&ingestion_id, &status, &result_json],
    )?;
    tx.commit()?;
    Ok(())
}

#[derive(Debug)]
struct IdentifierMatch {
    candidate_type: &'static str,
    canonical_key: String,
    matched_text: String,
    char_start: usize,
    char_end: usize,
    candidate_schema_version: &'static str,
    structured_metadata: Value,
    extraction_method: &'static str,
    assertion: &'static str,
}

#[derive(Deserialize)]
struct NucleiTemplateMetadata {
    id: String,
    info: NucleiTemplateInfo,
}

#[derive(Deserialize)]
struct NucleiTemplateInfo {
    name: String,
    severity: String,
}

fn identifier_matches(content: &str) -> Vec<IdentifierMatch> {
    static CVE: OnceLock<regex::Regex> = OnceLock::new();
    static CWE: OnceLock<regex::Regex> = OnceLock::new();
    let cve = CVE.get_or_init(|| {
        regex::Regex::new(r"(?i)\bCVE-\d{4}-\d{4,32}\b").expect("static CVE regex")
    });
    let cwe =
        CWE.get_or_init(|| regex::Regex::new(r"(?i)\bCWE-\d{1,6}\b").expect("static CWE regex"));
    let mut matches = cve
        .find_iter(content)
        .map(|found| IdentifierMatch {
            candidate_type: "vulnerability_identifier",
            canonical_key: format!("cve:{}", found.as_str().to_ascii_uppercase()),
            matched_text: found.as_str().to_ascii_uppercase(),
            char_start: content[..found.start()].chars().count(),
            char_end: content[..found.end()].chars().count(),
            candidate_schema_version: "identifier-mention.v1",
            structured_metadata: json!({}),
            extraction_method: "exact_identifier_pattern",
            assertion: "identifier mention only; not a verified vulnerability or weakness claim",
        })
        .chain(cwe.find_iter(content).map(|found| IdentifierMatch {
            candidate_type: "weakness_identifier",
            canonical_key: format!("cwe:{}", found.as_str().to_ascii_uppercase()),
            matched_text: found.as_str().to_ascii_uppercase(),
            char_start: content[..found.start()].chars().count(),
            char_end: content[..found.end()].chars().count(),
            candidate_schema_version: "identifier-mention.v1",
            structured_metadata: json!({}),
            extraction_method: "exact_identifier_pattern",
            assertion: "identifier mention only; not a verified vulnerability or weakness claim",
        }))
        .collect::<Vec<_>>();
    matches.sort_by_key(|matched| (matched.char_start, matched.char_end));
    matches
}

/// Extracts only bounded Nuclei template identity metadata. Request blocks,
/// payloads, and other template content are neither retained nor executed.
fn nuclei_template_match(content: &str) -> Result<Option<IdentifierMatch>, &'static str> {
    static ID_LINE: OnceLock<regex::Regex> = OnceLock::new();
    let id_line = ID_LINE.get_or_init(|| {
        regex::Regex::new(r#"(?m)^id:[ \t]*['"]?([A-Za-z0-9._/-]{1,100})['"]?[ \t]*(?:#.*)?$"#)
            .expect("static Nuclei id regex")
    });
    let Some(captures) = id_line.captures(content) else {
        return Ok(None);
    };
    if content.len() > MAX_NUCLEI_METADATA_BLOCK_BYTES {
        return Err("nuclei_block_too_large");
    }
    let Some(full_match) = captures.get(0) else {
        return Err("nuclei_id_missing");
    };
    let Some(id_match) = captures.get(1) else {
        return Err("nuclei_id_invalid");
    };
    let template: NucleiTemplateMetadata = serde_yaml_ng::from_str(&content[full_match.start()..])
        .map_err(|_| "nuclei_yaml_invalid")?;
    let id = id_match.as_str();
    if template.id != id
        || id.split('/').any(|part| matches!(part, "." | ".."))
        || template.info.name.trim().is_empty()
        || template.info.name.len() > 512
        || template.info.name.chars().any(char::is_control)
    {
        return Err("nuclei_metadata_invalid");
    }
    let severity = template.info.severity.to_ascii_lowercase();
    if !matches!(
        severity.as_str(),
        "info" | "low" | "medium" | "high" | "critical"
    ) {
        return Err("nuclei_severity_unsupported");
    }
    let char_start = content[..id_match.start()].chars().count();
    let char_end = content[..id_match.end()].chars().count();
    Ok(Some(IdentifierMatch {
        candidate_type: "nuclei_template",
        canonical_key: format!("nuclei:{id}"),
        matched_text: id.to_string(),
        char_start,
        char_end,
        candidate_schema_version: "nuclei-template-metadata.v1",
        structured_metadata: json!({
            "template_id": id,
            "name": template.info.name.trim(),
            "severity": severity
        }),
        extraction_method: "nuclei_yaml_metadata_v1",
        assertion: "static template metadata only; not a verified vulnerability or weakness claim",
    }))
}

fn block_hash_counts(contents: impl IntoIterator<Item = String>) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for content in contents {
        if content.trim().is_empty() {
            continue;
        }
        let hash = format!("{:x}", Sha256::digest(content.as_bytes()));
        *counts.entry(hash).or_insert(0) += 1;
    }
    counts
}

fn block_hash_diff(
    previous: &BTreeMap<String, usize>,
    current: &BTreeMap<String, usize>,
) -> (Value, Value, usize) {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut unchanged = 0usize;
    for hash in previous
        .keys()
        .chain(current.keys())
        .collect::<BTreeSet<_>>()
    {
        let old_count = previous.get(hash).copied().unwrap_or_default();
        let new_count = current.get(hash).copied().unwrap_or_default();
        unchanged += old_count.min(new_count);
        if new_count > old_count {
            added.push(json!({"sha256": hash, "count": new_count - old_count}));
        }
        if old_count > new_count {
            removed.push(json!({"sha256": hash, "count": old_count - new_count}));
        }
    }
    (json!(added), json!(removed), unchanged)
}

fn process_document_revision(
    pool: &PgPool,
    request: &KnowledgeIngestionRequest,
    ingestion_id: &str,
    document_id: &str,
    revision_id: &str,
) -> IntelResult<()> {
    let mut conn = pool.get()?;
    let mut tx = conn.transaction()?;
    let revision = tx
        .query_opt(
            "SELECT r.parse_state,r.content_hash,r.parser_version,r.extraction_quality,
                    r.revision_number
         FROM document_revisions r
         JOIN documents d ON d.document_id=r.document_id
         WHERE r.revision_id=$1 AND r.document_id=$2 AND d.workspace_id=$3
         FOR UPDATE",
            &[&revision_id, &document_id, &request.workspace_id],
        )?
        .ok_or_else(|| IntelError::Invalid("document revision not found in workspace".into()))?;
    let parse_state: String = revision.get(0);
    if !matches!(parse_state.as_str(), "APPROVED" | "REVIEW_PENDING") {
        return Err(IntelError::Invalid(format!(
            "document revision must be parsed before ingestion (parse_state={parse_state})"
        )));
    }
    let content_hash: String = revision.get(1);
    let parser_version: Option<String> = revision.get(2);
    let extraction_quality: Option<Value> = revision.get(3);
    let risk_flags = extraction_quality
        .as_ref()
        .and_then(|quality| quality.get("flags"))
        .and_then(Value::as_array)
        .cloned();
    let risk_metadata = match &risk_flags {
        Some(flags) => json!({"assessment": "available", "flags": flags}),
        None => json!({"assessment": "unavailable"}),
    };
    let risk_present = risk_flags.as_ref().is_none_or(|flags| !flags.is_empty());
    let priority_band = if parse_state == "REVIEW_PENDING" || risk_present {
        "P1"
    } else {
        "P2"
    };
    let priority_rank = if priority_band == "P1" { 1 } else { 2 };
    let revision_number: i64 = revision.get(4);
    let previous_revision_id: Option<String> = tx
        .query_opt(
            "SELECT revision_id FROM document_revisions
             WHERE document_id=$1 AND revision_number<$2
             ORDER BY revision_number DESC LIMIT 1",
            &[&document_id, &revision_number],
        )?
        .map(|row| row.get(0));
    let batch_id = format!("ingestion-{ingestion_id}");
    let batch_hash = format!(
        "{:x}",
        Sha256::digest(format!("{ingestion_id}:{content_hash}:{parser_version:?}").as_bytes())
    );
    tx.execute(
        "INSERT INTO review_batch(batch_id,domain,source_file,source_sha256,item_count)
         VALUES ($1,'knowledge_ingestion',$2,$3,0)
         ON CONFLICT(batch_id) DO NOTHING",
        &[
            &batch_id,
            &format!("document:{document_id}:{revision_id}"),
            &batch_hash,
        ],
    )?;
    let blocks = tx.query(
        "SELECT block_id,block_type,order_index,content,page_number,location_metadata
         FROM document_blocks WHERE document_id=$1 AND revision_id=$2
         ORDER BY order_index,block_id",
        &[&document_id, &revision_id],
    )?;
    let current_hashes = block_hash_counts(blocks.iter().map(|row| row.get::<_, String>(3)));
    let previous_hashes = if let Some(previous_id) = &previous_revision_id {
        let previous_blocks = tx.query(
            "SELECT content FROM document_blocks WHERE revision_id=$1",
            &[previous_id],
        )?;
        block_hash_counts(previous_blocks.iter().map(|row| row.get::<_, String>(0)))
    } else {
        BTreeMap::new()
    };
    let (added_block_hashes, removed_block_hashes, unchanged_blocks) =
        block_hash_diff(&previous_hashes, &current_hashes);
    let unchanged_block_count = i32::try_from(unchanged_blocks).map_err(|_| {
        IntelError::Invalid("document block count exceeds PostgreSQL integer range".into())
    })?;
    tx.execute(
        "INSERT INTO knowledge_ingestion_revision_diffs
         (ingestion_id,document_id,revision_id,previous_revision_id,
          added_block_hashes,removed_block_hashes,unchanged_block_count)
         VALUES ($1,$2,$3,$4,$5,$6,$7)
         ON CONFLICT(ingestion_id) DO NOTHING",
        &[
            &ingestion_id,
            &document_id,
            &revision_id,
            &previous_revision_id,
            &added_block_hashes,
            &removed_block_hashes,
            &unchanged_block_count,
        ],
    )?;
    let mut candidate_payloads: Vec<(String, Value)> = Vec::new();
    let mut evidence_count = 0usize;
    let mut extractor_diagnostics = Vec::new();
    let mut extractor_diagnostics_truncated = 0usize;
    for block in &blocks {
        let block_id: String = block.get(0);
        let block_type: String = block.get(1);
        let order_index: i64 = block.get(2);
        let content: String = block.get(3);
        if content.trim().is_empty() {
            continue;
        }
        let block_sha256 = format!("{:x}", Sha256::digest(content.as_bytes()));
        let location_json = json!({
            "block_type": block_type,
            "order_index": order_index,
            "page_number": block.get::<_, Option<i32>>(4),
            "location_metadata": block.get::<_, Option<Value>>(5),
            "parser_version": parser_version,
            "parse_state": parse_state,
            "source_content_sha256": content_hash
        });
        let evidence_id = format!(
            "inge-{}",
            &format!(
                "{:x}",
                Sha256::digest(format!("{ingestion_id}:{block_id}:{block_sha256}").as_bytes())
            )[..32]
        );
        tx.execute(
            "INSERT INTO knowledge_ingestion_evidence
             (evidence_id,ingestion_id,document_id,revision_id,block_id,block_sha256,location_json)
             VALUES ($1,$2,$3,$4,$5,$6,$7)
             ON CONFLICT (ingestion_id,block_id,block_sha256) DO NOTHING",
            &[
                &evidence_id,
                &ingestion_id,
                &document_id,
                &revision_id,
                &block_id,
                &block_sha256,
                &location_json,
            ],
        )?;
        let evidence_id: String = tx
            .query_one(
                "SELECT evidence_id FROM knowledge_ingestion_evidence
             WHERE ingestion_id=$1 AND block_id=$2 AND block_sha256=$3",
                &[&ingestion_id, &block_id, &block_sha256],
            )?
            .get(0);
        evidence_count += 1;
        let mut matches = identifier_matches(&content);
        match nuclei_template_match(&content) {
            Ok(Some(template_match)) => {
                matches.push(template_match);
                matches.sort_by_key(|matched| (matched.char_start, matched.char_end));
            }
            Ok(None) => {}
            Err(code) => {
                if extractor_diagnostics.len() < MAX_EXTRACTOR_DIAGNOSTICS {
                    extractor_diagnostics.push(json!({
                        "block_id": block_id,
                        "extractor": "nuclei-template-metadata.v1",
                        "code": code,
                        "source_text_untrusted": true
                    }));
                } else {
                    extractor_diagnostics_truncated += 1;
                }
            }
        }
        for matched in matches {
            tx.query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
                &[&format!(
                    "{}:{}:{}",
                    request.workspace_id, matched.candidate_type, matched.canonical_key
                )],
            )?;
            let prior_candidates = tx.query(
                "SELECT c.candidate_id,r.document_id,r.revision_number,
                        EXISTS (
                            SELECT 1 FROM knowledge_ingestion_candidate_evidence ce
                            JOIN knowledge_ingestion_evidence e USING(evidence_id)
                            WHERE ce.candidate_id=c.candidate_id AND e.block_sha256=$4
                        ) AS same_block_hash
                 FROM knowledge_ingestion_candidates c
                 JOIN knowledge_ingestions old_i ON old_i.ingestion_id=c.ingestion_id
                 JOIN document_revisions r
                   ON r.revision_id=old_i.source_document_revision_id
                 WHERE c.workspace_id=$1 AND c.candidate_type=$2 AND c.canonical_key=$3
                   AND c.ingestion_id <> $5 AND c.status <> 'REJECTED'
                 ORDER BY (r.document_id=$6) DESC,r.revision_number DESC,c.created_at DESC
                 LIMIT 1",
                &[
                    &request.workspace_id,
                    &matched.candidate_type,
                    &matched.canonical_key,
                    &block_sha256,
                    &ingestion_id,
                    &document_id,
                ],
            )?;
            let prior = prior_candidates.first();
            let prior_candidate_id: Option<String> = prior.map(|row| row.get(0));
            let prior_is_same_document_revision = prior.is_some_and(|row| {
                row.get::<_, String>(1) == document_id && row.get::<_, i64>(2) < revision_number
            });
            let same_block_hash = prior.is_some_and(|row| row.get::<_, bool>(3));
            let is_version_change = prior_is_same_document_revision && !same_block_hash;
            let duplicate_of = if prior.is_some() && !is_version_change {
                prior_candidate_id.clone()
            } else {
                None
            };
            let entity_key = matched.canonical_key.clone();
            let existing_entity_ref_id: Option<String> =
                if matched.candidate_type == "vulnerability_identifier" {
                    tx.query_opt(
                        "SELECT entity_ref_id FROM knowledge_entity_refs
                     WHERE entity_type='vulnerability' AND canonical_key=$1",
                        &[&entity_key],
                    )?
                    .map(|row| row.get(0))
                } else {
                    None
                };
            let candidate_id = crate::new_entity_id("ingc-");
            let normalized_hash = format!("{:x}", Sha256::digest(matched.canonical_key.as_bytes()));
            let candidate_status = if is_version_change {
                "VERSION_CHANGE_REVIEW"
            } else if duplicate_of.is_some() {
                "DUPLICATE_REVIEW"
            } else if existing_entity_ref_id.is_some() {
                "EXISTING_ENTITY"
            } else {
                "PENDING_REVIEW"
            };
            tx.execute(
                "INSERT INTO knowledge_ingestion_candidates
                 (candidate_id,ingestion_id,workspace_id,candidate_type,canonical_key,
                  normalized_hash,status,duplicate_of,prior_candidate_id,
                  existing_entity_ref_id,review_batch_id)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
                 ON CONFLICT (ingestion_id,candidate_type,canonical_key) DO NOTHING",
                &[
                    &candidate_id,
                    &ingestion_id,
                    &request.workspace_id,
                    &matched.candidate_type,
                    &matched.canonical_key,
                    &normalized_hash,
                    &candidate_status,
                    &duplicate_of,
                    &if is_version_change {
                        prior_candidate_id.clone()
                    } else {
                        None
                    },
                    &existing_entity_ref_id,
                    &batch_id,
                ],
            )?;
            let candidate_id: String = tx
                .query_one(
                    "SELECT candidate_id FROM knowledge_ingestion_candidates
                 WHERE ingestion_id=$1 AND candidate_type=$2 AND canonical_key=$3",
                    &[
                        &ingestion_id,
                        &matched.candidate_type,
                        &matched.canonical_key,
                    ],
                )?
                .get(0);
            let char_start = i32::try_from(matched.char_start).map_err(|_| {
                IntelError::Invalid(
                    "document identifier offset exceeds PostgreSQL integer range".into(),
                )
            })?;
            let char_end = i32::try_from(matched.char_end).map_err(|_| {
                IntelError::Invalid(
                    "document identifier offset exceeds PostgreSQL integer range".into(),
                )
            })?;
            tx.execute(
                "INSERT INTO knowledge_ingestion_candidate_evidence
                 (candidate_id,evidence_id,char_start,char_end,matched_text)
                 VALUES ($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING",
                &[
                    &candidate_id,
                    &evidence_id,
                    &char_start,
                    &char_end,
                    &matched.matched_text,
                ],
            )?;
            let payload = json!({
                "candidate_id": candidate_id,
                "candidate_type": matched.candidate_type,
                "canonical_key": matched.canonical_key,
                "candidate_status": candidate_status,
                "duplicate_of": duplicate_of,
                "prior_candidate_id": if is_version_change { prior_candidate_id } else { None },
                "change_type": if is_version_change {
                    "version_change"
                } else if duplicate_of.is_some() {
                    "exact_duplicate"
                } else if existing_entity_ref_id.is_some() {
                    "existing_entity"
                } else {
                    "new_mention"
                },
                "existing_entity_ref_id": existing_entity_ref_id,
                "evidence_id": evidence_id,
                "document_id": document_id,
                "revision_id": revision_id,
                "block_id": block_id,
                "char_start": matched.char_start,
                "char_end": matched.char_end,
                "matched_text": matched.matched_text,
                "candidate_schema_version": matched.candidate_schema_version,
                "structured_metadata": matched.structured_metadata,
                "structured_metadata_untrusted": true,
                "extraction_method": matched.extraction_method,
                "parser_version": parser_version,
                "source_content_sha256": content_hash,
                "extraction_quality": extraction_quality,
                "risk": risk_metadata,
                "assertion": matched.assertion
            });
            if let Some(index) = candidate_payloads
                .iter()
                .position(|(existing_id, _)| existing_id == &candidate_id)
            {
                merge_candidate_metadata(&mut candidate_payloads[index].1, &payload);
            } else {
                candidate_payloads.push((candidate_id, payload));
            }
        }
    }
    let mut candidate_ids = Vec::new();
    for (candidate_id, payload) in &candidate_payloads {
        tx.execute(
            "INSERT INTO review_batch_item
             (batch_id,candidate_id,priority_rank,priority_band,payload)
             VALUES ($1,$2,$3,$4,$5) ON CONFLICT(batch_id,candidate_id) DO NOTHING",
            &[
                &batch_id,
                candidate_id,
                &priority_rank,
                &priority_band,
                payload,
            ],
        )?;
        let item_id: i64 = tx
            .query_one(
                "SELECT item_id FROM review_batch_item WHERE batch_id=$1 AND candidate_id=$2",
                &[&batch_id, candidate_id],
            )?
            .get(0);
        tx.execute(
            "UPDATE knowledge_ingestion_candidates
             SET review_item_id=$2,updated_at=now() WHERE candidate_id=$1",
            &[candidate_id, &item_id],
        )?;
        tx.execute(
            "INSERT INTO knowledge_ingestion_outputs(ingestion_id,output_kind,output_id)
             VALUES ($1,'knowledge_candidate',$2) ON CONFLICT DO NOTHING",
            &[&ingestion_id, candidate_id],
        )?;
        candidate_ids.push(candidate_id.clone());
    }
    tx.execute(
        "UPDATE review_batch SET item_count=$2 WHERE batch_id=$1",
        &[&batch_id, &(candidate_ids.len() as i32)],
    )?;
    for row in &blocks {
        let block_id: String = row.get(0);
        let content: String = row.get(3);
        let block_sha256 = format!("{:x}", Sha256::digest(content.as_bytes()));
        if let Some(evidence_id) = tx
            .query_opt(
                "SELECT evidence_id FROM knowledge_ingestion_evidence
             WHERE ingestion_id=$1 AND block_id=$2 AND block_sha256=$3",
                &[&ingestion_id, &block_id, &block_sha256],
            )?
            .map(|row| row.get::<_, String>(0))
        {
            tx.execute(
                "INSERT INTO knowledge_ingestion_outputs(ingestion_id,output_kind,output_id)
                 VALUES ($1,'document_evidence',$2) ON CONFLICT DO NOTHING",
                &[&ingestion_id, &evidence_id],
            )?;
        }
    }
    tx.execute(
        "INSERT INTO knowledge_ingestion_outputs(ingestion_id,output_kind,output_id)
         VALUES ($1,'document_revision',$2) ON CONFLICT DO NOTHING",
        &[&ingestion_id, &revision_id],
    )?;
    let result_json = json!({
        "ingestion_id": ingestion_id,
        "document_id": document_id,
        "revision_id": revision_id,
        "parser_version": parser_version,
        "source_content_sha256": content_hash,
        "revision_diff": {
            "previous_revision_id": previous_revision_id,
            "added_block_hashes": added_block_hashes,
            "removed_block_hashes": removed_block_hashes,
            "unchanged_block_count": unchanged_block_count
        },
        "blocks_with_evidence": evidence_count,
        "extractor_diagnostics": extractor_diagnostics,
        "extractor_diagnostics_truncated": extractor_diagnostics_truncated,
        "candidates_created": candidate_ids.len(),
        "candidate_ids": candidate_ids,
        "review_batch_id": batch_id,
        "published": false
    });
    let status = if candidate_ids.is_empty() {
        "COMPLETED"
    } else {
        "REVIEW_PENDING"
    };
    tx.execute(
        "UPDATE knowledge_ingestions
         SET status=$2,result_json=$3,updated_at=now(),completed_at=now()
         WHERE ingestion_id=$1 AND status='PROCESSING'",
        &[&ingestion_id, &status, &result_json],
    )?;
    tx.commit()?;
    Ok(())
}

fn mark_failed(pool: &PgPool, ingestion_id: &str, error: &IntelError) -> IntelResult<()> {
    let mut conn = pool.get()?;
    conn.execute(
        "UPDATE knowledge_ingestions
         SET status='FAILED',error_code='ingestion_failed',error_message=$2,
             updated_at=now(),completed_at=now()
         WHERE ingestion_id=$1 AND status='PROCESSING'",
        &[&ingestion_id, &error.to_string()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(report_id: &str) -> KnowledgeIngestionRequest {
        KnowledgeIngestionRequest {
            contract_version: CONTRACT_VERSION.to_string(),
            workspace_id: "default".to_string(),
            idempotency_key: "test-key".to_string(),
            source: IngestionSource::ResearchReport {
                report_id: report_id.to_string(),
            },
            requester: "test".to_string(),
            run_id: None,
            trace_id: None,
        }
    }

    #[test]
    fn request_requires_versioned_source_and_idempotency() {
        assert!(validate_request(&request("rep-123")).is_ok());
        assert!(validate_request(&request("not-a-report")).is_err());
        let mut document_request = request("rep-123");
        document_request.source = IngestionSource::DocumentRevision {
            document_id: "doc-123".to_string(),
            revision_id: "drev-123".to_string(),
        };
        assert!(validate_request(&document_request).is_ok());
        document_request.source = IngestionSource::DocumentRevision {
            document_id: "not-a-document".to_string(),
            revision_id: "drev-123".to_string(),
        };
        assert!(validate_request(&document_request).is_err());
        let mut invalid = request("rep-123");
        invalid.idempotency_key.clear();
        assert!(validate_request(&invalid).is_err());
        let mut unsupported = request("rep-123");
        unsupported.contract_version = "2.0".to_string();
        assert!(validate_request(&unsupported).is_err());
    }

    #[test]
    fn request_hash_is_deterministic_and_sensitive_to_request_content() {
        let first = request("rep-123");
        let second = request("rep-123");
        assert_eq!(
            validate_request(&first).unwrap(),
            validate_request(&second).unwrap()
        );
        let mut changed = second;
        changed.requester = "different-requester".to_string();
        assert_ne!(
            validate_request(&first).unwrap(),
            validate_request(&changed).unwrap()
        );
    }

    #[test]
    fn identifier_extraction_is_exact_and_unicode_character_offset_based() {
        let content = "研究 CVE-2025-12345, cwe-79; not-CVE-2025-12345x";
        let matches = identifier_matches(content);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].canonical_key, "cve:CVE-2025-12345");
        assert_eq!(matches[0].matched_text, "CVE-2025-12345");
        assert_eq!(
            matches[0].char_start,
            content[..content.find("CVE-2025-12345").unwrap()]
                .chars()
                .count()
        );
        assert_eq!(matches[1].canonical_key, "cwe:CWE-79");
    }

    #[test]
    fn nuclei_extraction_returns_only_static_versioned_metadata() {
        let content = "研究记录提及 CVE-2099-12345678 和 CWE-79；重复提及 CVE-2099-12345678，这不是事实结论。\nid: community/test-template\ninfo:\n  name: Static metadata sample\n  severity: high\n  tags: sample";
        let matched = nuclei_template_match(content).unwrap().unwrap();
        assert_eq!(matched.candidate_type, "nuclei_template");
        assert_eq!(matched.canonical_key, "nuclei:community/test-template");
        assert_eq!(
            matched.candidate_schema_version,
            "nuclei-template-metadata.v1"
        );
        assert_eq!(
            matched.structured_metadata,
            json!({
                "template_id": "community/test-template",
                "name": "Static metadata sample",
                "severity": "high"
            })
        );
        assert_eq!(
            content
                .chars()
                .skip(matched.char_start)
                .take(matched.char_end - matched.char_start)
                .collect::<String>(),
            "community/test-template"
        );
        let case_variant = content.replace("community/test-template", "Community/Test-Template");
        assert_eq!(
            nuclei_template_match(&case_variant)
                .unwrap()
                .unwrap()
                .canonical_key,
            "nuclei:Community/Test-Template",
            "source template ID case is preserved"
        );
        let oversized = format!(
            "id: community/test-template\n{}",
            "x".repeat(MAX_NUCLEI_METADATA_BLOCK_BYTES)
        );
        assert_eq!(
            nuclei_template_match(&oversized).unwrap_err(),
            "nuclei_block_too_large"
        );
    }

    #[test]
    fn nuclei_extraction_rejects_unrecognized_severity() {
        let content =
            "id: community/test-template\ninfo:\n  name: Static metadata sample\n  severity: unknown";
        assert_eq!(
            nuclei_template_match(content).unwrap_err(),
            "nuclei_severity_unsupported"
        );
    }

    #[test]
    fn structured_metadata_conflicts_are_visible_and_bounded() {
        let mut existing = json!({"structured_metadata": {"name": "first"}});
        for index in 1..10 {
            merge_candidate_metadata(
                &mut existing,
                &json!({"structured_metadata": {"name": format!("variant-{index}")}}),
            );
        }
        assert_eq!(existing["structured_metadata_conflict"], true);
        assert_eq!(
            existing["structured_metadata_variants"]
                .as_array()
                .unwrap()
                .len(),
            8
        );
        assert_eq!(existing["structured_metadata_variants_truncated"], true);
    }

    #[test]
    fn block_hash_diff_keeps_duplicate_block_counts() {
        let previous = block_hash_counts(["same".to_string(), "removed".to_string()]);
        let current =
            block_hash_counts(["same".to_string(), "same".to_string(), "added".to_string()]);
        let (added, removed, unchanged) = block_hash_diff(&previous, &current);
        assert_eq!(unchanged, 1);
        assert_eq!(added.as_array().unwrap().len(), 2);
        assert_eq!(removed.as_array().unwrap().len(), 1);
    }
}
