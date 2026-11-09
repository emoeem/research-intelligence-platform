//! 验收测试三「Artifact 引用」（计划 §14:794 第三条；设计稿 §11 测试三映射）：
//! Mock Run 产出 Artifact 后，断言上下文中只有引用与哈希、无正文内联；
//! 全文经 expand 记账按需取得。

use context_broker::{
    ArtifactSource, AssembleRequest, Broker, BudgetPolicy, Candidate, ContentRef,
    ContextBrokerError, ContextItemKind, ContextLayer, ContextSource, InclusionForm, IntentConfig,
    ManifestStore,
};
use database::{DbPath, Pool, PoolConfig};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root must exist")
}

fn open_memory_pool() -> Pool {
    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("memory pool");
    database::migrate(&pool, &repo_root().join("migrations")).expect("migrations");
    pool
}

struct KbSource {
    source_id: String,
}

impl ContextSource for KbSource {
    fn id(&self) -> &str {
        &self.source_id
    }
    fn candidates(
        &self,
        _q: &context_broker::IntentQuery,
        _limit: usize,
    ) -> Result<Vec<Candidate>, ContextBrokerError> {
        Ok(vec![Candidate {
            item_id: "kb:1".to_string(),
            kind: ContextItemKind::Knowledge,
            layer: ContextLayer::L2,
            title: "kingdee case".to_string(),
            summary: "kingdee 案例摘要".to_string(),
            family: None,
            governance: None,
            quality_grade: None,
            reference: ContentRef {
                source: self.source_id.clone(),
                ref_id: "kb:1".to_string(),
                ref_version: None,
                content_hash: None,
                uri: None,
            },
            est_tokens: 0,
        }])
    }
    fn load(&self, _r: &ContentRef) -> Result<context_broker::Content, ContextBrokerError> {
        Err(ContextBrokerError::Source {
            source: self.source_id.clone(),
            reason: "not expanded in this test".to_string(),
        })
    }
}

#[test]
fn acceptance_artifact_ref_only_reference_and_hash_in_context() {
    let pool = open_memory_pool();
    // 005 数据面：Mock Run 产出 Artifact（role=primary，sha256 登记）。
    pool.with_conn(|c| {
        c.execute(
            "INSERT INTO tool (tool_id, name, governance, created_at, updated_at)
             VALUES ('tool:mock-http', 'mock http tool', 'ACTIVE', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )?;
        c.execute(
            "INSERT INTO tool_run (run_id, tool_id, workspace_id, status, created_at, updated_at)
             VALUES ('rn-acc-3', 'tool:mock-http', 'ws-acc', 'SUCCEEDED', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )?;
        c.execute(
            "INSERT INTO artifact (artifact_id, run_id, role, uri, sha256, size_bytes, mime_type, provenance, created_at)
             VALUES ('art-acc-3', 'rn-acc-3', 'primary', 'file:///tmp/art-acc-3.bin', ?, 4096, 'application/octet-stream', 'tool output body <HTML>SECRET</HTML>', '2026-01-01T00:00:00Z')",
            ["ef".repeat(32)],
        )?;
        Ok::<(), database::rusqlite::Error>(())
    })
    .unwrap();

    let artifact_source = ArtifactSource::new(pool.clone());
    let kb = KbSource {
        source_id: format!("mock-kb-{}", std::process::id()),
    };
    let broker = Broker::new(
        vec![Box::new(kb), Box::new(artifact_source)],
        BudgetPolicy::default_for(8000).unwrap(),
        IntentConfig::from_dir(&repo_root().join("rulepacks/rip-rules")).unwrap(),
        ManifestStore::new(pool.clone()),
        None,
    );
    let context = broker
        .assemble(&AssembleRequest {
            task_id: "t-acceptance-3".to_string(),
            text: "kingdee 检索".to_string(),
            correlation_id: "corr-acceptance-3".to_string(),
            actor: "acceptance".to_string(),
        })
        .expect("assemble");

    // 断言一：上下文含 Artifact 引用（l3_refs + citations，sha256 齐全），
    // 引用形态为 ref-only（不删 item，降级链终点）。
    assert!(
        context
            .layers
            .l3_refs
            .iter()
            .any(|r| r == "artifact:art-acc-3"),
        "l3_refs: {:?}",
        context.layers.l3_refs
    );
    let citation = context
        .citations
        .iter()
        .find(|c| c.ref_id == "artifact:art-acc-3")
        .expect("artifact citation present");
    assert_eq!(citation.source, "artifact-store");
    assert_eq!(
        citation.content_hash.as_deref(),
        Some("ef".repeat(32).as_str()),
        "哈希进引用（来源追踪，计划 §12:703）"
    );
    assert_eq!(citation.uri.as_deref(), Some("file:///tmp/art-acc-3.bin"));

    // 断言二：正文零内联（工具输出永不预装，设计稿 §5.3 第 7 条）。
    let serialized = serde_json::to_string(&context).unwrap();
    assert!(
        !serialized.contains("SECRET") && !serialized.contains("rn-acc-3"),
        "Run 产出正文与 run_id 不得内联进上下文"
    );
    // Manifest item 形态为 ref-only。
    let view = broker.manifest(&context.manifest_id).unwrap();
    let artifact_item = view
        .items
        .iter()
        .find(|i| i.ref_id == "artifact:art-acc-3")
        .expect("artifact manifest item");
    assert_eq!(artifact_item.included, InclusionForm::RefOnly);
    assert_eq!(artifact_item.layer, ContextLayer::L3);

    // 断言三：全文经 expand 记账取得（l3_reserved 扣账，引用与哈希保留）。
    let expanded = broker
        .expand(&context.manifest_id, "artifact:art-acc-3")
        .expect("expand artifact");
    assert!(expanded.content.body.contains("art-acc-3"));
    assert!(expanded.granted_tokens > 0);
    let expanded_again = broker
        .expand(&context.manifest_id, "artifact:art-acc-3")
        .expect("expand again");
    assert_eq!(
        expanded_again.content.body, expanded.content.body,
        "引用与哈希在展开后保持不变"
    );

    // 断言四：EXPANDED 事件落 ref_log（引用追踪闭环，设计稿 §6.2）。
    let events = broker.manifest(&context.manifest_id).unwrap().ref_events;
    assert!(
        events
            .iter()
            .any(|e| e.item_id == "artifact:art-acc-3"
                && e.event == context_broker::RefEvent::Expanded),
        "events: {:?}",
        events
            .iter()
            .map(|e| (e.item_id.clone(), e.event.as_str()))
            .collect::<Vec<_>>()
    );
}
