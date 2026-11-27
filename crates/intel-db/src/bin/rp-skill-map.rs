//! Import evidence-backed Skill -> Tool mapping candidates into the M1 graph.
//! Dry-run is the default. `--apply` writes only CANDIDATE relations and
//! UNREVIEWED source snapshots; it never approves or activates a mapping.

use domain::kcard::KnowledgeEntityType;
use intel_db::knowledge_graph::{
    self, EntityRefInput, EntitySourceInput, SkillToolMappingCandidateInput,
};
use intel_db::{open_pool, run_migrations, PgConfig};
use serde_json::{json, Value};
use std::path::PathBuf;

fn usage() -> ! {
    eprintln!("usage: rp-skill-map --input <candidate-report.json> [--apply] [--limit <n>]");
    std::process::exit(2);
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("candidate field {key:?} must be a non-empty string"))
}

fn plan_candidate(candidate: &Value) -> Result<SkillToolMappingCandidateInput, String> {
    let skill_id = text(candidate, "skill_id")?;
    let tool_id = text(candidate, "tool_id")?;
    let skill_source = candidate
        .get("source")
        .ok_or_else(|| "candidate source block is required".to_string())?;
    let tool_source = candidate.get("tool_source").ok_or_else(|| {
        "candidate tool_source block is required; regenerate the report".to_string()
    })?;
    let body_hash = text(skill_source, "body_sha256")?;
    let tool_hash = text(tool_source, "content_sha256")?;
    for (name, hash) in [
        ("body_sha256", body_hash),
        ("tool content_sha256", tool_hash),
    ] {
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!("{name} must be a 64-character SHA-256 hex digest"));
        }
    }
    let version = candidate
        .get("skill_version")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("unversioned-{}", &body_hash[..12.min(body_hash.len())]));
    let tool_version = text(tool_source, "source_version")?.to_string();
    let source_system = text(skill_source, "source_system")?.to_string();
    let source_pk = text(skill_source, "source_pk")?.to_string();
    let tool_system = text(tool_source, "source_system")?.to_string();
    let tool_pk = text(tool_source, "source_pk")?.to_string();
    let tool_uri = text(tool_source, "canonical_uri")?.to_string();
    let provenance = candidate
        .get("tool_provenance_ref")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let evidence_uri = format!("ripskill-body://{skill_id}/{version}");
    let relation_evidence = json!({
        "generator": "scripts/security-intel/suggest_skill_tool_mappings.py",
        "generator_version": "0.1.0",
        "match_kind": candidate.get("match_kind").cloned().unwrap_or(Value::Null),
        "heuristic_score": candidate.get("heuristic_score").cloned().unwrap_or(Value::Null),
        "heuristic_score_is_probability": false,
        "occurrence_count": candidate.get("occurrence_count").cloned().unwrap_or(Value::Null),
        "activation_eligible_after_review": candidate.get("activation_eligible_after_review").cloned().unwrap_or(Value::Bool(false)),
        "tool_provenance_ref": provenance,
        "skill_body_ref": skill_source.get("body_ref").cloned().unwrap_or(Value::Null),
        "matches": candidate.get("evidence").cloned().unwrap_or_else(|| json!([])),
        "review_status": "CANDIDATE"
    });
    let skill = EntityRefInput {
        entity_type: KnowledgeEntityType::Skill,
        canonical_key: format!("skill:{skill_id}"),
        source: EntitySourceInput {
            source_system: source_system.clone(),
            source_entity_id: source_pk,
            source_version: version.clone(),
            canonical_uri: Some(format!("ripskill://{skill_id}/{version}")),
            source_commit: None,
            content_sha256: Some(body_hash.to_string()),
        },
    };
    let tool = EntityRefInput {
        entity_type: KnowledgeEntityType::SecurityTool,
        canonical_key: format!("tool:{tool_id}"),
        source: EntitySourceInput {
            source_system: tool_system,
            source_entity_id: tool_pk,
            source_version: tool_version,
            canonical_uri: Some(tool_uri),
            source_commit: None,
            content_sha256: Some(tool_hash.to_string()),
        },
    };
    let evidence = EntityRefInput {
        entity_type: KnowledgeEntityType::Evidence,
        canonical_key: format!("skill-body:{skill_id}@{version}"),
        source: EntitySourceInput {
            source_system: "rip.skill-body".to_string(),
            source_entity_id: skill_id.to_string(),
            source_version: version,
            canonical_uri: Some(evidence_uri),
            source_commit: None,
            content_sha256: Some(body_hash.to_string()),
        },
    };
    Ok(SkillToolMappingCandidateInput {
        skill,
        tool,
        evidence,
        evidence_metadata: relation_evidence,
        relation_source: "rip.skill-map-candidate:0.1.0".to_string(),
        // The heuristic score is retained as evidence metadata, not asserted as
        // a calibrated probability/confidence value in the authoritative graph.
        confidence: 0.5,
    })
}

