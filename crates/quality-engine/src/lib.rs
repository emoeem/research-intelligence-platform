//! RIP 数据治理引擎（M4 垂直切片）。
//!
//! 职责（MODULE_BOUNDARIES §8.1 quality-engine 行"评分/隔离/License/D5 审核"；
//! §8.4 稳定接口 `Propose(Review) / Decide(review_id, decision)`）。归属裁定：
//! 治理为**独立 crate** 而非 search-engine 的内嵌模块，理由是 §8.1 把 scores +
//! reviews 数据面划给 quality-engine，而 search-engine 的数据面是"全部可重建"
//! 的派生索引（只读检索面）；治理是写路径（状态机跃迁、review 决议落库），
//! 并入会让索引重建流程与治理写路径耦合，违背 §8.2.3。
//!
//! - [`state`]：Resource 生命周期状态机（DISCOVERED→FETCHED→PARSED→CANDIDATE→
//!   VERIFIED→INDEXED / STALE）与治理轴（ACTIVE/QUARANTINED/REJECTED/ARCHIVED）
//!   双轴正交（M0 矛盾修正 K-08/K-16 口径），非法跃迁返回错误。
//! - [`governor`]：状态机落库 + **VERIFIED 守卫**（任何属性更新路径对 VERIFIED
//!   行返回 `NeedReview`，AI/自动管道不可覆盖人工验证结果）。
//! - [`review`]：Review 候选队列。从 d5 隔离清单（migrations/quarantine/*.jsonl）
//!   读候选，`propose` 幂等入队（decision=PENDING），`decide` 写决议到 review 表
//!   （与 platform review 表同形，便于迁移期对账）。
//!
//! 数据边界：治理库独立于 platform-dryrun.db（后者对本引擎只读）；治理写面
//! 落在自持库（如 platform-governance.db）或测试内存库。
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod error;
pub mod governor;
pub mod review;
pub mod state;

pub use error::GovernanceError;
pub use governor::{Governor, ResourceUpdate};
pub use review::{
    proposals_from_quarantine_dir, Proposal, ProposeOutcome, QuarantineProposeReport, ReviewRow,
    ReviewStore,
};
pub use state::{
    governance_allowed_targets, governance_transition_allowed, lifecycle_allowed_targets,
    lifecycle_transition_allowed,
};
