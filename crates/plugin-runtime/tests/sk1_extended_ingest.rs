//! SK-1 集成测试（P6-9 验收口径，安全知识源计划 §8 P6-9）：
//! - 三类新内容（catalog_entry / document_markdown / skill_candidate）
//!   dry-run 摄取（零落盘，计数与实跑同构）；
//! - 实跑入 knowledge_source_entry（溯源三元组 + item_key + DISCOVERED），
//!   重复运行 skipped=全量（幂等），changed=0；
//! - 审核通过（APPROVED）后 PluginSource 候选可见、assemble 产出插件条目
//!   且 Evidence 固化（引用片段 + 哈希可复核）；
//! - 禁用插件后候选与装配面无该插件条目，历史 Evidence 仍可读可验证；
//! - Search 第三源接线（APPROVED + 可见插件才入索引，rebuild 刷新）；
//! - SK-1 契约示例双份（yaml/json）过七条校验。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use context_broker::ContextSource;
use database::{DbPath, Pool, PoolConfig};
use domain::knowledge::ReviewStatus;
use domain::plugin::PluginTrust;
use plugin_runtime::{ingest_knowledge_source_with_opts, IngestOptions, PluginRuntime};
use serde_json::{json, Value};

fn unique(tag: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("sk1-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn memory_pool() -> Pool {
    Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap()
}

fn migrations_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations")
}

/// fixture 包：catalog/links.md（链接目录 + 表格行 + scheme 混杂）、docs/ 两篇
/// Markdown（frontmatter + 标题）、skills/ 两个候选（frontmatter）。
fn build_pkg(root: &Path) -> PathBuf {
    let pkg = root.join("pkg");
    fs::create_dir_all(pkg.join("catalog")).unwrap();
    fs::create_dir_all(pkg.join("docs/sub")).unwrap();
    fs::create_dir_all(pkg.join("skills")).unwrap();
    fs::create_dir_all(pkg.join("capabilities")).unwrap();
    fs::write(
        pkg.join("catalog/links.md"),
        "# 工具目录\n\n## 扫描\n\n- [NmapLike](https://example.com/nmap-like): 端口扫描器\n- [BadScheme](javascript:alert(1)): 应被过滤\n- [Relative](./next.md): 无 scheme 应被过滤\n\n## 表格区\n\n| 名称 | 链接 |\n|---|---|\n| FuzzLike | [Fuzz](https://example.com/fuzz-like) |\n",
    )
    .unwrap();
    fs::write(
        pkg.join("docs/guide.md"),
        "---\ntitle: 渗透测试指南\ndescription: 指南描述\n---\n\n# 渗透测试指南\n\n## 准备\n\n正文：端口扫描与资产梳理。\n",
    )
    .unwrap();
    fs::write(
        pkg.join("docs/sub/notes.md"),
        "# 笔记\n\n无 frontmatter 的正文。\n",
    )
    .unwrap();
    fs::write(
        pkg.join("skills/recon-skill.md"),
        "---\nname: recon-skill\ndescription: 基础侦察技能\nentry: skills/recon-skill.md\n---\n\n技能正文。\n",
    )
    .unwrap();
    fs::write(
        pkg.join("skills/report-skill.md"),
        "---\nname: report-skill\ndescription: 报告撰写技能\n---\n\n技能正文。\n",
    )
    .unwrap();
    fs::write(
        pkg.join("capabilities/knowledge_source.yaml"),
        "corpus_id: sk1-corpus\n",
    )
    .unwrap();
    pkg
}

