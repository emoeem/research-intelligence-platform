//! P6-5 Knowledge 插件摄取契约测试（ROADMAP §7.3 P6-5 完成判据）：
//! - 摄取管道三类语料（poc / skill_md / method_case）入 knowledge_ingest 表，
//!   行带 plugin_id + plugin_version + corpus_id 溯源三元组；
//! - 权限闸：data_access 非 ingest_via_api 一律拒绝（PLUGIN-CONTRACT §2
//!   原则 2 落进代码，不直写主库表）；
//! - 幂等：重跑摄取对内容不变行 no-op（skipped 全量，counts 空）；
//! - fixture 语料：mini knowledge.db（poc 表最小列集）+ skills md + pocs 文件。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use database::{DbPath, Pool, PoolConfig};
use domain::plugin::PluginTrust;
use plugin_registry::parse_document;
use plugin_runtime::{ingest_knowledge_source, PluginRuntime};
use serde_json::{json, Value};

fn unique(tag: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("p6ing-{tag}-{}-{n}", std::process::id()));
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

/// 构建 fixture 语料：mini knowledge.db（poc 最小列集 3 行）+ 2 个技能 md +
/// 2 个 pocs 文件（子目录嵌套，验证递归）。
fn build_corpus(root: &Path) -> PathBuf {
    let pkg = root.join("pkg");
    fs::create_dir_all(pkg.join("capabilities")).unwrap();
    fs::create_dir_all(pkg.join("skills")).unwrap();
    fs::create_dir_all(pkg.join("pocs/orch/case-a")).unwrap();
    // mini knowledge.db（poc 最小列集，列名与 RTIS 实测 schema 一致的子集）。
    let db = database::rusqlite::Connection::open(pkg.join("knowledge.db")).unwrap();
    db.execute_batch(
        "CREATE TABLE poc (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            code TEXT, title TEXT NOT NULL, content TEXT,
            usage TEXT, description TEXT, kind TEXT, category TEXT,
            cve TEXT, component TEXT, versions TEXT, severity TEXT,
            language TEXT, tags TEXT, verified INTEGER DEFAULT 0
        );
        INSERT INTO poc (code, title, content, kind, category, cve, component, tags, verified)
          VALUES ('svc-1', '服务1 判定', '正文 A：判定规则', 'script', 'info-leak', NULL, 'svc', '[\"k1\"]', 1);
        INSERT INTO poc (code, title, content, kind, category, cve, component, tags, verified)
          VALUES ('svc-2', '服务2 绕过', '正文 B：绕过矩阵', 'script', 'auth-bypass', 'CVE-2026-0001', 'svc', '[]', 0);
        INSERT INTO poc (code, title, usage, kind, category, component, tags, verified)
          VALUES ('svc-3', '服务3 探测', '用法 C（content 缺省回退 usage）', 'script', 'other', 'svc', '[]', 0);",
    )
    .unwrap();
    drop(db);
    // skills。
    fs::write(
        pkg.join("skills/recon-basic.md"),
        "# 技能：基础侦察\n\n步骤正文。\n",
    )
    .unwrap();
    fs::write(
        pkg.join("skills/report-write.md"),
        "# 技能：报告撰写\n\n正文。\n",
    )
    .unwrap();
    // pocs（嵌套目录）。
    fs::write(
        pkg.join("pocs/orch/case-a/step1.md"),
        "编排案例 A 第一步。\n",
    )
    .unwrap();
    fs::write(
        pkg.join("pocs/orch/case-a/step2.txt"),
        "编排案例 A 第二步。\n",
    )
    .unwrap();
    fs::write(
        pkg.join("capabilities/knowledge_source.yaml"),
        "corpus_id: fixture-corpus\ncontent_kinds: [poc, skill_md, method_case]\n",
    )
    .unwrap();
    pkg
}

