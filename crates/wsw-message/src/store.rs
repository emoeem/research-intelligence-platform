//! `wsw_body_blob` 登记/查询/GC 接口（迁移 023 表；DESIGN §3.3）。
//!
//! rusqlite 经 `database` 重导出使用，不直连（先例
//! `crates/database/src/lib.rs:30-31`「下游 engine 只依赖本 crate，不直接
//! 声明 rusqlite」）；迁移执行器唯一为 `database::migrate`
//! （`crates/database/src/migrate.rs:115`）。
//!
//! GC 引用计数（DESIGN §3.4/§4.8）：引用源 = `wsw_exchange` 的
//! request/response_body_artifact（WSW-3 History 批次建表）与用户收藏 pin。
//! 本批先落 GC 原语 [`delete_body_blobs_unreferenced`]：调用方传入当前全部
//! 存活引用的 sha256 集合，接口删除零引用行并返回被删 sha 列表（调用方随
//! 之删正文文件）；retention 执行位归 WSW-3（DESIGN §4.8 明文）。

use database::rusqlite::{self, Connection};
use database::DbError;

/// `wsw_body_blob` 行（023 迁移表）。
#[derive(Debug, Clone, PartialEq)]
pub struct BodyBlobRow {
    /// 正文 sha256（小写 hex，64 字符，主键）。
    pub sha256: String,
    /// 存储相对 uri：`wsw/<sha256 前2>/<sha256>`。
    pub uri: String,
    /// 原始字节数。
    pub size_bytes: i64,
    /// MIME 类型。
    pub mime_type: String,
    /// 首见 exchange_id（六 ID/Exchange 引用链，DESIGN §3.3）。
    pub first_seen_exchange_id: Option<String>,
    /// 登记时刻（ISO8601 UTC）。
    pub created_at: String,
}

/// sha256 形状守卫（与 023 表 CHECK(length=64) 双保险；小写 hex）。
fn validate_row(row: &BodyBlobRow) -> Result<(), DbError> {
    let sha_valid = super::cas::is_valid_sha256(&row.sha256);
    let expected_uri = if sha_valid {
        format!("wsw/{}/{}", &row.sha256[..2], row.sha256)
    } else {
        String::new()
    };
    let exchange_id_valid = row
        .first_seen_exchange_id
        .as_ref()
        .is_none_or(|value| crate::ids::ExchangeId::try_new(value.clone()).is_ok());
    let mime_valid =
        !row.mime_type.trim().is_empty() && !row.mime_type.bytes().any(|b| b < 0x20 || b == 0x7f);
    let ok = sha_valid
        && row.uri == expected_uri
        && row.size_bytes >= 0
        && mime_valid
        && !row.created_at.trim().is_empty()
        && exchange_id_valid;
    if !ok {
        return Err(DbError::Validation(format!(
            "invalid wsw_body_blob row: sha256={:?} uri={:?} size={}",
            row.sha256, row.uri, row.size_bytes
        )));
    }
    Ok(())
}

/// 登记正文 blob（INSERT OR IGNORE：同 sha256 CAS 去重幂等，022/018/020/021
/// 先例形态）。
pub fn register_body_blob(conn: &Connection, row: &BodyBlobRow) -> Result<(), DbError> {
    validate_row(row)?;
    conn.execute(
        "INSERT OR IGNORE INTO wsw_body_blob \
         (sha256, uri, size_bytes, mime_type, first_seen_exchange_id, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite_params(row),
    )
    .map_err(DbError::Sqlite)?;

    // INSERT OR IGNORE 只对相同内容寻址幂等；若主键已存在但 size/uri 不同，
    // 不能把冲突静默当作成功登记。
    let existing = get_body_blob(conn, &row.sha256)?
        .ok_or_else(|| DbError::Validation("CAS row missing after insert".into()))?;
    if existing.uri != row.uri || existing.size_bytes != row.size_bytes {
        return Err(DbError::Validation(format!(
            "CAS metadata conflict for sha256 {}",
            row.sha256
        )));
    }
    Ok(())
}

fn rusqlite_params(row: &BodyBlobRow) -> [&dyn rusqlite::ToSql; 6] {
    [
        &row.sha256,
        &row.uri,
        &row.size_bytes,
        &row.mime_type,
        &row.first_seen_exchange_id,
        &row.created_at,
    ]
}

/// 点查（sha256 → 行；CAS 命中语义）。
pub fn get_body_blob(conn: &Connection, sha256: &str) -> Result<Option<BodyBlobRow>, DbError> {
    conn.query_row(
        "SELECT sha256, uri, size_bytes, mime_type, first_seen_exchange_id, created_at \
         FROM wsw_body_blob WHERE sha256 = ?1",
        [sha256],
        |r| {
            Ok(BodyBlobRow {
                sha256: r.get(0)?,
                uri: r.get(1)?,
                size_bytes: r.get(2)?,
                mime_type: r.get(3)?,
                first_seen_exchange_id: r.get(4)?,
                created_at: r.get(5)?,
            })
        },
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(DbError::Sqlite(other)),
    })
}

/// 全量计数（对账用）。
pub fn blob_count(conn: &Connection) -> Result<u64, DbError> {
    conn.query_row("SELECT COUNT(*) FROM wsw_body_blob", [], |r| {
        r.get::<_, i64>(0)
    })
    .map(|n| n as u64)
    .map_err(DbError::Sqlite)
}

