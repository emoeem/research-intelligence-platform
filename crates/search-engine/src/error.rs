//! search-engine 错误类型。

use database::{rusqlite, DbError};
use std::fmt;

/// 检索引擎错误。
#[derive(Debug)]
pub enum SearchError {
    /// 数据库层错误。
    Db(DbError),
    /// 查询参数不合法（limit/offset 越界、FTS 语法注入风险等在构造期拒绝）。
    InvalidQuery(String),
    /// 索引库不存在或 schema 版本不符（需重建）。
    IndexNotReady(String),
}

impl fmt::Display for SearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SearchError::Db(e) => write!(f, "search-engine db error: {e}"),
            SearchError::InvalidQuery(reason) => write!(f, "invalid search query: {reason}"),
            SearchError::IndexNotReady(reason) => write!(f, "search index not ready: {reason}"),
        }
    }
}

impl std::error::Error for SearchError {}

impl From<DbError> for SearchError {
    fn from(e: DbError) -> Self {
        SearchError::Db(e)
    }
}

impl From<rusqlite::Error> for SearchError {
    fn from(e: rusqlite::Error) -> Self {
        SearchError::Db(DbError::Sqlite(e))
    }
}