fn fixture_manifest() -> Value {
    json!({
        "schema_version": "1",
        "id": "knowledge.fixture.corpus",
        "name": "Fixture Corpus",
        "version": "0.1.0",
        "kind": "knowledge",
        "description": "摄取契约测试语料",
        "license": "MIT",
        "source": {"type": "local_dir", "path": ".", "pinned": {"kind": "commit"}},
        "compatibility": {"rip_core": ">=0.2.0, <1.0.0", "manifest_schema": ["1"]},
        "capabilities": [{"id": "knowledge-source.fixture.corpus", "type": "knowledge_source",
            "entry": "capabilities/knowledge_source.yaml", "summary": "s",
            "provides": {"corpus_id": "fixture-corpus", "content_kinds": ["poc", "skill_md", "method_case"],
                          "index": {"fts": true, "vector": false}}}],
        "permissions": {"data_access": "ingest_via_api"},
        "data_retention": {"ingested_policy": "retain_with_provenance", "persistent": []},
        "context": {"summary": "s", "docs": "docs/usage.md"}
    })
}

fn install_plugin(rt: &PluginRuntime, pkg: &Path, manifest: &Value) {
    rt.install("user:test", manifest, pkg, PluginTrust::Local)
        .unwrap();
}

fn kind_counts(pool: &Pool, plugin_id: &str) -> Vec<(String, i64)> {
    pool.with_conn(|c| {
        let mut stmt = c
            .prepare("SELECT content_kind, COUNT(*) FROM knowledge_ingest WHERE plugin_id = ?1 GROUP BY content_kind ORDER BY content_kind")
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

// ---------------------------------------------------------------------------
// 主链：install → ingest 三类语料 + 溯源三元组 + 幂等重跑
// ---------------------------------------------------------------------------

#[test]
fn ingest_pipeline_provenance_and_idempotence() {
    let root = unique("prov");
    let pkg = build_corpus(&root);
    let manifest = fixture_manifest();
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let audit = audit_engine::AuditEngine::new(pool.clone());
    let rt = PluginRuntime::open(pool.clone(), audit);
    install_plugin(&rt, &pkg, &manifest);

    // 摄取：三类各入若干行。
    let report = ingest_knowledge_source(&pool, &manifest, &pkg).unwrap();
    assert_eq!(report.plugin_id, "knowledge.fixture.corpus");
    assert_eq!(report.corpus_id, "fixture-corpus");
    assert_eq!(report.counts.get("poc"), Some(&3));
    assert_eq!(report.counts.get("skill_md"), Some(&2));
    assert_eq!(report.counts.get("method_case"), Some(&2));
    assert_eq!(report.skipped, 0);
    assert!(report.total_bytes > 0);

    // 溯源三元组：全部行 plugin_id + plugin_version + corpus_id。
    let (n_pid, n_pver, n_corpus, n_sha): (i64, i64, i64, i64) = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FILTER (WHERE plugin_id = ?1),
                        COUNT(*) FILTER (WHERE plugin_version = '0.1.0'),
                        COUNT(*) FILTER (WHERE corpus_id = 'fixture-corpus'),
                        COUNT(*) FILTER (WHERE length(content_sha256) = 64)
                 FROM knowledge_ingest",
                ["knowledge.fixture.corpus"],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .map_err(database::DbError::from)
        })
        .unwrap();
    assert_eq!(
        (n_pid, n_pver, n_corpus, n_sha),
        (7, 7, 7, 7),
        "all rows carry provenance"
    );

    // 三类分布对账 + content 缺省回退 usage（svc-3）。
    assert_eq!(
        kind_counts(&pool, "knowledge.fixture.corpus"),
        vec![
            ("method_case".to_string(), 2),
            ("poc".to_string(), 3),
            ("skill_md".to_string(), 2)
        ]
    );
    let svc3: (String, i64) = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT body, body_bytes FROM knowledge_ingest WHERE source_pk = '3'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(database::DbError::from)
        })
        .unwrap();
    assert!(
        svc3.0.contains("用法 C"),
        "content-missing rows fall back to usage"
    );
    assert_eq!(
        svc3.1,
        svc3.0.len() as i64,
        "body_bytes matches body length"
    );

    // 幂等重跑：counts 空、skipped 全量。
    let again = ingest_knowledge_source(&pool, &manifest, &pkg).unwrap();
    assert!(again.counts.is_empty(), "second run inserts nothing");
    assert_eq!(again.skipped, 7, "all 7 rows skipped as unchanged");
    assert_eq!(
        kind_counts(&pool, "knowledge.fixture.corpus")
            .iter()
            .map(|(_, n)| n)
            .sum::<i64>(),
        7
    );
}

// ---------------------------------------------------------------------------
// 权限闸：非 ingest_via_api 拒绝（原则 2 落进代码）
// ---------------------------------------------------------------------------

