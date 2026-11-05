//! ResourceRepository trait（resource-engine 对外稳定接口）。
//!
//! MODULE_BOUNDARIES §8.4：`Resolve(ctx, limit) -> CandidateSet` 是 resource-engine
//! 首批稳定接口之一；JSON 形状与 eri 输出兼容，迁移期双轨。

use crate::error::EngineError;
use crate::model::{ResolveQuery, ResolveResult};
use domain::{Resource, ResourceId};

/// 资源仓库接口。实现：[`crate::sqlite::SqliteResourceRepository`]。
pub trait ResourceRepository {
    /// 按稳定 ID 取单个 Resource。
    fn get(&self, id: &ResourceId) -> Result<Option<Resource>, EngineError>;

    /// resource 表总行数（含非 ACTIVE 治理态，仅作基数统计）。
    fn count(&self) -> Result<i64, EngineError>;

    /// `Resolve(query) -> ResolveResult`：关键词检索 + 简化相关性排序。
    fn resolve(&self, query: &ResolveQuery) -> Result<ResolveResult, EngineError>;
}
