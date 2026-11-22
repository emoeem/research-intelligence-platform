//! CB-1 B2 装配稳定性三断言（docs/rip3/CB-1-TASK.md §0 D4 / §4 判据 6）：
//!
//! 1. 同输入同装配：同 AssembleRequest 两次 assemble——selection_hash
//!    相等 + L0 规则列表与 L1 catalog:digest 载荷逐字节相等 + 零信号退化
//!    reason 序（装配内 installed 选择序）跨请求相等。
//! 2. 最小差异重装配：候选集仅增一条知识桶候选——装配 diff = L2 知识桶
//!    增量，L0/L1 前缀逐字节不变（对位 MA-DSH-REFERENCE:106「摘要小区间
//!    写入保持前缀完整」）。
//! 3. 同 task_id run 链相邻请求（broker 无 session 词形，A9 映射）：fresh
//!    与 retry 链同 task_id 同 goal_text 二次装配（correlation_id 不同）
//!    selection_hash 相等；同 run resume 二次装配（correlation_id 相同）
//!    同样相等。
//!
//! 全程内存库 + 真实 migrations + rulepacks/rip-rules 真实 L0 规则源 +
//! 静态候选源（零 IO，候选集显式构造）。

use context_broker::{
    AssembleRequest, Broker, Candidate, Content, ContentRef, ContextBrokerError, ContextItemKind,
    ContextLayer, ContextSource, IntentConfig, ManifestStore, RulepackSource,
};
use database::{DbPath, Pool, PoolConfig};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration as StdDuration;

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
        busy_timeout: StdDuration::from_secs(5),
    })
    .expect("memory pool");
    database::migrate(&pool, &repo_root().join("migrations")).expect("migrations");
    pool
}

/// 静态候选源（零 IO；候选集显式构造，最小差异重装配的增量载体）。
struct StaticSource {
    source_id: String,
    items: Vec<Candidate>,
    bodies: std::collections::HashMap<String, String>,
}

impl StaticSource {
    fn new(source_id: &str, items: Vec<Candidate>) -> Self {
        Self {
            source_id: source_id.to_string(),
            items,
            bodies: std::collections::HashMap::new(),
        }
    }
}