#[test]
fn ingest_permission_gate() {
    let root = unique("gate");
    let pkg = build_corpus(&root);
    let mut manifest = fixture_manifest();
    manifest.as_object_mut().unwrap()["permissions"] = json!({"data_access": "read_only"});
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let audit = audit_engine::AuditEngine::new(pool.clone());
    let rt = PluginRuntime::open(pool.clone(), audit);
    // read_only 插件可安装（登记面），但摄取被拒（不直写主库表，原则 2）。
    install_plugin(&rt, &pkg, &manifest);
    let err = ingest_knowledge_source(&pool, &manifest, &pkg).unwrap_err();
    assert_eq!(err.code, "ingest_permission_denied");
    assert!(err.to_json_string().contains("principle 2"));
    // 数据面零行。
    assert!(kind_counts(&pool, "knowledge.fixture.corpus").is_empty());
}

// ---------------------------------------------------------------------------
// content_kinds 白名单外拒绝
// ---------------------------------------------------------------------------

#[test]
fn ingest_unknown_content_kind_rejected() {
    let root = unique("kind");
    let pkg = build_corpus(&root);
    let mut manifest = fixture_manifest();
    manifest.as_object_mut().unwrap()["capabilities"][0]["provides"]["content_kinds"] =
        json!(["poc", "skill_md", "method_case", "wiki"]);
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let audit = audit_engine::AuditEngine::new(pool.clone());
    let rt = PluginRuntime::open(pool.clone(), audit);
    install_plugin(&rt, &pkg, &manifest);
    let err = ingest_knowledge_source(&pool, &manifest, &pkg).unwrap_err();
    assert_eq!(err.code, "ingest_kind_unknown");
}

// ---------------------------------------------------------------------------
// 卸载不删已入主库知识（retain_with_provenance，§5.4）
// ---------------------------------------------------------------------------

#[test]
fn uninstall_retains_ingested_rows() {
    let root = unique("retain");
    let pkg = build_corpus(&root);
    let manifest = fixture_manifest();
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let audit = audit_engine::AuditEngine::new(pool.clone());
    let rt = PluginRuntime::open(pool.clone(), audit).with_data_root(root.join("data_root"));
    install_plugin(&rt, &pkg, &manifest);
    ingest_knowledge_source(&pool, &manifest, &pkg).unwrap();
    let before: i64 = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM knowledge_ingest WHERE plugin_id = ?1",
                ["knowledge.fixture.corpus"],
                |r| r.get(0),
            )
            .map_err(database::DbError::from)
        })
        .unwrap();
    assert_eq!(before, 7);

    // uninstall：插件行标记来源已卸载，knowledge_ingest 行保留（计划 415 行）。
    rt.uninstall("user:test", "knowledge.fixture.corpus")
        .unwrap();
    let row = rt
        .store()
        .get_active("knowledge.fixture.corpus")
        .unwrap()
        .unwrap();
    assert!(row.source_uninstalled);
    let after: i64 = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM knowledge_ingest WHERE plugin_id = ?1",
                ["knowledge.fixture.corpus"],
                |r| r.get(0),
            )
            .map_err(database::DbError::from)
        })
        .unwrap();
    assert_eq!(
        after, 7,
        "ingested knowledge retained after uninstall (plan 415)"
    );
}

// ---------------------------------------------------------------------------
// 真实契约示例仍过七条（pinned 全量 sha + UNLICENSED 裁决后）
// ---------------------------------------------------------------------------

#[test]
fn contract_example_still_validates() {
    let yaml = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../schemas/plugin-manifest/examples/knowledge.rtis-redteam.yaml"),
    )
    .unwrap();
    let manifest = parse_document(&yaml).unwrap();
    // license 裁决：UNLICENSED（SPDX 保留标识）为 P6-5 前裁决结果。
    assert_eq!(manifest["license"], "UNLICENSED");
    // pinned 全量 sha（40 hex）。
    let pinned = manifest["source"]["pinned"]["value"].as_str().unwrap();
    assert_eq!(
        pinned.len(),
        40,
        "pinned commit is the full sha (git rev-parse HEAD)"
    );
    let report =
        plugin_registry::validate_document(&yaml, None, &Default::default(), &Default::default());
    assert!(
        report.ok,
        "{}",
        plugin_registry::render_report_text(&report)
    );
}
