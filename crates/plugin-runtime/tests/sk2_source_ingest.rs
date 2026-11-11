//! SK-2 集成测试（P6-10，安全知识源计划 §3/§4.2/§8 P6-10）：三源真实
//! Manifest（manifests/plugins/）+ 固定 Commit 最小快照 fixture
//! （tests/fixtures/sk2/，见 FIXTURE-NOTES.md）的摄取链：
//! - 条数对账：awesome-hacking 85（分类表 81 + 自链/徽章/社交 4，no_scheme 2）、
//!   secskills 102 唯一键（README 102 目录条目含 1 条重复 URL 合并 + mseep
//!   徽章 1，no_scheme 17）、tongui 75（74 篇 + 来源主页，精选 31 双 ref）；
//! - 幂等：二跑 skipped=全量、changed=0；内容变更走 changed 计数不覆盖历史行；
//!   上游条目删除不物理删已有行；
//! - 许可/策略：UNLICENSED + metadata_only（tongui）、catalog_only（ah/secs）、
//!   document_markdown 恒 0（无许可正文不批量持久化）；
//! - 审核闸：APPROVED + 可见插件双闸进 Search 第三源；禁用即撤出。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use database::{DbPath, Pool, PoolConfig};
use domain::knowledge::ReviewStatus;
use domain::plugin::PluginTrust;
use plugin_registry::parse_document;
use plugin_runtime::{ingest_knowledge_source_with_opts, IngestOptions, PluginRuntime};

fn unique(tag: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("sk2-{tag}-{}-{n}", std::process::id()));
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

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn migrations_dir() -> PathBuf {
    repo_root().join("migrations")
}

/// fixture 拷贝进临时目录（install 落 checksums/plugin.lock，不得污染仓库 fixture）。
fn fixture_package(fixture: &str) -> PathBuf {
    fn copy_tree(src: &Path, dst: &Path) {
        fs::create_dir_all(dst).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let target = dst.join(entry.file_name());
            if entry.path().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), &target).unwrap();
            }
        }
    }
    let src = repo_root().join("tests/fixtures/sk2").join(fixture);
    let dir = unique(fixture);
    copy_tree(&src, &dir);
    dir
}

fn sk2_manifest(id: &str) -> serde_json::Value {
    let text = fs::read_to_string(repo_root().join(format!("manifests/plugins/{id}.yaml"))).unwrap();
    parse_document(&text).unwrap()
}

fn pinned_of(manifest: &serde_json::Value) -> String {
    manifest["source"]["pinned"]["value"]
        .as_str()
        .unwrap()
        .to_string()
}

fn kse_count(pool: &Pool, where_clause: &str) -> i64 {
    let sql = format!("SELECT COUNT(*) FROM knowledge_source_entry {where_clause}");
    pool.with_conn(|c| c.query_row(&sql, [], |r| r.get(0)))
        .unwrap()
}

fn setup_plugin(_pool: &Pool, rt: &PluginRuntime, id: &str, fixture: &str) -> PathBuf {
    let pkg = fixture_package(fixture);
    let manifest = sk2_manifest(id);
    rt.install("user:test", &manifest, &pkg, PluginTrust::Community)
        .unwrap();
    pkg
}

// ---------------------------------------------------------------------------
// 三源条数对账 + 幂等（fixture 全量摄取）
// ---------------------------------------------------------------------------

