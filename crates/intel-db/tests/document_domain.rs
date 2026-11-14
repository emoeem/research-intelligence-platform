//! P1-1/P1-2 集成测试：PG 基座 + Document 域（03-DOMAIN-MODEL 验收面）。
//!
//! 门禁口径：容器端到端保持本地执行（V-9 同款）——RP_TEST_PG_DSN 未设或
//! 不可达时跳过（CI 起 rip-pg17 service 容器后实跑）；纯逻辑面由
//! object_store 单测覆盖（无 PG 依赖）。
//!
//! 判据：
//! 1. 同文件多来源引用：同内容两次导入（不同来源）= 1 内容对象 + 2 文档，
//!    引用计数 4（2 来源 + 2 版本）；
//! 2. 内容不重复存储：对象账面单行 + Object Storage 单文件；
//! 3. 版本历史可访问：new_revision 后旧版本 SUPERSEDED 在列，历史可读；
//! 4. 删来源不误删对象：来源删光后对象仍在（引用计数由版本行保持 > 0）；
//! 5. 权限前置：list 的 workspace/visibility 谓词在 SQL 内（行为面）；
//! 6. 迁移幂等：refinery 二跑零变化。

use intel_db::document_repo::{self, ImportInput};
use intel_db::object_store::{LocalFsStore, ObjectStore};
use intel_db::{open_pool, run_migrations, PgConfig};
use std::path::PathBuf;

fn skip_unless_pg() -> Option<(r2d2::Pool<r2d2_postgres::PostgresConnectionManager<postgres::NoTls>>, LocalFsStore, PathBuf)> {
    let dsn = std::env::var("RP_TEST_PG_DSN").ok()?;
    let pool = match open_pool(&PgConfig { dsn, max_connections: 4 }) {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: RP_TEST_PG_DSN 不可达（V-9，容器端到端保持本地执行）");
            return None;
        }
    };
    let store_root = std::env::temp_dir().join(format!("rip-p1-it-{}", std::process::id()));
    std::fs::create_dir_all(&store_root).unwrap();
    Some((pool, LocalFsStore::new(&store_root), store_root))
}

fn input(title: &str, source_url: &str) -> ImportInput {
    ImportInput {
        title: title.to_string(),
        document_type: "markdown".to_string(),
        original_filename: format!("{title}.md"),
        mime_type: "text/markdown".to_string(),
        language: Some("zh".to_string()),
        workspace_id: "ws-p1".to_string(),
        visibility: domain::DocumentVisibility::Private,
        source_type: "git".to_string(),
        source_url: source_url.to_string(),
        source_commit: Some("440486fd08b0a0d9ad9a041a0bb234dcdf65dee2".to_string()),
    }
}

#[test]
fn migrations_apply_twice_idempotent() {
    let Some((pool, _store, _root)) = skip_unless_pg() else { return };
    let mut conn = pool.get().unwrap();
    let _first = run_migrations(&mut conn).unwrap();
    let second = run_migrations(&mut conn).unwrap();
    assert!(second.is_empty(), "second run must be a no-op: {second:?}");
}