fn sk1_manifest() -> Value {
    json!({
        "schema_version": "1",
        "id": "knowledge.sk1.extended",
        "name": "SK1 Extended",
        "version": "0.1.0",
        "kind": "knowledge",
        "description": "SK-1 三类内容摄取契约测试",
        "license": "CC0-1.0",
        "source": {"type": "local_dir", "path": ".", "pinned": {"kind": "commit",
                    "value": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}},
        "compatibility": {"rip_core": ">=0.2.0, <1.0.0", "manifest_schema": ["1"]},
        "capabilities": [{"id": "knowledge-source.sk1.extended", "type": "knowledge_source",
            "entry": "capabilities/knowledge_source.yaml", "summary": "s",
            "provides": {
                "corpus_id": "sk1-corpus",
                "content_kinds": ["catalog_entry", "document_markdown", "skill_candidate"],
                "kinds": {
                    "catalog_entry": {"source_path": "catalog/links.md", "parser": "markdown_links",
                                       "license_policy": "catalog_only"},
                    "document_markdown": {"source_path": "docs/", "parser": "markdown_doc",
                                           "license_policy": "approved_only"},
                    "skill_candidate": {"source_path": "skills/", "parser": "skill_frontmatter",
                                         "license_policy": "metadata_only"}
                },
                "index": {"fts": true, "vector": false}}}],
        "permissions": {"data_access": "ingest_via_api"},
        "data_retention": {"ingested_policy": "retain_with_provenance", "persistent": []},
        "context": {"summary": "s", "docs": "docs/guide.md"}
    })
}

fn install(rt: &PluginRuntime, pkg: &Path, manifest: &Value) {
    rt.install("user:test", manifest, pkg, PluginTrust::Local)
        .unwrap();
}

fn kse_counts(pool: &Pool, plugin_id: &str) -> Vec<(String, i64)> {
    pool.with_conn(|c| {
        let mut stmt = c
            .prepare("SELECT entry_kind, COUNT(*) FROM knowledge_source_entry WHERE plugin_id = ?1 GROUP BY entry_kind ORDER BY entry_kind")
            .unwrap();
        let rows = stmt
            .query_map([plugin_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        Ok::<Vec<(String, i64)>, database::rusqlite::Error>(rows)
    })
    .unwrap()
}

fn setup(tag: &str) -> (Pool, PathBuf, Value, PluginRuntime) {
    let root = unique(tag);
    let pkg = build_pkg(&root);
    let manifest = sk1_manifest();
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let audit = audit_engine::AuditEngine::new(pool.clone());
    let rt = PluginRuntime::open(pool.clone(), audit);
    install(&rt, &pkg, &manifest);
    (pool, pkg, manifest, rt)
}

// ---------------------------------------------------------------------------
// 三类新内容 dry-run → 实跑 → 幂等重跑
// ---------------------------------------------------------------------------

#[test]
fn three_extended_kinds_dry_run_ingest_and_idempotence() {
    let (pool, pkg, manifest, _rt) = setup("dry");

    // dry-run：计数与实跑同构，零落盘。
    let dry =
        ingest_knowledge_source_with_opts(&pool, &manifest, &pkg, &IngestOptions { dry_run: true })
            .unwrap();
    assert!(dry.dry_run);
    assert_eq!(
        dry.counts.get("catalog_entry"),
        Some(&2),
        "2 个 http(s) 链接入目"
    );
    assert_eq!(dry.counts.get("document_markdown"), Some(&2));
    assert_eq!(dry.counts.get("skill_candidate"), Some(&2));
    assert_eq!(dry.skipped, 0);
    assert!(
        dry.filtered.get("scheme_rejected") == Some(&1)
            && dry.filtered.get("no_scheme") == Some(&1),
        "scheme 过滤计数进报告：{:?}",
        dry.filtered
    );
    let before: i64 = pool
        .with_conn(|c| {
            c.query_row("SELECT COUNT(*) FROM knowledge_source_entry", [], |r| {
                r.get(0)
            })
        })
        .unwrap();
    assert_eq!(before, 0, "dry-run 落盘必须为零");

    // 实跑：三类入表，溯源三元组 + item_key 词形 + 初始 DISCOVERED。
    let report =
        ingest_knowledge_source_with_opts(&pool, &manifest, &pkg, &IngestOptions::default())
            .unwrap();
    assert!(!report.dry_run);
    assert_eq!(report.counts.get("catalog_entry"), Some(&2));
    assert_eq!(report.counts.get("document_markdown"), Some(&2));
    assert_eq!(report.counts.get("skill_candidate"), Some(&2));
    assert_eq!(report.changed, 0);
    assert_eq!(
        kse_counts(&pool, "knowledge.sk1.extended"),
        vec![
            ("catalog_entry".to_string(), 2),
            ("document_markdown".to_string(), 2),
            ("skill_candidate".to_string(), 2),
        ]
    );
    let (n_prov, n_discovered, n_sha): (i64, i64, i64) = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FILTER (WHERE plugin_id='knowledge.sk1.extended'
                                          AND plugin_version='0.1.0'
                                          AND corpus_id='sk1-corpus'),
                        COUNT(*) FILTER (WHERE review_status='DISCOVERED'),
                        COUNT(*) FILTER (WHERE length(content_sha256)=64
                                          AND length(metadata_sha256)=64)
                 FROM knowledge_source_entry",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(database::DbError::from)
        })
        .unwrap();
    assert_eq!(
        (n_prov, n_discovered, n_sha),
        (6, 6, 6),
        "溯源 + 初始 DISCOVERED + 双哈希全行命中"
    );
    // item_key 词形：catalog = 规范化 URL（尾部 / 归一）；document/skill = 包内相对路径。
    let keys: Vec<String> = pool
        .with_conn(|c| {
            let mut stmt = c
                .prepare("SELECT item_key FROM knowledge_source_entry WHERE plugin_id='knowledge.sk1.extended' ORDER BY entry_kind, item_key")
                .unwrap();
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            Ok::<Vec<String>, database::rusqlite::Error>(rows)
        })
        .unwrap();
    assert!(
        keys.contains(&"https://example.com/fuzz-like".to_string()),
        "{keys:?}"
    );
    assert!(keys.contains(&"docs/guide.md".to_string()), "{keys:?}");
    // 分类路径与源位置（catalog 条目带章节标题路径 + 行号锚点）。
    let (cat_path, loc): (String, String) = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT category_path_json, source_location_json FROM knowledge_source_entry
                 WHERE entry_kind='catalog_entry' AND item_key='https://example.com/nmap-like'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(database::DbError::from)
        })
        .unwrap();
    assert!(
        cat_path.contains("扫描"),
        "标题路径入 category_path_json: {cat_path}"
    );
    assert!(loc.contains("\"line\""), "源位置带行号: {loc}");

    // 幂等重跑：skipped=全量，counts 空，changed=0。
    let again =
        ingest_knowledge_source_with_opts(&pool, &manifest, &pkg, &IngestOptions::default())
            .unwrap();
    assert!(again.counts.is_empty(), "second run inserts nothing");
    assert_eq!(again.skipped, 6, "all 6 rows skipped as unchanged");
    assert_eq!(again.changed, 0, "no content drift on rerun");
}

