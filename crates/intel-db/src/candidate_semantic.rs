//! Review-only similarity suggestions for bounded static ingestion metadata.

use crate::embed_worker::{EmbedWorkerConfig, EmbedWorkerProvider};
use crate::{IntelError, IntelResult, PgPool};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

const MAX_WORKSPACE_CANDIDATES: usize = 5_000;
const MAX_TOP_K: u32 = 50;
const EMBED_MODEL_ID: &str = "bge-small-zh-v1.5";
const EMBED_VERSION: &str = "0.2.0";

#[derive(Debug, Serialize)]
pub struct CandidateSemanticOutcome {
    pub ingestion_id: String,
    pub workspace_id: String,
    pub source_candidates: usize,
    pub eligible_source_candidates: usize,
    pub skipped_source_candidates: usize,
    pub workspace_candidates_indexed: usize,
    pub vectors_embedded: usize,
    pub suggestions_persisted: usize,
    pub threshold: f32,
    pub top_k: u32,
    pub embedding_model_id: &'static str,
    pub embedding_version: &'static str,
    pub review_only: bool,
}

#[derive(Debug)]
struct Candidate {
    candidate_id: String,
    ingestion_id: String,
    text: String,
    input_sha256: String,
}

fn candidate_text(payload: &Value, canonical_key: &str) -> Option<String> {
    if payload
        .get("structured_metadata_conflict")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || payload
            .get("candidate_schema_version")
            .and_then(Value::as_str)
            != Some("nuclei-template-metadata.v1")
        || payload
            .get("structured_metadata_untrusted")
            .and_then(Value::as_bool)
            != Some(true)
    {
        return None;
    }
    let metadata = payload.get("structured_metadata")?;
    let template_id = metadata.get("template_id")?.as_str()?;
    if canonical_key != format!("nuclei:{template_id}") {
        return None;
    }
    let name = metadata.get("name")?.as_str()?.trim();
    let severity = metadata
        .get("severity")?
        .as_str()?
        .trim()
        .to_ascii_lowercase();
    if name.is_empty()
        || name.len() > 4096
        || name.chars().any(char::is_control)
        || !matches!(
            severity.as_str(),
            "info" | "low" | "medium" | "high" | "critical"
        )
    {
        return None;
    }
    Some(format!(
        "Nuclei template name: {name}\nSeverity: {severity}"
    ))
}

fn load_workspace_candidates(pool: &PgPool, workspace_id: &str) -> IntelResult<Vec<Candidate>> {
    let mut conn = pool.get()?;
    let rows = conn.query(
        "SELECT c.candidate_id,c.ingestion_id,c.status,rbi.decision,rbi.payload,c.canonical_key
         FROM knowledge_ingestion_candidates c
         JOIN review_batch_item rbi ON rbi.item_id=c.review_item_id
         WHERE c.workspace_id=$1 AND c.candidate_type='nuclei_template'
           AND c.status NOT IN ('REJECTED','DEFERRED')
           AND rbi.decision NOT IN ('REJECTED','DEFERRED')
         ORDER BY c.candidate_id
         LIMIT $2",
        &[&workspace_id, &((MAX_WORKSPACE_CANDIDATES + 1) as i64)],
    )?;
    if rows.len() > MAX_WORKSPACE_CANDIDATES {
        return Err(IntelError::Invalid(format!(
            "workspace has more than {MAX_WORKSPACE_CANDIDATES} eligible Nuclei candidates; narrow the semantic scan before retrying"
        )));
    }

    let mut candidates = Vec::new();
    for row in rows {
        let payload: Value = row.get(4);
        let canonical_key: String = row.get(5);
        let Some(text) = candidate_text(&payload, &canonical_key) else {
            continue;
        };
        let candidate_id: String = row.get(0);
        let ingestion_id: String = row.get(1);
        let input_sha256 = format!("{:x}", Sha256::digest(text.as_bytes()));
        candidates.push(Candidate {
            candidate_id,
            ingestion_id,
            text,
            input_sha256,
        });
    }
    Ok(candidates)
}

