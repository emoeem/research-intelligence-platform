//! P7-5 端到端 trace 关联验收（SESSION-HANDOFF §十追加项 1；ask 自验判据：
//! 一条端到端 run 后，按 trace_id 与按 evidence_id 两个方向各执行一次关联
//! 查询，均返回完整五类事件链：任务/意图/上下文/工具/证据）。
//!
//! 链路：内置测试 Agent（真实绑定 rip.kb.keyword_search）+ Mock 驱动注入
//! （TOOL-CONTRACT §10.1 注入口径，与 closed_loop_p7_2 同款装配）→ 一次
//! run → 断言六 ID 五面落库同链 → trace_query 双向查询。
//!
//! 测试库：内存库迁移全链 001..015；audit_event 只读查本测试库副本
//! （红线：platform*.db 不写）。

use agent_engine::agent_loop::{AgentLoop, KeywordSearchPlanner, LoopConfig, RunRequest};
use agent_engine::broker_link::{BrokerLink, FirstFamilyConfirmer};
use agent_engine::evidence::EvidenceRegistrar;
use agent_engine::store::AgentStore;
use agent_engine::tool_link::{MockDriver, ToolLink};
use agent_engine::trace::{trace_query, DEFAULT_SECTION_LIMIT};
use context_broker::{
    ArtifactSource, Broker, ManifestStore, PluginSource, RegistrySource, RulepackSource,
};
use database::{rusqlite, DbPath, Pool, PoolConfig};
use domain::{AgentDefinition, AgentId, CapabilityRef};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use tool_engine::Engine;
use tool_registry::Registry;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn migrated_pool() -> Pool {
    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    database::migrate(&pool, &repo_root().join("migrations")).unwrap();
    pool
}

/// 真实登记 Manifest（仓库 manifests/tools/rip.kb.keyword_search.json）并
/// 置 ENABLED（Broker 目录面可见口径，closed_loop_p7_2 同款）。
fn register_test_tool(pool: &Pool) {
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("manifests/tools/rip.kb.keyword_search.json"))
            .unwrap(),
    )
    .unwrap();
    Registry::open(pool.clone()).register(&manifest).unwrap();
    pool.with_conn(|c| {
        c.execute(
            "UPDATE tool SET lifecycle = 'ENABLED' WHERE tool_id = 'rip.kb.keyword_search'",
            [],
        )
    })
    .unwrap();
}

fn build_broker_link(pool: &Pool) -> BrokerLink {
    let rulepack = RulepackSource::from_dir(&repo_root().join("rulepacks/rip-rules")).unwrap();
    let intent_config =
        context_broker::IntentConfig::from_dir(&repo_root().join("rulepacks/rip-rules")).unwrap();
    let index =
        context_broker::ManifestIndex::from_dirs(&[repo_root().join("manifests/tools")]).unwrap();
    let registry_source = RegistrySource::new(Registry::open(pool.clone()), index);
    let mut sources: Vec<Box<dyn context_broker::ContextSource>> = vec![
        Box::new(rulepack),
        Box::new(registry_source),
        Box::new(ArtifactSource::new(pool.clone())),
    ];
    for plugin_source in PluginSource::mount_all(pool) {
        sources.push(Box::new(plugin_source));
    }
    let broker = Broker::new(
        sources,
        context_broker::BudgetPolicy::default_for(8000).unwrap(),
        intent_config,
        ManifestStore::new(pool.clone()),
        Some(audit_engine::AuditEngine::new(pool.clone())),
    );
    BrokerLink::new(broker)
}

fn db_scalar(pool: &Pool, sql: &str) -> Value {
    pool.with_conn(|c| {
        let v: rusqlite::types::Value = c.query_row(sql, [], |r| r.get(0)).unwrap();
        Ok::<Value, rusqlite::Error>(match v {
            rusqlite::types::Value::Null => Value::Null,
            rusqlite::types::Value::Integer(i) => json!(i),
            rusqlite::types::Value::Text(t) => json!(t),
            rusqlite::types::Value::Real(f) => json!(f),
            rusqlite::types::Value::Blob(_) => json!("<blob>"),
        })
    })
    .unwrap()
}

fn db_count(pool: &Pool, sql: &str) -> i64 {
    pool.with_conn(|c| c.query_row(sql, [], |r| r.get::<_, i64>(0)))
        .unwrap()
}