// ---------------------------------------------------------------------------
// APPROVED → Search/Context 可见；禁用 → 撤出匹配面；历史 Evidence 可读可验证
// ---------------------------------------------------------------------------

#[test]
fn approve_visible_disable_hidden_evidence_readable() {
    let (pool, pkg, manifest, rt) = setup("ctx");
    ingest_knowledge_source_with_opts(&pool, &manifest, &pkg, &IngestOptions::default()).unwrap();

    // 审核闸：DISCOVERED 不进检索/上下文面。
    let source = context_broker::PluginSource::new(pool.clone(), "knowledge.sk1.extended");
    let q = context_broker::IntentQuery {
        text_normalized: "端口扫描".to_string(),
        families: Vec::new(),
        knowledge_intents: Vec::new(),
        target_slots: Vec::new(),
        task_verbs: Vec::new(),
        constraints: Vec::new(),
        confidence: 1.0,
    };
    assert!(
        source.candidates(&q, 10).unwrap().is_empty(),
        "未审条目对模型面不可见"
    );
    // install 已登记插件行（REGISTERED），经生命周期 enable 进入可见态。
    rt.enable("user:test", "knowledge.sk1.extended").unwrap();
    assert!(
        source.candidates(&q, 10).unwrap().is_empty(),
        "DISCOVERED 且插件可见仍不可见"
    );

    // 受控审核：catalog + document 两条 APPROVED（skill_candidate 留未审）。
    let audit = audit_engine::AuditEngine::new(pool.clone());
    let keys = [
        "https://example.com/nmap-like",
        "https://example.com/fuzz-like",
        "docs/guide.md",
        "docs/sub/notes.md",
    ];
    let updated = plugin_runtime::knowledge_review::set_review_status(
        &pool,
        &audit,
        "user:test",
        "knowledge.sk1.extended",
        &keys,
        ReviewStatus::Approved,
    )
    .unwrap();
    assert_eq!(updated, 4);

    // 候选可见（L2 摘要形态，kind=Plugin）。
    let hits = source.candidates(&q, 10).unwrap();
    // 「端口」gram 命中 nmap-like（标题+描述）；docs/guide.md 的标题/正文含
    // 「端口扫描」同样召回（文档类 APPROVED 条目）。
    assert_eq!(hits.len(), 2, "端口扫描 召回 nmap-like 条目与指南文档");
    assert!(hits
        .iter()
        .all(|h| h.kind == context_broker::ContextItemKind::Plugin));
    assert!(hits
        .iter()
        .all(|h| h.reference.source == "plugin:knowledge.sk1.extended"));
    assert!(hits.iter().all(|h| h.reference.content_hash.is_some()));
    let ref_id = hits
        .iter()
        .find(|h| h.reference.ref_id.contains("catalog_entry"))
        .expect("catalog entry among hits")
        .reference
        .ref_id
        .clone();

    // L3 按需正文展开（全文）。
    let content = source.load(&hits[0].reference).unwrap();
    assert!(
        content.body.contains("NmapLike"),
        "L3 返回条目正文：{}",
        content.body
    );

    // Search 第三源：APPROVED + 可见插件入索引。
    let catalog = memory_pool();
    let index_pool = memory_pool();
    let report = search_engine::SearchIndex::build(&pool, &catalog, &index_pool).unwrap();
    // nmap-like + fuzz-like 两 catalog 条目；docs/guide.md 与 docs/sub/notes.md
    // 也是 APPROVED（resource 表空，无 gh: 行）→ 恰 4。
    assert_eq!(report.doc_count, 4);

    // assemble 产出插件条目 + Evidence 固化。
    let rulepack_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../rulepacks/rip-rules");
    let intent_config = context_broker::IntentConfig::from_dir(&rulepack_dir).unwrap();
    let policy = context_broker::BudgetPolicy::default_for(8000).unwrap();
    let mut sources: Vec<Box<dyn context_broker::ContextSource>> = Vec::new();
    for s in context_broker::PluginSource::mount_all(&pool) {
        sources.push(Box::new(s));
    }
    let broker = context_broker::Broker::new(
        sources,
        policy,
        intent_config,
        context_broker::ManifestStore::new(pool.clone()),
        None,
    )
    .with_evidence_store(resource_engine::sqlite::SqliteResourceRepository::new(
        pool.clone(),
    ));
    let context = broker
        .assemble(&context_broker::AssembleRequest {
            task_id: "t-sk1".to_string(),
            text: "端口扫描 工具调研".to_string(),
            correlation_id: "corr-sk1".to_string(),
            trace_id: None,
            actor: "test".to_string(),
        })
        .unwrap();
    let plugin_items: Vec<_> = context
        .citations
        .iter()
        .filter(|c| c.source == "plugin:knowledge.sk1.extended")
        .collect();
    assert!(!plugin_items.is_empty(), "装配面应产出插件知识条目");
    // Evidence 固化：引用片段 + 哈希可复核。
    let repo = resource_engine::sqlite::SqliteResourceRepository::new(pool.clone());
    let evidences = repo.list_citation_evidence(&context.manifest_id).unwrap();
    assert!(!evidences.is_empty(), "装配即固化引用证据");
    assert!(evidences.iter().any(|e| e.event_kind == "assembled"
        && e.ref_source == "plugin:knowledge.sk1.extended"
        && !e.excerpt.is_empty()
        && e.content_sha256.is_some()));
    let v = repo
        .verify_citation_evidence(&evidences[0].evidence_id)
        .unwrap();
    assert!(v.excerpt_ok, "excerpt 与 excerpt_sha256 互核通过");
    assert_eq!(v.source_ok, Some(true), "源行 content_sha256 互核 verified");

    // 禁用插件：候选与装配面即撤出；历史 Evidence 仍可读可验证。
    pool.with_conn(|c| {
        c.execute(
            "UPDATE plugin SET state='DISABLED' WHERE plugin_id='knowledge.sk1.extended'",
            [],
        )
    })
    .unwrap();
    assert!(
        source.candidates(&q, 10).unwrap().is_empty(),
        "禁用即撤出匹配面"
    );
    let load_after = source.load(&context_broker::ContentRef {
        source: "plugin:knowledge.sk1.extended".to_string(),
        ref_id,
        ref_version: None,
        content_hash: None,
        uri: None,
    });
    assert!(load_after.is_err(), "禁用后 L3 展开拒绝");
    let context2 = broker
        .assemble(&context_broker::AssembleRequest {
            task_id: "t-sk1-2".to_string(),
            text: "端口扫描 工具调研".to_string(),
            correlation_id: "corr-sk1-2".to_string(),
            trace_id: None,
            actor: "test".to_string(),
        })
        .unwrap();
    assert!(
        !context2
            .citations
            .iter()
            .any(|c| c.source == "plugin:knowledge.sk1.extended"),
        "禁用后装配面无该插件条目"
    );
    let evidences_still = repo.list_citation_evidence(&context.manifest_id).unwrap();
    assert!(
        !evidences_still.is_empty(),
        "历史 Evidence 可读（禁用不删行）"
    );
    let v2 = repo
        .verify_citation_evidence(&evidences_still[0].evidence_id)
        .unwrap();
    assert!(
        v2.excerpt_ok && v2.source_ok == Some(true),
        "禁用后历史证据仍可验证"
    );

    // Search 面同步撤出（rebuild 刷新派生索引）。
    search_engine::SearchIndex::rebuild(&pool, &catalog, &index_pool).unwrap();
    assert_eq!(
        search_engine::SearchIndex::doc_count(&index_pool).unwrap(),
        0,
        "rebuild 后插件条目全部撤出"
    );
}

