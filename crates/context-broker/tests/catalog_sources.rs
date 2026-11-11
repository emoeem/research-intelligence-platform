//! P4-2 验证载具（M4.1 判据）：登记面全量逐条出 tier1 card 与 tier2 brief、
//! 规则包源版本化加载、plugin 空源、Mock 仅作测试载具（P4-0 决议 2 口径）。
//!
//! 覆盖口径（P4-2 批次 ask）：TOOL-INVENTORY:193-204 的 T01 到 T10 已登记
//! 为 14 份 Manifest（manifests/tools/）+ Phase 3 两包 6 + SK-3 四工具双份
//! 在 containerfiles/ 下（c9e7b23），T11/T12 缓登不进目录；落库时以
//! discover() 实查数为准。rip-tool-base 的 rip.smoke.native 为 Phase 2
//! 通道验证负载（PHASE3-DESIGN §1 原文口径），不在本目录覆盖口径内。

use context_broker::{
    est_tokens, generate_brief, BudgetPolicy, Candidate, Content, ContentRef, ContextBrokerError,
    ContextItemKind, ContextLayer, ContextSource, IntentQuery, ManifestIndex, PluginSource,
    RegistrySource, RulepackSource, BRIEF_TOKEN_SOFT_CAP, CARD_TOKEN_SOFT_CAP,
};
use database::{DbPath, Pool, PoolConfig};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tool_registry::Registry;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root must exist")
}

/// 登记面目录（T01 到 T10 的 14 份 + Phase 3 两包 6 份；base 冒烟不计）。
fn manifest_dirs() -> Vec<PathBuf> {
    let root = repo_root();
    vec![
        root.join("manifests/tools"),
        root.join("containerfiles/rip-tool-webresearch"),
        root.join("containerfiles/rip-tool-coreutils"),
    ]
}

fn open_memory_registry() -> (Pool, Registry) {
    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("open memory pool");
    // 内存库先走全链迁移（001..008，runtime_node 载具同款先例），tool/tool_version
    // 两表由 001/003 承载。
    database::migrate(&pool, &repo_migrations_dir()).expect("migrations on memory db");
    let registry = Registry::open(pool.clone());
    (pool, registry)
}

fn repo_migrations_dir() -> PathBuf {
    repo_root().join("migrations")
}

fn register_manifest_files(registry: &Registry) -> usize {
    let mut registered = 0;
    for dir in manifest_dirs() {
        let entries =
            std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
        for entry in entries {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            let doc: serde_json::Value = serde_json::from_str(&text)
                .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
            registry
                .register(&doc)
                .unwrap_or_else(|e| panic!("register {} (nine checks): {e}", path.display()));
            registered += 1;
        }
    }
    registered
}

fn empty_intent() -> IntentQuery {
    IntentQuery {
        text_normalized: String::new(),
        families: Vec::new(),
        knowledge_intents: Vec::new(),
        target_slots: Vec::new(),
        task_verbs: Vec::new(),
        constraints: Vec::new(),
        confidence: 0.0,
    }
}

