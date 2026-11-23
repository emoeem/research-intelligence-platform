//! Phase 7 feedback and maintenance orchestration.
//!
//! Findings are imported only as report projections. RTIS remains the sole
//! vulnerability/POC fact producer; this module stores references and evidence
//! provenance, never POC bodies or exploit material. Promotion creates a
//! CANDIDATE KnowledgeCard and never approves it automatically.
use crate::{card_repo, new_entity_id, report_repo, IntelError, IntelResult, PgPool};
use domain::kcard::{
    KnowledgeCardId, KnowledgeType, ReviewStatus, SourceTrust, VerificationStatus,
};
use serde_json::{json, Value};

#[derive(Debug, Clone, serde::Serialize)]
pub struct FeedbackIngestResult {
    pub report_id: String,
    pub task_id: String,
    pub findings_seen: usize,
    pub candidates_created: usize,
    pub evidence_refs_recorded: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MaintenanceRunResult {
    pub run_id: String,
    pub findings_total: usize,
    pub errors: usize,
    pub warnings: usize,
    pub info: usize,
}

fn required_string<'a>(value: &'a Value, key: &str, at: &str) -> IntelResult<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| IntelError::Invalid(format!("{at}.{key} must be a non-empty string")))
}

fn evidence_refs(finding: &Value) -> IntelResult<Vec<Value>> {
    let refs = finding
        .get("evidence")
        .and_then(Value::as_array)
        .ok_or_else(|| IntelError::Invalid("finding.evidence must be an array".into()))?;
    for (i, e) in refs.iter().enumerate() {
        for key in [
            "evidence_id",
            "artifact_id",
            "tool_run_id",
            "trace_id",
            "uri",
            "content_hash",
        ] {
            required_string(e, key, &format!("finding.evidence[{i}]"))?;
        }
    }
    Ok(refs.clone())
}

fn candidate_content(finding: &Value) -> String {
    let mut parts = Vec::new();
    for key in [
        "title",
        "vuln_type",
        "principle",
        "preconditions",
        "impact",
        "remediation",
    ] {
        if let Some(v) = finding.get(key) {
            if let Some(s) = v.as_str() {
                if !s.trim().is_empty() {
                    parts.push(s.to_string());
                }
            } else if v.is_object() || v.is_array() {
                parts.push(v.to_string());
            }
        }
    }
    if let Some(scope) = finding.get("affected_scope").and_then(Value::as_array) {
        parts.extend(scope.iter().filter_map(Value::as_str).map(str::to_string));
    }
    parts.join("\n\n")
}