/// GC 原语：删除不在 `referenced` 存活引用集内的行，返回被删 sha256 列表
/// （调用方随之删正文文件；DESIGN §3.4「删除含正文（CAS 引用归零即删
/// 文件）」）。引用计数全量对账语义 —— `wsw_exchange` 表落位（WSW-3）后由
/// retention 执行位组装 `referenced`。
pub fn delete_body_blobs_unreferenced(
    conn: &Connection,
    referenced: &[&str],
) -> Result<Vec<String>, DbError> {
    let all: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT sha256 FROM wsw_body_blob")
            .map_err(DbError::Sqlite)?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(DbError::Sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(DbError::Sqlite)?;
        rows
    };
    let mut deleted = Vec::new();
    for sha in all {
        if !referenced.contains(&sha.as_str()) {
            conn.execute("DELETE FROM wsw_body_blob WHERE sha256 = ?1", [&sha])
                .map_err(DbError::Sqlite)?;
            deleted.push(sha);
        }
    }
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{migrate, DbPath, Pool, PoolConfig};
    use std::path::{Path, PathBuf};

    fn migrations_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("migrations")
    }

    fn mem_pool() -> Pool {
        Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: std::time::Duration::from_secs(5),
        })
        .expect("in-memory pool")
    }

    fn row(sha: &str) -> BodyBlobRow {
        BodyBlobRow {
            sha256: sha.to_string(),
            uri: format!("wsw/{}/{}", &sha[..2], sha),
            size_bytes: 42,
            mime_type: "application/octet-stream".into(),
            first_seen_exchange_id: Some("wsw-20261118000000-0-1f2e".into()),
            created_at: "2026-11-18T00:00:00Z".into(),
        }
    }

    #[test]
    fn migration_023_and_register_get_gc_roundtrip_in_memory() {
        let pool = mem_pool();
        migrate(&pool, &migrations_dir()).expect("migrations 全链含 023");

        pool.with_conn(|conn| -> Result<(), database::DbError> {
            let level: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
            assert_eq!(
                level, 25,
                "内存库迁移水位 25（025_wsw_exchange 入链后链尾）"
            );
            let sha = "ab".to_string() + &"0".repeat(62);
            register_body_blob(conn, &row(&sha)).expect("登记");
            // CAS 去重：同 sha 幂等，行数不增
            register_body_blob(conn, &row(&sha)).expect("幂等重登记");
            assert_eq!(blob_count(conn).unwrap(), 1);

            let got = get_body_blob(conn, &sha).unwrap().expect("点查命中");
            assert_eq!(got.uri, format!("wsw/ab/{sha}"));
            assert_eq!(got.size_bytes, 42);
            assert_eq!(got.mime_type, "application/octet-stream");
            assert_eq!(
                got.first_seen_exchange_id.as_deref(),
                Some("wsw-20261118000000-0-1f2e")
            );
            let absent_sha: String = "cd".to_string() + &"0".repeat(62);
            assert!(get_body_blob(conn, &absent_sha).unwrap().is_none());

            // GC 原语：引用集外的行删除并返回
            let deleted = delete_body_blobs_unreferenced(conn, &[&sha]).unwrap();
            assert!(deleted.is_empty(), "存活引用不被删");
            let deleted = delete_body_blobs_unreferenced(conn, &[]).unwrap();
            assert_eq!(deleted, vec![sha.clone()]);
            assert_eq!(blob_count(conn).unwrap(), 0);
            Ok(())
        })
        .expect("with_conn");
    }

    #[test]
    fn uri_must_match_content_address_and_duplicate_metadata_must_agree() {
        let pool = mem_pool();
        migrate(&pool, &migrations_dir()).expect("migrations");
        pool.with_conn(|conn| -> Result<(), database::DbError> {
            let sha = "cd".to_string() + &"0".repeat(62);
            let mut invalid_uri = row(&sha);
            invalid_uri.uri = "wsw/../outside".into();
            assert!(matches!(
                register_body_blob(conn, &invalid_uri),
                Err(DbError::Validation(_))
            ));

            let original = row(&sha);
            register_body_blob(conn, &original).expect("first registration");
            let mut conflicting_size = original.clone();
            conflicting_size.size_bytes += 1;
            assert!(matches!(
                register_body_blob(conn, &conflicting_size),
                Err(DbError::Validation(_))
            ));
            let stored = get_body_blob(conn, &sha).unwrap().unwrap();
            assert_eq!(stored.size_bytes, original.size_bytes);
            Ok(())
        })
        .expect("with_conn");
    }

    #[test]
    fn schema_rejects_uppercase_hash_and_noncanonical_uri() {
        let pool = mem_pool();
        migrate(&pool, &migrations_dir()).expect("migrations");
        pool.with_conn(|conn| -> Result<(), database::DbError> {
            let upper = "AB".to_string() + &"0".repeat(62);
            let uri = format!("wsw/AB/{upper}");
            let result = conn.execute(
                "INSERT INTO wsw_body_blob (sha256, uri, size_bytes, mime_type, created_at)
                 VALUES (?1, ?2, 1, 'application/octet-stream', '2026-11-18T00:00:00Z')",
                [&upper, &uri],
            );
            assert!(result.is_err(), "SQL CHECK must reject uppercase SHA-256");
            Ok(())
        })
        .expect("with_conn");
    }

    #[test]
    fn invalid_sha256_row_rejected() {
        let pool = mem_pool();
        migrate(&pool, &migrations_dir()).expect("migrations");
        pool.with_conn(|conn| -> Result<(), database::DbError> {
            let uppercase_sha = "AB".to_string() + &"0".repeat(62);
            let mut bad = row(&uppercase_sha); // 大写 hex 非法
            bad.sha256 = bad.sha256.to_uppercase();
            let err = register_body_blob(conn, &bad).unwrap_err();
            assert!(matches!(err, DbError::Validation(_)));
            let mut short = row("ab00"); // 长度非法
            short.sha256 = "ab00".into();
            assert!(register_body_blob(conn, &short).is_err());
            Ok(())
        })
        .expect("with_conn");
    }
}
