//! quality-engine 错误类型。

use database::{rusqlite, DbError};
use domain::DomainError;
use std::fmt;

/// 治理引擎错误。
#[derive(Debug)]
pub enum GovernanceError {
    /// 数据库层错误。
    Db(DbError),
    /// 域类型错误（状态解析失败等）。
    Domain(DomainError),
    /// 非法状态跃迁（状态机表外路径）。
    IllegalTransition {
        /// 轴名（lifecycle / governance）。
        axis: &'static str,
        /// 跃迁前状态。
        from: String,
        /// 跃迁目标状态。
        to: String,
    },
    /// VERIFIED 人工守卫触发：该行受人工验证保护，更新被拒绝。
    NeedReview {
        /// 资源 ID。
        resource_id: String,
        /// 当前生命周期。
        lifecycle: String,
        /// 守卫说明。
        note: String,
    },
    /// 目标行不存在。
    NotFound(String),
    /// 决议目标不存在或已决议（不可重复 decide）。
    AlreadyDecided(String),
    /// 决议值不合法（空值 / 保留字）。
    InvalidDecision(String),
}

impl fmt::Display for GovernanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GovernanceError::Db(e) => write!(f, "quality-engine db error: {e}"),
            GovernanceError::Domain(e) => write!(f, "quality-engine domain error: {e}"),
            GovernanceError::IllegalTransition { axis, from, to } => {
                write!(f, "illegal {axis} transition: {from} -> {to}")
            }
            GovernanceError::NeedReview {
                resource_id,
                lifecycle,
                note,
            } => write!(
                f,
                "need review: {resource_id} is {lifecycle}, update rejected: {note}"
            ),
            GovernanceError::NotFound(id) => write!(f, "not found: {id}"),
            GovernanceError::AlreadyDecided(id) => {
                write!(f, "review already decided: {id}")
            }
            GovernanceError::InvalidDecision(reason) => {
                write!(f, "invalid decision: {reason}")
            }
        }
    }
}

impl std::error::Error for GovernanceError {}

impl From<DbError> for GovernanceError {
    fn from(e: DbError) -> Self {
        GovernanceError::Db(e)
    }
}

impl From<DomainError> for GovernanceError {
    fn from(e: DomainError) -> Self {
        GovernanceError::Domain(e)
    }
}

impl From<rusqlite::Error> for GovernanceError {
    fn from(e: rusqlite::Error) -> Self {
        GovernanceError::Db(DbError::Sqlite(e))
    }
}