/// Ingest one stored Report IR into append-only task/finding observations and
/// the human review queue. Replaying the same report is idempotent.
pub fn ingest_report(
    pool: &PgPool,
    report_id: &str,
    workspace_id: &str,
) -> IntelResult<FeedbackIngestResult> {
    if workspace_id.trim().is_empty() {
        return Err(IntelError::Invalid("workspace_id must be non-empty".into()));
    }
    let report = report_repo::load_report(pool, report_id)?
        .ok_or_else(|| IntelError::Invalid(format!("report not found: {report_id}")))?;
    report_engine::validate_ir(&report.report_ir)
        .map_err(|e| IntelError::Invalid(format!("report_ir schema validation failed: {e}")))?;
    let findings = report
        .report_ir
        .get("findings")
        .and_then(Value::as_array)
        .ok_or_else(|| IntelError::Invalid("report_ir.findings must be an array".into()))?;
    let mut conn = pool.get()?;
    let mut tx = conn.transaction()?;
    let existing_workspace: Option<String> = tx
        .query_opt(
            "SELECT workspace_id FROM research_tasks WHERE task_id=$1 FOR UPDATE",
            &[&report.task_id],
        )?
        .map(|row| row.get(0));
    if existing_workspace
        .as_deref()
        .is_some_and(|existing| existing != workspace_id)
    {
        return Err(IntelError::Invalid(format!(
            "task {} already belongs to workspace {}",
            report.task_id,
            existing_workspace.as_deref().unwrap_or_default()
        )));
    }
    let task_upserted = tx.execute(
        "INSERT INTO research_tasks (task_id, workspace_id, latest_report_id, report_count)
         VALUES ($1,$2,$3,0) ON CONFLICT (task_id) DO UPDATE
         SET latest_report_id = EXCLUDED.latest_report_id, last_seen_at = now()
         WHERE research_tasks.workspace_id = EXCLUDED.workspace_id",
        &[&report.task_id, &workspace_id, &report.report_id],
    )?;
    if task_upserted != 1 {
        return Err(IntelError::Invalid(format!(
            "task {} belongs to a different workspace",
            report.task_id
        )));
    }
    let linked = tx.execute(
        "INSERT INTO research_task_reports (task_id, report_id) VALUES ($1,$2)
         ON CONFLICT (task_id, report_id) DO NOTHING",
        &[&report.task_id, &report.report_id],
    )?;
    if linked == 0 {
        tx.commit()?;
        return Ok(FeedbackIngestResult {
            report_id: report.report_id,
            task_id: report.task_id,
            findings_seen: 0,
            candidates_created: 0,
            evidence_refs_recorded: 0,
        });
    }
    tx.execute(
        "UPDATE research_tasks SET report_count = report_count + 1 WHERE task_id = $1",
        &[&report.task_id],
    )?;
    let mut candidates_created = 0;
    let mut evidence_recorded = 0;
    for (index, finding) in findings.iter().enumerate() {
        let at = format!("findings[{index}]");
        let finding_id = required_string(finding, "finding_id", &at)?;
        if !finding_id.starts_with("find-") {
            return Err(IntelError::Invalid(format!(
                "{at}.finding_id must start with find-"
            )));
        }
        let title = required_string(finding, "title", &at)?;
        let severity = required_string(finding, "severity", &at)?;
        let result_state = required_string(finding, "result_state", &at)?;
        let rtis_ref = finding.get("rtis_ref").and_then(Value::as_str);
        if let Some(uri) = rtis_ref {
            if !valid_rtis_ref(uri) {
                return Err(IntelError::Invalid(format!(
                    "{at}.rtis_ref is not a valid rtis://poc/<numeric-id> reference"
                )));
            }
        }
        let refs = evidence_refs(finding)?;
        let observation_inserted = tx.execute(
            "INSERT INTO finding_observations (report_id,task_id,finding_id,finding_snapshot,evidence_refs)
             VALUES ($1,$2,$3,$4,$5) ON CONFLICT (report_id,finding_id) DO NOTHING",
            &[&report.report_id, &report.task_id, &finding_id, finding, &json!(refs)],
        )?;
        if observation_inserted == 0 {
            continue;
        }
        tx.execute(
            "INSERT INTO findings (task_id,finding_id,rtis_ref,title,severity,result_state,finding_snapshot,evidence_refs)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
             ON CONFLICT (task_id,finding_id) DO UPDATE SET
               rtis_ref=EXCLUDED.rtis_ref,title=EXCLUDED.title,severity=EXCLUDED.severity,
               result_state=EXCLUDED.result_state,finding_snapshot=EXCLUDED.finding_snapshot,
               evidence_refs=EXCLUDED.evidence_refs,last_seen_at=now(),
               observation_count=findings.observation_count + 1",
            &[&report.task_id, &finding_id, &rtis_ref, &title, &severity, &result_state, finding, &json!(refs)],
        )?;
        // Only confirmed findings with explicit Evidence may enter knowledge review.
        if result_state != "confirmed" || refs.is_empty() {
            continue;
        }
        let content = candidate_content(finding);
        if content.trim().is_empty() {
            continue;
        }
        let hash = card_repo::normalized_hash(&content);
        let candidate_id = new_entity_id("kfc-");
        let existing = tx.query_opt(
            "SELECT candidate_id FROM knowledge_feedback_candidates WHERE report_id=$1 AND finding_id=$2",
            &[&report.report_id, &finding_id],
        )?;
        if existing.is_some() {
            continue;
        }
        let duplicate: Option<String> = tx
            .query_opt(
                "SELECT knowledge_id FROM knowledge_cards
             WHERE workspace_id=$1 AND normalized_hash=$2 AND review_status='APPROVED'
             ORDER BY updated_at DESC LIMIT 1",
                &[&workspace_id, &hash],
            )?
            .map(|r| r.get(0));
        let status = if duplicate.is_some() {
            "DUPLICATE_REVIEW"
        } else {
            "PENDING_REVIEW"
        };
        tx.execute(
            "INSERT INTO knowledge_feedback_candidates
             (candidate_id,report_id,task_id,finding_id,workspace_id,candidate_hash,candidate_snapshot,evidence_refs,review_status,duplicate_of)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
            &[&candidate_id,&report.report_id,&report.task_id,&finding_id,&workspace_id,&hash,finding,&json!(refs),&status,&duplicate],
        )?;
        candidates_created += 1;
        for e in &refs {
            let evidence_id = required_string(e, "evidence_id", &at)?;
            let artifact_id = required_string(e, "artifact_id", &at)?;
            let tool_run_id = required_string(e, "tool_run_id", &at)?;
            let trace_id = required_string(e, "trace_id", &at)?;
            let uri = required_string(e, "uri", &at)?;
            let content_hash = required_string(e, "content_hash", &at)?;
            if content_hash.len() != 64
                || !content_hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(IntelError::Invalid(format!(
                    "{at}.evidence content_hash must be 64 hex chars"
                )));
            }
            let provenance_id = new_entity_id("kprov-");
            let evidence_type = e.get("evidence_type").and_then(Value::as_str);
            let inserted = tx.execute(
                "INSERT INTO knowledge_evidence_provenance
                 (provenance_id,candidate_id,report_id,task_id,finding_id,evidence_id,artifact_id,tool_run_id,trace_id,evidence_uri,content_hash,evidence_type)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
                 ON CONFLICT (candidate_id,evidence_id,content_hash) DO NOTHING",
                &[&provenance_id,&candidate_id,&report.report_id,&report.task_id,&finding_id,&evidence_id,&artifact_id,&tool_run_id,&trace_id,&uri,&content_hash,&evidence_type],
            )?;
            evidence_recorded += inserted as usize;
        }
    }
    tx.commit()?;
    Ok(FeedbackIngestResult {
        report_id: report.report_id,
        task_id: report.task_id,
        findings_seen: findings.len(),
        candidates_created,
        evidence_refs_recorded: evidence_recorded,
    })
}

