//! P7-3 压缩/驱逐契约验收（编排批次第 3/7 批自验项；ROADMAP §8.3 P7-4
//! 判据「压缩/驱逐后 Evidence 与 Artifact 引用仍可解析（content_hash 校验
//! 通过）」+ 计划 §9:474 原文「原始证据不能因上下文压缩而丢失」）：
//!
//! 1. 内置测试 Agent（真实绑定 + Mock 驱动）挂三类 Memory 跑完整闭环；
//! 2. 三类记忆落账核对（Task/Evidence 台账/Research 回链）；
//! 3. 执行压缩（Task + Evidence + Research）与驱逐（Task）；
//! 4. [`verify_run_memory_integrity`] 全量核对：全部 Evidence 与 Artifact
//!    引用解析成功且 content_hash 校验通过（落盘产物逐字节复算）。
//!
//! 全程内存库 + 真实 migrations（001 到 013），不用手写假件。

#![deny(missing_docs)]

use agent_engine::agent_loop::{AgentLoop, KeywordSearchPlanner, LoopConfig, RunRequest};
use agent_engine::broker_link::{BrokerLink, FirstFamilyConfirmer};
use agent_engine::evidence::EvidenceRegistrar;
use agent_engine::memory::{verify_run_memory_integrity, MemoryStore};
use agent_engine::store::AgentStore;
use agent_engine::tool_link::{MockDriver, ToolLink};
use context_broker::{
    ArtifactSource, Broker, ManifestStore, PluginSource, RegistrySource, RulepackSource,
};
use database::{DbPath, Pool, PoolConfig};
use domain::{AgentDefinition, AgentId, CapabilityRef, MemoryLifecycle, MemoryRefType};
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

/// SK-1 形态种子（同 closed_loop_p7_2 口径）：插件登记行 + APPROVED 知识条目。
fn seed_plugin_knowledge(pool: &Pool) {
    pool.with_conn(|c| -> database::rusqlite::Result<()> {
        c.execute(
            "INSERT INTO plugin (plugin_id, version, name, kind, description, license, trust_level,
             state, source_type, source_ref, manifest_json, created_at, updated_at, is_active)
             VALUES ('sk.test.recon','0.1.0','Test Recon Knowledge','knowledge','d','MIT','local',
             'ENABLED','local_dir','.','{}','2026-11-13T00:00:00Z','2026-11-13T00:00:00Z',1)",
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
             '2026-11-13T00:00:00Z','2026-11-13T00:00:00Z','APPROVED')",
            [],
        )
        .unwrap();
        Ok(())
    })
    .unwrap();
}