#[test]
fn p4_2_registry_cards_and_briefs_cover_registered_surface() {
    let (pool, registry) = open_memory_registry();
    let registered = register_manifest_files(&registry);
    assert_eq!(
        registered, 24,
        "coverage: 14 manifests in manifests/tools/ + 6 in Phase 3 pack dirs + 4 SK-3 tools (base smoke excluded)"
    );

    // 登记后默认 VALIDATED（store.rs:100-104），目录可见性要求 ENABLED 活跃段
    // （设计稿 §4.3 一票否决）：测试载具内经 pool 推到 ENABLED（无跃迁 API，
    // 状态机跃迁校验属 tool-engine P1-5，不在本批范围）。
    let promoted = pool
        .with_conn(|c| c.execute("UPDATE tool SET lifecycle = 'ENABLED'", []))
        .unwrap();
    assert_eq!(
        promoted, 24,
        "ENABLED promotion must cover the full surface"
    );

    let discovered = registry.discover().expect("discover");
    assert_eq!(discovered.len(), 24, "落库时以 discover() 实查数为准");

    let index = ManifestIndex::from_dirs(&manifest_dirs()).expect("manifest index");
    assert_eq!(index.len(), 24, "file-side index covers the same surface");

    let source = RegistrySource::new(registry, index);

    // tier1 card：登记面全量逐条，软上限 45 tokens（约 30 的落面口径）。
    let cards = source.tool_cards().expect("cards");
    assert_eq!(cards.len(), 24, "every registered tool gets a card");
    for card in &cards {
        assert!(
            est_tokens(&card.line) <= CARD_TOKEN_SOFT_CAP,
            "card {} over soft cap: {} tokens (line: {})",
            card.tool_id,
            card.est_tokens,
            card.line
        );
        assert!(card.line.contains(&card.tool_id), "card line carries id");
        assert!(
            card.line.contains("ENABLED/ACTIVE"),
            "card line carries status: {}",
            card.line
        );
    }
    let max_card = cards.iter().map(|c| c.est_tokens).max().unwrap();
    println!("max card tokens = {max_card}");

    // tier2 brief：登记面全量逐条，软上限 120 tokens，必填键保留。
    let briefs = source.tool_briefs().expect("briefs");
    assert_eq!(briefs.len(), 24, "every registered tool gets a brief");
    for brief in &briefs {
        assert!(
            est_tokens(&brief.text) <= BRIEF_TOKEN_SOFT_CAP,
            "brief {} over soft cap: {} tokens",
            brief.tool_id,
            brief.est_tokens
        );
        assert_eq!(brief.est_tokens, est_tokens(&brief.text));
        if let Some(first_required) = brief.required_input.first() {
            assert!(
                brief.text.contains(first_required.as_str()),
                "brief {} must keep required key {first_required}",
                brief.tool_id
            );
        }
        assert!(
            brief.text.contains("v"),
            "brief carries version: {}",
            brief.text
        );
    }
    let max_brief = briefs.iter().map(|b| b.est_tokens).max().unwrap();
    println!("max brief tokens = {max_brief}");

    // tier3 full：Schema 全文按需展开（load 走 ContentRef；显式接口同面）。
    let (input, output) = source
        .tool_schema("rip.kb.keyword_search", None)
        .expect("schemas");
    assert_eq!(
        input
            .get("required")
            .and_then(|r| r.get(0))
            .and_then(|v| v.as_str()),
        Some("keyword")
    );
    assert!(
        output.get("properties").is_some(),
        "output schema carries properties"
    );

    let content = source
        .load(&ContentRef {
            source: "registry".to_string(),
            ref_id: "rip.kb.keyword_search".to_string(),
            ref_version: None,
            content_hash: None,
            uri: None,
        })
        .expect("load tier3 full");
    assert!(
        content.body.contains("input_schema") && content.body.contains("output_schema"),
        "tier3 body carries both schemas"
    );
    assert!(content.est_tokens > 0);

    // ContextSource candidates：可见全量、族标签齐全、引用权威。
    let source_dyn: Box<dyn ContextSource> = Box::new(source);
    let candidates = source_dyn
        .candidates(&empty_intent(), 100)
        .expect("candidates");
    assert_eq!(candidates.len(), 24);
    for candidate in &candidates {
        assert_eq!(candidate.kind, ContextItemKind::Tool);
        assert_eq!(candidate.layer, ContextLayer::L2);
        assert_eq!(
            candidate.reference.source, "registry",
            "reference source is authoritative"
        );
        assert!(
            candidate.family.as_deref().unwrap_or("").len() > 1,
            "manifest family present: {:?}",
            candidate.family
        );
    }

    // 越权源引用被拒（端口纪律）。
    let wrong_source = source_dyn.load(&ContentRef {
        source: "rulepack".to_string(),
        ref_id: "rp-rules:pack".to_string(),
        ref_version: None,
        content_hash: None,
        uri: None,
    });
    assert!(
        wrong_source.is_err(),
        "registry source must refuse foreign refs"
    );
}

#[test]
fn p4_2_rulepack_source_loads_versioned_rules() {
    let dir = repo_root().join("rulepacks/rip-rules");
    let source = RulepackSource::from_dir(&dir).expect("rulepack loads");

    let pack = source.pack();
    assert_eq!(pack.pack_id, "rip-rules");
    assert_eq!(pack.pack_version, "1.0.0");
    // 版本同源：rules.toml policy_version 与 BudgetPolicy 默认策略一致
    // （设计稿 §7.2「策略变化」失效触发器的数据基础）。
    assert_eq!(
        pack.policy_version,
        BudgetPolicy::default_for(8000)
            .expect("default policy")
            .policy_version(),
        "policy_version must be co-sourced with the budget policy"
    );
    assert!(!pack.rules.is_empty(), "rule pack ships rules");

    // L0 候选：逐条 Rule/L0、引用带包版本、预算预计算。
    let candidates = source
        .candidates(&empty_intent(), 100)
        .expect("rulepack candidates");
    assert_eq!(candidates.len(), pack.rules.len());
    for candidate in &candidates {
        assert_eq!(candidate.kind, ContextItemKind::Rule);
        assert_eq!(candidate.layer, ContextLayer::L0);
        assert_eq!(candidate.reference.source, "rulepack");
        assert_eq!(
            candidate.reference.ref_version.as_deref(),
            Some(pack.pack_version.as_str())
        );
        assert!(
            candidate.est_tokens > 0,
            "body precomputed: {}",
            candidate.item_id
        );
        assert!(candidate.family.is_none(), "rules carry no family tag");
    }

    // load：规则正文与包说明均可展开，未知引用拒绝。
    let rule_ref = candidates[0].reference.clone();
    let content = source.load(&rule_ref).expect("load rule body");
    assert!(!content.body.is_empty());
    let pack_content = source
        .load(&ContentRef {
            source: "rulepack".to_string(),
            ref_id: "rp-rules:pack".to_string(),
            ref_version: None,
            content_hash: None,
            uri: None,
        })
        .expect("load pack doc");
    assert!(
        pack_content.body.contains("# RIP L0 规则包"),
        "pack doc is RULEPACK.md"
    );
    let unknown = source.load(&ContentRef {
        source: "rulepack".to_string(),
        ref_id: "rp-rules:missing-rule".to_string(),
        ref_version: None,
        content_hash: None,
        uri: None,
    });
    assert!(unknown.is_err(), "unknown rule ref must be refused");
}

