//! RIP 数据库基础设施（MODULE_BOUNDARIES §8.1 database 行）。
//!
//! - [`pool`]：rusqlite Connection 连接池（同步，Mutex+Condvar，`with_conn` / `with_tx`）
//! - [`migrate`]：迁移执行器（按 `NNN_name.sql` 版本号顺序执行，user_version 幂等）
//! - [`backup`]：备份/恢复（Online Backup API 快照 + tar 归档 + sha256 manifest，
//!   ROADMAP §11.3 P10-5 / SESSION-HANDOFF §十）
//! - [`retention`]：数据保留归档（策略驱动旧行入档 + 行数对账，SESSION-HANDOFF §十）
//!
//! 不含 SQL 方言与业务表知识：表结构归各 engine 与 migrations/ 拥有。
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod backup;
pub mod error;
pub mod migrate;
pub mod pool;
pub mod retention;

pub use backup::{
    backup_to_archive, restore_from_archive, verify_archive, BackupManifest, BackupReport,
    RestoreReport, ARCHIVE_FORMAT_VERSION, DB_MEMBER, MANIFEST_MEMBER,
};
pub use error::{DbError, DbResult};
pub use migrate::{discover_migrations, migrate, AppliedMigration, Migration};
pub use pool::{DbPath, Pool, PoolConfig, PooledConn};
pub use retention::{
    builtin_policies, run_retention, RetentionOutcome, RetentionPolicy, RetentionReport,
};

/// 重导出 rusqlite：下游 engine 只依赖本 crate，不直接声明 rusqlite。
pub use rusqlite;
