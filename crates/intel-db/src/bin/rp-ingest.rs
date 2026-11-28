//! rp-ingest: submit and inspect durable Knowledge Ingestion jobs.
//!
//! Adapters consume stored Report IR or an already parsed Document Revision.
//! Document identifier matches are evidence-backed candidates, never claims.

use intel_db::ingestion::{self, IngestionSource, KnowledgeIngestionRequest, CONTRACT_VERSION};
use intel_db::{open_pool, run_migrations, PgConfig};
use serde_json::json;

struct Args {
    command: String,
    flags: std::collections::HashMap<String, String>,
}

fn usage() -> &'static str {
    "usage: rp-ingest <command> [flags]

commands:
  rp-ingest report --report-id ID --workspace ID --idempotency-key KEY --requester ID
                   [--run-id ID] [--trace-id ID]
  rp-ingest document --document-id ID --revision-id ID --workspace ID
                     --idempotency-key KEY --requester ID [--run-id ID] [--trace-id ID]
  rp-ingest status --ingestion-id ID --workspace ID
  rp-ingest retry --ingestion-id ID --workspace ID
  rp-ingest candidate --ingestion-id ID --candidate-id ID --workspace ID
  rp-ingest semantic-scan --ingestion-id ID --workspace ID --threshold FLOAT
                          [--top-k N]

The report adapter accepts stored Report IR only. The document adapter requires
an existing parsed Document Revision (run rp-doc parse first). Extracted CVE/CWE
mentions are linked to block/page evidence and sent to human review. Review does
not assert or publish verified knowledge. Candidate evidence is workspace-scoped
and returns bounded, explicitly untrusted source snippets.
semantic-scan embeds eligible static Nuclei metadata with the pinned worker and
stores review-only similarity suggestions. Threshold selection requires labeled
calibration; candidates are never auto-merged or published.
Requires RP_PG_DSN.

exit: success=0, operational failure=1, usage error=2
help: rp-ingest --help"
}

fn parse_args() -> Result<Args, String> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{}", usage());
        std::process::exit(0);
    }
    let Some(command) = argv.first() else {
        return Err("missing command".into());
    };
    let mut flags = std::collections::HashMap::new();
    let mut i = 1;
    while i < argv.len() {
        let flag = argv[i].as_str();
        if !flag.starts_with("--") {
            return Err(format!("unexpected argument {flag}"));
        }
        let value = argv
            .get(i + 1)
            .ok_or_else(|| format!("{flag} requires a value"))?;
        if value.starts_with("--") {
            return Err(format!("{flag} requires a value"));
        }
        if flags.insert(flag.to_string(), value.to_string()).is_some() {
            return Err(format!("{flag} may only be specified once"));
        }
        i += 2;
    }
    let allowed: &[&str] = match command.as_str() {
        "report" => &[
            "--report-id",
            "--workspace",
            "--idempotency-key",
            "--requester",
            "--run-id",
            "--trace-id",
        ],
        "document" => &[
            "--document-id",
            "--revision-id",
            "--workspace",
            "--idempotency-key",
            "--requester",
            "--run-id",
            "--trace-id",
        ],
        "status" | "retry" => &["--ingestion-id", "--workspace"],
        "candidate" => &["--ingestion-id", "--candidate-id", "--workspace"],
        "semantic-scan" => &["--ingestion-id", "--workspace", "--threshold", "--top-k"],
        _ => &[],
    };
    if let Some(flag) = flags.keys().find(|flag| !allowed.contains(&flag.as_str())) {
        return Err(format!("unknown flag {flag}"));
    }
    Ok(Args {
        command: command.clone(),
        flags,
    })
}

fn required(args: &Args, flag: &str) -> Result<String, String> {
    args.flags
        .get(flag)
        .cloned()
        .ok_or_else(|| format!("{} requires {flag}", args.command))
}

fn semantic_scan_args(args: &Args) -> Result<(String, String, f32, u32), String> {
    let ingestion_id = required(args, "--ingestion-id")?;
    let workspace_id = required(args, "--workspace")?;
    let threshold = required(args, "--threshold")?
        .parse::<f32>()
        .map_err(|_| "--threshold must be a finite number in (0, 1]".to_string())?;
    if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) || threshold == 0.0 {
        return Err("--threshold must be a finite number in (0, 1]".to_string());
    }
    let top_k = args
        .flags
        .get("--top-k")
        .map(|value| {
            value
                .parse::<u32>()
                .map_err(|_| "--top-k must be an integer in 1..=50".to_string())
        })
        .transpose()?
        .unwrap_or(10);
    if !(1..=50).contains(&top_k) {
        return Err("--top-k must be an integer in 1..=50".to_string());
    }
    Ok((ingestion_id, workspace_id, threshold, top_k))
}

fn fail(message: &str, code: i32) -> ! {
    eprintln!("{}", json!({"error": message}));
    eprintln!("{}", usage());
    std::process::exit(code);
}

