//! RIP 资源引擎（M2 垂直切片）。
//!
//! 职责（MODULE_BOUNDARIES §8.1 resource-engine 行的 Resolve 部分）：
//! - [`repository`]：`ResourceRepository` trait（`get` / `count` / `resolve`）
//! - [`sqlite`]：platform 库（migrations/001_init.sql schema）上的 SQLite 实现
//! - [`model`]：`Resolve(query) -> ResolveResult`，JSON 形状与 eri_p2.resolve
//!   字段级兼容（§8.4 兼容承诺：迁移期双轨）
//!
//! 数据边界：engine 只读 `resource` 表（治理轴 QUARANTINED / REJECTED / ARCHIVED
//! 不可见），不读其他模块的库表。
//!
//! 相关性权重：本批裁定简化为 keyword 60% + quality 40%，P2 六轴权重待
//! quality-engine / search-engine 落地后迁移（见 [`weights`] 注释）。
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod error;
pub mod model;
pub mod repository;
pub mod sqlite;
pub mod tokenizer;
pub mod weights;

pub use error::EngineError;
pub use model::{
    CapabilityGap, CoveredRef, ResolveFilters, ResolveQuery, ResolveResult, ResourceHit,
    RetrievalStats,
};
pub use repository::ResourceRepository;
pub use sqlite::SqliteResourceRepository;
