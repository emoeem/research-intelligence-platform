//! intel-db：RIP3 情报核 PostgreSQL 存取面（ADR-0001 绞杀者路线，P1-1/P1-2）。
//!
//! 职责：PG 连接池（同步 r2d2）、refinery 迁移执行（migrations_pg/ 每文件
//! 事务）、Document 域存取（document_repo，03-DOMAIN-MODEL 落库面）、
//! Object Storage Adapter 本地实现（object_store，S3 预留接口）。
//! 红线：本 crate 不触碰 SQLite 侧（运营核），不做 RTIS 直写（RTIS 接口经
//! rt.py CLI 既有适配，R2 3.4）。
//!
//! 运行配置：RP_PG_DSN（缺省 postgres://postgres:postgres@127.0.0.1:5432/postgres，
//! 指向 localhost/rip-pg17 容器服务，digest 见 containerfiles/README.md 台账）。
//! 迁移：migrations_pg/V0001 扩展装载 + V0002 起 Document 域，refinery 每文件
//! 事务 + checksum（12-STORAGE 第三节纪律）。

pub mod document_repo;
pub mod object_store;

use r2d2::Pool;
use r2d2_postgres::{postgres::NoTls, PostgresConnectionManager};
use std::fmt;

/// 情报核错误（手工 Display，纯 std 口径与 domain::error 同风格）。
#[derive(Debug)]
pub enum IntelError {
    /// 连接/查询失败。
    Pg(postgres::Error),
    /// 连接池失败。
    Pool(r2d2::Error),
    /// 迁移失败（refinery 报文透传）。
    Migrate(String),
    /// 域校验失败透传。
    Domain(domain::DomainError),
    /// 对象存储 IO 失败。
    Io(std::io::Error),
    /// 用法/参数非法。
    Invalid(String),
}

impl fmt::Display for IntelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pg(e) => write!(f, "pg error: {e}"),
            Self::Pool(e) => write!(f, "pool error: {e}"),
            Self::Migrate(m) => write!(f, "migration error: {m}"),
            Self::Domain(e) => write!(f, "domain error: {e}"),
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Invalid(m) => write!(f, "invalid: {m}"),
        }
    }
}

impl std::error::Error for IntelError {}

impl From<postgres::Error> for IntelError {
    fn from(e: postgres::Error) -> Self {
        Self::Pg(e)
    }
}

impl From<r2d2::Error> for IntelError {
    fn from(e: r2d2::Error) -> Self {
        Self::Pool(e)
    }
}

impl From<domain::DomainError> for IntelError {
    fn from(e: domain::DomainError) -> Self {
        Self::Domain(e)
    }
}

impl From<std::io::Error> for IntelError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

pub type IntelResult<T> = Result<T, IntelError>;

/// 同步 PG 连接别名（postgres Client）。
pub type PgConn = postgres::Client;

/// 同步连接池（r2d2 + postgres，NoTls：本地面容器，TLS 不在 P1 面）。
pub type PgPool = Pool<PostgresConnectionManager<NoTls>>;

/// 池配置（RP_PG_DSN 缺省口径见模块头注）。
#[derive(Debug, Clone)]
pub struct PgConfig {
    /// 连接串（libpq 形）。
    pub dsn: String,
    /// 最大连接数（单写者纪律对应物：情报核多写者由 PG MVCC 承接，池上限防突发）。
    pub max_connections: u32,
}

impl Default for PgConfig {
    fn default() -> Self {
        let dsn = std::env::var("RP_PG_DSN")
            .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/postgres".to_string());
        Self {
            dsn,
            max_connections: 4,
        }
    }
}

/// 打开同步连接池。
pub fn open_pool(cfg: &PgConfig) -> IntelResult<PgPool> {
    let config: postgres::Config =
        cfg.dsn.parse().map_err(|e| IntelError::Invalid(format!("bad RP_PG_DSN: {e}")))?;
    let manager = PostgresConnectionManager::new(config, NoTls);
    let pool = Pool::builder()
        .max_size(cfg.max_connections)
        .build(manager)?;
    Ok(pool)
}

/// 迁移嵌入（migrations_pg/ 于仓库根，路径相对 CARGO_MANIFEST_DIR）。
mod embedded {
    // 宏在模块内自引入（refinery 0.8 惯例：embed_migrations! 展开为
    // migrations/runner 项）。
    refinery::embed_migrations!("../../migrations_pg");
}

/// 执行 migrations_pg 迁移（refinery：每文件事务 + checksum 校验）。
/// 幂等：已应用版本跳过；返回应用报告（版本列表）。
pub fn run_migrations(client: &mut PgConn) -> IntelResult<Vec<u32>> {
    let report = embedded::migrations::runner()
        .run(client)
        .map_err(|e| IntelError::Migrate(e.to_string()))?;
    Ok(report.applied_migrations().iter().map(|m| m.version()).collect())
}

/// 健康面（selftest 与 /api/documents 健康卡共用）。
#[derive(Debug, serde::Serialize)]
pub struct HealthInfo {
    /// PG server 版本。
    pub server_version: String,
    /// 迁移链当前版本（refinery migrations 表最大 version；0 = 未迁移）。
    pub migration_version: u32,
    /// 扩展在位（vector/pg_search/pg_jieba）。
    pub extensions: Vec<String>,
    /// 情报核表行数抽样（documents/content_objects）。
    pub documents: i64,
    pub content_objects: i64,
}

/// 健康检查：连通 + 版本 + 扩展在位 + 迁移版本 + 行数抽样。
pub fn health(pool: &PgPool) -> IntelResult<HealthInfo> {
    let mut conn = pool.get()?;
    let server_version: String = conn.query_one("SELECT version()", &[])?.get(0);
    let extension_rows = conn.query(
        "SELECT extname FROM pg_extension WHERE extname IN ('vector','pg_search','pg_jieba') ORDER BY extname",
        &[],
    )?;
    let extensions: Vec<String> = extension_rows.iter().map(|r| r.get(0)).collect();
    let migration_version: i64 = conn
        .query_opt(
            "SELECT COALESCE(MAX(version), 0)::bigint FROM refinery_schema_history",
            &[],
        )?
        .map(|r| r.get(0))
        .unwrap_or(0);
    let documents: i64 = conn
        .query_one("SELECT COUNT(*) FROM documents", &[])
        .map(|r| r.get(0))
        .unwrap_or(0);
    let content_objects: i64 = conn
        .query_one("SELECT COUNT(*) FROM content_objects", &[])
        .map(|r| r.get(0))
        .unwrap_or(0);
    Ok(HealthInfo {
        server_version,
        migration_version: migration_version.max(0) as u32,
        extensions,
        documents,
        content_objects,
    })
}

/// 生成实体 ID（前缀 + 时间纳秒十六进制 16 位；服务端实现细节，域层只校验）。
pub fn new_entity_id(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}{nanos:016x}{seq:04x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_id_prefix_and_uniqueness() {
        let a = new_entity_id("doc-");
        let b = new_entity_id("doc-");
        assert!(a.starts_with("doc-"));
        assert_ne!(a, b);
    }
}
