//! P10-3/P10-4 性能基准载具（ROADMAP §11.3 P10-4「性能测试以不回归为门，
//! 对照 Phase 4 装配耗时既有记录」；docs/phase10/PERF-BASELINE.md 为基线
//! 对账文档）：Phase 4 上下文装配（context-broker L0 到 L3 装配流）耗时
//! 基准。本地执行，#[ignore] 不进常规测试流；measure.sh（autoresearch
//! 冻结载具）以 `cargo test ... -- --ignored --nocapture bench_` 调用，
//! 打印 `METRIC assemble_ms=<中位数>`。
//!
//! 口径：每次迭代独立任务文本（不命中同 content_hash 装配缓存，测冷装配
//! 全路径），数据面为 Mock 源 + rip-rules rulepack（与 P4-4 契约测试同构），
//! 内存库 + 内存迁移，无外部依赖。

use context_broker::{
    AssembleRequest, Broker, BudgetPolicy, Candidate, Content, ContentRef, ContextBrokerError,
    ContextItemKind, ContextLayer, ContextSource, IntentConfig, ManifestStore, RulepackSource,
    est_tokens,
};
use database::{DbPath, Pool, PoolConfig, migrate};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

struct MockSource {
    source_id: String,
    items: Vec<Candidate>,
    bodies: std::collections::HashMap<String, String>,
}

impl ContextSource for MockSource {
    fn id(&self) -> &str {
        &self.source_id
    }
    fn candidates(
        &self,
        _q: &context_broker::IntentQuery,
        _limit: usize,
    ) -> Result<Vec<Candidate>, ContextBrokerError> {
        Ok(self.items.clone())
    }
    fn load(&self, r: &ContentRef) -> Result<Content, ContextBrokerError> {
        if r.source != self.source_id {
            return Err(ContextBrokerError::Source {
                source: self.source_id.clone(),
                reason: format!("foreign ref {:?}", r.source),
            });
        }
        self.bodies
            .get(&r.ref_id)
            .map(|body| Content {
                reference: r.clone(),
                est_tokens: est_tokens(body),
                body: body.clone(),
            })
            .ok_or_else(|| ContextBrokerError::Source {
                source: self.source_id.clone(),
                reason: format!("unknown ref {:?}", r.ref_id),
            })
    }
}

fn candidate(
    source: &str,
    item_id: &str,
    kind: ContextItemKind,
    layer: ContextLayer,
    summary: &str,
) -> Candidate {
    Candidate {
        item_id: item_id.to_string(),
        kind,
        layer,
        title: format!("title of {item_id}"),
        summary: summary.to_string(),
        family: None,
        governance: None,
        quality_grade: None,
        reference: ContentRef {
            source: source.to_string(),
            ref_id: item_id.to_string(),
            ref_version: None,
            content_hash: None,
            uri: None,
        },
        est_tokens: est_tokens(summary),
    }
}

#[test]
#[ignore]
fn bench_assemble_latency() {
    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("open memory pool");
    migrate(&pool, &repo_root().join("migrations")).expect("migrations on memory db");

    let source_id = "mock-p103-bench";
    let mut items = Vec::new();
    let mut bodies = std::collections::HashMap::new();
    for i in 0..8 {
        let id = format!("kb:{i}");
        let summary = format!(
            "knowledge item {i}: retrieval, assembly, benchmark, context broker, evidence chain. 正文段落 {i}：检索信号、装配路径、证据引用与预算驱逐规则。"
        );
        items.push(candidate(
            source_id,
            &id,
            ContextItemKind::Knowledge,
            ContextLayer::L1,
            &summary,
        ));
        bodies.insert(
            id,
            format!("full body of kb item {i}. {}", "装配基准正文，".repeat(40)),
        );
    }
    let source = MockSource {
        source_id: source_id.to_string(),
        items,
        bodies,
    };
    let rulepack = RulepackSource::from_dir(&repo_root().join("rulepacks/rip-rules")).unwrap();
    let intent = IntentConfig::from_dir(&repo_root().join("rulepacks/rip-rules")).unwrap();
    let broker = Broker::new(
        vec![Box::new(source), Box::new(rulepack)],
        BudgetPolicy::default_for(8000).unwrap(),
        intent,
        ManifestStore::new(pool.clone()),
        None,
    );

    let iterations = 20;
    let mut samples_ms: Vec<f64> = Vec::with_capacity(iterations);
    for i in 0..iterations {
        let req = AssembleRequest {
            task_id: format!("t-p103-bench-{i}"),
            text: format!("kingdee 检索 装配 基准 iteration {i} evidence"),
            correlation_id: format!("corr-p103-bench-{i}"),
            trace_id: None,
            actor: "p10-3-bench".to_string(),
        };
        let start = std::time::Instant::now();
        let manifest = broker.assemble(&req).expect("assemble must succeed");
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        assert!(
            !manifest.citations.is_empty(),
            "bench assumes a populated assembly"
        );
        samples_ms.push(elapsed_ms);
    }
    samples_ms.sort_by(|a, b| a.partial_cmp(b).expect("finite floats"));
    let median = samples_ms[samples_ms.len() / 2];
    println!("METRIC assemble_ms={median:.3}");
    println!(
        "bench_assemble_latency: n={} median_ms={median:.3} min_ms={:.3} max_ms={:.3}",
        iterations,
        samples_ms[0],
        samples_ms[iterations - 1]
    );
}