#[test]
fn sk2_three_sources_ingest_counts_reconcile_and_idempotence() {
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let audit = audit_engine::AuditEngine::new(pool.clone());
    let rt = PluginRuntime::open(pool.clone(), audit);

    // 同趟重复行：secskills README 代码审计分类内 skill-dfyx_code_security_review
    // 两条目同 URL（上游原文如此），规范化 canonical URL 去重后 102 唯一键，
    // 首跑该行计 skipped=1（去重生效的实测形态），二跑 skipped=103。
    let cases = [
        ("knowledge.awesome-hacking", "awesome-hacking", 85u64, 2u64, 0u64),
        ("knowledge.secskills", "secskills", 102, 17, 1),
        ("knowledge.tongui-course-notes", "tongui-course-notes", 75, 0, 0),
    ];
    let mut total_rows = 0i64;
    for (id, fixture, expect_count, expect_no_scheme, in_pass_dup) in cases {
        let pkg = setup_plugin(&pool, &rt, id, fixture);
        let manifest = sk2_manifest(id);

        // dry-run：计数与实跑同构，零落盘。
        let dry = ingest_knowledge_source_with_opts(&pool, &manifest, &pkg, &IngestOptions { dry_run: true })
            .unwrap();
        assert_eq!(
            dry.counts.get("catalog_entry"),
            Some(&expect_count),
            "{id}: dry-run catalog_entry 计数与源 README/派生目录对账"
        );
        assert_eq!(
            dry.filtered.get("no_scheme").copied().unwrap_or(0),
            expect_no_scheme,
            "{id}: 无 scheme 过滤计数（0 时报告不携带该键）"
        );
        assert_eq!(dry.skipped, in_pass_dup, "{id}: dry-run skipped = 源内同键重复行数");
        let before = kse_count(&pool, &format!("WHERE plugin_id = '{id}'"));
        assert_eq!(before, 0, "{id}: dry-run 零落盘");

        // 实跑。
        let report = ingest_knowledge_source_with_opts(&pool, &manifest, &pkg, &IngestOptions::default())
            .unwrap();
        assert_eq!(report.counts.get("catalog_entry"), Some(&expect_count), "{id}");
        assert_eq!(report.changed, 0, "{id}: 首跑无内容变更");
        let rows = kse_count(&pool, &format!("WHERE plugin_id = '{id}'"));
        assert_eq!(rows, expect_count as i64, "{id}: 落表行数与计数一致");
        total_rows += rows;

        // 幂等二跑：skipped=全量（含源内同键重复行），counts 空，changed=0。
        let again = ingest_knowledge_source_with_opts(&pool, &manifest, &pkg, &IngestOptions::default())
            .unwrap();
        assert!(again.counts.is_empty(), "{id}: 二跑零新增");
        assert_eq!(again.skipped, expect_count + in_pass_dup, "{id}: 二跑 skipped=全量+源内重复");
        assert_eq!(again.changed, 0, "{id}: 二跑无变更");
    }
    assert_eq!(total_rows, 85 + 102 + 75, "三源合计行数");

    // 溯源与策略分组（python 只读断言的测试面镜像）：
    // license_status = Manifest license 原值；rights_policy = per-kind 声明；
    // source_commit = 固定 Commit 全 40 位。
    let groups: Vec<(String, String, i64)> = pool
        .with_conn(|c| {
            let mut stmt = c
                .prepare("SELECT license_status, rights_policy, COUNT(*) FROM knowledge_source_entry GROUP BY 1, 2 ORDER BY 1, 2")
                .unwrap();
            let rows = stmt
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            Ok::<Vec<_>, database::rusqlite::Error>(rows)
        })
        .unwrap();
    assert_eq!(
        groups,
        vec![
            ("Apache-2.0".to_string(), "catalog_only".to_string(), 102),
            ("CC0-1.0".to_string(), "catalog_only".to_string(), 85),
            ("UNLICENSED".to_string(), "metadata_only".to_string(), 75),
        ],
        "三源各行 license/rights 策略正确：{groups:?}"
    );
    for id in [
        "knowledge.awesome-hacking",
        "knowledge.secskills",
        "knowledge.tongui-course-notes",
    ] {
        let manifest = sk2_manifest(id);
        let pinned = pinned_of(&manifest);
        let bad = kse_count(
            &pool,
            &format!("WHERE plugin_id = '{id}' AND source_commit != '{pinned}'"),
        );
        assert_eq!(bad, 0, "{id}: 全行快照 Commit = 固定值");
        let short = kse_count(
            &pool,
            &format!("WHERE plugin_id = '{id}' AND length(source_commit) != 40"),
        );
        assert_eq!(short, 0, "{id}: pinned 为全 40 位 sha");
    }

    // tongui 无许可正文红线：document_markdown 恒 0；正文不批量持久化。
    assert_eq!(
        kse_count(&pool, "WHERE plugin_id = 'knowledge.tongui-course-notes' AND entry_kind = 'document_markdown'"),
        0,
        "tongui document_markdown 必须 0（UNLICENSED 正文不落库）"
    );
}

// ---------------------------------------------------------------------------
// tongui 合并语义：74 篇 + 主页，精选 31 双 source_ref，Git 路径为正文来源
// ---------------------------------------------------------------------------