/// Explicit human action: turn a queued report finding into a CANDIDATE card.
/// This function never writes APPROVED; approval remains rp-kc review's guarded path.
pub fn promote_feedback_candidate(
    pool: &PgPool,
    candidate_id: &str,
    reviewer: &str,
    reason: &str,
) -> IntelResult<KnowledgeCardId> {
    if reviewer.trim().is_empty() || reason.trim().is_empty() {
        return Err(IntelError::Invalid(
            "reviewer and reason are required".into(),
        ));
    }
    let mut conn = pool.get()?;
    let mut tx = conn.transaction()?;
    let row = tx.query_opt(
        "SELECT workspace_id,candidate_snapshot,candidate_hash,review_status,duplicate_of,knowledge_id
         FROM knowledge_feedback_candidates WHERE candidate_id=$1 FOR UPDATE",
        &[&candidate_id],
    )?.ok_or_else(|| IntelError::Invalid(format!("feedback candidate not found: {candidate_id}")))?;
    let workspace: String = row.get(0);
    let snapshot: Value = row.get(1);
    let review_status: String = row.get(3);
    let duplicate_of: Option<String> = row.get(4);
    let existing: Option<String> = row.get(5);
    if let Some(id) = existing {
        tx.commit()?;
        return KnowledgeCardId::try_new(id).map_err(IntelError::from);
    }
    if review_status == "REJECTED" {
        return Err(IntelError::Invalid(
            "rejected feedback candidate cannot be promoted".into(),
        ));
    }
    if duplicate_of.is_some() {
        return Err(IntelError::Invalid("candidate matches an approved card; review/reuse the existing card instead of creating a duplicate".into()));
    }
    let title = required_string(&snapshot, "title", "candidate_snapshot")?.to_string();
    let content = candidate_content(&snapshot);
    if content.trim().is_empty() {
        return Err(IntelError::Invalid("candidate content is empty".into()));
    }
    let input = card_repo::CardInput {
        knowledge_type: KnowledgeType::VulnSignature,
        title,
        summary: snapshot
            .get("vuln_type")
            .and_then(Value::as_str)
            .unwrap_or("Research finding; review required")
            .to_string(),
        content,
        structured_content: Some(snapshot.clone()),
        confidence: 0.5,
        verification: VerificationStatus::Unverified,
        evidence_level: None,
        source_trust: SourceTrust::AgentDerived,
        ai_derived: true,
        quality_flags: vec![],
        workspace_id: workspace,
        donor_caller_id: None,
        donor_channel: None,
    };
    let card = card_repo::create_card(pool, &input)?;
    card_repo::decide(pool, &card, ReviewStatus::Candidate, reason, reviewer)?;
    tx.execute(
        "UPDATE knowledge_feedback_candidates SET review_status='PROMOTED',knowledge_id=$2,
         reviewer=$3,decision_reason=$4,updated_at=now() WHERE candidate_id=$1",
        &[&candidate_id, &card.as_str(), &reviewer, &reason],
    )?;
    tx.commit()?;
    Ok(card)
}

