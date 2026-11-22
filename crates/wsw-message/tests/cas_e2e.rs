//! CAS + migration end-to-end checks. All database writes use temporary copies or memory DBs.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use database::{migrate, DbPath, Pool, PoolConfig};
use wsw_message::cas::{read_body, sha256_hex, store_body};
use wsw_message::parse::parse_wire;
use wsw_message::store::{blob_count, get_body_blob, register_body_blob, BodyBlobRow};
use wsw_message::types::CaptureFidelity;

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("migrations")
}

fn temp_root(tag: &str) -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "wsw-message-{tag}-{}-{}-{n}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&path).expect("create temporary test directory");
    path
}

fn memory_pool() -> Pool {
    Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("open in-memory database")
}

#[test]
fn cas_artifact_dedup_truncation_and_registration_roundtrip() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/wire/negative/oversize_body.http");
    let wire = fs::read(fixture).expect("read oversize body fixture");
    let parsed = parse_wire(&wire);
    assert_eq!(parsed.parse_status, wsw_message::types::ParseStatus::Ok);
    let body = parsed.message.expect("parsed body").body;
    assert!(body.len() > 8);

    let root = temp_root("cas");
    let first = store_body(&root, &body, 8).expect("store first body");
    assert_eq!(first.fidelity, CaptureFidelity::Truncated);
    assert_eq!(first.sha256, sha256_hex(&body));
    assert_eq!(first.size, body.len() as u64);
    assert_eq!(
        first.uri,
        format!("wsw/{}/{}", &first.sha256[..2], first.sha256)
    );
    let path = root.join(&first.uri);
    assert!(path.is_file());
    let stored = read_body(&root, &first.uri).expect("read CAS body");
    assert_eq!(stored, body);
    assert_eq!(sha256_hex(&stored), first.sha256);

    let second = store_body(&root, &body, 8).expect("deduplicated store");
    assert_eq!(second, first, "same bytes must resolve to one BodyRef");

    let pool = memory_pool();
    migrate(&pool, &migrations_dir()).expect("run migrations on in-memory database");
    let exchange_id = wsw_message::ids::ExchangeId::generate().to_string();
    let row = BodyBlobRow {
        sha256: first.sha256.clone(),
        uri: first.uri.clone(),
        size_bytes: first.size as i64,
        mime_type: "application/octet-stream".into(),
        first_seen_exchange_id: Some(exchange_id.clone()),
        created_at: "2026-11-18T00:00:00Z".into(),
    };
    pool.with_conn(|conn| -> Result<(), database::DbError> {
        register_body_blob(conn, &row).expect("register CAS row");
        register_body_blob(conn, &row).expect("idempotent duplicate registration");
        assert_eq!(blob_count(conn).expect("count"), 1);
        let found = get_body_blob(conn, &first.sha256)
            .expect("query")
            .expect("CAS row exists");
        assert_eq!(found.sha256, first.sha256);
        assert_eq!(found.uri, first.uri);
        assert_eq!(found.size_bytes, first.size as i64);
        assert_eq!(
            found.first_seen_exchange_id.as_deref(),
            Some(exchange_id.as_str())
        );
        Ok(())
    })
    .expect("database roundtrip");

    fs::remove_dir_all(root).expect("remove CAS temp directory");
}

#[test]
fn dev_b1_copy_applies_migration_023_without_touching_source_database() {
    let source = migrations_dir().join("dev-b1.db");
    assert!(
        source.is_file(),
        "expected development database copy at {}",
        source.display()
    );
    let source_sha_before = sha256_hex(&fs::read(&source).expect("read source DB read-only"));
    let root = temp_root("dev-b1-copy");
    let copy = root.join("dev-b1-wsw1-copy.db");
    fs::copy(&source, &copy).expect("copy dev-b1 database; source remains read-only");

    let pool = Pool::open(PoolConfig {
        path: DbPath::File(copy.clone()),
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("open disposable database copy");
    let source_copy_version: i64 = pool
        .with_conn(|conn| conn.query_row("PRAGMA user_version", [], |row| row.get(0)))
        .expect("read copied DB version before migration");
    // dev-b1.db 为 gitignored 开发副本：027 落地后本地遗留旧态，CI 测试二进制并行
    // 时序下也可能读到重建前状态——副本先幂等迁到当前链头再断言（migrate 只写副本；
    // 023..027 建表全 IF NOT EXISTS、登记行全 INSERT OR IGNORE，回放安全）。
    // 源文件零写入由 sha256 前后比对断言承载。
    if source_copy_version < 27 {
        migrate(&pool, &migrations_dir()).expect("bring disposable dev-b1 copy to chain tail");
    }
    let source_copy_version: i64 = pool
        .with_conn(|conn| conn.query_row("PRAGMA user_version", [], |row| row.get(0)))
        .expect("read copied DB version after bring-up");
    assert_eq!(
        source_copy_version, 27,
        "disposable copy must reach current chain tail 027 before the rewind simulation"
    );
    let existing_table: i64 = pool
        .with_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='wsw_body_blob'",
                [],
                |row| row.get(0),
            )
        })
        .expect("check current dev-b1 copy table");
    assert_eq!(existing_table, 1);

    // 源 dev-b1 已有 023。仅在临时副本回退 023 的表/版本行，模拟 pre-023
    // 状态，再验证正式 migration executor 能真实应用 023；源文件绝不写入。
    pool.with_conn(|conn| -> Result<(), database::DbError> {
        conn.execute_batch(
            "DROP TABLE wsw_body_blob;
             DELETE FROM schema_version WHERE version = 23;
             PRAGMA user_version = 22;",
        )?;
        Ok(())
    })
    .expect("prepare disposable pre-023 copy");
    let before_version: i64 = pool
        .with_conn(|conn| conn.query_row("PRAGMA user_version", [], |row| row.get(0)))
        .expect("read simulated pre-023 version");
    assert_eq!(before_version, 22);
    let applied = migrate(&pool, &migrations_dir()).expect("apply migrations to disposable copy");
    assert!(
        applied.iter().any(|item| item.version == 23),
        "023 must be applied to the disposable dev-b1 copy"
    );
    let version: i64 = pool
        .with_conn(|conn| conn.query_row("PRAGMA user_version", [], |row| row.get(0)))
        .expect("read copied DB version");
    assert_eq!(
        version, 27,
        "development copy must reach current chain tail 027 (023 re-applied after rollback)"
    );
    pool.with_conn(|conn| -> Result<(), database::DbError> {
        let table_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='wsw_body_blob'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(table_count, 1);
        Ok(())
    })
    .expect("verify migrated table");
    assert_eq!(version, 27);
    drop(pool);

    let source_sha_after = sha256_hex(&fs::read(&source).expect("re-read source DB read-only"));
    assert_eq!(
        source_sha_after, source_sha_before,
        "original dev-b1 database must remain byte-for-byte unchanged"
    );
    fs::remove_dir_all(root).expect("remove disposable database copy");
}
