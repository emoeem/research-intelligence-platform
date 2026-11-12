//! P7-2 闭环验收（ROADMAP §8.3 P7-2 判据 + 计划 §14:823 验收第 1 条缩小版）：
//! 内置测试 Agent 经 Broker 装配（SK 插件知识源 L2 召回 + L3 expand 记账
//! 进 context_ref_log），经 Tool Engine 对登记工具完成一次调用，Artifact/
//! Evidence 落账，run 终态 SUCCEEDED。
//!
//! 全程内存库 + 真实 migrations（001 到 012）+ 仓库登记 Manifest
//! （rip.kb.keyword_search.json），不用手写假件。

#![deny(missing_docs)]

use agent_engine::agent_loop::{AgentLoop, KeywordSearchPlanner, LoopConfig, RunRequest};
use agent_engine::broker_link::{BrokerLink, FirstFamilyConfirmer};
use agent_engine::evidence::EvidenceRegistrar;
use agent_engine::store::AgentStore;
use agent_engine::tool_link::{MockDriver, ToolLink};
use context_broker::{
    ArtifactSource, Broker, ManifestStore, PluginSource, RegistrySource, RulepackSource,
};
use database::{DbPath, Pool, PoolConfig};
use domain::{AgentDefinition, AgentId, CapabilityRef};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;
use tool_engine::Engine;
use tool_registry::Registry;

fn migrated_pool() -> Pool {
    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    database::migrate(&pool, &dir).unwrap();
    pool
}

/// SK-1 形态种子：插件登记行 + APPROVED 知识条目（含 nmap 正文，供
/// PluginSource 关键词召回）。
fn seed_plugin_knowledge(pool: &Pool) {
    pool.with_conn(|c| -> database::rusqlite::Result<()> {
        c.execute(
            "INSERT INTO plugin (plugin_id, version, name, kind, description, license, trust_level,
             state, source_type, source_ref, manifest_json, created_at, updated_at, is_active)
             VALUES ('sk.test.recon','0.1.0','Test Recon Knowledge','knowledge','d','MIT','local',
             'ENABLED','local_dir','.','{}','2026-11-12T00:00:00Z','2026-11-12T00:00:00Z',1)",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO knowledge_source_entry (plugin_id, plugin_version, corpus_id, entry_kind,
             item_key, title, body, content_sha256, metadata_sha256, first_seen_at, last_seen_at,
             review_status)
             VALUES ('sk.test.recon','0.1.0','c','catalog_entry','nmap-basics','nmap 端口扫描基础',
             'nmap 是端口扫描工具：nmap -sV 可做服务识别；端口扫描先确认连通性。',
             'deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef','meta',
             '2026-11-12T00:00:00Z','2026-11-12T00:00:00Z','APPROVED')",
            [],
        )
        .unwrap();
        Ok(())
    })
    .unwrap();
}

/// 真实登记 Manifest（仓库 manifests/tools/rip.kb.keyword_search.json）并
/// 置 ENABLED（Broker 目录面可见口径）。
fn register_test_tool(pool: &Pool) {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../manifests/tools/rip.kb.keyword_search.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(manifest_path).unwrap()).unwrap();
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
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let rulepack = RulepackSource::from_dir(&root.join("rulepacks/rip-rules")).unwrap();
    let intent_config =
        context_broker::IntentConfig::from_dir(&root.join("rulepacks/rip-rules")).unwrap();
    let index = context_broker::ManifestIndex::from_dirs(&[root.join("manifests/tools")]).unwrap();
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
    )
    .with_evidence_store(resource_engine::sqlite::SqliteResourceRepository::new(
        pool.clone(),
    ));
    BrokerLink::new(broker)
}