fn main() {
    let mut input = None;
    let mut apply = false;
    let mut limit = usize::MAX;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => input = args.next().map(PathBuf::from),
            "--apply" => apply = true,
            "--limit" => {
                limit = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| usage());
                if limit == 0 {
                    usage();
                }
            }
            "--help" | "-h" => usage(),
            _ => usage(),
        }
    }
    let input = input.unwrap_or_else(|| usage());
    let report: Value = match std::fs::read(&input)
        .map_err(|e| e.to_string())
        .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|e| e.to_string()))
    {
        Ok(value) => value,
        Err(error) => {
            eprintln!("{{\"error\":{}}}", json!(error));
            std::process::exit(1);
        }
    };
    let candidates = match report.get("candidates").and_then(Value::as_array) {
        Some(rows) => rows,
        None => {
            eprintln!("{{\"error\":\"input must contain a candidates array\"}}");
            std::process::exit(1);
        }
    };
    let planned: Result<Vec<_>, _> = candidates.iter().take(limit).map(plan_candidate).collect();
    let planned = match planned {
        Ok(rows) => rows,
        Err(error) => {
            eprintln!("{{\"error\":{}}}", json!(error));
            std::process::exit(1);
        }
    };
    if !apply {
        let skill_count = planned
            .iter()
            .map(|row| row.skill.canonical_key.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let tool_count = planned
            .iter()
            .map(|row| row.tool.canonical_key.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let evidence_count = planned
            .iter()
            .map(|row| row.evidence.canonical_key.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        println!(
            "{}",
            json!({
                "mode": "dry_run",
                "candidate_report": input,
                "candidate_rows": planned.len(),
                "would_create_or_reuse": {
                    "skill_identities": skill_count,
                    "security_tool_identities": tool_count,
                    "evidence_snapshots": evidence_count,
                    "candidate_relations": planned.len()
                },
                "writes_performed": false,
                "approval_performed": false,
                "next_step": "rerun with --apply against a disposable or explicitly selected RP_PG_DSN"
            })
        );
        return;
    }

    let pool = match open_pool(&PgConfig::default()) {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("{{\"error\":{}}}", json!(error.to_string()));
            std::process::exit(1);
        }
    };
    let mut conn = match pool.get() {
        Ok(conn) => conn,
        Err(error) => {
            eprintln!("{{\"error\":{}}}", json!(error.to_string()));
            std::process::exit(1);
        }
    };
    if let Err(error) = run_migrations(&mut conn) {
        eprintln!("{{\"error\":{}}}", json!(error.to_string()));
        std::process::exit(1);
    }
    drop(conn);

    let mut created = 0usize;
    let mut existing = 0usize;
    let mut evidence_linked = 0usize;
    for row in &planned {
        match knowledge_graph::add_skill_tool_mapping_candidate(&pool, row) {
            Ok(result) => {
                if result.relation_created {
                    created += 1;
                } else {
                    existing += 1;
                }
                if result.evidence_link_created {
                    evidence_linked += 1;
                }
            }
            Err(error) => {
                eprintln!(
                    "{{\"error\":{},\"created_before_failure\":{},\"existing_before_failure\":{}}}",
                    json!(error.to_string()),
                    created,
                    existing
                );
                std::process::exit(1);
            }
        }
    }
    println!(
        "{}",
        json!({
            "mode": "applied_candidates_only",
            "candidate_report": input,
            "processed": planned.len(),
            "relations_created": created,
            "relations_already_present": existing,
            "evidence_links_created": evidence_linked,
            "approval_performed": false,
            "source_assessments": "UNREVIEWED",
            "relation_review_status": "CANDIDATE"
        })
    );
}
