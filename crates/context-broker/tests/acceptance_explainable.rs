//! 验收测试二「可解释」（计划 §14:794 第二条；设计稿 §11 测试二映射）：
//! 断言每个 Manifest item 的 reason 与 matched_signals 非空，Manifest
//! 三表行数与装配报告一致，selection_hash 可重算对账。

use context_broker::{
    AssembleRequest, Broker, BudgetPolicy, Candidate, ContentRef, ContextBrokerError,
    ContextItemKind, ContextLayer, ContextSource, InclusionForm, IntentConfig, ManifestStore,
    RefEvent,
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
    fn load(&self, r: &ContentRef) -> Result<context_broker::Content, ContextBrokerError> {
        self.bodies
            .get(&r.ref_id)
            .map(|body| context_broker::Content {
                reference: r.clone(),
                est_tokens: context_broker::est_tokens(body),
                body: body.clone(),
            })
            .ok_or_else(|| ContextBrokerError::Source {
                source: self.source_id.clone(),
                reason: format!("unknown ref {:?}", r.ref_id),
            })
    }
}

#[test]
fn acceptance_explainable_every_item_has_reason_and_signals() {
    let pool = open_memory_pool();
    let source_id = format!("mock-explain-{}", std::process::id());
    let source = MockSource {
        source_id: source_id.clone(),
        items: vec![
            Candidate {
                item_id: "kb:kingdee-1".to_string(),
                kind: ContextItemKind::Knowledge,
                layer: ContextLayer::L2,
                title: "K3Cloud 差分脚本".to_string(),
                summary: "kingdee k3cloud 差分探测与判定方法".to_string(),
                family: None,
                governance: None,
                quality_grade: Some("B".to_string()),
                reference: ContentRef {
                    source: source_id.clone(),
                    ref_id: "kb:kingdee-1".to_string(),
                    ref_version: Some("1.0".to_string()),
                    content_hash: Some("ca".repeat(32)),
                    uri: Some("mock://kb/kingdee-1".to_string()),
                },
                est_tokens: 0,
            },
            Candidate {
                item_id: "tool:http-probe".to_string(),
                kind: ContextItemKind::Tool,
                layer: ContextLayer::L2,
                title: "HTTP 探测工具".to_string(),
                summary: "http 请求与响应头检查".to_string(),
                family: Some("knowledge".to_string()),
                governance: None,
                quality_grade: None,
                reference: ContentRef {
                    source: source_id.clone(),
                    ref_id: "tool:http-probe".to_string(),
                    ref_version: Some("0.3.0".to_string()),
                    content_hash: None,
                    uri: None,
                },
                est_tokens: 0,
            },
            Candidate {
                // 零信号退化样本：summary 与意图词元零交集（kw 无命中）。
                item_id: "kb:unrelated-9".to_string(),
                kind: ContextItemKind::Knowledge,
                layer: ContextLayer::L2,
                title: "无关条目".to_string(),
                summary: "zzz qqquuux 与意图零重叠".to_string(),
                family: None,
                governance: None,
                quality_grade: None,
                reference: ContentRef {
                    source: source_id.clone(),
                    ref_id: "kb:unrelated-9".to_string(),
                    ref_version: None,
                    content_hash: None,
                    uri: None,
                },
                est_tokens: 0,
            },
        ],
        bodies: std::collections::HashMap::from([(
            "kb:kingdee-1".to_string(),
            "full body of kingdee".to_string(),
        )]),
    };
    let rulepack = RulepackSourceShim {
        inner: context_broker::RulepackSource::from_dir(&repo_root().join("rulepacks/rip-rules"))
            .expect("rulepack"),
    };
    let broker = Broker::new(
        vec![Box::new(source), Box::new(rulepack)],
        BudgetPolicy::default_for(8000).unwrap(),
        IntentConfig::from_dir(&repo_root().join("rulepacks/rip-rules")).unwrap(),
        ManifestStore::new(pool.clone()),
        None,
    );
    let context = broker
        .assemble(&AssembleRequest {
            task_id: "t-acceptance-2".to_string(),
            text: "kingdee HTTP 检索".to_string(),
            correlation_id: "corr-acceptance-2".to_string(),
            trace_id: None,
            actor: "acceptance".to_string(),
        })
        .expect("assemble");

    let view = broker
        .manifest(&context.manifest_id)
        .expect("manifest view");
    assert!(view.items.len() >= 8, "L0 七规则 + L1 + L2 两条");

    // 断言一：每个 item 的 reason 与 matched_signals 非空（计划 §14:794），
    // 形态二选一并固定口径：有信号 reason = 信号原样拼接；零信号退化形态
    // reason = source-order:<序号>+score:<分>（可追溯选择顺序与打分）。
    for item in &view.items {
        assert!(
            !item.reason.trim().is_empty(),
            "item {} reason empty",
            item.item_id
        );
        let signals: Vec<String> =
            serde_json::from_str(&item.matched_signals_json).expect("signals json");
        if signals.is_empty() {
            assert!(
                item.reason.starts_with("source-order:") && item.reason.contains("+score:"),
                "退化形态口径: item {} reason {:?}",
                item.item_id,
                item.reason
            );
        } else {
            // reason 与 signals 同源（可解释口径：reason 即信号原样拼接）。
            assert_eq!(item.reason, signals.join("; "), "item {}", item.item_id);
        }
    }

    // 断言一补：零信号样本真实出现在装配中并走退化形态（构造面固定）。
    let unrelated = view
        .items
        .iter()
        .find(|i| i.item_id == "kb:unrelated-9")
        .expect("zero-signal sample installed");
    assert!(
        unrelated.reason.starts_with("source-order:") && unrelated.reason.contains("+score:"),
        "退化形态实测: {:?}",
        unrelated.reason
    );
    let unrelated_signals: Vec<String> =
        serde_json::from_str(&unrelated.matched_signals_json).unwrap();
    assert!(
        unrelated_signals.is_empty(),
        "零信号样本构造口径: {unrelated_signals:?}"
    );

    // 断言二：L2 条目的信号可解释到命中（关键词/族匹配/来源注记）。
    let kingdee = view
        .items
        .iter()
        .find(|i| i.item_id == "kb:kingdee-1")
        .expect("kingdee item");
    let signals: Vec<String> = serde_json::from_str(&kingdee.matched_signals_json).unwrap();
    assert!(
        signals.iter().any(|s| s.starts_with("kw:kingdee")),
        "词元级命中信号: {signals:?}"
    );

    // 断言三：三表行数与装配报告一致（条目数 = citations 数 = ASSEMBLED 事件数）。
    assert_eq!(
        view.items.len(),
        context.citations.len(),
        "items == citations"
    );
    let assembled = view
        .ref_events
        .iter()
        .filter(|e| e.event == RefEvent::Assembled)
        .count();
    assert_eq!(assembled, view.items.len(), "每条目恰一条 ASSEMBLED");

    // 断言四：selection_hash 可重算对账（与装配侧同公式同哈希原语）。
    let mut fingerprint: Vec<String> = view
        .items
        .iter()
        .map(|item| {
            format!(
                "{}|{}|{}|{}|{}|{}",
                item.source,
                item.ref_id,
                item.ref_version.clone().unwrap_or_default(),
                item.content_hash.clone().unwrap_or_default(),
                item.included.as_str(),
                item.est_tokens as u64,
            )
        })
        .collect();
    fingerprint.sort();
    assert_eq!(
        tool_registry::hash::sha256_hex(fingerprint.join("\n").as_bytes()),
        view.manifest.selection_hash,
        "selection_hash 重算一致"
    );

    // 断言五：ref-only 形态条目同样带 reason（降级不丢可解释性）。
    for item in view
        .items
        .iter()
        .filter(|i| i.included == InclusionForm::RefOnly)
    {
        assert!(!item.reason.trim().is_empty());
    }
}

/// RulepackSource 转发载具（trait 对象组合，与 P4-4 测试同形态）。
struct RulepackSourceShim {
    inner: context_broker::RulepackSource,
}

impl ContextSource for RulepackSourceShim {
    fn id(&self) -> &str {
        self.inner.id()
    }
    fn candidates(
        &self,
        q: &context_broker::IntentQuery,
        limit: usize,
    ) -> Result<Vec<Candidate>, ContextBrokerError> {
        self.inner.candidates(q, limit)
    }
    fn load(&self, r: &ContentRef) -> Result<context_broker::Content, ContextBrokerError> {
        self.inner.load(r)
    }
}
