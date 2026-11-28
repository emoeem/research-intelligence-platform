use mcp_server::dispatch::RunChannel;
use serde_json::{json, Value};
use std::path::PathBuf;

fn channel(caller: Option<&str>) -> RunChannel {
    RunChannel {
        run_db: PathBuf::from("unused-run.db"),
        migrations_dir: PathBuf::from("migrations"),
        manifest_dirs: Vec::new(),
        artifacts_dir: PathBuf::from("unused-artifacts"),
        workspaces_root: PathBuf::from("unused-workspaces"),
        runtime_node_id: "ingestion-dispatch-test".to_string(),
        working_dir: None,
        document_input_root: PathBuf::from("unused-documents"),
        capability_seed_path: PathBuf::from("unused-capability-seed.json"),
        collab_caller: caller.map(str::to_string),
        agent_bin: PathBuf::from("unused-rp-agent"),
        kc_bin: PathBuf::from("unused-rp-kc"),
        ingest_bin: PathBuf::from("/definitely-missing/rp-ingest"),
        subtask_timeout_s: 1,
    }
}

fn category(result: Result<Value, Value>) -> String {
    result.unwrap_err()["error"]["category"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn ingestion_tools_fail_closed_without_caller() {
    let channel = channel(None);
    assert_eq!(
        category(channel.submit_knowledge_ingestion(&json!({
            "source_kind": "research_report",
            "report_id": "rep-test",
            "idempotency_key": "test-key",
        }))),
        "POLICY_DENIED"
    );
    assert_eq!(
        category(channel.knowledge_ingestion_status(&json!({
            "ingestion_id": "ing-test",
        }))),
        "POLICY_DENIED"
    );
    assert_eq!(
        category(channel.knowledge_ingestion_candidate(&json!({
            "ingestion_id": "ing-test",
            "candidate_id": "ingc-test",
        }))),
        "POLICY_DENIED"
    );
}

#[test]
fn ingestion_submit_fixes_workspace_and_reports_spawn_failure() {
    let channel = channel(Some("caller-test"));
    let extra_workspace = channel.submit_knowledge_ingestion(&json!({
        "source_kind": "research_report",
        "report_id": "rep-test",
        "idempotency_key": "test-key",
        "workspace_id": "other",
    }));
    assert_eq!(category(extra_workspace), "VALIDATION");

    let status_workspace = channel.knowledge_ingestion_status(&json!({
        "ingestion_id": "ing-test",
        "workspace_id": "other",
    }));
    assert_eq!(category(status_workspace), "VALIDATION");

    let invalid_id = channel.submit_knowledge_ingestion(&json!({
        "source_kind": "research_report",
        "report_id": "not-a-report-id",
        "idempotency_key": "test-key",
    }));
    assert_eq!(category(invalid_id), "VALIDATION");

    let missing_binary = channel.submit_knowledge_ingestion(&json!({
        "source_kind": "research_report",
        "report_id": "rep-test",
        "idempotency_key": "test-key",
    }));
    assert_eq!(category(missing_binary), "INFRA");
}

#[test]
fn knowledge_tools_fix_workspace_and_reject_ai_approval() {
    let channel = channel(Some("caller-test"));

    let search_workspace = channel.search_knowledge(&json!({
        "q": "SQL injection",
        "workspace": "other",
    }));
    assert_eq!(category(search_workspace), "VALIDATION");

    let detail_workspace = channel.get_knowledge_detail(&json!({
        "knowledge_id": "kc-test",
        "workspace_id": "other",
    }));
    assert_eq!(category(detail_workspace), "VALIDATION");

    let review = channel.review_knowledge_candidate(&json!({
        "knowledge_id": "kc-test",
        "decision": "approve",
        "reason": "AI approval attempt",
    }));
    assert_eq!(category(review), "VALIDATION");

    let strict = channel.submit_knowledge_candidate(&json!({
        "contract_version": "1.0",
        "idempotency_key": "test-candidate-001",
        "knowledge_type": "vuln_signature",
        "title": "Test candidate",
        "content": "Test evidence-backed content",
        "source": {
            "type": "ai_finding",
            "title": "AI finding",
            "content_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "license": "unknown"
        },
        "evidence": [{
            "artifact_id": "art-test",
            "tool_run_id": "rn-test",
            "trace_id": "trace-test",
            "quote_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        }]
    }));
    assert_eq!(category(strict), "INFRA");

    let missing_evidence = channel.submit_knowledge_candidate(&json!({
        "contract_version": "1.0",
        "idempotency_key": "test-candidate-002",
        "knowledge_type": "vuln_signature",
        "title": "Test candidate",
        "content": "Test content",
        "source": {
            "type": "ai_finding",
            "title": "AI finding",
            "content_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "license": "unknown"
        },
        "evidence": []
    }));
    assert_eq!(category(missing_evidence), "VALIDATION");
}