// ---------------------------------------------------------------------------
// SK-1 契约示例双份过七条校验
// ---------------------------------------------------------------------------

#[test]
fn contract_example_double_files_validate() {
    for ext in ["yaml", "json"] {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../schemas/plugin-manifest/examples/knowledge.catalog-example.{ext}"
        ));
        let text = fs::read_to_string(&path).unwrap();
        let report = plugin_registry::validate_document(
            &text,
            None,
            &Default::default(),
            &Default::default(),
        );
        assert!(
            report.ok,
            "{ext} example must pass the seven rules: {}",
            plugin_registry::render_report_text(&report)
        );
        let parsed = plugin_registry::parse_document(&text).unwrap();
        assert_eq!(parsed["kind"], "knowledge");
        let kinds: Vec<&str> = parsed["capabilities"][0]["provides"]["content_kinds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(
            kinds,
            vec!["catalog_entry", "document_markdown", "skill_candidate"]
        );
    }
}

// ---------------------------------------------------------------------------
// validate rule 3：三类新值缺 kinds 声明 / parser 配错 → knowledge_provides_invalid
// ---------------------------------------------------------------------------

#[test]
fn validate_rejects_extended_kinds_without_per_kind_declaration() {
    let mut manifest = sk1_manifest();
    manifest.as_object_mut().unwrap()["capabilities"][0]["provides"]
        .as_object_mut()
        .unwrap()
        .remove("kinds");
    let text = serde_json::to_string(&manifest).unwrap();
    let report =
        plugin_registry::validate_document(&text, None, &Default::default(), &Default::default());
    assert!(!report.ok);
    assert_eq!(report.errors[0].code, "knowledge_provides_invalid");

    let mut manifest2 = sk1_manifest();
    manifest2.as_object_mut().unwrap()["capabilities"][0]["provides"]["kinds"]["catalog_entry"]
        ["parser"] = json!("markdown_doc");
    let text2 = serde_json::to_string(&manifest2).unwrap();
    let report2 =
        plugin_registry::validate_document(&text2, None, &Default::default(), &Default::default());
    assert!(!report2.ok);
    assert_eq!(report2.errors[0].code, "knowledge_provides_invalid");
}
