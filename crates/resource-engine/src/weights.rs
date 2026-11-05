//! 相关性权重（本批裁定：简化版）。
//!
//! **P2 权重待迁**：eri_p2.RESOLVER_WEIGHTS 六轴
//! （keyword 0.25 / category 0.20 / tech 0.15 / profile 0.15 / relation 0.15 /
//! quality 0.10）依赖 category（resource_categories）、tech（github_metadata.topics）、
//! relation（skill_mappings）三张 platform 库尚不存在的表。待 M4 quality-engine 与
//! search-engine 落地后按 MODULE_BOUNDARIES §8.3 迁移六轴权重，本批简化为：
//!
//! - keyword 命中 60%
//! - quality 40%
//!
//! 评分归一规则与 eri 一致：score = sum(权重x证据值) / sum(已参与权重)，
//! 即缺 quality 证据时 keyword 单轴归一到 1.0。

/// keyword 轴权重（60%）。
pub const KEYWORD_WEIGHT: f64 = 0.6;

/// quality 轴权重（40%）。
pub const QUALITY_WEIGHT: f64 = 0.4;

/// keyword 证据值：覆盖率奖励系数（与 eri 一致：min(1.0, cov * 1.5)）。
pub const KEYWORD_COVER_BOOST: f64 = 1.5;
