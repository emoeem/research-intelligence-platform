//! 混合排序权重（M4 初始口径）。
//!
//! **初始可调，黄金集校准待引入**：三轴权重为本批裁定值，排序质量尚未经过
//! golden-set 校准；后续以人工标注的相关性黄金集回归调参（计划 §M4 质量门）。
//! 权重常量集中在本模块，禁止散落。
//!
//! 混合公式（eri 缺证轴归一口径）：
//! `score = sum(w_i * v_i) / sum(w_i)`，只对有证据的轴参与。
//! - FTS bm25（文本相关）：v = 归一化 bm25（见 [`crate::query`]，0..1）
//! - quality：v = quality_final_score / 100，clamp 到 0..1（eri quality 轴同式）
//! - keyword 覆盖：v = min(1.0, cov * 1.5)（eri keyword 轴同式，六轴中
//!   可迁移项；category/tech/profile/relation 轴依赖的 skill_mappings/
//!   resource_categories/skill profile 维度在 platform 库尚无数据源，随
//!   M5+ workflow-engine / knowledge-engine 接口面落地后收编）

/// FTS bm25 轴权重（50%）。
pub const FTS_BM25_WEIGHT: f64 = 0.5;

/// quality 轴权重（30%）。
pub const QUALITY_WEIGHT: f64 = 0.3;

/// keyword 覆盖轴权重（20%）。
pub const KEYWORD_COVER_WEIGHT: f64 = 0.2;

/// keyword 覆盖证据值：覆盖率奖励系数（与 eri 一致：min(1.0, cov * 1.5)）。
pub const KEYWORD_COVER_BOOST: f64 = 1.5;