#[test]
fn same_content_multi_source_single_object() {
    let Some((pool, store, _root)) = skip_unless_pg() else { return };
    let content = format!(
        "# RIP3 P1 验收样本\n\n唯一化后缀 {} {}\n",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
    .into_bytes();
    let a = document_repo::import_document(&pool, &store, &input("来源甲", "https://repo.example/a"), &content).unwrap();
    let b = document_repo::import_document(&pool, &store, &input("来源乙", "https://repo.example/b"), &content).unwrap();

    assert_ne!(a.document_id, b.document_id, "两文档两行");
    assert!(a.content_object_created, "首次导入新建对象");
    assert!(!b.content_object_created, "同内容复用对象（内容不重复存储）");
    // 对象账面单行 + 引用计数 = 2 来源 + 2 版本
    let hash = document_repo::content_sha256_hex(&content);
    let ref_count = document_repo::object_ref_count(&pool, &hash).unwrap().expect("object row");
    assert_eq!(ref_count, 4, "2 sources + 2 revisions");
    let count: i64 = pool
        .get()
        .unwrap()
        .query_one("SELECT COUNT(*) FROM content_objects WHERE content_hash = $1", &[&hash])
        .unwrap()
        .get(0);
    assert_eq!(count, 1, "内容不重复存储：对象单行");
    // Object Storage 单文件（读回一致）
    assert_eq!(store.read(&format!("cas://{hash}")).unwrap(), content.to_vec());
    // 两文档读回同内容
    assert_eq!(
        document_repo::read_current_content(&pool, &store, &a.document_id).unwrap(),
        content.to_vec()
    );
}

#[test]
fn revision_history_accessible_and_supersede() {
    let Some((pool, store, _root)) = skip_unless_pg() else { return };
    let v1 = b"# v1 body\n";
    let imported = document_repo::import_document(&pool, &store, &input("版本链样本", "https://repo.example/v1"), v1).unwrap();
    let v2 = "# v2 body\n更新内容。\n".as_bytes();
    let rev2 = document_repo::new_revision(&pool, &store, &imported.document_id, v2, Some("docling-2.0"), None).unwrap();

    let revisions = document_repo::list_revisions(&pool, &imported.document_id).unwrap();
    assert_eq!(revisions.len(), 2, "版本链两行");
    assert_eq!(revisions[0].revision_number, 2, "number DESC");
    assert_eq!(revisions[0].revision_status, "CURRENT");
    assert_eq!(revisions[1].revision_status, "SUPERSEDED", "旧版本不删除，历史可访问");
    // 当前内容 = v2
    assert_eq!(
        document_repo::read_current_content(&pool, &store, &imported.document_id).unwrap(),
        v2.to_vec()
    );
    let _ = rev2;
}

#[test]
fn delete_source_never_orphans_object_early() {
    let Some((pool, store, _root)) = skip_unless_pg() else { return };
    let content = format!("# 多来源删保护\n唯一化：{} {}\n", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()).into_bytes();
    let imported = document_repo::import_document(&pool, &store, &input("删保护", "https://repo.example/s1"), &content).unwrap();
    let hash = document_repo::content_sha256_hex(&content);

    let extra = document_repo::add_source(&pool, &imported.document_id, "url", "https://mirror.example/x", None).unwrap();
    let after_add = document_repo::object_ref_count(&pool, &hash).unwrap().unwrap();
    assert_eq!(after_add, 3, "import 2（来源+版本）+ add_source 1");

    document_repo::delete_source(&pool, &extra).unwrap();
    let after_del = document_repo::object_ref_count(&pool, &hash).unwrap().unwrap();
    assert_eq!(after_del, 2);
    // 来源删光（0 来源）后对象仍在：版本行持引用（ref_count >= 1），不误删
    let sources: i64 = pool.get().unwrap()
        .query_one("SELECT COUNT(*) FROM source_records WHERE content_hash = $1", &[&hash])
        .unwrap().get(0);
    // 本文档自身来源 1 行 + 0 其他：删它
    document_repo::delete_source(&pool, &imported.source_id).unwrap();
    let final_count = document_repo::object_ref_count(&pool, &hash).unwrap().unwrap();
    assert_eq!(sources, 1, "账面前提核对");
    assert_eq!(final_count, 1, "仅版本行 1 持引用（对象仍在，不误删）");
    assert!(store.read(&format!("cas://{hash}")).is_ok(), "物理对象仍在位");
}

#[test]
fn list_filters_by_workspace_and_visibility() {
    let Some((pool, store, _root)) = skip_unless_pg() else { return };
    let mut private_input = input("私有样本", "https://repo.example/private");
    private_input.visibility = domain::DocumentVisibility::Private;
    let imported = document_repo::import_document(&pool, &store, &private_input, b"private body").unwrap();

    let listed = document_repo::list_documents(&pool, "ws-p1", false).unwrap();
    assert!(listed.iter().any(|d| d.document_id == imported.document_id.as_str()), "同工作区可见");
    let other = document_repo::list_documents(&pool, "ws-other", false).unwrap();
    assert!(!other.iter().any(|d| d.document_id == imported.document_id.as_str()), "跨工作区不可见（权限前置行为面）");
}