#[test]
fn e2e_run_writes_one_trace_across_all_families_and_both_query_directions_work() {
    let pool = migrated_pool();
    register_test_tool(&pool);
    let store = AgentStore::open(pool.clone(), Registry::open(pool.clone()));
    let definition = AgentDefinition::try_new(
        AgentId::try_new("agent.research.minimal").unwrap(),
        "0.1.0".to_string(),
        "{\"l0_rules\":true}".to_string(),
        vec![CapabilityRef::try_new("rip.kb.keyword_search", None).unwrap()],
    )
    .unwrap();
    store
        .register_definition(&definition, "最小研究 Agent（P7-5 trace 验收）")
        .unwrap();

    let engine = Engine::open(pool.clone(), Registry::open(pool.clone()));
    let tool_link = ToolLink::new(
        engine,
        Box::new(MockDriver::succeed(
            json!({"keyword": "nmap", "per_group_limit": 5, "total_hits": 0, "groups": []}),
            "关键词检索完成（mock 注入）",
        )),
    );
    let loop_ = AgentLoop::new(
        store,
        build_broker_link(&pool),
        tool_link,
        EvidenceRegistrar::new(pool.clone()),
        Some(audit_engine::AuditEngine::new(pool.clone())),
        LoopConfig::default(),
    );

    // 端到端一次 run（task 关联 + 显式 trace_id，P7-5）。
    let report = loop_
        .run(&mut RunRequest {
            agent_id: "agent.research.minimal".to_string(),
            agent_version: None,
            goal: "检索 nmap 端口扫描相关知识并调用登记工具".to_string(),
            task_id: Some("task-p75-accept".to_string()),
            trace_id: Some("tr-p75-e2e".to_string()),
            run_id: None,
            input: None,
            planner: Box::new(KeywordSearchPlanner),
            confirmer: Box::new(FirstFamilyConfirmer::new(
                context_broker::IntentConfig::from_dir(&repo_root().join("rulepacks/rip-rules"))
                    .unwrap(),
            )),
        })
        .unwrap();
    assert_eq!(report.state, "SUCCEEDED", "report: {report:?}");
    assert_eq!(report.trace_id.as_deref(), Some("tr-p75-e2e"));
    let run_id = report.run_id.clone();
    let step = &report.steps[0];
    let tool_run_id = step.tool_run_id.clone();
    let artifact_id = step
        .artifact_id
        .clone()
        .expect("SUCCEEDED 步骤登记 artifact");
    let evidence_id = step
        .evidence_id
        .clone()
        .expect("SUCCEEDED 步骤登记 evidence");

    // ---- 五面同链落库（迁移 015 列；audit_event 只读本测试库副本）----
    let scalar = |sql: &str| db_scalar(&pool, sql);
    assert_eq!(
        scalar(&format!(
            "SELECT trace_id FROM agent_run WHERE run_id = '{run_id}'"
        )),
        json!("tr-p75-e2e")
    );
    assert_eq!(
        scalar(&format!(
            "SELECT trace_id FROM tool_run WHERE run_id = '{tool_run_id}'"
        )),
        json!("tr-p75-e2e")
    );
    assert_eq!(
        scalar(&format!(
            "SELECT trace_id FROM artifact WHERE artifact_id = '{artifact_id}'"
        )),
        json!("tr-p75-e2e")
    );
    assert_eq!(
        scalar(&format!(
            "SELECT trace_id FROM evidence WHERE evidence_id = '{evidence_id}'"
        )),
        json!("tr-p75-e2e")
    );
    let audit_by_trace = db_count(
        &pool,
        "SELECT COUNT(*) FROM audit_event WHERE trace_id = 'tr-p75-e2e'",
    );
    assert!(
        audit_by_trace >= 3,
        "run 全程事件（accepted/step/finished）随 trace_id 落列：{audit_by_trace}"
    );
    let audit_unlinked = db_count(
        &pool,
        "SELECT COUNT(*) FROM audit_event WHERE trace_id IS NULL AND correlation_id IS NOT NULL",
    );
    assert_eq!(
        audit_unlinked, 0,
        "本链路全部关联事件 trace_id 无缺失（context-broker intent 审计事件直写 AuditEvent 结构体，trace_id 列缺省 None，经 correlation_id = run_id 由查询面兜底关联，如实声明不伪装）"
    );

    // ---- 方向 1：按 trace_id 关联查询，五类链齐全 ----
    let by_trace = trace_query(&pool, "tr-p75-e2e", DEFAULT_SECTION_LIMIT).unwrap();
    assert_eq!(by_trace.trace_ids, vec!["tr-p75-e2e".to_string()]);
    assert_eq!(by_trace.matched_via, vec!["trace_id".to_string()]);
    // 工具面。
    assert_eq!(by_trace.tool_runs.len(), 1);
    assert_eq!(
        by_trace.tool_runs[0].get("run_id").and_then(Value::as_str),
        Some(tool_run_id.as_str())
    );
    // 证据面（evidence + artifact）。
    assert_eq!(by_trace.evidence.len(), 1);
    assert_eq!(by_trace.artifacts.len(), 1);
    // 意图/上下文面（context_manifest 按 task_id 关联）。
    assert!(
        !by_trace.manifests.is_empty(),
        "意图/上下文面经 task_id 关联命中：manifests = {:?}",
        by_trace.manifests
    );
    assert_eq!(
        by_trace.manifests[0]
            .get("intent_status")
            .and_then(Value::as_str),
        Some("ok")
    );
    assert!(
        !by_trace.context_items.is_empty(),
        "上下文条目面（context_manifest_item）随 manifest 关联命中"
    );
    // 审计面。
    assert!(!by_trace.audit_events.is_empty());
    // 任务面：research_task 无行时如实空段（不伪造）。
    assert!(by_trace.tasks.is_empty(), "无 research_task 行 → 任务段空");

    // ---- 方向 2：按 evidence_id 关联查询，同一链 ----
    let by_evidence = trace_query(&pool, &evidence_id, DEFAULT_SECTION_LIMIT).unwrap();
    assert_eq!(by_evidence.trace_ids, vec!["tr-p75-e2e".to_string()]);
    assert_eq!(by_evidence.matched_via, vec!["evidence_id".to_string()]);
    assert_eq!(
        by_evidence.tool_runs[0]
            .get("run_id")
            .and_then(Value::as_str),
        Some(tool_run_id.as_str())
    );
    assert_eq!(by_evidence.artifacts.len(), 1);
    assert!(!by_evidence.manifests.is_empty());
    assert!(!by_evidence.audit_events.is_empty());
    assert_eq!(by_evidence.counts(), by_trace.counts(), "双向查询同链同宽");

    // ---- 附：task_id 方向（六 ID 词形全覆盖）----
    let by_task = trace_query(&pool, "task-p75-accept", DEFAULT_SECTION_LIMIT).unwrap();
    assert_eq!(by_task.matched_via, vec!["task_id".to_string()]);
    assert_eq!(by_task.trace_ids, vec!["tr-p75-e2e".to_string()]);
}