#[test]
fn builtin_agent_closes_loop_with_broker_knowledge_and_tool_evidence() {
    let pool = migrated_pool();
    seed_plugin_knowledge(&pool);
    register_test_tool(&pool);

    // 内置测试 Agent 登记（真实绑定 rip.kb.keyword_search）。
    let store = AgentStore::open(pool.clone(), Registry::open(pool.clone()));
    let definition = AgentDefinition::try_new(
        AgentId::try_new("agent.research.minimal").unwrap(),
        "0.1.0".to_string(),
        "{\"l0_rules\":true}".to_string(),
        vec![CapabilityRef::try_new("rip.kb.keyword_search", None).unwrap()],
    )
    .unwrap();
    store
        .register_definition(&definition, "最小研究 Agent（P7-2 内置测试）")
        .unwrap();

    // 组装闭环（Mock 驱动注入：TOOL-CONTRACT §10.1 注入口径）。
    let engine = Engine::open(pool.clone(), Registry::open(pool.clone()));
    let tool_link = ToolLink::new(
        engine,
        Box::new(MockDriver::succeed(
            json!({"keyword": "nmap", "per_group_limit": 5, "total_hits": 0, "groups": []}),
            "关键词检索完成（mock 注入）",
        )),
    );
    let loop_ = AgentLoop::new(
        store.clone(),
        build_broker_link(&pool),
        tool_link,
        EvidenceRegistrar::new(pool.clone()),
        Some(audit_engine::AuditEngine::new(pool.clone())),
        LoopConfig::default(),
    );

    let report = loop_
        .run(&mut RunRequest {
            agent_id: "agent.research.minimal".to_string(),
            agent_version: None,
            goal: "检索 nmap 端口扫描相关知识并调用登记工具".to_string(),
            task_id: Some("task-p72-accept".to_string()),
            trace_id: None,
            input: None,
            planner: Box::new(KeywordSearchPlanner),
            confirmer: Box::new(FirstFamilyConfirmer::new(
                context_broker::IntentConfig::from_dir(
                    &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../rulepacks/rip-rules"),
                )
                .unwrap(),
            )),
        })
        .unwrap();

    // ---- 报告面 ----
    assert_eq!(report.state, "SUCCEEDED", "report: {report:?}");
    assert_eq!(report.intent_status, "ok");
    assert_eq!(report.steps.len(), 1, "report: {report:?}");
    let step = &report.steps[0];
    assert_eq!(step.tool_id, "rip.kb.keyword_search");
    assert_eq!(step.status, "SUCCEEDED");
    assert_eq!(step.tool_version, "0.1.0");
    assert!(
        step.tool_run_id.starts_with("rn-"),
        "run_id: {}",
        step.tool_run_id
    );
    assert!(step.artifact_id.is_some() && step.evidence_id.is_some());
    // SK 插件知识经 Broker 召回并 L3 expand 记账。
    assert_eq!(report.l3_expansions.len(), 1, "report: {report:?}");
    assert!(report.l3_expansions[0].granted_tokens > 0);
    assert_eq!(
        report.l3_expansions[0].item_id,
        "kse:sk.test.recon:catalog_entry:nmap-basics"
    );

    // ---- context_ref_log：EXPANDED 记账可查（expand 扣账落 log）----
    let expanded: i64 = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM context_ref_log WHERE manifest_id = ?1 AND event = 'EXPANDED'",
                [&report.manifest_id],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert!(expanded >= 1, "context_ref_log must record EXPANDED");

    // ---- context_manifest：装配三表只追加落库 ----
    let manifest_rows: i64 = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM context_manifest WHERE manifest_id = ?1",
                [&report.manifest_id],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(manifest_rows, 1);

    // ---- Artifact / Evidence 落账（同哈希对账）----
    let (art_uri, art_sha, art_run): (String, String, String) = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT uri, sha256, run_id FROM artifact WHERE artifact_id = ?1",
                [step.artifact_id.as_deref().unwrap()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
        })
        .unwrap();
    assert_eq!(art_run, step.tool_run_id, "artifact FK 指回 tool_run");
    let (ev_sha, ev_ref, _ev_task): (String, String, Option<String>) = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT content_hash, storage_ref, task_ref FROM evidence WHERE evidence_id = ?1",
                [step.evidence_id.as_deref().unwrap()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
        })
        .unwrap();
    assert_eq!(ev_sha, art_sha, "evidence 与 artifact 同哈希");
    assert_eq!(ev_ref, art_uri, "evidence.storage_ref 指向 artifact");

    // ---- tool_run 行（经 Tool Engine 全状态机）----
    let (tr_status, tr_task): (String, Option<String>) = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT status, task_id FROM tool_run WHERE run_id = ?1",
                [&step.tool_run_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
        })
        .unwrap();
    assert_eq!(tr_status, "SUCCEEDED");
    assert_eq!(tr_task.as_deref(), Some("task-p72-accept"));

    // ---- agent_run 行：终态 + memory_refs 记账 ----
    let stored = store.run(&report.run_id).unwrap().unwrap();
    assert_eq!(stored.run.state(), domain::AgentRunState::Succeeded);
    assert!(
        stored
            .run
            .memory_refs()
            .iter()
            .any(|m| m == &format!("mem://manifest/{}", report.manifest_id)),
        "memory_refs: {:?}",
        stored.run.memory_refs()
    );
    assert!(
        stored
            .run
            .memory_refs()
            .iter()
            .any(|m| m == &format!("mem://evidence/{}", step.evidence_id.as_ref().unwrap())),
        "memory_refs: {:?}",
        stored.run.memory_refs()
    );
    assert!(stored.started_at.is_some() && stored.finished_at.is_some());

    // ---- 审计链（correlation_id = run_id）：意图 → 受理 → 步骤 → 结束 ----
    let audit = audit_engine::AuditEngine::new(pool.clone());
    let actions: Vec<String> = audit
        .trace(&report.run_id)
        .unwrap()
        .into_iter()
        .map(|s| s.event.action)
        .collect();
    for expected in [
        "context.intent.classify",
        "agent.run.accepted",
        "agent.step.completed",
        "agent.run.finished",
    ] {
        assert!(
            actions.iter().any(|a| a == expected),
            "audit chain missing {expected}: {actions:?}"
        );
    }
}