#[test]
fn p6_9_plugin_source_fail_closed_without_data() {
    // SK-1 实装后的空集语义：无插件表/无条目（内存库未迁移）即候选空集
    // （合法结果，设计稿 §9.2 第 4 条），load 拒绝（无可见插件可展开）。
    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    let source = PluginSource::new(pool, "knowledge.x");
    assert_eq!(source.id(), "plugin:knowledge.x");
    let candidates = source
        .candidates(&empty_intent(), 10)
        .expect("empty is legal");
    assert!(
        candidates.is_empty(),
        "no plugin table = fail-closed empty candidates"
    );
    let load = source.load(&ContentRef {
        source: "plugin:knowledge.x".to_string(),
        ref_id: "kse:knowledge.x:catalog_entry:https://a".to_string(),
        ref_version: None,
        content_hash: None,
        uri: None,
    });
    let err = load.expect_err("no visible plugin cannot resolve refs");
    assert!(
        err.to_string().contains("not visible"),
        "access control names the visibility gate: {err}"
    );
}

#[test]
fn p4_2_mock_is_test_vehicle_only_and_trait_object_usable() {
    /// 测试载具 Mock 源（决议 2：Mock 不进生产路径，仅确定性单测用）。
    struct MockSource;

    impl ContextSource for MockSource {
        fn id(&self) -> &str {
            "mock"
        }
        fn candidates(
            &self,
            _q: &IntentQuery,
            limit: usize,
        ) -> Result<Vec<Candidate>, ContextBrokerError> {
            Ok((0..limit.min(2))
                .map(|i| Candidate {
                    item_id: format!("mock:{i}"),
                    kind: ContextItemKind::Doc,
                    layer: ContextLayer::L3,
                    title: format!("mock doc {i}"),
                    summary: "mock summary".to_string(),
                    family: None,
                    governance: None,
                    quality_grade: None,
                    reference: ContentRef {
                        source: "mock".to_string(),
                        ref_id: format!("mock:{i}"),
                        ref_version: None,
                        content_hash: None,
                        uri: None,
                    },
                    est_tokens: 4,
                })
                .collect())
        }
        fn load(&self, _r: &ContentRef) -> Result<Content, ContextBrokerError> {
            Ok(Content {
                reference: ContentRef {
                    source: "mock".to_string(),
                    ref_id: "mock:0".to_string(),
                    ref_version: None,
                    content_hash: None,
                    uri: None,
                },
                body: "mock body".to_string(),
                est_tokens: 3,
            })
        }
    }

    let plugin_pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    let sources: Vec<Box<dyn ContextSource>> = vec![
        Box::new(MockSource),
        Box::new(PluginSource::new(plugin_pool, "knowledge.x")),
    ];
    let ids: Vec<&str> = sources.iter().map(|s| s.id()).collect();
    assert_eq!(ids, vec!["mock", "plugin:knowledge.x"]);
    let mock_candidates = sources[0].candidates(&empty_intent(), 10).expect("mock");
    assert_eq!(mock_candidates.len(), 2);
}

#[test]
fn p4_2_brief_compression_kicks_in_over_soft_cap() {
    // 600 个 CJK 字符的摘要：全量远超 120 tokens，压缩步进必须生效。
    let giant_summary: String = "长".repeat(600);
    let required: Vec<String> = (0..30).map(|i| format!("field_{i}")).collect();
    let out_keys: Vec<String> = (0..40).map(|i| format!("out_{i}")).collect();

    let brief = generate_brief(
        "rip.test.giant",
        "9.9.9",
        &giant_summary,
        &required,
        &out_keys,
        "网络 none，权限 T0",
    );
    assert!(brief.compressed, "over-cap generation must compress");
    assert!(
        est_tokens(&brief.text) <= BRIEF_TOKEN_SOFT_CAP,
        "compressed brief must respect soft cap: {}",
        brief.est_tokens
    );
    assert!(
        brief.summary.ends_with('\u{2026}'),
        "truncated summary keeps ellipsis marker"
    );
    assert!(
        brief.required_input.len() <= 30 + 1,
        "keys may carry ellipsis note"
    );

    // 未超限的常规 brief 不触发压缩。
    let plain = generate_brief(
        "rip.test.small",
        "1.0.0",
        "只读关键词检索",
        &["keyword".to_string()],
        &["keyword".to_string(), "groups".to_string()],
        "网络 none，权限 T0",
    );
    assert!(!plain.compressed, "under-cap brief must not compress");
    assert!(plain.text.contains("keyword"));
    assert!(plain.text.contains("限制: 网络 none"));
}