/// Embed eligible Nuclei candidates in one workspace and persist thresholded,
/// review-only suggestions for candidates belonging to the requested ingestion.
pub fn scan_candidates(
    pool: &PgPool,
    workspace_id: &str,
    ingestion_id: &str,
    threshold: f32,
    top_k: u32,
    config: &EmbedWorkerConfig,
) -> IntelResult<CandidateSemanticOutcome> {
    if workspace_id.trim().is_empty() || workspace_id.len() > 200 {
        return Err(IntelError::Invalid(
            "workspace_id must contain 1..=200 bytes".to_string(),
        ));
    }
    if !crate::ingestion::valid_source_id(ingestion_id, "ing-") {
        return Err(IntelError::Invalid("ingestion_id is invalid".to_string()));
    }
    if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) || threshold == 0.0 {
        return Err(IntelError::Invalid(
            "threshold must be finite and in (0, 1]".to_string(),
        ));
    }
    if !(1..=MAX_TOP_K).contains(&top_k) {
        return Err(IntelError::Invalid(format!(
            "top_k must be in 1..={MAX_TOP_K}"
        )));
    }

    let source_candidates = {
        let mut conn = pool.get()?;
        let ingestion_exists: bool = conn
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM knowledge_ingestions
                 WHERE ingestion_id=$1 AND workspace_id=$2)",
                &[&ingestion_id, &workspace_id],
            )?
            .get(0);
        if !ingestion_exists {
            return Err(IntelError::Invalid(
                "ingestion not found in workspace".to_string(),
            ));
        }
        conn.query_one(
            "SELECT count(*) FROM knowledge_ingestion_candidates
             WHERE ingestion_id=$1 AND workspace_id=$2",
            &[&ingestion_id, &workspace_id],
        )?
        .get::<_, i64>(0) as usize
    };

    let workspace_candidates = load_workspace_candidates(pool, workspace_id)?;
    let eligible_source_ids: HashSet<&str> = workspace_candidates
        .iter()
        .filter(|candidate| candidate.ingestion_id == ingestion_id)
        .map(|candidate| candidate.candidate_id.as_str())
        .collect();
    let eligible_source_candidates = eligible_source_ids.len();

    let existing_vectors = {
        let ids: Vec<String> = workspace_candidates
            .iter()
            .map(|candidate| candidate.candidate_id.clone())
            .collect();
        if ids.is_empty() {
            HashMap::new()
        } else {
            let mut conn = pool.get()?;
            conn.query(
                "SELECT candidate_id,embedding_input_sha256,embedding_model_id,embedding_version
                 FROM knowledge_ingestion_candidate_vectors
                 WHERE candidate_id=ANY($1)",
                &[&ids],
            )?
            .into_iter()
            .map(|row| {
                (
                    row.get::<_, String>(0),
                    (
                        row.get::<_, String>(1),
                        row.get::<_, String>(2),
                        row.get::<_, String>(3),
                    ),
                )
            })
            .collect::<HashMap<_, _>>()
        }
    };

    let provider = EmbedWorkerProvider::new(config.clone());
    let pending: Vec<&Candidate> = workspace_candidates
        .iter()
        .filter(|candidate| {
            existing_vectors.get(&candidate.candidate_id).is_none_or(
                |(input_hash, model, version)| {
                    input_hash != &candidate.input_sha256
                        || model != EMBED_MODEL_ID
                        || version != EMBED_VERSION
                },
            )
        })
        .collect();
    let mut vectors_embedded = 0usize;
    for batch in pending.chunks(64) {
        let texts: Vec<String> = batch
            .iter()
            .map(|candidate| candidate.text.clone())
            .collect();
        let vectors = provider.embed_texts(&texts)?;
        let mut conn = pool.get()?;
        let mut tx = conn.transaction()?;
        for (candidate, vector) in batch.iter().zip(vectors.iter()) {
            tx.execute(
                "INSERT INTO knowledge_ingestion_candidate_vectors
                    (candidate_id,workspace_id,candidate_type,embedding,embedding_model_id,
                     embedding_version,embedding_input_sha256,dimension)
                 VALUES ($1,$2,'nuclei_template',$3,$4,$5,$6,512)
                 ON CONFLICT (candidate_id) DO UPDATE SET
                    embedding=EXCLUDED.embedding,
                    embedding_model_id=EXCLUDED.embedding_model_id,
                    embedding_version=EXCLUDED.embedding_version,
                    embedding_input_sha256=EXCLUDED.embedding_input_sha256,
                    dimension=EXCLUDED.dimension,updated_at=now()",
                &[
                    &candidate.candidate_id,
                    &workspace_id,
                    vector,
                    &EMBED_MODEL_ID,
                    &EMBED_VERSION,
                    &candidate.input_sha256,
                ],
            )?;
            vectors_embedded += 1;
        }
        tx.commit()?;
    }

    let mut suggestions = Vec::new();
    if eligible_source_candidates > 0 {
        let mut conn = pool.get()?;
        for candidate in workspace_candidates
            .iter()
            .filter(|candidate| candidate.ingestion_id == ingestion_id)
        {
            let rows = conn.query(
                "SELECT other.candidate_id,
                        1.0 - (other.embedding <=> source.embedding) AS similarity
                 FROM knowledge_ingestion_candidate_vectors source
                 JOIN knowledge_ingestion_candidates source_candidate
                   ON source_candidate.candidate_id=source.candidate_id
                  AND source_candidate.workspace_id=source.workspace_id
                  AND source_candidate.candidate_type=source.candidate_type
                  AND source_candidate.ingestion_id=$2
                  AND source_candidate.status NOT IN ('REJECTED','DEFERRED')
                 JOIN review_batch_item source_review
                   ON source_review.item_id=source_candidate.review_item_id
                  AND source_review.decision NOT IN ('REJECTED','DEFERRED')
                 JOIN knowledge_ingestion_candidate_vectors other
                   ON other.workspace_id=source.workspace_id
                  AND other.candidate_type=source.candidate_type
                 JOIN knowledge_ingestion_candidates c
                   ON c.candidate_id=other.candidate_id
                  AND c.workspace_id=other.workspace_id
                  AND c.candidate_type=other.candidate_type
                 JOIN review_batch_item other_review
                   ON other_review.item_id=c.review_item_id
                  AND other_review.decision NOT IN ('REJECTED','DEFERRED')
                 WHERE source.candidate_id=$1
                   AND other.candidate_id<>source.candidate_id
                   AND c.status NOT IN ('REJECTED','DEFERRED')
                 ORDER BY other.embedding <=> source.embedding
                 LIMIT $3",
                &[&candidate.candidate_id, &ingestion_id, &(top_k as i64)],
            )?;
            for row in rows {
                let similarity = row.get::<_, f64>(1);
                if similarity >= f64::from(threshold) {
                    suggestions.push((
                        candidate.candidate_id.clone(),
                        row.get::<_, String>(0),
                        similarity as f32,
                    ));
                }
            }
        }
    }

    {
        let mut conn = pool.get()?;
        let mut tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM knowledge_ingestion_candidate_similarity_suggestions
             WHERE ingestion_id=$1 AND workspace_id=$2",
            &[&ingestion_id, &workspace_id],
        )?;
        for (source_id, matched_id, similarity) in &suggestions {
            tx.execute(
                "INSERT INTO knowledge_ingestion_candidate_similarity_suggestions
                    (ingestion_id,source_candidate_id,matched_candidate_id,workspace_id,
                     candidate_type,cosine_similarity,threshold,embedding_model_id,embedding_version)
                 VALUES ($1,$2,$3,$4,'nuclei_template',$5,$6,$7,$8)",
                &[
                    &ingestion_id,
                    source_id,
                    matched_id,
                    &workspace_id,
                    similarity,
                    &threshold,
                    &EMBED_MODEL_ID,
                    &EMBED_VERSION,
                ],
            )?;
        }
        tx.commit()?;
    }

    Ok(CandidateSemanticOutcome {
        ingestion_id: ingestion_id.to_string(),
        workspace_id: workspace_id.to_string(),
        source_candidates,
        eligible_source_candidates,
        skipped_source_candidates: source_candidates.saturating_sub(eligible_source_candidates),
        workspace_candidates_indexed: workspace_candidates.len(),
        vectors_embedded,
        suggestions_persisted: suggestions.len(),
        threshold,
        top_k,
        embedding_model_id: EMBED_MODEL_ID,
        embedding_version: EMBED_VERSION,
        review_only: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn candidate_text_requires_allowlisted_unconflicted_metadata() {
        let payload = json!({
            "candidate_schema_version": "nuclei-template-metadata.v1",
            "structured_metadata_untrusted": true,
            "structured_metadata": {
                "template_id": "community/test-template",
                "name": "Static metadata sample",
                "severity": "HIGH"
            }
        });
        assert_eq!(
            candidate_text(&payload, "nuclei:community/test-template").as_deref(),
            Some("Nuclei template name: Static metadata sample\nSeverity: high")
        );

        let conflicted = json!({
            "candidate_schema_version": "nuclei-template-metadata.v1",
            "structured_metadata_untrusted": true,
            "structured_metadata": {
                "template_id": "community/test-template",
                "name": "Static metadata sample",
                "severity": "high"
            },
            "structured_metadata_conflict": true
        });
        let unsupported_severity = json!({
            "candidate_schema_version": "nuclei-template-metadata.v1",
            "structured_metadata_untrusted": true,
            "structured_metadata": {
                "template_id": "community/test-template",
                "name": "Static metadata sample",
                "severity": "unknown"
            }
        });
        let control_character = json!({
            "candidate_schema_version": "nuclei-template-metadata.v1",
            "structured_metadata_untrusted": true,
            "structured_metadata": {
                "template_id": "community/test-template",
                "name": "Static\nmetadata",
                "severity": "high"
            }
        });
        assert!(candidate_text(&conflicted, "nuclei:community/test-template").is_none());
        assert!(candidate_text(&unsupported_severity, "nuclei:community/test-template").is_none());
        assert!(candidate_text(&control_character, "nuclei:community/test-template").is_none());
        assert!(candidate_text(&payload, "nuclei:other-template").is_none());
    }
}