#[test]
fn sk2_tongui_merge_dual_source_ref_metadata_only() {
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let audit = audit_engine::AuditEngine::new(pool.clone());
    let rt = PluginRuntime::open(pool.clone(), audit);
    let pkg = setup_plugin(&pool, &rt, "knowledge.tongui-course-notes", "tongui-course-notes");
    let manifest = sk2_manifest("knowledge.tongui-course-notes");
    ingest_knowledge_source_with_opts(&pool, &manifest, &pkg, &IngestOptions::default()).unwrap();

    // 逻辑资源 = 74：Git 路径条目数（主页条目不带 Git路径=）。
    let git_path_rows = kse_count(
        &pool,
        "WHERE plugin_id = 'knowledge.tongui-course-notes' AND description LIKE '%Git路径=渗透测试全套/%'",
    );
    assert_eq!(git_path_rows, 74, "74 篇课程条目全部携带 Git 路径（正文来源）");
    let homepage = kse_count(
        &pool,
        "WHERE plugin_id = 'knowledge.tongui-course-notes' AND item_key = 'https://tongui66.github.io/course-notes'",
    );
    assert_eq!(homepage, 1, "来源主页条目恰 1 条");

    // 精选合并：31 篇 canonical_url 用站点 URL（双 source_ref 的第一 ref），
    // Git 路径与 blob URL 落在描述（第二 ref）；未精选 43 篇 canonical_url 用
    // blob URL。
    let featured = kse_count(
        &pool,
        "WHERE plugin_id = 'knowledge.tongui-course-notes'
           AND canonical_url LIKE 'https://tongui66.github.io/course-notes/%'
           AND item_key != 'https://tongui66.github.io/course-notes'",
    );
    assert_eq!(featured, 31, "精选站点 31 篇与仓库 74 篇同文章合并为同一逻辑资源");
    let blob_only = kse_count(
        &pool,
        "WHERE plugin_id = 'knowledge.tongui-course-notes'
           AND canonical_url LIKE 'https://github.com/tongui66/Cyber-Security-Course-Notes/blob/%'",
    );
    assert_eq!(blob_only, 43, "未精选 43 篇以 blob URL 为 canonical");
    let dual_ref = kse_count(
        &pool,
        "WHERE plugin_id = 'knowledge.tongui-course-notes'
           AND canonical_url LIKE 'https://tongui66.github.io/course-notes/%'
           AND description LIKE '%Blob=https://github.com/tongui66/Cyber-Security-Course-Notes/blob/%'
           AND description LIKE '%Git路径=渗透测试全套/%'",
    );
    assert_eq!(dual_ref, 31, "精选条目双 source_ref（站点 URL + Git 路径 + blob）");

    // 元数据形态：章节（category_path 含章名）+ 标题（编号 课程）+ 哈希。
    let (title, cat, body, desc): (String, String, String, String) = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT title, category_path_json, body, description FROM knowledge_source_entry
                 WHERE plugin_id = 'knowledge.tongui-course-notes' AND title LIKE '23 %'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .map_err(database::DbError::from)
        })
        .unwrap();
    assert!(title.starts_with("23 "), "标题带编号前缀: {title}");
    assert!(cat.contains("渗透测试全套"), "分类路径含课程体系: {cat}");
    assert!(body.contains("Git路径="), "正文（元数据行）携带 Git 路径: {body}");
    assert!(desc.contains("状态="), "描述携带章节/状态元数据: {desc}");
}

// ---------------------------------------------------------------------------
// 内容变更走 changed 计数不覆盖历史行；上游删除不物理删
// ---------------------------------------------------------------------------

#[test]
fn sk2_changed_counts_preserve_history_and_no_physical_delete() {
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let audit = audit_engine::AuditEngine::new(pool.clone());
    let rt = PluginRuntime::open(pool.clone(), audit);
    let pkg = setup_plugin(&pool, &rt, "knowledge.tongui-course-notes", "tongui-course-notes");
    let manifest = sk2_manifest("knowledge.tongui-course-notes");
    ingest_knowledge_source_with_opts(&pool, &manifest, &pkg, &IngestOptions::default()).unwrap();

    // 取一行历史内容（首条 blob 条目）。
    let (key, old_title, old_sha): (String, String, String) = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT item_key, title, content_sha256 FROM knowledge_source_entry
                 WHERE plugin_id = 'knowledge.tongui-course-notes'
                   AND item_key LIKE 'https://github.com/%' LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(database::DbError::from)
        })
        .unwrap();

    // 同键改标题（内容变）：changed=1，历史行不被覆盖。
    let catalog = pkg.join("catalog/articles.md");
    let text = fs::read_to_string(&catalog).unwrap();
    let marker = format!("[{old_title}]");
    let idx = text.find(&marker).unwrap_or_else(|| panic!("marker {marker} in catalog"));
    let mutated = format!(
        "{}[{}（更名）]{}",
        &text[..idx],
        old_title,
        &text[idx + marker.len()..]
    );
    fs::write(&catalog, &mutated).unwrap();
    let report = ingest_knowledge_source_with_opts(&pool, &manifest, &pkg, &IngestOptions::default()).unwrap();
    assert_eq!(report.changed, 1, "键同内容变 = changed 计数");
    assert_eq!(report.skipped, 75, "其余 74 行 skipped + 变更行本身计 skipped");
    let (title_now, sha_now): (String, String) = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT title, content_sha256 FROM knowledge_source_entry
                 WHERE plugin_id = 'knowledge.tongui-course-notes' AND item_key = ?1",
                [&key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(database::DbError::from)
        })
        .unwrap();
    assert_eq!(title_now, old_title, "历史行不被覆盖（Evidence 可复核）");
    assert_eq!(sha_now, old_sha, "内容哈希保持历史值");

    // 上游删除：从目录移除该条目再摄取，行保留（不物理删）；
    // upstream_missing 自动标记属后续 sync 批次（本批列恒 0，见 NOTES §六）。
    let line_start = text[..idx]
        .rfind("\n- [")
        .map(|i| i + 1)
        .unwrap_or(0);
    let line_end = text[idx..].find('\n').map(|i| idx + i + 1).unwrap_or(text.len());
    let without = format!("{}{}", &text[..line_start], &text[line_end..]);
    fs::write(&catalog, &without).unwrap();
    let report2 = ingest_knowledge_source_with_opts(&pool, &manifest, &pkg, &IngestOptions::default()).unwrap();
    assert_eq!(report2.counts.get("catalog_entry"), None, "删除后不再新增");
    let still = kse_count(
        &pool,
        &format!("WHERE plugin_id = 'knowledge.tongui-course-notes' AND item_key = '{key}'"),
    );
    assert_eq!(still, 1, "上游删除不物理删已有行（保留历史溯源）");
}