/// List feedback candidates by workspace and optional review state.
pub fn list_feedback_candidates(
    pool: &PgPool,
    workspace_id: &str,
    status: Option<&str>,
    limit: i64,
) -> IntelResult<Value> {
    if workspace_id.trim().is_empty() || !(1..=1000).contains(&limit) {
        return Err(IntelError::Invalid(
            "workspace_id required and limit must be 1..=1000".into(),
        ));
    }
    if status.is_some_and(|s| {
        !matches!(
            s,
            "PENDING_REVIEW" | "DUPLICATE_REVIEW" | "PROMOTED" | "REJECTED"
        )
    }) {
        return Err(IntelError::Invalid("unknown feedback review status".into()));
    }
    let mut conn = pool.get()?;
    let rows = if let Some(status) = status {
        conn.query(
            "SELECT candidate_id,report_id,task_id,finding_id,candidate_hash,review_status,
                    knowledge_id,duplicate_of,created_at::text
             FROM knowledge_feedback_candidates WHERE workspace_id=$1 AND review_status=$2
             ORDER BY created_at DESC LIMIT $3",
            &[&workspace_id, &status, &limit],
        )?
    } else {
        conn.query(
            "SELECT candidate_id,report_id,task_id,finding_id,candidate_hash,review_status,
                    knowledge_id,duplicate_of,created_at::text
             FROM knowledge_feedback_candidates WHERE workspace_id=$1
             ORDER BY created_at DESC LIMIT $2",
            &[&workspace_id, &limit],
        )?
    };
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "candidate_id":r.get::<_,String>(0),
                "report_id":r.get::<_,String>(1),
                "task_id":r.get::<_,String>(2),
                "finding_id":r.get::<_,String>(3),
                "candidate_hash":r.get::<_,String>(4),
                "review_status":r.get::<_,String>(5),
                "knowledge_id":r.get::<_,Option<String>>(6),
                "duplicate_of":r.get::<_,Option<String>>(7),
                "created_at":r.get::<_,String>(8),
            })
        })
        .collect();
    Ok(json!({"workspace_id":workspace_id,"items":items,"count":items.len()}))
}

/// Retrieve a candidate and all immutable Evidence provenance references.
pub fn get_feedback_candidate(pool: &PgPool, candidate_id: &str) -> IntelResult<Option<Value>> {
    let mut conn = pool.get()?;
    let row = conn.query_opt(
        "SELECT candidate_id,report_id,task_id,finding_id,workspace_id,candidate_hash,
                candidate_snapshot,evidence_refs,review_status,knowledge_id,duplicate_of,
                reviewer,decision_reason,created_at::text,updated_at::text
         FROM knowledge_feedback_candidates WHERE candidate_id=$1",
        &[&candidate_id],
    )?;
    let Some(row) = row else { return Ok(None) };
    let provenance = conn.query(
        "SELECT evidence_id,artifact_id,tool_run_id,trace_id,evidence_uri,content_hash,evidence_type,captured_at::text
         FROM knowledge_evidence_provenance WHERE candidate_id=$1 ORDER BY captured_at,evidence_id",
        &[&candidate_id],
    )?;
    let evidence: Vec<Value> = provenance
        .iter()
        .map(|r| {
            json!({
                "evidence_id":r.get::<_,String>(0),
                "artifact_id":r.get::<_,String>(1),
                "tool_run_id":r.get::<_,String>(2),
                "trace_id":r.get::<_,String>(3),
                "uri":r.get::<_,String>(4),
                "content_hash":r.get::<_,String>(5),
                "evidence_type":r.get::<_,Option<String>>(6),
                "captured_at":r.get::<_,String>(7),
            })
        })
        .collect();
    Ok(Some(json!({
        "candidate_id":row.get::<_,String>(0),
        "report_id":row.get::<_,String>(1),
        "task_id":row.get::<_,String>(2),
        "finding_id":row.get::<_,String>(3),
        "workspace_id":row.get::<_,String>(4),
        "candidate_hash":row.get::<_,String>(5),
        "candidate_snapshot":row.get::<_,Value>(6),
        "evidence_refs":row.get::<_,Value>(7),
        "review_status":row.get::<_,String>(8),
        "knowledge_id":row.get::<_,Option<String>>(9),
        "duplicate_of":row.get::<_,Option<String>>(10),
        "reviewer":row.get::<_,Option<String>>(11),
        "decision_reason":row.get::<_,Option<String>>(12),
        "created_at":row.get::<_,String>(13),
        "updated_at":row.get::<_,String>(14),
        "evidence_provenance":evidence,
    })))
}