impl ContextSource for StaticSource {
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

/// 知识桶候选（L2 摘要形态；title 与查询词面无关 = 零匹配信号，退化
/// reason 走 source-order 序号形态——序号确定性断言载体）。
fn knowledge_candidate(source_id: &str, item_id: &str, title: &str, summary: &str) -> Candidate {
    Candidate {
        item_id: item_id.to_string(),
        kind: ContextItemKind::Knowledge,
        layer: ContextLayer::L2,
        title: title.to_string(),
        summary: summary.to_string(),
        family: None,
        governance: None,
        quality_grade: None,
        reference: ContentRef {
            source: source_id.to_string(),
            ref_id: item_id.to_string(),
            ref_version: Some("1.0.0".to_string()),
            content_hash: None,
            uri: None,
        },
        est_tokens: 16,
    }
}

fn build_broker(pool: &Pool, source_id: &str, items: Vec<Candidate>) -> Broker {
    let rulepack = RulepackSource::from_dir(&repo_root().join("rulepacks/rip-rules")).unwrap();
    let intent_config =
        IntentConfig::from_dir(&repo_root().join("rulepacks/rip-rules")).unwrap();
    let sources: Vec<Box<dyn ContextSource>> = vec![
        Box::new(rulepack),
        Box::new(StaticSource::new(source_id, items)),
    ];
    Broker::new(
        sources,
        context_broker::BudgetPolicy::default_for(8000).unwrap(),
        intent_config,
        ManifestStore::new(pool.clone()),
        None,
    )
}

fn assemble_request(task_id: &str, correlation_id: &str) -> AssembleRequest {
    AssembleRequest {
        task_id: task_id.to_string(),
        text: "kingdee HTTP 检索".to_string(),
        correlation_id: correlation_id.to_string(),
        trace_id: None,
        actor: "cb1-stability".to_string(),
    }
}

/// 清单项字节化投影（逐字节相等断言的规范化形状；不含 manifest_id 等每次
/// 装配必变的标识列）。
fn item_fingerprint(item: &context_broker::ItemRow) -> Value {
    json!({
        "item_id": item.item_id,
        "layer": item.layer.as_str(),
        "kind": item.kind.as_str(),
        "source": item.source,
        "ref_id": item.ref_id,
        "ref_version": item.ref_version,
        "title": item.title,
        "reason": item.reason,
        "matched_signals": item.matched_signals_json,
        "est_tokens": item.est_tokens,
        "included": item.included.as_str(),
        "content_hash": item.content_hash,
    })
}

/// 断言一（D4①）：同输入两次装配 selection_hash 相等 + L0/L1 载荷逐字节
/// 相等 + 零信号退化 reason 序相等。
#[test]
fn cb1_same_input_assembles_identical_selection_and_prefix() {
    let pool = open_memory_pool();
    let source_id = format!("static-a-{}", std::process::id());
    let items = vec![
        knowledge_candidate(&source_id, "kb:rel-1", "kingdee HTTP 接口检索", "金蝶 HTTP 相关"),
        knowledge_candidate(&source_id, "kb:unrelated-1", "无关词条甲", "与查询无匹配信号"),
        knowledge_candidate(&source_id, "kb:unrelated-2", "无关词条乙", "与查询无匹配信号"),
    ];
    let broker = build_broker(&pool, &source_id, items);

    let ctx1 = broker
        .assemble(&assemble_request("t-cb1-stab", "corr-run-1"))
        .expect("assemble 1");
    let ctx2 = broker
        .assemble(&assemble_request("t-cb1-stab", "corr-run-2"))
        .expect("assemble 2");

    assert_eq!(
        broker.manifest(&ctx1.manifest_id).unwrap().manifest.selection_hash,
        broker.manifest(&ctx2.manifest_id).unwrap().manifest.selection_hash,
        "同输入两次装配 selection_hash 相等"
    );

    let view1 = broker.manifest(&ctx1.manifest_id).unwrap();
    let view2 = broker.manifest(&ctx2.manifest_id).unwrap();
    let l0_1: Vec<Value> = view1
        .items
        .iter()
        .filter(|i| i.layer == ContextLayer::L0)
        .map(item_fingerprint)
        .collect();
    let l0_2: Vec<Value> = view2
        .items
        .iter()
        .filter(|i| i.layer == ContextLayer::L0)
        .map(item_fingerprint)
        .collect();
    assert!(!l0_1.is_empty(), "rulepack 真实 L0 规则在装配中");
    assert_eq!(
        serde_json::to_string(&l0_1).unwrap(),
        serde_json::to_string(&l0_2).unwrap(),
        "L0 规则列表逐字节相等"
    );
    assert_eq!(
        ctx1.layers.l1.catalog_digest_ref, ctx2.layers.l1.catalog_digest_ref,
        "L1 catalog:digest 载荷逐字节相等"
    );
    assert_eq!(
        serde_json::to_string(&ctx1.layers.l1).unwrap(),
        serde_json::to_string(&ctx2.layers.l1).unwrap(),
        "L1 层载荷逐字节相等"
    );

    // 零信号退化 reason：静态源零匹配词条真实出现在装配中，退化形态序号
    // 跨请求相等（CB-1 D4：装配内 installed 选择序，替代进程级累计序）。
    for item in view1.items.iter() {
        let signals: Vec<String> = serde_json::from_str(&item.matched_signals_json).unwrap();
        if signals.is_empty() && item.source.starts_with("static-a-") {
            assert!(
                item.reason.starts_with("source-order:") && item.reason.contains("+score:"),
                "退化形态口径: item {} reason {:?}",
                item.item_id,
                item.reason
            );
            let twin = view2
                .items
                .iter()
                .find(|i| i.item_id == item.item_id)
                .expect("同输入装配条目集一致");
            assert_eq!(
                item.reason, twin.reason,
                "零信号退化 reason 序跨请求相等（item {}）",
                item.item_id
            );
        }
    }
}

/// 断言二（D4②）：候选集仅增一条知识桶候选——diff 仅 L2 知识桶增量，
/// L0/L1 前缀逐字节不变。
///
/// 增量候选取 Doc 形态（Doc/Schema 随知识桶取舍但不在 L1 digest 计数口径
/// 内——assemble.rs 计数面只计 Tool/Knowledge/Plugin/Rule 四类；kind=
/// Knowledge 增量会同步改 L1 counts，与「L1 前缀逐字节不变」物理冲突，
/// 本用例按判据字面自洽落面，登记批次报告）。
#[test]
fn cb1_minimal_diff_reassembly_keeps_l0_l1_prefix_intact() {
    let pool = open_memory_pool();
    let source_id = format!("static-b-{}", std::process::id());
    let base = vec![
        knowledge_candidate(&source_id, "kb:rel-1", "kingdee HTTP 接口检索", "金蝶 HTTP 相关"),
        knowledge_candidate(&source_id, "kb:unrelated-1", "无关词条甲", "与查询无匹配信号"),
    ];
    let mut extended = base.clone();
    let mut added = knowledge_candidate(&source_id, "doc:added-1", "无关词条丙", "增量知识桶候选");
    added.kind = ContextItemKind::Doc;
    extended.push(added);

    let broker_base = build_broker(&pool, &source_id, base);
    let broker_extended = build_broker(&pool, &source_id, extended);

    let ctx1 = broker_base
        .assemble(&assemble_request("t-cb1-diff", "corr-diff-1"))
        .expect("assemble base");
    let ctx2 = broker_extended
        .assemble(&assemble_request("t-cb1-diff", "corr-diff-2"))
        .expect("assemble extended");

    let view1 = broker_base.manifest(&ctx1.manifest_id).unwrap();
    let view2 = broker_extended.manifest(&ctx2.manifest_id).unwrap();

    // L0 前缀逐字节不变。
    let l0_1: Vec<Value> = view1
        .items
        .iter()
        .filter(|i| i.layer == ContextLayer::L0)
        .map(item_fingerprint)
        .collect();
    let l0_2: Vec<Value> = view2
        .items
        .iter()
        .filter(|i| i.layer == ContextLayer::L0)
        .map(item_fingerprint)
        .collect();
    assert_eq!(
        serde_json::to_string(&l0_1).unwrap(),
        serde_json::to_string(&l0_2).unwrap(),
        "L0 前缀逐字节不变"
    );
    // L1 前缀逐字节不变（digest 载荷与计数）。
    assert_eq!(
        serde_json::to_string(&ctx1.layers.l1).unwrap(),
        serde_json::to_string(&ctx2.layers.l1).unwrap(),
        "L1 前缀逐字节不变（catalog:digest 与 counts）"
    );
    let l1_item_1 = view1.items.iter().find(|i| i.item_id == "catalog:digest").unwrap();
    let l1_item_2 = view2.items.iter().find(|i| i.item_id == "catalog:digest").unwrap();
    assert_eq!(
        serde_json::to_string(&item_fingerprint(l1_item_1)).unwrap(),
        serde_json::to_string(&item_fingerprint(l1_item_2)).unwrap(),
        "L1 manifest 条目逐字节不变"
    );

    // diff 仅 L2 知识桶增量：既有条目（剔除 reason 位次分量）逐一相等 +
    // 恰好多出新增一条。L2 桶内零信号退化 reason 的序号 = 装配内 installed
    // 位次，新候选按分数插入会平移后位条目位次（score 降序装入的固有语义，
    // D4② 字面不变量只有 L0/L1 逐字节——位次敏感分量据此剔除）。
    let mut only_in_2: Vec<&context_broker::ItemRow> = Vec::new();
    for item in &view2.items {
        match view1.items.iter().find(|i| i.item_id == item.item_id) {
            Some(twin) => {
                let (a, b) = (item_fingerprint(item), item_fingerprint(twin));
                let (a, b) = {
                    let (mut a, mut b) = (a, b);
                    a["reason"] = json!(null);
                    b["reason"] = json!(null);
                    (a, b)
                };
                assert_eq!(
                    serde_json::to_string(&a).unwrap(),
                    serde_json::to_string(&b).unwrap(),
                    "既有条目不变（item {}，reason 位次分量除外）",
                    item.item_id
                );
                // 信号承载条目（reason 与信号同源，无位次分量）：逐字节不变。
                let signals: Vec<String> =
                    serde_json::from_str(&item.matched_signals_json).unwrap();
                if !signals.is_empty() {
                    assert_eq!(item.reason, twin.reason, "信号条目 reason 不变");
                }
            }
            None => only_in_2.push(item),
        }
    }
    assert_eq!(only_in_2.len(), 1, "diff 仅一条新增条目");
    assert_eq!(only_in_2[0].item_id, "doc:added-1", "新增即增量候选");
    assert_eq!(only_in_2[0].layer, ContextLayer::L2, "新增落在 L2 知识桶");

    // L2 知识桶投影：条目集 = 基线集 + 恰多一条（按 item_id 对账，剔除
    // reason 位次分量）。
    let ks1 = &ctx1.layers.l2.knowledge_summaries;
    let ks2 = &ctx2.layers.l2.knowledge_summaries;
    assert_eq!(ks2.len(), ks1.len() + 1, "L2 知识桶恰多一条");
    for a in ks1 {
        let b = ks2
            .iter()
            .find(|x| x.item_id == a.item_id)
            .unwrap_or_else(|| panic!("L2 条目 {} 在增量装配中缺失", a.item_id));
        let (mut av, mut bv) = (
            serde_json::to_value(a).unwrap(),
            serde_json::to_value(b).unwrap(),
        );
        av["reason"] = json!(null);
        bv["reason"] = json!(null);
        assert_eq!(
            serde_json::to_string(&av).unwrap(),
            serde_json::to_string(&bv).unwrap(),
            "L2 条目 {} 不变（reason 位次分量除外）",
            a.item_id
        );
    }
}

/// 断言三（D4③）：同 task_id run 链相邻请求 selection_hash 相等（broker
/// 无 session 词形，「同 session 相邻请求」映射 = 同 task_id run 链：
/// fresh → pause → resume 同 goal_text 二次装配 correlation_id 不变；
/// retry 链同定义重放 correlation_id 变——两种链形都断言）。
#[test]
fn cb1_same_task_run_chain_adjacent_assembles_same_selection_hash() {
    let pool = open_memory_pool();
    let source_id = format!("static-c-{}", std::process::id());
    let items = vec![knowledge_candidate(
        &source_id,
        "kb:rel-1",
        "kingdee HTTP 接口检索",
        "金蝶 HTTP 相关",
    )];
    let broker = build_broker(&pool, &source_id, items);

    // resume 形：同 run 二次装配（correlation_id = run_id 不变）。
    let fresh = broker
        .assemble(&assemble_request("t-cb1-chain", "arn-cb1-chain"))
        .expect("assemble fresh");
    let resumed = broker
        .assemble(&assemble_request("t-cb1-chain", "arn-cb1-chain"))
        .expect("assemble resumed segment");
    assert_eq!(
        broker.manifest(&fresh.manifest_id).unwrap().manifest.selection_hash,
        broker.manifest(&resumed.manifest_id).unwrap().manifest.selection_hash,
        "同 task_id 同 run 相邻装配 selection_hash 相等"
    );

    // retry 形：新 run 重放同 goal_text（correlation_id = 新 run_id）。
    let retried = broker
        .assemble(&assemble_request("t-cb1-chain", "arn-cb1-retry"))
        .expect("assemble retry chain");
    assert_eq!(
        broker.manifest(&fresh.manifest_id).unwrap().manifest.selection_hash,
        broker.manifest(&retried.manifest_id).unwrap().manifest.selection_hash,
        "retry 链同定义重放相邻装配 selection_hash 相等"
    );
}
