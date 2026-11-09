//! P4-4 验证载具（M4.3 判据）：装配全流程契约。覆盖：
//! 1. 同一任务两次装配产生两个 manifest_id 且 selection_hash 一致（开发库
//!    副本落库 + mode=ro 对账，判据原句）；
//! 2. expand 写 context_ref_log 并从 l3_reserved 扣账返回增量 tokens，额度
//!    不足拒绝并附替代建议；
//! 3. 三表只追加不更新（重复主键即拒；模块内零 UPDATE）；
//! 4. 同 content_hash 只装一次；
//! 5. 内联量超 budget_report.used 即装配失败（fail fast，设计稿 §9.2 第 5 条）；
//! 6. 驱逐顺序（低分先逐）与降级链（ref-only 不删 item）；
//! 7. citations[] 与 l2 摘要分离且 source/version/content_hash/uri 齐全；
//! 8. Artifact 只读引用源（005 tool_run/artifact 数据面，正文不内联）。
//!
//! 库写只落开发库副本（migrations/dev-p4.db，gitignored）与测试内存库；
//! 生产 platform*.db 零写入（红线）。

use context_broker::{
    ArtifactSource, AssembleRequest, Broker, BudgetPolicy, Candidate, Content, ContentRef,
    ContextBrokerError, ContextItemKind, ContextLayer, ContextSource, IntentConfig, ManifestStore,
    RefEvent, RulepackSource,
};
use database::{DbPath, Pool, PoolConfig};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
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
    .expect("open memory pool");
    database::migrate(&pool, &repo_root().join("migrations")).expect("migrations on memory db");
    pool
}

fn load_intent_config() -> IntentConfig {
    IntentConfig::from_dir(&repo_root().join("rulepacks/rip-rules")).expect("intent config")
}

fn policy_for(total: u64) -> BudgetPolicy {
    BudgetPolicy::default_for(total).expect("default policy")
}

/// 确定性 Mock 源（P4-0 决议 2 口径：Mock 仅测试载具）。
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
                est_tokens: context_broker::est_tokens(body),
                body: body.clone(),
            })
            .ok_or_else(|| ContextBrokerError::Source {
                source: self.source_id.clone(),
                reason: format!("unknown ref {:?}", r.ref_id),
            })
    }
}

/// 候选构造辅助（测试载具；参数成组语义清晰，按批内纪律显式 allow）。
#[allow(clippy::too_many_arguments)]
fn candidate(
    source: &str,
    item_id: &str,
    kind: ContextItemKind,
    layer: ContextLayer,
    summary: &str,
    content_hash: Option<&str>,
    version: Option<&str>,
    uri: Option<&str>,
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
            ref_version: version.map(str::to_string),
            content_hash: content_hash.map(str::to_string),
            uri: uri.map(str::to_string),
        },
        est_tokens: context_broker::est_tokens(summary),
    }
}

static SEQ: AtomicU64 = AtomicU64::new(0);

fn unique_source_id(tag: &str) -> String {
    format!("mock-{tag}-{}", SEQ.fetch_add(1, Ordering::Relaxed))
}

fn assemble_request(task: &str, text: &str) -> AssembleRequest {
    AssembleRequest {
        task_id: task.to_string(),
        text: text.to_string(),
        correlation_id: format!("corr-{task}"),
        actor: "p44-test".to_string(),
    }
}

// ---------------------------------------------------------------------------
// 判据 1：两次装配两个 manifest_id、selection_hash 一致（开发库副本对账）
// ---------------------------------------------------------------------------

fn dev_copy_pool() -> Pool {
    let path = repo_root().join("migrations/dev-p4.db");
    for suffix in ["", "-wal", "-shm"] {
        let mut p = path.clone().into_os_string();
        p.push(suffix);
        let p = PathBuf::from(p);
        if p.exists() {
            std::fs::remove_file(&p).unwrap();
        }
    }
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(path),
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("open dev copy");
    database::migrate(&pool, &repo_root().join("migrations")).expect("migrate dev copy");
    pool
}

