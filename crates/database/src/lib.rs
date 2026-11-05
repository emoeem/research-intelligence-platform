//! RIP 数据库基础设施（MODULE_BOUNDARIES §8.1 database 行）。
//!
//! - [`pool`]：rusqlite Connection 连接池（同步，Mutex+Condvar，`with_conn` / `with_tx`）
//! - [`migrate`]：迁移执行器（按 `NNN_name.sql` 版本号顺序执行，user_version 幂等）
//!
//! 不含 SQL 方言与业务表知识：表结构归各 engine 与 migrations/ 拥有。
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod error;
pub mod migrate;
pub mod pool;

pub use error::{DbError, DbResult};
pub use migrate::{discover_migrations, migrate, AppliedMigration, Migration};
pub use pool::{DbPath, Pool, PoolConfig, PooledConn};

/// 重导出 rusqlite：下游 engine 只依赖本 crate，不直接声明 rusqlite。
pub use rusqlite;