fn main() {
    let args = parse_args().unwrap_or_else(|error| fail(&error, 2));
    let semantic_args = if args.command == "semantic-scan" {
        Some(semantic_scan_args(&args).unwrap_or_else(|error| fail(&error, 2)))
    } else {
        None
    };
    let pool = open_pool(&PgConfig::default())
        .unwrap_or_else(|error| fail(&format!("PostgreSQL connection failed: {error}"), 1));
    let mut conn = pool
        .get()
        .unwrap_or_else(|error| fail(&format!("PostgreSQL checkout failed: {error}"), 1));
    run_migrations(&mut conn)
        .unwrap_or_else(|error| fail(&format!("PostgreSQL migration failed: {error}"), 1));
    drop(conn);

    if let Some((ingestion_id, workspace_id, threshold, top_k)) = semantic_args {
        let config = intel_db::embed_worker::EmbedWorkerConfig::default();
        let outcome = intel_db::candidate_semantic::scan_candidates(
            &pool,
            &workspace_id,
            &ingestion_id,
            threshold,
            top_k,
            &config,
        )
        .unwrap_or_else(|error| fail(&format!("semantic scan failed: {error}"), 1));
        println!(
            "{}",
            serde_json::to_string_pretty(&outcome)
                .unwrap_or_else(|error| fail(&format!("serialization failed: {error}"), 1))
        );
        return;
    }

    let result = match args.command.as_str() {
        "report" => {
            let report_id = required(&args, "--report-id").unwrap_or_else(|e| fail(&e, 2));
            let workspace_id = required(&args, "--workspace").unwrap_or_else(|e| fail(&e, 2));
            let idempotency_key =
                required(&args, "--idempotency-key").unwrap_or_else(|e| fail(&e, 2));
            let requester = required(&args, "--requester").unwrap_or_else(|e| fail(&e, 2));
            let request = KnowledgeIngestionRequest {
                contract_version: CONTRACT_VERSION.to_string(),
                workspace_id,
                idempotency_key,
                source: IngestionSource::ResearchReport { report_id },
                requester,
                run_id: args.flags.get("--run-id").cloned(),
                trace_id: args.flags.get("--trace-id").cloned(),
            };
            ingestion::submit_report(&pool, &request)
        }
        "document" => {
            let document_id = required(&args, "--document-id").unwrap_or_else(|e| fail(&e, 2));
            let revision_id = required(&args, "--revision-id").unwrap_or_else(|e| fail(&e, 2));
            let workspace_id = required(&args, "--workspace").unwrap_or_else(|e| fail(&e, 2));
            let idempotency_key =
                required(&args, "--idempotency-key").unwrap_or_else(|e| fail(&e, 2));
            let requester = required(&args, "--requester").unwrap_or_else(|e| fail(&e, 2));
            let request = KnowledgeIngestionRequest {
                contract_version: CONTRACT_VERSION.to_string(),
                workspace_id,
                idempotency_key,
                source: IngestionSource::DocumentRevision {
                    document_id,
                    revision_id,
                },
                requester,
                run_id: args.flags.get("--run-id").cloned(),
                trace_id: args.flags.get("--trace-id").cloned(),
            };
            ingestion::submit_report(&pool, &request)
        }
        "status" => {
            let ingestion_id = required(&args, "--ingestion-id").unwrap_or_else(|e| fail(&e, 2));
            let workspace_id = required(&args, "--workspace").unwrap_or_else(|e| fail(&e, 2));
            match ingestion::get_status(&pool, &workspace_id, &ingestion_id) {
                Ok(Some(status)) => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&status).unwrap_or_else(|e| {
                            fail(&format!("status serialization failed: {e}"), 1)
                        })
                    );
                    return;
                }
                Ok(None) => fail("ingestion not found in workspace", 1),
                Err(error) => fail(&format!("status lookup failed: {error}"), 1),
            }
        }

        "candidate" => {
            let ingestion_id = required(&args, "--ingestion-id").unwrap_or_else(|e| fail(&e, 2));
            let candidate_id = required(&args, "--candidate-id").unwrap_or_else(|e| fail(&e, 2));
            let workspace_id = required(&args, "--workspace").unwrap_or_else(|e| fail(&e, 2));
            match ingestion::get_candidate_evidence(
                &pool,
                &workspace_id,
                &ingestion_id,
                &candidate_id,
            ) {
                Ok(Some(candidate)) => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&candidate).unwrap_or_else(|e| fail(
                            &format!("candidate serialization failed: {e}"),
                            1
                        ))
                    );
                    return;
                }
                Ok(None) => fail("candidate not found in ingestion workspace", 1),
                Err(error) => fail(&format!("candidate lookup failed: {error}"), 1),
            }
        }

        "retry" => {
            let ingestion_id = required(&args, "--ingestion-id").unwrap_or_else(|e| fail(&e, 2));
            let workspace_id = required(&args, "--workspace").unwrap_or_else(|e| fail(&e, 2));
            ingestion::retry_ingestion(&pool, &workspace_id, &ingestion_id)
        }
        other => fail(&format!("unknown command {other}"), 2),
    };
    match result {
        Ok(status) => println!(
            "{}",
            serde_json::to_string_pretty(&status)
                .unwrap_or_else(|error| fail(&format!("serialization failed: {error}"), 1))
        ),
        Err(error) => fail(&format!("ingestion failed: {error}"), 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn semantic_args(flags: &[(&str, &str)]) -> Args {
        Args {
            command: "semantic-scan".to_string(),
            flags: flags
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        }
    }

    #[test]
    fn semantic_scan_requires_an_explicit_valid_threshold() {
        let valid = semantic_args(&[
            ("--ingestion-id", "ing-test"),
            ("--workspace", "default"),
            ("--threshold", "0.85"),
        ]);
        assert_eq!(
            semantic_scan_args(&valid).unwrap(),
            ("ing-test".to_string(), "default".to_string(), 0.85, 10)
        );

        let missing = semantic_args(&[("--ingestion-id", "ing-test"), ("--workspace", "default")]);
        assert!(semantic_scan_args(&missing).is_err());

        let non_finite = semantic_args(&[
            ("--ingestion-id", "ing-test"),
            ("--workspace", "default"),
            ("--threshold", "NaN"),
        ]);
        assert!(semantic_scan_args(&non_finite).is_err());

        let excessive_top_k = semantic_args(&[
            ("--ingestion-id", "ing-test"),
            ("--workspace", "default"),
            ("--threshold", "0.85"),
            ("--top-k", "51"),
        ]);
        assert!(semantic_scan_args(&excessive_top_k).is_err());
    }
}
