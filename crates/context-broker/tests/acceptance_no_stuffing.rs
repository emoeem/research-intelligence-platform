//! 验收测试一「不默认塞入」（计划 §14:794 第一条；设计稿 §11 测试一映射）：
//! 登记全量工具面（20 个，TOOL-INVENTORY T01 到 T10 已登记 14 + Phase 3
//! 两包 6）与 RTIS 全库量级候选（14717 条，knowledge.db poc 表实查口径），
//! 装配后断言上下文只含目录卡片与少量摘要，used 总量不超过预算。

use context_broker::{
    AssembleRequest, Broker, BudgetPolicy, Candidate, ContentRef, ContextBrokerError,
    ContextItemKind, ContextLayer, ContextSource, IntentConfig, ManifestIndex, ManifestStore,
    RegistrySource, RulepackSource,
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

/// 登记面目录（P4-2 同口径：manifests/tools 14 + Phase 3 两包 6）。
fn manifest_dirs() -> Vec<PathBuf> {
    let root = repo_root();
    vec![
        root.join("manifests/tools"),
        root.join("containerfiles/rip-tool-webresearch"),
        root.join("containerfiles/rip-tool-coreutils"),
    ]
}

/// RTIS 全库量级的 Mock 知识源（14717 条候选，poc 表实查口径；Mock 仅测试
/// 载具，P4-0 决议 2；真实 RTIS 只读链路已在 P4-3 rtis_source 实测）。
struct RtisScaleSource {
    source_id: String,
    count: usize,
}

impl ContextSource for RtisScaleSource {
    fn id(&self) -> &str {
        &self.source_id
    }
    fn candidates(
        &self,
        _q: &context_broker::IntentQuery,
        limit: usize,
    ) -> Result<Vec<Candidate>, ContextBrokerError> {
        // 源级分页上限内返回（模拟 rt.py poc search --limit 语义）。
        Ok((0..self.count.min(limit))
            .map(|i| {
                let summary = format!("kingdee 相关条目 {i}：POC/EXP/案例摘要正文");
                Candidate {
                    item_id: format!("kb:{i}"),
                    kind: ContextItemKind::Knowledge,
                    layer: ContextLayer::L2,
                    title: format!("rtis entry {i}"),
                    summary,
                    family: None,
                    governance: None,
                    quality_grade: None,
                    reference: ContentRef {
                        source: self.source_id.clone(),
                        ref_id: format!("kb:{i}"),
                        ref_version: None,
                        content_hash: None,
                        uri: Some(format!("rtis://poc/{i}")),
                    },
                    est_tokens: 0,
                }
            })
            .collect())
    }
    fn load(&self, _r: &ContentRef) -> Result<context_broker::Content, ContextBrokerError> {
        Err(ContextBrokerError::Source {
            source: self.source_id.clone(),
            reason: "acceptance test does not expand".to_string(),
        })
    }
}

#[test]
fn acceptance_no_stuffing_full_surface_and_rtis_scale() {
    let pool = open_memory_pool();
    // 登记全量 20 工具面（九条校验全过）。
    let registry = tool_registry::Registry::open(pool.clone());
    let mut registered = 0usize;
    for dir in manifest_dirs() {
        let entries = std::fs::read_dir(&dir).expect("manifest dir");
        for entry in entries {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let doc: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).expect("read manifest"))
                    .expect("parse manifest");
            registry.register(&doc).expect("register (nine checks)");
            registered += 1;
        }
    }
    assert_eq!(
        registered, 20,
        "登记面口径（T01..T10 的 14 + Phase 3 两包 6）"
    );
    pool.with_conn(|c| c.execute("UPDATE tool SET lifecycle = 'ENABLED'", []))
        .unwrap();

    let rulepack = RulepackSource::from_dir(&repo_root().join("rulepacks/rip-rules")).unwrap();
    let registry_source = RegistrySource::new(
        registry,
        ManifestIndex::from_dirs(&manifest_dirs()).expect("index"),
    );
    let rtis = RtisScaleSource {
        source_id: "rtis".to_string(),
        count: 14717, // knowledge.db poc 表实查口径（P4-3 裁定）
    };

    let broker = Broker::new(
        vec![
            Box::new(rulepack),
            Box::new(registry_source),
            Box::new(rtis),
        ],
        BudgetPolicy::default_for(8000).unwrap(),
        IntentConfig::from_dir(&repo_root().join("rulepacks/rip-rules")).unwrap(),
        ManifestStore::new(pool.clone()),
        None,
    );
    let context = broker
        .assemble(&AssembleRequest {
            task_id: "t-acceptance-1".to_string(),
            text: "kingdee HTTP 检索".to_string(),
            correlation_id: "corr-acceptance-1".to_string(),
            actor: "acceptance".to_string(),
        })
        .expect("assemble");

    // 断言一：used 总量不超过预算（分项和 ≤ 总量）。
    let report = &context.budget_report;
    let used_sum = report.used.l0_rules
        + report.used.l1_catalog
        + report.used.l2_knowledge
        + report.used.l2_tools
        + report.used.l3_reserved
        + report.used.memory_history
        + report.used.reserve_output;
    assert!(
        used_sum <= report.total_tokens,
        "used {used_sum} <= {}",
        report.total_tokens
    );
    for (name, used, alloc) in [
        ("l0_rules", report.used.l0_rules, report.allocated.l0_rules),
        (
            "l1_catalog",
            report.used.l1_catalog,
            report.allocated.l1_catalog,
        ),
        (
            "l2_knowledge",
            report.used.l2_knowledge,
            report.allocated.l2_knowledge,
        ),
        ("l2_tools", report.used.l2_tools, report.allocated.l2_tools),
        (
            "l3_reserved",
            report.used.l3_reserved,
            report.allocated.l3_reserved,
        ),
    ] {
        assert!(used <= alloc, "{name} used {used} > alloc {alloc}");
    }

    // 断言二：上下文只含目录卡片与少量摘要（L1 是单项卡片；L2 条目数有界）。
    let l1_items = context
        .citations
        .iter()
        .filter(|c| c.item_id == "catalog:digest")
        .count();
    assert_eq!(l1_items, 1, "L1 为单项目录摘要");
    let l2_count =
        context.layers.l2.knowledge_summaries.len() + context.layers.l2.tool_briefs.len();
    assert!(
        l2_count <= 100,
        "L2 条目数受源级分页与预算闸约束（少量摘要，非全库），实际 {l2_count}"
    );
    assert!(
        l2_count * 100 < 14717,
        "RTIS 全库量级候选绝不整体进入上下文（{} << 14717）",
        l2_count
    );

    // 断言三：无工具 Schema 全文内联（L3 才有 Schema，expand 记账面）。
    let serialized = serde_json::to_string(&context).unwrap();
    assert!(
        !serialized.contains("input_schema"),
        "工具 Schema 全文不得默认塞入上下文"
    );
    // RTIS 候选仅摘要形态进入（每条恰出现一次，数量受预算约束）；
    // knowledge.db 全文（正文载荷）不经装配进上下文（L3 expand 面）。
    let marker_count = serialized.matches("POC/EXP/案例摘要正文").count();
    assert_eq!(
        marker_count,
        context.layers.l2.knowledge_summaries.len(),
        "RTIS 候选仅以摘要形态逐条进入（{marker_count} 条）"
    );
}