/// Explicit rejection before promotion. Rejections are immutable review decisions.
pub fn reject_feedback_candidate(
    pool: &PgPool,
    candidate_id: &str,
    reviewer: &str,
    reason: &str,
) -> IntelResult<()> {
    if reviewer.trim().is_empty() || reason.trim().is_empty() {
        return Err(IntelError::Invalid(
            "reviewer and reason are required".into(),
        ));
    }
    let mut conn = pool.get()?;
    let mut tx = conn.transaction()?;
    let row = tx.query_opt(
        "SELECT review_status FROM knowledge_feedback_candidates WHERE candidate_id=$1 FOR UPDATE",
        &[&candidate_id],
    )?.ok_or_else(|| IntelError::Invalid(format!("feedback candidate not found: {candidate_id}")))?;
    let status: String = row.get(0);
    if status == "PROMOTED" {
        return Err(IntelError::Invalid(
            "promoted candidate must be rejected through KnowledgeCard review".into(),
        ));
    }
    if status != "REJECTED" {
        tx.execute(
            "UPDATE knowledge_feedback_candidates SET review_status='REJECTED',reviewer=$2,
             decision_reason=$3,updated_at=now() WHERE candidate_id=$1",
            &[&candidate_id, &reviewer, &reason],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Human-reviewed duplicate resolution: attach the finding provenance to an
/// existing approved card instead of creating a second copy.
pub fn reuse_duplicate_candidate(
    pool: &PgPool,
    candidate_id: &str,
    reviewer: &str,
    reason: &str,
) -> IntelResult<KnowledgeCardId> {
    if reviewer.trim().is_empty() || reason.trim().is_empty() {
        return Err(IntelError::Invalid(
            "reviewer and reason are required".into(),
        ));
    }
    let mut conn = pool.get()?;
    let mut tx = conn.transaction()?;
    let row = tx
        .query_opt(
            "SELECT duplicate_of,review_status,knowledge_id FROM knowledge_feedback_candidates
         WHERE candidate_id=$1 FOR UPDATE",
            &[&candidate_id],
        )?
        .ok_or_else(|| {
            IntelError::Invalid(format!("feedback candidate not found: {candidate_id}"))
        })?;
    if let Some(existing) = row.get::<_, Option<String>>(2) {
        tx.commit()?;
        return KnowledgeCardId::try_new(existing).map_err(IntelError::from);
    }
    let duplicate: Option<String> = row.get(0);
    let status: String = row.get(1);
    if status != "DUPLICATE_REVIEW" {
        return Err(IntelError::Invalid(
            "candidate is not in duplicate review".into(),
        ));
    }
    let existing =
        duplicate.ok_or_else(|| IntelError::Invalid("duplicate target is missing".into()))?;
    let approved: bool = tx.query_one(
        "SELECT EXISTS(SELECT 1 FROM knowledge_cards WHERE knowledge_id=$1 AND review_status='APPROVED')",
        &[&existing],
    )?.get(0);
    if !approved {
        return Err(IntelError::Invalid(
            "duplicate target is no longer APPROVED".into(),
        ));
    }
    tx.execute(
        "UPDATE knowledge_feedback_candidates SET review_status='PROMOTED',knowledge_id=$2,
         reviewer=$3,decision_reason=$4,updated_at=now() WHERE candidate_id=$1",
        &[&candidate_id, &existing, &reviewer, &reason],
    )?;
    tx.commit()?;
    KnowledgeCardId::try_new(existing).map_err(IntelError::from)
}

fn valid_rtis_ref(uri: &str) -> bool {
    uri.strip_prefix("rtis://poc/")
        .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
}

// SQL projection helper intentionally mirrors the persisted audit row columns.
#[allow(clippy::too_many_arguments)]
fn insert_maintenance_finding(
    tx: &mut postgres::Transaction<'_>,
    run_id: &str,
    workspace_id: &str,
    subject_type: &str,
    subject_id: &str,
    code: &str,
    severity: &str,
    detail: Value,
    counts: &mut [usize; 3],
) -> IntelResult<()> {
    tx.execute(
        "INSERT INTO maintenance_findings (finding_row_id,run_id,workspace_id,subject_type,subject_id,check_code,severity,detail)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        &[&new_entity_id("mfind-"),&run_id,&workspace_id,&subject_type,&subject_id,&code,&severity,&detail],
    )?;
    counts[match severity {
        "ERROR" => 2,
        "WARN" => 1,
        _ => 0,
    }] += 1;
    Ok(())
}

/// Run deterministic quality/maintenance checks and persist an auditable report.
/// It flags issues; it never deletes, merges, rewrites, or auto-approves knowledge.
pub fn run_maintenance_audit(
    pool: &PgPool,
    workspace_id: &str,
) -> IntelResult<MaintenanceRunResult> {
    if workspace_id.trim().is_empty() {
        return Err(IntelError::Invalid("workspace_id must be non-empty".into()));
    }
    let run_id = new_entity_id("mrun-");
    let mut conn = pool.get()?;
    let mut tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO maintenance_runs (run_id,workspace_id) VALUES ($1,$2)",
        &[&run_id, &workspace_id],
    )?;
    let mut counts = [0usize; 3];
    let stale_candidates = tx.query(
        "SELECT candidate_id, finding_id, created_at::text FROM knowledge_feedback_candidates
         WHERE workspace_id=$1 AND review_status IN ('PENDING_REVIEW','DUPLICATE_REVIEW')
         AND created_at < now() - interval '30 days' ORDER BY created_at",
        &[&workspace_id],
    )?;
    for r in stale_candidates {
        insert_maintenance_finding(
            &mut tx,
            &run_id,
            workspace_id,
            "finding",
            &r.get::<_, String>(0),
            "FEEDBACK_REVIEW_OVERDUE",
            "WARN",
            json!({"finding_id":r.get::<_,String>(1),"created_at":r.get::<_,String>(2),"age_days_gt":30}),
            &mut counts,
        )?;
    }
    let missing_provenance = tx.query(
        "SELECT c.candidate_id,jsonb_array_length(c.evidence_refs)::bigint
         FROM knowledge_feedback_candidates c
         LEFT JOIN knowledge_evidence_provenance p ON p.candidate_id=c.candidate_id
         WHERE c.workspace_id=$1 GROUP BY c.candidate_id
         HAVING COUNT(p.provenance_id)=0",
        &[&workspace_id],
    )?;
    for r in missing_provenance {
        insert_maintenance_finding(
            &mut tx,
            &run_id,
            workspace_id,
            "evidence",
            &r.get::<_, String>(0),
            "EVIDENCE_PROVENANCE_MISSING",
            "ERROR",
            json!({"declared_evidence_refs":r.get::<_,i64>(1)}),
            &mut counts,
        )?;
    }
    let broken_source = tx.query(
        "SELECT s.source_id,s.source_type,s.source_url FROM source_records s
         WHERE s.workspace_id=$1 AND s.source_type IN ('url','github') AND btrim(s.source_url)=''",
        &[&workspace_id],
    )?;
    for r in broken_source {
        insert_maintenance_finding(
            &mut tx,
            &run_id,
            workspace_id,
            "source_record",
            &r.get::<_, String>(0),
            "SOURCE_URL_MISSING",
            "ERROR",
            json!({"source_type":r.get::<_,String>(1),"source_url":r.get::<_,String>(2)}),
            &mut counts,
        )?;
    }
    let monitor_issues = tx.query(
        "SELECT m.source_id,m.monitor_state,m.last_error,m.http_status FROM source_monitor_state m
         JOIN source_records s ON s.source_id=m.source_id WHERE s.workspace_id=$1
         AND m.monitor_state IN ('CHANGED','MISSING','ERROR')",
        &[&workspace_id],
    )?;
    for r in monitor_issues {
        insert_maintenance_finding(
            &mut tx,
            &run_id,
            workspace_id,
            "source_record",
            &r.get::<_, String>(0),
            "UPSTREAM_REQUIRES_REVIEW",
            "WARN",
            json!({"state":r.get::<_,String>(1),"last_error":r.get::<_,Option<String>>(2),"http_status":r.get::<_,Option<i32>>(3)}),
            &mut counts,
        )?;
    }
    let duplicate_groups = tx.query(
        "SELECT normalized_hash,COUNT(*)::bigint FROM knowledge_cards
         WHERE workspace_id=$1 AND normalized_hash IS NOT NULL
         GROUP BY normalized_hash HAVING COUNT(*) > 1",
        &[&workspace_id],
    )?;
    for r in duplicate_groups {
        insert_maintenance_finding(
            &mut tx,
            &run_id,
            workspace_id,
            "knowledge_card",
            &r.get::<_, String>(0),
            "DUPLICATE_CONTENT_GROUP",
            "INFO",
            json!({"card_count":r.get::<_,i64>(1),"action":"review SAME_CONTENT relations; do not auto-merge"}),
            &mut counts,
        )?;
    }
    let total = counts.iter().sum::<usize>();
    let summary =
        json!({"findings_total":total,"info":counts[0],"warnings":counts[1],"errors":counts[2]});
    tx.execute(
        "UPDATE maintenance_runs SET finished_at=now(),status='COMPLETED',summary=$2 WHERE run_id=$1",
        &[&run_id,&summary],
    )?;
    tx.commit()?;
    Ok(MaintenanceRunResult {
        run_id,
        findings_total: total,
        errors: counts[2],
        warnings: counts[1],
        info: counts[0],
    })
}

/// Read a complete persisted maintenance report (run summary + individual findings).
pub fn load_maintenance_report(pool: &PgPool, run_id: &str) -> IntelResult<Option<Value>> {
    let mut conn = pool.get()?;
    let run = conn.query_opt(
        "SELECT run_id,workspace_id,started_at::text,finished_at::text,status,summary
         FROM maintenance_runs WHERE run_id=$1",
        &[&run_id],
    )?;
    let Some(run) = run else { return Ok(None) };
    let rows = conn.query(
        "SELECT finding_row_id,subject_type,subject_id,check_code,severity,detail,
                detected_at::text,resolved_at::text
         FROM maintenance_findings WHERE run_id=$1 ORDER BY severity DESC,detected_at,check_code",
        &[&run_id],
    )?;
    let findings: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "finding_row_id":r.get::<_,String>(0),
                "subject_type":r.get::<_,String>(1),
                "subject_id":r.get::<_,String>(2),
                "check_code":r.get::<_,String>(3),
                "severity":r.get::<_,String>(4),
                "detail":r.get::<_,Value>(5),
                "detected_at":r.get::<_,String>(6),
                "resolved_at":r.get::<_,Option<String>>(7),
            })
        })
        .collect();
    Ok(Some(json!({
        "run_id":run.get::<_,String>(0),
        "workspace_id":run.get::<_,String>(1),
        "started_at":run.get::<_,String>(2),
        "finished_at":run.get::<_,Option<String>>(3),
        "status":run.get::<_,String>(4),
        "summary":run.get::<_,Value>(5),
        "findings":findings,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtis_reference_is_read_only_numeric_and_fail_closed() {
        assert!(valid_rtis_ref("rtis://poc/42"));
        for bad in [
            "rtis://poc/",
            "rtis://poc/4x",
            "rtis-poc:42",
            "http://host/42",
            "rtis://poc/42/",
        ] {
            assert!(!valid_rtis_ref(bad), "accepted invalid RTIS ref: {bad}");
        }
    }

    #[test]
    fn finding_content_is_stable_and_ignores_unrelated_fields() {
        let a = json!({"title":"SQL injection","vuln_type":"sqli","principle":"unsafe query","debug":"a"});
        let b = json!({"title":"SQL injection","vuln_type":"sqli","principle":"unsafe query","debug":"b"});
        assert_eq!(candidate_content(&a), candidate_content(&b));
        assert_eq!(
            card_repo::normalized_hash(&candidate_content(&a)),
            card_repo::normalized_hash(&candidate_content(&b))
        );
    }

    #[test]
    fn evidence_refs_require_all_provenance_fields() {
        let good = json!({"evidence":[{"evidence_id":"re-1","artifact_id":"art-1","tool_run_id":"run-1","trace_id":"tr-1","uri":"ripartifact://a","content_hash":"a".repeat(64)}]});
        assert_eq!(evidence_refs(&good).unwrap().len(), 1);
        let bad = json!({"evidence":[{"evidence_id":"re-1"}]});
        assert!(evidence_refs(&bad).is_err());
    }
}