/// 能力绑定外的规划步骤被护栏拒绝：run 落 FAILED 并在报告留痕。
#[test]
fn guardrail_rejects_step_tool_outside_capability_refs() {
    let pool = migrated_pool();
    register_test_tool(&pool);
    // 绑定另一个已登记工具（不在绑定面）：ghost 目标不存在则引用校验先拒，
    // 这里用「存在但未绑定」验证运行期护栏面。
    let manifest_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../manifests/tools/rip.doc.parse.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(manifest_path).unwrap()).unwrap();
    Registry::open(pool.clone()).register(&manifest).unwrap();
    pool.with_conn(|c| {
        c.execute(
            "UPDATE tool SET lifecycle = 'ENABLED' WHERE tool_id = 'rip.doc.parse'",
            [],
        )
    })
    .unwrap();

    let store = AgentStore::open(pool.clone(), Registry::open(pool.clone()));
    let definition = AgentDefinition::try_new(
        AgentId::try_new("agent.narrow").unwrap(),
        "0.1.0".to_string(),
        "{}".to_string(),
        vec![CapabilityRef::try_new("rip.kb.keyword_search", None).unwrap()],
    )
    .unwrap();
    store
        .register_definition(&definition, "窄绑定 Agent")
        .unwrap();

    let engine = Engine::open(pool.clone(), Registry::open(pool.clone()));
    let loop_ = AgentLoop::new(
        store.clone(),
        build_broker_link(&pool),
        ToolLink::new(engine, Box::new(MockDriver::succeed(json!({}), "unused"))),
        EvidenceRegistrar::new(pool.clone()),
        None,
        LoopConfig::default(),
    );

    // 直接给规划器一个绑定外步骤（绕过 KeywordSearchPlanner 的绑定闸）。
    struct RoguePlanner;
    impl agent_engine::agent_loop::Planner for RoguePlanner {
        fn plan(
            &self,
            _ctx: &agent_engine::agent_loop::PlanContext<'_>,
        ) -> agent_engine::agent_loop::Decision {
            agent_engine::agent_loop::Decision::Steps(vec![agent_engine::agent_loop::PlanStep {
                tool_id: "rip.doc.parse".to_string(),
                version: None,
                input: json!({"text": "x"}),
                rationale: "绑定外步骤（护栏用例）".to_string(),
            }])
        }
    }
    struct DeclineConfirmer;
    impl agent_engine::broker_link::IntentConfirmer for DeclineConfirmer {
        fn confirm(&mut self, _alternatives: &[String], _goal: &str) -> Option<String> {
            None
        }
    }
    let report = loop_
        .run(&mut RunRequest {
            agent_id: "agent.narrow".to_string(),
            agent_version: None,
            goal: "zzzq 无关词串".to_string(),
            task_id: None,
            trace_id: None,
            input: None,
            planner: Box::new(RoguePlanner),
            confirmer: Box::new(DeclineConfirmer),
        })
        .unwrap();
    assert_eq!(report.state, "FAILED", "report: {report:?}");
    assert!(
        report.finish_reason.contains("not in capability_refs"),
        "finish_reason: {}",
        report.finish_reason
    );
    assert!(report.steps.is_empty(), "绑定外步骤不得执行");
    let stored = store.run(&report.run_id).unwrap().unwrap();
    assert_eq!(stored.run.state(), domain::AgentRunState::Failed);
}
