//! Import exact-CVE Skill -> Vulnerability/Nuclei-template references.
//! Dry-run is the default. `--apply` writes evidence-backed CANDIDATE relations
//! only; graph source assessments remain UNREVIEWED and no approval occurs.

use domain::kcard::{KnowledgeEntityType, RelationType};
use intel_db::knowledge_graph::{
    self, EntityRefInput, EntitySourceInput, SkillEntityReferenceCandidateInput,
};
use intel_db::{open_pool, run_migrations, PgConfig};
use serde_json::{json, Value};
use std::path::PathBuf;

fn usage() -> ! {
    eprintln!("usage: rp-skill-ref-map --input <candidate-report.json> [--apply] [--limit <n>]");
    std::process::exit(2);
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("candidate field {key:?} must be a non-empty string"))
}

fn digest(value: &str, name: &str) -> Result<(), String> {
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("{name} must be a 64-character SHA-256 hex digest"));
    }
    Ok(())
}

fn target_type(raw: &str) -> Result<KnowledgeEntityType, String> {
    match raw {
        "vulnerability" => Ok(KnowledgeEntityType::Vulnerability),
        "detection_template" => Ok(KnowledgeEntityType::DetectionTemplate),
        "poc" => Ok(KnowledgeEntityType::Poc),
        "knowledge_card" => Ok(KnowledgeEntityType::KnowledgeCard),
        "technical_document" => Ok(KnowledgeEntityType::TechnicalDocument),
        "security_tool" => Ok(KnowledgeEntityType::SecurityTool),
        other => Err(format!("unsupported Skill reference target type: {other}")),
    }
}

fn plan_candidate(candidate: &Value) -> Result<SkillEntityReferenceCandidateInput, String> {
    let skill_id = text(candidate, "skill_id")?;
    let target_key = text(candidate, "target_key")?;
    let source = candidate
        .get("source")
        .ok_or("candidate source block is required")?;
    let target_source = candidate
        .get("target_source")
        .ok_or("candidate target_source block is required")?;
    let body_hash = text(source, "body_sha256")?;
    let target_hash = text(target_source, "content_sha256")?;
    digest(body_hash, "Skill body_sha256")?;
    digest(target_hash, "target content_sha256")?;
    let version = candidate
        .get("skill_version")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("unversioned-{}", &body_hash[..12]));
    let skill_source_system = text(source, "source_system")?.to_string();
    let skill_source_pk = text(source, "source_pk")?.to_string();
    let target_source_system = text(target_source, "source_system")?.to_string();
    let target_source_id = text(target_source, "source_entity_id")?.to_string();
    let target_version = text(target_source, "source_version")?.to_string();
    let target_uri = text(target_source, "canonical_uri")?.to_string();
    let target_type = target_type(text(candidate, "target_type")?)?;
    let relation_type =
        RelationType::parse(text(candidate, "relation_type")?).map_err(|e| e.to_string())?;
    let evidence_uri = format!("ripskill-body://{skill_id}/{version}");
    let metadata = json!({
        "generator": "scripts/security-intel/suggest_skill_knowledge_mappings.py",
        "generator_version": "0.1.0",
        "match_basis": candidate.get("match_basis").cloned().unwrap_or(Value::Null),
        "cve": candidate.get("cve").cloned().unwrap_or(Value::Null),
        "target_type": text(candidate, "target_type")?,
        "target_key": target_key,
        "target_title": candidate.get("template_title").cloned().unwrap_or(Value::Null),
        "source_review_status": target_source.get("source_review_status").cloned().unwrap_or(Value::Null),
        "license_status": target_source.get("license_status").cloned().unwrap_or(Value::Null),
        "activation_eligible_after_review": candidate.get("activation_eligible_after_review").cloned().unwrap_or(Value::Bool(false)),
        "evidence": candidate.get("evidence").cloned().unwrap_or_else(|| json!([])),
        "skill_body_ref": source.get("body_ref").cloned().unwrap_or(Value::Null),
        "skill_body_sha256": body_hash,
        "review_status": "CANDIDATE",
        "heuristic_score_is_probability": false,
    });
    Ok(SkillEntityReferenceCandidateInput {
        skill: EntityRefInput {
            entity_type: KnowledgeEntityType::Skill,
            canonical_key: format!("skill:{skill_id}"),
            source: EntitySourceInput {
                source_system: skill_source_system,
                source_entity_id: skill_source_pk,
                source_version: version.clone(),
                canonical_uri: Some(format!("ripskill://{skill_id}/{version}")),
                source_commit: None,
                content_sha256: Some(body_hash.to_string()),
            },
        },
        target: EntityRefInput {
            entity_type: target_type,
            canonical_key: target_key.to_string(),
            source: EntitySourceInput {
                source_system: target_source_system,
                source_entity_id: target_source_id,
                source_version: target_version,
                canonical_uri: Some(target_uri),
                source_commit: target_source
                    .get("source_commit")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                content_sha256: Some(target_hash.to_string()),
            },
        },
        evidence: EntityRefInput {
            entity_type: KnowledgeEntityType::Evidence,
            canonical_key: format!("skill-body:{skill_id}@{version}"),
            source: EntitySourceInput {
                source_system: "rip.skill-body".to_string(),
                source_entity_id: skill_id.to_string(),
                source_version: version.clone(),
                canonical_uri: Some(evidence_uri),
                source_commit: None,
                content_sha256: Some(body_hash.to_string()),
            },
        },
        relation_type,
        evidence_metadata: metadata,
        relation_source: "rip.skill-reference-candidate:0.1.0".to_string(),
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
        let skills = planned
            .iter()
            .map(|r| r.skill.canonical_key.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let targets = planned
            .iter()
            .map(|r| {
                (
                    r.target.entity_type.as_str(),
                    r.target.canonical_key.as_str(),
                )
            })
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let evidence = planned
            .iter()
            .map(|r| r.evidence.canonical_key.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        println!(
            "{}",
            json!({
                "mode": "dry_run", "candidate_report": input, "candidate_rows": planned.len(),
                "would_create_or_reuse": {"skill_identities": skills, "target_identities": targets, "evidence_snapshots": evidence, "candidate_relations": planned.len()},
                "writes_performed": false, "approval_performed": false
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
    let mut evidence_links = 0usize;
    for row in &planned {
        match knowledge_graph::add_skill_entity_reference_candidate(&pool, row) {
            Ok(result) => {
                if result.relation_created {
                    created += 1;
                } else {
                    existing += 1;
                }
                if result.evidence_link_created {
                    evidence_links += 1;
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
            "mode": "applied_candidates_only", "candidate_report": input, "processed": planned.len(),
            "relations_created": created, "relations_already_present": existing,
            "evidence_links_created": evidence_links, "relation_review_status": "CANDIDATE",
            "source_assessments": "UNREVIEWED", "approval_performed": false
        })
    );
}