/// 真实登记 Manifest 并置 ENABLED（Broker 目录面可见口径）。
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
fn compression_and_eviction_keep_all_evidence_and_artifact_refs_resolvable() {
    let pool = migrated_pool();
    seed_plugin_knowledge(&pool);
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
        .register_definition(&definition, "最小研究 Agent（P7-3 契约测试）")
        .unwrap();

    // 落盘产物目录：disk-hash 复算才有实体可核（引用形态如实降级为标注）。
    let artifacts_dir =
        std::env::temp_dir().join(format!("rip-agent-memory-contract-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&artifacts_dir);

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
        EvidenceRegistrar::new(pool.clone()).with_artifacts_dir(artifacts_dir.clone()),
        Some(audit_engine::AuditEngine::new(pool.clone())),
        LoopConfig::default(),
    )
    .with_memory(MemoryStore::open(pool.clone()));

    let confirmer_config = context_broker::IntentConfig::from_dir(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../rulepacks/rip-rules"),
    )
    .unwrap();
    let report = loop_
        .run(&mut RunRequest {
            agent_id: "agent.research.minimal".to_string(),
            agent_version: None,
            goal: "检索 nmap 端口扫描相关知识并调用登记工具".to_string(),
            task_id: Some("task-p73-contract".to_string()),
            trace_id: None,
            input: None,
            planner: Box::new(KeywordSearchPlanner),
            confirmer: Box::new(FirstFamilyConfirmer::new(confirmer_config)),
        })
        .unwrap();
    assert_eq!(report.state, "SUCCEEDED", "warnings: {:?}", report.warnings);
    let run_id = report.run_id.clone();

    // 落账核对（压缩前）：三类记忆各自在场。
    let memory = MemoryStore::open(pool.clone());
    let task_rows = memory.task().list_by_run(&run_id).unwrap();
    assert_eq!(task_rows.len(), report.steps.len(), "每步一条 Task Memory");
    let ledger_rows = memory.evidence().list_by_run(&run_id).unwrap();
    assert_eq!(
        ledger_rows.len(),
        report.steps.len(),
        "每步一条证据台账指针"
    );
    let research_rows = memory.research().list_by_run(&run_id).unwrap();
    assert_eq!(
        research_rows.len(),
        1,
        "run 收尾落一条 Research Memory（Context Memory 雏形）"
    );
    assert_eq!(
        report.research_memory_id.as_deref(),
        Some(research_rows[0].entry.memory_id()),
        "报告回指 Research Memory id"
    );
    assert!(
        research_rows[0]
            .entry
            .refs()
            .iter()
            .any(|r| r.ref_type() == MemoryRefType::ManifestItem),
        "Research Memory 必须带 manifest item 回链（可追溯面）"
    );
    // run memory_refs 追加 mem://memory/{id}。
    let stored_run = store.run(&run_id).unwrap().unwrap();
    assert!(
        stored_run
            .run
            .memory_refs()
            .iter()
            .any(|r| r == &format!("mem://memory/{}", research_rows[0].entry.memory_id())),
        "agent_run.memory_refs 应含 Research Memory 引用"
    );

    // 压缩/驱逐执行（契约用例本体）：Task 压缩 + 驱逐、Evidence 台账压缩、
    // Research 压缩。evidence/artifact 既有表零改动由完整性核对兜底。
    assert_eq!(
        memory.task().compress_run(&run_id).unwrap(),
        task_rows.len()
    );
    assert_eq!(memory.task().evict_run(&run_id).unwrap(), task_rows.len());
    assert_eq!(
        memory.evidence().compress_run(&run_id).unwrap(),
        ledger_rows.len()
    );
    memory
        .research()
        .compress(research_rows[0].entry.memory_id())
        .unwrap();

    // 压缩/驱逐后的状态断言：Task 墓碑、Research 详情置空但回链原样。
    for row in memory.task().list_by_run(&run_id).unwrap() {
        assert_eq!(row.entry.lifecycle(), MemoryLifecycle::Evicted);
        assert!(row.entry.content_json().is_empty() && row.entry.summary().is_empty());
    }
    let compressed = memory
        .research()
        .get(research_rows[0].entry.memory_id())
        .unwrap()
        .unwrap();
    assert_eq!(compressed.entry.lifecycle(), MemoryLifecycle::Compressed);
    assert!(compressed.entry.content_json().is_empty());
    assert_eq!(
        compressed.entry.refs().len(),
        research_rows[0].entry.refs().len(),
        "压缩不动回链"
    );

    // 契约判定（013 头注 + 计划 §9:478）：全部 Evidence 与 Artifact 引用
    // 解析成功且 content_hash 校验通过。
    let integrity = verify_run_memory_integrity(&pool, &run_id).unwrap();
    assert!(
        integrity.is_ok(),
        "integrity failures: {:?}",
        integrity.failures
    );
    assert!(
        integrity.evidence_ledger_checked >= 1,
        "证据台账必须至少核对一条"
    );
    assert!(
        integrity.refs_checked >= 2,
        "回链解析应覆盖 manifest item 与 artifact/evidence"
    );

    // 落盘产物逐字节复算的实体证据：磁盘文件在场且解析结论为 Some(true)。
    let resolved = memory
        .evidence()
        .resolve(ledger_rows[0].entry.memory_id())
        .unwrap();
    assert_eq!(
        resolved.disk_hash_verified,
        Some(true),
        "落盘产物必须逐字节复算通过（detail: {}）",
        resolved.detail
    );

    let _ = std::fs::remove_dir_all(&artifacts_dir);
}

#[test]
fn ledger_and_research_memory_refuse_eviction_by_contract() {
    // 域跃迁表是契约的第一道闸：Evidence/Research 记忆对 EVICTED 无合法对
    // （013 CHECK 词值兜底为第二道）。
    assert!(!MemoryLifecycle::Active
        .can_transition_kind(domain::MemoryKind::Evidence, MemoryLifecycle::Evicted));
    assert!(!MemoryLifecycle::Compressed
        .can_transition_kind(domain::MemoryKind::Research, MemoryLifecycle::Evicted));
    // 台账存储无驱逐 API：compile-time 契约由域跃迁表承载，这里做词值面断言。
    let entry = domain::EvidenceMemoryEntry::try_new(
        "mem-e-contract",
        domain::RunId::try_new("arn-contract").unwrap(),
        "ev-x",
        None,
        "a".repeat(64),
        "ref",
        MemoryLifecycle::Active,
    )
    .unwrap();
    assert!(entry
        .clone()
        .transition_to(MemoryLifecycle::Compressed)
        .is_ok());
    assert!(entry.transition_to(MemoryLifecycle::Evicted).is_err());
}
