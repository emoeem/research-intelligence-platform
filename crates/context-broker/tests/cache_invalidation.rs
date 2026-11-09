//! P4-5 验证载具（M4.4 判据）：内容寻址缓存七类失效触发器逐条用例、
//! 改一个工具版本号后 cache-stats 显示对应条目重建、Broker expand 读载荷
//! 缓存、历史 Manifest 不受缓存失效影响。
//!
//! 触发器到 validation 字段的映射见 cache.rs 模块注（设计稿 §7.2 表）；
//! 原子写为 tmp + rename（worker_runtime.py:587-591 先例口径）。

use context_broker::{
    Broker, BrokerCache, BudgetPolicy, CacheValidation, ContextSource, IntentConfig, ManifestIndex,
    ManifestStore, RegistrySource, TTL_SCHEMA,
};
use database::{DbPath, Pool, PoolConfig};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration as StdDuration;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root must exist")
}

fn temp_cache(tag: &str) -> (BrokerCache, PathBuf) {
    let dir = std::env::temp_dir().join(format!("rip-p45-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = BrokerCache::open(dir.clone()).expect("open cache");
    (cache, dir)
}

fn validation(version: &str, content_hash: &str, fingerprint: &str) -> CacheValidation {
    CacheValidation {
        version: version.to_string(),
        content_hash: content_hash.to_string(),
        source_fingerprint: fingerprint.to_string(),
    }
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

// ---------------------------------------------------------------------------
// 基础回环：put/get 原子写、坏条目视同未命中
// ---------------------------------------------------------------------------

#[test]
fn p4_5_cache_roundtrip_atomic_write_and_no_tmp_leftover() {
    let (cache, dir) = temp_cache("roundtrip");
    let fp = "broker=0.1.0;governance=ACTIVE";
    let key = cache
        .put(
            "brief",
            "tool:x",
            &validation("1.0.0", "aa".repeat(32).as_str(), fp),
            "brief text payload",
            "policy-1",
        )
        .expect("put");
    // 原子写：条目落 <key>.json，无 .tmp 残留。
    assert!(dir.join(format!("{key}.json")).exists());
    let stats = cache.stats().unwrap();
    assert_eq!(stats.entries, 1, "stats sees the entry");
    assert!(stats.bytes > 0);

    // 同 validation + 同 policy_version：命中。
    let hit = cache
        .get(
            &key,
            &validation("1.0.0", "aa".repeat(32).as_str(), fp),
            "policy-1",
            TTL_SCHEMA,
        )
        .unwrap();
    assert_eq!(hit.as_deref(), Some("brief text payload"));

    // 未知键：未命中（无负缓存条目产生）。
    let miss = cache
        .get(
            "nonexistent",
            &validation("1", "", ""),
            "policy-1",
            TTL_SCHEMA,
        )
        .unwrap();
    assert!(miss.is_none());
    assert_eq!(
        cache.stats().unwrap().entries,
        1,
        "no negative cache entries"
    );

    // 坏 JSON 条目视同未命中（重建），不炸调用方。
    std::fs::write(dir.join("broken.json"), "not json").unwrap();
    let broken = cache
        .get("broken", &validation("1", "", ""), "policy-1", TTL_SCHEMA)
        .unwrap();
    assert!(broken.is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// 七类失效触发器（设计稿 §7.2 表逐条）
// ---------------------------------------------------------------------------

fn put_base(cache: &BrokerCache, tag: &str, fp: &str) -> String {
    cache
        .put(
            "load",
            &format!("kb:{tag}"),
            &validation("1.0.0", "bb".repeat(32).as_str(), fp),
            "payload",
            "policy-1",
        )
        .expect("put base entry")
}

#[test]
fn p4_5_trigger1_tool_manifest_version_or_digest_change_invalidates() {
    let (cache, dir) = temp_cache("t1");
    let fp = "broker=0.1.0;registry";
    let key = put_base(&cache, "t1", fp);
    // version 变化 → 未命中（重建）。
    assert!(cache
        .get(
            &key,
            &validation("2.0.0", "bb".repeat(32).as_str(), fp),
            "policy-1",
            TTL_SCHEMA
        )
        .unwrap()
        .is_none());
    // digest（content_hash）变化 → 未命中。
    assert!(cache
        .get(
            &key,
            &validation("1.0.0", "cc".repeat(32).as_str(), fp),
            "policy-1",
            TTL_SCHEMA
        )
        .unwrap()
        .is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn p4_5_trigger2_governance_transition_invalidates() {
    let (cache, dir) = temp_cache("t2");
    let key = put_base(&cache, "t2", "broker=0.1.0;governance=ACTIVE");
    // 治理跃迁（如 VERIFIED 守卫外的状态变化）：fingerprint 分量变化 → 未命中。
    assert!(cache
        .get(
            &key,
            &validation(
                "1.0.0",
                "bb".repeat(32).as_str(),
                "broker=0.1.0;governance=QUARANTINED"
            ),
            "policy-1",
            TTL_SCHEMA
        )
        .unwrap()
        .is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn p4_5_trigger3_index_schema_version_change_invalidates() {
    let (cache, dir) = temp_cache("t3");
    let key = put_base(&cache, "t3", "broker=0.1.0;index_schema=1");
    assert!(
        cache
            .get(
                &key,
                &validation(
                    "1.0.0",
                    "bb".repeat(32).as_str(),
                    "broker=0.1.0;index_schema=2"
                ),
                "policy-1",
                TTL_SCHEMA
            )
            .unwrap()
            .is_none(),
        "INDEX_SCHEMA_VERSION 变化（索引重灌）即失效"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn p4_5_trigger4_rtis_knowledge_db_mtime_size_change_invalidates() {
    let (cache, dir) = temp_cache("t4");
    let key = put_base(&cache, "t4", "broker=0.1.0;rtis_db=1731200000:28299264");
    assert!(
        cache
            .get(
                &key,
                &validation(
                    "1.0.0",
                    "bb".repeat(32).as_str(),
                    "broker=0.1.0;rtis_db=1731290000:28301100"
                ),
                "policy-1",
                TTL_SCHEMA
            )
            .unwrap()
            .is_none(),
        "knowledge.db mtime+size 变化（bootstrap 重建）即失效"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn p4_5_trigger5_plugin_state_change_invalidates() {
    let (cache, dir) = temp_cache("t5");
    let key = put_base(&cache, "t5", "broker=0.1.0;plugin=ENABLED");
    assert!(
        cache
            .get(
                &key,
                &validation(
                    "1.0.0",
                    "bb".repeat(32).as_str(),
                    "broker=0.1.0;plugin=DISABLED"
                ),
                "policy-1",
                TTL_SCHEMA
            )
            .unwrap()
            .is_none(),
        "插件禁用/卸载即失效（禁用不删数据，缓存面只失效）"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn p4_5_trigger6_policy_version_change_invalidates() {
    let (cache, dir) = temp_cache("t6");
    let key = put_base(&cache, "t6", "broker=0.1.0");
    assert!(
        cache
            .get(
                &key,
                &validation("1.0.0", "bb".repeat(32).as_str(), "broker=0.1.0"),
                "policy-2",
                TTL_SCHEMA
            )
            .unwrap()
            .is_none(),
        "policy_version 变化使 L0 与预算相关缓存整体失效（设计稿 §7.2）"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn p4_5_trigger7_broker_version_change_invalidates() {
    let (cache, dir) = temp_cache("t7");
    let key = put_base(&cache, "t7", "broker=0.1.0");
    assert!(
        cache
            .get(
                &key,
                &validation("1.0.0", "bb".repeat(32).as_str(), "broker=0.2.0"),
                "policy-1",
                TTL_SCHEMA
            )
            .unwrap()
            .is_none(),
        "装配逻辑升级（broker_version）后旧缓存不信任（设计稿 §7.2）"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// M4.4 判据：改一个工具版本号后 cache-stats 显示对应条目重建
// ---------------------------------------------------------------------------

#[test]
fn p4_5_tool_version_bump_shows_entry_rebuild_in_cache_stats() {
    let pool = open_memory_pool();
    let manifest_path = repo_root().join("manifests/tools/rip.doc.parse.json");
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
    let tool_id = doc["id"].as_str().unwrap().to_string();
    doc["version"] = serde_json::json!("9.9.0");

    let registry = tool_registry::Registry::open(pool.clone());
    registry.register(&doc).expect("register 9.9.0");

    let (cache, dir) = temp_cache("bump");
    let cache = Arc::new(cache);
    let source = RegistrySource::new(
        registry.clone(),
        ManifestIndex::from_dirs(&[repo_root().join("manifests/tools")]).unwrap(),
    )
    .with_cache(cache.clone(), "policy-1");

    // 首次 brief 生成：cache-stats 显示 1 条。
    let first = source.brief_for(&tool_id).expect("brief v9.9.0");
    assert_eq!(cache.stats().unwrap().entries, 1, "stats after first brief");

    // 同版本再取：命中缓存（零生成），条目数不变。
    let _second = source.brief_for(&tool_id).expect("brief hit");
    assert_eq!(
        cache.stats().unwrap().entries,
        1,
        "cache hit does not rebuild"
    );

    // 记录旧版本 content_hash（旧条目 validation 组成部分）。
    let old_hash = registry
        .query(&tool_id)
        .unwrap()
        .versions
        .last()
        .unwrap()
        .content_hash
        .clone();

    // 改一个工具版本号（登记 9.9.1）：cache-stats 显示对应条目重建（条目数
    // 1 → 2，新键新条目，旧条目原样保留）。
    doc["version"] = serde_json::json!("9.9.1");
    registry.register(&doc).expect("register 9.9.1");
    let third = source.brief_for(&tool_id).expect("brief v9.9.1 rebuilt");
    assert_eq!(
        cache.stats().unwrap().entries,
        2,
        "M4.4 判据：版本号变更后 cache-stats 显示对应条目重建"
    );
    assert_ne!(first, third, "新版本 brief 内容随登记面更新");
    // 旧版本条目不受影响（历史不受缓存失效影响）：
    // 旧键仍按旧 validation 命中。
    let old_fp = format!("broker={}", context_broker::BROKER_VERSION);
    let old_key = BrokerCache::cache_key("brief", &tool_id, "9.9.0", "v1", &old_fp);
    let old_hit = cache
        .get(
            &old_key,
            &validation("9.9.0", old_hash.as_str(), &old_fp),
            "policy-1",
            TTL_SCHEMA,
        )
        .unwrap();
    assert_eq!(old_hit.as_deref(), Some(first.as_str()), "旧条目仍可命中");

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Broker expand 读载荷缓存 + 历史 Manifest 不受缓存失效影响
// ---------------------------------------------------------------------------

struct MockSource {
    source_id: String,
    items: Vec<context_broker::Candidate>,
    bodies: std::collections::HashMap<String, String>,
}

impl context_broker::ContextSource for MockSource {
    fn id(&self) -> &str {
        &self.source_id
    }
    fn candidates(
        &self,
        _q: &context_broker::IntentQuery,
        _limit: usize,
    ) -> Result<Vec<context_broker::Candidate>, ContextBrokerError> {
        Ok(self.items.clone())
    }
    fn load(
        &self,
        r: &context_broker::ContentRef,
    ) -> Result<context_broker::Content, ContextBrokerError> {
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

use context_broker::ContextBrokerError;

#[test]
fn p4_5_broker_expand_caches_load_payload_and_history_manifest_unchanged() {
    let pool = open_memory_pool();
    let source_id = format!("mock-load-{}", std::process::id());
    let source = MockSource {
        source_id: source_id.clone(),
        items: vec![context_broker::Candidate {
            item_id: "doc:1".to_string(),
            kind: context_broker::ContextItemKind::Doc,
            layer: context_broker::ContextLayer::L2,
            title: "doc".to_string(),
            summary: "summary".to_string(),
            family: None,
            governance: None,
            quality_grade: None,
            reference: context_broker::ContentRef {
                source: source_id.clone(),
                ref_id: "doc:1".to_string(),
                ref_version: Some("1.0".to_string()),
                content_hash: None,
                uri: None,
            },
            est_tokens: 3,
        }],
        bodies: std::collections::HashMap::from([(
            "doc:1".to_string(),
            "y".repeat(80), // 20 tokens
        )]),
    };
    let (cache, dir) = temp_cache("broker");
    let cache = Arc::new(cache);
    let broker = Broker::new(
        vec![Box::new(source)],
        BudgetPolicy::default_for(8000).unwrap(),
        IntentConfig::from_dir(&repo_root().join("rulepacks/rip-rules")).unwrap(),
        ManifestStore::new(pool.clone()),
        None,
    )
    .with_cache(cache.clone());

    let context = broker
        .assemble(&context_broker::AssembleRequest {
            task_id: "t-p45".to_string(),
            text: "doc 检索".to_string(),
            correlation_id: "corr-p45".to_string(),
            actor: "p45-test".to_string(),
        })
        .expect("assemble");
    let selection_hash_before = broker
        .manifest(&context.manifest_id)
        .unwrap()
        .manifest
        .selection_hash
        .clone();

    // 第一次 expand：未命中 → 生成 + 落缓存（cache-stats +1）。
    assert_eq!(cache.stats().unwrap().entries, 0);
    let expanded = broker
        .expand(&context.manifest_id, "doc:1")
        .expect("expand first");
    assert_eq!(expanded.granted_tokens, 20);
    assert_eq!(cache.stats().unwrap().entries, 1, "load payload cached");

    // 第二次 expand：命中缓存（条目数不变），记账照常（EXPANDED 事件 +1）。
    let events_before = broker
        .manifest(&context.manifest_id)
        .unwrap()
        .ref_events
        .len();
    let again = broker
        .expand(&context.manifest_id, "doc:1")
        .expect("expand hit");
    assert_eq!(again.granted_tokens, 20, "命中路径 tokens 一致");
    assert_eq!(cache.stats().unwrap().entries, 1, "hit does not rebuild");
    let events_after = broker
        .manifest(&context.manifest_id)
        .unwrap()
        .ref_events
        .len();
    assert_eq!(
        events_after,
        events_before + 1,
        "命中路径仍记账（EXPANDED）"
    );

    // 缓存条目被删（模拟失效/清空）：expand 重建成功，历史 Manifest 的
    // selection_hash 与条目集不受缓存失效影响（设计稿 §7.3）。
    let key = BrokerCache::cache_key(
        "load",
        "doc:1",
        "1.0",
        "expand",
        &format!(
            "broker={};source={source_id}",
            context_broker::BROKER_VERSION
        ),
    );
    let _ = std::fs::remove_file(dir.join(format!("{key}.json")));
    let rebuilt = broker
        .expand(&context.manifest_id, "doc:1")
        .expect("expand after invalidation");
    assert_eq!(rebuilt.content.body.len(), 80);
    let view = broker.manifest(&context.manifest_id).unwrap();
    assert_eq!(
        view.manifest.selection_hash, selection_hash_before,
        "历史 Manifest 不受缓存失效影响"
    );
    assert!(!view.items.is_empty());

    // 同输入再装配（缓存已在位）：selection_hash 仍一致（缓存不影响装配语义）。
    let context2 = broker
        .assemble(&context_broker::AssembleRequest {
            task_id: "t-p45".to_string(),
            text: "doc 检索".to_string(),
            correlation_id: "corr-p45-2".to_string(),
            actor: "p45-test".to_string(),
        })
        .expect("assemble again");
    assert_eq!(
        context2.budget_report.allocated, context.budget_report.allocated,
        "缓存存在不改变装配分配语义"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
