//! database 层错误类型。

use std::fmt;

/// 数据库层错误。
#[derive(Debug)]
pub enum DbError {
    /// rusqlite 错误。
    Sqlite(rusqlite::Error),
    /// 文件系统 / IO 错误（迁移目录读取等）。
    Io(std::io::Error),
    /// 连接池已达上限且无空闲连接（等待方应退避重试）。
    PoolBusy {
        /// 池上限。
        max: usize,
    },
    /// 入库前校验失败（缺必填字段等）。
    Validation(String),
    /// 迁移文件命名不合法（需 `NNN_name.sql`）。
    MigrationName {
        /// 违规文件名。
        path: String,
        /// 原因。
        reason: String,
    },
    /// 迁移执行后 user_version 未推进到目标版本。
    MigrationVersionMismatch {
        /// 迁移文件。
        file: String,
        /// 期望版本。
        expected: i64,
        /// 实际版本。
        actual: i64,
    },
    /// 迁移目录中发现重复版本号。
    DuplicateMigrationVersion {
        /// 重复的版本号。
        version: i64,
        /// 第二个使用该版本号的文件。
        second_file: String,
    },
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbError::Sqlite(e) => write!(f, "sqlite error: {e}"),
            DbError::Io(e) => write!(f, "io error: {e}"),
            DbError::PoolBusy { max } => {
                write!(f, "connection pool busy: all {max} connections in use")
            }
            DbError::Validation(reason) => write!(f, "validation failed: {reason}"),
            DbError::MigrationName { path, reason } => {
                write!(f, "bad migration file {path}: {reason}")
            }
            DbError::MigrationVersionMismatch {
                file,
                expected,
                actual,
            } => write!(
                f,
                "migration {file} left user_version={actual}, expected {expected}"
            ),
            DbError::DuplicateMigrationVersion {
                version,
                second_file,
            } => write!(f, "duplicate migration version {version} in {second_file}"),
        }
    }
}

impl std::error::Error for DbError {}

impl From<rusqlite::Error> for DbError {
    fn from(e: rusqlite::Error) -> Self {
        DbError::Sqlite(e)
    }
}

impl From<std::io::Error> for DbError {
    fn from(e: std::io::Error) -> Self {
        DbError::Io(e)
    }
}

/// database 层 Result 别名。
pub type DbResult<T> = Result<T, DbError>;
