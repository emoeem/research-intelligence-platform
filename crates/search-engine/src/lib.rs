//! RIP 统一检索引擎（M4 垂直切片）。
//!
//! 职责（MODULE_BOUNDARIES §8.1 search-engine 行）：FTS / 混合检索、分词、排序；
//! 数据面 = 派生索引库（[`index`]），全部可重建（§8.2.3：删掉整个索引库，
//! 从 domain 库重灌即恢复）。
//!
//! - [`index`]：FTS5 派生索引层。从 platform 库（`resource` + `resource_source`
//!   的 alias/name 即 source_section/source_repository 并入）与 production catalog
//!   库（`github_metadata` 的 topics/language/license/maintenance，补齐 M2 报告
//!   差异表 #2 的 topics 数据源缺口）构建独立索引库（全部表 `search_index_` 前缀，
//!   头注标注可重建）。
//! - [`query`]：`SearchQuery(keywords, filters, limit, offset) -> Hits`，混合排序：
//!   FTS bm25 50% + quality_score 归一 30% + 六轴中可迁移的 keyword 覆盖 20%
//!   （权重口径见 [`weights`]），命中带 score、来源标注与可解释 reasons。
//! - [`tokenizer`]：索引/查询分词。ASCII 沿用 eri 口径（`[a-z0-9+#.\-]{3,}` 连续段
//!   按 `-` 拆分，复用 resource-engine::tokenizer 的 ALIASES/词元语义，单一同步源，
//!   不跨模块读库，符合 §8.2）；中文按 2-gram 预分词入库（eri 无 2-gram 实现，
//!   其 CJK 策略为 CJK_BRIDGE 查表，本 crate 同步移植该表用于查询扩展）。
//!
//! 依赖声明：search-engine -> resource-engine 仅复用其公开 tokenizer 数据表
//! 与词元函数（ALIASES 同步源唯一化），不读 resource-engine 的库表（§8.2.2）。
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod error;
pub mod index;
pub mod model;
pub mod query;
pub mod tokenizer;
pub mod weights;

pub use error::SearchError;
pub use index::{SearchIndex, INDEX_SCHEMA_VERSION};
pub use model::{
    BuildReport, Hits, IndexDoc, SearchFilters, SearchHit, SearchQuery, WeightsSnapshot,
};
pub use query::SearchEngine;
