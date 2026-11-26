use database::{migrate, DbPath, Pool, PoolConfig};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../migrations")
        .canonicalize()
        .unwrap()
}

#[test]
fn migration_032_preserves_existing_rows_and_adds_only_nuclei_template_kind() {
    let source = migrations_dir();
    let old_dir = std::env::temp_dir().join(format!("rip-migrations-031-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&old_dir);
    std::fs::create_dir_all(&old_dir).unwrap();
    for entry in std::fs::read_dir(&source).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("sql") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy();
        if name.starts_with("032_") {
            continue;
        }
        std::fs::copy(&path, old_dir.join(name.as_ref())).unwrap();
    }

    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(3),
    })
    .unwrap();
    let before = migrate(&pool, &old_dir).unwrap();
    assert_eq!(before.last().map(|m| m.version), Some(31));
    pool.with_conn(|c| {
        c.execute(
            "INSERT INTO knowledge_source_entry (
                plugin_id, plugin_version, corpus_id, entry_kind, item_key, title,
                body, body_bytes, content_sha256, metadata_sha256, category_path_json,
                license_status, rights_policy, review_status, first_seen_at, last_seen_at
             ) VALUES (
                'knowledge.legacy','1.0.0','legacy','document_markdown','docs/a.md',
                'Legacy approved doc','legacy body',11,'content-hash','metadata-hash','[]',
                'MIT','approved_only','APPROVED','2026-11-25T00:00:00Z','2026-11-25T00:00:00Z'
             )",
            [],
        )
    })
    .unwrap();

    let applied = migrate(&pool, &source).unwrap();
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0].version, 32);
    let legacy: (String, String, String) = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT entry_kind, review_status, body FROM knowledge_source_entry
                 WHERE plugin_id='knowledge.legacy'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
        })
        .unwrap();
    assert_eq!(
        legacy,
        (
            "document_markdown".to_string(),
            "APPROVED".to_string(),
            "legacy body".to_string()
        )
    );

    pool.with_conn(|c| {
        c.execute(
            "INSERT INTO knowledge_source_entry (
                plugin_id, plugin_version, corpus_id, entry_kind, item_key, title,
                body, content_sha256, metadata_sha256, first_seen_at, last_seen_at
             ) VALUES (
                'knowledge.nuclei','0.2.0','rtis-redteam','nuclei_template',
                'http/cves/CVE-2025-12345.yaml','Example Nuclei template',
                'id: cve-2025-12345','sha','meta',
                '2026-11-26T00:00:00Z','2026-11-26T00:00:00Z'
             )",
            [],
        )
    })
    .unwrap();

    let rejected = pool.with_conn(|c| {
        c.execute(
            "INSERT INTO knowledge_source_entry (
                plugin_id, plugin_version, corpus_id, entry_kind, item_key, title,
                body, content_sha256, metadata_sha256, first_seen_at, last_seen_at
             ) VALUES ('bad','1','c','nuclei-template','x','bad','','sha','meta','now','now')",
            [],
        )
    });
    assert!(rejected.is_err(), "the entry_kind CHECK remains closed");

    let version: i64 = pool
        .with_conn(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(version, 32);
    let _ = std::fs::remove_dir_all(&old_dir);
}