#[test]
fn p4_4_assemble_twice_two_manifests_same_selection_hash_on_dev_copy() {
    let pool = dev_copy_pool();
    let source_id = unique_source_id("dup");
    let source = MockSource {
        source_id: source_id.clone(),
        items: vec![
            candidate(
                &source_id,
                "kb:1001",
                ContextItemKind::Knowledge,
                ContextLayer::L2,
                "kingdee http 差分脚本 kingdee 判定",
                Some("aabb"),
                Some("v1"),
                Some("mock://poc/1001"),
            ),
            candidate(
                &source_id,
                "tool:x",
                ContextItemKind::Tool,
                ContextLayer::L2,
                "x tool brief",
                None,
                Some("0.1.0"),
                None,
            ),
        ],
        bodies: std::collections::HashMap::from([(
            "kb:1001".to_string(),
            "full text of kb:1001".to_string(),
        )]),
    };
    let rulepack = RulepackSource::from_dir(&repo_root().join("rulepacks/rip-rules")).unwrap();
    let broker = Broker::new(
        vec![Box::new(source), Box::new(rulepack)],
        policy_for(8000),
        load_intent_config(),
        ManifestStore::new(pool.clone()),
        None,
    );

    let req = assemble_request("t-p44-dup", "kingdee http 检索");
    let first = broker.assemble(&req).expect("first assemble");
    let second = broker.assemble(&req).expect("second assemble");

    assert_ne!(
        first.manifest_id, second.manifest_id,
        "重装配出新 manifest_id"
    );
    assert_ne!(first.context_id, second.context_id);
    assert_eq!(
        first.budget_report.allocated, second.budget_report.allocated,
        "同输入同分配"
    );

    // 开发库副本 mode=ro 对账（判据原句：可对账）。
    let ro_path = repo_root().join("migrations/dev-p4.db");
    let ro = Pool::open(PoolConfig {
        path: DbPath::File(ro_path),
        readonly: true,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("open dev copy readonly");
    let rows: Vec<(String, String)> = ro
        .with_conn(|c| {
            let mut stmt = c
                .prepare("SELECT manifest_id, selection_hash FROM context_manifest ORDER BY rowid")
                .unwrap();
            let mapped = stmt
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .unwrap();
            mapped.collect::<Result<Vec<_>, _>>()
        })
        .unwrap();
    assert_eq!(rows.len(), 2, "两次装配落两行清单头");
    assert_eq!(rows[0].0, first.manifest_id);
    assert_eq!(rows[1].0, second.manifest_id);
    assert_eq!(
        rows[0].1, rows[1].1,
        "同一任务两次装配 selection_hash 一致（M4.3 判据）"
    );
    assert_eq!(rows[0].1, recomputed_selection_hash(&broker, &first));

    // 只读池写入必须被拒（mode=ro 对账面）。
    let write = ro.with_conn(|c| c.execute("DELETE FROM context_manifest", []));
    assert!(write.is_err(), "readonly pool must reject writes");
}

/// 用 manifest 视图重算 selection_hash（对账口径与装配侧同源：
/// source|ref_id|ref_version|content_hash|included|est_tokens 排序后 sha256，
/// 哈希原语复用 tool_registry::hash::sha256_hex）。
fn recomputed_selection_hash(
    broker: &Broker,
    context: &context_broker::AssembledContext,
) -> String {
    let view = broker.manifest(&context.manifest_id).unwrap();
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
    tool_registry::hash::sha256_hex(fingerprint.join("\n").as_bytes())
}

// ---------------------------------------------------------------------------
// 判据 2：expand 记账与 l3_reserved 扣账
// ---------------------------------------------------------------------------

#[test]
fn p4_4_expand_logs_ref_event_and_deducts_l3_reserved() {
    let pool = open_memory_pool();
    let source_id = unique_source_id("expand");
    let body = "x".repeat(40); // 40 ascii -> 10 tokens
    let source = MockSource {
        source_id: source_id.clone(),
        items: vec![candidate(
            &source_id,
            "doc:1",
            ContextItemKind::Doc,
            ContextLayer::L2,
            "doc summary",
            None,
            None,
            None,
        )],
        bodies: std::collections::HashMap::from([("doc:1".to_string(), body.clone())]),
    };
    // l3_reserved 有界（total 1200：l3 = 27% x 1140 = 307），40 tokens 载荷
    // 反复 expand 直至额度耗尽。
    let broker = Broker::new(
        vec![Box::new(source)],
        policy_for(1200),
        load_intent_config(),
        ManifestStore::new(pool.clone()),
        None,
    );
    let context = broker
        .assemble(&assemble_request("t-p44-expand", "doc 检索"))
        .expect("assemble");

    let expanded = broker
        .expand(&context.manifest_id, "doc:1")
        .expect("expand doc:1");
    assert_eq!(expanded.granted_tokens, 10, "40 ascii chars = 10 tokens");
    assert!(expanded.remaining_l3 > 0);
    assert!(expanded.content.body.starts_with("xxx"), "mock body 40 x");

    // context_ref_log 有 EXPANDED 事件，detail.granted_tokens = 10。
    let view = broker.manifest(&context.manifest_id).unwrap();
    let expanded_events: Vec<_> = view
        .ref_events
        .iter()
        .filter(|e| e.event == RefEvent::Expanded)
        .collect();
    assert_eq!(expanded_events.len(), 1);
    assert!(expanded_events[0]
        .detail_json
        .contains("\"granted_tokens\":10"));
    assert_eq!(
        broker
            .store()
            .expanded_tokens(&context.manifest_id)
            .unwrap(),
        10,
        "已用 l3 由 EXPANDED 事件推导（只追加语义）"
    );

    // 连续 expand 至额度不足：拒绝并返回剩余额度与替代建议（设计稿 §5.3 第 4 条）。
    let mut last_err = None;
    for _ in 0..40 {
        match broker.expand(&context.manifest_id, "doc:1") {
            Ok(e) => assert!(e.granted_tokens > 0),
            Err(e) => {
                last_err = Some(e);
                break;
            }
        }
    }
    let err = last_err.expect("tiny l3 budget must be exhausted eventually");
    let msg = err.to_string();
    assert!(msg.contains("l3_reserved"), "err: {msg}");
    assert!(msg.contains("fallback: read summary"), "err: {msg}");
}

// ---------------------------------------------------------------------------
// 判据 3：三表只追加不更新
// ---------------------------------------------------------------------------

#[test]
fn p4_4_three_tables_are_append_only() {
    let pool = open_memory_pool();
    let source_id = unique_source_id("append");
    let source = MockSource {
        source_id: source_id.clone(),
        items: vec![candidate(
            &source_id,
            "kb:1",
            ContextItemKind::Knowledge,
            ContextLayer::L2,
            "summary",
            None,
            None,
            None,
        )],
        bodies: std::collections::HashMap::new(),
    };
    let broker = Broker::new(
        vec![Box::new(source)],
        policy_for(8000),
        load_intent_config(),
        ManifestStore::new(pool.clone()),
        None,
    );
    let context = broker
        .assemble(&assemble_request("t-p44-append", "kingdee 检索"))
        .expect("assemble");

    // 重复插入同主键被拒（只追加语义的物理面）。
    let dup = pool.with_conn(|c| {
        c.execute(
            "INSERT INTO context_manifest (manifest_id, policy_version, broker_version,
               intent_status, created_at) VALUES (?1, 'p', 'b', 'ok', 'now')",
            [context.manifest_id.clone()],
        )
    });
    assert!(dup.is_err(), "duplicate manifest_id must be rejected by PK");

    // manifest.rs 源码零 UPDATE 语句（grep 级断言，编译期内嵌源码核对）。
    let source_text = include_str!("../src/manifest.rs");
    assert!(
        !source_text.to_uppercase().contains("UPDATE "),
        "manifest.rs must not issue UPDATE (append-only discipline)"
    );

    // 三表行数一致：清单 1、条目 >=1、每条目恰一条 ASSEMBLED 事件。
    let view = broker.manifest(&context.manifest_id).unwrap();
    assert!(!view.items.is_empty() && !view.ref_events.is_empty());
    let assembled: Vec<_> = view
        .ref_events
        .iter()
        .filter(|e| e.event == RefEvent::Assembled)
        .collect();
    assert_eq!(assembled.len(), view.items.len(), "每条目恰一条 ASSEMBLED");
}

// ---------------------------------------------------------------------------
// 判据 4 + 6：同 content_hash 去重 / 驱逐顺序 / 降级链
// ---------------------------------------------------------------------------

fn dup_source(source_id: &str) -> MockSource {
    MockSource {
        source_id: source_id.to_string(),
        items: vec![
            candidate(
                source_id,
                "kb:dup-a",
                ContextItemKind::Knowledge,
                ContextLayer::L2,
                "first copy",
                Some("deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"),
                None,
                None,
            ),
            candidate(
                source_id,
                "kb:dup-b",
                ContextItemKind::Knowledge,
                ContextLayer::L2,
                "second copy same hash",
                Some("deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"),
                None,
                None,
            ),
        ],
        bodies: std::collections::HashMap::new(),
    }
}

fn big_candidates_source(source_id: &str) -> MockSource {
    // 四个 800 CJK 字符候选（est 800/条）：l2_knowledge（8000 总量下 1900）
    // 装 big-1/big-2 两个 summary 后溢出，big-3/big-4 降级 ref-only。
    MockSource {
        source_id: source_id.to_string(),
        items: (1..=4)
            .map(|i| {
                candidate(
                    source_id,
                    &format!("kb:big-{i}"),
                    ContextItemKind::Knowledge,
                    ContextLayer::L2,
                    &"长".repeat(800),
                    None,
                    None,
                    None,
                )
            })
            .collect(),
        bodies: std::collections::HashMap::new(),
    }
}

#[test]
fn p4_4_same_content_hash_installed_once() {
    let pool = open_memory_pool();
    let source_id = unique_source_id("dedup");
    let broker = Broker::new(
        vec![Box::new(dup_source(&source_id))],
        policy_for(8000),
        load_intent_config(),
        ManifestStore::new(pool),
        None,
    );
    let context = broker
        .assemble(&assemble_request("t-p44-dedup", "kingdee 检索"))
        .expect("assemble");

    // content_hash deadbeef 只装一次（第二个候选去重，不产生第二个 item）。
    let dup_items: Vec<_> = context
        .layers
        .l2
        .knowledge_summaries
        .iter()
        .filter(|s| s.item_id.starts_with("kb:dup-"))
        .collect();
    assert_eq!(dup_items.len(), 1, "同 content_hash 只装一次");
    assert!(
        context
            .warnings
            .iter()
            .any(|w| w.starts_with("dedup:content_hash=")),
        "warnings: {:?}",
        context.warnings
    );
}

#[test]
fn p4_4_downgrade_chain_ref_only_keeps_item() {
    let pool = open_memory_pool();
    let source_id = unique_source_id("downgrade");
    let broker = Broker::new(
        vec![Box::new(big_candidates_source(&source_id))],
        policy_for(8000),
        load_intent_config(),
        ManifestStore::new(pool),
        None,
    );
    let context = broker
        .assemble(&assemble_request("t-p44-downgrade", "kingdee 检索"))
        .expect("assemble");

    // 降级链：big-1/big-2 装 summary（1600 <= 1900），big-3/big-4 装不下
    // 降级 ref-only（不删 item，只改 included 形态，摘要清空），无驱逐。
    let big1 = context
        .layers
        .l2
        .knowledge_summaries
        .iter()
        .find(|s| s.item_id == "kb:big-1")
        .expect("big-1 installed as summary");
    assert!(!big1.summary.is_empty(), "第一个候选摘要形态");
    let big2 = context
        .layers
        .l2
        .knowledge_summaries
        .iter()
        .find(|s| s.item_id == "kb:big-2")
        .expect("big-2 installed as summary");
    assert!(!big2.summary.is_empty());
    for id in ["kb:big-3", "kb:big-4"] {
        let ref_only = context
            .layers
            .l2
            .knowledge_summaries
            .iter()
            .find(|s| s.item_id == id)
            .unwrap_or_else(|| panic!("{id} downgraded to ref-only (item kept)"));
        assert!(ref_only.summary.is_empty(), "{id} ref-only 摘要清空");
    }
    assert!(
        context.budget_report.evicted.is_empty(),
        "ref-only 仍有额度时不驱逐: {:?}",
        context.budget_report.evicted
    );
}

#[test]
fn p4_4_eviction_order_lowest_score_first_and_buckets_isolated() {
    let pool = open_memory_pool();
    let source_id = unique_source_id("evict");
    // 三个工具候选同桶：中分者占位，低分者被逐；高分者装 summary。
    let filler = "x".repeat(2800);
    let source = MockSource {
        source_id: source_id.clone(),
        items: (0..9)
            .map(|i| {
                candidate(
                    &source_id,
                    &format!("tool:{i}"),
                    ContextItemKind::Tool,
                    ContextLayer::L2,
                    &format!("http 工具 {i} {filler}"),
                    None,
                    None,
                    None,
                )
            })
            .collect(),
        bodies: std::collections::HashMap::new(),
    };
    // l2_tools = 10% x 7600 = 760：tool:0 装 summary（约 700），tool:1 到
    // tool:7 降级 ref-only（每个 8 tokens，至 756），tool:8 连 ref-only
    // 额度也无（764 > 760）被驱逐（同分按 item_id 稳定序，尾部先逐）。
    let broker = Broker::new(
        vec![Box::new(source)],
        policy_for(8000),
        load_intent_config(),
        ManifestStore::new(pool),
        None,
    );
    let context = broker
        .assemble(&assemble_request("t-p44-evict", "http 端口扫描 http 检索"))
        .expect("assemble");

    let installed_ids: Vec<&str> = context
        .layers
        .l2
        .tool_briefs
        .iter()
        .map(|b| b.item_id.as_str())
        .collect();
    let evicted_ids: Vec<&str> = context
        .budget_report
        .evicted
        .iter()
        .map(|e| e.item_id.as_str())
        .collect();
    assert!(
        !installed_ids.is_empty() && !evicted_ids.is_empty(),
        "installed: {installed_ids:?}, evicted: {evicted_ids:?}"
    );
    assert_eq!(evicted_ids, vec!["tool:8"], "同分尾部先逐: {evicted_ids:?}");
    // 被逐条目不得同时出现在装入集。
    for id in &evicted_ids {
        assert!(
            !installed_ids.contains(id),
            "evicted {id} must not be installed"
        );
    }
    // 降级链面：tool:0 为 summary，tool:1 到 tool:7 为 ref-only（摘要清空）。
    assert!(!context
        .layers
        .l2
        .tool_briefs
        .iter()
        .find(|b| b.item_id == "tool:0")
        .unwrap()
        .summary
        .is_empty());
    for i in 1..8 {
        let brief = context
            .layers
            .l2
            .tool_briefs
            .iter()
            .find(|b| b.item_id == format!("tool:{i}"))
            .unwrap_or_else(|| panic!("tool:{i} kept as ref-only"));
        assert!(brief.summary.is_empty(), "tool:{i} ref-only 摘要清空");
    }
    // L0/L1 永不驱逐：evicted 集无 L0/L1 条目。
    assert!(
        context
            .budget_report
            .evicted
            .iter()
            .all(|e| e.kind != "rule" && !e.item_id.starts_with("catalog:")),
        "L0/L1 never evicted: {:?}",
        context.budget_report.evicted
    );
}

// ---------------------------------------------------------------------------
// 判据 5：内联超预算 fail fast
// ---------------------------------------------------------------------------

#[test]
fn p4_4_inline_over_budget_fails_fast() {
    let pool = open_memory_pool();
    let source_id = unique_source_id("flood");
    let items: Vec<Candidate> = (0..60)
        .map(|i| {
            candidate(
                &source_id,
                &format!("kb:flood-{i}"),
                ContextItemKind::Knowledge,
                ContextLayer::L2,
                &format!("flood candidate {i} with some summary text to bloat eviction records"),
                None,
                None,
                None,
            )
        })
        .collect();
    let source = MockSource {
        source_id: source_id.clone(),
        items,
        bodies: std::collections::HashMap::new(),
    };
    // 极小预算：大量驱逐记录使序列化结果远超总量，契约校验必须 fail fast
    // （设计稿 §9.2 第 5 条：不允许静默超预算）。
    let broker = Broker::new(
        vec![Box::new(source)],
        policy_for(100),
        load_intent_config(),
        ManifestStore::new(pool),
        None,
    );
    let err: ContextBrokerError = broker
        .assemble(&assemble_request("t-p44-flood", "kingdee 检索"))
        .expect_err("over-budget serialization must fail fast");
    let msg = err.to_string();
    assert!(msg.contains("budget_report.used"), "err: {msg}");
    assert!(msg.contains("fail fast"), "err: {msg}");
}

// ---------------------------------------------------------------------------
// 判据 7：citations 与摘要分离、字段齐全
// ---------------------------------------------------------------------------

#[test]
fn p4_4_citations_separated_from_summaries_with_full_fields() {
    let pool = open_memory_pool();
    let source_id = unique_source_id("cite");
    let source = MockSource {
        source_id: source_id.clone(),
        items: vec![candidate(
            &source_id,
            "kb:cite-1",
            ContextItemKind::Knowledge,
            ContextLayer::L2,
            "kingdee citation summary",
            Some("cafebabecafebabecafebabecafebabecafebabecafebabecafebabecafebabe"),
            Some("2.0.0"),
            Some("mock://kb/cite-1"),
        )],
        bodies: std::collections::HashMap::new(),
    };
    let broker = Broker::new(
        vec![Box::new(source)],
        policy_for(8000),
        load_intent_config(),
        ManifestStore::new(pool),
        None,
    );
    let context = broker
        .assemble(&assemble_request("t-p44-cite", "kingdee 检索"))
        .expect("assemble");

    // l2 摘要与 citations 分离：同一 item 两处呈现，citations 字段齐全。
    let summary = context
        .layers
        .l2
        .knowledge_summaries
        .iter()
        .find(|s| s.item_id == "kb:cite-1")
        .expect("summary present");
    assert!(!summary.summary.is_empty());
    let citation = context
        .citations
        .iter()
        .find(|c| c.item_id == "kb:cite-1")
        .expect("citation present");
    assert_eq!(citation.source, source_id);
    assert_eq!(citation.ref_id, "kb:cite-1");
    assert_eq!(citation.version.as_deref(), Some("2.0.0"));
    assert_eq!(
        citation.content_hash.as_deref(),
        Some("cafebabecafebabecafebabecafebabecafebabecafebabecafebabecafebabe")
    );
    assert_eq!(citation.uri.as_deref(), Some("mock://kb/cite-1"));
    // 下游唯一合法引用集 = citations（条目全部入集）。
    assert!(
        context.citations.iter().all(|c| context
            .layers
            .l2
            .knowledge_summaries
            .iter()
            .any(|s| s.item_id == c.item_id)
            || context
                .layers
                .l2
                .tool_briefs
                .iter()
                .any(|b| b.item_id == c.item_id)
            || c.item_id.starts_with("catalog:")
            || c.ref_id.starts_with("rp-rules:")
            || context.layers.l3_refs.contains(&c.ref_id)),
        "citations cover installed items: {:?}",
        context.citations
    );
}

// ---------------------------------------------------------------------------
// 判据 8：Artifact 只读引用源（005 数据面）
// ---------------------------------------------------------------------------

#[test]
fn p4_4_artifact_source_cites_without_inlining_body() {
    let pool = open_memory_pool();
    // 005 数据面：tool -> tool_run -> artifact 三行链。
    pool.with_conn(|c| {
        c.execute(
            "INSERT INTO tool (tool_id, name, governance, created_at, updated_at)
             VALUES ('tool:p44-art', 'p44 art', 'ACTIVE', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )?;
        c.execute(
            "INSERT INTO tool_run (run_id, tool_id, workspace_id, status, created_at, updated_at)
             VALUES ('rn-p44-art', 'tool:p44-art', 'ws-p44', 'SUCCEEDED', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )?;
        c.execute(
            "INSERT INTO artifact (artifact_id, run_id, role, uri, sha256, size_bytes, mime_type, provenance, created_at)
             VALUES ('art-p44-1', 'rn-p44-art', 'primary', 'file:///tmp/art-p44-1.bin', ?, 2048, 'application/octet-stream', 'tool output', '2026-01-01T00:00:00Z')",
            ["ab".repeat(32)],
        )?;
        Ok::<(), database::rusqlite::Error>(())
    })
    .unwrap();

    let artifact_source = ArtifactSource::new(pool.clone());
    let artifact_source_for_load = ArtifactSource::new(pool.clone());
    let source_id = unique_source_id("kb-for-art");
    let kb_source = MockSource {
        source_id: source_id.clone(),
        items: vec![candidate(
            &source_id,
            "kb:art-case",
            ContextItemKind::Knowledge,
            ContextLayer::L2,
            "kingdee 检索",
            None,
            None,
            None,
        )],
        bodies: std::collections::HashMap::new(),
    };
    let broker = Broker::new(
        vec![Box::new(kb_source), Box::new(artifact_source)],
        policy_for(8000),
        load_intent_config(),
        ManifestStore::new(pool.clone()),
        None,
    );
    let context = broker
        .assemble(&assemble_request("t-p44-art", "kingdee 检索"))
        .expect("assemble");

    // Artifact 引用进上下文：l3_refs + citations，正文零内联。
    assert!(
        context
            .layers
            .l3_refs
            .iter()
            .any(|r| r == "artifact:art-p44-1"),
        "l3_refs: {:?}",
        context.layers.l3_refs
    );
    let citation = context
        .citations
        .iter()
        .find(|c| c.ref_id == "artifact:art-p44-1")
        .expect("artifact citation");
    assert_eq!(citation.source, "artifact-store");
    assert_eq!(
        citation.content_hash.as_deref(),
        Some("ab".repeat(32).as_str()),
        "artifact sha256 进引用（计划 §14:794 第三条）"
    );
    // 正文零内联（设计稿 §5.3 第 7 条）：序列化面只有引用（uri/sha256），
    // run_id 与 mime 等结构化全文仅在 load 载荷里。
    let serialized = serde_json::to_string(&context).unwrap();
    assert!(
        !serialized.contains("rn-p44-art") && !serialized.contains("application/octet-stream"),
        "artifact 结构化全文不内联，仅 uri/sha256 引用"
    );

    // expand 记账路径：artifact 全文按需展开并扣 l3。
    let expanded = broker
        .expand(&context.manifest_id, "artifact:art-p44-1")
        .expect("expand artifact");
    assert!(expanded.content.body.contains("art-p44-1"));
    assert!(expanded.granted_tokens > 0);

    // 源级 load：按 ref 取回结构化全文（run/role/sha256）。
    let content = artifact_source_for_load
        .load(&ContentRef {
            source: "artifact-store".to_string(),
            ref_id: "artifact:art-p44-1".to_string(),
            ref_version: None,
            content_hash: None,
            uri: None,
        })
        .expect("load artifact");
    assert!(
        content.body.contains("\"role\":\"primary\""),
        "body: {}",
        content.body
    );
}

// ---------------------------------------------------------------------------
// 汇总断言：L0 装配 + 预算报告形状（契约面）
// ---------------------------------------------------------------------------

#[test]
fn p4_4_budget_report_shape_and_l0_rules_installed() {
    let pool = open_memory_pool();
    let rulepack = RulepackSource::from_dir(&repo_root().join("rulepacks/rip-rules")).unwrap();
    let broker = Broker::new(
        vec![Box::new(rulepack)],
        policy_for(8000),
        load_intent_config(),
        ManifestStore::new(pool),
        None,
    );
    let context = broker
        .assemble(&assemble_request("t-p44-shape", "kingdee 检索"))
        .expect("assemble");

    // L0 规则封顶内全量（7 条规则全装，远小于 400 tokens 的 l0 分项）。
    let rule_count = context
        .citations
        .iter()
        .filter(|c| c.ref_id.starts_with("rp-rules:"))
        .count();
    assert_eq!(rule_count, 7, "规则包全量装配: {rule_count}");

    // budget_report 形状：allocated 七分项之和 = 总量；used <= allocated。
    let report = &context.budget_report;
    let alloc_sum = report.allocated.l0_rules
        + report.allocated.l1_catalog
        + report.allocated.l2_knowledge
        + report.allocated.l2_tools
        + report.allocated.l3_reserved
        + report.allocated.memory_history
        + report.allocated.reserve_output;
    assert_eq!(alloc_sum, 8000, "七分项之和恒等于总量");
    assert!(report.used.l0_rules <= report.allocated.l0_rules);
    assert!(report.used.l1_catalog <= report.allocated.l1_catalog);
    assert_eq!(report.used.reserve_output, 0, "输出保留不被侵吞");
    assert_eq!(report.used.memory_history, 0, "记忆分项 Phase 7 前恒 0");

    // intent 摘要进入契约。
    assert_eq!(context.intent.status, "ok");
    assert!(context.schema_version == "1");
    assert!(context.broker_version == context_broker::BROKER_VERSION);
}

// ---------------------------------------------------------------------------
// 修复轮回归：new_id 熵源跨进程唯一 + CLI 端到端 ambiguous alternatives 落库
// ---------------------------------------------------------------------------

fn run_rp_context(args: &[&str]) -> (i32, String) {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_rp-context"))
        .args(args)
        .output()
        .expect("spawn rp-context");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
    )
}

fn temp_db(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rip-p46r2-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir.join(format!("{tag}.db"))
}

#[test]
fn p4_4r2_new_id_unique_across_processes_same_second() {
    // 修复轮回归（原缺陷：秒级 now_iso + 进内序号在「同 task 同秒跨进程」
    // 100% 碰撞，第二次 INSERT 撞 UNIQUE 硬失败）。两个独立子进程、同
    // task-id、背靠背同秒装配：两次都必须成功且 manifest_id 不同。
    let root = repo_root();
    let db = temp_db("collision");
    let rulepack = root.join("rulepacks/rip-rules");
    let manifests = root.join("manifests/tools");
    let migrations = root.join("migrations");
    let mut ids = Vec::new();
    for _ in 0..2 {
        let (code, stdout) = run_rp_context(&[
            "assemble",
            "--task-id",
            "t-collision-r2",
            "--db",
            db.to_str().unwrap(),
            "--rulepack-dir",
            rulepack.to_str().unwrap(),
            "--manifest-dirs",
            manifests.to_str().unwrap(),
            "--migrations-dir",
            migrations.to_str().unwrap(),
        ]);
        assert_eq!(
            code, 0,
            "both processes must succeed (no UNIQUE collision): {stdout}"
        );
        let manifest_id = stdout
            .lines()
            .find(|l| l.contains("manifest_id"))
            .and_then(|l| l.split('"').nth(3))
            .expect("manifest_id in stdout")
            .to_string();
        ids.push(manifest_id);
    }
    assert_ne!(
        ids[0], ids[1],
        "same-second cross-process ids must differ: {ids:?}"
    );

    // 库内两行清单头（第二次 INSERT 未被 UNIQUE 拒）。
    let ro = Pool::open(PoolConfig {
        path: DbPath::File(db.clone()),
        readonly: true,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("readonly pool");
    let count: i64 = ro
        .with_conn(|c| c.query_row("SELECT count(*) FROM context_manifest", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(count, 2, "both manifests persisted");
    let _ = std::fs::remove_dir_all(db.parent().unwrap());
}

#[test]
fn p4_4r2_cli_ambiguous_alternatives_persisted_in_intent_json() {
    // 单族命中文本（android：0.4 < 0.5）走 CLI 端到端：ambiguous + top3
    // 候选族落库（intent_json 快照含 alternatives，不只活在输出里）。
    let root = repo_root();
    let db = temp_db("amb");
    let rulepack = root.join("rulepacks/rip-rules");
    let manifests = root.join("manifests/tools");
    let migrations = root.join("migrations");
    let (code, stdout) = run_rp_context(&[
        "assemble",
        "--task-id",
        "t-amb-r2",
        "--text",
        "android",
        "--db",
        db.to_str().unwrap(),
        "--rulepack-dir",
        rulepack.to_str().unwrap(),
        "--manifest-dirs",
        manifests.to_str().unwrap(),
        "--migrations-dir",
        migrations.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stdout}");
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("assemble json");
    assert_eq!(parsed["intent"]["status"], "ambiguous");
    assert_eq!(parsed["intent"]["confidence"], 0.4);
    let alternatives = parsed["intent"]["alternatives"]
        .as_array()
        .expect("alternatives array");
    assert!(
        alternatives
            .iter()
            .any(|a| a.as_str() == Some("Mobile / APK")),
        "alternatives: {alternatives:?}"
    );
    let manifest_id = parsed["manifest_id"].as_str().unwrap().to_string();

    // 落库面：intent_json 快照自带 alternatives（ambiguous 库行可追溯）。
    let ro = Pool::open(PoolConfig {
        path: DbPath::File(db.clone()),
        readonly: true,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("readonly pool");
    let intent_json: String = ro
        .with_conn(|c| {
            c.query_row(
                "SELECT intent_json FROM context_manifest WHERE manifest_id = ?1",
                [manifest_id.clone()],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert!(
        intent_json.contains("alternatives") && intent_json.contains("Mobile / APK"),
        "intent_json: {intent_json}"
    );
    let _ = std::fs::remove_dir_all(db.parent().unwrap());
}