// ---------------------------------------------------------------------------
// 审核闸 + 检索面：APPROVED + 可见插件双闸，禁用即撤出
// ---------------------------------------------------------------------------

#[test]
fn sk2_review_gate_search_visibility_and_disable() {
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let audit = audit_engine::AuditEngine::new(pool.clone());
    let rt = PluginRuntime::open(pool.clone(), audit);
    let audit = audit_engine::AuditEngine::new(pool.clone());

    let cases = [
        ("knowledge.awesome-hacking", "awesome-hacking"),
        ("knowledge.secskills", "secskills"),
        ("knowledge.tongui-course-notes", "tongui-course-notes"),
    ];
    for (id, fixture) in &cases {
        setup_plugin(&pool, &rt, id, fixture);
    }
    for (id, _) in &cases {
        let manifest = sk2_manifest(id);
        let pkg_dir: String = pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT installed_dir FROM plugin WHERE plugin_id = ?1",
                    [id],
                    |r| r.get(0),
                )
                .map_err(database::DbError::from)
            })
            .unwrap();
        ingest_knowledge_source_with_opts(&pool, &manifest, Path::new(&pkg_dir), &IngestOptions::default())
            .unwrap();
    }
    // 初始 DISCOVERED：三源全量不进索引。
    let catalog = memory_pool();
    let index = memory_pool();
    let report = search_engine::SearchIndex::build(&pool, &catalog, &index).unwrap();
    assert_eq!(report.doc_count, 0, "DISCOVERED 不进检索面");

    // review --status APPROVED（三源全量翻转，模拟治理通过）。
    for (id, _) in &cases {
        let keys: Vec<String> = pool
            .with_conn(|c| {
                let mut stmt = c
                    .prepare("SELECT item_key FROM knowledge_source_entry WHERE plugin_id = ?1")
                    .unwrap();
                let rows = stmt
                    .query_map([id], |r| r.get::<_, String>(0))
                    .unwrap()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap();
                Ok::<Vec<_>, database::rusqlite::Error>(rows)
            })
            .unwrap();
        let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();
        let updated = plugin_runtime::knowledge_review::set_review_status(
            &pool,
            &audit,
            "user:test",
            id,
            &key_refs,
            ReviewStatus::Approved,
        )
        .unwrap();
        assert!(updated > 0, "{id}: 审核翻转生效");
    }

    // 插件可见（install 即 REGISTERED，enable 进可见态）→ 三源条目入索引。
    for (id, _) in &cases {
        rt.enable("user:test", id).unwrap();
    }
    search_engine::SearchIndex::rebuild(&pool, &catalog, &index).unwrap();
    assert_eq!(
        search_engine::SearchIndex::doc_count(&index).unwrap(),
        85 + 102 + 75,
        "APPROVED + 可见双闸通过后三源全量入索引"
    );

    // 禁用任一插件：rebuild 后该插件条目立即不可见，其余两源不受影响。
    rt.disable("user:test", "knowledge.secskills").unwrap();
    search_engine::SearchIndex::rebuild(&pool, &catalog, &index).unwrap();
    assert_eq!(
        search_engine::SearchIndex::doc_count(&index).unwrap(),
        85 + 75,
        "禁用 secskills 后仅撤出其条目（APPROVED+可见双闸）"
    );
}
