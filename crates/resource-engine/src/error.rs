//! resource-engine 错误类型。

use database::{rusqlite, DbError};
use domain::DomainError;
use std::fmt;

/// 引擎层错误。
#[derive(Debug)]
pub enum EngineError {
    /// 数据库层错误。
    Db(DbError),
    /// 域类型错误（ID / 状态解析失败）。
    Domain(DomainError),
    /// 查询参数不合法。
    InvalidQuery(String),
    /// 目标行不存在（SK-1 引用证据复核面）。
    NotFound(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::Db(e) => write!(f, "resource-engine db error: {e}"),
            EngineError::Domain(e) => write!(f, "resource-engine domain error: {e}"),
            EngineError::InvalidQuery(reason) => {
                write!(f, "invalid resolve query: {reason}")
            }
            EngineError::NotFound(reason) => {
                write!(f, "not found: {reason}")
            }
        }
    }
}

impl std::error::Error for EngineError {}

impl From<DbError> for EngineError {
    fn from(e: DbError) -> Self {
        EngineError::Db(e)
    }
}

impl From<DomainError> for EngineError {
    fn from(e: DomainError) -> Self {
        EngineError::Domain(e)
    }
}

impl From<rusqlite::Error> for EngineError {
    fn from(e: rusqlite::Error) -> Self {
        EngineError::Db(DbError::Sqlite(e))
    }
}
